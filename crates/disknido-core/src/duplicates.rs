//! Exact duplicate detection.
//!
//! 1. Same size (from the index; unique sizes are skipped).
//! 2. One open per candidate: physical identity (hard links share a FileKey
//!    and are collapsed — one copy, not duplicates) and a partial
//!    fingerprint (BLAKE3 of first + last 64 KiB). Filter only.
//! 3. Full BLAKE3, streamed in 1 MiB chunks, cached by (path, size, mtime, key).
//! 4. Byte-by-byte comparison inside each hash group. Only groups that pass
//!    are stored as `verified`. Files that change during the run are dropped.
//!
//! Reads are issued in path order with one worker pool per drive, so disks
//! are read in parallel and an HDD never paces an SSD.

use crate::fsx::{self, FileKey};
use crate::query::PathResolver;
use crate::volumes::{self, Media};
use crate::{Control, Profile, Result, safety};
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::time::Instant;

const PARTIAL: u64 = 64 * 1024;
const CHUNK: usize = 1024 * 1024;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DupRequest {
    #[serde(default = "one")]
    pub min_size: u64,
    /// Folder or drive prefixes to search; empty = everything indexed.
    #[serde(default)]
    pub scopes: Vec<String>,
    #[serde(default)]
    pub profile: Profile,
}

fn one() -> u64 {
    1
}

#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct DupProgress {
    /// grouping | partial | hashing | verifying | saving | done | cancelled
    pub phase: String,
    pub files_done: u64,
    pub files_total: u64,
    pub bytes_done: u64,
    pub bytes_total: u64,
    pub elapsed_ms: u64,
    /// File being read right now (shown so long reads never look frozen).
    pub current: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DupSummary {
    pub groups: u64,
    pub files: u64,
    pub redundant_bytes: u64,
    pub hard_links_collapsed: u64,
    pub skipped: Vec<(String, String)>,
    pub elapsed_ms: u64,
    pub status: String,
}

#[derive(Debug, Clone)]
pub struct Candidate {
    pub path: PathBuf,
    pub size: u64,
    pub mtime: i64,
    pub key: Option<FileKey>,
}

/// Streams the whole file through BLAKE3.
pub fn hash_file(path: &Path, control: &Control, bytes_done: &AtomicU64) -> Result<String> {
    let mut f = File::open(path)?;
    let mut h = blake3::Hasher::new();
    let mut buf = vec![0u8; CHUNK];
    loop {
        control.checkpoint()?;
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
        bytes_done.fetch_add(n as u64, Ordering::Relaxed);
    }
    Ok(h.finalize().to_hex().to_string())
}

/// BLAKE3 of the first and last 64 KiB (the whole file if smaller).
/// Equal partial hashes prove nothing; this is only a filter.
pub fn partial_hash(path: &Path, size: u64) -> Result<String> {
    let mut f = File::open(path)?;
    let mut h = blake3::Hasher::new();
    let mut buf = vec![0u8; PARTIAL as usize];
    let n = read_full(&mut f, &mut buf)?;
    h.update(&buf[..n]);
    if size > 2 * PARTIAL {
        f.seek(SeekFrom::End(-(PARTIAL as i64)))?;
        let n = read_full(&mut f, &mut buf)?;
        h.update(&buf[..n]);
    } else if size > PARTIAL {
        let n = read_full(&mut f, &mut buf)?;
        h.update(&buf[..n]);
    }
    Ok(h.finalize().to_hex().to_string())
}

fn read_full(f: &mut File, buf: &mut [u8]) -> std::io::Result<usize> {
    let mut total = 0;
    while total < buf.len() {
        let n = f.read(&mut buf[total..])?;
        if n == 0 {
            break;
        }
        total += n;
    }
    Ok(total)
}

/// True only if both files have exactly the same bytes.
pub fn files_identical(a: &Path, b: &Path, control: &Control) -> Result<bool> {
    files_identical_counted(a, b, control, &|_| {})
}

fn files_identical_counted(a: &Path, b: &Path, control: &Control, read: &dyn Fn(u64)) -> Result<bool> {
    let (mut fa, mut fb) = (File::open(a)?, File::open(b)?);
    if fa.metadata()?.len() != fb.metadata()?.len() {
        return Ok(false);
    }
    let mut ba = vec![0u8; CHUNK];
    let mut bb = vec![0u8; CHUNK];
    loop {
        control.checkpoint()?;
        let na = read_full(&mut fa, &mut ba)?;
        let nb = read_full(&mut fb, &mut bb)?;
        read((na + nb) as u64);
        if na != nb || ba[..na] != bb[..nb] {
            return Ok(false);
        }
        if na == 0 {
            return Ok(true);
        }
    }
}

/// Splits a set of files into classes of byte-identical content.
/// ponytail: compares each member against a representative, so the
/// representative is re-read per member; fine for typical groups of 2–5.
pub fn split_identical(paths: &[PathBuf], control: &Control) -> Result<Vec<Vec<usize>>> {
    split_identical_counted(paths, control, &|_| {})
}

fn split_identical_counted(paths: &[PathBuf], control: &Control, read: &dyn Fn(u64)) -> Result<Vec<Vec<usize>>> {
    let mut classes: Vec<Vec<usize>> = Vec::new();
    'outer: for i in 0..paths.len() {
        for class in classes.iter_mut() {
            if files_identical_counted(&paths[class[0]], &paths[i], control, read)? {
                class.push(i);
                continue 'outer;
            }
        }
        classes.push(vec![i]);
    }
    Ok(classes)
}

fn cached(conn: &Connection, c: &Candidate, col: &str) -> Option<String> {
    let key = c.key?.as_string();
    conn.query_row(
        &format!("SELECT {col} FROM hash_cache WHERE path=?1 AND size=?2 AND mtime=?3 AND file_key=?4"),
        params![safety::path_key(&c.path), c.size as i64, c.mtime, key],
        |r| r.get::<_, Option<String>>(0),
    )
    .optional()
    .ok()
    .flatten()
    .flatten()
}

/// Writes freshly computed hashes in one transaction (per-row commits were a
/// measurable cost with 100 000 candidates).
fn store_cache_batch<'a>(
    conn: &Connection,
    rows: impl Iterator<Item = (&'a Candidate, Option<&'a str>, Option<&'a str>)>,
) -> Result<()> {
    let tx = conn.unchecked_transaction()?;
    for (c, partial, full) in rows {
        store_cache(&tx, c, partial, full);
    }
    tx.commit()?;
    Ok(())
}

fn store_cache(conn: &Connection, c: &Candidate, partial: Option<&str>, full: Option<&str>) {
    let Some(key) = c.key else { return };
    let _ = conn.execute(
        "INSERT INTO hash_cache(path,size,mtime,file_key,partial,full) VALUES(?1,?2,?3,?4,?5,?6)
         ON CONFLICT(path) DO UPDATE SET
           partial=COALESCE(excluded.partial, CASE WHEN size=excluded.size AND mtime=excluded.mtime AND file_key=excluded.file_key THEN partial END),
           full=COALESCE(excluded.full, CASE WHEN size=excluded.size AND mtime=excluded.mtime AND file_key=excluded.file_key THEN full END),
           size=excluded.size, mtime=excluded.mtime, file_key=excluded.file_key",
        params![safety::path_key(&c.path), c.size as i64, c.mtime, key.as_string(), partial, full],
    );
}

fn unchanged(c: &Candidate) -> bool {
    std::fs::metadata(&c.path).map(|m| m.len() == c.size && fsx::mtime_of(&m) == c.mtime).unwrap_or(false)
}

fn in_scope(path: &Path, scopes: &[String]) -> bool {
    scopes.is_empty() || scopes.iter().any(|s| safety::is_within(path, Path::new(s)))
}

/// Loads same-size candidates from the index.
pub fn load_candidates(conn: &Connection, req: &DupRequest) -> Result<Vec<Candidate>> {
    let mut res = PathResolver::load(conn)?;
    let mut st = conn.prepare(
        "SELECT parent_id, name, size, mtime, attrs FROM entries
         WHERE is_dir=0 AND size >= ?1 AND size IN
           (SELECT size FROM entries WHERE is_dir=0 AND size >= ?1 GROUP BY size HAVING COUNT(*) > 1)",
    )?;
    let rows = st.query_map([req.min_size.max(1) as i64], |r| {
        Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?, r.get::<_, i64>(2)?, r.get::<_, i64>(3)?, r.get::<_, u32>(4)?))
    })?;
    let mut out = Vec::new();
    for row in rows {
        let (parent, name, size, mtime, attrs) = row?;
        if fsx::is_cloud_only_attrs(attrs) || attrs & fsx::ATTR_REPARSE_POINT != 0 {
            continue;
        }
        let Some(path) = res.file_path(parent, &name) else { continue };
        if in_scope(&path, &req.scopes) {
            out.push(Candidate { path, size: size as u64, mtime, key: None });
        }
    }
    Ok(out)
}

/// Runs `f` over `items`, one worker pool per drive so an SSD is not paced
/// by an HDD and two physical disks are read at the same time. Within a
/// drive, items are processed in their given (path) order. Results keep
/// input order.
fn par_by_drive<T: Sync, R: Send>(
    items: &[T],
    path_of: impl Fn(&T) -> &Path + Sync,
    threads_for: impl Fn(&str) -> usize,
    f: impl Fn(&T) -> R + Sync,
) -> Vec<R> {
    let mut by_drive: HashMap<String, Vec<usize>> = HashMap::new();
    for (i, it) in items.iter().enumerate() {
        let d = path_of(it).to_string_lossy().get(..2).unwrap_or("").to_ascii_uppercase();
        by_drive.entry(d).or_default().push(i);
    }
    let out: Mutex<Vec<Option<R>>> = Mutex::new((0..items.len()).map(|_| None).collect());
    std::thread::scope(|s| {
        for (drive, idx) in &by_drive {
            let next = std::sync::Arc::new(AtomicUsize::new(0));
            for _ in 0..threads_for(drive).max(1) {
                let (next, out, f) = (next.clone(), &out, &f);
                s.spawn(move || {
                    loop {
                        let k = next.fetch_add(1, Ordering::Relaxed);
                        let Some(&i) = idx.get(k) else { break };
                        let r = f(&items[i]);
                        out.lock().unwrap()[i] = Some(r);
                    }
                });
            }
        }
    });
    out.into_inner().unwrap().into_iter().map(|r| r.expect("every item processed")).collect()
}

/// Opens a file once: its physical identity (hard links share it) and the
/// partial fingerprint. Files up to 128 KiB are hashed completely here.
fn identity_and_partial(path: &Path, size: u64) -> Result<(FileKey, String)> {
    let mut f = File::open(path)?;
    let key = fsx::file_key(&f)?;
    let mut h = blake3::Hasher::new();
    let mut buf = vec![0u8; PARTIAL as usize];
    let n = read_full(&mut f, &mut buf)?;
    h.update(&buf[..n]);
    if size > 2 * PARTIAL {
        f.seek(SeekFrom::End(-(PARTIAL as i64)))?;
        let n = read_full(&mut f, &mut buf)?;
        h.update(&buf[..n]);
    } else if size > PARTIAL {
        let n = read_full(&mut f, &mut buf)?;
        h.update(&buf[..n]);
    }
    Ok((key, h.finalize().to_hex().to_string()))
}

pub fn find_duplicates(
    db_path: &Path,
    req: &DupRequest,
    control: &Control,
    on_progress: &(dyn Fn(&DupProgress) + Sync),
) -> Result<DupSummary> {
    let started = Instant::now();
    let db = Mutex::new(crate::db::open(db_path)?);
    let current = Mutex::new(String::new());
    let progress = |phase: &str, done: u64, total: u64, bd: u64, bt: u64| {
        on_progress(&DupProgress {
            phase: phase.into(),
            files_done: done,
            files_total: total,
            bytes_done: bd,
            bytes_total: bt,
            elapsed_ms: started.elapsed().as_millis() as u64,
            current: current.lock().map(|c| c.clone()).unwrap_or_default(),
        })
    };
    let set_current = |p: &Path| {
        if let Ok(mut c) = current.try_lock() {
            *c = p.to_string_lossy().into_owned();
        }
    };
    progress("grouping", 0, 0, 0, 0);
    let mut candidates = load_candidates(&db.lock().unwrap(), req)?;
    candidates.sort_by(|a, b| a.path.cmp(&b.path));
    let vols = volumes::list_volumes();
    let threads_for = |drive: &str| {
        let media = vols.iter().find(|v| v.letter.eq_ignore_ascii_case(drive)).map(|v| v.media).unwrap_or(Media::Unknown);
        volumes::reader_threads(req.profile, media)
    };
    let skipped: Mutex<Vec<(String, String)>> = Mutex::new(Vec::new());
    let skip = |p: &Path, why: String| skipped.lock().unwrap().push((p.to_string_lossy().into_owned(), why));
    let ticker = Mutex::new(Instant::now() - std::time::Duration::from_secs(1));
    let due = || {
        ticker.try_lock().is_ok_and(|mut t| {
            let ok = t.elapsed().as_millis() >= 200;
            if ok {
                *t = Instant::now();
            }
            ok
        })
    };

    // Phase 1: open each candidate once — identity (hard links) + partial
    // fingerprint.
    let total = candidates.len() as u64;
    let done = AtomicU64::new(0);
    let firsts: Vec<Option<(FileKey, String)>> = par_by_drive(
        &candidates,
        |c| &c.path,
        threads_for,
        |c| {
            if control.checkpoint().is_err() {
                return None;
            }
            set_current(&c.path);
            let d = done.fetch_add(1, Ordering::Relaxed) + 1;
            if due() {
                progress("partial", d, total, 0, 0);
            }
            match identity_and_partial(&c.path, c.size) {
                Ok(r) => Some(r),
                Err(e) => {
                    skip(&c.path, format!("No se pudo leer: {e}"));
                    None
                }
            }
        },
    );
    control.checkpoint()?;
    let mut hard_links = 0u64;
    let mut pmap: HashMap<(u64, String), Vec<Candidate>> = HashMap::new();
    for (mut c, r) in candidates.into_iter().zip(firsts) {
        if let Some((k, h)) = r {
            c.key = Some(k);
            pmap.entry((c.size, h)).or_default().push(c);
        }
    }
    // Within a group, paths sharing a FileKey are the same physical file.
    let mut flat: Vec<(Candidate, String)> = Vec::new();
    for ((_, h), g) in pmap {
        let mut seen = std::collections::HashSet::new();
        let unique: Vec<Candidate> = g
            .into_iter()
            .filter(|c| {
                let new = seen.insert(c.key);
                if !new {
                    hard_links += 1;
                }
                new
            })
            .collect();
        if unique.len() > 1 {
            flat.extend(unique.into_iter().map(|c| (c, h.clone())));
        }
    }
    flat.sort_by(|a, b| a.0.path.cmp(&b.0.path));

    // Phase 2: full BLAKE3, except files the partial pass read completely.
    let bytes_total: u64 = flat.iter().filter(|c| c.0.size > 2 * PARTIAL).map(|c| c.0.size).sum();
    let bytes_done = AtomicU64::new(0);
    let done = AtomicU64::new(0);
    let total = flat.len() as u64;
    let fulls: Vec<Option<(String, bool)>> = par_by_drive(
        &flat,
        |c| &c.0.path,
        threads_for,
        |(c, partial)| {
            if control.checkpoint().is_err() {
                return None;
            }
            let d = done.fetch_add(1, Ordering::Relaxed) + 1;
            if c.size <= 2 * PARTIAL {
                return Some((partial.clone(), false));
            }
            set_current(&c.path);
            if let Some(h) = cached(&db.lock().unwrap(), c, "full") {
                bytes_done.fetch_add(c.size, Ordering::Relaxed);
                return Some((h, false));
            }
            // Hash in chunks and keep the progress ticking for huge files.
            let res = (|| -> Result<String> {
                let mut f = File::open(&c.path)?;
                let mut h = blake3::Hasher::new();
                let mut buf = vec![0u8; CHUNK];
                loop {
                    control.checkpoint()?;
                    let n = f.read(&mut buf)?;
                    if n == 0 {
                        break;
                    }
                    h.update(&buf[..n]);
                    let b = bytes_done.fetch_add(n as u64, Ordering::Relaxed) + n as u64;
                    if due() {
                        progress("hashing", d, total, b, bytes_total);
                    }
                }
                Ok(h.finalize().to_hex().to_string())
            })();
            match res {
                Ok(h) => Some((h, true)),
                Err(crate::Error::Cancelled) => None,
                Err(e) => {
                    skip(&c.path, format!("No se pudo leer: {e}"));
                    None
                }
            }
        },
    );
    control.checkpoint()?;
    store_cache_batch(
        &db.lock().unwrap(),
        flat.iter().zip(&fulls).filter_map(|((c, _), h)| h.as_ref().filter(|h| h.1).map(|h| (c, None, Some(h.0.as_str())))),
    )?;
    let mut hmap: HashMap<(u64, String), Vec<Candidate>> = HashMap::new();
    for ((c, _), h) in flat.into_iter().zip(fulls) {
        if let Some((h, _)) = h {
            hmap.entry((c.size, h)).or_default().push(c);
        }
    }
    let mut hash_groups: Vec<((u64, String), Vec<Candidate>)> = hmap.into_iter().filter(|(_, g)| g.len() > 1).collect();
    hash_groups.sort_by(|a, b| a.1[0].path.cmp(&b.1[0].path));

    // Phase 3: byte-by-byte verification, with byte progress for big files.
    let total = hash_groups.len() as u64;
    let vbytes_total: u64 = hash_groups.iter().map(|((s, _), g)| s * g.len() as u64).sum();
    let vbytes = AtomicU64::new(0);
    let mut verified: Vec<(u64, String, Vec<Candidate>)> = Vec::new();
    for (i, ((size, hash), group)) in hash_groups.into_iter().enumerate() {
        control.checkpoint()?;
        set_current(&group[0].path);
        progress("verifying", i as u64, total, vbytes.load(Ordering::Relaxed), vbytes_total);
        let paths: Vec<PathBuf> = group.iter().map(|c| c.path.clone()).collect();
        let classes = match split_identical_counted(&paths, control, &|n| {
            let b = vbytes.fetch_add(n, Ordering::Relaxed) + n;
            if due() {
                progress("verifying", i as u64, total, b, vbytes_total);
            }
        }) {
            Ok(c) => c,
            Err(crate::Error::Cancelled) => return Err(crate::Error::Cancelled),
            Err(e) => {
                for p in &paths {
                    skip(p, format!("No se pudo comparar: {e}"));
                }
                continue;
            }
        };
        for class in classes {
            let members: Vec<Candidate> = class
                .into_iter()
                .map(|i| group[i].clone())
                .filter(|c| {
                    let ok = unchanged(c);
                    if !ok {
                        skip(&c.path, "El archivo cambió durante el análisis".into());
                    }
                    ok
                })
                .collect();
            if members.len() > 1 {
                verified.push((size, hash.clone(), members));
            }
        }
    }

    progress("saving", 0, 0, 0, 0);
    let mut conn = db.into_inner().unwrap();
    let tx = conn.transaction()?;
    tx.execute_batch("DELETE FROM dup_files; DELETE FROM dup_groups;")?;
    let now = crate::now_unix();
    let (mut files, mut redundant) = (0u64, 0u64);
    for (size, hash, members) in &verified {
        tx.execute("INSERT INTO dup_groups(run_at,size,hash,verified) VALUES(?1,?2,?3,1)", params![now, *size as i64, hash])?;
        let gid = tx.last_insert_rowid();
        for c in members {
            let reason = safety::duplicate_exclusion_reason(&c.path)
                .or_else(|| crate::backups::backup_ancestor(&c.path).map(|b| format!("Parte del respaldo «{b}»")));
            tx.execute(
                "INSERT INTO dup_files(group_id,path,mtime,file_key,protected_reason) VALUES(?1,?2,?3,?4,?5)",
                params![gid, c.path.to_string_lossy(), c.mtime, c.key.map(|k| k.as_string()).unwrap_or_default(), reason],
            )?;
        }
        files += members.len() as u64;
        redundant += size * (members.len() as u64 - 1);
    }
    tx.commit()?;
    let summary = DupSummary {
        groups: verified.len() as u64,
        files,
        redundant_bytes: redundant,
        hard_links_collapsed: hard_links,
        skipped: skipped.into_inner().unwrap(),
        elapsed_ms: started.elapsed().as_millis() as u64,
        status: "done".into(),
    };
    progress("done", files, files, 0, 0);
    Ok(summary)
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DupFile {
    pub path: String,
    pub mtime: i64,
    pub protected_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DupGroup {
    pub id: i64,
    pub size: u64,
    pub hash: String,
    pub verified: bool,
    pub run_at: i64,
    pub files: Vec<DupFile>,
    pub redundant_bytes: u64,
    /// Index into `files` of the suggested copy to keep, with the reason.
    pub suggested_keep: usize,
    pub suggestion_reason: String,
}

/// Transparent keep suggestion: a protected copy (backup, etc.) first, then
/// the shortest path (usually the "organised" location). Never presented as
/// proof of which copy is the original.
pub fn suggest_keep(files: &[DupFile]) -> (usize, String) {
    if let Some(i) = files.iter().position(|f| f.protected_reason.is_some()) {
        return (i, "Está en una ubicación protegida".into());
    }
    let i = files.iter().enumerate().min_by_key(|(_, f)| (f.path.len(), f.path.clone())).map(|(i, _)| i).unwrap_or(0);
    (i, "Ruta más corta (sugerencia, no garantía de originalidad)".into())
}

pub fn list_groups(conn: &Connection, offset: i64, limit: i64) -> Result<(Vec<DupGroup>, i64)> {
    let total: i64 = conn.query_row("SELECT COUNT(*) FROM dup_groups", [], |r| r.get(0))?;
    let mut st = conn.prepare(
        "SELECT g.id, g.size, g.hash, g.verified, g.run_at, COUNT(f.path) n FROM dup_groups g JOIN dup_files f ON f.group_id=g.id
         GROUP BY g.id ORDER BY g.size*(n-1) DESC LIMIT ?1 OFFSET ?2",
    )?;
    let heads: Vec<(i64, i64, String, bool, i64)> = st
        .query_map(params![limit, offset], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)))?
        .collect::<rusqlite::Result<_>>()?;
    let mut fst = conn.prepare("SELECT path, mtime, protected_reason FROM dup_files WHERE group_id=?1 ORDER BY path")?;
    let mut out = Vec::new();
    for (id, size, hash, verified, run_at) in heads {
        let files: Vec<DupFile> = fst
            .query_map([id], |r| Ok(DupFile { path: r.get(0)?, mtime: r.get(1)?, protected_reason: r.get(2)? }))?
            .collect::<rusqlite::Result<_>>()?;
        let (keep, why) = suggest_keep(&files);
        out.push(DupGroup {
            id,
            size: size as u64,
            hash,
            verified,
            run_at,
            redundant_bytes: size as u64 * (files.len() as u64).saturating_sub(1),
            suggested_keep: keep,
            suggestion_reason: why,
            files,
        });
    }
    Ok((out, total))
}

pub fn totals(conn: &Connection) -> Result<(u64, u64)> {
    let (groups, redundant): (i64, i64) = conn.query_row(
        "SELECT COUNT(*), COALESCE(SUM(size*(n-1)),0) FROM (SELECT g.size, COUNT(f.path) n FROM dup_groups g JOIN dup_files f ON f.group_id=g.id GROUP BY g.id)",
        [],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    Ok((groups as u64, redundant as u64))
}

/// CSV export of the verified groups (paths can be anonymised).
pub fn export_csv(conn: &Connection, anonymize: bool) -> Result<String> {
    let home = std::env::var("USERPROFILE").unwrap_or_default().to_lowercase();
    let mut st = conn.prepare(
        "SELECT g.id, g.size, g.hash, f.path FROM dup_groups g JOIN dup_files f ON f.group_id=g.id ORDER BY g.id, f.path",
    )?;
    let mut out = String::from("group,size_bytes,blake3,path\n");
    let rows =
        st.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?, r.get::<_, String>(2)?, r.get::<_, String>(3)?)))?;
    for row in rows {
        let (g, s, h, mut p) = row?;
        if anonymize && !home.is_empty() && p.to_lowercase().starts_with(&home) {
            p = format!("%USERPROFILE%{}", &p[home.len()..]);
        }
        out.push_str(&format!("{g},{s},{h},\"{}\"\n", p.replace('"', "\"\"")));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keep_suggestion_prefers_protected_then_short() {
        let f = |p: &str, r: Option<&str>| DupFile { path: p.into(), mtime: 0, protected_reason: r.map(String::from) };
        let (i, _) = suggest_keep(&[f(r"C:\Users\a\Downloads\copia.jpg", None), f(r"D:\Fotos\a.jpg", None)]);
        assert_eq!(i, 1);
        let (i, _) = suggest_keep(&[f(r"D:\a.jpg", None), f(r"E:\Backups\2024\x\a.jpg", Some("respaldo"))]);
        assert_eq!(i, 1);
    }
}
