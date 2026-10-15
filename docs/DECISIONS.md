# Decision log

Format: decision — why — consequences.

### D1. Tauri 2 + Rust engine + React UI
Small native binary with WebView2; the engine is Rust so walking and hashing
never run in JavaScript. React only renders paginated/aggregated data.

### D2. Engine as a separate crate (`disknido-core`)
Testable and benchmarkable without building the UI; keeps the command layer
thin. No other layers (no services, no traits with one implementation).

### D3. `FindFirstFileExW` instead of `walkdir`/`jwalk`/`ignore`
One call per directory returns everything we need **including the reparse
tag**, which is required to walk OneDrive placeholder folders while never
following junctions/symlinks. A ≈60-line LIFO work-stack walker gives parallel,
cache-friendly traversal without a dependency. (`jwalk`/`rayon` were evaluated
and not needed.)

### D4. `windows-sys` instead of `windows`
Raw bindings compile much faster and we call only a handful of functions.

### D5. Ids assigned during the walk; one reverse aggregation pass
Children always get larger ids than their parents, so totals need no recursion.
Ceiling: ≈40 bytes RAM per entry during aggregation (≈40 MB per million).

### D6. BLAKE3 + mandatory byte-by-byte verification
BLAKE3 is fast and collision-resistant, but the requirement is "really
identical", so groups are confirmed byte by byte and re-checked again before
any copy is removed. The partial fingerprint is only a filter.

### D7. Recycle Bin via `SHFileOperationW` (`FOF_ALLOWUNDO | FOF_WANTNUKEWARNING`)
`IFileOperation` would need COM and the heavier `windows` crate. The legacy
API does exactly what we need; `FOF_WANTNUKEWARNING` prevents silent permanent
deletion. Limitation: no `\\?\` paths (MAX_PATH) — such items are refused with
an explanation.

### D8. ZIP (pure-Rust `zip` crate, Deflate via `zlib-rs`) as the only format
Opens natively on Windows; no external 7-Zip; MIT licensed; no C toolchain.
7z/tar.zst add little for the goal and would not open natively on Windows 10.

### D9. Same-volume organisation only (v0.1)
Native atomic rename, no copy+delete. Cross-volume verified copy is on the
roadmap.

### D10. No router, no state library, no component library, no chart library
Ten screens, one context, a 70-line squarified treemap, a 25-line virtual
list. Dependencies: React, Tauri API/plugins, lucide-react (icons).

### D11. Spanish source strings, typed keys
`es.ts` is the source; `en.ts` must have every key (`Record<Key, string>`),
and a test checks placeholders match.

### D12. No telemetry, no network
Nothing to configure, nothing to leak.

### D13. Skills
* **apple-design** (github.com/dickwu/apple-design-skill): read `SKILL.md`;
  applied its principles (one accent colour, hierarchy by type weight, quiet
  surfaces, motion optional and reduced-motion aware, colour never the only
  carrier of meaning) adapted to Windows conventions (Segoe UI Variable,
  left navigation, native dialogs). Not vendored (no licence file).
* **ponytail** (github.com/afnadsherief/ponytail, MIT): read the `ponytail` and
  `ponytail-review` skills and applied the ladder by hand. Its plugin was not
  installed because it registers hooks/MCP from a very new repository; the
  review was done manually (see PROGRESS.md). Safety checks, error handling,
  tests and accessibility were kept regardless of line count.
