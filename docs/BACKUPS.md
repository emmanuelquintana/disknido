# Backups: detection, dates and organisation

Source: `crates/disknido-core/src/backups.rs`, `organize.rs`.
Tests: unit tests in `backups.rs`, integration tests in
`crates/disknido-core/tests/backups_organize.rs`.

Flow: **detect → review → classify → preview → organise → verify → (optional)
compress.** You can stop after any step.

## Detection rules

Names are lower-cased, accents folded and split into alphabetic and numeric
tokens (`BACKUP_2024_08` → `backup 2024 08`, `bk02` → `bk 02`). Keywords must
be whole tokens, so `Bakery 2024` or `Backgrounds` never match.

| Rule | Example | Result |
| --- | --- | --- |
| Strong keyword: backup(s), respaldo(s), bkp, bk, bak, bck, "copia(s) de seguridad" | `Respaldo Laptop 2023-11` | backup |
| Weak keyword (copia(s), archive(s), archivado, histórico(s), historical, "mis archivos") **plus** any date part | `Copia PC Junio 2023` | backup |
| Weak keyword without a date | `Copia`, `copia de fotos` | **not** a backup |
| Name made only of keywords | `Backups`, `Respaldos` | a *container*, not a backup |
| Dated folder inside a container | `D:\Respaldos\2023-11` | backup |
| `<root>\YYYY\YYYY-MM\<name>` | `D:\Backups\2025\2025-01\Backup Enero 2025` | backup, already organised |

Only the top-most match of each subtree is listed, so the contents of a backup
never appear as further backups.

## Dates

Month names in Spanish and English with abbreviations (enero/ene/january/jan,
septiembre/setiembre/sep/sept/set, diciembre/dic/december/dec, …) and these
numeric forms:

| Form | Example |
| --- | --- |
| `YYYY-MM`, `YYYY_MM`, `YYYY.MM`, `YYYY MM` | `2025-01` |
| `MM-YYYY` | `01-2024`, `bk-02-2025` |
| `YYYYMM`, `YYYYMMDD` | `202501`, `20250115` |
| `YYYY-MM-DD` | `2024-03-15` |
| Month name + year | `Enero 2025`, `Jan 2025` |
| Year only | `Respaldo 2026` (month unknown) |
| Month only | `Copia Marzo` (year unknown → review) |

**Ambiguity is never guessed.** `Backup 03-04` (no year) and `03-04-2025`
(day/month order unknown) are flagged "Fecha ambigua" and need review.
`25-04-2025` is unambiguous (25 cannot be a month). Two different years or
months in one name are also flagged.

**Confidence:** *High* = strong keyword + explicit year and month.
*Medium* = date partially known or weak keyword. *Review* = ambiguous or no year.

When a name has no date, the newest file modification inside is offered as a
**labelled** fallback ("Fecha inferida del contenido"). It never overrides a
date found in the name, and items whose date is inferred are **not
preselected** for organisation. You can set year and month, choose
"Sin fecha", or mark "No es un respaldo"; these overrides are stored per path.

## Organisation plan (dry run)

Each backup moves **inside its own volume** to
`<parent>\<root>\<YYYY>\<YYYY-MM>\<original name>` — `<root>` defaults to
`Backups` and `<parent>` to the drive root; both are configurable per volume
(the parent must be on the same volume). A "year only" layout is available.

The preview lists, per item: source, destination, files, bytes, status and the
reason for anything not ready, plus a tree of the expected result.

| Status | Meaning |
| --- | --- |
| Ready | will move |
| Name adjusted | destination taken → `Name (2)`; nothing is merged or overwritten |
| Already organised | source equals destination (re-running is a no-op) |
| Needs date | no year/month and not "Sin fecha" |
| Cannot be moved | protected, inside another item of the plan, destination inside the source, junction on the path, unreadable… |

## Execution

For each ready item, again at execution time:

1. Re-validate the source (exists, not protected, no junctions, readable).
2. Refuse if the destination appeared after the preview.
3. Count files and bytes; **refuse if they differ from the preview**.
4. Create the parent folders and call `MoveFileExW` **without**
   `MOVEFILE_REPLACE_EXISTING` and **without** `MOVEFILE_COPY_ALLOWED`: an
   atomic rename on the same volume that can neither overwrite nor silently
   become a copy+delete.
5. Count files and bytes at the destination and compare.

Every step is journaled (`operations`, `operation_items`). An organisation
can be undone from History or right after it runs: each folder is renamed back
if its original location is still free.

Cross-volume moves are not offered in this version: the organiser works per
volume, as required. See ROADMAP for verified copy-then-remove.

## Verified properties (tests)

* Files are not lost and contents are byte-identical after moving.
* Existing folders are never overwritten or merged.
* Ambiguous dates are flagged for review and keep no guessed year.
* Two backups of the same month stay separate.
* A destination inside its own source is refused.
* A second run does not reorganise already organised backups.
* Content changed after the preview aborts that item.
* Interrupted moves are reconciled from disk after a crash.
