# Changelog

All notable changes to this project are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/) and the project uses
[Semantic Versioning](https://semver.org/).

## [Unreleased]

### Changed
- A scan replaces only the drives/folders it covers: analysing D: and then E:
  separately keeps both in the index. Each drive shows its own last-analysis
  date. Index schema v2 (`entries.scan_id`), migrated automatically.

## [0.2.0] — unreleased

Fixes from testing on real drives (5.4 M files, 4.4 TB).

### Added
- **Archive catalog**: every verified ZIP records its file list and location;
  add existing ZIPs; accent- and case-insensitive search; CSV export.
- One index per drive: scanning D: and then E: keeps both.
- Old folders recognise installed applications and games (programs + DLLs,
  launcher libraries) and hide them by default.

### Changed
- The app no longer freezes while loading: each screen query uses its own
  database connection in the background; Home shows its summary in two steps.
- Extension/category distribution is computed while scanning (26 s → 2 ms on
  6.2 M entries).
- Drives on different physical disks are scanned at the same time.
- Duplicate search opens each candidate once, reads drives in parallel, and
  always shows the file being read and its progress. Default minimum size is
  1 MB (older settings at 1 KB are migrated).
- Disk images: small `.bin`/`.img` files are listed only as part of a CUE/CCD
  set or when larger than 32 MB (firmware and game data were listed before).
- Faster index statistics after a scan (`analysis_limit`).
- System locations (Windows, Program Files, ProgramData, AppData, WinSxS,
  Installer…, on any drive) never appear in duplicates, disk images, backups
  or old folders.
- Items sent to the Recycle Bin disappear from every list at once: the index
  forgets them and updates folder and extension totals.
- Duplicates: every copy has an explicit "Keep" / "To Recycle Bin" choice, a
  "Keep only the suggested one" button and a plan summary; the last kept copy
  can never be sent to the bin.
- Backups: per-year bars (click to filter), a legend for the confidence
  badges; app/tool folders (`.name`, node_modules, XboxGames, steamapps,
  caches), empty folders, tiny undated folders and impossible years (e.g.
  "2072") are no longer listed as backups.

## [0.1.0] — unreleased

First functional version.

### Added
- Volume discovery (label, file system, capacity, SSD/HDD, bus, disk number).
- Parallel read-only scanner (FindFirstFileExW, long paths, Unicode, links
  recorded and never followed, cloud-only files never opened), SQLite index,
  folder totals, cancel and pause, partial results.
- Space explorer: treemap and virtualised list, properties, largest files and
  folders, distribution by category and extension, name search.
- Exact duplicate detection: size → hard-link identity → partial BLAKE3 →
  full BLAKE3 → byte-by-byte; protected copies; re-verification before
  recycling; CSV export with optional anonymisation.
- Disk images: formats, CUE/GDI sets, ISO 9660 labels, orphan warnings.
- Backups: interpretable detection, Spanish/English dates, ambiguity flags,
  overrides, library with timeline, dry-run organisation plan with expected
  tree, atomic same-volume moves with verification, undo.
- Verified ZIP compression (Deflate/ZIP64, BLAKE3 re-verification, originals
  kept, partial files cleaned up).
- Smart cleanup with documented rules and confidence levels; Recycle Bin
  integration; separate confirmed "empty Recycle Bin".
- Old folders by newest modification time; marks (important, favorite,
  ignored, excluded).
- Operation journal, history, crash reconciliation.
- Spanish and English UI, light/dark/system theme, keyboard and screen-reader
  basics.
- Benchmark and demo-data example; CI and release workflows.
