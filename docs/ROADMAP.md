# Roadmap

Priority is reliability of the six core jobs. Items below are documented, not
implemented, and there are no empty modules for them.

## Next
* Allocated-size accounting (`GetCompressedFileSizeW`) and hard-link-aware
  folder totals.
* Keep several indexes (one per volume) instead of one.
* Cross-volume backup moves: copy → verify (BLAKE3) → confirm → recycle source.
* ARM64 build and test.
* Settings: start with Windows (off by default), notifications.

## Later
* Scheduled scans and low-space notifications.
* Historical comparison of usage.
* Incremental analysis via the NTFS change journal (USN).
* Abandoned project detection; media library analysis.
* Advanced NAS support; snapshots.
* Native deduplication / hard-link replacement — only with strong guarantees.
* Advanced report export; custom rules; public CLI; plugins.
* Code signing and verified automatic updates.
