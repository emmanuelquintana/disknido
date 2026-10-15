//! Disk images (ISO, BIN/CUE, MDF/MDS, CHD, RVZ…). Platform hints are only
//! given when the evidence is verifiable: a platform-specific container
//! format, or the ISO 9660 volume label read from the file header.

use crate::classify::DISK_IMAGE_EXTS;
use crate::query::PathResolver;
use crate::{Result, fsx};
use rusqlite::Connection;
use serde::Serialize;
use std::collections::HashMap;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiskImage {
    pub id: i64,
    pub name: String,
    pub path: String,
    pub folder: String,
    pub volume: String,
    pub format: String,
    pub size: u64,
    pub mtime: i64,
    pub ctime: i64,
    pub cloud_only: bool,
    /// gameConsole | operatingSystem | unknown
    pub kind: String,
    pub hint: Option<String>,
    pub evidence: Option<String>,
    pub volume_label: Option<String>,
    /// Paths of files that belong to the same set (e.g. .cue + .bin tracks).
    pub related: Vec<String>,
    /// Related files referenced by a descriptor but not found.
    pub missing: Vec<String>,
    /// Size of the whole set (this file + related).
    pub set_size: u64,
}

fn format_hint(ext: &str) -> Option<(&'static str, &'static str)> {
    Some(match ext {
        "rvz" | "wbfs" => ("gameConsole", "Nintendo GameCube / Wii"),
        "wud" | "wux" => ("gameConsole", "Nintendo Wii U"),
        "cso" | "zso" => ("gameConsole", "Sony PSP / PS2"),
        "gdi" | "cdi" => ("gameConsole", "Sega Dreamcast"),
        _ => return None,
    })
}

/// ISO 9660 primary volume descriptor at sector 16: "CD001" and the volume id.
pub fn iso_label(path: &Path) -> Option<String> {
    let mut f = std::fs::File::open(path).ok()?;
    f.seek(SeekFrom::Start(0x8000)).ok()?;
    let mut buf = [0u8; 72];
    f.read_exact(&mut buf).ok()?;
    if &buf[1..6] != b"CD001" || buf[0] != 1 {
        return None;
    }
    let label = String::from_utf8_lossy(&buf[40..72]).trim().to_string();
    (!label.is_empty()).then_some(label)
}

fn os_hint(label: &str) -> Option<&'static str> {
    let l = label.to_ascii_uppercase();
    if l.starts_with("CCSA")
        || l.starts_with("CPBA")
        || l.starts_with("CENA")
        || l.starts_with("J_CCSA")
        || l.contains("WIN") && l.contains("X64")
    {
        Some("Instalación de Windows")
    } else if ["UBUNTU", "DEBIAN", "FEDORA", "LINUX MINT", "ARCH_", "MANJARO", "KALI"].iter().any(|k| l.contains(k)) {
        Some("Distribución Linux")
    } else {
        None
    }
}

/// File names referenced by a .cue or .gdi sheet.
pub fn sheet_references(path: &Path) -> Vec<String> {
    let Ok(text) = std::fs::read_to_string(path) else { return Vec::new() };
    let gdi = path.extension().is_some_and(|e| e.eq_ignore_ascii_case("gdi"));
    let mut out = Vec::new();
    for line in text.lines().take(500) {
        let line = line.trim();
        if gdi {
            // "<track> <lba> <type> <sector size> <file> <offset>"
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 6 {
                out.push(parts[4..parts.len() - 1].join(" ").trim_matches('"').to_string());
            }
        } else if line.len() > 5 && line[..5].eq_ignore_ascii_case("FILE ") {
            let rest = &line[5..];
            let name = if let Some(stripped) = rest.strip_prefix('"') {
                stripped.split('"').next().unwrap_or("")
            } else {
                rest.split_whitespace().next().unwrap_or("")
            };
            if !name.is_empty() {
                out.push(name.to_string());
            }
        }
    }
    out
}

/// Smallest standalone `.bin`/`.img` treated as a disk image (a CD is ~650 MB;
/// small console images exist, firmware blobs are usually far smaller).
const AMBIGUOUS_MIN: u64 = 32 * 1024 * 1024;

type ImgRow = (i64, i64, String, String, i64, i64, i64, u32);

/// A descriptor (.cue/.ccd/.mds/.toc) with the same stem sits in the folder.
fn has_sheet(rows: &[ImgRow], siblings: &[usize], name: &str) -> bool {
    let stem = Path::new(name).file_stem().map(|s| s.to_string_lossy().to_lowercase()).unwrap_or_default();
    siblings.iter().any(|&j| {
        let (n, e) = (&rows[j].2, rows[j].3.as_str());
        matches!(e, "cue" | "ccd" | "mds" | "gdi")
            && (Path::new(n).file_stem().is_some_and(|s| s.to_string_lossy().to_lowercase() == stem) || e == "gdi" || e == "cue")
    })
}

pub fn list(conn: &Connection, read_headers: bool) -> Result<Vec<DiskImage>> {
    let mut res = PathResolver::load(conn)?;
    let placeholders = DISK_IMAGE_EXTS.iter().map(|e| format!("'{e}'")).collect::<Vec<_>>().join(",");
    let mut st = conn.prepare(&format!(
        "SELECT id, parent_id, name, ext, size, mtime, ctime, attrs FROM entries WHERE is_dir=0 AND ext IN ({placeholders})"
    ))?;
    let rows: Vec<ImgRow> = st
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?, r.get(6)?, r.get(7)?)))?
        .collect::<rusqlite::Result<_>>()?;
    // Index by folder for set detection.
    let mut by_folder: HashMap<i64, Vec<usize>> = HashMap::new();
    for (i, r) in rows.iter().enumerate() {
        by_folder.entry(r.1).or_default().push(i);
    }
    let mut out = Vec::with_capacity(rows.len());
    for (id, parent, name, ext, size, mtime, ctime, attrs) in &rows {
        // `.bin` and `.img` are also used by firmware, game data and many
        // apps. Count them only as part of a set (cue/ccd/mds next to them) or
        // when they are large enough to be a disc.
        if (ext == "bin" || ext == "img") && (*size as u64) < AMBIGUOUS_MIN && !has_sheet(&rows, &by_folder[parent], name) {
            continue;
        }
        let folder = res.dir_path(*parent).unwrap_or_default();
        let path = folder.join(name);
        if crate::safety::is_system_location(&path) {
            continue;
        }
        let cloud_only = fsx::is_cloud_only_attrs(*attrs);
        let mut img = DiskImage {
            id: *id,
            name: name.clone(),
            path: path.to_string_lossy().into_owned(),
            folder: folder.to_string_lossy().into_owned(),
            volume: path.to_string_lossy().get(..2).unwrap_or("").to_ascii_uppercase(),
            format: ext.to_ascii_uppercase(),
            size: *size as u64,
            mtime: *mtime,
            ctime: *ctime,
            cloud_only,
            kind: "unknown".into(),
            hint: None,
            evidence: None,
            volume_label: None,
            related: Vec::new(),
            missing: Vec::new(),
            set_size: *size as u64,
        };
        if let Some((kind, hint)) = format_hint(ext) {
            img.kind = kind.into();
            img.hint = Some(hint.into());
            img.evidence = Some(format!("Formato .{ext}, propio de esa plataforma"));
        }
        if read_headers
            && !cloud_only
            && ext == "iso"
            && let Some(label) = iso_label(&path)
        {
            if let Some(h) = os_hint(&label) {
                img.kind = "operatingSystem".into();
                img.hint = Some(h.into());
                img.evidence = Some(format!("Etiqueta de volumen «{label}»"));
            }
            img.volume_label = Some(label);
        }
        let siblings = &by_folder[parent];
        let stem = Path::new(name).file_stem().map(|s| s.to_string_lossy().to_lowercase()).unwrap_or_default();
        match ext.as_str() {
            "cue" | "gdi" if !cloud_only => {
                for r in sheet_references(&path) {
                    let target = folder.join(&r);
                    if let Some(j) = siblings.iter().find(|&&j| rows[j].2.eq_ignore_ascii_case(&r)) {
                        img.set_size += rows[*j].4 as u64;
                        img.related.push(target.to_string_lossy().into_owned());
                    } else if !target.exists() {
                        img.missing.push(r);
                    } else {
                        img.related.push(target.to_string_lossy().into_owned());
                    }
                }
            }
            "mds" | "mdf" | "ccd" | "img" | "bin" | "cue" => {
                let partners: &[&str] = match ext.as_str() {
                    "mds" => &["mdf"],
                    "mdf" => &["mds"],
                    "ccd" => &["img", "sub"],
                    "img" | "bin" => &["cue", "ccd"],
                    _ => &["bin"],
                };
                for &j in siblings {
                    let (n, e) = (&rows[j].2, &rows[j].3);
                    if partners.contains(&e.as_str())
                        && Path::new(n).file_stem().is_some_and(|s| s.to_string_lossy().to_lowercase() == stem)
                    {
                        img.related.push(folder.join(n).to_string_lossy().into_owned());
                    }
                }
            }
            _ => {}
        }
        out.push(img);
    }
    out.sort_by_key(|i| std::cmp::Reverse(i.size));
    Ok(out)
}

/// When some files of a set are selected for removal, list the remaining
/// members that would become useless.
pub fn orphaned_by(selection: &[PathBuf], images: &[DiskImage]) -> Vec<String> {
    let sel: Vec<String> = selection.iter().map(|p| crate::safety::path_key(p)).collect();
    let mut out = Vec::new();
    for img in images {
        let me = crate::safety::path_key(Path::new(&img.path));
        if !sel.contains(&me) {
            continue;
        }
        for r in &img.related {
            let k = crate::safety::path_key(Path::new(r));
            if !sel.contains(&k) && !out.contains(r) {
                out.push(r.clone());
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cue_references() {
        let d = tempfile::tempdir().unwrap();
        let cue = d.path().join("game.cue");
        std::fs::write(&cue, "FILE \"Game (Track 1).bin\" BINARY\n  TRACK 01 MODE2/2352\nFILE track2.bin BINARY\n").unwrap();
        assert_eq!(sheet_references(&cue), vec!["Game (Track 1).bin".to_string(), "track2.bin".to_string()]);
    }

    #[test]
    fn iso_label_from_header() {
        let d = tempfile::tempdir().unwrap();
        let iso = d.path().join("x.iso");
        let mut data = vec![0u8; 0x8000 + 2048];
        data[0x8000] = 1;
        data[0x8001..0x8006].copy_from_slice(b"CD001");
        let label = b"CCSA_X64FRE_ES-ES_DV9            ";
        data[0x8028..0x8028 + 32].copy_from_slice(&label[..32]);
        std::fs::write(&iso, &data).unwrap();
        assert_eq!(iso_label(&iso).as_deref(), Some("CCSA_X64FRE_ES-ES_DV9"));
        assert_eq!(os_hint("CCSA_X64FRE_ES-ES_DV9"), Some("Instalación de Windows"));
        std::fs::write(&iso, vec![0u8; 100]).unwrap();
        assert_eq!(iso_label(&iso), None);
    }
}
