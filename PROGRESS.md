# Progress log

Real state of the project. Updated 2026-10-14.

## Phases

| Phase | State | Evidence |
| --- | --- | --- |
| 1. Research & architecture | done | `docs/ARCHITECTURE.md`, `docs/DECISIONS.md` |
| 2. Scanner | done | `scan.rs`, `fsx.rs`, `volumes.rs`; `tests/scanner.rs` (5) |
| 3. Duplicates | done | `duplicates.rs`; `tests/duplicates.rs` (4, incl. real Recycle Bin round-trip when `DISKNIDO_TEST_RECYCLE=1`) |
| 4. Backups | done (same-volume) | `backups.rs`, `organize.rs`; `tests/backups_organize.rs` (4) |
| 5. Compression | done (ZIP) | `compress.rs`; `tests/compression.rs` (5) |
| 6. Cleanup & images | done | `cleanup.rs`, `images.rs`, `recycle.rs` |
| 7. Interface | done | 10 screens, es/en, light/dark; 14 frontend tests; E2E run on the real app |
| 8. Performance & safety | done for 10 k / 100 k; 1 M pending | `docs/PERFORMANCE.md` (duplicates 108 s → 15.7 s on HDD after measured fixes) |
| 9. Publication | see below | CI/release workflows, docs, installers |

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
* Several indexes at once (a new scan replaces the index).
* Cross-volume verified moves; allocated-size accounting; ARM64; signing.
* Start-with-Windows and notification settings (not implemented).
* CI has not run yet on GitHub at the time of writing (see the Actions tab).
