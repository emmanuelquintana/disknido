import { describe, expect, it } from "vitest";
import { squarify } from "../components/Treemap";
import { expectedTree } from "../features/Backups";
import { sortImages } from "../features/Images";
import { baseName, formatBytes, formatDuration } from "../format";
import { translator } from "../i18n";
import { en } from "../i18n/en";
import { es } from "../i18n/es";
import type { DiskImage, PlanItem } from "../types";

describe("format", () => {
  it("uses binary units like Explorer", () => {
    expect(formatBytes(0)).toBe("0 B");
    expect(formatBytes(1024)).toBe("1,00 KB");
    expect(formatBytes(1536, "en")).toBe("1.50 KB");
    expect(formatBytes(5 * 1024 ** 3, "en")).toBe("5.00 GB");
    expect(formatBytes(-2048, "en")).toBe("−2.00 KB");
  });
  it("formats durations and names", () => {
    expect(formatDuration(65_000)).toBe("1:05");
    expect(formatDuration(3_725_000)).toBe("1:02:05");
    expect(baseName("D:\\Backups\\2025\\Backup Enero 2025\\")).toBe("Backup Enero 2025");
  });
});

describe("i18n", () => {
  it("English covers every Spanish key with the same placeholders", () => {
    for (const k of Object.keys(es) as (keyof typeof es)[]) {
      expect(en[k], k).toBeTruthy();
      const vars = (s: string) => (s.match(/\{\w+\}/g) ?? []).sort().join();
      expect(vars(en[k]), k).toBe(vars(es[k]));
    }
  });
  it("interpolates", () => {
    expect(translator("es")("common.files", { n: 3 })).toBe("3 archivos");
    expect(translator("en")("dup.copies", { n: 2 })).toBe("2 copies");
  });
});

describe("treemap", () => {
  it("tiles fill the box without overflow", () => {
    const vals = [500, 300, 120, 50, 20, 10];
    const rects = squarify(vals, 1000, 340);
    const area = rects.reduce((a, r) => a + r.w * r.h, 0);
    expect(area).toBeCloseTo(1000 * 340, 3);
    for (const r of rects) {
      expect(r.x).toBeGreaterThanOrEqual(-1e-6);
      expect(r.y).toBeGreaterThanOrEqual(-1e-6);
      expect(r.x + r.w).toBeLessThanOrEqual(1000 + 1e-6);
      expect(r.y + r.h).toBeLessThanOrEqual(340 + 1e-6);
    }
    rects.forEach((r, i) => expect((r.w * r.h) / (1000 * 340)).toBeCloseTo(vals[i]! / 1000, 5));
  });
  it("handles empty input", () => {
    expect(squarify([], 10, 10)).toEqual([]);
    expect(squarify([0, 0], 10, 10)).toHaveLength(2);
  });
});

describe("backup plan tree", () => {
  const item = (dst: string, status: PlanItem["status"] = "ready"): PlanItem => ({
    src: "x",
    dst,
    volume: dst.slice(0, 2),
    year: 2025,
    month: 1,
    files: 1,
    bytes: 1,
    status,
    message: null,
  });
  it("groups by volume, year and month and skips refused items", () => {
    const tree = expectedTree([
      item("D:\\Backups\\2025\\2025-01\\Backup Enero 2025"),
      item("D:\\Backups\\2025\\2025-02\\Backup Febrero 2025"),
      item("E:\\Backups\\2026\\2026-10\\Backup Octubre 2026"),
      item("D:\\Backups\\2024\\2024-01\\Nope", "refused"),
    ]);
    expect(tree).toBe(
      [
        "D:",
        "  Backups",
        "    2025",
        "      2025-01",
        "        Backup Enero 2025",
        "      2025-02",
        "        Backup Febrero 2025",
        "E:",
        "  Backups",
        "    2026",
        "      2026-10",
        "        Backup Octubre 2026",
      ].join("\n"),
    );
  });
});

describe("disk image sorting", () => {
  const img = (name: string, size: number, format: string, mtime: number) => ({ name, size, format, mtime }) as DiskImage;
  const list = [img("b.iso", 10, "ISO", 3), img("a.chd", 30, "CHD", 1), img("c.bin", 20, "BIN", 2)];
  it("sorts by size descending and by format ascending", () => {
    expect(sortImages(list, "size", -1).map((i) => i.name)).toEqual(["a.chd", "c.bin", "b.iso"]);
    expect(sortImages(list, "format", 1).map((i) => i.format)).toEqual(["BIN", "CHD", "ISO"]);
    expect(sortImages(list, "date", 1).map((i) => i.mtime)).toEqual([1, 2, 3]);
  });
});
