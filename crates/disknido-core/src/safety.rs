//! Path rules enforced in Rust before any mutating operation. The UI may
//! hide buttons, but these checks are the real authorization.

use crate::fsx::{self, ATTR_REPARSE_POINT};
use crate::{Error, Result};
use std::path::{Component, Path, PathBuf};
use std::sync::OnceLock;

static APP_DATA_DIR: OnceLock<PathBuf> = OnceLock::new();

/// The application's own data folder is never a target.
pub fn set_app_data_dir(p: PathBuf) {
    let _ = APP_DATA_DIR.set(p);
}

/// Case-insensitive, separator-normalised key for comparing Windows paths.
pub fn path_key(p: &Path) -> String {
    let s = p.to_string_lossy().replace('/', "\\").to_lowercase();
    let s = s.strip_prefix(r"\\?\").map(str::to_string).unwrap_or(s);
    if s.len() > 3 { s.trim_end_matches('\\').to_string() } else { s }
}

/// `child` equals `parent` or lies inside it (component-wise, any case).
pub fn is_within(child: &Path, parent: &Path) -> bool {
    let c = path_key(child);
    let p = path_key(parent);
    c == p || (c.starts_with(&p) && (p.ends_with('\\') || c[p.len()..].starts_with('\\')))
}

fn env_dir(var: &str) -> Option<PathBuf> {
    std::env::var_os(var).map(PathBuf::from).filter(|p| !p.as_os_str().is_empty())
}

const PROTECTED_NAMES: &[&str] = &[
    "system volume information",
    "$recycle.bin",
    "$winreagent",
    "$windows.~bt",
    "$windows.~ws",
    "$sysreset",
    "recovery",
    "config.msi",
    "windows.old",
    "boot",
    "efi",
    // Program and system folders also exist on other drives (D:\Program Files).
    "program files",
    "program files (x86)",
    "programdata",
    "windows",
    "windowsapps",
];

const PROTECTED_ROOT_FILES: &[&str] = &["pagefile.sys", "hiberfil.sys", "swapfile.sys", "bootmgr", "bootnxt"];

/// Why a path must never be modified by DiskNido, or `None`.
/// `allow_appdata` is only true for the curated cleanup rules.
pub fn protected_reason(path: &Path, allow_appdata: bool) -> Option<String> {
    let key = path_key(path);
    for (var, label) in [
        ("SystemRoot", "Carpeta de Windows"),
        ("ProgramFiles", "Aplicaciones instaladas"),
        ("ProgramFiles(x86)", "Aplicaciones instaladas"),
        ("ProgramW6432", "Aplicaciones instaladas"),
        ("ProgramData", "Datos compartidos de aplicaciones"),
    ] {
        if let Some(d) = env_dir(var)
            && is_within(path, &d)
        {
            return Some(label.into());
        }
    }
    if let Some(d) = APP_DATA_DIR.get()
        && is_within(path, d)
    {
        return Some("Datos de DiskNido".into());
    }
    if !allow_appdata
        && let Some(home) = env_dir("USERPROFILE")
        && is_within(path, &home.join("AppData"))
    {
        return Some("Datos de aplicaciones (AppData)".into());
    }
    let comps: Vec<String> = path
        .components()
        .filter_map(|c| match c {
            Component::Normal(s) => Some(s.to_string_lossy().to_lowercase()),
            _ => None,
        })
        .collect();
    if comps.is_empty() {
        return Some("Raíz de una unidad".into());
    }
    if comps.len() == 1 && PROTECTED_ROOT_FILES.contains(&comps[0].as_str()) {
        return Some("Archivo del sistema".into());
    }
    if let Some(first) = comps.first()
        && PROTECTED_NAMES.contains(&first.as_str())
    {
        return Some("Carpeta protegida del sistema".into());
    }
    if comps.iter().any(|c| c == "system volume information" || c == "$recycle.bin") {
        return Some("Carpeta protegida del sistema".into());
    }
    if key.ends_with(".dll") || key.ends_with(".sys") || key.ends_with(".drv") {
        return Some("Componente de sistema o controlador".into());
    }
    None
}

/// Windows, installed programs, program data, AppData, recovery and other
/// system locations. These never appear in duplicate, disk image, backup or
/// old-folder lists: they are not the user's files to review.
pub fn is_system_location(path: &Path) -> bool {
    protected_reason(path, false).is_some()
}

/// Extra exclusions for removing duplicate copies: content types where two
/// identical copies are often intentional or in active use.
pub fn duplicate_exclusion_reason(path: &Path) -> Option<String> {
    if let Some(r) = protected_reason(path, false) {
        return Some(r);
    }
    let ext = path.extension().map(|e| e.to_string_lossy().to_lowercase());
    match crate::classify::category(ext.as_deref()) {
        crate::classify::Category::VirtualMachines => Some("Disco de máquina virtual".into()),
        crate::classify::Category::Databases => Some("Base de datos".into()),
        crate::classify::Category::System => Some("Componente de sistema".into()),
        _ => None,
    }
}

/// Lexical checks: absolute local path, no `..`, not a bare drive root.
pub fn check_lexical(path: &Path) -> Result<()> {
    if !path.is_absolute() {
        return Err(Error::Refused(format!("Ruta no absoluta: {}", path.display())));
    }
    if path.components().any(|c| matches!(c, Component::ParentDir | Component::CurDir)) {
        return Err(Error::Refused(format!("Ruta no válida: {}", path.display())));
    }
    if path.to_string_lossy().starts_with(r"\\") && !path.to_string_lossy().starts_with(r"\\?\") {
        return Err(Error::Refused("Las ubicaciones de red no se modifican.".into()));
    }
    Ok(())
}

/// No component of the path (excluding the drive root) may be a junction or
/// symlink; otherwise the real target could be somewhere unexpected.
/// Cloud placeholder folders (OneDrive) are regular folders and allowed.
pub fn check_no_links(path: &Path) -> Result<()> {
    let mut cur = PathBuf::new();
    for c in path.components() {
        cur.push(c.as_os_str());
        if !matches!(c, Component::Normal(_)) {
            continue;
        }
        let md = match std::fs::symlink_metadata(&cur) {
            Ok(m) => m,
            Err(_) => return Ok(()), // the rest does not exist yet (e.g. a destination)
        };
        if md.file_type().is_symlink() || attrs_of(&md) & ATTR_REPARSE_POINT != 0 {
            let tag = fsx::path_attrs(&cur).map(|(_, t)| t).unwrap_or(0);
            if !fsx::is_cloud_reparse_tag(tag) {
                return Err(Error::Refused(format!(
                    "La ruta pasa por un enlace o unión: {}. Por seguridad no se modifica.",
                    cur.display()
                )));
            }
        }
    }
    Ok(())
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

/// Expected state recorded at analysis time.
#[derive(Debug, Clone, Copy, serde::Deserialize, serde::Serialize)]
pub struct Expected {
    pub size: u64,
    pub mtime: i64,
}

/// Full validation of an existing source before it is moved, archived or
/// recycled. Returns fresh metadata.
pub fn validate_source(path: &Path, expected: Option<Expected>, allow_appdata: bool) -> Result<std::fs::Metadata> {
    check_lexical(path)?;
    if let Some(r) = protected_reason(path, allow_appdata) {
        return Err(Error::Refused(format!("Ubicación protegida ({r}): {}", path.display())));
    }
    check_no_links(path)?;
    let md = std::fs::symlink_metadata(path).map_err(|_| Error::Refused(format!("Ya no existe: {}", path.display())))?;
    if fsx::is_cloud_only_attrs(attrs_of(&md)) {
        return Err(Error::Refused(format!("Solo está en la nube; no se descarga ni se modifica: {}", path.display())));
    }
    if let Some(e) = expected
        && md.is_file()
        && (md.len() != e.size || fsx::mtime_of(&md) != e.mtime)
    {
        return Err(Error::Refused(format!("El archivo cambió desde el análisis: {}", path.display())));
    }
    Ok(md)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn within_is_component_based() {
        assert!(is_within(Path::new(r"D:\Backups\2025"), Path::new(r"d:\backups")));
        assert!(!is_within(Path::new(r"D:\Backups2"), Path::new(r"D:\Backups")));
        assert!(is_within(Path::new(r"D:\x"), Path::new(r"D:\")));
        assert!(is_within(Path::new(r"D:\Backups\"), Path::new(r"D:\Backups")));
    }

    #[cfg(windows)]
    #[test]
    fn system_locations_are_protected() {
        let win = std::env::var("SystemRoot").unwrap();
        assert!(protected_reason(&Path::new(&win).join("System32").join("kernel32.dll"), true).is_some());
        assert!(protected_reason(Path::new(r"C:\System Volume Information\x"), false).is_some());
        assert!(protected_reason(Path::new(r"C:\pagefile.sys"), false).is_some());
        assert!(protected_reason(Path::new(r"C:\"), false).is_some());
        assert!(protected_reason(Path::new(r"D:\Fotos\a.jpg"), false).is_none());
        assert!(protected_reason(Path::new(r"D:\$RECYCLE.BIN\S-1\x"), false).is_some());
        let home = std::env::var("USERPROFILE").unwrap();
        let temp = Path::new(&home).join(r"AppData\Local\Temp\x.tmp");
        assert!(protected_reason(&temp, false).is_some());
        assert!(protected_reason(&temp, true).is_none());
    }

    #[cfg(windows)]
    #[test]
    fn system_locations_are_not_user_files() {
        let win = std::env::var("SystemRoot").unwrap();
        assert!(is_system_location(&Path::new(&win).join(r"Installer\b3533.msp")));
        assert!(is_system_location(&Path::new(&win).join(r"WinSxS\Temp\x")));
        let home = std::env::var("USERPROFILE").unwrap();
        assert!(is_system_location(&Path::new(&home).join(r"AppData\Local\Android\Sdk\system.img")));
        assert!(!is_system_location(&Path::new(&home).join(r"Downloads\LibreOffice.msi")));
        assert!(!is_system_location(Path::new(r"E:\Juegos\BLEACH.iso")));
        assert!(is_system_location(Path::new(r"D:\Program Files\TxGameAssistant\rootfs.bin")));
        assert!(is_system_location(Path::new(r"E:\ProgramData\x")));
    }

    #[test]
    fn lexical_rules() {
        assert!(check_lexical(Path::new("relative\\x")).is_err());
        #[cfg(windows)]
        {
            assert!(check_lexical(Path::new(r"C:\a\..\Windows")).is_err());
            assert!(check_lexical(Path::new(r"\\server\share\x")).is_err());
            assert!(check_lexical(Path::new(r"D:\ok\file.txt")).is_ok());
        }
    }

    #[test]
    fn duplicate_exclusions() {
        assert!(duplicate_exclusion_reason(Path::new(r"D:\vms\win.vhdx")).is_some());
        assert!(duplicate_exclusion_reason(Path::new(r"D:\mail\outlook.pst")).is_some());
        assert!(duplicate_exclusion_reason(Path::new(r"D:\fotos\a.jpg")).is_none());
    }

    #[test]
    fn changed_file_is_refused() {
        let d = tempfile::tempdir().unwrap();
        let f = d.path().join("a.txt");
        std::fs::write(&f, b"one").unwrap();
        let md = std::fs::metadata(&f).unwrap();
        let exp = Expected { size: md.len(), mtime: fsx::mtime_of(&md) };
        // Temp dirs live under AppData on Windows; allow it for the test.
        assert!(validate_source(&f, Some(exp), true).is_ok());
        std::fs::write(&f, b"three").unwrap();
        assert!(validate_source(&f, Some(exp), true).is_err());
    }
}
