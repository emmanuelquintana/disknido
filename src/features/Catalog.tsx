import { open, save } from "@tauri-apps/plugin-dialog";
import { FileArchive, Search, ShieldCheck } from "lucide-react";
import { useEffect, useState } from "react";
import { api, errorText } from "../api";
import { Badge, ConfirmDialog, EmptyState, Notice, PageHeader, PathActions, PathText, useAsync } from "../components/ui";
import { baseName, formatBytes, formatDate, formatNumber } from "../format";
import { useLang, useT } from "../i18n";
import { useApp } from "../state";
import type { ArchiveInfo, CatalogHit } from "../types";

function HitRow({ h }: { h: CatalogHit }) {
  const t = useT();
  const lang = useLang();
  return (
    <tr>
      <td>
        <strong className="path" title={h.relPath}>
          {h.name}
        </strong>
        <PathText path={h.relPath} className="caption" />
        <span className="caption">
          {t("catalog.inside")} <PathText path={h.archivePath} />
        </span>
      </td>
      <td className="shrink num caption">{formatBytes(h.size, lang)}</td>
      <td className="shrink caption">{formatDate(h.mtime, lang)}</td>
      <td className="shrink">
        <PathActions path={h.archivePath} />
      </td>
    </tr>
  );
}

function ArchiveCard({ a, open, onToggle, onChanged }: { a: ArchiveInfo; open: boolean; onToggle: () => void; onChanged: () => void }) {
  const t = useT();
  const lang = useLang();
  const { toast } = useApp();
  const [confirm, setConfirm] = useState(false);
  const entries = useAsync(() => (open ? api.catalogEntries(a.id, 0, 2000) : Promise.resolve(null)), [open, a.id]);
  const exportCsv = async () => {
    const path = await save({ defaultPath: `${baseName(a.path).replace(/\.zip$/i, "")}.csv`, filters: [{ name: "CSV", extensions: ["csv"] }] });
    if (path) api.catalogExport(a.id, path).then(() => toast(t("settings.saved")), (e) => toast(errorText(e)));
  };
  return (
    <article className="card">
      <div className="row-between">
        <div style={{ minWidth: 0 }}>
          <strong className="path" title={a.path}>
            {baseName(a.path)}
          </strong>
          <PathText path={a.path} className="caption" />
          {a.source && <PathText path={t("catalog.from", { path: a.source })} className="caption" />}
        </div>
        <div className="row">
          {a.verified ? (
            <Badge tone="success" title={t("catalog.origin.created")}>
              <ShieldCheck size={14} aria-hidden /> {t("compress.verifiedBadge")}
            </Badge>
          ) : (
            <Badge title={t("catalog.origin.imported")}>{t("catalog.origin.imported").split(" (")[0]}</Badge>
          )}
          {!a.exists && <Badge tone="warning">{t("catalog.missing")}</Badge>}
          <PathActions path={a.path} />
        </div>
      </div>
      <div className="caption num" style={{ marginTop: 6 }}>
        {t("common.files", { n: formatNumber(a.files, lang) })} · {formatBytes(a.bytes, lang)} → {formatBytes(a.archiveBytes, lang)} · {formatDate(a.createdAt, lang)}
      </div>
      <div className="actions" style={{ marginTop: 10 }}>
        <button className="btn btn-sm" aria-expanded={open} onClick={onToggle}>
          {open ? t("common.hide") : t("catalog.contents")}
        </button>
        <button className="btn btn-sm" onClick={exportCsv}>
          {t("catalog.export")}
        </button>
        <button className="btn btn-sm btn-subtle" onClick={() => setConfirm(true)}>
          {t("catalog.forget")}
        </button>
      </div>
      {open && entries.data && (
        <div style={{ maxHeight: 360, overflow: "auto", marginTop: 8 }}>
          <table className="table">
            <tbody>
              {entries.data.map((h) => (
                <tr key={h.relPath}>
                  <td>
                    <PathText path={h.relPath} />
                  </td>
                  <td className="shrink num caption">{formatBytes(h.size, lang)}</td>
                  <td className="shrink caption">{formatDate(h.mtime, lang)}</td>
                </tr>
              ))}
            </tbody>
          </table>
          {a.files > entries.data.length && <p className="caption">{t("cleanup.showingFirst", { n: entries.data.length })}</p>}
        </div>
      )}
      <ConfirmDialog
        open={confirm}
        title={t("catalog.forget")}
        body={t("catalog.forgetBody")}
        confirmLabel={t("catalog.forget")}
        onConfirm={() => {
          setConfirm(false);
          api.catalogForget(a.id).then(onChanged, (e) => toast(errorText(e)));
        }}
        onClose={() => setConfirm(false)}
      />
    </article>
  );
}

export function CatalogPage() {
  const t = useT();
  const lang = useLang();
  const { params, toast } = useApp();
  const list = useAsync(() => api.catalogList(), []);
  const [query, setQuery] = useState("");
  const [hits, setHits] = useState<CatalogHit[] | null>(null);
  const [openId, setOpenId] = useState<number | null>(params.archive ? Number(params.archive) : null);

  // Search as you type, debounced.
  useEffect(() => {
    const q = query.trim();
    const timer = setTimeout(() => {
      if (q.length < 2) setHits(null);
      else api.catalogSearch(q).then(setHits, (e) => toast(errorText(e)));
    }, 250);
    return () => clearTimeout(timer);
  }, [query, toast]);

  const importZips = async () => {
    const picked = await open({ multiple: true, filters: [{ name: "ZIP", extensions: ["zip"] }] });
    const paths = Array.isArray(picked) ? picked : picked ? [picked] : [];
    if (paths.length === 0) return;
    const res = await api.catalogImport(paths);
    const ok = res.filter((r) => "Ok" in r).length;
    const errors = res.flatMap((r) => ("Err" in r ? [r.Err] : []));
    toast([t("catalog.imported", { n: ok }), ...errors].join(" · "));
    list.reload();
  };

  return (
    <>
      <PageHeader
        title={t("catalog.title")}
        subtitle={t("catalog.subtitle")}
        actions={
          <button className="btn" onClick={() => importZips().catch((e) => toast(errorText(e)))}>
            <FileArchive size={16} aria-hidden /> {t("catalog.import")}
          </button>
        }
      />
      <div className="card">
        <label className="row" style={{ gap: 8 }}>
          <Search size={18} aria-hidden />
          <input
            className="input"
            style={{ flex: 1 }}
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            placeholder={t("catalog.search")}
            aria-label={t("catalog.search")}
            autoFocus
          />
        </label>
        <p className="caption">{t("catalog.searchHint")}</p>
        {hits && (
          <>
            <p className="caption num">{t("catalog.results", { n: formatNumber(hits.length, lang) })}</p>
            <div style={{ maxHeight: 420, overflow: "auto" }}>
              <table className="table">
                <tbody>
                  {hits.map((h) => (
                    <HitRow key={`${h.archiveId}:${h.relPath}`} h={h} />
                  ))}
                </tbody>
              </table>
            </div>
          </>
        )}
      </div>

      <section className="section" style={{ marginTop: 16 }}>
        <h2 className="section-title">{t("catalog.archives")}</h2>
        {list.error && <Notice tone="danger">{list.error}</Notice>}
        {list.data && list.data.length === 0 && (
          <div className="card">
            <EmptyState icon={<FileArchive size={40} />} title={t("catalog.none")} />
          </div>
        )}
        <div className="stack">
          {list.data?.map((a) => (
            <ArchiveCard key={a.id} a={a} open={openId === a.id} onToggle={() => setOpenId(openId === a.id ? null : a.id)} onChanged={list.reload} />
          ))}
        </div>
      </section>
    </>
  );
}
