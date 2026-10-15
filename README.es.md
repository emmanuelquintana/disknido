<p align="center"><img src="assets/logo.svg" width="96" alt="Logotipo de DiskNido"></p>

<h1 align="center">DiskNido</h1>

<p align="center"><b>Entiende, limpia y organiza tus discos de Windows — sin perder datos.</b><br>
Gratis · código abierto (MIT) · local · sin registro · sin publicidad · sin telemetría</p>

<p align="center"><a href="README.md">Read in English</a></p>

![Inicio](docs/screenshots/03-home.png)

DiskNido es una aplicación de escritorio para Windows 10 y 11 que te ayuda a
recuperar espacio sin perder información. Muestra qué ocupa tus discos,
encuentra duplicados **exactos**, imágenes de disco y respaldos antiguos,
organiza los respaldos por unidad, año y mes, y los comprime en ZIP
**verificados**.

> Estado: **0.1.0 — primera versión funcional.** Todo lo que aparece abajo está
> implementado y probado; consulta las [limitaciones](docs/LIMITATIONS.md).

## Qué hace

| | |
| --- | --- |
| **Ver qué ocupa espacio** | Unidades con capacidad, SSD/HDD y disco físico; análisis paralelo de solo lectura; explorador con mapa y lista; carpetas y archivos más grandes; por categoría y extensión. |
| **Limpieza razonable** | Solo reglas documentadas en ubicaciones conocidas (temporales de más de 7 días, caché de navegadores, volcados e informes de errores, instaladores antiguos, descargas grandes y antiguas, Papelera). Niveles de confianza y riesgos. Los componentes de Windows nunca se tocan (abre el Sensor de almacenamiento). |
| **Duplicados exactos** | Tamaño → enlaces físicos → huella parcial BLAKE3 → BLAKE3 completo → **byte a byte**. Se vuelve a verificar justo antes de enviar una copia a la Papelera. Las copias dentro de respaldos están protegidas. |
| **Imágenes de disco** | ISO, BIN/CUE, MDF/MDS, CHD, RVZ, WBFS, WUD/WUX, CSO, GDI… Conjuntos CUE/GDI, etiquetas ISO 9660 y avisos de archivos huérfanos. |
| **Respaldos por unidad, año y mes** | Detecta `Backup Enero 2025`, `BACKUP_2024_08`, `bk-02-2025`, `Respaldo Laptop 2023-11`…; marca fechas ambiguas; plan previo con el árbol resultante; movimientos atómicos dentro de la unidad; verificación; deshacer. |
| **Catálogo de comprimidos** | Cada ZIP verificado guarda una lista buscable de su contenido y su ubicación; también puedes añadir ZIP existentes. Búsqueda sin distinguir mayúsculas ni acentos. |
| **Compresión verificada** | ZIP (Deflate/ZIP64) que se vuelve a abrir y se compara archivo por archivo con los originales (BLAKE3). Los originales siempre se conservan. Cuentas de espacio honestas. |

| Duplicados | Plan de respaldos | ZIP verificado |
| --- | --- | --- |
| ![](docs/screenshots/02-duplicates.png) | ![](docs/screenshots/08-backups-plan.png) | ![](docs/screenshots/09-compress-done.png) |

| Explorador | Imágenes de disco | Tema oscuro |
| --- | --- | --- |
| ![](docs/screenshots/04-explorer-map.png) | ![](docs/screenshots/06-images.png) | ![](docs/screenshots/13-duplicates-dark.png) |

Las capturas son reales: la aplicación funcionando sobre datos sintéticos de
demostración (`bench demo`).

## Principios de seguridad

1. **Analizar nunca modifica archivos.**
2. **Una recomendación no es una autorización.** Cada cambio requiere tu
   selección y una confirmación, y se vuelve a validar en Rust.
3. **Nada se sobrescribe y nada se borra de forma permanente.** Retirar
   archivos los envía a la Papelera; vaciarla es una acción aparte y confirmada.
4. **Resultados exactos.** Los duplicados son idénticos byte a byte; las fechas
   ambiguas no se adivinan; grande o antiguo no significa prescindible.
5. **Privado.** Todo se procesa en tu equipo; nada se envía a ningún sitio.

Detalles: [SAFETY](docs/SAFETY.md) · [PRIVACY](docs/PRIVACY.md).

## Instalación

Requisitos: Windows 10 (1809+) u 11, x64, WebView2 (incluido en Windows 11).
No necesitas herramientas de desarrollo.

Descarga `DiskNido-<versión>-windows-x64-setup.exe` desde
[Releases](https://github.com/emmanuelquintana/disknido/releases) cuando haya
una versión publicada, compárala con `SHA256SUMS.txt` y ejecútala (instalación
por usuario, sin administrador). Los instaladores **aún no están firmados**:
SmartScreen puede avisar → *Más información → Ejecutar de todas formas*.

Mientras no haya una versión publicada, puedes compilarla tú (ver abajo).

## Cómo empezar

1. **Analizar** → elige *Completo*, marca unidades → *Iniciar análisis*.
2. **Explorador de espacio** para navegar; **Duplicados**, **Imágenes de
   disco**, **Respaldos** y **Carpetas antiguas** para revisar lo encontrado.
3. **Respaldos → Organizar** → *Previsualizar organización* → confirmar. Se
   puede deshacer desde el Historial.
4. **Respaldos → Comprimir** → *Revisar* → *Crear ZIP verificado*.

Manual completo: [Español](docs/USER_GUIDE.es.md) · [English](docs/USER_GUIDE.en.md).

## Preguntas frecuentes

**¿DiskNido puede borrar algo por su cuenta?** No. Y nunca borra de forma
permanente: usa la Papelera.
**¿Envía algo a Internet?** No hace ninguna conexión de red.
**¿Por qué no puedo marcar una copia duplicada?** Está protegida (respaldo,
sistema, base de datos, disco de máquina virtual) o es la copia que conservas.
**¿Por qué el ZIP ahorró poco?** Fotos, vídeos y comprimidos ya están
comprimidos; DiskNido te lo avisa antes de empezar.

## Desarrollo

```powershell
pnpm install
pnpm tauri dev
cargo test -p disknido-core
pnpm typecheck; pnpm lint; pnpm test
pnpm tauri build          # instaladores en target/release/bundle
```

Stack: Rust (estable) · Tauri 2 · React 19 + TypeScript (estricto) · Vite ·
SQLite (rusqlite, incluido) · BLAKE3 · zip · windows-sys. La documentación
técnica está en inglés en [`docs/`](docs/) para facilitar la colaboración
internacional.

## Contribuir

Las contribuciones son bienvenidas: [CONTRIBUTING.md](CONTRIBUTING.md) y
[Código de conducta](CODE_OF_CONDUCT.md). Seguridad: [SECURITY.md](SECURITY.md).

## Hoja de ruta

[ROADMAP.md](docs/ROADMAP.md): tamaño asignado en disco, varios índices,
movimientos verificados entre unidades, ARM64, análisis programados, firma y
actualizaciones.

## Advertencia

DiskNido se ofrece "tal cual", sin garantía (ver [LICENSE](LICENSE)). Está
diseñado para evitar pérdidas de datos, pero tú decides qué mover, comprimir o
enviar a la Papelera. Mantén copias de seguridad independientes de lo
importante.

## Licencia

[MIT](LICENSE) © colaboradores de DiskNido.
