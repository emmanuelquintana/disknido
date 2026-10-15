# DiskNido user guide

DiskNido shows what fills your drives, finds exact duplicates, disk images and
old backups, organises backups by date and compresses them with verification.
**Analysing never modifies files.** Anything that moves, compresses or recycles
needs your selection and confirmation, and is recorded in History.

## 1. Home
Each drive with letter, label, file system, capacity, free space, SSD/HDD,
external flag and physical disk number (two letters may share one disk).
Below: the last analysis summary, labelled with its date — stored data, not a
live measurement.

## 2. Analyze
Pick **Quick** (sizes, dates, types), **Full** (plus exact duplicate search
when it finishes) or **Custom** (specific folders), select drives (network
drives are never included automatically) and start. You see phase, files,
folders, data, time and unreadable folders; you can pause, cancel (partial
results are kept) or switch screens. An analysis updates only the drives or
folders it covers: analyse D: today and E: tomorrow and both stay in the
index, each with its own "last analysis" date.

## 3. Space explorer
Browse drive → folder → file as a **map** (areas proportional to size) or a
**list**. Properties show size, file count, newest change, full path and main
types; open the location, copy the path, find duplicates inside, mark as
important or exclude from future analyses. Distribution by category and
extension, largest folders and files. Large does not mean disposable.

## 4. Smart cleanup
**Analyze** lists categories with description, files, size, confidence, risk
and locations: *High* (user temp older than 7 days, browser caches), *Medium*
(crash dumps, Windows error reports, Recycle Bin), *Manual review* (old
installers, large old downloads), *Protected* (Windows components → opens
Storage Sense). Select files and send them to the Recycle Bin. "Select all"
exists only for high-confidence categories. The bin keeps using space until
emptied; emptying is a separate, irreversible, confirmed action.

## 5. Duplicates
**Find duplicates** (choose where and minimum size). Only files with exactly
the same content, checked byte by byte. Choose the copy to **keep** (a
suggestion is shown — not proof of originality), mark copies to remove, and
send them to the Recycle Bin; each one is compared with the kept copy again
right before. Copies inside backups, system locations, databases or virtual
machines are protected. Export a CSV report (optionally hiding your user
folder).

## 6. Disk images
ISO, BIN/CUE, IMG, NRG, MDF/MDS, CHD, CSO/ZSO, RVZ, WBFS, WUD/WUX, GDI… Sort,
filter by drive, search, group by folder. A platform is suggested only with
evidence (console-specific format, OS installer volume label). Removing part of
a set (e.g. a .cue without its .bin) shows which files would be orphaned.

## 7. Backups
**Library**: detected backups with drive, date, date source (name / set by you
/ inferred from contents), confidence, size and state; filters and timeline.
**Edit date** shows why it was detected and lets you set year/month, "No date"
or "Not a backup". **Organize**: folder name (default `Backups`), layout
(Year\Year-Month or Year), optional folder per drive; **Preview** shows
source → destination, status and the resulting tree. Moves stay within each
drive, never merge or overwrite, and contents are counted before and after.
Undo is available. **Compress**: choose folder, destination, name and level;
**Review** shows size, free space and how much is already compressed; **Create
verified ZIP** writes, reopens and compares every file (BLAKE3). Originals are
always kept; you see source size, ZIP size and the potential saving *if* you
later remove the original.

## 8. Old folders
Folders unchanged for 6/12/24/36 months or a custom period, with a minimum
size; mark important, favorite, ignore, always exclude, compress, or find
duplicates inside. Windows does not reliably record "last used", so the newest
modification is used.

## 9. History
Every operation with date, duration, result, items, errors, free space before
and after when measurable, and whether it can be undone.

## 10. Settings
Language, theme, analysis intensity, hidden files, exclusions, duplicate
minimum size, backup folder and layout, compression level, report
anonymisation, clear index. No telemetry.
