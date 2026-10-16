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

    // The archive and its contents are in the catalog, searchable by name.
    let hits = disknido_core::catalog::search(&conn, "informe ano", 10).unwrap();
    assert_eq!(hits.len(), 1, "accent-insensitive search finds the file inside the ZIP");
    assert_eq!(Path::new(&hits[0].archive_path), zip_path);
    assert_eq!(hits[0].rel_path, "documentos/informe año 2025 – ñ.txt");
    assert!(hits[0].blake3.is_some());
    let a = disknido_core::catalog::get(&conn, r.catalog_id).unwrap().unwrap();
    assert!(a.verified && a.exists);
    assert_eq!(a.files, r.files);
    // Importing an existing ZIP reads its file list.
    let imported = disknido_core::catalog::import_zip(&conn, &zip_path).unwrap();
    assert_eq!(imported.files, r.files);
    assert_eq!(disknido_core::catalog::list(&conn).unwrap().len(), 1, "same path replaces the record");

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

fn batch(items: Vec<CompressRequest>, recycle_originals: bool) -> compress::BatchRequest {
    compress::BatchRequest { items, recycle_originals }
}

#[test]
fn batch_recycles_only_originals_that_match_their_verified_zip() {
    let tmp = fixture_root();
    let a = tmp.path().join("Backup A 2021");
    let b = tmp.path().join("Backup B 2022");
    write(&a.join("docs/a.txt"), "aaa ".repeat(10_000).as_bytes());
    write(&b.join("b.txt"), "bbb ".repeat(10_000).as_bytes());
    let out = tmp.path().join("out");
    std::fs::create_dir_all(&out).unwrap();
    let conn = db::open(&db(tmp.path())).unwrap();
    let phases = std::sync::Mutex::new(Vec::new());
    let r = compress::compress_batch(&conn, &batch(vec![req(&a, &out), req(&b, &out)], true), &Control::default(), &|p| {
        phases.lock().unwrap().push((p.index, p.phase.clone()));
        // A file appears in B after its ZIP verified: B must be kept.
        if p.index == 1 && p.phase == "checking" {
            write(&b.join("nuevo.txt"), b"added after the zip");
        }
    })
    .unwrap();
    assert_eq!((r.verified, r.failed, r.recycled), (2, 0, 1));
    assert!(r.items[0].recycled && !a.exists(), "A matched its ZIP and went to the Recycle Bin");
    assert!(!r.items[1].recycled && b.join("b.txt").exists() && b.join("nuevo.txt").exists());
    assert!(r.items[1].kept_reason.as_deref().unwrap().contains("se conserva"), "{:?}", r.items[1].kept_reason);
    assert!(out.join("Backup A 2021.zip").exists() && out.join("Backup B 2022.zip").exists());
    assert_eq!(r.bytes_in_bin, r.items[0].result.as_ref().unwrap().source_bytes);
    let p = phases.into_inner().unwrap();
    assert!(p.contains(&(0, "verifying".into())) && p.contains(&(0, "recycling".into())));

    // The ZIP of the recycled folder holds every byte of it.
    let mut z = zip::ZipArchive::new(std::fs::File::open(out.join("Backup A 2021.zip")).unwrap()).unwrap();
    let mut buf = String::new();
    z.by_name("docs/a.txt").unwrap().read_to_string(&mut buf).unwrap();
    assert_eq!(buf, "aaa ".repeat(10_000));
}

#[test]
fn batch_without_removal_keeps_originals_and_refuses_nested_or_repeated() {
    let tmp = fixture_root();
    let p = tmp.path().join("Padre");
    write(&p.join("hijo/x.txt"), b"x");
    write(&p.join("y.txt"), b"y");
    let out = tmp.path().join("out");
    std::fs::create_dir_all(&out).unwrap();
    let conn = db::open(&db(tmp.path())).unwrap();
    let items = vec![req(&p, &out), req(&p.join("hijo"), &out), req(&p, &out)];
    let r = compress::compress_batch(&conn, &batch(items, false), &Control::default(), &|_| {}).unwrap();
    assert_eq!((r.verified, r.failed, r.recycled), (1, 2, 0));
    assert!(r.items[1].error.is_some() && r.items[2].error.is_some());
    assert!(p.join("hijo/x.txt").exists() && p.join("y.txt").exists(), "nothing is removed without the option");
}
