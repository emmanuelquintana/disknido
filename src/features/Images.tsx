import { Disc3 } from "lucide-react";
import { useMemo, useState } from "react";
import { api, errorText } from "../api";
import { Badge, ConfirmDialog, EmptyState, Notice, PageHeader, PathActions, PathText, useAsync } from "../components/ui";
import { formatBytes, formatDate } from "../format";
import { useLang, useT, type Key } from "../i18n";
import { isRunning, useApp } from "../state";
import type { DiskImage } from "../types";

type Sort = "size" | "format" | "date" | "name";

export function sortImages(list: DiskImage[], sort: Sort, dir: 1 | -1): DiskImage[] {
  const key = (i: DiskImage): number | string =>
    sort === "size" ? i.size : sort === "date" ? i.mtime : sort === "format" ? i.format : i.name.toLowerCase();
  return [...list].sort((a, b) => {
    const x = key(a);
    const y = key(b);
    return (x < y ? -1 : x > y ? 1 : 0) * dir;
  });
}

export function ImagesPage() {
  const t = useT();
  const lang = useLang();
  const { indexVersion, latest, toast, navigate, bumpIndex } = useApp();
  const scanning = isRunning(latest("scan"));
  const [readHeaders, setReadHeaders] = useState(false);
  const data = useAsync(() => (scanning ? Promise.resolve(null) : api.diskImages(readHeaders)), [indexVersion, readHeaders, scanning]);
  const [sort, setSort] = useState<Sort>("size");
  const [dir, setDir] = useState<1 | -1>(-1);
  const [drive, setDrive] = useState("");
  const [query, setQuery] = useState("");
  const [byFolder, setByFolder] = useState(false);
  const [sel, setSel] = useState<Set<string>>(new Set());
  const [orphans, setOrphans] = useState<string[] | null>(null);
  const [confirm, setConfirm] = useState(false);
  const [busy, setBusy] = useState(false);

  const all = useMemo(() => data.data ?? [], [data.data]);
  const drives = Array.from(new Set(all.map((i) => i.volume))).sort();
  const shown = useMemo(() => {
    const q = query.trim().toLowerCase();
    const filtered = all.filter((i) => (!drive || i.volume === drive) && (!q || i.name.toLowerCase().includes(q)));
    const sorted = sortImages(filtered, sort, dir);
    return byFolder ? [...sorted].sort((a, b) => a.folder.localeCompare(b.folder)) : sorted;
  }, [all, drive, query, sort, dir, byFolder]);

  const header = (s: Sort, label: string) => (
    <th aria-sort={sort === s ? (dir === 1 ? "ascending" : "descending") : "none"} className={s === "name" ? "" : "shrink"}>
      <button
        onClick={() => {
          if (sort === s) setDir(dir === 1 ? -1 : 1);
          else {
            setSort(s);
            setDir(s === "name" || s === "format" ? 1 : -1);
          }
        }}
      >
        {label} {sort === s ? (dir === 1 ? "↑" : "↓") : ""}
      </button>
    </th>
  );

  const askRecycle = async () => {
    try {
      setOrphans(await api.imageOrphans([...sel]));
      setConfirm(true);
    } catch (e) {
      toast(errorText(e));
    }
  };
  const doRecycle = async () => {
    setBusy(true);
    try {
      const targets = all.filter((i) => sel.has(i.path)).map((i) => ({ path: i.path, expected: { size: i.size, mtime: i.mtime } }));
      const r = await api.recycleImages(targets);
      toast(r.failed ? r.messages.join(" · ") : `${t("cleanup.sentToBin")}: ${formatBytes(r.bytesSentToBin, lang)}`);
      setSel(new Set());
      bumpIndex();
    } catch (e) {
      toast(errorText(e));
    } finally {
      setBusy(false);
      setConfirm(false);
    }
  };

  if (data.data && all.length === 0)
    return (
      <>
        <PageHeader title={t("images.title")} subtitle={t("images.subtitle")} />
        <div className="card">
          <EmptyState icon={<Disc3 size={40} />} title={t("images.none")} body={t("common.needScan")} action={<button className="btn" onClick={() => navigate("scan")}>{t("common.goScan")}</button>} />
        </div>
      </>
    );

  return (
    <>
      <PageHeader title={t("images.title")} subtitle={t("images.subtitle")} />
      {scanning && <Notice>{t("common.busyScan")}</Notice>}
      {data.error && <Notice tone="danger">{data.error}</Notice>}
      <div className="card">
        <div className="row" style={{ flexWrap: "wrap", gap: 12, alignItems: "flex-end" }}>
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
            <span>{t("common.search")}</span>
            <input className="input" value={query} onChange={(e) => setQuery(e.target.value)} />
          </label>
          <label className="check">
            <input type="checkbox" checked={byFolder} onChange={(e) => setByFolder(e.target.checked)} /> {t("images.byFolder")}
          </label>
          <label className="check">
            <input type="checkbox" checked={readHeaders} onChange={(e) => setReadHeaders(e.target.checked)} /> {t("images.readHeaders")}
          </label>
          <span style={{ flex: 1 }} />
          <button className="btn btn-primary" disabled={sel.size === 0} onClick={askRecycle}>
            {t("images.recycle", { n: sel.size })}
          </button>
        </div>
        <p className="caption">{t("images.notGame")}</p>
        <div style={{ maxHeight: 620, overflow: "auto" }}>
          <table className="table">
            <thead>
              <tr>
                <th className="shrink">
                  <span className="sr-only">{t("common.selected", { n: "" })}</span>
                </th>
                {header("name", t("common.name"))}
                {header("format", t("common.format"))}
                {header("size", t("common.size"))}
                {header("date", t("common.modified"))}
                <th className="shrink" />
              </tr>
            </thead>
            <tbody>
              {shown.map((i) => (
                <tr key={i.path} aria-selected={sel.has(i.path)}>
                  <td className="shrink">
                    <input
                      type="checkbox"
                      aria-label={i.path}
                      checked={sel.has(i.path)}
                      disabled={i.cloudOnly}
                      onChange={(e) =>
                        setSel((s) => {
                          const n = new Set(s);
                          if (e.target.checked) n.add(i.path);
                          else n.delete(i.path);
                          return n;
                        })
                      }
                    />
                  </td>
                  <td>
                    <strong className="path" title={i.path}>
                      {i.name}
                    </strong>
                    <PathText path={i.folder} className="caption" />
                    <span className="row" style={{ flexWrap: "wrap" }}>
                      {i.hint && (
                        <Badge tone="accent" title={i.evidence ?? undefined}>
                          {t(`images.kind.${i.kind}` as Key)}: {i.hint}
                        </Badge>
                      )}
                      {i.volumeLabel && <Badge title={t("images.label")}>{i.volumeLabel}</Badge>}
                      {i.related.length > 0 && <Badge title={i.related.join("\n")}>{t("images.setSize", { size: formatBytes(i.setSize, lang) })}</Badge>}
                      {i.missing.length > 0 && <Badge tone="warning">{t("images.missing", { list: i.missing.join(", ") })}</Badge>}
                      {i.cloudOnly && <Badge>{t("explorer.cloudOnly")}</Badge>}
                    </span>
                  </td>
                  <td className="shrink">{i.format}</td>
                  <td className="shrink num">{formatBytes(i.size, lang)}</td>
                  <td className="shrink caption">{formatDate(i.mtime, lang)}</td>
                  <td className="shrink">
                    <PathActions path={i.path} />
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      </div>
      <ConfirmDialog
        open={confirm}
        title={t("cleanup.confirmTitle")}
        body={
          <>
            {orphans && orphans.length > 0 && (
              <Notice tone="warning">
                <strong>{t("images.orphanTitle")}</strong>
                <p>{t("images.orphanBody")}</p>
                <ul>
                  {orphans.map((o) => (
                    <li key={o}>
                      <PathText path={o} />
                    </li>
                  ))}
                </ul>
              </Notice>
            )}
            <p>
              {t("cleanup.confirmBody", {
                n: sel.size,
                size: formatBytes(all.filter((i) => sel.has(i.path)).reduce((a, i) => a + i.size, 0), lang),
              })}
            </p>
          </>
        }
        confirmLabel={t("images.recycle", { n: sel.size })}
        busy={busy}
        onConfirm={doRecycle}
        onClose={() => setConfirm(false)}
      />
    </>
  );
}
