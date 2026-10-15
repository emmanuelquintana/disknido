# Releasing

1. Update the version in `Cargo.toml` (`[workspace.package]`),
   `package.json` and `src-tauri/tauri.conf.json` (SemVer).
2. Update `CHANGELOG.md`.
3. Make sure CI is green on `main`.
4. Tag and push:

   ```powershell
   git tag v0.1.0
   git push origin v0.1.0
   ```

5. `.github/workflows/release.yml` builds on `windows-latest`, runs the tests,
   builds the NSIS and MSI installers, renames them
   (`DiskNido-<version>-windows-x64-*`), computes `SHA256SUMS.txt` and creates
   a **draft** GitHub Release with the changelog section and the notes from
   `.github/release-notes.md`.
6. Download the artifacts from the draft, install on a clean Windows VM, run
   the smoke checklist below, then publish the draft.

## Smoke checklist

* Installs without admin (NSIS) and starts.
* Home lists drives with correct capacity.
* Scan of a test folder completes; cancel works.
* Duplicates found only for identical files.
* Organise a test backup and undo it.
* Compress a test folder; ZIP opens in Explorer.
* Uninstall leaves user files untouched.
