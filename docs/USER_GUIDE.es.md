# Guía de usuario de DiskNido

DiskNido te ayuda a entender qué ocupa tus discos, encontrar duplicados
exactos, imágenes de disco y respaldos antiguos, organizarlos por fecha y
comprimirlos con verificación. **Analizar nunca modifica archivos.** Todo lo
que mueve, comprime o envía a la Papelera necesita tu selección y tu
confirmación, y se registra en el Historial.

## 1. Inicio

Muestra cada unidad: letra, nombre, sistema de archivos, capacidad, espacio
libre, si es SSD/HDD, si es externa y su número de disco físico (dos letras
pueden estar en el mismo disco). Debajo aparece el resumen del último análisis
con su fecha: son datos guardados, no una medición en tiempo real.

## 2. Analizar

1. Elige el tipo:
   * **Rápido**: tamaños, fechas y tipos.
   * **Completo**: lo anterior y, al terminar, la búsqueda de duplicados exactos.
   * **Personalizado**: carpetas concretas.
2. Marca las unidades (las de red nunca se incluyen solas).
3. Pulsa **Iniciar análisis**.

Verás fase, archivos, carpetas, datos, tiempo y carpetas que no se pudieron
leer. Puedes **pausar**, **cancelar** (se conservan los resultados parciales)
o cambiar de pantalla. La intensidad (bajo impacto / equilibrado / máximo) se
cambia en Configuración.

> Cada análisis reemplaza el índice anterior. Si quieres D: y E: a la vez,
> selecciona ambas en el mismo análisis.

## 3. Explorador de espacio

Navega Unidad → Carpeta → Subcarpeta con el **mapa** (rectángulos
proporcionales al tamaño) o la **lista**. Al seleccionar un elemento verás
tamaño, número de archivos, cambio más reciente, ruta completa y tipos
principales, y podrás abrir su ubicación, copiar la ruta, buscar duplicados
dentro, marcarlo como importante o excluirlo de futuros análisis. También hay
distribución por categoría y extensión, y las carpetas y archivos más grandes.
Grande no significa prescindible.

## 4. Limpieza inteligente

Pulsa **Analizar**. Cada categoría muestra descripción, número de archivos,
tamaño, nivel de confianza, riesgo y ubicaciones:

* **Alta confianza**: temporales del usuario de más de 7 días, caché de
  navegadores.
* **Confianza media**: volcados de errores, informes de errores de Windows,
  Papelera.
* **Revisión manual**: instaladores antiguos y descargas grandes y antiguas.
* **Protegido**: componentes de Windows (abre el Sensor de almacenamiento).

Despliega una categoría, marca archivos y envíalos a la Papelera. "Seleccionar
todos" solo existe en categorías de alta confianza. Enviar a la Papelera **no
libera espacio hasta vaciarla**; vaciarla es una acción aparte, irreversible y
con confirmación.

## 5. Duplicados

Pulsa **Buscar duplicados** (elige dónde y el tamaño mínimo). Solo se muestran
archivos con **exactamente el mismo contenido**, comprobado byte a byte. Para
cada grupo verás copias, tamaño, espacio redundante, fechas y ubicaciones.

* Elige qué copia **conservar** (hay una sugerencia, que no garantiza cuál es
  el original).
* Marca las copias a retirar y pulsa **Enviar copias marcadas a la Papelera**.
  Justo antes, DiskNido vuelve a comparar cada copia con la que conservas.
* Las copias dentro de respaldos, de sistema, bases de datos o máquinas
  virtuales están protegidas y no se pueden marcar.
* **Exportar informe CSV** (con opción de ocultar tu carpeta de usuario).

## 6. Imágenes de disco

Lista ISO, BIN/CUE, IMG, NRG, MDF/MDS, CHD, CSO/ZSO, RVZ, WBFS, WUD/WUX, GDI…
Ordena por nombre, formato, tamaño o fecha; filtra por unidad; busca por
nombre; agrupa por carpeta. Solo se indica una posible plataforma cuando hay
evidencia (formato propio de una consola o etiqueta de volumen de un
instalador). Si vas a retirar parte de un conjunto (por ejemplo el .cue sin
su .bin) verás un aviso con los archivos que quedarían huérfanos.

## 7. Respaldos

**Biblioteca**: todos los respaldos detectados con unidad, fecha, origen de la
fecha (nombre, asignada por ti, inferida del contenido), confianza, tamaño y
estado. Filtra por unidad, año o estado; mira la línea de tiempo. En
**Editar fecha** ves por qué se detectó y puedes asignar año y mes, "Sin fecha"
o "No es un respaldo".

**Organizar**: elige el nombre de la carpeta (por defecto `Backups`), la
estructura (Año\Año-Mes o solo Año) y, si quieres, otra carpeta por unidad.
Pulsa **Previsualizar organización**: verás origen → destino, estado y el árbol
resultante. Nada se mueve hasta que confirmes. Cada respaldo se mueve dentro de
su misma unidad, sin combinar ni sobrescribir; se cuenta su contenido antes y
después. Puedes **deshacer** desde el resultado o desde el Historial.

**Comprimir**: elige la carpeta, el destino, el nombre y el nivel. **Revisar**
muestra tamaño, espacio libre y qué parte ya está comprimida. **Crear ZIP
verificado** crea el ZIP, lo vuelve a abrir y compara cada archivo con el
original (BLAKE3). Los originales se conservan siempre. Verás tamaño de
origen, tamaño del ZIP y el ahorro potencial *si* después retiras el original.

## 8. Carpetas antiguas

Carpetas sin modificaciones desde hace 6, 12, 24, 36 meses o un plazo propio,
con tamaño mínimo. Muestra tamaño, último cambio, archivo más antiguo y tipos
principales. Acciones: importante, favorito, ignorar, excluir siempre,
comprimir, buscar duplicados dentro. Windows no registra siempre el "último
uso": se usa la fecha de modificación.

## 9. Historial

Cada operación (organizar, comprimir, Papelera, vaciar Papelera) con fecha,
duración, resultado, elementos, errores, espacio libre antes/después cuando se
pudo medir y si es reversible.

## 10. Configuración

Idioma (español/inglés), tema (sistema/claro/oscuro), intensidad del análisis,
archivos ocultos, carpetas excluidas, tamaño mínimo de duplicados, carpeta y
estructura de respaldos, nivel de compresión, anonimizar informes y borrar el
índice. Sin telemetría.

## Preguntas frecuentes

**¿Puede DiskNido borrar algo sin que yo lo pida?** No. Y nunca borra de forma
permanente: usa la Papelera.

**¿Por qué una copia no se puede marcar?** Está protegida (respaldo, sistema,
base de datos, máquina virtual) o es la copia que conservas.

**¿Por qué no aparece mi respaldo "Copia"?** "Copia" sin fecha es demasiado
genérico; añade una fecha al nombre o búscalo en el Explorador.

**¿Por qué el ZIP no ahorra casi nada?** Fotos, vídeos y comprimidos ya están
comprimidos.
