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

## Real drives (15 Oct 2026)

Measured with `bench scan` / `bench queries` on the developer's own PC
(read-only; the index was written to another drive). Same machine as above.

| Drive | Medium | Files | Data | Scan (alone) |
| --- | --- | --- | --- | --- |
| G: | NVMe SSD | 126 949 | 245 GB | 0.7 s |
| D: | HDD (WD 1 TB) | 1 248 567 | 987 GB | 145 s (first, cold run) |
| E: | HDD (Seagate 2 TB) | 533 831 | 1.7 TB | 46 s |
| C: | NVMe SSD | 3 533 986 | 1.4 TB | 61 s walk, 97 s total |

**All four drives in one analysis (5.4 M files, 6.2 M entries):** walking
63 s, folder totals 12 s, index build 28 s → 102 s. Before this change the
drives were walked one after another (sum of the walks ≈ 250 s); now each
physical disk has its own walker. The second run had a warmer cache, so part
of the gain is cache; the structural gain is that the total is bounded by the
slowest disk instead of the sum.

UI queries on that 6.2 M-entry index:

| Query | Before | After |
| --- | --- | --- |
| Extension / category distribution (Home, Explorer) | 25.8 s each | 2 ms (totals written while scanning) |
| Largest files / folders | 0.4 s / 1.4 s | same, now off the UI thread |
| Backups / disk images / old folders (Home summary) | 3.6 s / 0.7 s / 0.7 s | same, loaded separately after Home renders |
| Name search | 1.2 s | same, off the UI thread |

The 26-second distribution query ran while holding the single shared
database connection, which froze every other screen. Now each screen query
runs on its own connection in the background, and the distribution comes
from `ext_stats`.

### Duplicate search on the real drives (min 1 MB)

47 849 candidates (same size as another file), 31 257 of them with an equal
partial fingerprint, **285 GB** to hash in full. Result: 11 229 groups,
31 096 files, 153 GB redundant, 216 files skipped (unreadable or changed).

| Run | Partial fingerprints | Full hashes | Byte verification | Total |
| --- | --- | --- | --- | --- |
| First search, old pipeline | ~12 min | ~40 min | would re-read 272 GB (stopped) | > 1 h |
| Repeat search, new pipeline (full hashes cached) | ~7 min | cached | at removal time | **7.8 min** |
| Repeat search again (fingerprints cached, OS file cache warm) | cached | cached | at removal time | **4.6 s** |

The partial phase is bound by HDD seeks (≈70–190 files/s with one reader per
HDD). Repeat searches now also reuse cached partial fingerprints (same path,
size, modification time and file identity), so only the open is paid.

### In the release app on the real drives

Driving the v0.2.0 release window over the WebView2 DevTools protocol:

| Check | Result |
| --- | --- |
| Screen switch while scanning all four drives | avg 16 ms, max 75 ms |
| Screen switch while the duplicate search hashes | ≈ 90 ms |
| Home summary (first numbers / folder findings) | 26 ms / 4.4 s (loads after the page) |
| Cancel a running duplicate search | 0.4 s |
| Old folders (36 months, ≥ 1 GB) | 3.9 s; 52 shown, 27 installed apps/games hidden |
| Disk images listed | 3 403 (was 61 162 before the `.bin`/`.img` rule) |
| Explorer: open C: (3.5 M files) | 0.24 s |
| Catalog search | 0.8 s |

**Caveat for every real-drive number on this page:** during all of these
measurements a stuck v0.1 instance (the one the user reported) was still
running a 1 KB duplicate search in the background and reading D: until it was
closed at the end. Real numbers without that contention should be equal or
better.

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
