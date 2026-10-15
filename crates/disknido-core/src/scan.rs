//! Read-only scanner.
//!
//! Phases: enumerate+metadata (parallel walkers, one listing call per
//! directory gives names, sizes, times and attributes) → persist (single
//! writer thread, batched transactions) → aggregate (folder totals computed
//! in one reverse pass, no recursive SQL).
//!
//! Per-root replacement: a scan replaces only the indexed roots that overlap
//! the roots it walks (same path, inside it or containing it); other roots
//! stay. Each root of a scan gets its own id range of `ROOT_SPAN` ids
//! starting after MAX(id), so every root's subtree is the id range from the
//! root up to the next root, and children keep larger ids than their parents.
//!
//! Roots on different physical disks are walked at the same time (one walker
//! pool per disk); roots on the same disk one after another, so they never
//! compete for the same heads. Extension totals per root are accumulated
//! while writing (`ext_stats`), so the UI never has to group millions of rows.
//!
//! Links (symlinks, junctions, mount points) are recorded but never followed,
//! so there are no cycles. Cloud placeholder folders are walked; cloud-only
//! files are recorded with their attributes and never opened.

use crate::fsx::{self, ATTR_HIDDEN, ATTR_SYSTEM};
use crate::volumes::{self, Media};
use crate::{Control, Error, Profile, Result, now_unix, safety};
use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, SyncSender, sync_channel};
use std::sync::{Condvar, Mutex};
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanRequest {
    pub roots: Vec<PathBuf>,
    /// quick | full | custom — recorded for the UI; the walk is identical.
    pub mode: String,
    #[serde(default)]
    pub profile: Profile,
    /// Folder paths the user excluded (any case).
    #[serde(default)]
    pub exclusions: Vec<String>,
    #[serde(default = "yes")]
    pub include_hidden: bool,
}

fn yes() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ScanProgress {
    pub scan_id: i64,
    /// enumerating | aggregating | indexing | done | cancelled
    pub phase: String,
    pub root: String,
    pub current: String,
    pub files: u64,
    pub dirs: u64,
    pub bytes: u64,
    pub errors: u64,
    pub elapsed_ms: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanSummary {
    pub scan_id: i64,
    pub status: String,
    pub files: u64,
    pub dirs: u64,
    pub bytes: u64,
    pub errors: u64,
    pub elapsed_ms: u64,
}

struct Row {
    id: u64,
    parent: u64,
    name: String,
    is_dir: bool,
    size: u64,
    mtime: i64,
    ctime: i64,
    attrs: u32,
}

enum Msg {
    /// Sent just before a root's own row: drop the old roots it replaces.
    ReplaceRoot(PathBuf),
    Rows(Vec<Row>),
    Error(String, String),
    /// All walkers finished.
    Done,
}

#[derive(Default)]
struct Counters {
    files: AtomicU64,
    dirs: AtomicU64,
    bytes: AtomicU64,
    errors: AtomicU64,
    current: Mutex<String>,
}

/// Ids reserved for each root of a scan (4 billion entries per root).
const ROOT_SPAN: u64 = 1 << 32;

struct Ctx<'a> {
    tx: SyncSender<Msg>,
    control: &'a Control,
    counters: &'a Counters,
    exclusions: Vec<String>,
    include_hidden: bool,
}

pub fn ext_of(name: &str) -> Option<String> {
    let (stem, ext) = name.rsplit_once('.')?;
    if stem.is_empty() || ext.is_empty() || ext.len() > 10 || ext.contains(' ') {
        return None;
    }
    Some(ext.to_ascii_lowercase())
}

fn is_excluded(ctx: &Ctx, path: &Path) -> bool {
    if ctx.exclusions.is_empty() {
        return false;
    }
    let p = path.to_string_lossy().to_lowercase();
    ctx.exclusions.iter().any(|e| p == *e || (p.starts_with(e.as_str()) && p[e.len()..].starts_with('\\')))
}

/// Parallel depth-first walk of one root. LIFO work stack keeps reads local,
/// which matters on HDDs.
fn walk(root: PathBuf, root_id: u64, threads: usize, ctx: &Ctx, ids: &AtomicU64) {
    let queue = Mutex::new((vec![(root, root_id)], 0usize));
    let cv = Condvar::new();
    std::thread::scope(|s| {
        for _ in 0..threads.max(1) {
            s.spawn(|| {
                loop {
                    let (dir, dir_id) = {
                        let mut q = queue.lock().unwrap();
                        loop {
                            if ctx.control.is_cancelled() {
                                cv.notify_all();
                                return;
                            }
                            if let Some(item) = q.0.pop() {
                                q.1 += 1;
                                break item;
                            }
                            if q.1 == 0 {
                                cv.notify_all();
                                return;
                            }
                            q = cv.wait_timeout(q, Duration::from_millis(200)).unwrap().0;
                        }
                    };
                    let _ = ctx.control.checkpoint();
                    let children = list_one(&dir, dir_id, ctx, ids);
                    let mut q = queue.lock().unwrap();
                    q.0.extend(children);
                    q.1 -= 1;
                    cv.notify_all();
                }
            });
        }
    });
}

fn list_one(dir: &Path, dir_id: u64, ctx: &Ctx, ids: &AtomicU64) -> Vec<(PathBuf, u64)> {
    let items = match fsx::list_dir(dir) {
        Ok(items) => items,
        Err(e) => {
            ctx.counters.errors.fetch_add(1, Ordering::Relaxed);
            let _ = ctx.tx.send(Msg::Error(dir.to_string_lossy().into_owned(), e.to_string()));
            return Vec::new();
        }
    };
    if let Ok(mut cur) = ctx.counters.current.try_lock() {
        *cur = dir.to_string_lossy().into_owned();
    }
    let mut rows = Vec::with_capacity(items.len());
    let mut descend = Vec::new();
    let (mut files, mut dirs, mut bytes) = (0u64, 0u64, 0u64);
    for it in items {
        if !ctx.include_hidden && it.attrs & (ATTR_HIDDEN | ATTR_SYSTEM) != 0 {
            continue;
        }
        let path = dir.join(&it.name);
        if it.is_dir && is_excluded(ctx, &path) {
            continue;
        }
        let id = ids.fetch_add(1, Ordering::Relaxed);
        if it.is_dir {
            dirs += 1;
            if !it.is_link() {
                descend.push((path, id));
            }
        } else {
            files += 1;
            bytes += it.size;
        }
        rows.push(Row {
            id,
            parent: dir_id,
            size: if it.is_dir { 0 } else { it.size },
            name: it.name,
            is_dir: it.is_dir,
            mtime: it.mtime,
            ctime: it.ctime,
            attrs: it.attrs,
        });
    }
    ctx.counters.files.fetch_add(files, Ordering::Relaxed);
    ctx.counters.dirs.fetch_add(dirs, Ordering::Relaxed);
    ctx.counters.bytes.fetch_add(bytes, Ordering::Relaxed);
    // Rows are sent before children are queued: a directory row always
    // reaches the writer before rows of its contents.
    if !rows.is_empty() {
        let _ = ctx.tx.send(Msg::Rows(rows));
    }
    descend
}

/// Per-entry accumulator for folder totals, indexed by `id - base` (base =
/// first id of this scan). ponytail: ~40 bytes/entry in RAM (≈40 MB per
/// million entries); switch to an on-disk pass if that matters.
#[derive(Clone, Copy)]
struct Agg {
    /// Parent's index + 1; 0 for scan roots.
    parent: u32,
    is_dir: bool,
    size: u64,
    count: u32,
    newest: i64,
    oldest: i64,
}

const EMPTY: Agg = Agg { parent: 0, is_dir: false, size: 0, count: 0, newest: i64::MIN, oldest: i64::MAX };

/// Deletes indexed roots from earlier scans (ids below `base`) that overlap
/// `root`, with their whole subtree (the id range up to the next root) and
/// their extension totals.
fn delete_overlapping(tx: &rusqlite::Transaction, root: &Path, base: u64) -> Result<()> {
    let old: Vec<(i64, String)> = {
        let mut st = tx.prepare_cached("SELECT id, name FROM entries WHERE parent_id=0 AND id<?1")?;
        st.query_map([base as i64], |r| Ok((r.get(0)?, r.get(1)?)))?.collect::<rusqlite::Result<_>>()?
    };
    for (id, name) in old {
        let old_root = Path::new(&name);
        if !safety::is_within(root, old_root) && !safety::is_within(old_root, root) {
            continue;
        }
        let end: i64 =
            tx.query_row("SELECT COALESCE(MIN(id), 9223372036854775807) FROM entries WHERE parent_id=0 AND id>?1", [id], |r| {
                r.get(0)
            })?;
        tx.execute("DELETE FROM entries WHERE id>=?1 AND id<?2", params![id, end])?;
        tx.execute("DELETE FROM ext_stats WHERE root_id=?1", [id])?;
    }
    Ok(())
}

/// Folder accumulators per root (`aggs[k][id - root_base(k)]`) and
/// (root, ext) → (files, bytes).
type WriterOut = (Vec<Vec<Agg>>, std::collections::HashMap<(usize, String), (u64, u64)>);

fn writer(conn: &mut Connection, scan_id: i64, base: u64, roots: usize, rx: Receiver<Msg>) -> Result<WriterOut> {
    let mut aggs: Vec<Vec<Agg>> = vec![Vec::new(); roots];
    let mut exts: std::collections::HashMap<(usize, String), (u64, u64)> = std::collections::HashMap::new();
    let mut tx = conn.transaction()?;
    let mut pending = 0usize;
    let mut last = Instant::now();
    while let Ok(msg) = rx.recv() {
        match msg {
            Msg::Done => break,
            Msg::ReplaceRoot(root) => delete_overlapping(&tx, &root, base)?,
            Msg::Rows(rows) => {
                {
                    let mut st = tx.prepare_cached(
                        "INSERT INTO entries(id,parent_id,name,is_dir,size,mtime,ctime,attrs,ext,scan_id) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)",
                    )?;
                    for r in &rows {
                        let ext = if r.is_dir { None } else { ext_of(&r.name) };
                        let k = ((r.id - base) / ROOT_SPAN) as usize;
                        let root_base = base + k as u64 * ROOT_SPAN;
                        if !r.is_dir {
                            let e = exts.entry((k, ext.clone().unwrap_or_default())).or_default();
                            e.0 += 1;
                            e.1 += r.size;
                        }
                        st.execute(params![
                            r.id as i64,
                            r.parent as i64,
                            r.name,
                            r.is_dir,
                            r.size as i64,
                            r.mtime,
                            r.ctime,
                            r.attrs,
                            ext,
                            (r.parent == 0).then_some(scan_id)
                        ])?;
                        let agg = &mut aggs[k];
                        let i = (r.id - root_base) as usize;
                        if agg.len() <= i {
                            agg.resize((i + 1).max(agg.len() * 2), EMPTY);
                        }
                        let parent = if r.parent == 0 { 0 } else { (r.parent - root_base + 1) as u32 };
                        agg[i] = if r.is_dir {
                            Agg { parent, is_dir: true, ..EMPTY }
                        } else {
                            Agg { parent, is_dir: false, size: r.size, count: 1, newest: r.mtime, oldest: r.mtime }
                        };
                    }
                }
                pending += rows.len();
            }
            Msg::Error(path, message) => {
                tx.execute("INSERT INTO scan_errors(scan_id,path,message) VALUES(?1,?2,?3)", params![scan_id, path, message])?;
            }
        }
        if pending >= 50_000 || last.elapsed() > Duration::from_millis(750) {
            tx.commit()?;
            tx = conn.transaction()?;
            pending = 0;
            last = Instant::now();
        }
    }
    tx.commit()?;
    Ok((aggs, exts))
}

fn aggregate(conn: &mut Connection, base: u64, (mut aggs, exts): WriterOut) -> Result<()> {
    // Children always have larger ids than their parents, so one reverse
    // pass per root rolls every subtree up into its folder.
    for agg in aggs.iter_mut() {
        for i in (0..agg.len()).rev() {
            let a = agg[i];
            if a.parent == 0 {
                continue;
            }
            let parent = &mut agg[a.parent as usize - 1];
            parent.size += a.size;
            parent.count += a.count;
            parent.newest = parent.newest.max(a.newest);
            parent.oldest = parent.oldest.min(a.oldest);
        }
    }
    let tx = conn.transaction()?;
    {
        let mut st =
            tx.prepare("UPDATE entries SET total_size=?2, file_count=?3, newest_mtime=?4, oldest_mtime=?5 WHERE id=?1")?;
        for (k, agg) in aggs.iter().enumerate() {
            let root_base = base + k as u64 * ROOT_SPAN;
            for (i, a) in agg.iter().enumerate() {
                if a.is_dir {
                    let newest = (a.newest != i64::MIN).then_some(a.newest);
                    let oldest = (a.oldest != i64::MAX).then_some(a.oldest);
                    st.execute(params![(root_base + i as u64) as i64, a.size as i64, a.count, newest, oldest])?;
                }
            }
        }
        let mut st = tx.prepare("INSERT OR REPLACE INTO ext_stats(root_id,ext,files,bytes) VALUES(?1,?2,?3,?4)")?;
        for ((k, ext), (files, bytes)) in &exts {
            // Only roots whose row was written (a cancelled scan may not reach some).
            if aggs[*k].first().is_some_and(|a| a.is_dir) {
                st.execute(params![(base + *k as u64 * ROOT_SPAN) as i64, ext, *files as i64, *bytes as i64])?;
            }
        }
    }
    tx.commit()?;
    Ok(())
}

fn disk_for(root: &Path, vols: &[volumes::Volume]) -> Option<u32> {
    let s = root.to_string_lossy().to_ascii_uppercase();
    vols.iter().find(|v| s.starts_with(&v.letter.to_ascii_uppercase())).and_then(|v| v.disk_number)
}

fn media_for(root: &Path, vols: &[volumes::Volume]) -> Media {
    let s = root.to_string_lossy().to_ascii_uppercase();
    vols.iter().find(|v| s.starts_with(&v.letter.to_ascii_uppercase())).map(|v| v.media).unwrap_or(Media::Unknown)
}

/// Requested roots without repeats or roots nested in another requested one
/// (walking both would index the same files twice).
fn distinct_roots(roots: &[PathBuf]) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = Vec::new();
    for (i, r) in roots.iter().enumerate() {
        let covered =
            roots.iter().enumerate().any(|(j, o)| j != i && safety::is_within(r, o) && (!safety::is_within(o, r) || j < i));
        if !covered {
            out.push(r.clone());
        }
    }
    out
}

/// Runs a scan and replaces, in the index, the roots it walks (see the
/// module docs); other indexed roots are kept. Blocking; call from a worker
/// thread. `on_progress` is called roughly every 150 ms.
pub fn run_scan(
    db_path: &Path,
    req: &ScanRequest,
    control: &Control,
    on_progress: &(dyn Fn(&ScanProgress) + Sync),
) -> Result<ScanSummary> {
    if req.roots.is_empty() {
        return Err(Error::Refused("Selecciona al menos una unidad o carpeta.".into()));
    }
    for r in &req.roots {
        if !r.is_absolute() || !r.is_dir() {
            return Err(Error::Refused(format!("No se puede acceder a {}", r.display())));
        }
    }
    let mut conn = crate::db::open(db_path)?;
    let started = Instant::now();
    conn.execute(
        "INSERT INTO scans(started_at,status,mode,roots) VALUES(?1,'running',?2,?3)",
        params![now_unix(), req.mode, serde_json::to_string(&req.roots).unwrap_or_default()],
    )?;
    let scan_id = conn.last_insert_rowid();
    let roots = distinct_roots(&req.roots);
    let base = conn.query_row("SELECT COALESCE(MAX(id),0)+1 FROM entries", [], |r| r.get::<_, i64>(0))? as u64;
    // Drop secondary indexes during the bulk delete/insert and rebuild them
    // once at the end (measurably faster).
    conn.execute_batch(
        "DROP INDEX IF EXISTS entries_parent; DROP INDEX IF EXISTS entries_file_size; DROP INDEX IF EXISTS entries_ext;",
    )?;

    let vols = volumes::list_volumes();
    let counters = Counters::default();
    let (tx, rx) = sync_channel::<Msg>(256);
    let ctx = Ctx {
        tx,
        control,
        counters: &counters,
        exclusions: req.exclusions.iter().map(|e| e.trim_end_matches('\\').to_lowercase()).collect(),
        include_hidden: req.include_hidden,
    };
    let current_root = Mutex::new(String::new());
    let walking = AtomicBool::new(true);
    let snapshot = |phase: &str| ScanProgress {
        scan_id,
        phase: phase.into(),
        root: current_root.lock().unwrap().clone(),
        current: counters.current.lock().unwrap().clone(),
        files: counters.files.load(Ordering::Relaxed),
        dirs: counters.dirs.load(Ordering::Relaxed),
        bytes: counters.bytes.load(Ordering::Relaxed),
        errors: counters.errors.load(Ordering::Relaxed),
        elapsed_ms: started.elapsed().as_millis() as u64,
    };

    // Roots grouped by physical disk: disks are walked in parallel, roots on
    // the same disk sequentially.
    let mut by_disk: Vec<(Option<u32>, Vec<usize>)> = Vec::new();
    for (k, root) in roots.iter().enumerate() {
        let disk = disk_for(root, &vols);
        match by_disk.iter_mut().find(|(d, _)| d.is_some() && *d == disk) {
            Some((_, ks)) => ks.push(k),
            None => by_disk.push((disk, vec![k])),
        }
    }
    let active: Mutex<Vec<String>> = Mutex::new(Vec::new());
    let walk_disk = |ks: &[usize]| -> Result<()> {
        for &k in ks {
            if control.is_cancelled() {
                break;
            }
            let root = &roots[k];
            let label = root.to_string_lossy().into_owned();
            active.lock().unwrap().push(label.clone());
            *current_root.lock().unwrap() = active.lock().unwrap().join(" · ");
            let md = std::fs::metadata(root)?;
            // A root is replaced only once its walk starts, so cancelling
            // keeps the old index of roots not reached yet.
            let _ = ctx.tx.send(Msg::ReplaceRoot(root.clone()));
            let id = base + k as u64 * ROOT_SPAN;
            let _ = ctx.tx.send(Msg::Rows(vec![Row {
                id,
                parent: 0,
                name: label.clone(),
                is_dir: true,
                size: 0,
                mtime: fsx::mtime_of(&md),
                ctime: 0,
                attrs: 0,
            }]));
            counters.dirs.fetch_add(1, Ordering::Relaxed);
            let threads = volumes::walker_threads(req.profile, media_for(root, &vols));
            walk(root.clone(), id, threads, &ctx, &AtomicU64::new(id + 1));
            active.lock().unwrap().retain(|a| *a != label);
            *current_root.lock().unwrap() = active.lock().unwrap().join(" · ");
        }
        Ok(())
    };
    let nroots = roots.len();
    let agg = std::thread::scope(|s| -> Result<WriterOut> {
        let w = s.spawn(|| writer(&mut conn, scan_id, base, nroots, rx));
        s.spawn(|| {
            while walking.load(Ordering::Relaxed) {
                on_progress(&snapshot("enumerating"));
                std::thread::sleep(Duration::from_millis(150));
            }
        });
        let walkers: Vec<_> = by_disk.iter().map(|(_, ks)| s.spawn(|| walk_disk(ks))).collect();
        let mut walk_err = None;
        for h in walkers {
            if let Err(e) = h.join().map_err(|_| Error::Other("walker thread panicked".into())).and_then(|r| r) {
                walk_err.get_or_insert(e);
            }
        }
        let _ = ctx.tx.send(Msg::Done);
        let res = w.join().map_err(|_| Error::Other("writer thread panicked".into()))?;
        walking.store(false, Ordering::Relaxed);
        match walk_err {
            Some(e) => Err(e),
            None => res,
        }
    });
    let agg = match agg {
        Ok(a) => a,
        Err(e) => {
            let _ = create_indexes(&conn);
            conn.execute("UPDATE scans SET status='failed', finished_at=?2 WHERE id=?1", params![scan_id, now_unix()])?;
            return Err(e);
        }
    };
    on_progress(&snapshot("aggregating"));
    aggregate(&mut conn, base, agg)?;
    on_progress(&snapshot("indexing"));
    create_indexes(&conn)?;
    let status = if control.is_cancelled() { "cancelled" } else { "done" };
    let p = snapshot(status);
    conn.execute(
        "UPDATE scans SET status=?2, finished_at=?3, files=?4, dirs=?5, bytes=?6, errors=?7 WHERE id=?1",
        params![scan_id, status, now_unix(), p.files as i64, p.dirs as i64, p.bytes as i64, p.errors as i64],
    )?;
    on_progress(&p);
    Ok(ScanSummary {
        scan_id,
        status: status.into(),
        files: p.files,
        dirs: p.dirs,
        bytes: p.bytes,
        errors: p.errors,
        elapsed_ms: p.elapsed_ms,
    })
}

fn create_indexes(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch(
        "CREATE INDEX IF NOT EXISTS entries_parent ON entries(parent_id);
         CREATE INDEX IF NOT EXISTS entries_file_size ON entries(is_dir, size);
         CREATE INDEX IF NOT EXISTS entries_ext ON entries(ext) WHERE ext IS NOT NULL;
         PRAGMA analysis_limit=1000;
         ANALYZE;",
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nested_and_repeated_roots_are_walked_once() {
        let p = PathBuf::from;
        let got = distinct_roots(&[p(r"D:\Fotos"), p(r"E:\"), p(r"d:\"), p(r"E:\"), p(r"D:\Fotos\2020")]);
        assert_eq!(got, vec![p(r"E:\"), p(r"d:\")]);
    }

    #[test]
    fn ext_parsing() {
        assert_eq!(ext_of("a.ISO").as_deref(), Some("iso"));
        assert_eq!(ext_of(".gitignore"), None);
        assert_eq!(ext_of("noext"), None);
        assert_eq!(ext_of("Backup 2025.part 1"), None);
        assert_eq!(ext_of("x.tar.gz").as_deref(), Some("gz"));
    }
}
