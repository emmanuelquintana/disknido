import { Sparkles } from "lucide-react";
import { useState } from "react";
import { api, errorText } from "../api";
import { Badge, ConfirmDialog, EmptyState, Meter, Notice, PageHeader, PathActions, PathText, Stat, type Tone } from "../components/ui";
import { formatBytes, formatDate, formatNumber } from "../format";
import { useLang, useT, type Key } from "../i18n";
import { useApp } from "../state";
import type { CleanupCategory, CleanupConfidence, RecycleResult } from "../types";

const TONE: Record<CleanupConfidence, Tone> = { high: "success", medium: "warning", manual: "neutral", protected: "accent" };

export function CategoryCard({ c, onChanged }: { c: CleanupCategory; onChanged: () => void }) {
  const t = useT();
  const lang = useLang();
  const { toast } = useApp();
  const [open, setOpen] = useState(false);
  const [sel, setSel] = useState<Set<string>>(new Set());
  const [confirm, setConfirm] = useState<"recycle" | "empty" | null>(null);
  const [busy, setBusy] = useState(false);
  const [result, setResult] = useState<RecycleResult | null>(null);
  const k = (s: string) => `cleanup.rule.${c.id}.${s}` as Key;
  const selectedBytes = c.files.filter((f) => sel.has(f.path)).reduce((a, f) => a + f.size, 0);

  const doRecycle = async () => {
    setBusy(true);
    try {
      const r = await api.cleanupRecycle(c.id, [...sel]);
      setResult(r);
      setSel(new Set());
      onChanged();
    } catch (e) {
      toast(errorText(e));
    } finally {
      setBusy(false);
      setConfirm(null);
    }
  };
  const doEmpty = async () => {
    setBusy(true);
    try {
      await api.emptyRecycleBin();
      toast(t("state.done"));
      onChanged();
    } catch (e) {
      toast(errorText(e));
    } finally {
      setBusy(false);
      setConfirm(null);
    }
  };

  return (
    <article className="card" aria-labelledby={`cat-${c.id}`}>
      <div className="row-between">
        <div>
          <h2 id={`cat-${c.id}`} className="section-title" style={{ marginBottom: 2 }}>
            {t(k("name"))}
          </h2>
          <p className="muted" style={{ margin: 0 }}>
            {t(k("desc"))}
          </p>
        </div>
        <Badge tone={TONE[c.confidence]} title={t(`cleanup.confDesc.${c.confidence}` as Key)}>
          {t(`cleanup.conf.${c.confidence}` as Key)}
        </Badge>
      </div>
      <div className="grid grid-stats" style={{ marginTop: 12 }}>
        {c.action !== "openStorageSettings" && <Stat value={formatBytes(c.bytes, lang)} label={t("cleanup.potential")} />}
        {c.action !== "openStorageSettings" && <Stat value={formatNumber(c.fileCount, lang)} label={t("common.files", { n: "" }).trim()} />}
      </div>
      <dl className="kv" style={{ marginTop: 12 }}>
        <dt>{t("common.risk")}</dt>
        <dd>{t(k("risk"))}</dd>
        <dt>{t("common.location")}</dt>
        <dd>
          {c.locations.slice(0, 4).map((l) => (
            <PathText key={l} path={l} />
          ))}
          {c.locations.length > 4 && <span className="caption">+{c.locations.length - 4}</span>}
        </dd>
      </dl>

      {result && (
        <div style={{ marginTop: 12 }}>
          <Notice tone={result.failed ? "warning" : "success"}>
            <div className="grid grid-stats">
              <Stat value={formatBytes(result.bytesSentToBin, lang)} label={t("cleanup.sentToBin")} />
              <Stat
                value={result.freeBefore != null && result.freeAfter != null ? formatBytes(result.freeAfter - result.freeBefore, lang) : "—"}
                label={t("cleanup.freed")}
              />
              <Stat value={formatBytes(result.bytesSentToBin, lang)} label={t("cleanup.stillUsed")} />
            </div>
            {result.messages.length > 0 && (
              <details className="disclosure">
                <summary>{t("backups.resultPartial", { done: result.recycled, failed: result.failed })}</summary>
                <ul>
                  {result.messages.slice(0, 50).map((m) => (
                    <li key={m} className="caption">
                      {m}
                    </li>
                  ))}
                </ul>
              </details>
            )}
          </Notice>
        </div>
      )}

      <div className="actions" style={{ marginTop: 12 }}>
        {c.action === "recycle" && c.fileCount > 0 && (
          <button className="btn btn-sm" aria-expanded={open} onClick={() => setOpen(!open)}>
            {open ? t("common.hide") : t("common.show")} ({formatNumber(c.files.length, lang)})
          </button>
        )}
        {c.action === "recycle" && sel.size > 0 && (
          <button className="btn btn-sm btn-primary" onClick={() => setConfirm("recycle")}>
            {t("cleanup.recycleSelected", { n: sel.size })}
          </button>
        )}
        {c.action === "emptyBin" && c.fileCount > 0 && (
          <button className="btn btn-sm btn-danger" onClick={() => setConfirm("empty")}>
            {t("cleanup.emptyBin")}
          </button>
        )}
        {c.action === "openStorageSettings" && (
          <button className="btn btn-sm" onClick={() => api.openStorageSettings().catch((e) => toast(errorText(e)))}>
            {t("cleanup.openStorage")}
          </button>
        )}
      </div>
      {c.action === "recycle" && c.fileCount === 0 && <p className="caption">{t("cleanup.noCandidates")}</p>}

      {open && (
        <div style={{ marginTop: 12 }}>
          <div className="row-between">
            {/* Select-all exists only for documented regenerable data. */}
            {c.confidence === "high" ? (
              <button className="btn btn-sm" onClick={() => setSel(new Set(c.files.map((f) => f.path)))}>
                {t("cleanup.selectAllInCategory", { n: c.files.length })}
              </button>
            ) : (
              <span />
            )}
            <span className="caption num">
              {t("common.selected", { n: sel.size })} · {formatBytes(selectedBytes, lang)}
            </span>
          </div>
          {c.files.length < c.fileCount && <p className="caption">{t("cleanup.showingFirst", { n: c.files.length })}</p>}
          <div style={{ maxHeight: 320, overflow: "auto", marginTop: 8 }}>
            <table className="table">
              <tbody>
                {c.files.map((f) => (
                  <tr key={f.path} aria-selected={sel.has(f.path)}>
                    <td className="shrink">
                      <input
                        type="checkbox"
                        aria-label={f.path}
                        checked={sel.has(f.path)}
                        onChange={(e) =>
                          setSel((s) => {
                            const n = new Set(s);
                            if (e.target.checked) n.add(f.path);
                            else n.delete(f.path);
                            return n;
                          })
                        }
                      />
                    </td>
                    <td>
                      <PathText path={f.path} />
                    </td>
                    <td className="shrink num caption">{formatBytes(f.size, lang)}</td>
                    <td className="shrink caption">{formatDate(f.mtime, lang)}</td>
                    <td className="shrink">
                      <PathActions path={f.path} />
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        </div>
      )}

      <ConfirmDialog
        open={confirm === "recycle"}
        title={t("cleanup.confirmTitle")}
        body={t("cleanup.confirmBody", { n: sel.size, size: formatBytes(selectedBytes, lang) })}
        confirmLabel={t("cleanup.recycleSelected", { n: sel.size })}
        busy={busy}
        onConfirm={doRecycle}
        onClose={() => setConfirm(null)}
      />
      <ConfirmDialog
        open={confirm === "empty"}
        title={t("cleanup.emptyBinTitle")}
        body={t("cleanup.emptyBinBody", { n: formatNumber(c.fileCount, lang), size: formatBytes(c.bytes, lang) })}
        confirmLabel={t("cleanup.emptyBin").replace("…", "")}
        danger
        busy={busy}
        onConfirm={doEmpty}
        onClose={() => setConfirm(null)}
      />
    </article>
  );
}

export function CleanupPage() {
  const t = useT();
  const [cats, setCats] = useState<CleanupCategory[] | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const analyze = async () => {
    setBusy(true);
    setError(null);
    try {
      setCats(await api.cleanupAnalyze());
    } catch (e) {
      setError(errorText(e));
    } finally {
      setBusy(false);
    }
  };
  return (
    <>
      <PageHeader
        title={t("cleanup.title")}
        subtitle={t("cleanup.subtitle")}
        actions={
          <button className="btn btn-primary" onClick={analyze} disabled={busy}>
            {cats ? t("common.refresh") : t("cleanup.analyze")}
          </button>
        }
      />
      <Notice>{t("cleanup.binNote")}</Notice>
      {busy && (
        <div className="card" style={{ marginTop: 16 }} aria-live="polite">
          <p>{t("cleanup.analyzing")}</p>
          <Meter label={t("cleanup.analyzing")} />
        </div>
      )}
      {error && <Notice tone="danger">{error}</Notice>}
      {!cats && !busy && (
        <div className="card" style={{ marginTop: 16 }}>
          <EmptyState icon={<Sparkles size={40} />} title={t("cleanup.title")} body={t("cleanup.subtitle")} action={<button className="btn btn-primary" onClick={analyze}>{t("cleanup.analyze")}</button>} />
        </div>
      )}
      <div className="stack" style={{ marginTop: 16 }}>
        {cats?.map((c) => (
          <CategoryCard key={c.id} c={c} onChanged={analyze} />
        ))}
      </div>
    </>
  );
}
