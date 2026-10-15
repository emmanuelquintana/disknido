# Scanner

Source: `crates/disknido-core/src/scan.rs`, `fsx.rs`, `volumes.rs`.

## Guarantees

* **Read-only.** The scanner opens no file for reading; it only lists
  directories. The only writes are to DiskNido's own SQLite index.
* **No cycles.** Symlinks, junctions and mount points are recorded with their
  attributes and never followed.
* **No cloud downloads.** OneDrive / cloud placeholder *folders*
  (`IO_REPARSE_TAG_CLOUD_*`) are walked; cloud-only *files*
  (`OFFLINE`, `RECALL_ON_OPEN`, `RECALL_ON_DATA_ACCESS`) are recorded and never
  opened by any DiskNido feature.
* **Never forces access.** "Access denied" folders are recorded in
  `scan_errors` and shown in the UI. ACLs are never changed and the app does not
  ask for elevation.

## Per-root replacement

The index holds every scanned root (parent_id = 0), possibly from different
scans. A scan replaces only what it walks:

* Requested roots are de-duplicated first; a root inside another requested
  root is dropped (it would be walked twice).
* New ids continue from `MAX(id)+1`. Ids are handed out while walking and
  roots are walked one after another, so every root's subtree is exactly the
  id range from the root up to the next root, and a child's id is always
  larger than its parent's.
* Just before a root is walked, the writer deletes every older indexed root
  that overlaps it (same path, inside it or containing it, any case) by that
  id range. Other roots are not touched, and their folder totals stay valid
  because aggregation only updates the rows of the current scan.
* Root rows store the `scan_id` that produced them, so the UI shows the date
  of each drive's own last analysis.

## Phases

1. **Enumerate + metadata.** One `FindFirstFileExW` loop per directory gives
   name, size, times, attributes and reparse tag. Paths use the `\\?\` prefix,
   so paths longer than `MAX_PATH` work.
2. **Persist.** Workers send rows (in directory batches) over a bounded
   channel (back-pressure) to a single writer that commits every 50 000 rows or
   750 ms. Secondary indexes are dropped during the bulk insert and rebuilt at
   the end.
3. **Aggregate.** Ids grow from parent to child, so one reverse pass over this
   scan's rows (an array indexed by `id - first id`) computes every folder's
   total size, file count and newest/oldest modification time.
4. **Index + ANALYZE**, then the scan row is marked `done` or `cancelled`.

Classification, duplicate detection, backup and image detection are computed
from the index afterwards, on demand.

## Progress, cancel, pause

Progress (files, folders, bytes, unreadable folders, current folder, elapsed
time) is emitted every ≈150 ms. There is no percentage during enumeration:
the total is unknown and DiskNido does not invent one. Cancel stops the
workers, keeps what was written (partial results) and marks the scan
`cancelled`; roots it had not started keep their previous index. Pause blocks workers between directories.

## Concurrency

Thread counts depend on the impact profile and on the medium of the drive
(`IncursSeekPenalty` from `IOCTL_STORAGE_QUERY_PROPERTY`). HDDs get few
readers because parallel reads become head seeks. Roots are walked one after
another so two letters on the same physical disk do not compete.

## Special cases

| Case | Behaviour |
| --- | --- |
| Long paths | `\\?\` verbatim paths in every native call |
| Unicode names | UTF-16 → UTF-8 (lossless for valid names) |
| Junctions / symlinks / mount points | recorded, size 0, not followed |
| Hard links | counted once per path in totals (see LIMITATIONS); collapsed in duplicates |
| Sparse / NTFS-compressed files | logical size is reported (allocated size not yet) |
| Cloud-only files | recorded, never opened |
| Files changing during the scan | the listing reflects the moment of listing; later features re-check |
| Disconnected drive | the walk ends with errors recorded; partial results are kept |
| Exclusions | case-insensitive, component-based prefix match; excluded folders are not walked |

## Schema

See [DATA_MODEL.md](DATA_MODEL.md).
