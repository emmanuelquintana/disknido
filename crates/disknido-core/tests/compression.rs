mod common;
use common::*;
use disknido_core::compress::{self, CompressRequest};
use disknido_core::{Control, Error, db};
use std::io::Read;
use std::path::Path;

fn req(src: &Path, dest: &Path) -> CompressRequest {
    CompressRequest { source: src.to_path_buf(), dest_dir: dest.to_path_buf(), name: None, level: Some(6) }
}

#[test]
fn archive_is_verified_and_originals_kept() {
    let tmp = fixture_root();
    let src = tmp.path().join("Backup Enero 2025");
    write(&src.join("documentos/informe año 2025 – ñ.txt"), "contenido único ✓".repeat(500).as_bytes());
    write(&src.join("fotos/foto.jpg"), &bytes(1, 200_000));
    write(&src.join("grande.bin"), &vec![7u8; 30 * 1024 * 1024]);
    let mut deep = src.join("profundo");
    for i in 0..20 {
        deep.push(format!("nivel{i}"));
    }
    write(&deep.join("hoja.txt"), b"deep");
    std::fs::create_dir_all(src.join("vacia")).unwrap();
    let out = tmp.path().join("out");
    std::fs::create_dir_all(&out).unwrap();
    let conn = db::open(&db(tmp.path())).unwrap();

    let pv = compress::preview(&req(&src, &out), &Control::default()).unwrap();
    assert_eq!(pv.files, 4);
    assert!(pv.problems.is_empty(), "{:?}", pv.problems);
    let r = compress::compress(&conn, &req(&src, &out), &Control::default(), &|_| {}).unwrap();
    assert!(r.verified);
    let zip_path = out.join("Backup Enero 2025.zip");
    assert_eq!(Path::new(&r.archive), zip_path);
    assert!(!out.join("Backup Enero 2025.zip.partial").exists());
    assert!(r.archive_bytes < r.source_bytes, "30 MB of a repeated byte compresses");
    assert_eq!(r.potential_saving, r.source_bytes as i64 - r.archive_bytes as i64);
    assert!(src.join("grande.bin").exists(), "originals are never removed");

    // Independent check: unzip and compare every file with the original.
    let mut z = zip::ZipArchive::new(std::fs::File::open(&zip_path).unwrap()).unwrap();
    for name in ["documentos/informe año 2025 – ñ.txt", "fotos/foto.jpg", "grande.bin"] {
        let mut e = z.by_name(name).unwrap();
        let mut buf = Vec::new();
        e.read_to_end(&mut buf).unwrap();
        assert_eq!(buf, std::fs::read(src.join(name)).unwrap(), "{name}");
    }
    assert!(z.by_name("vacia/").is_ok(), "empty folders are kept");
    assert!(compress::test_archive(&zip_path).unwrap() > 0);

    // Existing archive is never overwritten.
    let e = compress::compress(&conn, &req(&src, &out), &Control::default(), &|_| {}).unwrap_err();
    assert!(matches!(e, Error::Refused(_)));
}

#[test]
fn empty_folder_and_bad_destinations() {
    let tmp = fixture_root();
    let src = tmp.path().join("vacía");
    std::fs::create_dir_all(&src).unwrap();
    let conn = db::open(&db(tmp.path())).unwrap();
    let r = compress::compress(&conn, &req(&src, tmp.path()), &Control::default(), &|_| {}).unwrap();
    assert_eq!(r.files, 0);
    // destination inside the source
    write(&tmp.path().join("b/x.txt"), b"x");
    let inner = tmp.path().join("b/inner");
    std::fs::create_dir_all(&inner).unwrap();
    assert!(compress::compress(&conn, &req(&tmp.path().join("b"), &inner), &Control::default(), &|_| {}).is_err());
    // invalid name
    let mut bad = req(&tmp.path().join("b"), tmp.path());
    bad.name = Some("..\\evil".into());
    assert!(compress::compress(&conn, &bad, &Control::default(), &|_| {}).is_err());
}

#[test]
fn change_during_compression_fails_cleanly() {
    let tmp = fixture_root();
    let src = tmp.path().join("src");
    write(&src.join("a.txt"), &bytes(2, 3 * 1024 * 1024));
    write(&src.join("b.txt"), &bytes(3, 3 * 1024 * 1024));
    let out = tmp.path().join("out");
    std::fs::create_dir_all(&out).unwrap();
    let conn = db::open(&db(tmp.path())).unwrap();
    let touched = std::sync::atomic::AtomicBool::new(false);
    let a = src.join("a.txt");
    let e = compress::compress(&conn, &req(&src, &out), &Control::default(), &|_| {
        if !touched.swap(true, std::sync::atomic::Ordering::SeqCst) {
            std::fs::write(&a, b"modified while archiving").unwrap();
        }
    })
    .unwrap_err();
    assert!(e.to_string().contains("cambió"), "{e}");
    assert!(!out.join("src.zip").exists() && !out.join("src.zip.partial").exists());
    assert!(src.join("b.txt").exists());
    let h = disknido_core::ops::history(&conn, 1).unwrap();
    assert_eq!(h[0].status, "failed");
}

#[test]
fn cancellation_removes_partial_file() {
    let tmp = fixture_root();
    let src = tmp.path().join("src");
    write(&src.join("a.bin"), &bytes(4, 8 * 1024 * 1024));
    let out = tmp.path().join("out");
    std::fs::create_dir_all(&out).unwrap();
    let conn = db::open(&db(tmp.path())).unwrap();
    let ctl = Control::default();
    let e = compress::compress(&conn, &req(&src, &out), &ctl, &|_| ctl.cancel()).unwrap_err();
    assert!(matches!(e, Error::Cancelled));
    assert!(std::fs::read_dir(&out).unwrap().next().is_none(), "no partial archive left");
}

#[test]
fn corrupted_archive_is_detected() {
    let tmp = fixture_root();
    let src = tmp.path().join("src");
    write(&src.join("a.txt"), "texto ".repeat(100_000).as_bytes());
    let conn = db::open(&db(tmp.path())).unwrap();
    let r = compress::compress(&conn, &req(&src, tmp.path()), &Control::default(), &|_| {}).unwrap();
    let mut data = std::fs::read(&r.archive).unwrap();
    let mid = data.len() / 2;
    for b in &mut data[mid..mid + 64] {
        *b ^= 0x5A;
    }
    std::fs::write(&r.archive, data).unwrap();
    assert!(compress::test_archive(Path::new(&r.archive)).is_err());
}
