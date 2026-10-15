//! Exact duplicate detection.
//!
//! 1. Same size (from the index; unique sizes are skipped).
//! 2. Same physical file? Hard links share a FileKey and are collapsed — they
//!    are one copy, not duplicates.
//! 3. Partial fingerprint (BLAKE3 of first + last 64 KiB). Filter only.
//! 4. Full BLAKE3, streamed in 1 MiB chunks, cached by (path, size, mtime, key).
//! 5. Byte-by-byte comparison inside each hash group. Only groups that pass
//!    are stored as `verified`. Files that change during the run are dropped.

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
    /// grouping | identity | partial | hashing | verifying | saving | done | cancelled
    pub phase: String,
    pub files_done: u64,
    pub files_total: u64,
    pub bytes_done: u64,
    pub bytes_total: u64,
    pub elapsed_ms: u64,
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
    let mut classes: Vec<Vec<usize>> = Vec::new();
    'outer: for i in 0..paths.len() {
        for class in classes.iter_mut() {
            if files_identical(&paths[class[0]], &paths[i], control)? {
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

fn group_by<K: std::hash::Hash + Eq>(items: Vec<Candidate>, key: impl Fn(&Candidate) -> Option<K>) -> Vec<Vec<Candidate>> {
    let mut map: HashMap<K, Vec<Candidate>> = HashMap::new();
    for c in items {
        if let Some(k) = key(&c) {
            map.entry(k).or_default().push(c);
        }
    }
    map.into_values().filter(|g| g.len() > 1).collect()
}

/// Runs `f` over `items` with `threads` workers; results keep input order.
fn par_map<T: Sync, R: Send>(items: &[T], threads: usize, f: impl Fn(&T) -> R + Sync) -> Vec<R> {
    let next = AtomicUsize::new(0);
    let out: Mutex<Vec<Option<R>>> = Mutex::new((0..items.len()).map(|_| None).collect());
    std::thread::scope(|s| {
        for _ in 0..threads.max(1) {
            s.spawn(|| {
                loop {
                    let i = next.fetch_add(1, Ordering::Relaxed);
                    if i >= items.len() {
                        break;
                    }
                    let r = f(&items[i]);
                    out.lock().unwrap()[i] = Some(r);
                }
            });
        }
    });
    out.into_inner().unwrap().into_iter().map(|r| r.expect("every item processed")).collect()
}

pub fn find_duplicates(
    db_path: &Path,
    req: &DupRequest,
    control: &Control,
    on_progress: &(dyn Fn(&DupProgress) + Sync),
) -> Result<DupSummary> {
    let started = Instant::now();
    let db = Mutex::new(crate::db::open(db_path)?);
    let progress = |phase: &str, done: u64, total: u64, bd: u64, bt: u64| {
        on_progress(&DupProgress {
            phase: phase.into(),
            files_done: done,
            files_total: total,
            bytes_done: bd,
            bytes_total: bt,
            elapsed_ms: started.elapsed().as_millis() as u64,
        })
    };
    progress("grouping", 0, 0, 0, 0);
    let candidates = load_candidates(&db.lock().unwrap(), req)?;
    let media = media_of(&req.scopes, &candidates);
    let threads = volumes::reader_threads(req.profile, media);
    let skipped: Mutex<Vec<(String, String)>> = Mutex::new(Vec::new());
    let skip = |p: &Path, why: String| skipped.lock().unwrap().push((p.to_string_lossy().into_owned(), why));

    // Identity: open each file once (also proves it is readable now).
    let total = candidates.len() as u64;
    progress("identity", 0, total, 0, 0);
    let mut by_size = group_by(candidates, |c| Some(c.size));
    let mut hard_links = 0u64;
    let mut pool = Vec::new();
    for group in by_size.iter_mut() {
        control.checkpoint()?;
        let mut seen: HashMap<FileKey, ()> = HashMap::new();
        for mut c in group.drain(..) {
            match File::open(&c.path).and_then(|f| fsx::file_key(&f)) {
                Ok(k) => {
                    if seen.insert(k, ()).is_some() {
                        hard_links += 1; // same physical file as an earlier path
                        continue;
                    }
                    c.key = Some(k);
                    pool.push(c);
                }
                Err(e) => skip(&c.path, format!("No se pudo abrir: {e}")),
            }
        }
    }
    drop(by_size);

    // Partial fingerprint.
    let size_groups = group_by(pool, |c| Some(c.size));
    let flat: Vec<Candidate> = size_groups.into_iter().flatten().collect();
    let done = AtomicU64::new(0);
    let total = flat.len() as u64;
    let partials: Vec<Option<String>> = par_map(&flat, threads, |c| {
        if control.is_cancelled() {
            return None;
        }
        let _ = control.checkpoint();
        let d = done.fetch_add(1, Ordering::Relaxed);
        if d % 64 == 0 {
            progress("partial", d, total, 0, 0);
        }
        if let Some(h) = cached(&db.lock().unwrap(), c, "partial") {
            return Some(h);
        }
        match partial_hash(&c.path, c.size) {
            Ok(h) => {
                store_cache(&db.lock().unwrap(), c, Some(&h), None);
                Some(h)
            }
            Err(e) => {
                skip(&c.path, format!("No se pudo leer: {e}"));
                None
            }
        }
    });
    control.checkpoint()?;
    let tagged: Vec<(Candidate, Option<String>)> = flat.into_iter().zip(partials).collect();
    let mut pmap: HashMap<(u64, String), Vec<Candidate>> = HashMap::new();
    for (c, h) in tagged {
        if let Some(h) = h {
            pmap.entry((c.size, h)).or_default().push(c);
        }
    }
    let flat: Vec<Candidate> = pmap.into_values().filter(|g| g.len() > 1).flatten().collect();

    // Full hash.
    let bytes_total: u64 = flat.iter().map(|c| c.size).sum();
    let bytes_done = AtomicU64::new(0);
    let done = AtomicU64::new(0);
    let total = flat.len() as u64;
    let fulls: Vec<Option<String>> = par_map(&flat, threads, |c| {
        if control.is_cancelled() {
            return None;
        }
        let d = done.fetch_add(1, Ordering::Relaxed);
        progress("hashing", d, total, bytes_done.load(Ordering::Relaxed), bytes_total);
        if let Some(h) = cached(&db.lock().unwrap(), c, "full") {
            bytes_done.fetch_add(c.size, Ordering::Relaxed);
            return Some(h);
        }
        match hash_file(&c.path, control, &bytes_done) {
            Ok(h) => {
                store_cache(&db.lock().unwrap(), c, None, Some(&h));
                Some(h)
            }
            Err(crate::Error::Cancelled) => None,
            Err(e) => {
                skip(&c.path, format!("No se pudo leer: {e}"));
                None
            }
        }
    });
    control.checkpoint()?;
    let mut hmap: HashMap<(u64, String), Vec<Candidate>> = HashMap::new();
    for (c, h) in flat.into_iter().zip(fulls) {
        if let Some(h) = h {
            hmap.entry((c.size, h)).or_default().push(c);
        }
    }
    let hash_groups: Vec<((u64, String), Vec<Candidate>)> = hmap.into_iter().filter(|(_, g)| g.len() > 1).collect();

    // Byte-by-byte verification.
    let total = hash_groups.len() as u64;
    let mut verified: Vec<(u64, String, Vec<Candidate>)> = Vec::new();
    for (i, ((size, hash), group)) in hash_groups.into_iter().enumerate() {
        control.checkpoint()?;
        progress("verifying", i as u64, total, 0, 0);
        let paths: Vec<PathBuf> = group.iter().map(|c| c.path.clone()).collect();
        let classes = match split_identical(&paths, control) {
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

fn media_of(scopes: &[String], cands: &[Candidate]) -> Media {
    // Use the slowest medium involved: one HDD in the set means HDD pacing.
    let vols = volumes::list_volumes();
    let mut letters: Vec<String> = scopes.iter().filter_map(|s| s.get(..2).map(|x| x.to_ascii_uppercase())).collect();
    if letters.is_empty() {
        letters = cands.iter().filter_map(|c| c.path.to_str()?.get(..2).map(|x| x.to_ascii_uppercase())).collect();
        letters.sort();
        letters.dedup();
    }
    let medias: Vec<Media> =
        letters.iter().filter_map(|l| vols.iter().find(|v| v.letter.eq_ignore_ascii_case(l)).map(|v| v.media)).collect();
    if medias.contains(&Media::Hdd) {
        Media::Hdd
    } else if medias.is_empty() {
        Media::Unknown
    } else {
        Media::Ssd
    }
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
