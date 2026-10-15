import { open } from "@tauri-apps/plugin-dialog";
import { Archive, ArrowRight, ShieldCheck } from "lucide-react";
import { useMemo, useState } from "react";
import { api, errorText } from "../api";
import { Badge, ConfirmDialog, EmptyState, Meter, Notice, PageHeader, PathActions, PathText, Stat, useAsync, type Tone } from "../components/ui";
import { baseName, formatBytes, formatDate, formatNumber, formatPercent, monthName } from "../format";
import { useLang, useT, type Key } from "../i18n";
import { isRunning, useApp } from "../state";
import type { BackupItem, CompressPreview, CompressProgress, CompressResult, ExecResult, Plan, PlanItem } from "../types";

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
  const [error, setError] = useState<string | null>(null);
  const [confirm, setConfirm] = useState(false);
  const task = latest("organize");
  const running = isRunning(task);
  const result = task?.done?.ok ? (task.done.result as ExecResult) : null;
  const vols = Array.from(new Set(items.filter((i) => selected.has(i.path)).map((i) => i.volume))).sort();

  const preview = async () => {
    setError(null);
    try {
      const chosen = items.filter((i) => selected.has(i.path));
      setPlan(
        await api.organizePlan({
          items: chosen.map((i) => ({ path: i.path, year: i.noDate ? null : i.year, month: i.noDate ? null : i.month, noDate: i.noDate })),
          rootName: settings.rootName,
          parents: settings.backupParents,
          layout: settings.layout,
        }),
      );
    } catch (e) {
      setError(errorText(e));
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
          <button className="btn btn-primary" disabled={selected.size === 0 || running} onClick={preview}>
            {t("backups.makePlan")} ({selected.size})
          </button>
        </div>
        {error && <Notice tone="danger">{error}</Notice>}
      </div>

      {plan && (
        <div className="card stack">
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

export function Compress({ items, initialSource }: { items: BackupItem[]; initialSource?: string }) {
  const t = useT();
  const lang = useLang();
  const { settings, trackTask, latest, toast, navigate } = useApp();
  const [source, setSource] = useState(initialSource ?? "");
  const [destDir, setDestDir] = useState("");
  const [name, setName] = useState("");
  const [level, setLevel] = useState(settings.compressionLevel);
  const [preview, setPreview] = useState<CompressPreview | null>(null);
  const [error, setError] = useState<string | null>(null);
  const task = latest("compress");
  const running = isRunning(task);
  const prog = task?.progress as CompressProgress | null;
  const result = task?.done?.ok ? (task.done.result as CompressResult) : null;
  const req = () => ({ source, destDir: destDir || source.replace(/[\\/][^\\/]+[\\/]?$/, ""), name: name || null, level });

  const doPreview = async () => {
    setError(null);
    try {
      setPreview(await api.compressPreview(req()));
    } catch (e) {
      setPreview(null);
      setError(errorText(e));
    }
  };
  const start = async () => {
    try {
      trackTask(await api.startCompress(req()), "compress");
    } catch (e) {
      toast(errorText(e));
    }
  };

  return (
    <div className="stack">
      <div className="card stack">
        <h2 className="section-title">{t("compress.title")}</h2>
        <Notice>{t("compress.explain")}</Notice>
        <div className="grid grid-2" style={{ gap: 12 }}>
          <label className="field">
            <span>{t("compress.source")}</span>
            <div className="row">
              <select className="select" style={{ flex: 1, minWidth: 0 }} value={source} onChange={(e) => { setSource(e.target.value); setPreview(null); }}>
                <option value="">{t("compress.selectSource")}</option>
                {source && !items.some((i) => i.path === source) && <option value={source}>{source}</option>}
                {items.map((i) => (
                  <option key={i.path} value={i.path}>
                    {i.name} — {i.path}
                  </option>
                ))}
              </select>
              <button className="btn" onClick={async () => { const p = await open({ directory: true }); if (typeof p === "string") { setSource(p); setPreview(null); } }}>
                {t("compress.choose")}
              </button>
            </div>
          </label>
          <label className="field">
            <span>{t("compress.dest")}</span>
            <div className="row">
              <input className="input" style={{ flex: 1, minWidth: 0 }} value={destDir} placeholder={source ? source.replace(/[\\/][^\\/]+[\\/]?$/, "") : ""} onChange={(e) => setDestDir(e.target.value)} />
              <button className="btn" onClick={async () => { const p = await open({ directory: true }); if (typeof p === "string") setDestDir(p); }}>
                {t("compress.choose")}
              </button>
            </div>
          </label>
          <label className="field">
            <span>{t("compress.name")}</span>
            <input className="input" value={name} placeholder={source ? baseName(source) : ""} onChange={(e) => setName(e.target.value)} />
          </label>
          <label className="field">
            <span>{t("compress.level")}</span>
            <select className="select" value={level} onChange={(e) => setLevel(Number(e.target.value))}>
              <option value={1}>{t("compress.level.fast")}</option>
              <option value={6}>{t("compress.level.balanced")}</option>
              <option value={9}>{t("compress.level.small")}</option>
            </select>
          </label>
        </div>
        <div className="actions">
          <button className="btn" disabled={!source || running} onClick={doPreview}>
            {t("compress.preview")}
          </button>
          <button className="btn btn-primary" disabled={!preview || preview.problems.length > 0 || running} onClick={start}>
            {t("compress.start")}
          </button>
        </div>
        {error && <Notice tone="danger">{error}</Notice>}
        {preview && (
          <div className="stack">
            <div className="grid grid-stats">
              <Stat value={formatBytes(preview.bytes, lang)} label={t("compress.sourceSize")} />
              <Stat value={formatNumber(preview.files, lang)} label={t("common.files", { n: "" }).trim()} />
              {preview.destFree != null && <Stat value={formatBytes(preview.destFree, lang)} label={t("compress.destFree", { size: "" }).replace(":", "").trim()} />}
            </div>
            <PathText path={preview.dest} className="caption" />
            {preview.alreadyCompressedShare > 0.3 && <Notice tone="warning">{t("compress.alreadyCompressed", { pct: formatPercent(preview.alreadyCompressedShare, lang) })}</Notice>}
            {preview.problems.map((p) => (
              <Notice key={p} tone="danger">
                {p}
              </Notice>
            ))}
            <p className="caption">{t("compress.keepNote")}</p>
          </div>
        )}
      </div>
      {running && (
        <div className="card" aria-live="polite">
          <div className="row-between">
            <strong>{prog?.phase === "verifying" ? t("compress.verifying") : t("compress.reading")}</strong>
            {prog && (
              <span className="caption num">
                {formatBytes(prog.bytesDone, lang)} / {formatBytes(prog.bytesTotal, lang)}
              </span>
            )}
          </div>
          <div style={{ margin: "8px 0" }}>
            <Meter value={prog?.bytesTotal ? prog.bytesDone / prog.bytesTotal : null} label={t("compress.reading")} />
          </div>
          {prog?.current && <PathText path={prog.current} className="caption" />}
          {task && (
            <button className="btn btn-sm" onClick={() => api.cancelTask(task.id)}>
              {t("common.cancel")}
            </button>
          )}
        </div>
      )}
      {task?.done && !running && (
        <div className="card stack">
          {task.done.error && <Notice tone="danger">{task.done.error}</Notice>}
          {task.done.cancelled && <Notice tone="warning">{t("history.status.cancelled")}</Notice>}
          {result && (
            <>
              <div className="row-between">
                <h2 className="section-title" style={{ margin: 0 }}>
                  {t("compress.done")}
                </h2>
                <Badge tone="success">
                  <ShieldCheck size={14} aria-hidden /> {t("compress.verifiedBadge")}
                </Badge>
              </div>
              <div className="grid grid-stats">
                <Stat value={formatBytes(result.sourceBytes, lang)} label={t("compress.sourceSize")} />
                <Stat value={formatBytes(result.archiveBytes, lang)} label={t("compress.archiveSize")} />
                <Stat value={formatBytes(result.potentialSaving, lang)} label={t("compress.potential")} />
              </div>
              <div className="row">
                <PathText path={result.archive} />
                <PathActions path={result.archive} />
              </div>
              <p className="caption">{t("compress.keepNote")}</p>
              <div className="row">
                <span className="caption">{t("compress.inCatalog")}</span>
                <button className="btn btn-sm" onClick={() => navigate("catalog", { archive: String(result.catalogId) })}>
                  {t("compress.openCatalog")}
                </button>
              </div>
              <details className="disclosure">
                <summary>{t("compress.notPreserved")}</summary>
                <ul>
                  {result.notPreserved.map((n) => (
                    <li key={n}>{n}</li>
                  ))}
                </ul>
              </details>
            </>
          )}
        </div>
      )}
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
          {tab === "compress" && <Compress items={items} initialSource={params.source} />}
        </div>
      )}
    </>
  );
}
