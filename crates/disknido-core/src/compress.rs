//! Verified ZIP archives (Deflate, ZIP64 when needed, UTF-8 names).
//!
//! The archive is written to `<name>.zip.partial`, every source file is
//! hashed with BLAKE3 while it is streamed in, the archive is then reopened
//! and every entry is decompressed and hashed again. Only when every hash and
//! size matches — and the sources did not change meanwhile — is the file
//! renamed to `<name>.zip`. Originals are never deleted here.

use crate::classify;
use crate::fsx::{self, TreeEntry};
use crate::{Control, Error, Result, ops, safety};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::{BufWriter, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

const MARGIN: u64 = 64 * 1024 * 1024;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompressRequest {
    pub source: PathBuf,
    /// Folder that will receive the archive.
    pub dest_dir: PathBuf,
    /// File name without `.zip`; defaults to the source folder name.
    pub name: Option<String>,
    /// 1 (fast) – 9 (smallest). Default 6.
    pub level: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Preview {
    pub source: String,
    pub dest: String,
    pub files: u64,
    pub folders: u64,
    pub bytes: u64,
    /// Share of bytes in formats that are already compressed (0–1).
    pub already_compressed_share: f64,
    pub dest_free: Option<u64>,
    pub enough_space: bool,
    pub cloud_only: u64,
    pub links: u64,
    pub dest_exists: bool,
    pub problems: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct CompressProgress {
    /// reading | verifying | done
    pub phase: String,
    pub bytes_done: u64,
    pub bytes_total: u64,
    pub current: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CompressResult {
    pub op_id: i64,
    pub archive: String,
    pub files: u64,
    pub source_bytes: u64,
    pub archive_bytes: u64,
    /// Space that would be freed only if the original folder were removed.
    pub potential_saving: i64,
    pub verified: bool,
    pub not_preserved: Vec<String>,
}

fn file_name_ok(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 200
        && !name.chars().any(|c| matches!(c, '\\' | '/' | ':' | '*' | '?' | '"' | '<' | '>' | '|') || c.is_control())
        && !name.ends_with('.')
        && !name.ends_with(' ')
}

fn dest_path(req: &CompressRequest) -> Result<PathBuf> {
    let name = match &req.name {
        Some(n) => n.trim().trim_end_matches(".zip").to_string(),
        None => req.source.file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default(),
    };
    if !file_name_ok(&name) {
        return Err(Error::Refused(format!("Nombre de archivo no válido: «{name}»")));
    }
    Ok(req.dest_dir.join(format!("{name}.zip")))
}

fn check_paths(req: &CompressRequest) -> Result<PathBuf> {
    safety::validate_source(&req.source, None, false)?;
    if !req.source.is_dir() {
        return Err(Error::Refused("Solo se pueden comprimir carpetas".into()));
    }
    safety::check_lexical(&req.dest_dir)?;
    if safety::is_within(&req.dest_dir, &req.source) {
        return Err(Error::Refused("El archivo comprimido no puede guardarse dentro de la carpeta que se comprime".into()));
    }
    if let Some(r) = safety::protected_reason(&req.dest_dir.join("x.zip"), false) {
        return Err(Error::Refused(format!("Destino protegido ({r})")));
    }
    safety::check_no_links(&req.dest_dir)?;
    if !req.dest_dir.is_dir() {
        return Err(Error::Refused(format!("La carpeta de destino no existe: {}", req.dest_dir.display())));
    }
    dest_path(req)
}

pub fn preview(req: &CompressRequest, control: &Control) -> Result<Preview> {
    let dest = check_paths(req)?;
    let tree = fsx::collect_tree(&req.source, control)?;
    let (files, bytes) = fsx::tree_totals(&tree);
    let compressed: u64 = tree
        .iter()
        .filter(|e| !e.item.is_dir && classify::already_compressed(crate::scan::ext_of(&e.item.name).as_deref()))
        .map(|e| e.item.size)
        .sum();
    let cloud_only = tree.iter().filter(|e| e.item.is_cloud_only()).count() as u64;
    let links = tree.iter().filter(|e| e.item.is_link()).count() as u64;
    let dest_free = fsx::free_space(&req.dest_dir).ok().map(|f| f.0);
    let enough_space = dest_free.is_none_or(|f| f >= bytes + MARGIN);
    let mut problems = Vec::new();
    if cloud_only > 0 {
        problems.push(format!("{cloud_only} archivos están solo en la nube. Descárgalos primero; DiskNido no los descarga."));
    }
    if !enough_space {
        problems.push(
            "Puede no haber espacio suficiente en el destino (en el peor caso el ZIP ocupa lo mismo que el origen).".into(),
        );
    }
    if dest.exists() {
        problems.push("Ya existe un archivo con ese nombre; elige otro nombre.".into());
    }
    Ok(Preview {
        source: req.source.to_string_lossy().into_owned(),
        dest: dest.to_string_lossy().into_owned(),
        files,
        folders: tree.iter().filter(|e| e.item.is_dir).count() as u64,
        bytes,
        already_compressed_share: if bytes == 0 { 0.0 } else { compressed as f64 / bytes as f64 },
        dest_free,
        enough_space,
        cloud_only,
        links,
        dest_exists: dest.exists(),
        problems,
    })
}

struct Manifest {
    rel: String,
    size: u64,
    hash: blake3::Hash,
    mtime: i64,
}

fn zip_name(rel: &Path) -> String {
    rel.to_string_lossy().replace('\\', "/")
}

fn dos_time(ts: i64) -> zip::DateTime {
    chrono::DateTime::from_timestamp(ts, 0)
        .map(|d| d.with_timezone(&chrono::Local).naive_local())
        .and_then(|n| zip::DateTime::try_from(n).ok())
        .unwrap_or_default()
}

pub fn compress(
    conn: &Connection,
    req: &CompressRequest,
    control: &Control,
    on_progress: &dyn Fn(&CompressProgress),
) -> Result<CompressResult> {
    let dest = check_paths(req)?;
    if dest.exists() {
        return Err(Error::Refused(format!("Ya existe {}; no se sobrescribe", dest.display())));
    }
    let tree = fsx::collect_tree(&req.source, control)?;
    if tree.iter().any(|e| e.item.is_cloud_only()) {
        return Err(Error::Refused("La carpeta contiene archivos que solo están en la nube".into()));
    }
    let (files, bytes) = fsx::tree_totals(&tree);
    if let Ok((free, _)) = fsx::free_space(&req.dest_dir)
        && free < bytes + MARGIN
    {
        return Err(Error::Refused("Espacio insuficiente en el destino".into()));
    }
    let partial = PathBuf::from(format!("{}.partial", dest.to_string_lossy()));
    let summary = serde_json::json!({ "source": req.source, "archive": dest, "files": files, "bytes": bytes });
    let op = ops::begin(conn, "compress", &summary, None, false)?;
    ops::add_item(conn, op, 0, &req.source.to_string_lossy(), Some(&dest.to_string_lossy()))?;
    ops::item_status(conn, op, 0, "running", None)?;

    let result = (|| -> Result<CompressResult> {
        let manifest =
            write_archive(&req.source, &tree, &partial, req.level.unwrap_or(6).clamp(1, 9), bytes, control, on_progress)?;
        // Sources must not have changed while we read them.
        for m in &manifest {
            let md = std::fs::metadata(req.source.join(&m.rel))?;
            if md.len() != m.size || fsx::mtime_of(&md) != m.mtime {
                return Err(Error::Other(format!("«{}» cambió durante la compresión; el ZIP no se conserva", m.rel)));
            }
        }
        verify_archive(
            &partial,
            &manifest,
            tree.iter().filter(|e| e.item.is_dir && !e.item.is_link()).count(),
            control,
            on_progress,
        )?;
        fsx::rename_no_replace(&partial, &dest)?;
        let archive_bytes = std::fs::metadata(&dest)?.len();
        let mut not_preserved = vec![
            "Permisos NTFS (ACL), atributos (oculto, sistema) y flujos alternativos de datos".to_string(),
            "Fecha de creación (se guarda la de modificación, con precisión de 2 segundos)".to_string(),
        ];
        let links = tree.iter().filter(|e| e.item.is_link()).count();
        if links > 0 {
            not_preserved.push(format!("{links} enlaces simbólicos o uniones (no se siguen ni se incluyen)"));
        }
        Ok(CompressResult {
            op_id: op,
            archive: dest.to_string_lossy().into_owned(),
            files,
            source_bytes: bytes,
            archive_bytes,
            potential_saving: bytes as i64 - archive_bytes as i64,
            verified: true,
            not_preserved,
        })
    })();
    match &result {
        Ok(r) => {
            ops::item_status(conn, op, 0, "done", None)?;
            ops::finish(conn, op, "done", r.archive_bytes, None, None)?;
        }
        Err(e) => {
            let _ = std::fs::remove_file(&partial); // our own incomplete file
            let status = if matches!(e, Error::Cancelled) { "cancelled" } else { "failed" };
            ops::item_status(conn, op, 0, "failed", Some(&e.to_string()))?;
            ops::finish(conn, op, status, 0, None, Some(&e.to_string()))?;
        }
    }
    result
}

struct HashingReader<R> {
    inner: R,
    hasher: blake3::Hasher,
    n: u64,
}

impl<R: Read> Read for HashingReader<R> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let n = self.inner.read(buf)?;
        self.hasher.update(&buf[..n]);
        self.n += n as u64;
        Ok(n)
    }
}

fn write_archive(
    src: &Path,
    tree: &[TreeEntry],
    partial: &Path,
    level: i64,
    total: u64,
    control: &Control,
    on_progress: &dyn Fn(&CompressProgress),
) -> Result<Vec<Manifest>> {
    let out = std::fs::OpenOptions::new().write(true).create_new(true).open(partial)?;
    let mut zip = zip::ZipWriter::new(BufWriter::with_capacity(1 << 20, out));
    let done = AtomicU64::new(0);
    let mut manifest = Vec::new();
    let mut entries: Vec<&TreeEntry> = tree.iter().filter(|e| !e.item.is_link()).collect();
    entries.sort_by(|a, b| a.rel.cmp(&b.rel));
    for e in entries {
        control.checkpoint()?;
        let name = zip_name(&e.rel);
        let opts = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated)
            .compression_level(Some(level))
            .last_modified_time(dos_time(e.item.mtime))
            .large_file(e.item.size >= u32::MAX as u64 / 2);
        if e.item.is_dir {
            zip.add_directory(&name, opts)?;
            continue;
        }
        let opts = if classify::already_compressed(crate::scan::ext_of(&e.item.name).as_deref()) {
            opts.compression_method(zip::CompressionMethod::Stored).compression_level(None)
        } else {
            opts
        };
        zip.start_file(&name, opts)?;
        let f = File::open(src.join(&e.rel))?;
        let mut r = HashingReader { inner: f, hasher: blake3::Hasher::new(), n: 0 };
        let mut buf = vec![0u8; 1 << 20];
        loop {
            control.checkpoint()?;
            let n = r.read(&mut buf)?;
            if n == 0 {
                break;
            }
            zip.write_all(&buf[..n])?;
            let d = done.fetch_add(n as u64, Ordering::Relaxed) + n as u64;
            on_progress(&CompressProgress { phase: "reading".into(), bytes_done: d, bytes_total: total, current: name.clone() });
        }
        manifest.push(Manifest {
            rel: e.rel.to_string_lossy().into_owned(),
            size: r.n,
            hash: r.hasher.finalize(),
            mtime: e.item.mtime,
        });
    }
    let mut w = zip.finish()?;
    w.flush()?;
    w.into_inner().map_err(|e| Error::Io(e.into_error()))?.sync_all()?;
    Ok(manifest)
}

fn verify_archive(
    archive: &Path,
    manifest: &[Manifest],
    dirs: usize,
    control: &Control,
    on_progress: &dyn Fn(&CompressProgress),
) -> Result<()> {
    let mut zip = zip::ZipArchive::new(File::open(archive)?)?;
    if zip.len() != manifest.len() + dirs {
        return Err(Error::Other(format!("El ZIP tiene {} entradas y se esperaban {}", zip.len(), manifest.len() + dirs)));
    }
    let total: u64 = manifest.iter().map(|m| m.size).sum();
    let mut done = 0u64;
    let mut buf = vec![0u8; 1 << 20];
    for m in manifest {
        control.checkpoint()?;
        let name = zip_name(Path::new(&m.rel));
        let mut entry = zip.by_name(&name).map_err(|_| Error::Other(format!("Falta «{name}» en el ZIP")))?;
        if entry.size() != m.size {
            return Err(Error::Other(format!("Tamaño incorrecto para «{name}»")));
        }
        let mut h = blake3::Hasher::new();
        loop {
            let n = entry.read(&mut buf)?;
            if n == 0 {
                break;
            }
            h.update(&buf[..n]);
            done += n as u64;
        }
        if h.finalize() != m.hash {
            return Err(Error::Other(format!("El contenido de «{name}» no coincide con el original")));
        }
        on_progress(&CompressProgress { phase: "verifying".into(), bytes_done: done, bytes_total: total, current: name });
    }
    Ok(())
}

/// Re-checks an existing archive: opens it and reads every entry (CRC check).
pub fn test_archive(path: &Path) -> Result<u64> {
    let mut zip = zip::ZipArchive::new(File::open(path)?)?;
    let mut n = 0;
    for i in 0..zip.len() {
        let mut e = zip.by_index(i)?;
        std::io::copy(&mut e, &mut std::io::sink())?;
        n += 1;
    }
    Ok(n)
}
