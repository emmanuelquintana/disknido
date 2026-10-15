import { useLang } from "../i18n";
import { formatBytes } from "../format";

export interface Rect {
  x: number;
  y: number;
  w: number;
  h: number;
}

/** Squarified treemap (Bruls et al.) over values sorted descending, in a w×h box. */
export function squarify(values: number[], w: number, h: number): Rect[] {
  const total = values.reduce((a, b) => a + b, 0);
  const out: Rect[] = new Array(values.length);
  if (total <= 0 || w <= 0 || h <= 0) return values.map(() => ({ x: 0, y: 0, w: 0, h: 0 }));
  const scale = (w * h) / total;
  const areas = values.map((v) => v * scale);
  let box = { x: 0, y: 0, w, h };
  let i = 0;
  const worst = (row: number[], side: number) => {
    const s = row.reduce((a, b) => a + b, 0);
    const max = Math.max(...row);
    const min = Math.min(...row);
    return Math.max((side * side * max) / (s * s), (s * s) / (side * side * min));
  };
  while (i < areas.length) {
    const side = Math.min(box.w, box.h);
    const row = [areas[i]!];
    let j = i + 1;
    while (j < areas.length && worst([...row, areas[j]!], side) <= worst(row, side)) {
      row.push(areas[j]!);
      j++;
    }
    const sum = row.reduce((a, b) => a + b, 0);
    if (box.w >= box.h) {
      const colW = sum / box.h;
      let y = box.y;
      row.forEach((a, k) => {
        const hh = a / colW;
        out[i + k] = { x: box.x, y, w: colW, h: hh };
        y += hh;
      });
      box = { x: box.x + colW, y: box.y, w: box.w - colW, h: box.h };
    } else {
      const rowH = sum / box.w;
      let x = box.x;
      row.forEach((a, k) => {
        const ww = a / rowH;
        out[i + k] = { x, y: box.y, w: ww, h: rowH };
        x += ww;
      });
      box = { x: box.x, y: box.y + rowH, w: box.w, h: box.h - rowH };
    }
    i = j;
  }
  return out;
}

export interface TreemapItem {
  id: number;
  name: string;
  size: number;
  isDir: boolean;
}

const W = 1000;
const H = 340;

export function Treemap({ items, onOpen, label }: { items: TreemapItem[]; onOpen: (it: TreemapItem) => void; label: string }) {
  const lang = useLang();
  const shown = items.filter((i) => i.size > 0).slice(0, 60);
  const rects = squarify(
    shown.map((i) => i.size),
    W,
    H,
  );
  return (
    <div className="treemap" role="group" aria-label={label}>
      {shown.map((it, k) => {
        const r = rects[k]!;
        return (
          <button
            key={it.id}
            className={`tile ${it.isDir ? "dir" : "file"}`}
            style={{ left: `${(r.x / W) * 100}%`, top: `${(r.y / H) * 100}%`, width: `${(r.w / W) * 100}%`, height: `${(r.h / H) * 100}%` }}
            title={`${it.name} — ${formatBytes(it.size, lang)}`}
            aria-label={`${it.name}, ${formatBytes(it.size, lang)}`}
            onClick={() => onOpen(it)}
          >
            {r.w > 70 && r.h > 34 && (
              <>
                <strong>{it.name}</strong>
                <span className="num">{formatBytes(it.size, lang)}</span>
              </>
            )}
          </button>
        );
      })}
    </div>
  );
}
