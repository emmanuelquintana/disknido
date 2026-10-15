# Compression

Source: `crates/disknido-core/src/compress.rs`. Tests: `tests/compression.rs`.

## Format

**ZIP** (Deflate, UTF-8 names, ZIP64 automatically for large entries), opened
natively by Windows Explorer. Built with the pure-Rust `zip` crate
(`deflate-flate2-zlib-rs` only, no C toolchain needed, MIT). 7-Zip is not
required. Files whose format is already compressed (JPEG, PNG, MP4, MKV, ZIP,
RAR, 7z, CHD, RVZ, Office XML…) are *stored* instead of deflated: deflating
them wastes time and gains nothing.

`7z` and `tar.zst` were evaluated and not added: they would not open natively
on Windows 10, and ZIP covers the goal. See DECISIONS.

## Safety model

* The default and only mode is **create the archive and keep the originals**.
  DiskNido never deletes the source folder.
* The archive is written to `<name>.zip.partial` (created with `create_new`, so
  nothing is overwritten) and renamed to `<name>.zip` only after verification,
  with a rename that refuses to replace an existing file.
* Refused up front: destination inside the source, protected destination,
  destination behind a junction, invalid file name, existing archive,
  cloud-only files in the source, not enough free space (source size + 64 MB,
  the worst case when nothing compresses).
* On any failure or cancellation the `.partial` file is removed, the error is
  recorded in History, and the originals are untouched. After a crash, the
  next start removes the leftover `.partial` of interrupted operations.

## Verification (always on)

1. While writing, every source file is streamed through BLAKE3 (1 MiB
   buffers; memory does not grow with the backup size).
2. After writing, every source file's size and modification time are checked
   again; a change aborts ("cambió durante la compresión").
3. The archive is reopened. The number of entries must equal files + folders.
   Every entry is decompressed and hashed; the size and BLAKE3 must match the
   original's.

Only then does the UI show "ZIP creado y verificado". `test_archive` can also
re-check an existing ZIP (CRC of every entry); a corrupted ZIP is detected
(tested).

## What a ZIP does not preserve

Shown in the result: NTFS permissions (ACLs), attributes (hidden, system),
alternate data streams, creation time, and modification time beyond 2-second
precision. Symlinks/junctions inside the folder are not followed nor included.

## Space: what is actually recovered

Three separate numbers are shown: source size, archive size, and the
*potential* saving if you later remove the original. Example shown in the UI:

> Compressing 100 GB into a 70 GB ZIP while keeping the originals uses about
> 170 GB. You would only recover about 30 GB if you later remove the
> originals.

Before compressing, the preview shows which share of the data is already in
compressed formats ("94 % ya está comprimido"), because savings there will be
small. There is no fixed savings promise and no sampling estimate.
