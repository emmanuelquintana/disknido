import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { App } from "../App";
import { CategoryCard } from "../features/Cleanup";
import { GroupCard } from "../features/Duplicates";
import { HomePage } from "../features/Home";
import { ScanPage } from "../features/Scan";
import type { CleanupCategory, DupGroup, Overview, Volume } from "../types";
import { mockCommands, renderApp } from "./utils";

const vol = (letter: string, extra: Partial<Volume> = {}): Volume => ({
  root: `${letter}:\\`,
  letter: `${letter}:`,
  label: `Disco ${letter}`,
  fileSystem: "NTFS",
  kind: "fixed",
  media: "ssd",
  external: false,
  totalBytes: 1000 * 1024 ** 3,
  freeBytes: 250 * 1024 ** 3,
  volumeId: null,
  diskNumber: 0,
  available: true,
  ...extra,
});

const emptyOverview: Overview = {
  lastScan: null,
  roots: [],
  categories: [],
  extensions: [],
  duplicateGroups: 0,
  duplicateBytes: 0,
  backups: 0,
  backupBytes: 0,
  images: 0,
  imageBytes: 0,
  oldFolders: 0,
  oldFolderBytes: 0,
  recycleBin: { items: 0, bytes: 0 },
};

describe("shell", () => {
  it("has labelled navigation and marks the current page", async () => {
    mockCommands({ list_volumes: [vol("C")], overview: emptyOverview });
    renderApp(<App />);
    const nav = screen.getByRole("navigation", { name: "Navegación principal" });
    expect(within(nav).getByRole("button", { name: /Inicio/ })).toHaveAttribute("aria-current", "page");
    await userEvent.click(within(nav).getByRole("button", { name: /Duplicados/ }));
    expect(within(nav).getByRole("button", { name: /Duplicados/ })).toHaveAttribute("aria-current", "page");
  });
});

describe("home", () => {
  it("shows drives with usage and an empty state before any analysis", async () => {
    mockCommands({ list_volumes: [vol("D"), vol("E", { external: true, media: "hdd" })], overview: emptyOverview });
    renderApp(<HomePage />);
    expect(await screen.findByText("D:")).toBeInTheDocument();
    expect(screen.getByText("Externa")).toBeInTheDocument();
    expect(screen.getByText("HDD")).toBeInTheDocument();
    expect(screen.getAllByRole("progressbar")[0]).toHaveAttribute("aria-valuenow", "75");
    expect(await screen.findByText("Empieza con un análisis")).toBeInTheDocument();
  });
});

describe("scan", () => {
  it("requires a selection and sends the chosen drives", async () => {
    const calls: unknown[] = [];
    mockCommands({
      list_volumes: [vol("D"), vol("E")],
      start_scan: (a: Record<string, unknown>) => {
        calls.push(a.request);
        return "scan-1";
      },
    });
    renderApp(<ScanPage />);
    const start = screen.getByRole("button", { name: "Iniciar análisis" });
    expect(start).toBeDisabled();
    await userEvent.click(await screen.findByText(/D: Disco D/));
    await userEvent.click(screen.getByText(/E: Disco E/));
    expect(start).toBeEnabled();
    await userEvent.click(start);
    await waitFor(() => expect(calls).toHaveLength(1));
    expect(calls[0]).toMatchObject({ roots: ["D:\\", "E:\\"], mode: "full", profile: "balanced" });
  });
});

describe("duplicates", () => {
  const group: DupGroup = {
    id: 7,
    size: 1024,
    hash: "ab",
    verified: true,
    runAt: 0,
    redundantBytes: 2048,
    suggestedKeep: 0,
    suggestionReason: "Ruta más corta",
    files: [
      { path: "D:\\Fotos\\a.jpg", mtime: 1, protectedReason: null },
      { path: "E:\\Backups\\2024\\a.jpg", mtime: 1, protectedReason: "Parte del respaldo «2024»" },
      { path: "C:\\Users\\x\\Downloads\\copia.jpg", mtime: 1, protectedReason: null },
    ],
  };

  it("never lets the kept or a protected copy be marked, and confirms before recycling", async () => {
    const recycled: unknown[] = [];
    mockCommands({
      recycle_duplicates: (a: Record<string, unknown>) => {
        recycled.push(a);
        return { opId: 1, status: "done", recycled: 1, failed: 0, bytesSentToBin: 1024, freeBefore: null, freeAfter: null, messages: [] };
      },
    });
    renderApp(<GroupCard g={group} onDone={() => {}} />);
    expect(screen.getByText("Duplicado verificado")).toBeInTheDocument();
    const boxes = screen.getAllByRole("checkbox");
    expect(boxes[0]).toBeDisabled(); // kept copy
    expect(boxes[1]).toBeDisabled(); // protected backup copy
    const send = screen.getByRole("button", { name: /Enviar copias marcadas/ });
    expect(send).toBeDisabled();
    await userEvent.click(boxes[2]!);
    await userEvent.click(send);
    expect(recycled).toHaveLength(0); // dialog first
    const dlg = screen.getByRole("dialog");
    expect(within(dlg).getByText(/byte a byte/)).toBeInTheDocument();
    await userEvent.click(within(dlg).getByRole("button", { name: /Enviar copias marcadas/ }));
    await waitFor(() => expect(recycled).toHaveLength(1));
    expect(recycled[0]).toEqual({ groupId: 7, keep: "D:\\Fotos\\a.jpg", remove: ["C:\\Users\\x\\Downloads\\copia.jpg"] });
  });
});

describe("cleanup", () => {
  const cat = (confidence: CleanupCategory["confidence"], id = "userTemp"): CleanupCategory => ({
    id,
    confidence,
    action: "recycle",
    locations: ["C:\\Users\\x\\AppData\\Local\\Temp"],
    files: [
      { path: "C:\\Users\\x\\AppData\\Local\\Temp\\a.tmp", size: 10, mtime: 1 },
      { path: "C:\\Users\\x\\AppData\\Local\\Temp\\b.tmp", size: 20, mtime: 1 },
    ],
    fileCount: 2,
    bytes: 30,
    infoBytes: null,
  });

  it("offers select-all only for high-confidence categories", async () => {
    mockCommands({});
    const { unmount } = renderApp(<CategoryCard c={cat("high")} onChanged={() => {}} />);
    await userEvent.click(screen.getByRole("button", { name: /Mostrar/ }));
    expect(screen.getByRole("button", { name: /Seleccionar todos/ })).toBeInTheDocument();
    unmount();
    renderApp(<CategoryCard c={cat("manual", "oldInstallers")} onChanged={() => {}} />);
    await userEvent.click(screen.getByRole("button", { name: /Mostrar/ }));
    expect(screen.queryByRole("button", { name: /Seleccionar todos/ })).not.toBeInTheDocument();
  });

  it("empties the Recycle Bin only after an explicit danger confirmation", async () => {
    let emptied = 0;
    mockCommands({
      empty_recycle_bin: () => {
        emptied++;
        return { items: 0, bytes: 0 };
      },
    });
    renderApp(<CategoryCard c={{ ...cat("medium", "recycleBin"), action: "emptyBin", files: [] }} onChanged={() => {}} />);
    await userEvent.click(screen.getByRole("button", { name: "Vaciar Papelera…" }));
    const dlg = screen.getByRole("dialog");
    expect(within(dlg).getByText(/No podrás restaurarlos/)).toBeInTheDocument();
    await userEvent.click(within(dlg).getByRole("button", { name: "Cancelar" }));
    expect(emptied).toBe(0);
  });
});
