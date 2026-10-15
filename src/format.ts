import type { Lang } from "./i18n";

const UNITS = ["B", "KB", "MB", "GB", "TB", "PB"];

/** Binary units with the labels Windows Explorer uses (1 KB = 1024 B). */
export function formatBytes(n: number, lang: Lang = "es"): string {
  if (!Number.isFinite(n)) return "—";
  const neg = n < 0;
  let v = Math.abs(n);
  let i = 0;
  while (v >= 1024 && i < UNITS.length - 1) {
    v /= 1024;
    i++;
  }
  const digits = i === 0 ? 0 : v < 10 ? 2 : v < 100 ? 1 : 0;
  const s = v.toLocaleString(lang === "es" ? "es-ES" : "en-US", {
    minimumFractionDigits: digits,
    maximumFractionDigits: digits,
  });
  return `${neg ? "−" : ""}${s} ${UNITS[i]}`;
}

export function formatNumber(n: number, lang: Lang = "es"): string {
  return n.toLocaleString(lang === "es" ? "es-ES" : "en-US");
}

/** Unix seconds → local date. */
export function formatDate(ts: number | null | undefined, lang: Lang = "es", withTime = false): string {
  if (!ts) return "—";
  return new Date(ts * 1000).toLocaleString(lang === "es" ? "es-ES" : "en-US", {
    year: "numeric",
    month: "short",
    day: "numeric",
    ...(withTime ? { hour: "2-digit", minute: "2-digit" } : {}),
  });
}

export function formatDuration(ms: number): string {
  const s = Math.floor(ms / 1000);
  const h = Math.floor(s / 3600);
  const m = Math.floor((s % 3600) / 60);
  const sec = s % 60;
  const pad = (x: number) => String(x).padStart(2, "0");
  return h > 0 ? `${h}:${pad(m)}:${pad(sec)}` : `${m}:${pad(sec)}`;
}

export function formatPercent(x: number, lang: Lang = "es"): string {
  return (x * 100).toLocaleString(lang === "es" ? "es-ES" : "en-US", { maximumFractionDigits: 0 }) + " %";
}

export function monthName(m: number, lang: Lang = "es"): string {
  return new Date(2000, m - 1, 1).toLocaleString(lang === "es" ? "es-ES" : "en-US", { month: "long" });
}

/** Last path component, tolerant of trailing separators. */
export function baseName(p: string): string {
  const parts = p.replace(/[\\/]+$/, "").split(/[\\/]/);
  return parts[parts.length - 1] || p;
}

export function dirName(p: string): string {
  const i = Math.max(p.lastIndexOf("\\"), p.lastIndexOf("/"));
  return i > 0 ? p.slice(0, i) : p;
}
