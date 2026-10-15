//! Thin filesystem layer: raw directory listing with Windows attributes and
//! reparse tags, file identity (for hard links), free space, and the
//! "safe" rename that never replaces an existing destination.
//!
//! Everything platform-specific lives here so the rest of the engine is
//! plain Rust that can be unit-tested anywhere.

use std::io;
use std::path::Path;

pub const ATTR_READONLY: u32 = 0x1;
pub const ATTR_HIDDEN: u32 = 0x2;
pub const ATTR_SYSTEM: u32 = 0x4;
pub const ATTR_DIRECTORY: u32 = 0x10;
pub const ATTR_SPARSE: u32 = 0x200;
pub const ATTR_REPARSE_POINT: u32 = 0x400;
pub const ATTR_COMPRESSED: u32 = 0x800;
pub const ATTR_OFFLINE: u32 = 0x1000;
pub const ATTR_RECALL_ON_OPEN: u32 = 0x4_0000;
pub const ATTR_RECALL_ON_DATA_ACCESS: u32 = 0x40_0000;

/// Attributes that mean "the bytes are not on this disk" (OneDrive and other
/// cloud providers). Reading such a file would trigger a download, so the
/// engine never opens them.
pub const ATTR_CLOUD_MASK: u32 = ATTR_OFFLINE | ATTR_RECALL_ON_OPEN | ATTR_RECALL_ON_DATA_ACCESS;

/// Cloud-files reparse tags are `0x9000_x01A`. Directories carrying them are
/// regular folders (OneDrive placeholders) and are safe to descend into.
pub fn is_cloud_reparse_tag(tag: u32) -> bool {
    tag & 0xFFFF_0FFF == 0x9000_001A
}

#[derive(Debug, Clone)]
pub struct RawItem {
    pub name: String,
    pub is_dir: bool,
    pub size: u64,
    /// Unix seconds.
    pub mtime: i64,
    pub ctime: i64,
    pub attrs: u32,
    pub reparse_tag: u32,
}

impl RawItem {
    pub fn is_cloud_only(&self) -> bool {
        self.attrs & ATTR_CLOUD_MASK != 0
    }
    /// Symlinks, junctions and mount points: recorded, never followed.
    pub fn is_link(&self) -> bool {
        self.attrs & ATTR_REPARSE_POINT != 0 && !is_cloud_reparse_tag(self.reparse_tag)
    }
}

pub fn is_cloud_only_attrs(attrs: u32) -> bool {
    attrs & ATTR_CLOUD_MASK != 0
}

/// Stable identity of a physical file: (volume serial, file index). Two paths
/// with the same key are hard links to the same data.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FileKey {
    pub volume: u64,
    pub index: u64,
}

impl FileKey {
    pub fn as_string(&self) -> String {
        format!("{:x}:{:x}", self.volume, self.index)
    }
}

#[cfg(windows)]
mod imp {
    use super::*;
    use std::os::windows::ffi::OsStrExt;
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::Foundation::{ERROR_FILE_NOT_FOUND, ERROR_NO_MORE_FILES, FILETIME, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::Storage::FileSystem::{
        BY_HANDLE_FILE_INFORMATION, FIND_FIRST_EX_LARGE_FETCH, FindClose, FindExInfoBasic, FindExSearchNameMatch,
        FindFirstFileExW, FindNextFileW, GetDiskFreeSpaceExW, GetFileInformationByHandle, MoveFileExW, WIN32_FIND_DATAW,
    };

    /// `\\?\`-prefixed, NUL-terminated wide path so lengths beyond MAX_PATH work.
    pub fn verbatim_wide(path: &Path) -> Vec<u16> {
        let s = path.as_os_str();
        let raw: Vec<u16> = s.encode_wide().collect();
        let starts = |p: &str| {
            let p: Vec<u16> = p.encode_utf16().collect();
            raw.len() >= p.len() && raw[..p.len()] == p[..]
        };
        let mut out: Vec<u16> = if starts(r"\\?\") {
            raw.clone()
        } else if starts(r"\\") {
            let mut v: Vec<u16> = r"\\?\UNC\".encode_utf16().collect();
            v.extend_from_slice(&raw[2..]);
            v
        } else {
            let mut v: Vec<u16> = r"\\?\".encode_utf16().collect();
            v.extend_from_slice(&raw);
            v
        };
        // Verbatim paths do not normalise '/', so do it here.
        for c in out.iter_mut() {
            if *c == b'/' as u16 {
                *c = b'\\' as u16;
            }
        }
        out.push(0);
        out
    }

    fn filetime_to_unix(ft: FILETIME) -> i64 {
        let t = ((ft.dwHighDateTime as u64) << 32) | ft.dwLowDateTime as u64;
        if t == 0 {
            return 0;
        }
        (t / 10_000_000) as i64 - 11_644_473_600
    }

    pub fn list_dir(dir: &Path) -> io::Result<Vec<RawItem>> {
        let mut pattern = dir.to_path_buf();
        pattern.push("*");
        let wide = verbatim_wide(&pattern);
        let mut data: WIN32_FIND_DATAW = unsafe { std::mem::zeroed() };
        let handle = unsafe {
            FindFirstFileExW(
                wide.as_ptr(),
                FindExInfoBasic,
                &mut data as *mut _ as *mut _,
                FindExSearchNameMatch,
                std::ptr::null(),
                FIND_FIRST_EX_LARGE_FETCH,
            )
        };
        if handle == INVALID_HANDLE_VALUE {
            let err = io::Error::last_os_error();
            if err.raw_os_error() == Some(ERROR_FILE_NOT_FOUND as i32) {
                return Ok(Vec::new());
            }
            return Err(err);
        }
        let mut out = Vec::new();
        loop {
            let len = data.cFileName.iter().position(|&c| c == 0).unwrap_or(data.cFileName.len());
            let name_w = &data.cFileName[..len];
            let is_dot = name_w == [b'.' as u16] || name_w == [b'.' as u16, b'.' as u16];
            if !is_dot {
                let attrs = data.dwFileAttributes;
                out.push(RawItem {
                    name: String::from_utf16_lossy(name_w),
                    is_dir: attrs & ATTR_DIRECTORY != 0,
                    size: ((data.nFileSizeHigh as u64) << 32) | data.nFileSizeLow as u64,
                    mtime: filetime_to_unix(data.ftLastWriteTime),
                    ctime: filetime_to_unix(data.ftCreationTime),
                    attrs,
                    reparse_tag: if attrs & ATTR_REPARSE_POINT != 0 { data.dwReserved0 } else { 0 },
                });
            }
            if unsafe { FindNextFileW(handle, &mut data) } == 0 {
                let err = io::Error::last_os_error();
                unsafe { FindClose(handle) };
                if err.raw_os_error() == Some(ERROR_NO_MORE_FILES as i32) {
                    return Ok(out);
                }
                return Err(err);
            }
        }
    }

    pub fn file_key(file: &std::fs::File) -> io::Result<FileKey> {
        let mut info: BY_HANDLE_FILE_INFORMATION = unsafe { std::mem::zeroed() };
        if unsafe { GetFileInformationByHandle(file.as_raw_handle() as _, &mut info) } == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(FileKey {
            volume: info.dwVolumeSerialNumber as u64,
            index: ((info.nFileIndexHigh as u64) << 32) | info.nFileIndexLow as u64,
        })
    }

    pub fn free_space(path: &Path) -> io::Result<(u64, u64)> {
        let wide: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
        let (mut avail, mut total, mut free) = (0u64, 0u64, 0u64);
        if unsafe { GetDiskFreeSpaceExW(wide.as_ptr(), &mut avail, &mut total, &mut free) } == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok((avail, total))
    }

    /// Rename that fails if `to` exists (no MOVEFILE_REPLACE_EXISTING) and
    /// never copies across volumes (no MOVEFILE_COPY_ALLOWED).
    pub fn rename_no_replace(from: &Path, to: &Path) -> io::Result<()> {
        let f = verbatim_wide(from);
        let t = verbatim_wide(to);
        if unsafe { MoveFileExW(f.as_ptr(), t.as_ptr(), 0) } == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    }
}

#[cfg(not(windows))]
mod imp {
    use super::*;
    use std::os::unix::fs::MetadataExt;

    pub fn list_dir(dir: &Path) -> io::Result<Vec<RawItem>> {
        let mut out = Vec::new();
        for e in std::fs::read_dir(dir)? {
            let e = e?;
            let md = std::fs::symlink_metadata(e.path())?;
            let link = md.file_type().is_symlink();
            out.push(RawItem {
                name: e.file_name().to_string_lossy().into_owned(),
                is_dir: md.is_dir(),
                size: if md.is_dir() { 0 } else { md.len() },
                mtime: md.mtime(),
                ctime: md.ctime(),
                attrs: if link { ATTR_REPARSE_POINT } else { 0 },
                reparse_tag: if link { 0xA000_000C } else { 0 },
            });
        }
        Ok(out)
    }

    pub fn file_key(file: &std::fs::File) -> io::Result<FileKey> {
        let md = file.metadata()?;
        Ok(FileKey { volume: md.dev(), index: md.ino() })
    }

    pub fn free_space(_path: &Path) -> io::Result<(u64, u64)> {
        Err(io::Error::new(io::ErrorKind::Unsupported, "free space query is Windows-only"))
    }

    pub fn rename_no_replace(from: &Path, to: &Path) -> io::Result<()> {
        if to.exists() {
            return Err(io::Error::new(io::ErrorKind::AlreadyExists, "destination exists"));
        }
        std::fs::rename(from, to)
    }
}

pub use imp::*;

/// Attributes and reparse tag of a single path without following links.
pub fn path_attrs(path: &Path) -> io::Result<(u32, u32)> {
    let parent = path.parent().ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "no parent"))?;
    let name = path.file_name().ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "no file name"))?;
    let name = name.to_string_lossy();
    list_dir(parent)?
        .into_iter()
        .find(|i| i.name.eq_ignore_ascii_case(&name))
        .map(|i| (i.attrs, i.reparse_tag))
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "not found"))
}

#[derive(Debug, Clone)]
pub struct TreeEntry {
    /// Relative to the walked root.
    pub rel: std::path::PathBuf,
    pub item: RawItem,
}

/// Fresh sequential listing of a folder tree (links recorded, not followed).
/// Used to verify moves and to build archives; not for whole drives.
pub fn collect_tree(root: &Path, control: &crate::Control) -> crate::Result<Vec<TreeEntry>> {
    let mut out = Vec::new();
    let mut stack = vec![std::path::PathBuf::new()];
    while let Some(rel) = stack.pop() {
        control.checkpoint()?;
        for item in list_dir(&root.join(&rel))? {
            let r = rel.join(&item.name);
            if item.is_dir && !item.is_link() {
                stack.push(r.clone());
            }
            out.push(TreeEntry { rel: r, item });
        }
    }
    Ok(out)
}

/// (files, bytes) of a tree listing.
pub fn tree_totals(entries: &[TreeEntry]) -> (u64, u64) {
    entries.iter().filter(|e| !e.item.is_dir).fold((0, 0), |(n, b), e| (n + 1, b + e.item.size))
}

/// Unix-seconds modification time, matching what the scanner stores.
pub fn mtime_of(md: &std::fs::Metadata) -> i64 {
    md.modified().ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_secs() as i64).unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cloud_tags() {
        assert!(is_cloud_reparse_tag(0x9000_001A));
        assert!(is_cloud_reparse_tag(0x9000_F01A));
        assert!(is_cloud_reparse_tag(0x9000_301A));
        assert!(!is_cloud_reparse_tag(0xA000_0003)); // mount point / junction
        assert!(!is_cloud_reparse_tag(0xA000_000C)); // symlink
    }

    #[test]
    fn lists_unicode_names() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("año ñandú 日本.txt"), b"abc").unwrap();
        std::fs::create_dir(dir.path().join("sub")).unwrap();
        let mut items = list_dir(dir.path()).unwrap();
        items.sort_by(|a, b| a.name.cmp(&b.name));
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].name, "año ñandú 日本.txt");
        assert_eq!(items[0].size, 3);
        assert!(items[1].is_dir);
    }

    #[test]
    fn long_paths_work() {
        let dir = tempfile::tempdir().unwrap();
        let mut p = dir.path().to_path_buf();
        for i in 0..12 {
            p.push(format!("carpeta-con-nombre-bastante-largo-{i:02}"));
        }
        assert!(p.as_os_str().len() > 300);
        std::fs::create_dir_all(&p).unwrap();
        std::fs::write(p.join("f.bin"), b"x").unwrap();
        let items = list_dir(&p).unwrap();
        assert_eq!(items.len(), 1);
    }

    #[test]
    fn rename_never_replaces() {
        let dir = tempfile::tempdir().unwrap();
        let a = dir.path().join("a");
        let b = dir.path().join("b");
        std::fs::create_dir(&a).unwrap();
        std::fs::create_dir(&b).unwrap();
        std::fs::write(b.join("keep.txt"), b"keep").unwrap();
        assert!(rename_no_replace(&a, &b).is_err());
        assert_eq!(std::fs::read(b.join("keep.txt")).unwrap(), b"keep");
        let c = dir.path().join("c");
        rename_no_replace(&a, &c).unwrap();
        assert!(c.is_dir() && !a.exists());
    }
}
