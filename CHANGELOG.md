# Changelog

All notable changes to this project are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/) and the project uses
[Semantic Versioning](https://semver.org/).

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
