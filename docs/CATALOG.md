# Archive catalog

Source: `crates/disknido-core/src/catalog.rs`. UI: *Catálogo de comprimidos*.

The catalog answers "where is this file?" for everything you have compressed,
without opening any archive.

* **Automatic.** Every ZIP that DiskNido creates and verifies is recorded with
  its location, the folder it came from, and the list of files inside (path
  in the archive, size, modification time and the BLAKE3 used to verify it).
* **Existing ZIPs.** *Añadir ZIP existentes…* reads the archive's central
  directory (names, sizes, dates). Nothing is extracted and no hashes are
  computed, so these entries are marked "not verified".
* **Search.** Type two or more letters; every word must appear in the path
  inside the archive or in the archive's own path. Case and Spanish accents
  are ignored ("cancun" finds "Cancún.jpg"). Results show the file, its folder
  inside the ZIP and the ZIP's location (open it in Explorer or copy the path).
* **Lists.** Each archive can be browsed or exported as CSV
  (`archive, path_in_archive, size_bytes, blake3`).
* **Missing archives.** If a ZIP is moved or deleted, the record stays and is
  flagged "El ZIP ya no está en esa ubicación".
* **Removing.** *Quitar del catálogo* deletes only the record, never the ZIP.
  "Borrar índice de análisis" does not clear the catalog: it is your record,
  not analysis data.

Tables: `archives` and `archive_entries` (schema v3, see
[DATA_MODEL.md](DATA_MODEL.md)). Search keys are stored lower-cased with
accents folded.
