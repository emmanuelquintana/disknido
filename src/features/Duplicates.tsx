import { save } from "@tauri-apps/plugin-dialog";
import { Copy, Download, ShieldCheck, X } from "lucide-react";
import { useState } from "react";
import { api, errorText } from "../api";
import { Badge, ConfirmDialog, EmptyState, Meter, Notice, PageHeader, PathActions, PathText, useAsync } from "../components/ui";
import { formatBytes, formatDate, formatNumber } from "../format";
import { useLang, useT, type Key } from "../i18n";
import { isRunning, useApp } from "../state";
import type { DupGroup, DupProgress, DupSummary } from "../types";

const SIZES = [1024, 1024 * 1024, 10 * 1024 * 1024, 100 * 1024 * 1024];

export function GroupCard({ g, onDone }: { g: DupGroup; onDone: () => void }) {
  const t = useT();
  const lang = useLang();
  const { toast } = useApp();
  const [keep, setKeep] = useState(g.suggestedKeep);
  const [marked, setMarked] = useState<Set<number>>(new Set());
  const [confirm, setConfirm] = useState(false);
  const [busy, setBusy] = useState(false);
  const toggle = (i: number) =>
    setMarked((m) => {
      const n = new Set(m);
      if (n.has(i)) n.delete(i);
      else n.add(i);
      return n;
    });
  const run = async () => {
    setBusy(true);
    try {
      const r = await api.recycleDuplicates(
        g.id,
        g.files[keep]!.path,
        [...marked].map((i) => g.files[i]!.path),
      );
      toast(
        r.failed
          ? t("backups.resultPartial", { done: r.recycled, failed: r.failed }) + " " + r.messages.join(" · ")
          : `${t("cleanup.sentToBin")}: ${formatBytes(r.bytesSentToBin, lang)}`,
      );
      onDone();
    } catch (e) {
      toast(errorText(e));
    } finally {
      setBusy(false);
      setConfirm(false);
    }
  };
  return (
    <article className="card" aria-label={t("dup.copies", { n: g.files.length })}>
      <div className="row-between">
        <div className="row">
          <strong className="num">{t("dup.copies", { n: g.files.length })}</strong>
          <span className="muted num">· {t("dup.each", { size: formatBytes(g.size, lang) })}</span>
          <span className="muted num">· {t("dup.redundant", { size: formatBytes(g.redundantBytes, lang) })}</span>
        </div>
        {g.verified ? (
          <Badge tone="success" title={t("dup.verifiedDesc")}>
            <ShieldCheck size={14} aria-hidden /> {t("dup.verified")}
          </Badge>
        ) : (
          <Badge tone="accent" title={t("dup.identicalDesc")}>
            <ShieldCheck size={14} aria-hidden /> {t("dup.identical")}
          </Badge>
        )}
      </div>
      <table className="table" style={{ marginTop: 8 }}>
        <thead>
          <tr>
            <th className="shrink">{t("dup.keep")}</th>
            <th className="shrink">
              <span className="sr-only">{t("dup.recycle")}</span>
              <X size={14} aria-hidden />
            </th>
            <th>{t("common.location")}</th>
            <th className="shrink">{t("common.modified")}</th>
            <th className="shrink" />
          </tr>
        </thead>
        <tbody>
          {g.files.map((f, i) => {
            const isKeep = i === keep;
            return (
              <tr key={f.path} aria-selected={marked.has(i)}>
                <td className="shrink">
                  <input
                    type="radio"
                    name={`keep-${g.id}`}
                    checked={isKeep}
                    aria-label={`${t("dup.keep")} ${f.path}`}
                    onChange={() => {
                      setKeep(i);
                      setMarked((m) => {
                        const n = new Set(m);
                        n.delete(i);
                        return n;
                      });
                    }}
                  />
                </td>
                <td className="shrink">
                  <input
                    type="checkbox"
                    checked={marked.has(i)}
                    disabled={isKeep || !!f.protectedReason}
                    aria-label={`${t("dup.recycle")}: ${f.path}`}
                    onChange={() => toggle(i)}
                  />
                </td>
                <td>
                  <PathText path={f.path} />
                  <span className="row">
                    {i === g.suggestedKeep && <Badge tone="accent" title={g.suggestionReason}>{t("dup.suggested")}</Badge>}
                    {f.protectedReason && <Badge tone="warning">{t("dup.protectedCopy", { reason: f.protectedReason })}</Badge>}
                  </span>
                </td>
                <td className="shrink caption">{formatDate(f.mtime, lang)}</td>
                <td className="shrink">
                  <PathActions path={f.path} />
                </td>
              </tr>
            );
          })}
        </tbody>
      </table>
      <div className="row-between" style={{ marginTop: 8 }}>
        <span className="caption">{t("dup.suggestionNote")}</span>
        <button className="btn btn-sm" disabled={marked.size === 0 || busy} onClick={() => setConfirm(true)}>
          {t("dup.recycle")} ({marked.size})
        </button>
      </div>
      <ConfirmDialog
        open={confirm}
        title={t("dup.confirmTitle", { n: marked.size })}
        body={
          <>
            <p>{t("dup.confirmBody")}</p>
            <p>{t("cleanup.binNote")}</p>
          </>
        }
        confirmLabel={t("dup.recycle")}
        busy={busy}
        onConfirm={run}
        onClose={() => setConfirm(false)}
      />
      <details className="disclosure">
        <summary>{t("common.technical")}</summary>
        <code className="caption">BLAKE3 {g.hash}</code>
      </details>
    </article>
  );
}

export function DuplicatesPage() {
  const t = useT();
  const lang = useLang();
  const { settings, params, latest, trackTask, indexVersion, toast, navigate } = useApp();
  const [scope, setScope] = useState(params.scope ?? "");
  const [minSize, setMinSize] = useState(settings.dupMinSize);
  const [page, setPage] = useState(0);
  const [nonce, setNonce] = useState(0);
  const task = latest("duplicates");
  const scanTask = latest("scan");
  const running = isRunning(task);
  const ov = useAsync(() => (isRunning(scanTask) ? Promise.resolve(null) : api.overview(settings.rootName)), [indexVersion]);
  const groups = useAsync(() => api.duplicateGroups(page * 50, 50), [page, indexVersion, nonce, task?.done]);
  const p = task?.progress as DupProgress | null;
  const summary = task?.done?.ok ? (task.done.result as DupSummary) : null;

  const start = async () => {
    try {
      const id = await api.startDuplicates({ minSize, scopes: scope ? [scope] : [], profile: settings.profile });
      trackTask(id, "duplicates");
    } catch (e) {
      toast(errorText(e));
    }
  };

  const exportCsv = async () => {
    const path = await save({ defaultPath: "duplicados-disknido.csv", filters: [{ name: "CSV", extensions: ["csv"] }] });
    if (!path) return;
    api.exportDuplicates(path, settings.anonymizeReports).then(() => toast(t("settings.saved")), (e) => toast(errorText(e)));
  };

  const roots = (ov.data?.roots ?? []).map((r) => r.name);
  const noScan = ov.data && !ov.data.lastScan;
  const frac = p && p.phase === "hashing" && p.bytesTotal ? p.bytesDone / p.bytesTotal : p && p.filesTotal ? p.filesDone / p.filesTotal : null;

  return (
    <>
      <PageHeader
        title={t("dup.title")}
        subtitle={t("dup.subtitle")}
        actions={
          <button className="btn" onClick={exportCsv} disabled={!groups.data?.[1]}>
            <Download size={16} aria-hidden /> {t("dup.exportCsv")}
          </button>
        }
      />
      {noScan ? (
        <div className="card">
          <EmptyState icon={<Copy size={40} />} title={t("common.needScan")} action={<button className="btn btn-primary" onClick={() => navigate("scan")}>{t("common.goScan")}</button>} />
        </div>
      ) : (
        <div className="card">
          <div className="row" style={{ flexWrap: "wrap", gap: 16, alignItems: "flex-end" }}>
            <label className="field">
              <span>{t("dup.scope")}</span>
              <select className="select" value={scope} onChange={(e) => setScope(e.target.value)}>
                <option value="">{t("dup.scopeAll")}</option>
                {roots.map((r) => (
                  <option key={r} value={r}>
                    {r}
                  </option>
                ))}
                {scope && !roots.includes(scope) && <option value={scope}>{scope}</option>}
              </select>
            </label>
            <label className="field">
              <span>{t("dup.minSize")}</span>
              <select className="select" value={minSize} onChange={(e) => setMinSize(Number(e.target.value))}>
                {SIZES.map((s) => (
                  <option key={s} value={s}>
                    {formatBytes(s, lang)}
                  </option>
                ))}
              </select>
            </label>
            <button className="btn btn-primary" onClick={start} disabled={running || isRunning(scanTask)}>
              {t("dup.search")}
            </button>
            {running && task && (
              <button className="btn" onClick={() => api.cancelTask(task.id)}>
                {t("common.cancel")}
              </button>
            )}
          </div>
          {running && (
            <div style={{ marginTop: 16 }} aria-live="polite">
              <div className="row-between">
                <strong>{t(`dup.phase.${p?.phase ?? "grouping"}` as Key)}</strong>
                {p && p.filesTotal > 0 && (
                  <span className="caption num">
                    {formatNumber(p.filesDone, lang)} / {formatNumber(p.filesTotal, lang)}
                    {p.phase === "hashing" && ` · ${formatBytes(p.bytesDone, lang)} / ${formatBytes(p.bytesTotal, lang)}`}
                  </span>
                )}
              </div>
              <div style={{ marginTop: 8 }}>
                <Meter value={frac} label={t(`dup.phase.${p?.phase ?? "grouping"}` as Key)} />
              </div>
              {p?.current && (
                <div style={{ marginTop: 8 }}>
                  <span className="caption">{t("scan.current")}</span>
                  <PathText path={p.current} className="caption" />
                </div>
              )}
              <p className="caption">{t("dup.canLeave")}</p>
            </div>
          )}
          {task?.done?.error && (
            <div style={{ marginTop: 12 }}>
              <Notice tone="danger">{task.done.error}</Notice>
            </div>
          )}
          {summary && (
            <div className="stack" style={{ marginTop: 16 }}>
              <Notice tone="success">{t("dup.summary", { groups: formatNumber(summary.groups, lang), files: formatNumber(summary.files, lang), size: formatBytes(summary.redundantBytes, lang) })}</Notice>
              {summary.hardLinksCollapsed > 0 && <p className="caption">{t("dup.hardLinks", { n: summary.hardLinksCollapsed })}</p>}
              {summary.skipped.length > 0 && (
                <details className="disclosure">
                  <summary>{t("dup.skipped", { n: summary.skipped.length })}</summary>
                  <ul>
                    {summary.skipped.slice(0, 200).map(([path, why]) => (
                      <li key={path}>
                        <PathText path={path} /> <span className="caption">{why}</span>
                      </li>
                    ))}
                  </ul>
                </details>
              )}
            </div>
          )}
        </div>
      )}

      <section className="section" style={{ marginTop: 16 }}>
        {groups.data && groups.data[1] === 0 && !running && !noScan && (
          <div className="card">
            <EmptyState icon={<Copy size={40} />} title={task?.done ? t("dup.none") : t("dup.noRun")} />
          </div>
        )}
        {groups.data && groups.data[1] > 0 && (
          <>
            <p className="caption">{t("dup.markHint")}</p>
            <div className="stack">
              {groups.data[0].map((g) => (
                <GroupCard key={`${g.id}-${nonce}`} g={g} onDone={() => setNonce((n) => n + 1)} />
              ))}
            </div>
            <div className="row" style={{ marginTop: 16 }}>
              <button className="btn btn-sm" disabled={page === 0} onClick={() => setPage(page - 1)}>
                {t("common.back")}
              </button>
              <span className="caption num">
                {page * 50 + 1}–{Math.min((page + 1) * 50, groups.data[1])} / {formatNumber(groups.data[1], lang)}
              </span>
              <button className="btn btn-sm" disabled={(page + 1) * 50 >= groups.data[1]} onClick={() => setPage(page + 1)}>
                {t("explorer.more")}
              </button>
            </div>
          </>
        )}
      </section>
    </>
  );
}
