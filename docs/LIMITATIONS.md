# Known limitations (v0.1.0)

* **Folder totals count hard links once per path.** Logical sizes of folders
  that contain several hard links to the same file are overstated. Duplicate
  detection is correct (hard links are collapsed there).
* **Logical size only.** Allocated size for sparse or NTFS-compressed files
  is not computed yet.
* **"Last used" is unknown.** Windows does not reliably record last access;
  old-folder detection uses the newest modification time inside the folder.
* **A scan replaces overlapping roots whole.** Scanning D: and then E:
  separately keeps both; re-scanning D: replaces only D:. But a root that
  overlaps an indexed one replaces it entirely: scanning `D:\Fotos` after
  `D:\` leaves only `D:\Fotos` indexed for that drive (no file is ever indexed
  twice). Cancelling keeps the old index of roots the scan had not reached; a
  root whose walk was cancelled keeps only its partial new results.
* **Duplicate results cover one search.** Each duplicate search replaces the
  previous groups, and a full analysis searches only the roots it scanned, so
  after a full analysis of E: the duplicate list no longer shows D:'s groups.
  Run a duplicate search with "all" as scope to cover every indexed drive.
* **Organisation is per volume.** Cross-volume moves (verified copy then
  remove) are not implemented.
* **Recycle Bin paths limited to MAX_PATH** (Windows shell API). Longer paths
  are refused with an explanation.
* **Disk images:** a `.bin`/`.img` smaller than 32 MB is listed only when a
  `.cue`/`.ccd`/`.mds` sits next to it.
* **Installed-app detection is a heuristic** (programs and libraries inside,
  or a known launcher folder). It only hides folders from "old folders" by
  default; nothing is modified.
* **Catalog of imported ZIPs is not verified:** their lists are read from the
  ZIP's directory without hashing.
* **Disk image platform hints** come only from platform-specific formats
  (RVZ/WBFS, WUD/WUX, CSO/ZSO, GDI/CDI) and ISO 9660 labels of known OS
  installers. No game metadata is guessed.
* **No scheduled scans, notifications, autostart, or update checks.**
* **ARM64 builds** are not produced or tested yet.
* **Unsigned binaries.** Windows SmartScreen may warn on first run.
* **Tests and builds run on Windows.** Some unit tests use Windows paths and
  are compiled only on Windows.
