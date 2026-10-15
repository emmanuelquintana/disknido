# Known limitations (v0.1.0)

* **Folder totals count hard links once per path.** Logical sizes of folders
  that contain several hard links to the same file are overstated. Duplicate
  detection is correct (hard links are collapsed there).
* **Logical size only.** Allocated size for sparse or NTFS-compressed files
  is not computed yet.
* **"Last used" is unknown.** Windows does not reliably record last access;
  old-folder detection uses the newest modification time inside the folder.
* **One index at a time.** A new scan replaces the previous index (history of
  scans is kept). Scanning D: then E: separately keeps only E:; select both in
  one scan to keep both.
* **Organisation is per volume.** Cross-volume moves (verified copy then
  remove) are not implemented.
* **Recycle Bin paths limited to MAX_PATH** (Windows shell API). Longer paths
  are refused with an explanation.
* **Disk image platform hints** come only from platform-specific formats
  (RVZ/WBFS, WUD/WUX, CSO/ZSO, GDI/CDI) and ISO 9660 labels of known OS
  installers. No game metadata is guessed.
* **No scheduled scans, notifications, autostart, or update checks.**
* **ARM64 builds** are not produced or tested yet.
* **Unsigned binaries.** Windows SmartScreen may warn on first run.
* **Tests and builds run on Windows.** Some unit tests use Windows paths and
  are compiled only on Windows.
