# Performance

## How to reproduce

```powershell
cargo build --release -p disknido-core --example bench
$b = "target\release\examples\bench.exe"
& $b gen D:\tmp\dn-bench-100k 100000     # synthetic tree, no personal data
& $b run D:\tmp\dn-bench-100k            # add --profile low|balanced|max
```

`gen` writes N files of 0.5–16.5 KB in folders of 200 (≈870 MB for 100 000);
≈5 % are exact copies of earlier files and ≈2 % same-size near-copies (last
byte flipped). `run` scans into a fresh index next to the folder, runs the
duplicate search twice (second run uses the hash cache), times UI queries, and
prints JSON. Peak memory is the process peak working set
(`GetProcessMemoryInfo`).

## Machine

AMD Ryzen 9 7950X (16 cores / 32 threads), 63 GB RAM, Windows 11 Pro
(10.0.26300). HDD: Seagate ST2000DM008 2 TB (SATA, NTFS, drive F:). SSD: XPG
SPECTRIX S40G 256 GB (NVMe, NTFS, drive G:). Release build, Rust 1.99.

## Results (14 Oct 2026)

**Important caveat:** the datasets had just been generated, so much of the
metadata (and file data) was in the Windows file cache. These numbers are
*warm-cache* results; a cold first scan of an HDD will be slower. Repeated
runs varied (e.g. a 100 k HDD scan took 1.5 s in one run and 8.7 s in another
after other disk activity). Treat them as orders of magnitude, not promises.

### Scan (enumeration + metadata + SQLite index + folder totals)

| Dataset | Drive | Profile | Time | Files/s |
| --- | --- | --- | --- | --- |
| 10 000 files | HDD | balanced | 0.30 s | ≈33 000 |
| 10 000 files | SSD | balanced | 0.24 s | ≈41 000 |
| 100 000 files | HDD | balanced | 1.5 s – 8.7 s | ≈11 000 – 66 000 |
| 100 000 files | HDD | low | 1.4 s | ≈69 000 |
| 100 000 files | SSD | balanced | 0.28 s – 0.47 s | ≈213 000 – 354 000 |
| 100 000 files | SSD | max | 0.28 s | ≈353 000 |

Peak working set for 100 000 files: ≈72 MB. Index size: ≈31–35 MB
(≈330 bytes per entry including indexes).

### Duplicate search (100 000 files, 8 070 files in 2 222 verified groups)

| Version | HDD | SSD |
| --- | --- | --- |
| First implementation | 107.9 s | 25.9 s |
| After optimisation (below) | **15.7 s** | **11.3 s** |
| Second run, hash cache warm | 7.6 s | 7.8 s |

The optimisation was driven by this measurement: (1) candidates are read in
path order instead of hash-map order, which on an HDD turns random seeks into
forward reads; (2) files up to 128 KiB are hashed completely by the partial
pass, so they are not read a second time; (3) the hash cache is written in one
transaction per phase instead of one commit per file. Results were identical
before and after (same groups and files), and the duplicate tests still pass.
Most of the remaining time is opening every candidate once (hard-link
identity) and the mandatory byte-by-byte comparison.

### UI queries on the 100 000-entry index

| Query | Time |
| --- | --- |
| Children of a folder (first 500, by size) | < 0.2 ms |
| Largest 50 files | < 1 ms |
| Old folders | < 0.5 ms |
| Name search (LIKE) | ≈8–10 ms |
| Extension distribution | ≈30 ms |

## Not measured yet

* 1 000 000 entries: the generator supports it (`gen <dir> 1000000`, ≈8.7 GB);
  the test machine did not have enough free space on the drives available.
  Memory for aggregation is ≈40 bytes per entry by design.
* Cold-cache runs (would need a reboot or cache flush between runs).
* CPU usage percentage.

## Profiling

Measurements above come from the `bench` example (wall-clock `Instant`
timings around each phase). For deeper profiling on Windows use
[Windows Performance Recorder/Analyzer](https://learn.microsoft.com/windows-hardware/test/wpt/)
(`wpr -start CPU -start DiskIO` … `wpr -stop trace.etl`) or
`cargo install flamegraph` with `--release` builds.

## Resource budget (design)

* The UI never blocks: every engine call runs off the UI thread, progress is
  throttled to ≈8 events/s, lists are paginated (500 rows) and virtualised.
* Bounded channel between walkers and the SQLite writer (back-pressure).
* Hashing streams 1 MiB chunks; memory does not grow with file size.
* Reader threads: 1 on HDD, 1/3/6 on SSD by profile.
