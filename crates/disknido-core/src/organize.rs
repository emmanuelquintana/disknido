//! Backup organisation: plan (dry run) then execute.
//!
//! Each backup moves inside its own volume to
//! `<drive>\<root>\<YYYY>\<YYYY-MM>\<original name>`. Moves are native
//! renames (MoveFileExW without REPLACE_EXISTING or COPY_ALLOWED), so they
//! are atomic, never overwrite and never cross volumes. Folder contents are
//! counted before and after; a mismatch is reported.

use crate::fsx;
use crate::{Control, Error, Result, ops, safety};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanInput {
    pub path: PathBuf,
    pub year: Option<i32>,
    pub month: Option<u32>,
    /// User chose "Sin fecha": goes to `<root>\Sin fecha\`.
    #[serde(default)]
    pub no_date: bool,
    /// File count and size from the index. When given, the preview uses them
    /// instead of walking the folder (a large backup on an HDD takes minutes);
    /// moving still counts every file before and after.
    #[serde(default)]
    pub indexed: Option<(u64, u64)>,
}

#[derive(Debug, Clone, Copy, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum Layout {
    #[default]
    YearMonth,
    Year,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanRequest {
    pub items: Vec<PlanInput>,
    /// Name of the organised folder, e.g. "Backups".
    pub root_name: String,
    /// Optional parent folder per volume ("D:" → `D:\Archivo`). Defaults to
    /// the drive root, giving `D:\Backups`.
    #[serde(default)]
    pub parents: std::collections::HashMap<String, PathBuf>,
    #[serde(default)]
    pub layout: Layout,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ItemStatus {
    Ready,
    /// Destination name was taken; a numbered name is proposed.
    Renamed,
    AlreadyOrganized,
    NeedsDate,
    Refused,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanItem {
    pub src: String,
    pub dst: String,
    pub volume: String,
    pub year: Option<i32>,
    pub month: Option<u32>,
    pub files: u64,
    pub bytes: u64,
    pub status: ItemStatus,
    pub message: Option<String>,
    /// `files`/`bytes` come from the index, not from walking the folder.
    /// Moving then counts the folder itself and verifies the destination
    /// against that count.
    #[serde(default)]
    pub from_index: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Plan {
    pub items: Vec<PlanItem>,
    pub total_files: u64,
    pub total_bytes: u64,
    pub ready: usize,
    pub warnings: Vec<String>,
}

fn drive_root(p: &Path) -> Option<PathBuf> {
    let s = p.to_string_lossy();
    let b = s.as_bytes();
    (b.len() >= 2 && b[1] == b':' && b[0].is_ascii_alphabetic()).then(|| PathBuf::from(format!("{}\\", &s[..2])))
}

fn valid_root_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 64
        && !name.chars().any(|c| matches!(c, '\\' | '/' | ':' | '*' | '?' | '"' | '<' | '>' | '|') || c.is_control())
        && name != "."
        && name != ".."
        && !name.ends_with('.')
        && !name.ends_with(' ')
}

/// Organised base folder for the volume of `src`.
pub fn base_for(src: &Path, req: &PlanRequest) -> Result<PathBuf> {
    let root = drive_root(src).ok_or_else(|| Error::Refused("Solo unidades locales con letra".into()))?;
    let vol = root.to_string_lossy()[..2].to_ascii_uppercase();
    let parent = req.parents.iter().find(|(k, _)| k.eq_ignore_ascii_case(&vol)).map(|(_, v)| v.clone()).unwrap_or(root.clone());
    safety::check_lexical(&parent)?;
    if !safety::is_within(&parent, &root) {
        return Err(Error::Refused(format!("La carpeta de respaldos de {vol} debe estar en la misma unidad")));
    }
    Ok(parent.join(&req.root_name))
}

pub fn target_for(
    src: &Path,
    base: &Path,
    layout: Layout,
    year: Option<i32>,
    month: Option<u32>,
    no_date: bool,
) -> Option<PathBuf> {
    let name = src.file_name()?;
    let mut d = base.to_path_buf();
    if no_date {
        d.push("Sin fecha");
    } else {
        let y = year?;
        d.push(y.to_string());
        if layout == Layout::YearMonth {
            d.push(format!("{y}-{:02}", month?));
        }
    }
    d.push(name);
    Some(d)
}

fn numbered(dst: &Path, n: u32) -> PathBuf {
    let name = dst.file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    dst.with_file_name(format!("{name} ({n})"))
}

pub fn plan(req: &PlanRequest, control: &Control) -> Result<Plan> {
    if !valid_root_name(&req.root_name) {
        return Err(Error::Refused(format!("Nombre de carpeta raíz no válido: «{}»", req.root_name)));
    }
    let mut items = Vec::new();
    let mut taken: HashSet<String> = HashSet::new();
    let mut warnings = Vec::new();
    let srcs: Vec<String> = req.items.iter().map(|i| safety::path_key(&i.path)).collect();
    for (idx, input) in req.items.iter().enumerate() {
        control.checkpoint()?;
        let src = &input.path;
        let volume = drive_root(src).map(|r| r.to_string_lossy()[..2].to_string()).unwrap_or_default();
        let mut it = PlanItem {
            src: src.to_string_lossy().into_owned(),
            dst: String::new(),
            volume,
            year: input.year,
            month: input.month,
            files: 0,
            bytes: 0,
            status: ItemStatus::Ready,
            message: None,
            from_index: false,
        };
        let refuse = |it: &mut PlanItem, msg: String| {
            it.status = ItemStatus::Refused;
            it.message = Some(msg);
        };
        if srcs[..idx].contains(&srcs[idx]) {
            refuse(&mut it, "Aparece dos veces en el plan".into());
            items.push(it);
            continue;
        }
        if let Some(other) = srcs
            .iter()
            .enumerate()
            .find(|(j, k)| *j != idx && safety::is_within(src, Path::new(k.as_str())) && k.as_str() != srcs[idx])
        {
            refuse(&mut it, format!("Está dentro de otro respaldo del plan: {}", other.1));
            items.push(it);
            continue;
        }
        match safety::validate_source(src, None, false) {
            Ok(md) if md.is_dir() => {}
            Ok(_) => {
                refuse(&mut it, "No es una carpeta".into());
                items.push(it);
                continue;
            }
            Err(e) => {
                refuse(&mut it, e.to_string());
                items.push(it);
                continue;
            }
        }
        let base = match base_for(src, req) {
            Ok(b) => b,
            Err(e) => {
                refuse(&mut it, e.to_string());
                items.push(it);
                continue;
            }
        };
        let Some(mut dst) = target_for(src, &base, req.layout, input.year, input.month, input.no_date) else {
            it.status = ItemStatus::NeedsDate;
            it.message = Some("Asigna año y mes, o elige «Sin fecha»".into());
            items.push(it);
            continue;
        };
        if safety::path_key(&dst) == safety::path_key(src) {
            it.status = ItemStatus::AlreadyOrganized;
            it.dst = dst.to_string_lossy().into_owned();
            items.push(it);
            continue;
        }
        if safety::is_within(&dst, src) {
            refuse(&mut it, "El destino quedaría dentro del propio origen".into());
            items.push(it);
            continue;
        }
        if let Some(r) = safety::protected_reason(&dst, false) {
            refuse(&mut it, format!("Destino protegido ({r})"));
            items.push(it);
            continue;
        }
        if let Err(e) = safety::check_no_links(&dst) {
            refuse(&mut it, e.to_string());
            items.push(it);
            continue;
        }
        let mut n = 2;
        while dst.exists() || taken.contains(&safety::path_key(&dst)) {
            dst = numbered(&target_for(src, &base, req.layout, input.year, input.month, input.no_date).unwrap(), n);
            n += 1;
        }
        if n > 2 {
            it.status = ItemStatus::Renamed;
            it.message = Some(format!(
                "Ya existe una carpeta con ese nombre; se usará «{}». No se combina ni se sobrescribe nada.",
                dst.file_name().unwrap().to_string_lossy()
            ));
        }
        taken.insert(safety::path_key(&dst));
        if let Some((f, b)) = input.indexed {
            it.files = f;
            it.bytes = b;
            it.from_index = true;
            it.dst = dst.to_string_lossy().into_owned();
            items.push(it);
            continue;
        }
        match fsx::collect_tree(src, control) {
            Ok(tree) => {
                let (f, b) = fsx::tree_totals(&tree);
                it.files = f;
                it.bytes = b;
                let links = tree.iter().filter(|e| e.item.is_link()).count();
                if links > 0 {
                    it.message.get_or_insert_with(|| format!("Contiene {links} enlaces; se mueven como enlaces, sin seguirlos"));
                }
            }
            Err(e) => {
                refuse(&mut it, format!("No se pudo leer el contenido: {e}"));
                items.push(it);
                continue;
            }
        }
        it.dst = dst.to_string_lossy().into_owned();
        items.push(it);
    }
    let ready: Vec<&PlanItem> = items.iter().filter(|i| matches!(i.status, ItemStatus::Ready | ItemStatus::Renamed)).collect();
    if items.iter().any(|i| i.status == ItemStatus::NeedsDate) {
        warnings.push("Algunos respaldos no tienen fecha confirmada y no se moverán.".into());
    }
    Ok(Plan {
        total_files: ready.iter().map(|i| i.files).sum(),
        total_bytes: ready.iter().map(|i| i.bytes).sum(),
        ready: ready.len(),
        items,
        warnings,
    })
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecResult {
    pub op_id: i64,
    pub status: String,
    pub done: usize,
    pub failed: usize,
    pub messages: Vec<String>,
}

/// Executes the `ready`/`renamed` items of a previously shown plan. Every
/// item is validated again; anything that changed since the preview is
/// skipped rather than adapted silently.
pub fn execute(conn: &Connection, items: &[PlanItem], control: &Control, on_item: &dyn Fn(usize, usize)) -> Result<ExecResult> {
    let todo: Vec<&PlanItem> = items.iter().filter(|i| matches!(i.status, ItemStatus::Ready | ItemStatus::Renamed)).collect();
    let summary = serde_json::json!({ "count": todo.len(), "bytes": todo.iter().map(|i| i.bytes).sum::<u64>() });
    let op = ops::begin(conn, "organize", &summary, None, true)?;
    for (seq, it) in todo.iter().enumerate() {
        ops::add_item(conn, op, seq as i64, &it.src, Some(&it.dst))?;
    }
    let (mut done, mut failed, mut messages) = (0, 0, Vec::new());
    let mut moved_bytes = 0;
    for (seq, it) in todo.iter().enumerate() {
        if control.is_cancelled() {
            break;
        }
        on_item(seq, todo.len());
        let seq = seq as i64;
        ops::item_status(conn, op, seq, "running", None)?;
        match move_one(it, control) {
            Ok(()) => {
                done += 1;
                moved_bytes += it.bytes;
                ops::item_status(conn, op, seq, "done", None)?;
                // Lists show the folder at its new place without a new scan.
                crate::query::move_path(conn, Path::new(&it.src), Path::new(&it.dst))?;
            }
            Err(e) => {
                failed += 1;
                let msg = e.to_string();
                messages.push(format!("{}: {msg}", it.src));
                ops::item_status(conn, op, seq, "failed", Some(&msg))?;
            }
        }
    }
    let status = ops::status_from(done, failed, control.is_cancelled());
    ops::finish(conn, op, status, moved_bytes, None, None)?;
    Ok(ExecResult { op_id: op, status: status.into(), done, failed, messages })
}

fn move_one(it: &PlanItem, control: &Control) -> Result<()> {
    let src = Path::new(&it.src);
    let dst = Path::new(&it.dst);
    safety::validate_source(src, None, false)?;
    if dst.exists() {
        return Err(Error::Refused("El destino apareció después de la vista previa; no se sobrescribe".into()));
    }
    if safety::is_within(dst, src) || drive_root(src) != drive_root(dst) {
        return Err(Error::Refused("Destino no válido".into()));
    }
    safety::check_no_links(dst)?;
    let before = fsx::tree_totals(&fsx::collect_tree(src, control)?);
    if !it.from_index && before != (it.files, it.bytes) {
        return Err(Error::Refused("El contenido cambió desde la vista previa; vuelve a generar el plan".into()));
    }
    std::fs::create_dir_all(dst.parent().unwrap())?;
    fsx::rename_no_replace(src, dst)?;
    let after = fsx::tree_totals(&fsx::collect_tree(dst, control)?);
    if after != before {
        return Err(Error::Other(format!(
            "Verificación: se esperaban {} archivos / {} bytes y hay {} / {} en el destino",
            before.0, before.1, after.0, after.1
        )));
    }
    Ok(())
}

/// Moves organised folders back where they were (only items that are done
/// and whose original location is still free).
pub fn undo(conn: &Connection, op: i64, control: &Control) -> Result<ExecResult> {
    let items = ops::items(conn, op)?;
    let (mut done, mut failed, mut messages) = (0, 0, Vec::new());
    for it in items.iter().filter(|i| i.status == "done") {
        let Some(dst) = &it.dst else { continue };
        let res = (|| -> Result<()> {
            control.checkpoint()?;
            let (from, to) = (Path::new(dst), Path::new(&it.src));
            safety::validate_source(from, None, false)?;
            if to.exists() {
                return Err(Error::Refused("La ubicación original ya está ocupada".into()));
            }
            std::fs::create_dir_all(to.parent().unwrap())?;
            fsx::rename_no_replace(from, to)?;
            Ok(())
        })();
        match res {
            Ok(()) => {
                done += 1;
                ops::item_status(conn, op, it.seq, "reverted", None)?;
                crate::query::move_path(conn, Path::new(dst), Path::new(&it.src))?;
            }
            Err(e) => {
                failed += 1;
                messages.push(format!("{dst}: {e}"));
            }
        }
    }
    Ok(ExecResult { op_id: op, status: ops::status_from(done, failed, false).into(), done, failed, messages })
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    #[test]
    fn targets() {
        let t = target_for(
            Path::new(r"D:\Escritorio\Backup Febrero 2025"),
            Path::new(r"D:\Backups"),
            Layout::YearMonth,
            Some(2025),
            Some(2),
            false,
        );
        assert_eq!(t.unwrap(), PathBuf::from(r"D:\Backups\2025\2025-02\Backup Febrero 2025"));
        let t = target_for(Path::new(r"E:\x\Respaldo 2026"), Path::new(r"E:\Respaldos"), Layout::Year, Some(2026), None, false);
        assert_eq!(t.unwrap(), PathBuf::from(r"E:\Respaldos\2026\Respaldo 2026"));
        assert!(
            target_for(Path::new(r"E:\x\Respaldo"), Path::new(r"E:\Backups"), Layout::YearMonth, Some(2026), None, false)
                .is_none()
        );
        let req = PlanRequest {
            items: vec![],
            root_name: "Backups".into(),
            parents: [("E:".to_string(), PathBuf::from(r"D:\x"))].into(),
            layout: Layout::YearMonth,
        };
        assert!(base_for(Path::new(r"E:\a"), &req).is_err(), "base on another volume is refused");
        assert_eq!(base_for(Path::new(r"D:\a"), &req).unwrap(), PathBuf::from(r"D:\Backups"));
        assert!(valid_root_name("Backups"));
        assert!(!valid_root_name(r"..\Windows"));
        assert!(!valid_root_name("a:b"));
    }
}
