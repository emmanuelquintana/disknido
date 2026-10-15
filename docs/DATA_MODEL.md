# Data model (SQLite)

File: `%LOCALAPPDATA%\io.github.disknido\index.sqlite`. WAL journal,
`synchronous=NORMAL`, foreign keys on. Schema version in `PRAGMA user_version`;
migrations live in `crates/disknido-core/src/db.rs` (`MIGRATIONS`, applied in
order, each in a transaction). Current version: **1**.

| Table | Purpose | Lifetime |
| --- | --- | --- |
| `scans` | one row per analysis: status (`running/done/cancelled/failed/interrupted`), mode, roots (JSON), totals | history |
| `entries` | the current index: `id`, `parent_id` (0 = root), `name`, `is_dir`, `size`, `mtime`, `ctime`, `attrs`, `ext`; folders also `total_size`, `file_count`, `newest_mtime`, `oldest_mtime` | replaced by each scan |
| `scan_errors` | folders that could not be listed (path, message) | per scan |
| `hash_cache` | `path` (lower-case key) → `size`, `mtime`, `file_key`, `partial`, `full` BLAKE3 | until index reset |
| `dup_groups` / `dup_files` | last verified duplicate result; files carry `protected_reason` | replaced by each search |
| `operations` / `operation_items` | journal: kind, status, summary JSON, bytes, free space before/after, reversible, per-item src/dst/status/error | permanent |
| `marks` | user marks per folder: important / ignored / favorite / excluded | permanent |
| `backup_overrides` | user-set year/month, "no date", "not a backup" | permanent |
| `settings` | UI settings JSON under key `app` | permanent |

Indexes on `entries(parent_id)`, `entries(is_dir, size)`,
`entries(ext)` (partial), `dup_files(group_id)`. During a scan the `entries`
indexes are dropped and rebuilt after the bulk insert, followed by `ANALYZE`.

Paths are not stored per entry: they are rebuilt from `(parent_id, name)`
using an in-memory map of folders (`PathResolver`), or a recursive CTE for
single lookups.

Nothing in the database contains file contents. "Borrar índice" deletes
`entries`, `scans`, `scan_errors`, `hash_cache` and duplicate results; it never
touches user files.
