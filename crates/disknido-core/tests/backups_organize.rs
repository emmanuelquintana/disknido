#![cfg(windows)]
mod common;
use common::*;
use disknido_core::backups::{self, Confidence};
use disknido_core::organize::{self, ItemStatus, Layout, PlanInput, PlanRequest};
use disknido_core::{Control, db, ops};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

fn fixture(d: &Path) {
    write(&d.join("Archivos viejos/Backup Enero 2025/a.txt"), b"a");
    write(&d.join("Archivos viejos/Backup Enero 2025/sub/b.txt"), b"bb");
    write(&d.join("Escritorio/Backup Febrero 2025/c.txt"), b"ccc");
    write(&d.join("Copias/Backup Marzo 2025/d.txt"), b"dddd");
    write(&d.join("Trabajo/Backup PC Trabajo Enero 2025/report.txt"), b"trabajo");
    write(&d.join("Personal/Backup PC Personal Enero 2025/report.txt"), b"personal");
    write(&d.join("Otros/Backup 03-04/g.txt"), b"g");
    write(&d.join("Varios/Respaldo 2024-12/h.txt"), b"h");
    write(&d.join("Fotos/Copia Marzo/i.txt"), b"i");
    write(&d.join("Fotos/Copia/j.txt"), b"j");
    write(&d.join("Proyecto/notas.txt"), b"k");
}

fn request(d: &Path, items: Vec<PlanInput>) -> PlanRequest {
    let vol = d.to_string_lossy()[..2].to_string();
    PlanRequest {
        items,
        root_name: "Backups".into(),
        parents: HashMap::from([(vol, d.to_path_buf())]),
        layout: Layout::YearMonth,
    }
}

fn inputs(list: &[backups::BackupItem]) -> Vec<PlanInput> {
    list.iter().map(|b| PlanInput { path: PathBuf::from(&b.path), year: b.year, month: b.month, no_date: false }).collect()
}

#[test]
fn detect_plan_execute_and_stay_stable() {
    let tmp = fixture_root();
    let d = tmp.path().join("data");
    fixture(&d);
    // An existing organised folder with the same name: must not be merged.
    write(&d.join("Backups/2025/2025-03/Backup Marzo 2025/existing.txt"), b"keep me");
    let dbp = db(tmp.path());
    scan(&dbp, &d);
    let conn = db::open(&dbp).unwrap();
    let list = backups::list(&conn, "Backups").unwrap();
    let by_name: HashMap<&str, &backups::BackupItem> = list.iter().map(|b| (b.name.as_str(), b)).collect();

    assert!(!by_name.contains_key("Copia"), "plain «Copia» is not a backup");
    assert!(!by_name.contains_key("Proyecto"));
    assert!(!by_name.contains_key("Backups"), "the container itself is not a backup");
    let ene = by_name["Backup Enero 2025"];
    assert_eq!((ene.year, ene.month, ene.confidence.clone()), (Some(2025), Some(1), Confidence::High));
    assert_eq!((by_name["Respaldo 2024-12"].year, by_name["Respaldo 2024-12"].month), (Some(2024), Some(12)));
    let amb = by_name["Backup 03-04"];
    assert!(amb.ambiguous && amb.confidence == Confidence::Review, "ambiguous date flagged for review");
    assert_eq!(amb.year, None, "no guessed year for an ambiguous name");
    assert_eq!(by_name["Copia Marzo"].confidence, Confidence::Review);
    let existing = list.iter().find(|b| b.organized).expect("organised layout recognised");
    assert_eq!(existing.name, "Backup Marzo 2025");

    // Plan only the not-yet-organised ones; the ambiguous one keeps no date.
    let todo: Vec<backups::BackupItem> = list.iter().filter(|b| !b.organized).cloned().collect();
    let mut ins = inputs(&todo);
    for i in ins.iter_mut() {
        if i.path.ends_with("Backup 03-04") {
            i.year = None;
            i.month = None;
        }
    }
    let plan = organize::plan(&request(&d, ins), &Control::default()).unwrap();
    let find = |name: &str| plan.items.iter().find(|i| i.src.ends_with(name)).unwrap();
    assert_eq!(find("Backup 03-04").status, ItemStatus::NeedsDate);
    let ene = find("Backup Enero 2025");
    assert_eq!(PathBuf::from(&ene.dst), d.join(r"Backups\2025\2025-01\Backup Enero 2025"));
    assert_eq!((ene.files, ene.bytes), (2, 3));
    let t = find("Backup PC Trabajo Enero 2025");
    let p = find("Backup PC Personal Enero 2025");
    assert_ne!(t.dst, p.dst, "two backups of the same month stay separate");
    let mar = find("Copias\\Backup Marzo 2025");
    assert_eq!(mar.status, ItemStatus::Renamed);
    assert!(mar.dst.ends_with("Backup Marzo 2025 (2)"));

    let res = organize::execute(&conn, &plan.items, &Control::default(), &|_, _| {}).unwrap();
    assert_eq!(res.failed, 0, "{:?}", res.messages);
    assert_eq!(res.done, plan.ready);
    assert_eq!(std::fs::read(d.join(r"Backups\2025\2025-01\Backup Enero 2025\sub\b.txt")).unwrap(), b"bb");
    assert!(!d.join("Archivos viejos/Backup Enero 2025").exists());
    assert_eq!(std::fs::read(d.join(r"Backups\2025\2025-03\Backup Marzo 2025\existing.txt")).unwrap(), b"keep me");
    assert_eq!(std::fs::read(d.join(r"Backups\2025\2025-03\Backup Marzo 2025 (2)\d.txt")).unwrap(), b"dddd");
    assert!(d.join("Otros/Backup 03-04/g.txt").exists(), "undated backup untouched");

    // Second pass: organised backups are recognised and not moved again.
    scan(&dbp, &d);
    let list2 = backups::list(&conn, "Backups").unwrap();
    let organised: Vec<_> = list2.iter().filter(|b| b.organized).collect();
    assert!(organised.len() >= 6, "{organised:?}");
    let plan2 = organize::plan(&request(&d, inputs(&list2)), &Control::default()).unwrap();
    for it in plan2.items.iter().filter(|i| Path::new(&i.src).starts_with(d.join("Backups"))) {
        assert_eq!(it.status, ItemStatus::AlreadyOrganized, "{} would move again", it.src);
    }

    // Undo restores the original locations.
    let undo = organize::undo(&conn, res.op_id, &Control::default()).unwrap();
    assert_eq!(undo.failed, 0, "{:?}", undo.messages);
    assert!(d.join("Archivos viejos/Backup Enero 2025/sub/b.txt").exists());
    assert!(d.join("Copias/Backup Marzo 2025/d.txt").exists());
    let history = ops::history(&conn, 10).unwrap();
    assert_eq!(history[0].kind, "organize");
}

#[test]
fn destination_inside_source_is_refused() {
    let tmp = fixture_root();
    let d = tmp.path().join("data");
    write(&d.join("Backup Enero 2025/a.txt"), b"a");
    let src = d.join("Backup Enero 2025");
    let vol = d.to_string_lossy()[..2].to_string();
    let req = PlanRequest {
        items: vec![PlanInput { path: src.clone(), year: Some(2025), month: Some(1), no_date: false }],
        root_name: "Backups".into(),
        parents: HashMap::from([(vol, src.clone())]),
        layout: Layout::YearMonth,
    };
    let plan = organize::plan(&req, &Control::default()).unwrap();
    assert_eq!(plan.items[0].status, ItemStatus::Refused);
    assert_eq!(plan.ready, 0);
}

#[test]
fn content_change_after_preview_aborts_item() {
    let tmp = fixture_root();
    let d = tmp.path().join("data");
    write(&d.join("x/Backup Junio 2024/a.txt"), b"a");
    let req =
        request(&d, vec![PlanInput { path: d.join("x/Backup Junio 2024"), year: Some(2024), month: Some(6), no_date: false }]);
    let plan = organize::plan(&req, &Control::default()).unwrap();
    write(&d.join("x/Backup Junio 2024/new.txt"), b"added after preview");
    let conn = db::open(&db(tmp.path())).unwrap();
    let res = organize::execute(&conn, &plan.items, &Control::default(), &|_, _| {}).unwrap();
    assert_eq!((res.done, res.failed), (0, 1));
    assert!(d.join("x/Backup Junio 2024/new.txt").exists());
}

#[test]
fn interrupted_moves_are_reconciled_from_disk() {
    let tmp = fixture_root();
    let d = tmp.path().join("data");
    write(&d.join("moved/a.txt"), b"a");
    let conn = db::open(&db(tmp.path())).unwrap();
    let op = ops::begin(&conn, "organize", &serde_json::json!({}), None, true).unwrap();
    ops::add_item(&conn, op, 0, &d.join("gone").to_string_lossy(), Some(&d.join("moved").to_string_lossy())).unwrap();
    ops::add_item(&conn, op, 1, &d.join("moved").to_string_lossy(), Some(&d.join("nowhere").to_string_lossy())).unwrap();
    ops::item_status(&conn, op, 0, "running", None).unwrap();
    ops::item_status(&conn, op, 1, "running", None).unwrap();
    assert_eq!(ops::reconcile_interrupted(&conn).unwrap(), 2);
    db::recover_interrupted(&conn).unwrap();
    let items = ops::items(&conn, op).unwrap();
    assert_eq!(items[0].status, "done");
    assert_eq!(items[1].status, "failed");
    assert_eq!(ops::history(&conn, 1).unwrap()[0].status, "interrupted");
}
