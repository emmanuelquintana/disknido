import { FolderClock } from "lucide-react";
import { useState } from "react";
import { api, errorText } from "../api";
import { Badge, EmptyState, Notice, PageHeader, PathActions, PathText, useAsync } from "../components/ui";
import { formatBytes, formatDate, formatNumber } from "../format";
import { useLang, useT, type Key } from "../i18n";
import { isRunning, useApp } from "../state";

const THRESHOLDS = [6, 12, 24, 36];
const MIN_SIZES = [0, 10 * 1024 * 1024, 100 * 1024 * 1024, 1024 * 1024 * 1024];

export function OldFoldersPage() {
  const t = useT();
  const lang = useLang();
  const { indexVersion, latest, navigate, toast, settings, updateSettings } = useApp();
  const scanning = isRunning(latest("scan"));
  const [months, setMonths] = useState(12);
  const [custom, setCustom] = useState("");
  const [minBytes, setMinBytes] = useState(100 * 1024 * 1024);
  const effective = custom ? Math.max(1, Number(custom)) : months;
  const data = useAsync(() => (scanning ? Promise.resolve(null) : api.oldFolders(effective, minBytes)), [effective, minBytes, indexVersion, scanning]);
  const apps = (data.data ?? []).filter((f) => f.appReason).length;
  const shown = (data.data ?? []).filter((f) => settings.showInstalledApps || !f.appReason);

  const mark = (path: string, m: string | null) =>
    api.setMark(path, m).then(() => {
      if (m === "excluded" && !settings.exclusions.includes(path)) updateSettings({ exclusions: [...settings.exclusions, path] });
      data.reload();
    }, (e) => toast(errorText(e)));

  return (
    <>
      <PageHeader title={t("old.title")} subtitle={t("old.subtitle")} />
      <Notice>{t("old.note")}</Notice>
      {scanning && <Notice>{t("common.busyScan")}</Notice>}
      <div className="card" style={{ marginTop: 16 }}>
        <div className="row" style={{ gap: 16, flexWrap: "wrap", alignItems: "flex-end" }}>
          <div className="field">
            <span>{t("old.threshold")}</span>
            <div className="segmented" role="group">
              {THRESHOLDS.map((m) => (
                <button key={m} aria-pressed={!custom && months === m} onClick={() => { setCustom(""); setMonths(m); }}>
                  {t("old.months", { n: m })}
                </button>
              ))}
            </div>
          </div>
          <label className="field">
            <span>{t("old.months", { n: "…" })}</span>
            <input className="input num" style={{ width: 90 }} inputMode="numeric" value={custom} onChange={(e) => setCustom(e.target.value.replace(/\D/g, "").slice(0, 3))} />
          </label>
          <label className="field">
            <span>{t("old.minSize")}</span>
            <select className="select" value={minBytes} onChange={(e) => setMinBytes(Number(e.target.value))}>
              {MIN_SIZES.map((s) => (
                <option key={s} value={s}>
                  {s === 0 ? "—" : formatBytes(s, lang)}
                </option>
              ))}
            </select>
          </label>
          <label className="check">
            <input type="checkbox" checked={settings.showInstalledApps} onChange={(e) => updateSettings({ showInstalledApps: e.target.checked })} />
            {t("old.showApps", { n: apps })}
          </label>
        </div>
      </div>
      {data.error && <Notice tone="danger">{data.error}</Notice>}
      {data.data && shown.length === 0 && (
        <div className="card" style={{ marginTop: 16 }}>
          <EmptyState icon={<FolderClock size={40} />} title={t("old.none")} />
        </div>
      )}
      <div className="stack" style={{ marginTop: 16 }}>
        {data.loading && !data.data && (
          <div className="card" aria-live="polite">
            <p>{t("common.loading")}</p>
          </div>
        )}
        {shown.map((f) => (
          <article key={f.entry.id} className="card">
            <div className="row-between">
              <div style={{ minWidth: 0 }}>
                <strong className="path" title={f.entry.path}>
                  {f.entry.name}
                </strong>
                <PathText path={f.entry.path} className="caption" />
              </div>
              <div className="row">
                {f.appReason && <Badge tone="accent" title={f.appReason}>{t("old.appBadge")}</Badge>}
                <Badge tone="warning">{t("old.idle", { n: f.monthsIdle })}</Badge>
                {f.mark && <Badge tone="accent">{t(`old.${f.mark === "important" ? "markImportant" : f.mark === "favorite" ? "favorite" : "ignore"}` as Key)}</Badge>}
                <PathActions path={f.entry.path} />
              </div>
            </div>
            <dl className="kv" style={{ marginTop: 8 }}>
              <dt>{t("common.size")}</dt>
              <dd className="num">
                {formatBytes(f.entry.size, lang)} · {t("common.files", { n: formatNumber(f.entry.fileCount, lang) })}
              </dd>
              <dt>{t("old.newest")}</dt>
              <dd>{formatDate(f.entry.newestMtime, lang)}</dd>
              <dt>{t("old.oldest")}</dt>
              <dd>{formatDate(f.oldestMtime, lang)}</dd>
              <dt>{t("old.types")}</dt>
              <dd>{f.topExtensions.map((b) => `${b.key ? "." + b.key : "—"} (${formatBytes(b.bytes, lang)})`).join(" · ") || "—"}</dd>
              {f.protectedReason && (
                <>
                  <dt>{t("common.protected")}</dt>
                  <dd>{f.protectedReason}</dd>
                </>
              )}
              {f.appReason && (
                <>
                  <dt>{t("common.reason")}</dt>
                  <dd>{f.appReason}</dd>
                </>
              )}
            </dl>
            <div className="actions" style={{ marginTop: 12 }}>
              <button className="btn btn-sm" onClick={() => mark(f.entry.path, "important")}>
                {t("old.markImportant")}
              </button>
              <button className="btn btn-sm" onClick={() => mark(f.entry.path, "favorite")}>
                {t("old.favorite")}
              </button>
              <button className="btn btn-sm" onClick={() => mark(f.entry.path, "ignored")}>
                {t("old.ignore")}
              </button>
              <button className="btn btn-sm" onClick={() => mark(f.entry.path, "excluded")}>
                {t("old.exclude")}
              </button>
              {!f.protectedReason && !f.appReason && (
                <button className="btn btn-sm" onClick={() => navigate("backups", { tab: "compress", source: f.entry.path })}>
                  {t("old.compress")}
                </button>
              )}
              <button className="btn btn-sm" onClick={() => navigate("duplicates", { scope: f.entry.path })}>
                {t("old.reviewDup")}
              </button>
              <button className="btn btn-sm btn-subtle" onClick={() => navigate("backups")}>
                {t("old.organize")}
              </button>
            </div>
          </article>
        ))}
      </div>
    </>
  );
}
