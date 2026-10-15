# Testing

## Run everything

```powershell
cargo test -p disknido-core      # unit + integration tests (Windows)
pnpm test                        # Vitest + Testing Library (jsdom)
pnpm typecheck; pnpm lint
cargo clippy --workspace --all-targets -- -D warnings
```

## Fixtures and safety

Integration tests create their data under Cargo's per-target temp dir
(`CARGO_TARGET_TMPDIR/fixtures/dn-*`), deleted when each test ends. They never
use your documents and never a whole drive. The single test that would put a
real file into your Recycle Bin is opt-in:

```powershell
$env:DISKNIDO_TEST_RECYCLE = "1"; cargo test -p disknido-core --test duplicates
```

## What is covered

**Engine unit tests** (`src/*.rs`): date parsing (Spanish/English months,
abbreviations, numeric forms, ambiguity), backup keyword rules and containers,
organised-layout recognition, path normalisation and containment, protected
locations, lexical rules, duplicate exclusions, cleanup rule matching
(extension is not evidence, age, cloud-only), categories, ext parsing, cue
parsing, ISO label parsing, cloud reparse tags, Unicode and >260-char paths,
rename never replaces, migrations idempotent, keep suggestion.

**Engine integration tests** (`tests/*.rs`):

| File | Scenarios |
| --- | --- |
| `scanner.rs` | counts and folder totals, children paging, Unicode search, junction pointing to its own parent is recorded but not followed, exclusions (case-insensitive), cancellation keeps a `cancelled` scan, old folders use the newest file time, re-scan replaces the index |
| `duplicates.rs` | different names/same content → grouped; same name/different content → not; same size/different content → not; same head and tail, different middle (partial hash collides) → not; empty files → not; 24 MB identical pair and a 1-byte-different twin; hard links collapsed; file changed during verification → excluded; locked file → excluded and reported; scope; recycle refuses backup copy, the kept copy, foreign paths and stale (modified) copies |
| `backups_organize.rs` | detection of the spec's names, ambiguous `Backup 03-04` flagged, same-month backups stay separate, existing destination → `(2)` and existing files untouched, contents identical after move, second pass is a no-op, undo restores, destination inside source refused, change after preview aborts, crash reconciliation |
| `compression.rs` | Unicode names, 20-level deep tree, empty folder, 30 MB file, independent unzip comparison, existing archive refused, destination inside source refused, invalid name refused, change during compression → no archive and no partial, cancellation removes partial, corrupted ZIP detected |

**Frontend** (`src/test/*.test.ts(x)`): byte/duration formatting, every
Spanish key exists in English with the same placeholders, treemap tiles fill
the box, expected backup tree, image sorting, labelled navigation with
`aria-current`, drive cards and usage `progressbar`, empty state, scan start
requires a selection and sends the right roots, duplicate group: kept and
protected copies cannot be marked and recycling needs a confirmation dialog,
select-all only in high-confidence cleanup categories, emptying the bin needs
a danger confirmation.

## Manual end-to-end check

The release candidate was exercised against the real app (WebView2 driven over
the DevTools protocol) on synthetic data from `bench demo`: scan, duplicate
search, explorer, disk images with ISO labels, backup library, organisation
plan, real organisation of 11 backups (+ undo), verified ZIP, old folders,
history and dark theme. The screenshots in `docs/screenshots/` come from that
run.

## Not covered automatically

* Different physical volumes in one duplicate group (needs two drives in CI).
* Disk-full during compression (checked up front; not simulated).
* Network drives.
