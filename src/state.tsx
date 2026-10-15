// App-wide state: settings, navigation, background tasks and toasts.
import { listen } from "@tauri-apps/api/event";
import { createContext, useCallback, useContext, useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { api } from "./api";
import { I18nContext, translator, type Lang } from "./i18n";
import type { Profile, TaskDoneEvent, TaskKind, TaskProgressEvent } from "./types";

export type Section =
  | "home"
  | "scan"
  | "explorer"
  | "cleanup"
  | "duplicates"
  | "images"
  | "backups"
  | "oldFolders"
  | "catalog"
  | "history"
  | "settings";

export interface Settings {
  lang: Lang;
  theme: "system" | "light" | "dark";
  profile: Profile;
  includeHidden: boolean;
  exclusions: string[];
  dupMinSize: number;
  rootName: string;
  layout: "yearMonth" | "year";
  backupParents: Record<string, string>;
  compressionLevel: number;
  anonymizeReports: boolean;
  showInstalledApps: boolean;
  settingsVersion: number;
}

export const defaultSettings: Settings = {
  lang: "es",
  theme: "system",
  profile: "balanced",
  includeHidden: true,
  exclusions: [],
  dupMinSize: 1024 * 1024,
  rootName: "Backups",
  layout: "yearMonth",
  backupParents: {},
  compressionLevel: 6,
  anonymizeReports: true,
  showInstalledApps: false,
  settingsVersion: 2,
};

/** Older saved settings get the new defaults where the old default was unsafe for big drives. */
export function migrateSettings(saved: Partial<Settings>): Settings {
  const s = { ...defaultSettings, ...saved };
  if ((saved.settingsVersion ?? 1) < 2) {
    // v1 defaulted to 1 KB: millions of tiny candidates made the search look frozen.
    if ((saved.dupMinSize ?? 1024) <= 1024) s.dupMinSize = defaultSettings.dupMinSize;
    s.settingsVersion = 2;
  }
  return s;
}

export interface TaskState {
  id: string;
  kind: TaskKind;
  progress: unknown;
  done: TaskDoneEvent | null;
  paused: boolean;
}

interface AppCtx {
  settings: Settings;
  updateSettings: (patch: Partial<Settings>) => void;
  section: Section;
  params: Record<string, string>;
  navigate: (s: Section, params?: Record<string, string>) => void;
  tasks: Record<string, TaskState>;
  /** The most recent task of a kind (running or finished). */
  latest: (kind: TaskKind) => TaskState | undefined;
  trackTask: (id: string, kind: TaskKind) => void;
  toast: (msg: string) => void;
  /** Increments whenever the index changes (scan finished, files moved). */
  indexVersion: number;
  bumpIndex: () => void;
}

const Ctx = createContext<AppCtx | null>(null);

export function useApp(): AppCtx {
  const c = useContext(Ctx);
  if (!c) throw new Error("useApp outside AppProvider");
  return c;
}

function systemDark(): boolean {
  return typeof window !== "undefined" && !!window.matchMedia?.("(prefers-color-scheme: dark)").matches;
}

export function AppProvider({ children, initialSettings }: { children: ReactNode; initialSettings?: Settings }) {
  const [settings, setSettings] = useState<Settings>(initialSettings ?? defaultSettings);
  const [section, setSection] = useState<Section>("home");
  const [params, setParams] = useState<Record<string, string>>({});
  const [tasks, setTasks] = useState<Record<string, TaskState>>({});
  const [toasts, setToasts] = useState<{ id: number; msg: string }[]>([]);
  const [indexVersion, setIndexVersion] = useState(0);
  const loaded = useRef(!!initialSettings);

  useEffect(() => {
    if (loaded.current) return;
    api
      .getSettings()
      .then((s) => {
        if (s && typeof s === "object") setSettings(migrateSettings(s as Partial<Settings>));
      })
      .catch(() => {})
      .finally(() => {
        loaded.current = true;
      });
  }, []);

  // Theme: explicit choice or follow the system, live.
  useEffect(() => {
    const apply = () => {
      const dark = settings.theme === "dark" || (settings.theme === "system" && systemDark());
      document.documentElement.dataset.theme = dark ? "dark" : "light";
    };
    apply();
    const mq = window.matchMedia?.("(prefers-color-scheme: dark)");
    mq?.addEventListener?.("change", apply);
    return () => mq?.removeEventListener?.("change", apply);
  }, [settings.theme]);

  useEffect(() => {
    document.documentElement.lang = settings.lang;
  }, [settings.lang]);

  const updateSettings = useCallback((patch: Partial<Settings>) => {
    setSettings((prev) => {
      const next = { ...prev, ...patch };
      api.setSettings(next).catch(() => {});
      return next;
    });
  }, []);

  const bumpIndex = useCallback(() => setIndexVersion((v) => v + 1), []);

  useEffect(() => {
    const unsubs: Promise<() => void>[] = [];
    unsubs.push(
      listen<TaskProgressEvent>("task://progress", (e) => {
        const p = e.payload;
        setTasks((t) => ({
          ...t,
          [p.taskId]: { ...(t[p.taskId] ?? { id: p.taskId, kind: p.kind, done: null, paused: false }), progress: p.data },
        }));
      }),
    );
    unsubs.push(
      listen<TaskDoneEvent>("task://done", (e) => {
        const d = e.payload;
        setTasks((t) => ({
          ...t,
          [d.taskId]: { ...(t[d.taskId] ?? { id: d.taskId, kind: d.kind, progress: null, paused: false }), done: d },
        }));
        if (d.kind === "scan" || d.kind === "organize" || d.kind === "duplicates") setIndexVersion((v) => v + 1);
      }),
    );
    return () => {
      unsubs.forEach((u) => u.then((f) => f()).catch(() => {}));
    };
  }, []);

  const trackTask = useCallback((id: string, kind: TaskKind) => {
    setTasks((t) => ({ ...t, [id]: t[id] ?? { id, kind, progress: null, done: null, paused: false } }));
  }, []);

  const latest = useCallback(
    (kind: TaskKind) =>
      Object.values(tasks)
        .filter((x) => x.kind === kind)
        .sort((a, b) => Number(b.id.split("-").pop()) - Number(a.id.split("-").pop()))[0],
    [tasks],
  );

  const toast = useCallback((msg: string) => {
    const id = Date.now() + Math.random();
    setToasts((ts) => [...ts, { id, msg }]);
    setTimeout(() => setToasts((ts) => ts.filter((x) => x.id !== id)), 4000);
  }, []);

  const navigate = useCallback((s: Section, p: Record<string, string> = {}) => {
    setSection(s);
    setParams(p);
  }, []);

  const value = useMemo<AppCtx>(
    () => ({ settings, updateSettings, section, params, navigate, tasks, latest, trackTask, toast, indexVersion, bumpIndex }),
    [settings, updateSettings, section, params, navigate, tasks, latest, trackTask, toast, indexVersion, bumpIndex],
  );
  const i18n = useMemo(() => ({ lang: settings.lang, t: translator(settings.lang) }), [settings.lang]);

  return (
    <I18nContext.Provider value={i18n}>
      <Ctx.Provider value={value}>
        {children}
        <div className="toast-region" role="status" aria-live="polite">
          {toasts.map((t) => (
            <div key={t.id} className="toast">
              {t.msg}
            </div>
          ))}
        </div>
      </Ctx.Provider>
    </I18nContext.Provider>
  );
}

export function isRunning(t: TaskState | undefined): boolean {
  return !!t && !t.done;
}
