import { Archive, Copy, Disc3, FolderClock, HardDrive, ScanSearch, Trash2 } from "lucide-react";
import { api } from "../api";
import { Badge, EmptyState, Meter, Notice, PageHeader, Stat, useAsync } from "../components/ui";
import { formatBytes, formatDate, formatNumber } from "../format";
import { useLang, useT } from "../i18n";
import { isRunning, useApp } from "../state";
import type { Volume } from "../types";

export function DriveCard({ v, onAnalyze, lastScanned }: { v: Volume; onAnalyze?: () => void; lastScanned?: number | null }) {
  const t = useT();
  const lang = useLang();
  const used = v.totalBytes - v.freeBytes;
  const frac = v.totalBytes ? used / v.totalBytes : 0;
  return (
    <div className="card drive-card">
      <div className="row-between">
        <div className="row">
          <HardDrive size={20} aria-hidden />
          <span className="letter">{v.letter}</span>
          <span className="muted">{v.label || v.fileSystem}</span>
        </div>
        <div className="row">
          {v.media !== "unknown" && <Badge>{t(v.media === "ssd" ? "scan.badge.ssd" : "scan.badge.hdd")}</Badge>}
          {v.external && <Badge tone="accent">{t("home.external")}</Badge>}
          {v.kind === "network" && <Badge tone="warning">{t("home.network")}</Badge>}
        </div>
      </div>
      <div style={{ margin: "14px 0 8px" }}>
        <Meter value={frac} label={`${v.letter} ${Math.round(frac * 100)}%`} warn={frac > 0.9} />
      </div>
      <div className="row-between">
        <span className="caption num">
          {v.available ? t("home.freeOf", { free: formatBytes(v.freeBytes, lang), total: formatBytes(v.totalBytes, lang) }) : t("home.unavailable")}
        </span>
        {v.diskNumber !== null && <span className="caption">{t("home.sameDisk", { n: v.diskNumber })}</span>}
      </div>
      <div className="row-between" style={{ marginTop: 12 }}>
        <span className="caption">{lastScanned ? t("home.lastScan", { date: formatDate(lastScanned, lang, true) }) : t("home.neverScanned")}</span>
        {onAnalyze && (
          <button className="btn btn-sm" onClick={onAnalyze} disabled={!v.available}>
            {t("home.analyze")}
          </button>
        )}
      </div>
    </div>
  );
}

export function HomePage() {
  const t = useT();
  const lang = useLang();
  const { navigate, settings, indexVersion, latest } = useApp();
  const vols = useAsync(() => api.listVolumes(), [indexVersion]);
  const scanning = isRunning(latest("scan"));
  const ov = useAsync(() => (scanning ? Promise.resolve(null) : api.overview(settings.rootName)), [indexVersion, scanning, settings.rootName]);
  const fd = useAsync(() => (scanning ? Promise.resolve(null) : api.findings(settings.rootName)), [indexVersion, scanning, settings.rootName]);
  const last = ov.data?.lastScan;
  // Drives can come from different scans: use each drive's latest indexed root.
  const scannedAt = new Map<string, number>();
  for (const r of ov.data?.roots ?? []) {
    const letter = r.name.slice(0, 2).toUpperCase();
    if (r.scannedAt != null && r.scannedAt > (scannedAt.get(letter) ?? 0)) scannedAt.set(letter, r.scannedAt);
  }

  return (
    <>
      <PageHeader
        title={t("home.title")}
        subtitle={t("home.subtitle")}
        actions={
          <button className="btn btn-primary" onClick={() => navigate("scan")}>
            <ScanSearch size={16} aria-hidden /> {t("home.start")}
          </button>
        }
      />
      <section className="section">
        <h2 className="section-title">{t("home.drives")}</h2>
        {vols.data && vols.data.length === 0 && <Notice tone="warning">{t("home.noDrives")}</Notice>}
        <div className="grid grid-drives">
          {(vols.data ?? []).map((v) => (
            <DriveCard
              key={v.root}
              v={v}
              lastScanned={scannedAt.get(v.letter.toUpperCase()) ?? null}
              onAnalyze={() => navigate("scan", { drive: v.root })}
            />
          ))}
        </div>
      </section>

      <section className="section" style={{ marginTop: 32 }}>
        <h2 className="section-title">{t("home.summary")}</h2>
        {scanning && <Notice>{t("common.busyScan")}</Notice>}
        {!scanning && ov.data && !last && (
          <div className="card">
            <EmptyState
              icon={<ScanSearch size={40} />}
              title={t("home.noScanTitle")}
              body={t("home.noScanBody")}
              action={
                <button className="btn btn-primary" onClick={() => navigate("scan")}>
                  {t("home.start")}
                </button>
              }
            />
          </div>
        )}
        {!scanning && ov.data && last && (
          <>
            <p className="caption" style={{ marginTop: 0 }}>
              {t("common.fromScan", { date: formatDate(last.finishedAt ?? last.startedAt, lang, true) })}
            </p>
            <div className="grid grid-stats">
              <SummaryCard icon={<Copy size={18} />} label={t("home.recoverable")} value={formatBytes(ov.data.duplicateBytes, lang)} sub={`${formatNumber(ov.data.duplicateGroups, lang)} · ${t("home.duplicates")}`} onClick={() => navigate("duplicates")} />
              <SummaryCard icon={<Archive size={18} />} label={t("home.backups")} value={fd.data ? formatNumber(fd.data.backups, lang) : "…"} sub={fd.data ? formatBytes(fd.data.backupBytes, lang) : t("common.loading")} onClick={() => navigate("backups")} />
              <SummaryCard icon={<Disc3 size={18} />} label={t("home.images")} value={fd.data ? formatNumber(fd.data.images, lang) : "…"} sub={fd.data ? formatBytes(fd.data.imageBytes, lang) : t("common.loading")} onClick={() => navigate("images")} />
              <SummaryCard icon={<FolderClock size={18} />} label={t("home.oldFolders")} value={fd.data ? formatNumber(fd.data.oldFolders, lang) : "…"} sub={fd.data ? formatBytes(fd.data.oldFolderBytes, lang) : t("common.loading")} onClick={() => navigate("oldFolders")} />
              <SummaryCard icon={<Trash2 size={18} />} label={t("home.recycleBin")} value={formatBytes(ov.data.recycleBin.bytes, lang)} sub={t("common.items", { n: formatNumber(ov.data.recycleBin.items, lang) })} onClick={() => navigate("cleanup")} />
            </div>
          </>
        )}
        {ov.error && <Notice tone="danger">{ov.error}</Notice>}
      </section>
    </>
  );
}

function SummaryCard({ icon, label, value, sub, onClick }: { icon: React.ReactNode; label: string; value: string; sub: string; onClick: () => void }) {
  return (
    <button className="card choice" onClick={onClick} style={{ display: "block" }}>
      <div className="row muted" style={{ marginBottom: 8 }}>
        {icon}
      </div>
      <Stat value={value} label={label} />
      <div className="caption" style={{ marginTop: 4 }}>
        {sub}
      </div>
    </button>
  );
}
