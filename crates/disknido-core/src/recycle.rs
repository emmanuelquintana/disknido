//! Recycle Bin integration. Files are sent to the bin (restorable), never
//! deleted permanently by DiskNido. If Windows cannot recycle an item (for
//! example a drive without a Recycle Bin) its own warning dialog appears,
//! because FOF_WANTNUKEWARNING is set.

use crate::duplicates;
use crate::{Control, Error, Result, fsx, ops, safety};
use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[cfg(windows)]
fn shell_recycle(path: &Path) -> Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::UI::Shell::{
        FO_DELETE, FOF_ALLOWUNDO, FOF_NOCONFIRMATION, FOF_NOERRORUI, FOF_SILENT, FOF_WANTNUKEWARNING, SHFILEOPSTRUCTW,
        SHFileOperationW,
    };
    // SHFileOperation needs a plain absolute path, double-NUL terminated,
    // and does not accept the \\?\ prefix (so MAX_PATH applies).
    let s = path.as_os_str();
    if s.len() >= 260 {
        return Err(Error::Refused("La ruta es demasiado larga para la Papelera de Windows; muévelo manualmente".into()));
    }
    let mut from: Vec<u16> = s.encode_wide().collect();
    from.extend([0, 0]);
    let mut op: SHFILEOPSTRUCTW = unsafe { std::mem::zeroed() };
    op.wFunc = FO_DELETE;
    op.pFrom = from.as_ptr();
    op.fFlags = (FOF_ALLOWUNDO | FOF_NOCONFIRMATION | FOF_WANTNUKEWARNING | FOF_NOERRORUI | FOF_SILENT) as u16;
    let rc = unsafe { SHFileOperationW(&mut op) };
    if rc != 0 {
        return Err(Error::Other(format!("Windows no pudo enviarlo a la Papelera (código {rc}). ¿Está en uso?")));
    }
    if op.fAnyOperationsAborted != 0 {
        return Err(Error::Cancelled);
    }
    if path.exists() {
        return Err(Error::Other("El archivo sigue en su lugar".into()));
    }
    Ok(())
}

#[cfg(not(windows))]
fn shell_recycle(_path: &Path) -> Result<()> {
    Err(Error::Refused("La Papelera solo está disponible en Windows".into()))
}

#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct BinInfo {
    pub items: u64,
    pub bytes: u64,
}

#[cfg(windows)]
pub fn bin_info() -> BinInfo {
    use windows_sys::Win32::UI::Shell::{SHQUERYRBINFO, SHQueryRecycleBinW};
    let mut info: SHQUERYRBINFO = unsafe { std::mem::zeroed() };
    info.cbSize = std::mem::size_of::<SHQUERYRBINFO>() as u32;
    if unsafe { SHQueryRecycleBinW(std::ptr::null(), &mut info) } != 0 {
        return BinInfo::default();
    }
    BinInfo { items: info.i64NumItems as u64, bytes: info.i64Size as u64 }
}

#[cfg(not(windows))]
pub fn bin_info() -> BinInfo {
    BinInfo::default()
}

/// Empties the Recycle Bin of all drives. Irreversible; the UI must confirm.
#[cfg(windows)]
pub fn empty_bin(conn: &Connection) -> Result<BinInfo> {
    use windows_sys::Win32::UI::Shell::{SHERB_NOPROGRESSUI, SHERB_NOSOUND, SHEmptyRecycleBinW};
    let before = bin_info();
    let op =
        ops::begin(conn, "empty_recycle_bin", &serde_json::json!({ "items": before.items, "bytes": before.bytes }), None, false)?;
    let rc = unsafe { SHEmptyRecycleBinW(std::ptr::null_mut(), std::ptr::null(), SHERB_NOPROGRESSUI | SHERB_NOSOUND) };
    // S_OK, or E_UNEXPECTED when it was already empty.
    let ok = rc == 0 || bin_info().items == 0;
    ops::finish(
        conn,
        op,
        if ok { "done" } else { "failed" },
        before.bytes,
        None,
        (!ok).then_some("Windows no pudo vaciar la Papelera"),
    )?;
    if ok { Ok(before) } else { Err(Error::Other("Windows no pudo vaciar la Papelera".into())) }
}

#[cfg(not(windows))]
pub fn empty_bin(_conn: &Connection) -> Result<BinInfo> {
    Err(Error::Refused("Solo en Windows".into()))
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecycleTarget {
    pub path: PathBuf,
    pub expected: Option<safety::Expected>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecycleResult {
    pub op_id: i64,
    pub status: String,
    pub recycled: usize,
    pub failed: usize,
    pub bytes_sent_to_bin: u64,
    pub free_before: Option<u64>,
    pub free_after: Option<u64>,
    pub messages: Vec<String>,
}

fn free_of(p: &Path) -> Option<u64> {
    let root = p.to_string_lossy().get(..3).map(PathBuf::from)?;
    fsx::free_space(&root).ok().map(|f| f.0)
}

/// Sends files to the Recycle Bin after per-item validation. `check` runs
/// extra rule-specific checks (e.g. "still byte-identical to the kept copy").
pub fn recycle(
    conn: &Connection,
    kind: &str,
    targets: &[RecycleTarget],
    allow_appdata: bool,
    control: &Control,
    check: &dyn Fn(&RecycleTarget) -> Result<()>,
) -> Result<RecycleResult> {
    let free_before = targets.first().and_then(|t| free_of(&t.path));
    let summary = serde_json::json!({ "kind": kind, "count": targets.len() });
    let op = ops::begin(conn, "recycle", &summary, free_before, true)?;
    for (i, t) in targets.iter().enumerate() {
        ops::add_item(conn, op, i as i64, &t.path.to_string_lossy(), None)?;
    }
    let (mut ok, mut failed, mut bytes, mut messages) = (0, 0, 0u64, Vec::new());
    for (i, t) in targets.iter().enumerate() {
        if control.is_cancelled() {
            break;
        }
        let seq = i as i64;
        ops::item_status(conn, op, seq, "running", None)?;
        let res = (|| -> Result<u64> {
            let md = safety::validate_source(&t.path, t.expected, allow_appdata)?;
            check(t)?;
            shell_recycle(&t.path)?;
            Ok(md.len())
        })();
        match res {
            Ok(n) => {
                ok += 1;
                bytes += n;
                ops::item_status(conn, op, seq, "done", None)?;
                // The file is gone from its folder: drop it from the index so
                // no list keeps showing it.
                crate::query::forget_path(conn, &t.path)?;
            }
            Err(e) => {
                failed += 1;
                messages.push(format!("{}: {e}", t.path.display()));
                ops::item_status(conn, op, seq, "failed", Some(&e.to_string()))?;
            }
        }
    }
    let free_after = targets.first().and_then(|t| free_of(&t.path));
    let status = ops::status_from(ok, failed, control.is_cancelled());
    ops::finish(conn, op, status, bytes, free_after, None)?;
    Ok(RecycleResult {
        op_id: op,
        status: status.into(),
        recycled: ok,
        failed,
        bytes_sent_to_bin: bytes,
        free_before,
        free_after,
        messages,
    })
}

/// Recycles redundant copies of one duplicate group (identical BLAKE3). The kept copy
/// must still exist, and every removed copy is compared byte by byte with
/// it immediately before being recycled.
pub fn recycle_duplicates(
    conn: &Connection,
    group_id: i64,
    keep: &Path,
    remove: &[PathBuf],
    control: &Control,
) -> Result<RecycleResult> {
    // Groups have identical BLAKE3; the bytes of every removed copy are
    // compared with the kept copy below, right before it is recycled.
    let size: i64 = conn.query_row("SELECT size FROM dup_groups WHERE id=?1", [group_id], |r| r.get(0))?;
    let mut st = conn.prepare("SELECT path, mtime, protected_reason FROM dup_files WHERE group_id=?1")?;
    let files: Vec<(String, i64, Option<String>)> =
        st.query_map([group_id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?.collect::<rusqlite::Result<_>>()?;
    let find = |p: &Path| files.iter().find(|f| safety::path_key(Path::new(&f.0)) == safety::path_key(p));
    let keep_row = find(keep).ok_or_else(|| Error::Refused("La copia a conservar no pertenece al grupo".into()))?;
    safety::validate_source(keep, Some(safety::Expected { size: size as u64, mtime: keep_row.1 }), true)?;
    let mut targets = Vec::new();
    for p in remove {
        let row = find(p).ok_or_else(|| Error::Refused(format!("{} no pertenece al grupo", p.display())))?;
        if safety::path_key(p) == safety::path_key(keep) {
            return Err(Error::Refused("No puedes retirar la copia que conservas".into()));
        }
        if let Some(r) = &row.2 {
            return Err(Error::Refused(format!("{} está protegido: {r}", p.display())));
        }
        targets.push(RecycleTarget { path: p.clone(), expected: Some(safety::Expected { size: size as u64, mtime: row.1 }) });
    }
    if targets.len() >= files.len() {
        return Err(Error::Refused("Debe quedar al menos una copia".into()));
    }
    let res = recycle(conn, "duplicates", &targets, false, control, &|t| {
        if duplicates::files_identical(keep, &t.path, control)? {
            Ok(())
        } else {
            Err(Error::Refused("Ya no es idéntico a la copia conservada".into()))
        }
    })?;
    // Remove recycled paths from the stored group.
    for t in &targets {
        if !t.path.exists() {
            conn.execute("DELETE FROM dup_files WHERE group_id=?1 AND path=?2", params![group_id, t.path.to_string_lossy()])?;
        }
    }
    conn.execute("DELETE FROM dup_groups WHERE id=?1 AND (SELECT COUNT(*) FROM dup_files WHERE group_id=?1) < 2", [group_id])?;
    Ok(res)
}
