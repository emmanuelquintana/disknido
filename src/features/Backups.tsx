import { open } from "@tauri-apps/plugin-dialog";
import { Archive, ArrowRight, ShieldCheck } from "lucide-react";
import { useEffect, useMemo, useRef, useState } from "react";
import { api, errorText } from "../api";
import { Badge, ConfirmDialog, EmptyState, Meter, Notice, PageHeader, PathActions, PathText, Stat, useAsync, type Tone } from "../components/ui";
import { baseName, formatBytes, formatDate, formatNumber, monthName } from "../format";
import { useLang, useT, type Key } from "../i18n";
import { isRunning, useApp } from "../state";
import type { BackupItem, BatchProgress, BatchResult, CompressPreview, CompressRequest, CompressResult, ExecResult, Plan, PlanItem } from "../types";

type Tab = "library" | "organize" | "compress";
const CONF_TONE: Record<BackupItem["confidence"], Tone> = { high: "success", medium: "accent", review: "warning" };

function DateEditor({ b, onSaved }: { b: BackupItem; onSaved: () => void }) {
  const t = useT();
  const lang = useLang();
  const { toast } = useApp();
  const [year, setYear] = useState(b.year?.toString() ?? "");
  const [month, setMonth] = useState(b.month?.toString() ?? "");
  const save = (patch: { noDate?: boolean; excluded?: boolean }) => {
    const y = Number(year);
    api
      .backupOverride(b.path, {
        year: patch.noDate || !year ? null : y,
        month: patch.noDate || !month ? null : Number(month),
        noDate: !!patch.noDate,
        excluded: !!patch.excluded,
      })
      .then(() => {
        toast(t("backups.saved"));
        onSaved();
      }, (e) => toast(errorText(e)));
  };
  return (
    <div className="row" style={{ flexWrap: "wrap", gap: 8, alignItems: "flex-end" }}>
      <label className="field">
        <span>{t("backups.year")}</span>
        <input className="input num" style={{ width: 90 }} inputMode="numeric" value={year} onChange={(e) => setYear(e.target.value.replace(/\D/g, "").slice(0, 4))} />
      </label>
      <label className="field">
        <span>{t("backups.month")}</span>
        <select className="select" value={month} onChange={(e) => setMonth(e.target.value)}>
          <option value="">—</option>
          {Array.from({ length: 12 }, (_, i) => (
            <option key={i + 1} value={i + 1}>
              {monthName(i + 1, lang)}
            </option>
          ))}
        </select>
      </label>
      <button className="btn btn-sm" disabled={!/^(19|20)\d\d$/.test(year)} onClick={() => save({})}>
        {t("common.save")}
      </button>
      <button className="btn btn-sm" onClick={() => save({ noDate: true })}>
        {t("backups.noDate")}
      </button>
      <button className="btn btn-sm btn-subtle" onClick={() => save({ excluded: true })}>
        {t("backups.exclude")}
      </button>
    </div>
  );
}

function dateLabel(b: { year: number | null; month: number | null; noDate?: boolean }, none: string) {
  if (b.noDate || !b.year) return none;
  return b.month ? `${b.year}-${String(b.month).padStart(2, "0")}` : String(b.year);
}

function Library({ items, selected, setSelected, reload }: { items: BackupItem[]; selected: Set<string>; setSelected: (s: Set<string>) => void; reload: () => void }) {
  const t = useT();
  const lang = useLang();
  const [drive, setDrive] = useState("");
  const [year, setYear] = useState("");
  const [state, setState] = useState("");
  const [openRow, setOpenRow] = useState<string | null>(null);
  const drives = Array.from(new Set(items.map((i) => i.volume))).sort();
  const years = Array.from(new Set(items.map((i) => i.year).filter((y): y is number => !!y))).sort();
  const shown = items.filter(
    (i) =>
      (!drive || i.volume === drive) &&
      (!year || String(i.year) === year) &&
      (!state || (state === "organized" ? i.organized : state === "review" ? i.confidence === "review" : !i.organized)),
  );
  // Per-year bars: count and size of backups with a known year.
  const perYear = years.map((y) => {
    const of = items.filter((i) => i.year === y);
    return { year: y, n: of.length, bytes: of.reduce((a, i) => a + i.size, 0) };
  });
  const max = Math.max(1, ...perYear.map((p) => p.n));
  return (
    <>
      {perYear.length > 0 && (
        <div className="card">
          <h2 className="section-title">{t("backups.timeline")}</h2>
          <div className="year-bars">
            {perYear.map((p) => (
              <button
                key={p.year}
                className="year-bar"
                aria-pressed={year === String(p.year)}
                title={`${p.year}: ${p.n} · ${formatBytes(p.bytes, lang)}`}
                onClick={() => setYear(year === String(p.year) ? "" : String(p.year))}
              >
                <span className="num caption">{p.n}</span>
                <span className="bar" style={{ height: `${Math.max(6, (p.n / max) * 64)}px` }} />
                <span className="caption">{p.year}</span>
              </button>
            ))}
          </div>
        </div>
      )}
      <div className="card">
        <div className="row" style={{ gap: 12, flexWrap: "wrap", alignItems: "flex-end" }}>
          <label className="field">
            <span>{t("common.drive")}</span>
            <select className="select" value={drive} onChange={(e) => setDrive(e.target.value)}>
              <option value="">{t("common.allDrives")}</option>
              {drives.map((d) => (
                <option key={d}>{d}</option>
              ))}
            </select>
          </label>
          <label className="field">
            <span>{t("backups.filterYear")}</span>
            <select className="select" value={year} onChange={(e) => setYear(e.target.value)}>
              <option value="">{t("backups.allYears")}</option>
              {years.map((y) => (
                <option key={y}>{y}</option>
              ))}
            </select>
          </label>
          <label className="field">
            <span>{t("common.status")}</span>
            <select className="select" value={state} onChange={(e) => setState(e.target.value)}>
              <option value="">—</option>
              <option value="pending">{t("backups.notOrganized")}</option>
              <option value="organized">{t("backups.organized")}</option>
              <option value="review">{t("backups.conf.review")}</option>
            </select>
          </label>
          <span className="caption">{t("common.selected", { n: selected.size })}</span>
        </div>
        <div className="legend">
          <span className="row"><Badge tone="success">{t("backups.conf.high")}</Badge>{t("backups.legend.high")}</span>
          <span className="row"><Badge tone="accent">{t("backups.conf.medium")}</Badge>{t("backups.legend.medium")}</span>
          <span className="row"><Badge tone="warning">{t("backups.conf.review")}</Badge>{t("backups.legend.review")}</span>
        </div>
        <div style={{ overflow: "auto", maxHeight: 640, marginTop: 12 }}>
          <table className="table">
            <thead>
              <tr>
                <th className="shrink">
                  <span className="sr-only">{t("backups.selectForPlan")}</span>
                </th>
                <th style={{ width: "45%" }}>{t("common.name")}</th>
                <th className="shrink">{t("common.date")}</th>
                <th className="shrink">{t("common.size")}</th>
                <th className="shrink">{t("common.status")}</th>
                <th className="shrink" />
              </tr>
            </thead>
            <tbody>
              {shown.map((b) => (
                <FragmentRow key={b.path} b={b} open={openRow === b.path} onToggle={() => setOpenRow(openRow === b.path ? null : b.path)} selected={selected.has(b.path)} onSelect={(v) => {
                  const n = new Set(selected);
                  if (v) n.add(b.path);
                  else n.delete(b.path);
                  setSelected(n);
                }} reload={reload} lang={lang} />
              ))}
            </tbody>
          </table>
        </div>
      </div>
    </>
  );
}

function FragmentRow({ b, open, onToggle, selected, onSelect, reload, lang }: { b: BackupItem; open: boolean; onToggle: () => void; selected: boolean; onSelect: (v: boolean) => void; reload: () => void; lang: "es" | "en" }) {
  const t = useT();
  return (
    <>
      <tr aria-selected={selected}>
        <td className="shrink">
          <input type="checkbox" aria-label={`${t("backups.selectForPlan")}: ${b.name}`} checked={selected} disabled={b.organized || !!b.protectedReason} onChange={(e) => onSelect(e.target.checked)} />
        </td>
        <td>
          <strong className="path" title={b.path}>
            {b.name}
          </strong>
          <PathText path={b.path} className="caption" />
        </td>
        <td className="shrink">
          <div className="num">{dateLabel(b, t("backups.noDate"))}</div>
          <span className="caption">{t(`backups.source.${b.dateSource}` as Key)}</span>
        </td>
        <td className="shrink num">
          {formatBytes(b.size, lang)}
          <div className="caption">{t("common.files", { n: formatNumber(b.fileCount, lang) })}</div>
        </td>
        <td className="shrink">
          <div className="row" style={{ flexWrap: "wrap" }}>
            <Badge tone={CONF_TONE[b.confidence]}>{b.ambiguous ? t("backups.ambiguous") : t(`backups.conf.${b.confidence}` as Key)}</Badge>
            {b.organized && <Badge tone="success">{t("backups.organized")}</Badge>}
            {b.protectedReason && <Badge>{b.protectedReason}</Badge>}
          </div>
        </td>
        <td className="shrink">
          <div className="row">
            <PathActions path={b.path} />
            <button className="btn btn-sm" aria-expanded={open} onClick={onToggle}>
              {t("backups.editDate")}
            </button>
          </div>
        </td>
      </tr>
      {open && (
        <tr>
          <td />
          <td colSpan={5} style={{ maxWidth: "none" }}>
            <div className="stack">
              <div>
                <span className="caption">{t("backups.why")}</span>
                <ul style={{ margin: "4px 0", paddingLeft: 18 }}>
                  {b.reasons.map((r) => (
                    <li key={r}>{r}</li>
                  ))}
                </ul>
                <span className="caption">
                  {t("old.newest")}: {formatDate(b.newestMtime, lang)} · {t("old.oldest")}: {formatDate(b.oldestMtime, lang)}
                </span>
              </div>
              <DateEditor b={b} onSaved={reload} />
            </div>
          </td>
        </tr>
      )}
    </>
  );
}

/** Text tree of the expected result: volume → year → month → folder. */
export function expectedTree(items: PlanItem[]): string {
  const ok = items.filter((i) => i.status === "ready" || i.status === "renamed");
  const byVol = new Map<string, PlanItem[]>();
  for (const i of ok) byVol.set(i.volume, [...(byVol.get(i.volume) ?? []), i]);
  const lines: string[] = [];
  for (const [vol, list] of [...byVol].sort()) {
    lines.push(vol);
    const rels = list.map((i) => i.dst.slice(vol.length + 1)).sort();
    let prev: string[] = [];
    for (const rel of rels) {
      const parts = rel.split("\\");
      parts.forEach((p, d) => {
        if (prev[d] !== p || prev.slice(0, d).join("\\") !== parts.slice(0, d).join("\\")) lines.push(`${"  ".repeat(d + 1)}${p}`);
      });
      prev = parts;
    }
  }
  return lines.join("\n");
}

function Organize({ items, selected, onDone }: { items: BackupItem[]; selected: Set<string>; onDone: () => void }) {
  const t = useT();
  const lang = useLang();
  const { settings, updateSettings, trackTask, latest, toast, navigate } = useApp();
  const [plan, setPlan] = useState<Plan | null>(null);
  const [planning, setPlanning] = useState(false);
  const planRef = useRef<HTMLDivElement>(null);
  const [error, setError] = useState<string | null>(null);
  const [confirm, setConfirm] = useState(false);
  const task = latest("organize");
  const running = isRunning(task);
  const result = task?.done?.ok ? (task.done.result as ExecResult) : null;
  const vols = Array.from(new Set(items.filter((i) => selected.has(i.path)).map((i) => i.volume))).sort();

  // Bring the preview into view: it renders below the options.
  useEffect(() => {
    if (plan) planRef.current?.scrollIntoView({ behavior: "smooth", block: "start" });
  }, [plan]);

  const preview = async () => {
    setError(null);
    setPlanning(true);
    try {
      const chosen = items.filter((i) => selected.has(i.path));
      setPlan(
        await api.organizePlan({
          items: chosen.map((i) => ({ path: i.path, year: i.noDate ? null : i.year, month: i.noDate ? null : i.month, noDate: i.noDate, indexed: [i.fileCount, i.size] })),
          rootName: settings.rootName,
          parents: settings.backupParents,
          layout: settings.layout,
        }),
      );
    } catch (e) {
      setError(errorText(e));
    } finally {
      setPlanning(false);
    }
  };
  const execute = async () => {
    if (!plan) return;
    setConfirm(false);
    try {
      trackTask(await api.organizeExecute(plan.items), "organize");
    } catch (e) {
      toast(errorText(e));
    }
  };

  return (
    <div className="stack">
      <div className="card">
        <Notice>{t("backups.planNote")}</Notice>
        <div className="row" style={{ gap: 16, flexWrap: "wrap", alignItems: "flex-end", marginTop: 12 }}>
          <label className="field">
            <span>{t("backups.rootFolder")}</span>
            <input className="input" value={settings.rootName} onChange={(e) => updateSettings({ rootName: e.target.value })} />
          </label>
          <label className="field">
            <span>{t("backups.layout")}</span>
            <select className="select" value={settings.layout} onChange={(e) => updateSettings({ layout: e.target.value as "yearMonth" | "year" })}>
              <option value="yearMonth">{t("backups.layout.yearMonth")}</option>
              <option value="year">{t("backups.layout.year")}</option>
            </select>
          </label>
          {vols.map((v) => (
            <div key={v} className="field">
              <span>
                {t("backups.otherVolume")} {v}
              </span>
              <div className="row">
                <PathText path={`${settings.backupParents[v] ?? `${v}\\`}\\${settings.rootName}`.replace(/\\\\/g, "\\")} />
                <button
                  className="btn btn-sm"
                  onClick={async () => {
                    const p = await open({ directory: true, defaultPath: `${v}\\` });
                    if (typeof p === "string") {
                      if (!p.toUpperCase().startsWith(v.toUpperCase())) return toast(t("state.conflict"));
                      updateSettings({ backupParents: { ...settings.backupParents, [v]: p } });
                    }
                  }}
                >
                  {t("compress.choose")}
                </button>
                {settings.backupParents[v] && (
                  <button
                    className="btn btn-sm btn-subtle"
                    onClick={() => {
                      const rest = { ...settings.backupParents };
                      delete rest[v];
                      updateSettings({ backupParents: rest });
                    }}
                  >
                    {t("backups.driveRoot")}
                  </button>
                )}
              </div>
            </div>
          ))}
          <button className="btn btn-primary" disabled={selected.size === 0 || running || planning} onClick={preview} aria-busy={planning}>
            {planning ? t("backups.planning") : `${t("backups.makePlan")} (${selected.size})`}
          </button>
        </div>
        {error && <Notice tone="danger">{error}</Notice>}
      </div>

      {plan && (
        <div className="card stack" ref={planRef}>
          <div className="row-between">
            <h2 className="section-title" style={{ margin: 0 }}>
              {t("backups.planTitle")}
            </h2>
            <span className="caption num">
              {t("backups.totals", { n: plan.ready, files: formatNumber(plan.totalFiles, lang), size: formatBytes(plan.totalBytes, lang) })}
            </span>
          </div>
          {plan.warnings.map((w) => (
            <Notice key={w} tone="warning">
              {w}
            </Notice>
          ))}
          <div style={{ overflow: "auto", maxHeight: 420 }}>
            <table className="table">
              <tbody>
                {plan.items.map((i) => (
                  <tr key={i.src}>
                    <td>
                      <PathText path={i.src} />
                      <div className="row caption">
                        <ArrowRight size={12} aria-hidden /> <PathText path={i.dst || "—"} />
                      </div>
                      {i.message && <span className="caption">{i.message}</span>}
                    </td>
                    <td className="shrink num caption">{formatBytes(i.bytes, lang)}</td>
                    <td className="shrink">
                      <Badge tone={i.status === "ready" ? "success" : i.status === "renamed" ? "accent" : i.status === "alreadyOrganized" ? "neutral" : "warning"}>
                        {t(`backups.status.${i.status}` as Key)}
                      </Badge>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
          {plan.ready > 0 && (
            <>
              <span className="caption">{t("backups.expectedTree")}</span>
              <pre className="tree">{expectedTree(plan.items)}</pre>
            </>
          )}
          <div className="actions">
            <button className="btn btn-primary" disabled={plan.ready === 0 || running} onClick={() => setConfirm(true)}>
              {t("backups.execute", { n: plan.ready })}
            </button>
          </div>
        </div>
      )}

      {running && (
        <div className="card" aria-live="polite">
          <p>{t("backups.executing")}</p>
          <Meter label={t("backups.executing")} />
        </div>
      )}
      {task?.done && (
        <div className="card stack">
          {task.done.error && <Notice tone="danger">{task.done.error}</Notice>}
          {result && (
            <Notice tone={result.failed ? "warning" : "success"}>
              {result.failed ? t("backups.resultPartial", { done: result.done, failed: result.failed }) : t("backups.resultOk", { n: result.done })}
              {result.messages.map((m) => (
                <div key={m} className="caption">
                  {m}
                </div>
              ))}
            </Notice>
          )}
          {result && result.done > 0 && (
            <div className="actions">
              <button
                className="btn"
                onClick={() =>
                  api.organizeUndo(result.opId).then((r) => {
                    toast(r.failed ? r.messages.join(" · ") : t("history.status.reverted"));
                    onDone();
                  }, (e) => toast(errorText(e)))
                }
              >
                {t("backups.undo")}
              </button>
              <button className="btn btn-primary" onClick={() => navigate("backups", { tab: "compress" })}>
                {t("backups.compressNext")}
              </button>
              <button className="btn btn-subtle" onClick={() => navigate("scan")}>
                {t("nav.scan")}
              </button>
            </div>
          )}
        </div>
      )}
      <ConfirmDialog
        open={confirm}
        title={t("backups.confirmTitle", { n: plan?.ready ?? 0 })}
        body={t("backups.confirmBody")}
        confirmLabel={t("backups.execute", { n: plan?.ready ?? 0 })}
        onConfirm={execute}
        onClose={() => setConfirm(false)}
      />
    </div>
  );
}

/** Compresses one or several folders, each into its own verified ZIP. With
 *  the option on, each original goes to the Recycle Bin only after its ZIP
 *  verified and only if the folder did not change (checked in Rust). */
export function Compress({ items, initialSource, marked }: { items: BackupItem[]; initialSource?: string; marked: Set<string> }) {
  const t = useT();
  const lang = useLang();
  const { settings, trackTask, latest, toast, navigate } = useApp();
  const [queue, setQueue] = useState<string[]>(initialSource ? [initialSource] : []);
  const [destDir, setDestDir] = useState("");
  const [name, setName] = useState("");
  const [level, setLevel] = useState(settings.compressionLevel);
  const [recycleOriginals, setRecycleOriginals] = useState(false);
  const [previews, setPreviews] = useState<Record<string, CompressPreview | { error: string }>>({});
  const [reviewing, setReviewing] = useState<number | null>(null);
  const [confirm, setConfirm] = useState(false);
  const [confirmEmpty, setConfirmEmpty] = useState(false);
  const task = latest("compress");
  const running = isRunning(task);
  const prog = task?.progress as BatchProgress | null;
  const result = task?.done?.ok ? (task.done.result as BatchResult | CompressResult) : null;
  const batch = result && "items" in result ? result : null;
  const byPath = useMemo(() => new Map(items.map((i) => [i.path, i])), [items]);
  // "E:\\Backup" → "E:\\" (a bare "E:" would mean the current folder of E:).
  const parentOf = (p: string) => p.replace(/[\\/][^\\/]+[\\/]?$/, "").replace(/^([A-Za-z]:)$/, "$1\\");
  const req = (source: string): CompressRequest => ({
    source,
    destDir: destDir || parentOf(source),
    name: queue.length === 1 && name ? name : null,
    level,
  });
  const reset = () => setPreviews({});
  const add = (paths: string[]) => {
    setQueue((q) => [...q, ...paths.filter((p) => !q.includes(p))]);
    reset();
  };
  const okPreview = (p: string) => {
    const v = previews[p];
    return v && !("error" in v) && v.problems.length === 0 ? v : null;
  };
  const ready = queue.filter((p) => okPreview(p));
  const reviewed = queue.length > 0 && queue.every((p) => previews[p]);

  // Space needed per destination drive (worst case: a ZIP as big as its source).
  const space = new Map<string, { need: number; free: number | null }>();
  for (const p of ready) {
    const v = okPreview(p)!;
    const drive = v.dest.slice(0, 2).toUpperCase();
    const s = space.get(drive) ?? { need: 0, free: v.destFree };
    s.need += v.bytes;
    space.set(drive, s);
  }
  const short = [...space.entries()].filter(([, s]) => s.free != null && s.need > s.free);

  const review = async () => {
    const out: Record<string, CompressPreview | { error: string }> = {};
    for (const [i, p] of queue.entries()) {
      setReviewing(i);
      try {
        out[p] = await api.compressPreview(req(p));
      } catch (e) {
        out[p] = { error: errorText(e) };
      }
      setPreviews({ ...out });
    }
    setReviewing(null);
  };
  const start = async () => {
    setConfirm(false);
    try {
      trackTask(await api.startCompressBatch({ items: ready.map(req), recycleOriginals }), "compress");
    } catch (e) {
      toast(errorText(e));
    }
  };
  const emptyBin = async () => {
    setConfirmEmpty(false);
    try {
      const r = await api.emptyRecycleBin();
      toast(`${t("cleanup.emptyBin").replace("…", "")}: ${formatBytes(r.bytes, lang)}`);
    } catch (e) {
      toast(errorText(e));
    }
  };
  const phaseText = (p?: string) =>
    p === "verifying" ? t("compress.verifying") : p === "checking" ? t("compress.phase.checking") : p === "recycling" ? t("compress.phase.recycling") : t("compress.reading");
  const notQueued = items.filter((i) => !queue.includes(i.path) && !i.protectedReason);
  const markedNew = [...marked].filter((p) => !queue.includes(p));

  return (
    <div className="stack">
      <div className="card stack">
        <h2 className="section-title">{t("compress.title")}</h2>
        <Notice>{t("compress.explain")}</Notice>

        <div className="stack" style={{ gap: 8 }}>
          <strong>{t("compress.queue", { n: queue.length })}</strong>
          {queue.length === 0 ? (
            <p className="caption">{t("compress.queueEmpty")}</p>
          ) : (
            <ul className="dup-list">
              {queue.map((p) => {
                const v = previews[p];
                const known = byPath.get(p);
                return (
                  <li key={p} className="dup-row">
                    <div style={{ minWidth: 0 }}>
                      <PathText path={p} />
                      <span className="row caption" style={{ flexWrap: "wrap" }}>
                        {v && !("error" in v) ? (
                          <>
                            {formatBytes(v.bytes, lang)} · {t("common.files", { n: formatNumber(v.files, lang) })} → {baseName(v.dest)}
                          </>
                        ) : known ? (
                          <>
                            {formatBytes(known.size, lang)} · {t("common.files", { n: formatNumber(known.fileCount, lang) })}
                          </>
                        ) : null}
                        {v && "error" in v && <Badge tone="danger">{v.error}</Badge>}
                        {v && !("error" in v) && v.problems.map((x) => <Badge key={x} tone="danger">{x}</Badge>)}
                        {v && !("error" in v) && v.problems.length === 0 && <Badge tone="success">{t("compress.ready")}</Badge>}
                        {v && !("error" in v) && recycleOriginals && v.links > 0 && <Badge tone="warning">{t("compress.linksKept")}</Badge>}
                      </span>
                    </div>
                    <PathActions path={p} />
                    <button
                      className="btn btn-sm btn-subtle"
                      disabled={running || reviewing != null}
                      onClick={() => {
                        setQueue((q) => q.filter((x) => x !== p));
                        reset();
                      }}
                    >
                      {t("compress.remove")}
                    </button>
                  </li>
                );
              })}
            </ul>
          )}
          <div className="row" style={{ flexWrap: "wrap" }}>
            {markedNew.length > 0 && (
              <button className="btn btn-sm" disabled={running} onClick={() => add(markedNew)}>
                {t("compress.addMarked", { n: markedNew.length })}
              </button>
            )}
            <select className="select" style={{ maxWidth: 360 }} value="" disabled={running} aria-label={t("compress.addBackup")} onChange={(e) => e.target.value && add([e.target.value])}>
              <option value="">{t("compress.addBackup")}</option>
              {notQueued.map((i) => (
                <option key={i.path} value={i.path}>
                  {i.name} — {i.path}
                </option>
              ))}
            </select>
            <button
              className="btn btn-sm"
              disabled={running}
              onClick={async () => {
                const p = await open({ directory: true, multiple: true });
                if (typeof p === "string") add([p]);
                else if (Array.isArray(p)) add(p);
              }}
            >
              {t("compress.addFolder")}
            </button>
          </div>
        </div>

        <div className="grid grid-2" style={{ gap: 12 }}>
          <label className="field">
            <span>{t("compress.dest")}</span>
            <div className="row">
              <input className="input" style={{ flex: 1, minWidth: 0 }} value={destDir} placeholder={t("compress.destNextTo")} onChange={(e) => { setDestDir(e.target.value); reset(); }} />
              <button className="btn" onClick={async () => { const p = await open({ directory: true }); if (typeof p === "string") { setDestDir(p); reset(); } }}>
                {t("compress.choose")}
              </button>
            </div>
          </label>
          <label className="field">
            <span>{t("compress.level")}</span>
            <select className="select" value={level} onChange={(e) => setLevel(Number(e.target.value))}>
              <option value={1}>{t("compress.level.fast")}</option>
              <option value={6}>{t("compress.level.balanced")}</option>
              <option value={9}>{t("compress.level.small")}</option>
            </select>
          </label>
          {queue.length === 1 && (
            <label className="field">
              <span>{t("compress.name")}</span>
              <input className="input" value={name} placeholder={baseName(queue[0]!)} onChange={(e) => { setName(e.target.value); reset(); }} />
            </label>
          )}
        </div>

        <label className="row" style={{ alignItems: "flex-start", gap: 10 }}>
          <input type="checkbox" checked={recycleOriginals} disabled={running} onChange={(e) => setRecycleOriginals(e.target.checked)} style={{ marginTop: 4 }} />
          <span>
            <strong>{t("compress.recycleOriginals")}</strong>
            <br />
            <span className="caption">{t("compress.recycleRules")}</span>
          </span>
        </label>

        <div className="actions">
          <button className="btn" disabled={queue.length === 0 || running || reviewing != null} onClick={review} aria-busy={reviewing != null}>
            {reviewing != null ? t("compress.reviewing", { i: reviewing + 1, n: queue.length }) : t("compress.reviewN", { n: queue.length })}
          </button>
          <button className="btn btn-primary" disabled={!reviewed || ready.length === 0 || running || reviewing != null} onClick={() => (recycleOriginals ? setConfirm(true) : start())}>
            {t("compress.startN", { n: ready.length })}
          </button>
        </div>
        {reviewed && ready.length > 0 && (
          <div className="grid grid-stats">
            <Stat value={formatBytes(ready.reduce((a, p) => a + okPreview(p)!.bytes, 0), lang)} label={t("compress.sourceSize")} />
            <Stat value={formatNumber(ready.reduce((a, p) => a + okPreview(p)!.files, 0), lang)} label={t("common.files", { n: "" }).trim()} />
          </div>
        )}
        {short.map(([drive, s]) => (
          <Notice key={drive} tone="warning">
            {t("compress.spaceShort", { drive, need: formatBytes(s.need, lang), free: formatBytes(s.free ?? 0, lang) })}
          </Notice>
        ))}
        {reviewed && ready.some((p) => okPreview(p)!.alreadyCompressedShare > 0.3) && <Notice tone="warning">{t("compress.someAlreadyCompressed")}</Notice>}
        {!recycleOriginals && <p className="caption">{t("compress.keepNote")}</p>}
      </div>

      {running && (
        <div className="card stack" aria-live="polite">
          <div className="row-between">
            <strong>
              {prog && prog.count ? `${t("compress.progressItem", { i: prog.index + 1, n: prog.count })} · ` : ""}
              {phaseText(prog?.phase)}
            </strong>
            {prog && prog.bytesTotal > 0 && (
              <span className="caption num">
                {formatBytes(prog.bytesDone, lang)} / {formatBytes(prog.bytesTotal, lang)}
              </span>
            )}
          </div>
          {prog?.source && <PathText path={prog.source} />}
          <Meter value={prog?.bytesTotal ? prog.bytesDone / prog.bytesTotal : null} label={phaseText(prog?.phase)} />
          {prog && prog.count > 1 && <Meter value={prog.index / prog.count} label={t("compress.overall")} />}
          {prog?.current && <PathText path={prog.current} className="caption" />}
          {task && (
            <button className="btn btn-sm" style={{ alignSelf: "flex-start" }} onClick={() => api.cancelTask(task.id)}>
              {t("common.cancel")}
            </button>
          )}
        </div>
      )}

      {task?.done && !running && (
        <div className="card stack">
          {task.done.error && <Notice tone="danger">{task.done.error}</Notice>}
          {task.done.cancelled && <Notice tone="warning">{t("history.status.cancelled")}</Notice>}
          {batch && (
            <>
              <div className="row-between">
                <h2 className="section-title" style={{ margin: 0 }}>
                  {t("compress.batchDone")}
                </h2>
                <Badge tone={batch.failed ? "warning" : "success"}>
                  <ShieldCheck size={14} aria-hidden /> {t("compress.verifiedCount", { n: batch.verified, total: batch.items.length })}
                </Badge>
              </div>
              <div style={{ overflow: "auto" }}>
                <table className="table">
                  <thead>
                    <tr>
                      <th>{t("compress.col.folder")}</th>
                      <th className="shrink">{t("compress.col.size")}</th>
                      <th className="shrink">{t("common.status")}</th>
                      <th className="shrink">{t("compress.col.original")}</th>
                    </tr>
                  </thead>
                  <tbody>
                    {batch.items.map((it) => (
                      <tr key={it.source}>
                        <td style={{ minWidth: 0 }}>
                          <PathText path={it.source} />
                          {it.result && (
                            <span className="row caption">
                              → <PathText path={it.result.archive} />
                              <PathActions path={it.result.archive} />
                            </span>
                          )}
                        </td>
                        <td className="num shrink">
                          {it.result ? `${formatBytes(it.result.sourceBytes, lang)} → ${formatBytes(it.result.archiveBytes, lang)}` : "—"}
                        </td>
                        <td className="shrink">
                          {it.result ? <Badge tone="success">{t("compress.verifiedBadge")}</Badge> : <Badge tone="danger" title={it.error ?? ""}>{t("common.error")}</Badge>}
                          {it.error && <div className="caption">{it.error}</div>}
                        </td>
                        <td className="shrink">
                          {it.recycled ? (
                            <Badge tone="accent">{t("compress.original.inBin")}</Badge>
                          ) : (
                            <>
                              <Badge>{t("compress.original.kept")}</Badge>
                              {it.keptReason && <div className="caption">{it.keptReason}</div>}
                            </>
                          )}
                        </td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
              {batch.recycled > 0 && (
                <Notice>
                  <div className="stack" style={{ gap: 8 }}>
                    <span>{t("compress.binNote", { n: batch.recycled, size: formatBytes(batch.bytesInBin, lang), saving: formatBytes(Math.max(0, batch.saving), lang) })}</span>
                    <button className="btn btn-sm" style={{ alignSelf: "flex-start" }} onClick={() => setConfirmEmpty(true)}>
                      {t("cleanup.emptyBin")}
                    </button>
                  </div>
                </Notice>
              )}
              <div className="row">
                <span className="caption">{t("compress.inCatalog")}</span>
                <button className="btn btn-sm" onClick={() => navigate("catalog")}>
                  {t("compress.openCatalog")}
                </button>
              </div>
            </>
          )}
        </div>
      )}
      <ConfirmDialog
        open={confirm}
        title={t("compress.confirmTitle", { n: ready.length })}
        body={
          <>
            <p>{t("compress.confirmBody")}</p>
            <p>{t("cleanup.binNote")}</p>
          </>
        }
        confirmLabel={t("compress.startN", { n: ready.length })}
        onConfirm={start}
        onClose={() => setConfirm(false)}
      />
      <ConfirmDialog
        open={confirmEmpty}
        title={t("cleanup.emptyBinTitle")}
        body={<p>{t("compress.emptyBinBody")}</p>}
        confirmLabel={t("cleanup.emptyBin").replace("…", "")}
        onConfirm={emptyBin}
        onClose={() => setConfirmEmpty(false)}
      />
    </div>
  );
}

export function BackupsPage() {
  const t = useT();
  const { settings, params, indexVersion, latest, navigate } = useApp();
  const scanning = isRunning(latest("scan"));
  const [tab, setTab] = useState<Tab>((params.tab as Tab) ?? "library");
  const list = useAsync(() => (scanning ? Promise.resolve(null) : api.backupsList(settings.rootName)), [indexVersion, settings.rootName, scanning]);
  const items = useMemo(() => list.data ?? [], [list.data]);
  const [selected, setSelected] = useState<Set<string> | null>(null);
  // Default plan selection: unorganised backups whose year and month come
  // from the name or from the user — never from an inferred content date.
  const sel =
    selected ??
    new Set(
      items
        .filter((i) => !i.organized && !i.protectedReason && i.year && i.month && !i.ambiguous && (i.dateSource === "name" || i.dateSource === "override"))
        .map((i) => i.path),
    );

  return (
    <>
      <PageHeader
        title={t("backups.title")}
        subtitle={t("backups.subtitle")}
        actions={
          <div className="segmented" role="tablist">
            {(["library", "organize", "compress"] as Tab[]).map((k) => (
              <button key={k} role="tab" aria-selected={tab === k} aria-pressed={tab === k} onClick={() => setTab(k)}>
                {t(`backups.tab.${k}` as Key)}
              </button>
            ))}
          </div>
        }
      />
      {scanning && <Notice>{t("common.busyScan")}</Notice>}
      {list.error && <Notice tone="danger">{list.error}</Notice>}
      {list.data && items.length === 0 && tab !== "compress" ? (
        <div className="card">
          <EmptyState icon={<Archive size={40} />} title={t("backups.none")} body={t("common.needScan")} action={<button className="btn" onClick={() => navigate("scan")}>{t("common.goScan")}</button>} />
        </div>
      ) : (
        <div className="stack">
          {tab === "library" && <Library items={items} selected={sel} setSelected={setSelected} reload={list.reload} />}
          {tab === "organize" && <Organize items={items} selected={sel} onDone={list.reload} />}
          {tab === "compress" && <Compress items={items} initialSource={params.source} marked={selected ?? new Set()} />}
        </div>
      )}
    </>
  );
}
