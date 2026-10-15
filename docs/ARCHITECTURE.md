# Architecture

DiskNido is a single Tauri 2 desktop process. There are no services, local
servers or helper processes.

```
┌──────────────────────── WebView2 (React + TypeScript) ────────────────────────┐
│ features/*  pages · components/ui · state.tsx (settings, tasks, navigation)    │
│ api.ts — typed invoke() wrappers           i18n — es (source) / en             │
└───────────────▲───────────────────────────────────────────┬───────────────────┘
                │ events: task://progress, task://done       │ invoke(command, args)
┌───────────────┴───────────────────────────────────────────▼───────────────────┐
│ src-tauri/src/lib.rs — command layer                                           │
│   • validates arguments, never trusts UI state                                 │
│   • long jobs → spawn_blocking + Control (cancel/pause) + throttled events     │
│   • every query on its own SQLite connection off the UI thread (WAL readers)   │
└───────────────────────────────────────▲───────────────────────────────────────┘
                                        │ plain Rust calls
┌───────────────────────────────────────┴───────────────────────────────────────┐
│ crates/disknido-core — engine (no Tauri dependency, testable on its own)       │
│  fsx        FindFirstFileExW listing, reparse tags, file identity, safe rename │
│  volumes    drives, labels, capacity, SSD/HDD, bus type, disk number          │
│  scan       parallel read-only walk → batched SQLite writer → folder totals   │
│  query      paginated/aggregated reads for the UI                             │
│  classify   extension → category (charts only, never deletion evidence)       │
│  duplicates size → identity → partial BLAKE3 → full BLAKE3 → byte compare      │
│  backups    interpretable backup + date detection                            │
│  organize   dry-run plan → re-validated native renames → verify → undo        │
│  compress   streamed ZIP (Deflate/ZIP64) → BLAKE3 re-verification             │
│  catalog    what is inside each ZIP and where it is; accent-insensitive search │
│  cleanup    documented rules over known locations                            │
│  images     disk images, CUE/GDI sets, ISO 9660 labels                        │
│  recycle    Recycle Bin (SHFileOperationW + FOF_ALLOWUNDO), empty bin         │
│  safety     protected locations, lexical rules, link checks, freshness        │
│  ops        operation journal, history, crash reconciliation                  │
│  db         schema + versioned migrations (PRAGMA user_version), WAL          │
└────────────────────────────────────────────────────────────────────────────────┘
```

## Key decisions

See [DECISIONS.md](DECISIONS.md) for the full log. The short version:

* **Engine in its own crate.** `disknido-core` has no Tauri dependency, so its
  Rust tests and the benchmark run without building a UI.
* **Native listing, not `walkdir`/`jwalk`.** `FindFirstFileExW` with
  `FindExInfoBasic` and `FIND_FIRST_EX_LARGE_FETCH` returns names, sizes,
  times, attributes and the reparse tag in one call per directory. We need the
  reparse tag to tell OneDrive placeholder folders (walk them) from junctions
  and symlinks (record, never follow). A small hand-written work-stealing walker
  (≈60 lines) replaces a crate.
* **Ids encode the hierarchy.** Ids are assigned while walking, so a child's id
  is always larger than its parent's. Folder totals are one reverse pass over
  an array — no recursive SQL, no second walk.
* **The index is replaceable per root.** Each scan replaces, in `entries`,
  only the roots it walks (and indexed roots overlapping them); history,
  duplicates, marks and operations survive. "Reset index" never touches files.
* **Rust is the authority.** Every mutating command re-validates paths
  (protected locations, `..`, junctions on the path, cloud-only, size/mtime
  freshness) regardless of what the UI allowed.
* **No permanent deletes.** Removal means the Recycle Bin. Emptying the bin is
  a separate, explicitly confirmed action.

## Frontend ↔ backend contract

* Commands return `Result<T, String>`; errors are user-facing Spanish text
  produced by the engine (`Error::Refused` for safety refusals).
* Long tasks return a task id immediately. Progress arrives as
  `task://progress {taskId, kind, data}` (throttled to ≈8/s) and completion as
  `task://done {taskId, kind, ok, cancelled, result, error}`.
* `cancel_task(id)` and `pause_task(id, paused)` flip atomics that every loop
  checks (`Control::checkpoint`).
* While a scan rewrites the index, index readers return a clear "analysis in
  progress" error instead of reading half-written data.

## Threads

| Work | Threads |
| --- | --- |
| Directory walk | 1 (low) · 2 HDD / 4 SSD (balanced) · 3 HDD / up to 16 SSD (max) |
| Hashing & verification | 1 on HDD; 1 / 3 / 6 on SSD by profile |
| SQLite writer | 1 (batched transactions every 50 000 rows or 750 ms) |
| UI | never blocked: all engine calls run in `spawn_blocking` |

## Repository layout

```
crates/disknido-core/   engine + integration tests + examples/bench.rs
src-tauri/              Tauri app, command layer, icons, capabilities
src/                    React app (features/, components/, i18n/, styles/, test/)
docs/                   documentation (this folder) and real screenshots
assets/                 original logo (SVG)
.github/                CI, release workflow, issue/PR templates
```
