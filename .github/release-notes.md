## DiskNido

Analiza, limpia y organiza tus discos de Windows de forma segura. Todo se procesa localmente.

**Descargas**: `DiskNido-<versión>-windows-x64-setup.exe` (recomendado, sin administrador) o los `.msi`.
Verifica la descarga con `SHA256SUMS.txt`:

```powershell
Get-FileHash .\DiskNido-*-windows-x64-setup.exe -Algorithm SHA256
```

**Requisitos**: Windows 10 (1809+) u 11, x64, WebView2 (incluido en Windows 11).

**Seguridad**: analizar nunca modifica archivos; eliminar siempre va a la Papelera; comprimir conserva los originales y verifica el ZIP.

**Aviso**: los instaladores no están firmados digitalmente; SmartScreen puede mostrar una advertencia ("Más información" → "Ejecutar de todas formas").

**Limitaciones**: ver `docs/LIMITATIONS.md`. Cambios: ver `CHANGELOG.md`.
