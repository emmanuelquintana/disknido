# Development setup

## Requirements (Windows 10/11 x64)

| Tool | Version used | Notes |
| --- | --- | --- |
| Rust | stable 1.99 (MSRV 1.88, edition 2024) | `rustup` with the MSVC host |
| Visual Studio Build Tools | 2022 or 2026 with "Desktop development with C++" | MSVC linker + Windows SDK |
| Node.js | 24 LTS | |
| pnpm | 10.x | `corepack enable` or `npm i -g pnpm` |
| WebView2 Runtime | any evergreen | included in Windows 11 |
| Git | any | |

## First run

```powershell
git clone https://github.com/emmanuelquintana/disknido
cd disknido
pnpm install
pnpm tauri dev
```

`.npmrc` sets `node-linker=hoisted` (plain `node_modules`, no symlinks), which
avoids issues with sandboxed/virtualised folders on Windows.

### Low disk space?

Rust toolchains and Tauri builds need several GB. You can keep them on another
drive:

```powershell
$env:RUSTUP_HOME = "F:\dev\rust\rustup"
$env:CARGO_HOME  = "F:\dev\rust\cargo"
$env:CARGO_TARGET_DIR = "F:\dev\disknido-target"
```

## Useful commands

```powershell
cargo test -p disknido-core          # engine tests (fast, no UI build)
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all
pnpm typecheck; pnpm lint; pnpm test
pnpm tauri dev                       # app with hot reload
cargo run --release -p disknido-core --example bench -- demo D:\tmp\disknido-demo
```

`bench demo <dir>` creates a small synthetic tree (backups with dated names,
duplicates, an ISO with a real ISO 9660 label, a CUE/BIN set, an old project)
that exercises every screen. Never point the app at real data while
developing destructive features.

## Code map

* Engine: `crates/disknido-core/src/*.rs` — one file per module, documented at
  the top. Start with `lib.rs`.
* Commands: `src-tauri/src/lib.rs`.
* UI: `src/features/<Screen>.tsx`, shared bits in `src/components/ui.tsx`,
  state in `src/state.tsx`, strings in `src/i18n/es.ts` (add the same key to
  `en.ts`; TypeScript enforces it).

## Conventions

* User-visible text: Spanish source in `es.ts`, English in `en.ts`. Engine
  error messages are Spanish user-facing sentences.
* Any new mutating feature must: validate in Rust (`safety.rs`), journal in
  `ops.rs`, prefer the Recycle Bin, never overwrite, and come with an
  integration test that uses fixtures from `tests/common` (never real folders).
