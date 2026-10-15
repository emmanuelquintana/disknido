# Building installers

```powershell
pnpm install
pnpm tauri build
```

Outputs (x64), under `<target>/release/bundle/`:

* `nsis/DiskNido_<version>_x64-setup.exe` — per-user installer (no admin).
* `msi/DiskNido_<version>_x64_es-ES.msi` and `_en-US.msi` — MSI packages.

The release workflow renames them to `DiskNido-<version>-windows-x64-setup.exe`
and `DiskNido-<version>-windows-x64-<lang>.msi` and writes `SHA256SUMS.txt`.

## WebView2

`bundle.windows.webviewInstallMode = downloadBootstrapper`: if the WebView2
Runtime is missing (rare on Windows 10, never on Windows 11), the installer
downloads Microsoft's bootstrapper. For fully offline installation, switch to
`offlineInstaller` (adds ≈130 MB) or `embedBootstrapper`.

## Requirements on the user's PC

Windows 10 1809+ or Windows 11, x64, WebView2 Runtime. No Rust, Node or
developer tools.

## Code signing

Builds are **not signed**. SmartScreen may show "Windows protected your PC" →
"More info" → "Run anyway". DiskNido does not try to bypass SmartScreen.
Signing can be added with `bundle.windows.certificateThumbprint` or a signing
command once a certificate is available.

## ARM64

Not built yet. `rustup target add aarch64-pc-windows-msvc` and
`pnpm tauri build --target aarch64-pc-windows-msvc` should work but are
untested.
