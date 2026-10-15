# Progress log

Real state of the project. Updated 2026-10-15.

## Phases

| Phase | State | Evidence |
| --- | --- | --- |
| 1. Research & architecture | done | `docs/ARCHITECTURE.md`, `docs/DECISIONS.md` |
| 2. Scanner | done | `scan.rs`, `fsx.rs`, `volumes.rs`; `tests/scanner.rs` (9, incl. per-root replacement) |
| 3. Duplicates | done | `duplicates.rs`; `tests/duplicates.rs` (4, incl. real Recycle Bin round-trip when `DISKNIDO_TEST_RECYCLE=1`) |
| 4. Backups | done (same-volume) | `backups.rs`, `organize.rs`; `tests/backups_organize.rs` (4) |
| 5. Compression | done (ZIP) | `compress.rs`; `tests/compression.rs` (5) |
| 6. Cleanup & images | done | `cleanup.rs`, `images.rs`, `recycle.rs` |
| 7. Interface | done | 10 screens, es/en, light/dark; 14 frontend tests; E2E run on the real app |
| 8. Performance & safety | done for 10 k / 100 k; 1 M pending | `docs/PERFORMANCE.md` (duplicates 108 s → 15.7 s on HDD after measured fixes) |
| 9. Publication | done (draft release) | Public repo, CI green (check + audit), `v0.1.0` draft release with NSIS/MSI + SHA256SUMS built by GitHub Actions |

## Tests executed (2026-10-14, Windows 11, Rust 1.99, Node 24)

* `cargo test -p disknido-core` → 26 unit + 18 integration, all passing
  (also with `DISKNIDO_TEST_RECYCLE=1`).
* `cargo clippy --workspace --all-targets -D warnings` → clean;
  `cargo fmt --check` → clean.
* `pnpm typecheck`, `pnpm lint` → clean; `pnpm test` → 14 passing.
* End-to-end on the real dev app (WebView2 over CDP) with synthetic data:
  scan, duplicates, explorer, images (ISO label), backups library, plan,
  organisation of 11 backups + undo, verified ZIP, old folders, history, dark
  theme. Found and fixed: inferred content dates were preselected for
  organisation; grid overflow; card spacing in grids.

## Per-root index (2026-10-15)

Acceptance criterion "Puedo seleccionar D: y E: y analizarlas por separado o
juntas" now holds for separate scans: a scan replaces only the indexed roots
it walks (and roots overlapping them); other drives stay. Schema v2 adds
`entries.scan_id` on root rows; Home shows each drive's own last-analysis
date. Design in `docs/SCANNER.md#per-root-replacement`.

* `cargo test -p disknido-core` → 28 unit + 22 integration, all passing
  (new: scan A then B keeps both, rescan A replaces only A, overlapping roots
  replace each other, cancelled scan keeps unreached roots, v1→v2 migration,
  nested request roots de-duplicated).
* `cargo clippy --workspace --all-targets -D warnings`, `cargo fmt --check` →
  clean. `pnpm typecheck`, `pnpm lint` → clean; `pnpm test` → 15 passing.
* Not yet re-checked end-to-end on the real app or against a real D:/E:.

## Dependency audit

* CI `cargo audit` found RustSec advisories in `time` 0.3.45 and `quick-xml` 0.38 (transitive, via Tauri); updated to `time` 0.3.55 and `quick-xml` 0.42 (`plist` 1.10.1). MSRV raised to 1.88 (already required by `zip`). `pnpm audit --prod`: no known vulnerabilities.

## Reviews

* **Ponytail-style review (manual):** removed unused commands (`entry`,
  `marks`, `running_tasks`), unused helpers (`dirName`, `query::entry`),
  replaced a `HashMap<_, ()>` with `HashSet`, replaced an unsafe static flag
  with an `Arc<AtomicBool>`, avoided chart/router/state/UI libraries. Safety
  checks, error handling, tests and accessibility were kept.
* **apple-design-style review (manual):** one accent colour; status always
  has text, not only colour; reduced motion respected; treemap colours now
  encode one thing (folder vs file); consistent spacing in grids.

## Decisions

See `docs/DECISIONS.md`.

## Pending / known gaps

* 1 M-entry benchmark (generator ready; not enough free disk on the test PC).
* Duplicate results are still replaced per search (a full analysis of E: drops D:'s groups); see docs/LIMITATIONS.md.
* Cross-volume verified moves; allocated-size accounting; ARM64; signing.
* Start-with-Windows and notification settings (not implemented).
* The `v0.1.0` GitHub Release is a **draft**: install the CI-built setup on a clean Windows PC (see docs/RELEASING.md) before publishing it.
* A local NSIS build was installed per-user, launched (window "DiskNido", index created) and uninstalled successfully.
