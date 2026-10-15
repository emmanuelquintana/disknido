//! Read-only queries over the index for the UI. Everything is paginated or
//! aggregated here so the frontend never receives the whole tree.

use crate::classify::{self, Category};
use crate::fsx;
use crate::{Result, safety};
use rusqlite::{Connection, OptionalExtension, params};
use serde::Serialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EntryView {
    pub id: i64,
    pub parent_id: i64,
    pub name: String,
    pub path: String,
    pub is_dir: bool,
    /// Logical size; for folders the total of everything below.
    pub size: u64,
    pub file_count: u64,
    pub mtime: i64,
    pub newest_mtime: Option<i64>,
    pub attrs: u32,
    pub cloud_only: bool,
    pub is_link: bool,
}

/// Resolves full paths from (parent_id, name) using an in-memory map of
/// folders. Folders are far fewer than files, so this stays small.
pub struct PathResolver {
    dirs: HashMap<i64, (i64, String)>,
    cache: HashMap<i64, PathBuf>,
}

impl PathResolver {
    pub fn load(conn: &Connection) -> Result<Self> {
        let mut st = conn.prepare("SELECT id, parent_id, name FROM entries WHERE is_dir=1")?;
        let dirs = st
            .query_map([], |r| Ok((r.get::<_, i64>(0)?, (r.get::<_, i64>(1)?, r.get::<_, String>(2)?))))?
            .collect::<rusqlite::Result<HashMap<_, _>>>()?;
        Ok(Self { dirs, cache: HashMap::new() })
    }

    pub fn dir_path(&mut self, id: i64) -> Option<PathBuf> {
        if let Some(p) = self.cache.get(&id) {
            return Some(p.clone());
        }
        let (parent, name) = self.dirs.get(&id)?.clone();
        let p = if parent == 0 { PathBuf::from(name) } else { self.dir_path(parent)?.join(name) };
        self.cache.insert(id, p.clone());
        Some(p)
    }

    pub fn parent_of(&self, id: i64) -> Option<i64> {
        self.dirs.get(&id).map(|d| d.0)
    }

    pub fn file_path(&mut self, parent: i64, name: &str) -> Option<PathBuf> {
        Some(self.dir_path(parent)?.join(name))
    }
}

/// Path of a single entry via a recursive query (for one-off lookups).
pub fn entry_path(conn: &Connection, id: i64) -> Result<Option<PathBuf>> {
    let mut st = conn.prepare(
        "WITH RECURSIVE up(id, parent_id, name, depth) AS (
             SELECT id, parent_id, name, 0 FROM entries WHERE id=?1
             UNION ALL SELECT e.id, e.parent_id, e.name, up.depth+1 FROM entries e JOIN up ON e.id=up.parent_id)
         SELECT name FROM up ORDER BY depth DESC",
    )?;
    let names: Vec<String> = st.query_map([id], |r| r.get(0))?.collect::<rusqlite::Result<_>>()?;
    if names.is_empty() {
        return Ok(None);
    }
    let mut p = PathBuf::from(&names[0]);
    for n in &names[1..] {
        p.push(n);
    }
    Ok(Some(p))
}

const VIEW_COLS: &str = "id, parent_id, name, is_dir, CASE WHEN is_dir=1 THEN COALESCE(total_size,0) ELSE size END, COALESCE(file_count, CASE WHEN is_dir=1 THEN 0 ELSE 1 END), mtime, newest_mtime, attrs";

fn view(r: &rusqlite::Row, path: String) -> rusqlite::Result<EntryView> {
    let attrs: u32 = r.get(8)?;
    Ok(EntryView {
        id: r.get(0)?,
        parent_id: r.get(1)?,
        name: r.get(2)?,
        path,
        is_dir: r.get(3)?,
        size: r.get::<_, i64>(4)? as u64,
        file_count: r.get::<_, i64>(5)? as u64,
        mtime: r.get(6)?,
        newest_mtime: r.get(7)?,
        attrs,
        cloud_only: fsx::is_cloud_only_attrs(attrs),
        is_link: attrs & fsx::ATTR_REPARSE_POINT != 0,
    })
}

pub fn roots(conn: &Connection) -> Result<Vec<EntryView>> {
    let mut st = conn.prepare(&format!("SELECT {VIEW_COLS} FROM entries WHERE parent_id=0 ORDER BY id"))?;
    let v = st.query_map([], |r| {
        let name: String = r.get(2)?;
        view(r, name)
    })?;
    Ok(v.collect::<rusqlite::Result<_>>()?)
}

/// An indexed root with the scan that last walked it (roots can come from
/// different scans: each scan replaces only the roots it walks).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IndexedRoot {
    #[serde(flatten)]
    pub entry: EntryView,
    pub scan_id: Option<i64>,
    /// When that scan finished (its start if it never finished).
    pub scanned_at: Option<i64>,
    pub scan_status: Option<String>,
}

pub fn indexed_roots(conn: &Connection) -> Result<Vec<IndexedRoot>> {
    let mut st = conn.prepare(&format!(
        "SELECT {VIEW_COLS}, scan_id,
                (SELECT COALESCE(s.finished_at, s.started_at) FROM scans s WHERE s.id=entries.scan_id),
                (SELECT s.status FROM scans s WHERE s.id=entries.scan_id)
         FROM entries WHERE parent_id=0 ORDER BY id"
    ))?;
    let v = st.query_map([], |r| {
        let name: String = r.get(2)?;
        Ok(IndexedRoot { entry: view(r, name)?, scan_id: r.get(9)?, scanned_at: r.get(10)?, scan_status: r.get(11)? })
    })?;
    Ok(v.collect::<rusqlite::Result<_>>()?)
}

/// Children of a folder sorted by size (largest first), paginated.
pub fn children(conn: &Connection, parent_id: i64, offset: i64, limit: i64) -> Result<(Vec<EntryView>, i64)> {
    let parent_path = entry_path(conn, parent_id)?.unwrap_or_default();
    let total: i64 = conn.query_row("SELECT COUNT(*) FROM entries WHERE parent_id=?1", [parent_id], |r| r.get(0))?;
    let mut st = conn.prepare(&format!(
        "SELECT {VIEW_COLS} FROM entries WHERE parent_id=?1
         ORDER BY CASE WHEN is_dir=1 THEN COALESCE(total_size,0) ELSE size END DESC, name LIMIT ?2 OFFSET ?3"
    ))?;
    let v = st.query_map(params![parent_id, limit, offset], |r| {
        let name: String = r.get(2)?;
        view(r, parent_path.join(&name).to_string_lossy().into_owned())
    })?;
    Ok((v.collect::<rusqlite::Result<_>>()?, total))
}

pub fn largest_files(conn: &Connection, limit: i64) -> Result<Vec<EntryView>> {
    let mut res = PathResolver::load(conn)?;
    let mut st = conn.prepare(&format!("SELECT {VIEW_COLS} FROM entries WHERE is_dir=0 ORDER BY size DESC LIMIT ?1"))?;
    let rows: Vec<(i64, String, EntryView)> =
        st.query_map([limit], |r| Ok((r.get(1)?, r.get(2)?, view(r, String::new())?)))?.collect::<rusqlite::Result<_>>()?;
    Ok(rows
        .into_iter()
        .map(|(p, n, mut v)| {
            v.path = res.file_path(p, &n).map(|x| x.to_string_lossy().into_owned()).unwrap_or(n);
            v
        })
        .collect())
}

/// Largest folders that are not just the sum of one bigger listed folder:
/// a folder is skipped when one child holds ≥ 90% of it.
pub fn largest_folders(conn: &Connection, limit: i64) -> Result<Vec<EntryView>> {
    let mut res = PathResolver::load(conn)?;
    let mut st = conn.prepare(&format!(
        "SELECT {VIEW_COLS} FROM entries d WHERE is_dir=1 AND parent_id<>0
           AND NOT EXISTS (SELECT 1 FROM entries c WHERE c.parent_id=d.id AND c.is_dir=1 AND c.total_size*10 >= d.total_size*9)
         ORDER BY total_size DESC LIMIT ?1"
    ))?;
    let rows: Vec<EntryView> = st.query_map([limit], |r| view(r, String::new()))?.collect::<rusqlite::Result<_>>()?;
    Ok(rows
        .into_iter()
        .map(|mut v| {
            v.path = res.dir_path(v.id).map(|x| x.to_string_lossy().into_owned()).unwrap_or_default();
            v
        })
        .collect())
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Bucket {
    pub key: String,
    pub files: u64,
    pub bytes: u64,
}

/// Fills `ext_stats` for indexed roots that have none (indexes written by
/// older versions). One range scan per root; runs once.
fn backfill_ext_stats(conn: &Connection) -> Result<()> {
    let missing: Vec<i64> = {
        let mut st = conn.prepare(
            "SELECT id FROM entries r WHERE parent_id=0 AND NOT EXISTS (SELECT 1 FROM ext_stats s WHERE s.root_id=r.id)",
        )?;
        st.query_map([], |r| r.get(0))?.collect::<rusqlite::Result<_>>()?
    };
    for id in missing {
        let end: i64 = conn.query_row(
            "SELECT COALESCE(MIN(id), 9223372036854775807) FROM entries WHERE parent_id=0 AND id>?1",
            [id],
            |r| r.get(0),
        )?;
        conn.execute(
            "INSERT OR REPLACE INTO ext_stats(root_id,ext,files,bytes)
             SELECT ?1, COALESCE(ext,''), COUNT(*), SUM(size) FROM entries WHERE id>?1 AND id<?2 AND is_dir=0 GROUP BY ext",
            params![id, end],
        )?;
        // A root with no files still gets a marker row so it is not rescanned.
        conn.execute("INSERT OR IGNORE INTO ext_stats(root_id,ext,files,bytes) VALUES(?1,'',0,0)", [id])?;
    }
    Ok(())
}

/// Extension distribution from the per-root totals written at scan time.
pub fn extensions(conn: &Connection, limit: i64) -> Result<Vec<Bucket>> {
    backfill_ext_stats(conn)?;
    let mut st = conn.prepare(
        "SELECT ext, SUM(files), SUM(bytes) FROM ext_stats WHERE root_id IN (SELECT id FROM entries WHERE parent_id=0)
         GROUP BY ext HAVING SUM(files) > 0 ORDER BY SUM(bytes) DESC LIMIT ?1",
    )?;
    let v = st.query_map([limit], |r| {
        Ok(Bucket { key: r.get(0)?, files: r.get::<_, i64>(1)? as u64, bytes: r.get::<_, i64>(2)? as u64 })
    })?;
    Ok(v.collect::<rusqlite::Result<_>>()?)
}

pub fn categories(conn: &Connection) -> Result<Vec<Bucket>> {
    let mut map: HashMap<Category, (u64, u64)> = HashMap::new();
    for b in extensions(conn, i64::MAX)? {
        let ext = (!b.key.is_empty()).then_some(b.key.as_str());
        let e = map.entry(classify::category(ext)).or_default();
        e.0 += b.files;
        e.1 += b.bytes;
    }
    let mut out: Vec<Bucket> = map
        .into_iter()
        .map(|(c, (files, bytes))| Bucket {
            key: serde_json::to_value(c).ok().and_then(|v| v.as_str().map(String::from)).unwrap_or_default(),
            files,
            bytes,
        })
        .collect();
    out.sort_by_key(|b| std::cmp::Reverse(b.bytes));
    Ok(out)
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanInfo {
    pub id: i64,
    pub started_at: i64,
    pub finished_at: Option<i64>,
    pub status: String,
    pub mode: String,
    pub roots: Vec<String>,
    pub files: u64,
    pub dirs: u64,
    pub bytes: u64,
    pub errors: u64,
}

pub fn last_scan(conn: &Connection) -> Result<Option<ScanInfo>> {
    Ok(conn
        .query_row(
            "SELECT id, started_at, finished_at, status, mode, roots, files, dirs, bytes, errors FROM scans
             WHERE status IN ('done','cancelled') ORDER BY id DESC LIMIT 1",
            [],
            |r| {
                let roots: String = r.get(5)?;
                Ok(ScanInfo {
                    id: r.get(0)?,
                    started_at: r.get(1)?,
                    finished_at: r.get(2)?,
                    status: r.get(3)?,
                    mode: r.get(4)?,
                    roots: serde_json::from_str(&roots).unwrap_or_default(),
                    files: r.get::<_, i64>(6)? as u64,
                    dirs: r.get::<_, i64>(7)? as u64,
                    bytes: r.get::<_, i64>(8)? as u64,
                    errors: r.get::<_, i64>(9)? as u64,
                })
            },
        )
        .optional()?)
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanError {
    pub path: String,
    pub message: String,
}

pub fn scan_errors(conn: &Connection, scan_id: i64, limit: i64) -> Result<Vec<ScanError>> {
    let mut st = conn.prepare("SELECT path, message FROM scan_errors WHERE scan_id=?1 LIMIT ?2")?;
    let v = st.query_map(params![scan_id, limit], |r| Ok(ScanError { path: r.get(0)?, message: r.get(1)? }))?;
    Ok(v.collect::<rusqlite::Result<_>>()?)
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OldFolder {
    pub entry: EntryView,
    pub oldest_mtime: Option<i64>,
    pub months_idle: i64,
    pub top_extensions: Vec<Bucket>,
    pub protected_reason: Option<String>,
    pub mark: Option<String>,
    /// Why this looks like an installed application or game (programs and
    /// libraries inside, or a known launcher library). Shown separately: an
    /// installed game is not an "old folder" to archive.
    pub app_reason: Option<String>,
}

/// Folder names used by launchers and installers for installed software.
const APP_LIBRARIES: &[&str] = &[
    "steamapps",
    "steamlibrary",
    "epic games",
    "gog games",
    "gog galaxy",
    "xboxgames",
    "windowsapps",
    "riot games",
    "ea games",
    "ubisoft game launcher",
    "battle.net",
    "program files",
    "program files (x86)",
];

/// Folders whose subtree contains both programs (.exe) and libraries (.dll):
/// a strong sign of an installed application or game. One pass over those
/// files climbing to the root, so it scales to millions of entries.
fn folders_with_programs(conn: &Connection, res: &PathResolver) -> Result<std::collections::HashSet<i64>> {
    use std::collections::HashSet;
    let (mut exe, mut dll): (HashSet<i64>, HashSet<i64>) = (HashSet::new(), HashSet::new());
    let mut st = conn.prepare("SELECT parent_id, ext FROM entries WHERE is_dir=0 AND ext IN ('exe','dll')")?;
    let rows = st.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))?;
    for row in rows {
        let (mut dir, ext) = row?;
        let set = if ext == "exe" { &mut exe } else { &mut dll };
        while dir != 0 && set.insert(dir) {
            dir = res.parent_of(dir).unwrap_or(0);
        }
    }
    Ok(exe.intersection(&dll).copied().collect())
}

/// Folders whose newest file modification is older than `months`. Only the
/// top-most qualifying folder of each subtree is reported. Modification time
/// is the only reliable signal Windows offers; "last used" is not known.
pub fn old_folders(conn: &Connection, months: i64, min_bytes: u64, limit: usize, with_types: bool) -> Result<Vec<OldFolder>> {
    let now = crate::now_unix();
    let cutoff = now - months * 30 * 86_400;
    let mut res = PathResolver::load(conn)?;
    let marks = load_marks(conn)?;
    let programs = folders_with_programs(conn, &res)?;
    let mut st = conn.prepare(&format!(
        "SELECT {VIEW_COLS}, oldest_mtime FROM entries
         WHERE is_dir=1 AND parent_id<>0 AND newest_mtime IS NOT NULL AND newest_mtime < ?1 AND total_size >= ?2
         ORDER BY id"
    ))?;
    let rows: Vec<(EntryView, Option<i64>)> = st
        .query_map(params![cutoff, min_bytes as i64], |r| Ok((view(r, String::new())?, r.get(9)?)))?
        .collect::<rusqlite::Result<_>>()?;
    // Ordered by id: ancestors come first, so a set of accepted ids is enough.
    let mut accepted = std::collections::HashSet::new();
    let mut out = Vec::new();
    for (mut e, oldest) in rows {
        if accepted.contains(&e.parent_id) {
            accepted.insert(e.id);
            continue;
        }
        accepted.insert(e.id);
        let path = res.dir_path(e.id).unwrap_or_default();
        e.path = path.to_string_lossy().into_owned();
        let mark = marks.get(&safety::path_key(&path)).cloned();
        if mark.as_deref() == Some("excluded") || mark.as_deref() == Some("ignored") {
            continue;
        }
        let key = safety::path_key(&path);
        let app_reason = if let Some(lib) = APP_LIBRARIES.iter().find(|l| key.split('\\').any(|c| c == **l)) {
            Some(format!("Está en una biblioteca de programas o juegos («{lib}»)"))
        } else if programs.contains(&e.id) {
            Some("Contiene programas (.exe) y bibliotecas (.dll): parece una aplicación o un juego instalado".to_string())
        } else {
            None
        };
        out.push(OldFolder {
            app_reason,
            months_idle: (now - e.newest_mtime.unwrap_or(now)) / (30 * 86_400),
            protected_reason: safety::protected_reason(&path, false),
            top_extensions: Vec::new(),
            oldest_mtime: oldest,
            mark,
            entry: e,
        });
    }
    out.sort_by_key(|f| std::cmp::Reverse(f.entry.size));
    out.truncate(limit);
    if with_types {
        for f in &mut out {
            f.top_extensions = subtree_extensions(conn, f.entry.id, 3)?;
        }
    }
    Ok(out)
}

pub fn subtree_extensions(conn: &Connection, id: i64, limit: i64) -> Result<Vec<Bucket>> {
    let mut st = conn.prepare(
        "WITH RECURSIVE t(id) AS (SELECT ?1 UNION ALL SELECT e.id FROM entries e JOIN t ON e.parent_id=t.id WHERE e.is_dir=1)
         SELECT COALESCE(e.ext,''), COUNT(*), SUM(e.size) FROM entries e JOIN t ON e.parent_id=t.id
         WHERE e.is_dir=0 GROUP BY e.ext ORDER BY SUM(e.size) DESC LIMIT ?2",
    )?;
    let v = st.query_map(params![id, limit], |r| {
        Ok(Bucket { key: r.get(0)?, files: r.get::<_, i64>(1)? as u64, bytes: r.get::<_, i64>(2)? as u64 })
    })?;
    Ok(v.collect::<rusqlite::Result<_>>()?)
}

pub fn load_marks(conn: &Connection) -> Result<HashMap<String, String>> {
    let mut st = conn.prepare("SELECT path, mark FROM marks")?;
    let v = st.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?;
    Ok(v.collect::<rusqlite::Result<_>>()?)
}

pub fn set_mark(conn: &Connection, path: &Path, mark: Option<&str>) -> Result<()> {
    let key = safety::path_key(path);
    match mark {
        Some(m) => conn.execute(
            "INSERT INTO marks(path, mark, created_at) VALUES(?1,?2,?3) ON CONFLICT(path) DO UPDATE SET mark=excluded.mark",
            params![key, m, crate::now_unix()],
        )?,
        None => conn.execute("DELETE FROM marks WHERE path=?1", [key])?,
    };
    Ok(())
}

/// Case-insensitive name search over the index (paginated).
pub fn search(conn: &Connection, needle: &str, limit: i64) -> Result<Vec<EntryView>> {
    let mut res = PathResolver::load(conn)?;
    let pattern = format!("%{}%", needle.replace(['%', '_'], ""));
    let mut st = conn.prepare(&format!(
        "SELECT {VIEW_COLS} FROM entries WHERE name LIKE ?1 AND parent_id<>0
         ORDER BY CASE WHEN is_dir=1 THEN COALESCE(total_size,0) ELSE size END DESC LIMIT ?2"
    ))?;
    let rows: Vec<EntryView> =
        st.query_map(params![pattern, limit], |r| view(r, String::new()))?.collect::<rusqlite::Result<_>>()?;
    Ok(rows
        .into_iter()
        .map(|mut v| {
            v.path = if v.is_dir { res.dir_path(v.id) } else { res.file_path(v.parent_id, &v.name) }
                .map(|x| x.to_string_lossy().into_owned())
                .unwrap_or_default();
            v
        })
        .collect())
}
