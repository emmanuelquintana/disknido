mod common;
use common::*;
use disknido_core::{Control, db, query, scan};

#[test]
fn scan_counts_aggregates_and_lists() {
    let tmp = fixture_root();
    let data = tmp.path().join("data");
    write(&data.join("a/1.bin"), &bytes(1, 100));
    write(&data.join("a/b/2.bin"), &bytes(2, 200));
    write(&data.join("c.txt"), b"hello");
    write(&data.join("ñandú/日本語 – año.txt"), b"unicode");
    let s = scan(&db(tmp.path()), &data);
    assert_eq!(s.status, "done");
    assert_eq!(s.files, 4);
    assert_eq!(s.bytes, 312);

    let conn = db::open(&db(tmp.path())).unwrap();
    let roots = query::roots(&conn).unwrap();
    assert_eq!(roots.len(), 1);
    assert_eq!(roots[0].size, 312, "root total is the sum of everything below");
    let (kids, total) = query::children(&conn, roots[0].id, 0, 100).unwrap();
    assert_eq!(total, 3);
    assert_eq!(kids[0].name, "a");
    assert_eq!(kids[0].size, 300);
    assert_eq!(kids[0].file_count, 2);
    assert!(kids[0].path.ends_with("data\\a") || kids[0].path.ends_with("data/a"));
    let found = query::search(&conn, "año", 10).unwrap();
    assert_eq!(found.len(), 1);
    assert!(found[0].path.contains("ñandú"));
}

#[cfg(windows)]
#[test]
fn junctions_are_recorded_not_followed() {
    let tmp = fixture_root();
    let data = tmp.path().join("data");
    write(&data.join("real/1.bin"), &bytes(1, 1000));
    let link = data.join("loop");
    let st = std::process::Command::new("cmd")
        .args(["/C", "mklink", "/J"])
        .arg(&link)
        .arg(&data) // points at its own parent: a cycle if followed
        .output()
        .unwrap();
    assert!(st.status.success(), "mklink /J failed: {}", String::from_utf8_lossy(&st.stderr));
    let s = scan(&db(tmp.path()), &data);
    assert_eq!(s.files, 1, "files behind the junction are not counted again");
    let conn = db::open(&db(tmp.path())).unwrap();
    let roots = query::roots(&conn).unwrap();
    let (kids, _) = query::children(&conn, roots[0].id, 0, 10).unwrap();
    let j = kids.iter().find(|k| k.name == "loop").unwrap();
    assert!(j.is_link);
    assert_eq!(j.size, 0);
}

#[test]
fn exclusions_and_cancellation() {
    let tmp = fixture_root();
    let data = tmp.path().join("data");
    write(&data.join("keep/1.bin"), &bytes(1, 10));
    write(&data.join("skip/2.bin"), &bytes(2, 10));
    let req = scan::ScanRequest {
        roots: vec![data.clone()],
        mode: "custom".into(),
        profile: Default::default(),
        exclusions: vec![data.join("SKIP").to_string_lossy().into_owned()],
        include_hidden: true,
    };
    let s = scan::run_scan(&db(tmp.path()), &req, &Control::default(), &|_| {}).unwrap();
    assert_eq!(s.files, 1, "excluded folder is not walked (case-insensitive)");

    let c = Control::default();
    c.cancel();
    let s = scan::run_scan(&db(tmp.path()), &req, &c, &|_| {}).unwrap();
    assert_eq!(s.status, "cancelled");
    let conn = db::open(&db(tmp.path())).unwrap();
    assert_eq!(query::last_scan(&conn).unwrap().unwrap().status, "cancelled");
}

#[test]
fn old_folders_use_newest_file_time() {
    let tmp = fixture_root();
    let data = tmp.path().join("data");
    write(&data.join("old/a.bin"), &bytes(1, 2048));
    write(&data.join("mixed/a.bin"), &bytes(2, 2048));
    write(&data.join("mixed/new.bin"), &bytes(3, 2048));
    let two_years = std::time::SystemTime::now() - std::time::Duration::from_secs(2 * 365 * 86_400);
    for p in ["old/a.bin", "mixed/a.bin"] {
        std::fs::File::options().write(true).open(data.join(p)).unwrap().set_modified(two_years).unwrap();
    }
    scan(&db(tmp.path()), &data);
    let conn = db::open(&db(tmp.path())).unwrap();
    let old = query::old_folders(&conn, 12, 1, 100, true).unwrap();
    let names: Vec<&str> = old.iter().map(|o| o.entry.name.as_str()).collect();
    assert_eq!(names, vec!["old"], "a folder with one recent file is not old");
    assert!(old[0].months_idle >= 23);
}

#[test]
fn rescan_replaces_index() {
    let tmp = fixture_root();
    let data = tmp.path().join("data");
    write(&data.join("a.bin"), &bytes(1, 10));
    scan(&db(tmp.path()), &data);
    std::fs::remove_file(data.join("a.bin")).unwrap();
    write(&data.join("b.bin"), &bytes(1, 20));
    let s = scan(&db(tmp.path()), &data);
    assert_eq!((s.files, s.bytes), (1, 20));
}
