import { open } from "@tauri-apps/plugin-dialog";
import { useState } from "react";
import { api, errorText } from "../api";
import { ConfirmDialog, PageHeader, PathText, useAsync } from "../components/ui";
import { formatBytes } from "../format";
import { useLang, useT, type Key, type Lang } from "../i18n";
import { useApp, type Settings } from "../state";
import type { Profile } from "../types";

const DUP_SIZES = [0, 1024, 1024 * 1024, 10 * 1024 * 1024];

export function SettingsPage() {
  const t = useT();
  const lang = useLang();
  const { settings, updateSettings, toast, bumpIndex } = useApp();
  const info = useAsync(() => api.appInfo(), []);
  const [confirmReset, setConfirmReset] = useState(false);

  return (
    <>
      <PageHeader title={t("settings.title")} />
      <section className="card stack">
        <h2 className="section-title">{t("settings.general")}</h2>
        <div className="row" style={{ gap: 24, flexWrap: "wrap" }}>
          <label className="field">
            <span>{t("settings.language")}</span>
            <select className="select" value={settings.lang} onChange={(e) => updateSettings({ lang: e.target.value as Lang })}>
              <option value="es">Español</option>
              <option value="en">English</option>
            </select>
          </label>
          <label className="field">
            <span>{t("settings.theme")}</span>
            <select className="select" value={settings.theme} onChange={(e) => updateSettings({ theme: e.target.value as Settings["theme"] })}>
              <option value="system">{t("settings.theme.system")}</option>
              <option value="light">{t("settings.theme.light")}</option>
              <option value="dark">{t("settings.theme.dark")}</option>
            </select>
          </label>
        </div>
        {info.data && (
          <div>
            <span className="caption">{t("settings.dataDir")}</span>
            <PathText path={info.data.dataDir} />
          </div>
        )}
      </section>

      <section className="card stack">
        <h2 className="section-title">{t("settings.analysis")}</h2>
        <div className="grid grid-stats" role="radiogroup" aria-label={t("settings.profile")}>
          {(["low", "balanced", "max"] as Profile[]).map((p) => (
            <button key={p} className="choice" role="radio" aria-checked={settings.profile === p} aria-pressed={settings.profile === p} onClick={() => updateSettings({ profile: p })}>
              <div>
                <strong>{t(`settings.profile.${p}` as Key)}</strong>
                <span className="caption">{t(`settings.profile.${p}Desc` as Key)}</span>
              </div>
            </button>
          ))}
        </div>
        <label className="check">
          <input type="checkbox" checked={settings.includeHidden} onChange={(e) => updateSettings({ includeHidden: e.target.checked })} />
          {t("settings.hidden")}
        </label>
        <div>
          <div className="row-between">
            <span className="caption">{t("settings.exclusions")}</span>
            <button
              className="btn btn-sm"
              onClick={async () => {
                const p = await open({ directory: true });
                if (typeof p === "string" && !settings.exclusions.includes(p)) updateSettings({ exclusions: [...settings.exclusions, p] });
              }}
            >
              {t("settings.addExclusion")}
            </button>
          </div>
          {settings.exclusions.length === 0 && <p className="caption">{t("settings.noExclusions")}</p>}
          {settings.exclusions.map((x) => (
            <div key={x} className="row-between" style={{ padding: "4px 0" }}>
              <PathText path={x} />
              <button className="btn btn-sm btn-subtle" onClick={() => updateSettings({ exclusions: settings.exclusions.filter((y) => y !== x) })}>
                {t("scan.remove")}
              </button>
            </div>
          ))}
        </div>
      </section>

      <section className="card stack">
        <h2 className="section-title">{t("settings.duplicates")}</h2>
        <label className="field" style={{ maxWidth: 260 }}>
          <span>{t("settings.dupMinSize")}</span>
          <select className="select" value={settings.dupMinSize} onChange={(e) => updateSettings({ dupMinSize: Number(e.target.value) })}>
            {DUP_SIZES.map((s) => (
              <option key={s} value={s}>
                {s === 0 ? "—" : formatBytes(s, lang)}
              </option>
            ))}
          </select>
        </label>
        <p className="caption">{t("settings.dupBackupProtect")}</p>
      </section>

      <section className="card stack">
        <h2 className="section-title">{t("settings.backups")}</h2>
        <div className="row" style={{ gap: 24, flexWrap: "wrap" }}>
          <label className="field">
            <span>{t("settings.rootName")}</span>
            <input className="input" value={settings.rootName} onChange={(e) => updateSettings({ rootName: e.target.value })} />
          </label>
          <label className="field">
            <span>{t("backups.layout")}</span>
            <select className="select" value={settings.layout} onChange={(e) => updateSettings({ layout: e.target.value as Settings["layout"] })}>
              <option value="yearMonth">{t("backups.layout.yearMonth")}</option>
              <option value="year">{t("backups.layout.year")}</option>
            </select>
          </label>
        </div>
      </section>

      <section className="card stack">
        <h2 className="section-title">{t("settings.compression")}</h2>
        <label className="field" style={{ maxWidth: 260 }}>
          <span>{t("settings.compressionLevel")}</span>
          <select className="select" value={settings.compressionLevel} onChange={(e) => updateSettings({ compressionLevel: Number(e.target.value) })}>
            <option value={1}>{t("compress.level.fast")}</option>
            <option value={6}>{t("compress.level.balanced")}</option>
            <option value={9}>{t("compress.level.small")}</option>
          </select>
        </label>
        <p className="caption">{t("settings.verifyAlways")}</p>
      </section>

      <section className="card stack">
        <h2 className="section-title">{t("settings.privacy")}</h2>
        <p>{t("settings.privacyNote")}</p>
        <label className="check">
          <input type="checkbox" checked={settings.anonymizeReports} onChange={(e) => updateSettings({ anonymizeReports: e.target.checked })} />
          {t("dup.anonymize")}
        </label>
        <div className="row">
          <button className="btn" onClick={() => setConfirmReset(true)}>
            {t("settings.resetIndex")}
          </button>
          {info.data && <span className="caption">{t("settings.indexSize", { size: formatBytes(info.data.dbBytes, lang) })}</span>}
        </div>
      </section>

      <section className="card">
        <h2 className="section-title">{t("settings.about")}</h2>
        <p className="muted">
          DiskNido · {info.data ? t("settings.version", { v: info.data.version }) : ""} · MIT
        </p>
      </section>

      <ConfirmDialog
        open={confirmReset}
        title={t("settings.resetIndex")}
        body={t("settings.resetConfirm")}
        confirmLabel={t("settings.resetIndex")}
        onConfirm={() => {
          setConfirmReset(false);
          api.resetIndex().then(() => {
            toast(t("settings.resetDone"));
            bumpIndex();
            info.reload();
          }, (e) => toast(errorText(e)));
        }}
        onClose={() => setConfirmReset(false)}
      />
    </>
  );
}
