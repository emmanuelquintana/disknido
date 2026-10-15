//! Shared fixture helpers. Fixtures live under Cargo's per-target temp dir
//! (never in the user's documents, never on a whole drive), outside AppData
//! so the safety rules treat them like ordinary user folders.
#![allow(dead_code)]

use std::path::{Path, PathBuf};

pub fn fixture_root() -> tempfile::TempDir {
    let base = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("fixtures");
    std::fs::create_dir_all(&base).unwrap();
    tempfile::Builder::new().prefix("dn-").tempdir_in(base).unwrap()
}

pub fn write(path: &Path, data: &[u8]) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, data).unwrap();
}

/// Deterministic pseudo-random bytes (no external RNG needed).
pub fn bytes(seed: u64, len: usize) -> Vec<u8> {
    let mut x = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
    (0..len)
        .map(|_| {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            (x >> 24) as u8
        })
        .collect()
}

pub fn db(dir: &Path) -> PathBuf {
    dir.join("index.sqlite")
}

pub fn scan(db_path: &Path, root: &Path) -> disknido_core::scan::ScanSummary {
    let req = disknido_core::scan::ScanRequest {
        roots: vec![root.to_path_buf()],
        mode: "full".into(),
        profile: Default::default(),
        exclusions: vec![],
        include_hidden: true,
    };
    disknido_core::scan::run_scan(db_path, &req, &Default::default(), &|_| {}).unwrap()
}
