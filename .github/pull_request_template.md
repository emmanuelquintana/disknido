## Qué cambia / What changes

## Por qué / Why

## Seguridad de los archivos / File safety
- [ ] No modifica archivos durante el análisis / Analysis stays read-only
- [ ] Las operaciones que modifican archivos se validan en Rust, se registran y usan la Papelera / Mutations are validated in Rust, journaled and use the Recycle Bin
- [ ] No hay sobrescrituras silenciosas / No silent overwrites

## Pruebas / Tests
- [ ] `cargo test -p disknido-core`
- [ ] `pnpm typecheck && pnpm lint && pnpm test`
- [ ] Textos nuevos en `es.ts` y `en.ts` / New strings in both languages
