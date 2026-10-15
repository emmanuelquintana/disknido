//! Reproducible benchmarks and demo data. Synthetic data only.
//!
//!   cargo run --release -p disknido-core --example bench -- gen <dir> <files>
//!   cargo run --release -p disknido-core --example bench -- run <dir> [--profile low|balanced|max]
//!   cargo run --release -p disknido-core --example bench -- demo <dir>
//!
//! `run` scans <dir> into a fresh index next to it, searches duplicates and
//! times common UI queries, printing one JSON object with the results.

use disknido_core::{Control, Profile, db, duplicates, query, scan};
use std::path::{Path, PathBuf};
use std::time::Instant;

fn rng(seed: &mut u64) -> u64 {
    *seed ^= *seed << 13;
    *seed ^= *seed >> 7;
    *seed ^= *seed << 17;
    *seed
}

/// `files` files spread over a tree ~3 levels deep, 200 files per folder.
/// About 5 % of files are exact copies of an earlier file; another 2 % have
/// the same size but different bytes (tests the partial-hash filter).
fn generate(dir: &Path, files: usize) {
    let mut seed = 0x9E37_79B9_7F4A_7C15u64;
    let per_dir = 200;
    let mut originals: Vec<(PathBuf, Vec<u8>)> = Vec::new();
    for i in 0..files {
        let d = i / per_dir;
        let folder = dir.join(format!("a{:03}", d / 100)).join(format!("b{:03}", d % 100));
        if i % per_dir == 0 {
            std::fs::create_dir_all(&folder).unwrap();
        }
        let r = rng(&mut seed);
        let path = folder.join(format!("file_{i:07}.{}", ["txt", "jpg", "bin", "pdf", "dat"][(r % 5) as usize]));
        let data = if r.is_multiple_of(20) && !originals.is_empty() {
            originals[(r as usize / 20) % originals.len()].1.clone()
        } else if r % 50 == 1 && !originals.is_empty() {
            let mut d = originals[(r as usize / 50) % originals.len()].1.clone();
            if let Some(b) = d.last_mut() {
                *b ^= 0xFF;
            }
            d
        } else {
            let len = 512 + (rng(&mut seed) % 16_384) as usize;
            (0..len).map(|_| rng(&mut seed) as u8).collect()
        };
        std::fs::write(&path, &data).unwrap();
        if originals.len() < 2000 && r.is_multiple_of(7) {
            originals.push((path, data));
        }
    }
}

/// A small, realistic-looking (but synthetic) tree for screenshots and manual testing.
fn demo(dir: &Path) {
    let w = |p: &str, data: &[u8]| {
        let p = dir.join(p);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, data).unwrap();
    };
    let mut seed = 42u64;
    let mut blob = |n: usize| (0..n).map(|_| rng(&mut seed) as u8).collect::<Vec<u8>>();
    let photo = blob(3 * 1024 * 1024);
    let video = blob(24 * 1024 * 1024);
    for (folder, n) in [
        ("Archivos viejos/Backup Enero 2025", 4),
        ("Escritorio/Backup Febrero 2025", 3),
        ("Copias/Backup Marzo 2025", 5),
        ("Trabajo/Backup PC Trabajo Enero 2025", 2),
        ("Personal/Backup PC Personal Enero 2025", 2),
        ("Discos/BACKUP_2024_08", 3),
        ("Discos/bk-02-2025", 2),
        ("Respaldos/Respaldo Laptop 2023-11", 3),
        ("Respaldos/Desktop Backup 01-2024", 2),
        ("Varios/Backup 03-04", 1),
        ("Varios/Copia Marzo", 1),
        ("Varios/Mis Archivos Diciembre 2025", 2),
    ] {
        for i in 0..n {
            w(
                &format!("{folder}/documentos/informe_{i}.txt"),
                &"Informe sintético de prueba.\n".repeat(2000 + i * 300).into_bytes(),
            );
            w(&format!("{folder}/fotos/IMG_{i:04}.jpg"), &blob(400_000 + i * 50_000));
        }
    }
    w("Fotos/Vacaciones/foto1.jpg", &photo);
    w("Archivos viejos/Backup Enero 2025/fotos/imagen.jpg", &photo);
    w("Descargas/copia.jpg", &photo);
    w("Videos/viaje.mp4", &video);
    w("Videos/Copia de viaje.mp4", &video);
    // ISO with a real ISO 9660 primary volume descriptor (label only; synthetic content).
    let mut iso = vec![0u8; 0x8800];
    iso[0x8000] = 1;
    iso[0x8001..0x8006].copy_from_slice(b"CD001");
    iso[0x8028..0x8028 + 21].copy_from_slice(b"CCSA_X64FRE_ES-ES_DV9");
    iso.extend(blob(6 * 1024 * 1024));
    w("Imagenes/Windows 11.iso", &iso);
    w(
        "Imagenes/Juegos/Aventura (Disco 1).cue",
        b"FILE \"Aventura (Disco 1).bin\" BINARY\n  TRACK 01 MODE2/2352\n    INDEX 01 00:00:00\n",
    );
    w("Imagenes/Juegos/Aventura (Disco 1).bin", &blob(9 * 1024 * 1024));
    w("Imagenes/Juegos/Carreras.rvz", &blob(7 * 1024 * 1024));
    w("Imagenes/Juegos/Plataformas.chd", &blob(5 * 1024 * 1024));
    // An old project folder (mtime pushed back 3 years).
    w("Proyectos/Tesis 2021/capitulo1.docx", &blob(800_000));
    w("Proyectos/Tesis 2021/datos.csv", &"a,b,c\n1,2,3\n".repeat(50_000).into_bytes());
    let three_years = std::time::SystemTime::now() - std::time::Duration::from_secs(3 * 365 * 86_400);
    for f in ["Proyectos/Tesis 2021/capitulo1.docx", "Proyectos/Tesis 2021/datos.csv"] {
        std::fs::File::options().write(true).open(dir.join(f)).unwrap().set_modified(three_years).unwrap();
    }
}

#[cfg(windows)]
fn peak_rss() -> u64 {
    use windows_sys::Win32::System::ProcessStatus::{GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS};
    use windows_sys::Win32::System::Threading::GetCurrentProcess;
    let mut c: PROCESS_MEMORY_COUNTERS = unsafe { std::mem::zeroed() };
    c.cb = std::mem::size_of::<PROCESS_MEMORY_COUNTERS>() as u32;
    unsafe { GetProcessMemoryInfo(GetCurrentProcess(), &mut c, c.cb) };
    c.PeakWorkingSetSize as u64
}
#[cfg(not(windows))]
fn peak_rss() -> u64 {
    0
}

fn time<T>(f: impl FnOnce() -> T) -> (T, f64) {
    let t = Instant::now();
    let r = f();
    (r, t.elapsed().as_secs_f64() * 1000.0)
}

fn run(dir: &Path, profile: Profile) {
    let db_path = dir.with_extension("bench.sqlite");
    for ext in ["", "-wal", "-shm"] {
        let _ = std::fs::remove_file(format!("{}{ext}", db_path.display()));
    }
    let req = scan::ScanRequest {
        roots: vec![dir.to_path_buf()],
        mode: "full".into(),
        profile,
        exclusions: vec![],
        include_hidden: true,
    };
    let (s, scan_ms) = time(|| scan::run_scan(&db_path, &req, &Control::default(), &|_| {}).unwrap());
    let dreq = duplicates::DupRequest { min_size: 1, scopes: vec![], profile };
    let (d, dup_ms) = time(|| duplicates::find_duplicates(&db_path, &dreq, &Control::default(), &|_| {}).unwrap());
    let (d2, dup_cached_ms) = time(|| duplicates::find_duplicates(&db_path, &dreq, &Control::default(), &|_| {}).unwrap());
    assert_eq!(d.groups, d2.groups);
    let conn = db::open(&db_path).unwrap();
    let root = query::roots(&conn).unwrap()[0].id;
    let (_, q_children) = time(|| query::children(&conn, root, 0, 500).unwrap());
    let (_, q_largest) = time(|| query::largest_files(&conn, 50).unwrap());
    let (_, q_ext) = time(|| query::extensions(&conn, 20).unwrap());
    let (_, q_old) = time(|| query::old_folders(&conn, 12, 0, 500, false).unwrap());
    let (_, q_search) = time(|| query::search(&conn, "file_00012", 100).unwrap());
    drop(conn);
    let index_bytes: u64 =
        ["", "-wal"].iter().filter_map(|e| std::fs::metadata(format!("{}{e}", db_path.display())).ok()).map(|m| m.len()).sum();
    let out = serde_json::json!({
        "profile": format!("{profile:?}"),
        "files": s.files, "dirs": s.dirs, "bytes": s.bytes,
        "scan_ms": scan_ms, "files_per_sec": s.files as f64 / (scan_ms / 1000.0),
        "dup_ms": dup_ms, "dup_cached_ms": dup_cached_ms, "dup_groups": d.groups, "dup_files": d.files,
        "query_ms": { "children": q_children, "largest_files": q_largest, "extensions": q_ext, "old_folders": q_old, "search": q_search },
        "index_bytes": index_bytes,
        "peak_working_set_bytes": peak_rss(),
        "cpus": std::thread::available_parallelism().map(|n| n.get()).unwrap_or(0),
    });
    println!("{}", serde_json::to_string_pretty(&out).unwrap());
}

fn phase_logger(label: &'static str) -> impl Fn(&str, String) + Sync {
    let start = Instant::now();
    let last = std::sync::Mutex::new((String::new(), Instant::now()));
    move |phase: &str, detail: String| {
        let mut l = last.lock().unwrap();
        if l.0 != phase || l.1.elapsed().as_secs() >= 10 {
            eprintln!("[{label} {:>8.1}s] {phase:<12} {detail}", start.elapsed().as_secs_f64());
            *l = (phase.to_string(), Instant::now());
        }
    }
}

fn diag_scan(db: &Path, roots: Vec<PathBuf>) {
    let log = phase_logger("scan");
    let req =
        scan::ScanRequest { roots, mode: "full".into(), profile: Profile::Balanced, exclusions: vec![], include_hidden: true };
    let s = scan::run_scan(db, &req, &Control::default(), &|p| {
        log(&p.phase, format!("files={} dirs={} bytes={} errors={}", p.files, p.dirs, p.bytes, p.errors))
    })
    .unwrap();
    println!("{}", serde_json::to_string(&serde_json::json!({"files": s.files, "dirs": s.dirs, "bytes": s.bytes, "errors": s.errors, "ms": s.elapsed_ms, "peak_ws": peak_rss()})).unwrap());
}

fn diag_dups(db: &Path, min_size: u64) {
    let log = phase_logger("dups");
    let req = duplicates::DupRequest { min_size, scopes: vec![], profile: Profile::Balanced };
    let t = Instant::now();
    let d = duplicates::find_duplicates(db, &req, &Control::default(), &|p| {
        log(&p.phase, format!("{}/{} files, {}/{} MB", p.files_done, p.files_total, p.bytes_done >> 20, p.bytes_total >> 20))
    })
    .unwrap();
    println!(
        "groups={} files={} redundant_mb={} skipped={} ms={}",
        d.groups,
        d.files,
        d.redundant_bytes >> 20,
        d.skipped.len(),
        t.elapsed().as_millis()
    );
}

fn diag_queries(db: &Path) {
    let conn = db::open(db).unwrap();
    let n: i64 = conn.query_row("SELECT COUNT(*) FROM entries", [], |r| r.get(0)).unwrap();
    let dirs: i64 = conn.query_row("SELECT COUNT(*) FROM entries WHERE is_dir=1", [], |r| r.get(0)).unwrap();
    println!("entries={n} dirs={dirs}");
    let root = query::roots(&conn).unwrap()[0].id;
    let (_, a) = time(|| query::children(&conn, root, 0, 500).unwrap());
    let (_, b) = time(|| query::largest_files(&conn, 50).unwrap());
    let (_, c) = time(|| query::largest_folders(&conn, 50).unwrap());
    let (_, d) = time(|| query::extensions(&conn, 20).unwrap());
    let (_, e) = time(|| query::categories(&conn).unwrap());
    let (o, f) = time(|| query::old_folders(&conn, 12, 100 << 20, 100_000, false).unwrap());
    let (_, g) = time(|| query::old_folders(&conn, 36, 1 << 30, 500, true).unwrap());
    let (bk, h) = time(|| disknido_core::backups::list(&conn, "Backups").unwrap());
    let (im, i) = time(|| disknido_core::images::list(&conn, false).unwrap());
    let (_, j) = time(|| query::search(&conn, "setup", 200).unwrap());
    let (_, k) = time(|| duplicates::totals(&conn).unwrap());
    println!(
        "children {a:.0}ms | largest_files {b:.0} | largest_folders {c:.0} | extensions {d:.0} | categories {e:.0} | old(12m,count={}) {f:.0} | old(36m,types) {g:.0} | backups(n={}) {h:.0} | images(n={}) {i:.0} | search {j:.0} | dup_totals {k:.0}",
        o.len(),
        bk.len(),
        im.len()
    );
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let dir = PathBuf::from(args.get(2).expect("usage: bench gen|run|demo <dir> ..."));
    match args[1].as_str() {
        "gen" => generate(&dir, args.get(3).and_then(|n| n.parse().ok()).unwrap_or(10_000)),
        "demo" => demo(&dir),
        "run" => {
            let profile = match args.iter().position(|a| a == "--profile").and_then(|i| args.get(i + 1)).map(String::as_str) {
                Some("low") => Profile::Low,
                Some("max") => Profile::Max,
                _ => Profile::Balanced,
            };
            run(&dir, profile)
        }
        // bench scan <db> <root>... : real-drive diagnostics, phase timings
        "scan" => {
            let roots: Vec<PathBuf> = args[3..].iter().filter(|a| !a.starts_with("--")).map(PathBuf::from).collect();
            diag_scan(&dir, roots)
        }
        // bench dups <db> <min_size> : duplicate search with phase timings
        "dups" => diag_dups(&dir, args.get(3).and_then(|n| n.parse().ok()).unwrap_or(1 << 20)),
        // bench queries <db> : time the UI queries over an existing index
        "queries" => diag_queries(&dir),
        other => panic!("unknown command {other}"),
    }
}
