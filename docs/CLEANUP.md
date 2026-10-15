# Smart cleanup rules

Source: `crates/disknido-core/src/cleanup.rs`.

Principles:

* Only known locations. Extensions alone (`.tmp`, `.log`, `.bak`) are never
  evidence: a `.bak` can be a critical backup, a `.log` an audit trail.
* Analysis is read-only. Nothing is preselected except where noted.
* Removal = Recycle Bin. When you send files, Rust re-checks that **each path
  still belongs to the rule** (location, age, size, type) and refuses
  anything else, including paths the UI did not get from the rule.
* Files in use cannot be moved by Windows; they are skipped and reported.
* "Select all" exists only in *High confidence* categories.

| Rule | Location | Criterion | Confidence | Risk / note |
| --- | --- | --- | --- | --- |
| `userTemp` | `%TEMP%` (recursive) | unchanged ≥ 7 days | High | Documented temporary area. In-use files are skipped by Windows. |
| `browserCache` | Chrome / Edge / Brave `User Data\<profile>\{Cache\Cache_Data, Code Cache, GPUCache}`, Firefox `Profiles\*\cache2` | any | High | Regenerated while browsing. Close the browser first. |
| `crashDumps` | `%LOCALAPPDATA%\CrashDumps` | any | Medium | Useful when troubleshooting with support. |
| `errorReports` | `%LOCALAPPDATA%\Microsoft\Windows\WER\ReportArchive`, `ReportQueue` | any | Medium | Reports can no longer be sent. |
| `oldInstallers` | `%USERPROFILE%\Downloads` (top level) | `.exe .msi .msix .msixbundle .appx`, unchanged ≥ 90 days | Manual review | Some versions may not be downloadable again. |
| `oldLargeDownloads` | `%USERPROFILE%\Downloads` (top level) | ≥ 500 MB, unchanged ≥ 180 days, not installers | Manual review | Recommendation only. |
| `recycleBin` | Recycle Bin (all drives) | size via `SHQueryRecycleBinW` | Medium | "Empty" is separate, irreversible and confirmed. |
| `windowsComponents` | Windows, WinSxS, updates, restore points | — | Protected | Never touched. Button opens Windows Storage Sense. |

## Never done

* Deleting from Windows, WinSxS, System32, Windows Installer, recovery or
  restore folders, Program Files or ProgramData.
* Registry "cleaning" or "optimisation".
* Removing DLLs, drivers or files of running applications.
* Changing ACLs or bypassing permissions.

## Space accounting

The result of a cleanup shows separately: potentially recoverable,
sent to the Recycle Bin, free space measured before/after on the drive, and
what stays occupied until the bin is emptied.
