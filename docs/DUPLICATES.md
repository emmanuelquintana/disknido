# Exact duplicates

Source: `crates/disknido-core/src/duplicates.rs`, `recycle.rs`.
Tests: `crates/disknido-core/tests/duplicates.rs`.

DiskNido only reports files whose **content is byte-for-byte identical**.
Names, dates and extensions are never used as evidence.

## Pipeline

| Phase | What happens | Can it declare duplicates? |
| --- | --- | --- |
| 1. Size | Files from the index grouped by size; unique sizes dropped. Empty files and cloud-only files are skipped. | No |
| 2. Identity | Each candidate is opened once; `GetFileInformationByHandle` gives (volume serial, file index). Paths sharing it are **hard links to the same data** — one copy, not duplicates. Unreadable/locked files are excluded and listed. | No |
| 3. Partial fingerprint | BLAKE3 of the first and last 64 KiB. Different → not duplicates. Equal → still unknown. | **No — filter only** |
| 4. Full BLAKE3 | Whole content streamed in 1 MiB chunks (never loaded into RAM). Cached by `(path, size, mtime, file_key)`; any mismatch invalidates the entry. | No |
| 5. Byte-by-byte | Inside each BLAKE3 group every file is compared with a representative, chunk by chunk; groups split on any difference. Size and mtime are re-checked afterwards; files that changed are dropped. | **Yes — only here** |

Only groups that pass phase 5 are stored, with `verified = 1`, and only those
are shown with the "Duplicado verificado" label.

The test `only_byte_identical_files_are_grouped` builds two files whose first
and last 64 KiB are identical but differ in the middle, asserts that their
partial fingerprints collide, and asserts they are **not** reported.

## Before removing a copy

`recycle_duplicates(group, keep, remove[])` refuses unless:

* the group is verified and every path belongs to it;
* the kept copy still exists with the recorded size and mtime;
* each removed copy is not the kept one, is not protected, still has the
  recorded size and mtime, is not behind a junction, and is not cloud-only;
* at least one copy remains;
* **each removed copy is compared byte-by-byte with the kept copy again,
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
