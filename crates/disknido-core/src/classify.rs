//! File categories by extension. Used for distribution charts and hints
//! only — never as evidence that a file can be deleted.

use serde::Serialize;

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "camelCase")]
pub enum Category {
    Images,
    Video,
    Audio,
    Documents,
    Archives,
    DiskImages,
    Installers,
    Code,
    VirtualMachines,
    Databases,
    System,
    Other,
}

pub const DISK_IMAGE_EXTS: &[&str] = &[
    "iso", "bin", "cue", "img", "nrg", "mdf", "mds", "chd", "cso", "zso", "rvz", "wbfs", "wud", "wux", "gdi", "cdi", "ccd", "vcd",
];

pub fn category(ext: Option<&str>) -> Category {
    let Some(e) = ext else { return Category::Other };
    match e {
        "jpg" | "jpeg" | "png" | "gif" | "bmp" | "webp" | "heic" | "heif" | "tif" | "tiff" | "raw" | "cr2" | "nef" | "arw"
        | "dng" | "psd" | "svg" | "ico" | "avif" => Category::Images,
        "mp4" | "mkv" | "avi" | "mov" | "wmv" | "m4v" | "webm" | "flv" | "mpg" | "mpeg" | "ts" | "m2ts" | "3gp" => {
            Category::Video
        }
        "mp3" | "flac" | "wav" | "aac" | "ogg" | "m4a" | "wma" | "opus" | "aiff" => Category::Audio,
        "pdf" | "doc" | "docx" | "xls" | "xlsx" | "ppt" | "pptx" | "odt" | "ods" | "odp" | "txt" | "rtf" | "md" | "csv"
        | "epub" => Category::Documents,
        "zip" | "rar" | "7z" | "gz" | "tgz" | "bz2" | "xz" | "zst" | "tar" | "cab" => Category::Archives,
        "exe" | "msi" | "msix" | "appx" | "msixbundle" => Category::Installers,
        "rs" | "js" | "tsx" | "jsx" | "py" | "java" | "c" | "cpp" | "h" | "cs" | "go" | "rb" | "php" | "html" | "css"
        | "json" | "xml" | "yml" | "yaml" | "toml" => Category::Code,
        "vhd" | "vhdx" | "vmdk" | "vdi" | "qcow2" | "avhdx" | "vmsn" | "vmem" => Category::VirtualMachines,
        "db" | "sqlite" | "sqlite3" | "mdb" | "accdb" | "ldf" | "pst" | "ost" | "edb" | "ndf" => Category::Databases,
        "dll" | "sys" | "drv" | "ocx" | "cpl" | "mui" | "cat" | "inf" | "pdb" => Category::System,
        e if DISK_IMAGE_EXTS.contains(&e) => Category::DiskImages,
        _ => Category::Other,
    }
}

/// Formats whose content is already compressed: zipping them saves little.
pub fn already_compressed(ext: Option<&str>) -> bool {
    matches!(
        ext,
        Some(
            "jpg"
                | "jpeg"
                | "png"
                | "gif"
                | "webp"
                | "heic"
                | "avif"
                | "mp4"
                | "mkv"
                | "mov"
                | "webm"
                | "m4v"
                | "mp3"
                | "aac"
                | "ogg"
                | "m4a"
                | "opus"
                | "flac"
                | "zip"
                | "rar"
                | "7z"
                | "gz"
                | "tgz"
                | "bz2"
                | "xz"
                | "zst"
                | "cab"
                | "docx"
                | "xlsx"
                | "pptx"
                | "chd"
                | "cso"
                | "zso"
                | "rvz"
                | "wux"
                | "msi"
                | "msix"
                | "appx"
                | "jar"
                | "apk"
        )
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn categories() {
        assert_eq!(category(Some("iso")), Category::DiskImages);
        assert_eq!(category(Some("bin")), Category::DiskImages);
        assert_eq!(category(Some("JPG".to_lowercase().as_str())), Category::Images);
        assert_eq!(category(Some("vhdx")), Category::VirtualMachines);
        assert_eq!(category(None), Category::Other);
        assert!(already_compressed(Some("mkv")));
        assert!(!already_compressed(Some("txt")));
    }
}
