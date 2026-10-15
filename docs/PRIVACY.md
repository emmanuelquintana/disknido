# Privacy

* **Local only.** All analysis happens on your PC. DiskNido makes no network
  connections: no telemetry, no analytics, no crash reporting, no update
  checks, no online metadata lookups. The Content Security Policy of the app
  window does not allow remote connections.
* **What is stored.** File and folder names, sizes, dates and attributes,
  BLAKE3 fingerprints of duplicate candidates, your marks and settings, and
  the history of operations — in `%LOCALAPPDATA%\io.github.disknido\` on your
  disk. File contents are never stored.
* **Deleting your data.** Settings → Privacy → "Borrar índice de análisis"
  clears scans, duplicates and the hash cache. Uninstalling and deleting that
  folder removes everything.
* **Reports.** The duplicate CSV export can replace your user folder with
  `%USERPROFILE%` (on by default). Review exported files before sharing them.
* **Logs.** DiskNido does not write log files. If you attach information to a
  bug report, remove personal paths first (see TROUBLESHOOTING).
* **Installer.** The NSIS/MSI installer may download Microsoft's WebView2
  runtime bootstrapper if it is missing (Windows 11 already includes it). That
  is a Microsoft component; DiskNido itself sends nothing.
