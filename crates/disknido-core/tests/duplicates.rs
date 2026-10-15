mod common;
use common::*;
use disknido_core::duplicates::{self, DupRequest};
use disknido_core::{Control, Error, db, recycle};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

fn names(groups: &[duplicates::DupGroup], root: &Path) -> BTreeSet<Vec<String>> {
    groups
        .iter()
        .map(|g| {
            let mut v: Vec<String> = g
                .files
                .iter()
                .map(|f| Path::new(&f.path).strip_prefix(root).unwrap().to_string_lossy().replace('\\', "/"))
                .collect();
            v.sort();
            v
        })
        .collect()
}

#[test]
fn only_identical_files_are_grouped() {
    let tmp = fixture_root();
    let d = tmp.path().join("data");
    // 1. different names, same content
    let photo = bytes(1, 300_000);
    write(&d.join("x/foto1.jpg"), &photo);
    write(&d.join("y/imagen.jpg"), &photo);
    // 2. same name, different content
    write(&d.join("x/report.txt"), &bytes(2, 5000));
    write(&d.join("y/report.txt"), &bytes(3, 5000));
    // 3. same size, different content
    write(&d.join("z/s1.bin"), &bytes(4, 300_000));
    write(&d.join("z/s2.bin"), &bytes(5, 300_000));
    // 4. same head and tail, different middle: partial fingerprint collides
    let base = bytes(6, 400_000);
    let mut mid = base.clone();
    mid[200_000] ^= 0xFF;
    write(&d.join("m/a.bin"), &base);
    write(&d.join("m/b.bin"), &mid);
    // 5. empty files
    write(&d.join("e/empty1"), b"");
    write(&d.join("e/empty2"), b"");
    // 6. large files: identical pair + one differing by a single byte
    let big = bytes(7, 24 * 1024 * 1024);
    let mut big2 = big.clone();
    big2[12 * 1024 * 1024] ^= 1;
    write(&d.join("big/one.bin"), &big);
    write(&d.join("big/two.bin"), &big);
    write(&d.join("big/near.bin"), &big2);
    // 9. hard link + a real copy
    let hl = bytes(8, 70_000);
    write(&d.join("hl/orig.bin"), &hl);
    std::fs::hard_link(d.join("hl/orig.bin"), d.join("hl/link.bin")).unwrap();
    write(&d.join("hl/copy.bin"), &hl);

    assert_eq!(
        duplicates::partial_hash(&d.join("m/a.bin"), 400_000).unwrap(),
        duplicates::partial_hash(&d.join("m/b.bin"), 400_000).unwrap(),
        "fixture must collide on the partial fingerprint"
    );

    let dbp = db(tmp.path());
    scan(&dbp, &d);
    let req = DupRequest { min_size: 1, scopes: vec![], profile: Default::default() };
    let sum = duplicates::find_duplicates(&dbp, &req, &Control::default(), &|_| {}).unwrap();
    let conn = db::open(&dbp).unwrap();
    let (groups, _) = duplicates::list_groups(&conn, 0, 100).unwrap();
    assert!(groups.iter().all(|g| !g.verified), "byte verification happens before removal, not during the search");
    let got = names(&groups, &d);
    let hl_group: Vec<String> =
        groups.iter().flat_map(|g| g.files.iter()).map(|f| f.path.replace('\\', "/")).filter(|p| p.contains("/hl/")).collect();
    assert_eq!(hl_group.len(), 2, "hard links count as one copy: {hl_group:?}");
    let without_hl: BTreeSet<Vec<String>> = got.into_iter().filter(|g| !g[0].starts_with("hl/")).collect();
    let expected: BTreeSet<Vec<String>> =
        [vec!["big/one.bin".to_string(), "big/two.bin".to_string()], vec!["x/foto1.jpg".to_string(), "y/imagen.jpg".to_string()]]
            .into();
    assert_eq!(without_hl, expected);
    assert!(sum.hard_links_collapsed >= 1);
    assert_eq!(sum.groups, 3);
}

#[test]
fn files_changing_or_locked_are_excluded() {
    let tmp = fixture_root();
    let d = tmp.path().join("data");
    let a = bytes(10, 200_000);
    write(&d.join("chg/a.bin"), &a);
    write(&d.join("chg/b.bin"), &a);
    let l = bytes(11, 200_000);
    write(&d.join("lock/a.bin"), &l);
    write(&d.join("lock/b.bin"), &l);
    let dbp = db(tmp.path());
    scan(&dbp, &d);

    #[cfg(windows)]
    let _lock = {
        use std::os::windows::fs::OpenOptionsExt;
        std::fs::OpenOptions::new().read(true).share_mode(0).open(d.join("lock/b.bin")).unwrap()
    };
    let changed = std::sync::atomic::AtomicBool::new(false);
    let target = d.join("chg/b.bin");
    let req = DupRequest { min_size: 1, scopes: vec![], profile: Default::default() };
    let sum = duplicates::find_duplicates(&dbp, &req, &Control::default(), &|p| {
        if p.phase == "checking" && !changed.swap(true, std::sync::atomic::Ordering::SeqCst) {
            // Different length: the index keeps whole-second mtimes, so a
            // same-size rewrite within the same second is caught only by the
            // byte comparison at removal time (see docs/LIMITATIONS.md).
            let mut other = a.clone();
            other.push(1);
            std::fs::write(&target, &other).unwrap();
        }
    })
    .unwrap();
    assert_eq!(sum.groups, 0, "neither the changed pair nor the locked pair may be reported");
    #[cfg(windows)]
    assert!(sum.skipped.iter().any(|(p, _)| p.ends_with("b.bin") && p.contains("lock")));
}

#[test]
fn scope_limits_search() {
    let tmp = fixture_root();
    let d = tmp.path().join("data");
    let c = bytes(12, 10_000);
    write(&d.join("in/a.bin"), &c);
    write(&d.join("in/b.bin"), &c);
    write(&d.join("out/c.bin"), &c);
    let dbp = db(tmp.path());
    scan(&dbp, &d);
    let req = DupRequest { min_size: 1, scopes: vec![d.join("in").to_string_lossy().into_owned()], profile: Default::default() };
    let s = duplicates::find_duplicates(&dbp, &req, &Control::default(), &|_| {}).unwrap();
    assert_eq!((s.groups, s.files), (1, 2));
}

#[test]
fn recycling_copies_is_guarded() {
    let tmp = fixture_root();
    let d = tmp.path().join("data");
    let c = bytes(13, 50_000);
    write(&d.join("docs/doc.pdf"), &c);
    write(&d.join("old/Backup Enero 2025/doc.pdf"), &c);
    write(&d.join("tmp/doc copy.pdf"), &c);
    let dbp = db(tmp.path());
    scan(&dbp, &d);
    let req = DupRequest { min_size: 1, scopes: vec![], profile: Default::default() };
    duplicates::find_duplicates(&dbp, &req, &Control::default(), &|_| {}).unwrap();
    let conn = db::open(&dbp).unwrap();
    let (groups, _) = duplicates::list_groups(&conn, 0, 10).unwrap();
    let g = &groups[0];
    assert_eq!(g.files.len(), 3);
    let backup = g.files.iter().find(|f| f.path.contains("Backup Enero")).unwrap();
    assert!(backup.protected_reason.as_deref().unwrap().contains("respaldo"));
    assert_eq!(g.files[g.suggested_keep].path, backup.path, "protected copy is suggested to keep");

    let keep = PathBuf::from(&d.join("docs/doc.pdf"));
    let ctl = Control::default();
    // a backup copy cannot be removed through the common duplicate flow
    let e = recycle::recycle_duplicates(&conn, g.id, &keep, &[PathBuf::from(&backup.path)], &ctl).unwrap_err();
    assert!(matches!(e, Error::Refused(_)));
    // the kept copy cannot be removed
    assert!(recycle::recycle_duplicates(&conn, g.id, &keep, std::slice::from_ref(&keep), &ctl).is_err());
    // a path outside the group is refused
    assert!(recycle::recycle_duplicates(&conn, g.id, &keep, &[d.join("nope.pdf")], &ctl).is_err());
    // a copy modified after the analysis is refused (stale result)
    let copy = d.join("tmp/doc copy.pdf");
    std::fs::write(&copy, bytes(14, 50_000)).unwrap();
    let r = recycle::recycle_duplicates(&conn, g.id, &keep, std::slice::from_ref(&copy), &ctl).unwrap();
    assert_eq!((r.recycled, r.failed), (0, 1));
    assert!(copy.exists(), "nothing is removed on a stale result");

    // Optional real Recycle Bin round-trip (puts a small file in your bin).
    if std::env::var("DISKNIDO_TEST_RECYCLE").is_ok() {
        std::fs::write(&copy, &c).unwrap();
        scan(&dbp, &d);
        duplicates::find_duplicates(&dbp, &req, &Control::default(), &|_| {}).unwrap();
        let (groups, _) = duplicates::list_groups(&conn, 0, 10).unwrap();
        let r = recycle::recycle_duplicates(&conn, groups[0].id, &keep, std::slice::from_ref(&copy), &ctl).unwrap();
        assert_eq!(r.recycled, 1);
        assert!(!copy.exists() && keep.exists());
    }
}
