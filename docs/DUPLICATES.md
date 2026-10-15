# Exact duplicates

Source: `crates/disknido-core/src/duplicates.rs`, `recycle.rs`.
Tests: `crates/disknido-core/tests/duplicates.rs`.

DiskNido only reports files whose **whole content** is identical. Names,
dates and extensions are never used as evidence. Removal of a copy always
requires a byte-by-byte comparison with the copy you keep.

## Pipeline

| Phase | What happens | Can it declare duplicates? |
| --- | --- | --- |
| 1. Size | Files from the index grouped by size; unique sizes dropped. Empty files and cloud-only files are skipped. | No |
| 2. Identity + partial fingerprint | Each candidate is opened **once**: `GetFileInformationByHandle` gives (volume serial, file index) — paths sharing it are **hard links to the same data**, one copy, not duplicates — and BLAKE3 of the first and last 64 KiB is computed from the same handle. Unreadable/locked files are excluded and listed. | **No — filter only** |
| 3. Full BLAKE3 | Whole content streamed in 1 MiB chunks (never loaded into RAM); files ≤ 128 KiB were already read completely in phase 2. Cached by `(path, size, mtime, file_key)`; any mismatch invalidates the entry. Size and mtime are re-checked at the end; files that changed are dropped. | Shown as **"Contenido idéntico"** |
| 4. Byte-by-byte | Right before a copy is sent to the Recycle Bin it is compared, chunk by chunk, with the copy you keep. Any difference refuses the removal. | **Required for every removal** |

Why phase 4 runs at removal time: on real drives the full-hash phase can read
hundreds of GB (285 GB on the developer's 4.4 TB test set); comparing all of
it byte by byte during the search doubled that reading for groups the user
may never act on. The guarantee that matters — never removing a copy that is
not byte-identical to the kept one — is enforced where the removal happens.

Reads are issued in path order with one worker pool per drive: disks are read
in parallel, an HDD never paces an SSD, and the UI shows the file being read.

The test `only_identical_files_are_grouped` builds two files whose first
and last 64 KiB are identical but differ in the middle, asserts that their
partial fingerprints collide, and asserts they are **not** reported.

## Before removing a copy

`recycle_duplicates(group, keep, remove[])` refuses unless:

* every path belongs to the group;
* the kept copy still exists with the recorded size and mtime;
* each removed copy is not the kept one, is not protected, still has the
  recorded size and mtime, is not behind a junction, and is not cloud-only;
* at least one copy remains;
* **each removed copy is compared byte-by-byte with the kept copy,
  immediately before** it is sent to the Recycle Bin.

Anything stale is skipped and reported. Nothing is deleted permanently.

## Protection

Copies are protected (cannot be marked) when they are:

* in system or application locations (Windows, Program Files, ProgramData,
  AppData, DiskNido's data folder, recovery folders…);
* virtual machine disks, databases or system components (by type);
* **inside a detected backup**. Two full backups often share thousands of
  files on purpose; removing one copy would break the history of a backup.

## Keep suggestion

A transparent rule, not a guarantee: a protected copy first, otherwise the
shortest path. The UI says so ("La sugerencia no garantiza cuál es el
original"). The oldest date is never treated as proof of originality.

## Scope

Search everything indexed, a drive, or a folder (Explorer → "Buscar
duplicados aquí"). Groups can span volumes.

## Not implemented (by design, for now)

Replacing copies with hard links. It changes semantics (editing one changes
all) and needs more safety work. See [ROADMAP.md](ROADMAP.md).
