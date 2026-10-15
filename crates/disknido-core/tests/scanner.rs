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

/// Every child id is larger than its parent's and every parent exists.
fn assert_tree_invariants(conn: &rusqlite::Connection) {
    let bad: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM entries c LEFT JOIN entries p ON p.id=c.parent_id
             WHERE c.parent_id<>0 AND (p.id IS NULL OR c.id <= p.id)",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(bad, 0, "orphans or child ids not above their parent");
}

fn root_sizes(conn: &rusqlite::Connection) -> Vec<(String, u64, Option<i64>)> {
    query::indexed_roots(conn).unwrap().into_iter().map(|r| (r.entry.name, r.entry.size, r.scan_id)).collect()
}

#[test]
fn separate_scans_keep_other_roots() {
    let tmp = fixture_root();
    let (a, b) = (tmp.path().join("A"), tmp.path().join("B"));
    write(&a.join("x/1.bin"), &bytes(1, 100));
    write(&b.join("2.bin"), &bytes(2, 30));
    let db_path = db(tmp.path());
    let sa = scan(&db_path, &a);
    let sb = scan(&db_path, &b);

    let conn = db::open(&db_path).unwrap();
    let roots = root_sizes(&conn);
    let name = |p: &std::path::Path| p.to_string_lossy().into_owned();
    assert_eq!(roots, vec![(name(&a), 100, Some(sa.scan_id)), (name(&b), 30, Some(sb.scan_id))]);
    assert_tree_invariants(&conn);
    // Global views see both roots.
    let files = query::largest_files(&conn, 10).unwrap();
    assert_eq!(files.len(), 2);
    assert!(files.iter().any(|f| f.path.starts_with(&name(&a))));
    assert!(files.iter().any(|f| f.path.starts_with(&name(&b))));
    // A's folder totals survive B's scan (B's aggregation touches only B).
    let (kids, _) = query::children(&conn, query::roots(&conn).unwrap()[0].id, 0, 10).unwrap();
    assert_eq!((kids[0].name.as_str(), kids[0].size), ("x", 100));
}

#[test]
fn rescan_replaces_only_that_root() {
    let tmp = fixture_root();
    let (a, b) = (tmp.path().join("A"), tmp.path().join("B"));
    write(&a.join("old.bin"), &bytes(1, 100));
    write(&b.join("sub/2.bin"), &bytes(2, 30));
    let db_path = db(tmp.path());
    scan(&db_path, &a);
    let sb = scan(&db_path, &b);
    std::fs::remove_file(a.join("old.bin")).unwrap();
    write(&a.join("new/n.bin"), &bytes(3, 7));
    let sa2 = scan(&db_path, &a);
    assert_eq!((sa2.files, sa2.bytes), (1, 7));

    let conn = db::open(&db_path).unwrap();
    let name = |p: &std::path::Path| p.to_string_lossy().into_owned();
    // B (now first by id) is untouched; A was replaced, not duplicated.
    assert_eq!(root_sizes(&conn), vec![(name(&b), 30, Some(sb.scan_id)), (name(&a), 7, Some(sa2.scan_id))]);
    assert!(query::search(&conn, "old.bin", 10).unwrap().is_empty(), "stale rows of A are gone");
    assert_eq!(query::search(&conn, "2.bin", 10).unwrap().len(), 1);
    assert_tree_invariants(&conn);
}

#[test]
fn overlapping_roots_replace_each_other() {
    let tmp = fixture_root();
    let data = tmp.path().join("data");
    write(&data.join("in/1.bin"), &bytes(1, 10));
    write(&data.join("2.bin"), &bytes(2, 20));
    let db_path = db(tmp.path());
    scan(&db_path, &data.join("in"));
    scan(&db_path, &data); // contains the earlier root
    let conn = db::open(&db_path).unwrap();
    let roots = root_sizes(&conn);
    assert_eq!(roots.len(), 1, "no file is indexed twice");
    assert_eq!(roots[0].1, 30);
    drop(conn);
    scan(&db_path, &data.join("in")); // inside the indexed root
    let conn = db::open(&db_path).unwrap();
    let roots = root_sizes(&conn);
    assert_eq!(roots.len(), 1);
    assert_eq!(roots[0].1, 10);
    assert_tree_invariants(&conn);
}

#[test]
fn multi_root_scan_and_cancel_keep_unreached_roots() {
    let tmp = fixture_root();
    let (a, b) = (tmp.path().join("A"), tmp.path().join("B"));
    write(&a.join("1.bin"), &bytes(1, 10));
    write(&b.join("2.bin"), &bytes(2, 20));
    let db_path = db(tmp.path());
    let req = |roots: Vec<std::path::PathBuf>| scan::ScanRequest {
        roots,
        mode: "full".into(),
        profile: Default::default(),
        exclusions: vec![],
        include_hidden: true,
    };
    let s = scan::run_scan(&db_path, &req(vec![a.clone(), b.clone()]), &Control::default(), &|_| {}).unwrap();
    assert_eq!((s.files, s.bytes), (2, 30));
    // A cancelled scan replaces nothing it did not start walking.
    let c = Control::default();
    c.cancel();
    scan::run_scan(&db_path, &req(vec![b.clone()]), &c, &|_| {}).unwrap();
    let conn = db::open(&db_path).unwrap();
    let sizes: Vec<u64> = root_sizes(&conn).into_iter().map(|r| r.1).collect();
    assert_eq!(sizes, vec![10, 20]);
    assert_tree_invariants(&conn);
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

#[test]
fn extension_totals_follow_the_index() {
    let tmp = fixture_root();
    let (a, b) = (tmp.path().join("a"), tmp.path().join("b"));
    write(&a.join("x.iso"), &bytes(1, 100));
    write(&a.join("sub/y.iso"), &bytes(2, 50));
    write(&b.join("z.txt"), &bytes(3, 7));
    write(&b.join("noext"), &bytes(4, 3));
    let dbp = db(tmp.path());
    let req = scan::ScanRequest {
        roots: vec![a.clone(), b.clone()],
        mode: "full".into(),
        profile: Default::default(),
        exclusions: vec![],
        include_hidden: true,
    };
    scan::run_scan(&dbp, &req, &Control::default(), &|_| {}).unwrap();
    let conn = db::open(&dbp).unwrap();
    let ext = |k: &str| query::extensions(&conn, 100).unwrap().into_iter().find(|x| x.key == k).map(|x| (x.files, x.bytes));
    assert_eq!(ext("iso"), Some((2, 150)));
    assert_eq!(ext("txt"), Some((1, 7)));
    assert_eq!(ext(""), Some((1, 3)));
    // Rescanning a alone replaces only a's totals.
    std::fs::remove_file(a.join("x.iso")).unwrap();
    scan(&dbp, &a);
    assert_eq!(ext("iso"), Some((1, 50)));
    assert_eq!(ext("txt"), Some((1, 7)));
}

#[test]
fn installed_programs_are_told_apart_from_old_folders() {
    let tmp = fixture_root();
    let d = tmp.path().join("data");
    write(&d.join("Games/Juego/bin/juego.exe"), &bytes(1, 4096));
    write(&d.join("Games/Juego/bin/motor.dll"), &bytes(2, 4096));
    write(&d.join("Proyectos/tesis.docx"), &bytes(3, 4096));
    write(&d.join("Instaladores/setup.exe"), &bytes(4, 4096));
    let old = std::time::SystemTime::now() - std::time::Duration::from_secs(3 * 365 * 86_400);
    for f in ["Games/Juego/bin/juego.exe", "Games/Juego/bin/motor.dll", "Proyectos/tesis.docx", "Instaladores/setup.exe"] {
        std::fs::File::options().write(true).open(d.join(f)).unwrap().set_modified(old).unwrap();
    }
    scan(&db(tmp.path()), &d);
    let conn = db::open(&db(tmp.path())).unwrap();
    let found = query::old_folders(&conn, 12, 0, 100, false).unwrap();
    let reason = |n: &str| found.iter().find(|f| f.entry.name == n).map(|f| f.app_reason.is_some());
    assert_eq!(reason("Games"), Some(true), "exe + dll below: installed program");
    assert_eq!(reason("Proyectos"), Some(false));
    assert_eq!(reason("Instaladores"), Some(false), "a lone installer is not an installed app");
}

#[test]
fn small_bin_files_are_not_disk_images_unless_in_a_set() {
    let tmp = fixture_root();
    let d = tmp.path().join("data");
    write(&d.join("firmware/update.bin"), &bytes(1, 4096));
    write(&d.join("juego/Juego (Track 01).bin"), &bytes(2, 4096));
    write(&d.join("juego/Juego.cue"), b"FILE \"Juego (Track 01).bin\" BINARY\n");
    write(&d.join("x/disco.iso"), &bytes(3, 4096));
    scan(&db(tmp.path()), &d);
    let conn = db::open(&db(tmp.path())).unwrap();
    let mut names: Vec<String> = disknido_core::images::list(&conn, false).unwrap().into_iter().map(|i| i.name).collect();
    names.sort();
    assert_eq!(names, vec!["Juego (Track 01).bin", "Juego.cue", "disco.iso"]);
}

#[test]
fn forgetting_a_path_updates_totals() {
    let tmp = fixture_root();
    let d = tmp.path().join("data");
    write(&d.join("img/juego.iso"), &bytes(1, 1000));
    write(&d.join("img/otro.iso"), &bytes(2, 500));
    write(&d.join("doc.txt"), &bytes(3, 10));
    scan(&db(tmp.path()), &d);
    let conn = db::open(&db(tmp.path())).unwrap();
    query::forget_path(&conn, &d.join("IMG").join("Juego.iso")).unwrap();
    let names: Vec<String> = disknido_core::images::list(&conn, false).unwrap().into_iter().map(|i| i.name).collect();
    assert_eq!(names, vec!["otro.iso"], "a recycled image no longer appears");
    let root = &query::roots(&conn).unwrap()[0];
    assert_eq!((root.size, root.file_count), (510, 2));
    let iso = query::extensions(&conn, 10).unwrap().into_iter().find(|b| b.key == "iso").unwrap();
    assert_eq!((iso.files, iso.bytes), (1, 500));
    // Unknown paths are ignored.
    query::forget_path(&conn, &d.join("nope.bin")).unwrap();
}
