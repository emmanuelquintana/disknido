# Contributing to DiskNido

¡Gracias! Contributions in Spanish or English are welcome.

## Before you start

* Read [docs/SAFETY.md](docs/SAFETY.md). DiskNido handles personal files; safety
  rules are not negotiable, even when they cost lines of code.
* For larger changes, open an issue first to agree on the approach.

## Setup

See [docs/DEVELOPMENT.md](docs/DEVELOPMENT.md).

## Rules for changes

1. **Analysis stays read-only.**
2. Every new mutating operation must be validated in Rust (`safety.rs`),
   journaled (`ops.rs`), never overwrite, prefer the Recycle Bin, and have an
   integration test with fixtures from `crates/disknido-core/tests/common`.
   Never test against real user folders or a whole drive.
3. Keep it simple: no abstractions with a single implementation, no new
   dependency for what a few lines can do. Explain any new dependency in the PR.
4. User-visible text goes to `src/i18n/es.ts` **and** `src/i18n/en.ts`.
   Use plain language ("No se pudo leer este archivo", not "Read I/O error").
5. Accessibility: keyboard reachable, visible focus, labels, no meaning by
   colour alone, respect reduced motion.

## Checks (CI runs the same)

```powershell
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test -p disknido-core
pnpm typecheck; pnpm lint; pnpm test
```

## Commits

Conventional commits: `feat(scanner): …`, `fix(ui): …`, `docs: …`, `test: …`.

## Reporting bugs

Use the issue template. Remove personal paths and never attach your
`index.sqlite` (it contains file names). See
[docs/TROUBLESHOOTING.md](docs/TROUBLESHOOTING.md#reporting-a-bug).
