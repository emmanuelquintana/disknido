# Safety

DiskNido works with personal files. These are the rules that protect them and
where they are enforced.

## 1. Analysis never modifies files

Scanning, duplicate search, backup/image detection and cleanup analysis only
read. The only writes are to DiskNido's own database in
`%LOCALAPPDATA%\io.github.disknido\index.sqlite`.

## 2. Every modification is explicit and re-validated in Rust

Mutating commands: organise backups, compress (writes a new file), send to
Recycle Bin (duplicates, images, cleanup), empty Recycle Bin, undo organise.
Each one requires a selection plus a confirmation dialog in the UI, and the
Rust side re-checks everything (`safety.rs`):

| Check | Why |
| --- | --- |
| absolute path, no `.`/`..`, not a network path | path traversal, surprises |
| not a protected location (Windows, Program Files, ProgramData, AppData*, DiskNido data, recovery/boot/System Volume Information/$Recycle.Bin, pagefile/hiberfil/swapfile, drive roots, .dll/.sys/.drv) | system integrity |
| no junction or symlink in any path component (cloud placeholder folders allowed) | junction redirection, link races |
| not cloud-only | no forced downloads |
| size and mtime equal to the analysed values | stale results |
| rule membership (cleanup), group membership + byte compare (duplicates), unchanged counts (organise) | per-feature guarantees |

\* AppData is allowed only for the curated cleanup rules that live there.

## 3. Nothing is overwritten, nothing is deleted permanently

* Renames use `MoveFileExW` without `REPLACE_EXISTING` / `COPY_ALLOWED`.
* New files use `create_new`.
* Removal always goes through the Recycle Bin (`FOF_ALLOWUNDO`). If Windows
  cannot recycle an item (e.g. a drive without a bin), Windows shows its own
  warning (`FOF_WANTNUKEWARNING`) instead of deleting silently.
* Emptying the Recycle Bin is a separate, red, confirmed action.
* Compression never deletes originals.

## 4. Journal and recovery

Each operation and each item is written to the journal *before* it runs and
updated after. After a crash, on the next start:

* operations still `running` become `interrupted`;
* items are reconciled from disk (a move is "done" only if the destination
  exists and the source does not); nothing destructive is replayed;
* unfinished `.zip.partial` files are removed.

Organisation can be undone (rename back if the original location is free).
Recycle Bin operations are reversible from the Windows bin. Emptying the bin
and creating archives are recorded as not reversible.

## 5. Privileges

DiskNido runs as a normal user, never asks for elevation and never changes
ACLs. Unreadable folders are reported, not forced.

## Residual risks

* Another program can change a file between DiskNido's last check and the
  rename / recycle call (a narrow race). The impact is limited because the
  operation acts on the path you approved and is reversible.
* The Recycle Bin has a size limit per drive; Windows may offer permanent
  deletion for very large items — you will see Windows' own prompt.
* Hard-link totals in folder sizes can be overstated (see LIMITATIONS).
