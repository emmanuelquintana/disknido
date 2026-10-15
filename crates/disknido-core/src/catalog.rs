//! Catalog of archives: the list of files inside every ZIP DiskNido created
//! (with their BLAKE3) or that the user added, so "where is X?" can be
//! answered without opening anything. Removing an archive from the catalog
//! never touches the archive file.

use crate::{Error, Result, safety};
use rusqlite::{Connection, OptionalExtension, params};
use serde::Serialize;
use std::path::Path;

/// One file recorded inside an archive.
pub struct NewEntry<'a> {
    pub rel_path: &'a str,
    pub size: u64,
    pub mtime: Option<i64>,
    pub blake3: Option<String>,
}

pub struct NewArchive<'a> {
    pub path: &'a Path,
    pub source: Option<&'a Path>,
    pub origin: &'a str,
    pub archive_bytes: u64,
    pub verified: bool,
}

fn name_of(rel: &str) -> &str {
    rel.trim_end_matches('/').rsplit('/').next().unwrap_or(rel)
}

/// Records (or replaces) an archive and its file list in one transaction.
pub fn record<'a>(conn: &Connection, a: &NewArchive, entries: impl IntoIterator<Item = NewEntry<'a>>) -> Result<i64> {
    let tx = conn.unchecked_transaction()?;
    let key = safety::path_key(a.path);
    tx.execute("DELETE FROM archives WHERE path_key=?1", [&key])?;
    tx.execute(
        "INSERT INTO archives(path,path_key,search_key,source,origin,created_at,files,bytes,archive_bytes,verified)
         VALUES(?1,?2,?8,?3,?4,?5,0,0,?6,?7)",
        params![
            a.path.to_string_lossy(),
            key,
            a.source.map(|s| s.to_string_lossy().into_owned()),
            a.origin,
            crate::now_unix(),
            a.archive_bytes as i64,
            a.verified,
            crate::backups::fold(&a.path.to_string_lossy())
        ],
    )?;
    let id = tx.last_insert_rowid();
    let (mut files, mut bytes) = (0i64, 0i64);
    {
        let mut st = tx.prepare(
            "INSERT INTO archive_entries(archive_id,rel_path,name,search_key,size,mtime,blake3) VALUES(?1,?2,?3,?7,?4,?5,?6)",
        )?;
        for e in entries {
            st.execute(params![
                id,
                e.rel_path,
                name_of(e.rel_path),
                e.size as i64,
                e.mtime,
                e.blake3,
                crate::backups::fold(e.rel_path)
            ])?;
            files += 1;
            bytes += e.size as i64;
        }
    }
    tx.execute("UPDATE archives SET files=?2, bytes=?3 WHERE id=?1", params![id, files, bytes])?;
    tx.commit()?;
    Ok(id)
}

/// Adds an existing ZIP by reading its central directory (no extraction, no
/// hashes). The file is opened read-only.
pub fn import_zip(conn: &Connection, path: &Path) -> Result<ArchiveInfo> {
    safety::check_lexical(path)?;
    let md = std::fs::metadata(path).map_err(|_| Error::Refused(format!("No existe: {}", path.display())))?;
    if crate::fsx::is_cloud_only_attrs(attrs_of(&md)) {
        return Err(Error::Refused("Solo está en la nube; descárgalo antes de añadirlo".into()));
    }
    let mut zip =
        zip::ZipArchive::new(std::fs::File::open(path)?).map_err(|e| Error::Refused(format!("No es un ZIP válido: {e}")))?;
    let mut rows = Vec::with_capacity(zip.len());
    for i in 0..zip.len() {
        let f = zip.by_index_raw(i)?;
        if f.is_dir() {
            continue;
        }
        let mtime = f
            .last_modified()
            .and_then(|d| {
                chrono::NaiveDate::from_ymd_opt(d.year() as i32, d.month() as u32, d.day() as u32)?.and_hms_opt(
                    d.hour() as u32,
                    d.minute() as u32,
                    d.second() as u32,
                )
            })
            .map(|n| n.and_utc().timestamp());
        rows.push((f.name()?.into_owned(), f.size(), mtime));
    }
    let id = record(
        conn,
        &NewArchive { path, source: None, origin: "imported", archive_bytes: md.len(), verified: false },
        rows.iter().map(|(n, s, m)| NewEntry { rel_path: n, size: *s, mtime: *m, blake3: None }),
    )?;
    get(conn, id)?.ok_or_else(|| Error::Other("catalog row missing".into()))
}

#[cfg(windows)]
fn attrs_of(md: &std::fs::Metadata) -> u32 {
    use std::os::windows::fs::MetadataExt;
    md.file_attributes()
}
#[cfg(not(windows))]
fn attrs_of(_md: &std::fs::Metadata) -> u32 {
    0
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ArchiveInfo {
    pub id: i64,
    pub path: String,
    pub source: Option<String>,
    pub origin: String,
    pub created_at: i64,
    pub files: u64,
    pub bytes: u64,
    pub archive_bytes: u64,
    pub verified: bool,
    /// The archive file is still where the catalog says.
    pub exists: bool,
}

const COLS: &str = "id, path, source, origin, created_at, files, bytes, archive_bytes, verified";

fn info(r: &rusqlite::Row) -> rusqlite::Result<ArchiveInfo> {
    let path: String = r.get(1)?;
    Ok(ArchiveInfo {
        id: r.get(0)?,
        exists: Path::new(&path).is_file(),
        path,
        source: r.get(2)?,
        origin: r.get(3)?,
        created_at: r.get(4)?,
        files: r.get::<_, i64>(5)? as u64,
        bytes: r.get::<_, i64>(6)? as u64,
        archive_bytes: r.get::<_, i64>(7)? as u64,
        verified: r.get(8)?,
    })
}

pub fn get(conn: &Connection, id: i64) -> Result<Option<ArchiveInfo>> {
    Ok(conn.query_row(&format!("SELECT {COLS} FROM archives WHERE id=?1"), [id], info).optional()?)
}

pub fn list(conn: &Connection) -> Result<Vec<ArchiveInfo>> {
    let mut st = conn.prepare(&format!("SELECT {COLS} FROM archives ORDER BY created_at DESC"))?;
    let v = st.query_map([], info)?.collect::<rusqlite::Result<_>>()?;
    Ok(v)
}

/// Removes the catalog record only; the archive file is not touched.
pub fn forget(conn: &Connection, id: i64) -> Result<()> {
    conn.execute("DELETE FROM archives WHERE id=?1", [id])?;
    Ok(())
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Hit {
    pub archive_id: i64,
    pub archive_path: String,
    pub rel_path: String,
    pub name: String,
    pub size: u64,
    pub mtime: Option<i64>,
    pub blake3: Option<String>,
}

/// Search by words in the path inside archives or in the archive's own
/// path; case- and accent-insensitive ("cancun" finds "Cancún").
pub fn search(conn: &Connection, needle: &str, limit: i64) -> Result<Vec<Hit>> {
    let words: Vec<String> =
        crate::backups::fold(needle).split_whitespace().map(|w| format!("%{}%", w.replace(['%', '_'], ""))).collect();
    if words.is_empty() {
        return Ok(Vec::new());
    }
    // Every word must appear in the inner path or in the archive path.
    let cond = (0..words.len())
        .map(|i| format!("(e.search_key LIKE ?{0} OR a.search_key LIKE ?{0})", i + 1))
        .collect::<Vec<_>>()
        .join(" AND ");
    let sql = format!(
        "SELECT a.id, a.path, e.rel_path, e.name, e.size, e.mtime, e.blake3 FROM archive_entries e
         JOIN archives a ON a.id=e.archive_id WHERE {cond} ORDER BY e.name LIMIT {}",
        limit.clamp(1, 2000)
    );
    let mut st = conn.prepare(&sql)?;
    let v = st
        .query_map(rusqlite::params_from_iter(words.iter()), |r| {
            Ok(Hit {
                archive_id: r.get(0)?,
                archive_path: r.get(1)?,
                rel_path: r.get(2)?,
                name: r.get(3)?,
                size: r.get::<_, i64>(4)? as u64,
                mtime: r.get(5)?,
                blake3: r.get(6)?,
            })
        })?
        .collect::<rusqlite::Result<_>>()?;
    Ok(v)
}

/// All files of one archive (for browsing and CSV export).
pub fn entries(conn: &Connection, id: i64, offset: i64, limit: i64) -> Result<Vec<Hit>> {
    let mut st = conn.prepare(
        "SELECT a.id, a.path, e.rel_path, e.name, e.size, e.mtime, e.blake3 FROM archive_entries e
         JOIN archives a ON a.id=e.archive_id WHERE a.id=?1 ORDER BY e.rel_path LIMIT ?2 OFFSET ?3",
    )?;
    let v = st
        .query_map(params![id, limit, offset], |r| {
            Ok(Hit {
                archive_id: r.get(0)?,
                archive_path: r.get(1)?,
                rel_path: r.get(2)?,
                name: r.get(3)?,
                size: r.get::<_, i64>(4)? as u64,
                mtime: r.get(5)?,
                blake3: r.get(6)?,
            })
        })?
        .collect::<rusqlite::Result<_>>()?;
    Ok(v)
}

pub fn export_csv(conn: &Connection, id: i64) -> Result<String> {
    let mut out = String::from("archive,path_in_archive,size_bytes,blake3\n");
    for h in entries(conn, id, 0, i64::MAX)? {
        out.push_str(&format!(
            "\"{}\",\"{}\",{},{}\n",
            h.archive_path.replace('"', "\"\""),
            h.rel_path.replace('"', "\"\""),
            h.size,
            h.blake3.unwrap_or_default()
        ));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn record_search_forget() {
        let c = crate::db::open_in_memory().unwrap();
        let id = record(
            &c,
            &NewArchive {
                path: Path::new(r"D:\Backups\Backup Enero 2025.zip"),
                source: None,
                origin: "created",
                archive_bytes: 10,
                verified: true,
            },
            [
                NewEntry { rel_path: "fotos/Vacaciones Cancún.jpg", size: 5, mtime: Some(1), blake3: Some("ab".into()) },
                NewEntry { rel_path: "docs/informe.pdf", size: 7, mtime: None, blake3: None },
            ],
        )
        .unwrap();
        let a = get(&c, id).unwrap().unwrap();
        assert_eq!((a.files, a.bytes), (2, 12));
        assert_eq!(search(&c, "CANCUN", 10).unwrap().len(), 1, "case- and accent-insensitive");
        assert_eq!(search(&c, "VACACIONES", 10).unwrap()[0].name, "Vacaciones Cancún.jpg");
        assert_eq!(search(&c, "enero informe", 10).unwrap().len(), 1, "words can match archive or inner path");
        assert!(search(&c, "nada", 10).unwrap().is_empty());
        // recording the same path again replaces it
        record(
            &c,
            &NewArchive {
                path: Path::new(r"d:\backups\BACKUP ENERO 2025.zip"),
                source: None,
                origin: "imported",
                archive_bytes: 1,
                verified: false,
            },
            [],
        )
        .unwrap();
        assert_eq!(list(&c).unwrap().len(), 1);
        forget(&c, list(&c).unwrap()[0].id).unwrap();
        assert!(list(&c).unwrap().is_empty());
    }
}
