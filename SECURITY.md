# Security policy

## Supported versions

Only the latest released version receives fixes.

## Reporting a vulnerability

Please **do not open a public issue**. Use GitHub's private reporting:
*Security → Report a vulnerability* on
<https://github.com/emmanuelquintana/disknido/security/advisories/new>.

Include the version, Windows version, steps to reproduce and the impact
(for example: a way to make DiskNido modify, move or delete a file it should
refuse to touch). Do not include real personal files or your `index.sqlite`.

We aim to acknowledge reports within 7 days.

## Scope highlights

* Path validation bypasses (traversal, junctions/symlinks, protected
  locations, stale results).
* Any path where an analysis writes to user files.
* Commands reachable from the WebView beyond those listed in
  `src-tauri/capabilities/default.json`.

DiskNido makes no network connections; reports about telemetry or data
exfiltration are taken especially seriously.
