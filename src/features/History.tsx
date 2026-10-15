import { History as HistoryIcon } from "lucide-react";
import { api, errorText } from "../api";
import { Badge, EmptyState, Notice, PageHeader, PathText, useAsync, type Tone } from "../components/ui";
import { formatBytes, formatDate, formatDuration } from "../format";
import { useLang, useT, type Key } from "../i18n";
import { useApp } from "../state";

const TONE: Record<string, Tone> = { done: "success", partial: "warning", failed: "danger", interrupted: "danger", cancelled: "neutral", running: "accent" };

export function HistoryPage() {
  const t = useT();
  const lang = useLang();
  const { toast, bumpIndex } = useApp();
  const ops = useAsync(() => api.history(200), []);
  const label = (prefix: string, v: string) => {
    const key = `${prefix}.${v}` as Key;
    const s = t(key);
    return s === key ? v : s;
  };
  return (
    <>
      <PageHeader title={t("history.title")} subtitle={t("history.subtitle")} actions={<button className="btn" onClick={ops.reload}>{t("common.refresh")}</button>} />
      {ops.error && <Notice tone="danger">{ops.error}</Notice>}
      {ops.data && ops.data.length === 0 && (
        <div className="card">
          <EmptyState icon={<HistoryIcon size={40} />} title={t("history.empty")} />
        </div>
      )}
      <div className="stack">
        {ops.data?.map((op) => (
          <article key={op.id} className="card">
            <div className="row-between">
              <div>
                <strong>{label("history.kind", op.kind)}</strong>
                <div className="caption">
                  {formatDate(op.createdAt, lang, true)}
                  {op.finishedAt && ` · ${t("history.duration")} ${formatDuration((op.finishedAt - op.createdAt) * 1000)}`}
                </div>
              </div>
              <div className="row">
                <Badge tone={TONE[op.status] ?? "neutral"}>{label("history.status", op.status)}</Badge>
                <Badge>{op.reversible ? t("history.reversible") : t("history.irreversible")}</Badge>
              </div>
            </div>
            <dl className="kv" style={{ marginTop: 8 }}>
              <dt>{t("common.size")}</dt>
              <dd className="num">{formatBytes(op.bytes, lang)}</dd>
              {op.freeBefore != null && (
                <>
                  <dt>{t("history.freeBefore")}</dt>
                  <dd className="num">{formatBytes(op.freeBefore, lang)}</dd>
                </>
              )}
              {op.freeAfter != null && (
                <>
                  <dt>{t("history.freeAfter")}</dt>
                  <dd className="num">{formatBytes(op.freeAfter, lang)}</dd>
                </>
              )}
              {op.error && (
                <>
                  <dt>{t("common.error")}</dt>
                  <dd>{op.error}</dd>
                </>
              )}
            </dl>
            {op.items.length > 0 && (
              <details className="disclosure">
                <summary>{t("common.items", { n: op.items.length })}</summary>
                <table className="table">
                  <tbody>
                    {op.items.map((i) => (
                      <tr key={i.seq}>
                        <td>
                          <PathText path={i.src} />
                          {i.dst && <PathText path={`→ ${i.dst}`} className="caption" />}
                          {i.error && <span className="caption">{i.error}</span>}
                        </td>
                        <td className="shrink">
                          <Badge tone={TONE[i.status] ?? "neutral"}>{label("history.status", i.status)}</Badge>
                        </td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </details>
            )}
            {op.kind === "organize" && op.items.some((i) => i.status === "done") && (
              <button
                className="btn btn-sm"
                style={{ marginTop: 8 }}
                onClick={() =>
                  api.organizeUndo(op.id).then((r) => {
                    toast(r.failed ? r.messages.join(" · ") : t("history.status.reverted"));
                    ops.reload();
                    bumpIndex();
                  }, (e) => toast(errorText(e)))
                }
              >
                {t("history.undo")}
              </button>
            )}
          </article>
        ))}
      </div>
    </>
  );
}
