# Troubleshooting

**"Windows protected your PC" when installing.** The installer is not signed
yet. Click "More info" → "Run anyway", or build it yourself.

**The app window is blank.** Install or repair the Microsoft Edge WebView2
Runtime.

**"Hay un análisis en curso".** Screens that read the index wait until the
running analysis finishes or is cancelled.

**Some folders "could not be read".** They are protected by Windows or by
permissions (e.g. `System Volume Information`). DiskNido never forces access or
asks for administrator rights. The list is under *Analizar → Ver carpetas no
leídas*.

**A file was skipped: "El archivo cambió desde el análisis".** It was modified
after the analysis. Run the analysis (or the duplicate search) again.

**"La ruta pasa por un enlace o unión".** The path goes through a junction or
symbolic link. For safety DiskNido does not modify files reached that way;
use the real path.

**"La ruta es demasiado larga para la Papelera de Windows".** The Windows
shell API used for the Recycle Bin is limited to 260 characters. Move or
rename the folder first.

**OneDrive files are listed as "Solo en la nube".** They are not on your disk;
DiskNido does not download them, hash them or move them.

**A compression failed.** Nothing was deleted; the partial archive was
removed. The reason is in *Historial*.

**Reset everything.** Settings → Privacy → "Borrar índice de análisis". To
remove all app data, delete `%LOCALAPPDATA%\io.github.disknido`.

## Reporting a bug

Open an issue with the bug template. Include the Windows version, the DiskNido
version (Settings → About), steps to reproduce, and the error text. **Before
attaching anything, replace personal paths** — for example
`C:\Users\<you>\…` → `%USERPROFILE%\…` — and do not attach your
`index.sqlite`, which contains file names.
