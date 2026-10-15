import { open } from "@tauri-apps/plugin-dialog";
import { FolderPlus, HardDrive, Pause, Play, X } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { api, errorText } from "../api";
import { Badge, Meter, Notice, PageHeader, PathText, Stat, useAsync } from "../components/ui";
import { formatBytes, formatDuration, formatNumber } from "../format";
import { useLang, useT, type Key } from "../i18n";
import { isRunning, useApp, type TaskState } from "../state";
import type { ScanProgress, ScanRequest } from "../types";

type Mode = ScanRequest["mode"];

// Scans started in "full" mode chain a duplicate search when they finish.
const fullScans = new Map<string, string[]>();

/** Mounted once in the shell so the chained search starts even if the user navigated away. */
export function AutoDuplicates() {
  const { tasks, settings, trackTask } = useApp();
  useEffect(() => {
    for (const [id, roots] of fullScans) {
      const task = tasks[id];
      if (task?.done) {
        fullScans.delete(id);
        if (task.done.ok) {
          api
            .startDuplicates({ minSize: settings.dupMinSize, scopes: roots, profile: settings.profile })
            .then((dupId) => trackTask(dupId, "duplicates"))
            .catch(() => {});
        }
      }
    }
  }, [tasks, settings.dupMinSize, settings.profile, trackTask]);
  return null;
}

export function ScanProgressView({ task }: { task: TaskState }) {
  const t = useT();
  const lang = useLang();
  const { navigate } = useApp();
  const p = (task.done?.result as ScanProgress | null) ?? (task.progress as ScanProgress | null);
  const [paused, setPaused] = useState(false);
  const [errors, setErrors] = useState<{ path: string; message: string }[] | null>(null);
  const done = task.done;
  const phase: Key = done ? (done.cancelled || (done.result as { status?: string })?.status === "cancelled" ? "scan.phase.cancelled" : "scan.phase.done") : p ? (`scan.phase.${p.phase}` as Key) : "scan.phase.enumerating";
  return (
    <div className="card" aria-live="polite">
      <div className="row-between">
        <div>
          <h2 className="section-title" style={{ marginBottom: 4 }}>
            {paused && !done ? t("scan.paused") : t(phase)}
          </h2>
          {p?.root && <span className="caption">{p.root}</span>}
        </div>
        {!done && (
          <div className="actions">
            <button
              className="btn"
              onClick={() => {
                api.pauseTask(task.id, !paused);
                setPaused(!paused);
              }}
            >
              {paused ? <Play size={16} aria-hidden /> : <Pause size={16} aria-hidden />} {paused ? t("common.resume") : t("common.pause")}
            </button>
            <button className="btn" onClick={() => api.cancelTask(task.id)}>
              <X size={16} aria-hidden /> {t("common.cancel")}
            </button>
          </div>
        )}
      </div>
      {!done && (
        <div style={{ margin: "16px 0" }}>
          <Meter label={t(phase)} />
        </div>
      )}
      <div className="grid grid-stats" style={{ marginTop: 16 }}>
        <Stat value={formatNumber(p?.files ?? 0, lang)} label={t("scan.filesProcessed")} />
        <Stat value={formatNumber(p?.dirs ?? 0, lang)} label={t("scan.foldersProcessed")} />
        <Stat value={formatBytes(p?.bytes ?? 0, lang)} label={t("scan.dataProcessed")} />
        <Stat value={formatDuration(p?.elapsedMs ?? 0)} label={t("scan.elapsed")} />
        <Stat value={formatNumber(p?.errors ?? 0, lang)} label={t("scan.errors")} />
      </div>
      {!done && p?.current && (
        <div style={{ marginTop: 16 }}>
          <span className="caption">{t("scan.current")}</span>
          <PathText path={p.current} />
        </div>
      )}
      {!done && <p className="caption">{t("scan.canLeave")}</p>}
      {done && (
        <div className="stack" style={{ marginTop: 16 }}>
          {done.error && <Notice tone="danger">{done.error}</Notice>}
          {phase === "scan.phase.cancelled" && <Notice tone="warning">{t("scan.cancelledNote")}</Notice>}
          <div className="actions">
            <button className="btn btn-primary" onClick={() => navigate("explorer")}>
              {t("scan.viewResults")}
            </button>
            {(p?.errors ?? 0) > 0 && p && (
              <button className="btn" onClick={() => api.scanErrors(p.scanId).then(setErrors)}>
                {t("scan.seeErrors")}
              </button>
            )}
          </div>
          {errors && (
            <div>
              <h3 className="section-title">{t("scan.errorsTitle")}</h3>
              <p className="caption">{t("scan.errorsNote")}</p>
              <ul style={{ maxHeight: 220, overflow: "auto", paddingLeft: 18 }}>
                {errors.map((e) => (
                  <li key={e.path}>
                    <PathText path={e.path} />
                    <span className="caption">{e.message}</span>
                  </li>
                ))}
              </ul>
            </div>
          )}
        </div>
      )}
    </div>
  );
}

export function ScanPage() {
  const t = useT();
  const lang = useLang();
  const { settings, params, trackTask, latest, toast } = useApp();
  const vols = useAsync(() => api.listVolumes(), []);
  const [mode, setMode] = useState<Mode>("full");
  const [drives, setDrives] = useState<string[]>(params.drive ? [params.drive] : []);
  const [folders, setFolders] = useState<string[]>(params.folder ? [params.folder] : []);
  const [error, setError] = useState<string | null>(null);
  const task = latest("scan");
  const running = isRunning(task);
  const startedHere = useRef(false);

  const roots = mode === "custom" ? folders : drives;
  const start = async () => {
    setError(null);
    try {
      const id = await api.startScan({
        roots,
        mode,
        profile: settings.profile,
        exclusions: settings.exclusions,
        includeHidden: settings.includeHidden,
      });
      if (mode !== "quick") fullScans.set(id, roots);
      trackTask(id, "scan");
      startedHere.current = true;
    } catch (e) {
      setError(errorText(e));
    }
  };

  const addFolder = async () => {
    const picked = await open({ directory: true, multiple: true });
    const list = Array.isArray(picked) ? picked : picked ? [picked] : [];
    setFolders((f) => Array.from(new Set([...f, ...list])));
  };

  const modes: { id: Mode; title: Key; desc: Key }[] = [
    { id: "quick", title: "scan.mode.quick", desc: "scan.mode.quickDesc" },
    { id: "full", title: "scan.mode.full", desc: "scan.mode.fullDesc" },
    { id: "custom", title: "scan.mode.custom", desc: "scan.mode.customDesc" },
  ];

  return (
    <>
      <PageHeader title={t("scan.title")} subtitle={t("scan.subtitle")} />
      {task && (running || task.done) && (
        <section className="section" style={{ marginBottom: 24 }}>
          <ScanProgressView task={task} />
        </section>
      )}
      {!running && (
        <>
          <section className="section">
            <h2 className="section-title">{t("scan.mode")}</h2>
            <div className="grid grid-stats" role="radiogroup" aria-label={t("scan.mode")}>
              {modes.map((m) => (
                <button key={m.id} className="choice" role="radio" aria-checked={mode === m.id} aria-pressed={mode === m.id} onClick={() => setMode(m.id)}>
                  <div>
                    <strong>{t(m.title)}</strong>
                    <span className="caption">{t(m.desc)}</span>
                  </div>
                </button>
              ))}
            </div>
          </section>

          <section className="section" style={{ marginTop: 24 }}>
            {mode !== "custom" ? (
              <>
                <h2 className="section-title">{t("scan.drives")}</h2>
                <div className="grid grid-drives">
                  {(vols.data ?? [])
                    .filter((v) => v.kind !== "optical")
                    .map((v) => (
                      <label key={v.root} className="choice">
                        <input
                          type="checkbox"
                          className="sr-only"
                          checked={drives.includes(v.root)}
                          disabled={!v.available}
                          onChange={(e) => setDrives((d) => (e.target.checked ? [...d, v.root] : d.filter((x) => x !== v.root)))}
                        />
                        <HardDrive size={20} aria-hidden />
                        <div style={{ flex: 1 }}>
                          <div className="row-between">
                            <strong>
                              {v.letter} {v.label}
                            </strong>
                            <span className="row">
                              {v.media !== "unknown" && <Badge>{t(v.media === "ssd" ? "scan.badge.ssd" : "scan.badge.hdd")}</Badge>}
                              {v.external && <Badge tone="accent">{t("home.external")}</Badge>}
                              {v.kind === "network" && <Badge tone="warning">{t("home.network")}</Badge>}
                            </span>
                          </div>
                          <span className="caption num">
                            {t("home.freeOf", { free: formatBytes(v.freeBytes, lang), total: formatBytes(v.totalBytes, lang) })}
                          </span>
                        </div>
                      </label>
                    ))}
                </div>
                <p className="caption">{t("scan.networkNote")}</p>
              </>
            ) : (
              <>
                <div className="row-between">
                  <h2 className="section-title">{t("scan.folders")}</h2>
                  <button className="btn" onClick={addFolder}>
                    <FolderPlus size={16} aria-hidden /> {t("scan.addFolder")}
                  </button>
                </div>
                <div className="card">
                  {folders.length === 0 && <p className="muted">{t("scan.noSelection")}</p>}
                  {folders.map((f) => (
                    <div key={f} className="row-between" style={{ padding: "6px 0" }}>
                      <PathText path={f} />
                      <button className="btn btn-sm btn-subtle" onClick={() => setFolders((x) => x.filter((y) => y !== f))}>
                        {t("scan.remove")}
                      </button>
                    </div>
                  ))}
                </div>
              </>
            )}
          </section>

          {error && (
            <div style={{ marginTop: 16 }}>
              <Notice tone="danger">{error}</Notice>
            </div>
          )}
          <div className="actions" style={{ marginTop: 24 }}>
            <button className="btn btn-primary" disabled={roots.length === 0} onClick={() => start().catch((e) => toast(errorText(e)))}>
              {t("scan.start")}
            </button>
            {roots.length === 0 && <span className="caption">{t("scan.noSelection")}</span>}
          </div>
        </>
      )}
    </>
  );
}
