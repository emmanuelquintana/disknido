import {
  Archive,
  Copy,
  Disc3,
  FolderClock,
  History as HistoryIcon,
  Library,
  House,
  LayoutGrid,
  ScanSearch,
  Settings as SettingsIcon,
  Sparkles,
} from "lucide-react";
import type { ReactNode } from "react";
import { Meter } from "./components/ui";
import { BackupsPage } from "./features/Backups";
import { CatalogPage } from "./features/Catalog";
import { CleanupPage } from "./features/Cleanup";
import { DuplicatesPage } from "./features/Duplicates";
import { ExplorerPage } from "./features/Explorer";
import { HistoryPage } from "./features/History";
import { HomePage } from "./features/Home";
import { ImagesPage } from "./features/Images";
import { OldFoldersPage } from "./features/OldFolders";
import { AutoDuplicates, ScanPage } from "./features/Scan";
import { SettingsPage } from "./features/Settings";
import { useT, type Key } from "./i18n";
import { useApp, type Section } from "./state";

const NAV: { id: Section; key: Key; icon: ReactNode }[] = [
  { id: "home", key: "nav.home", icon: <House size={18} /> },
  { id: "scan", key: "nav.scan", icon: <ScanSearch size={18} /> },
  { id: "explorer", key: "nav.explorer", icon: <LayoutGrid size={18} /> },
  { id: "cleanup", key: "nav.cleanup", icon: <Sparkles size={18} /> },
  { id: "duplicates", key: "nav.duplicates", icon: <Copy size={18} /> },
  { id: "images", key: "nav.images", icon: <Disc3 size={18} /> },
  { id: "backups", key: "nav.backups", icon: <Archive size={18} /> },
  { id: "oldFolders", key: "nav.oldFolders", icon: <FolderClock size={18} /> },
  { id: "catalog", key: "nav.catalog", icon: <Library size={18} /> },
  { id: "history", key: "nav.history", icon: <HistoryIcon size={18} /> },
];

const PAGES: Record<Section, () => ReactNode> = {
  home: () => <HomePage />,
  scan: () => <ScanPage />,
  explorer: () => <ExplorerPage />,
  cleanup: () => <CleanupPage />,
  duplicates: () => <DuplicatesPage />,
  images: () => <ImagesPage />,
  backups: () => <BackupsPage />,
  oldFolders: () => <OldFoldersPage />,
  catalog: () => <CatalogPage />,
  history: () => <HistoryPage />,
  settings: () => <SettingsPage />,
};

function RunningTask() {
  const t = useT();
  const { tasks, navigate } = useApp();
  const running = Object.values(tasks).find((x) => !x.done);
  if (!running) return null;
  const target: Section = running.kind === "scan" ? "scan" : running.kind === "duplicates" ? "duplicates" : "backups";
  const p = running.progress as { bytesDone?: number; bytesTotal?: number } | null;
  const frac = p?.bytesTotal ? (p.bytesDone ?? 0) / p.bytesTotal : null;
  const name = t(`task.${running.kind}` as Key);
  return (
    <button className="task-chip" onClick={() => navigate(target)} aria-live="polite">
      <span className="label">{t("common.taskRunning", { task: name })}</span>
      <div style={{ marginTop: 6 }}>
        <Meter value={frac} label={name} />
      </div>
    </button>
  );
}

export function App() {
  const t = useT();
  const { section, navigate } = useApp();
  return (
    <div className="shell">
      <AutoDuplicates />
      <nav className="sidebar" aria-label={t("nav.main")}>
        <div className="brand">
          <img src="/logo.svg" alt="" />
          <span>{t("app.name")}</span>
        </div>
        {NAV.map((n) => (
          <button
            key={n.id}
            className="nav-item"
            aria-current={section === n.id ? "page" : undefined}
            onClick={() => navigate(n.id)}
            title={t(n.key)}
          >
            {n.icon}
            <span className="label">{t(n.key)}</span>
          </button>
        ))}
        <div className="nav-spacer" />
        <RunningTask />
        <button
          className="nav-item"
          aria-current={section === "settings" ? "page" : undefined}
          onClick={() => navigate("settings")}
          title={t("nav.settings")}
        >
          <SettingsIcon size={18} />
          <span className="label">{t("nav.settings")}</span>
        </button>
      </nav>
      <main className="main" id="main">
        <div className="page" key={section}>
          {PAGES[section]()}
        </div>
      </main>
    </div>
  );
}
