import { Cloud, File, Folder, Link2, ScanSearch, Search } from "lucide-react";
import { useEffect, useState } from "react";
import { api, errorText } from "../api";
import { Treemap } from "../components/Treemap";
import { Badge, EmptyState, Notice, PageHeader, PathActions, PathText, VirtualList, useAsync } from "../components/ui";
import { formatBytes, formatDate, formatNumber } from "../format";
import { useLang, useT, type Key } from "../i18n";
import { isRunning, useApp } from "../state";
import type { EntryView } from "../types";

const PAGE = 500;

function EntryIcon({ e }: { e: EntryView }) {
  if (e.isLink) return <Link2 size={16} aria-label="link" />;
  if (e.cloudOnly) return <Cloud size={16} aria-label="cloud" />;
  return e.isDir ? <Folder size={16} aria-hidden /> : <File size={16} aria-hidden />;
}

function Properties({ e }: { e: EntryView }) {
  const t = useT();
  const lang = useLang();
  const { navigate, settings, updateSettings, toast } = useApp();
  const types = useAsync(() => (e.isDir ? api.subtreeExtensions(e.id) : Promise.resolve([])), [e.id]);
  return (
    <div className="card stack">
      <div className="row-between">
        <h2 className="section-title" style={{ margin: 0, overflowWrap: "anywhere" }}>
          {e.name}
        </h2>
        <PathActions path={e.path} />
      </div>
      <dl className="kv">
        <dt>{t("common.size")}</dt>
        <dd className="num">{formatBytes(e.size, lang)}</dd>
        {e.isDir && (
          <>
            <dt>{t("explorer.contents")}</dt>
            <dd>{t("common.files", { n: formatNumber(e.fileCount, lang) })}</dd>
            <dt>{t("explorer.newest")}</dt>
            <dd>{formatDate(e.newestMtime, lang)}</dd>
          </>
        )}
        <dt>{t("common.modified")}</dt>
        <dd>{formatDate(e.mtime, lang)}</dd>
        <dt>{t("explorer.path")}</dt>
        <dd>{e.path}</dd>
      </dl>
      <div className="row">
        {e.cloudOnly && <Badge tone="accent">{t("explorer.cloudOnly")}</Badge>}
        {e.isLink && <Badge tone="warning">{t("explorer.link")}</Badge>}
      </div>
      {e.isDir && types.data && types.data.length > 0 && (
        <div>
          <span className="caption">{t("explorer.mainTypes")}</span>
          <ul style={{ margin: "4px 0 0", paddingLeft: 18 }}>
            {types.data.map((b) => (
              <li key={b.key} className="num">
                {b.key ? `.${b.key}` : "—"} · {formatBytes(b.bytes, lang)}
              </li>
            ))}
          </ul>
        </div>
      )}
      {e.isDir && (
        <div className="actions">
          <button className="btn btn-sm" onClick={() => navigate("duplicates", { scope: e.path })}>
            {t("explorer.findDuplicates")}
          </button>
          <button
            className="btn btn-sm"
            onClick={() => {
              api.setMark(e.path, "important").then(() => toast(t("settings.saved")), (err) => toast(errorText(err)));
            }}
          >
            {t("explorer.markImportant")}
          </button>
          <button
            className="btn btn-sm"
            onClick={() => {
              if (!settings.exclusions.includes(e.path)) updateSettings({ exclusions: [...settings.exclusions, e.path] });
              toast(t("explorer.excluded"));
            }}
          >
            {t("explorer.excludeFromAnalysis")}
          </button>
        </div>
      )}
    </div>
  );
}

export function ExplorerPage() {
  const t = useT();
  const lang = useLang();
  const { navigate, indexVersion, latest, settings } = useApp();
  const scanning = isRunning(latest("scan"));
  const [trail, setTrail] = useState<EntryView[]>([]);
  const [items, setItems] = useState<EntryView[]>([]);
  const [total, setTotal] = useState(0);
  const [selected, setSelected] = useState<EntryView | null>(null);
  const [view, setView] = useState<"map" | "list">("map");
  const [query, setQuery] = useState("");
  const [results, setResults] = useState<EntryView[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const overview = useAsync(() => (scanning ? Promise.resolve(null) : api.overview(settings.rootName)), [indexVersion, scanning, settings.rootName]);
  const largest = useAsync(() => (scanning ? Promise.resolve(null) : api.largest(50)), [indexVersion, scanning]);
  const current = trail[trail.length - 1];

  useEffect(() => {
    if (scanning) return;
    if (!current) {
      api.overview(settings.rootName).then(
        (o) => {
          setError(null);
          setItems(o.roots);
          setTotal(o.roots.length);
        },
        (e) => setError(errorText(e)),
      );
      return;
    }
    api.children(current.id, 0, PAGE).then(
      ([rows, n]) => {
        setError(null);
        setItems(rows);
        setTotal(n);
      },
      (e) => setError(errorText(e)),
    );
  }, [current, indexVersion, scanning, settings.rootName]);

  const loadMore = () =>
    current &&
    api.children(current.id, items.length, PAGE).then(([rows]) => setItems((x) => [...x, ...rows]), (e) => setError(errorText(e)));

  const open = (e: EntryView) => {
    setSelected(e);
    if (e.isDir && !e.isLink) setTrail((tr) => [...tr, e]);
  };

  const runSearch = async () => {
    if (query.trim().length < 2) return setResults(null);
    setResults(await api.search(query, 200));
  };

  if (scanning) return (<><PageHeader title={t("explorer.title")} subtitle={t("explorer.subtitle")} /><Notice>{t("common.busyScan")}</Notice></>);
  if (overview.data && !overview.data.lastScan)
    return (
      <>
        <PageHeader title={t("explorer.title")} subtitle={t("explorer.subtitle")} />
        <div className="card">
          <EmptyState icon={<ScanSearch size={40} />} title={t("common.needScan")} action={<button className="btn btn-primary" onClick={() => navigate("scan")}>{t("common.goScan")}</button>} />
        </div>
      </>
    );
  const maxSize = items[0]?.size || 1;

  return (
    <>
      <PageHeader
        title={t("explorer.title")}
        subtitle={t("explorer.subtitle")}
        actions={
          <form
            className="row"
            role="search"
            onSubmit={(e) => {
              e.preventDefault();
              runSearch().catch((err) => setError(errorText(err)));
            }}
          >
            <input className="input" value={query} onChange={(e) => setQuery(e.target.value)} placeholder={t("explorer.searchPlaceholder")} aria-label={t("common.search")} style={{ width: 280 }} />
            <button className="btn icon-btn" aria-label={t("common.search")} type="submit">
              <Search size={16} aria-hidden />
            </button>
          </form>
        }
      />
      {overview.data?.lastScan && <p className="caption">{t("common.fromScan", { date: formatDate(overview.data.lastScan.finishedAt, lang, true) })}</p>}
      {error && <Notice tone="danger">{error}</Notice>}

      {results && (
        <div className="card" style={{ marginBottom: 16 }}>
          <div className="row-between">
            <h2 className="section-title">{t("common.items", { n: results.length })}</h2>
            <button className="btn btn-sm" onClick={() => setResults(null)}>
              {t("common.close")}
            </button>
          </div>
          {results.map((r) => (
            <button key={r.id} className="list-row" onClick={() => setSelected(r)} aria-selected={selected?.id === r.id}>
              <EntryIcon e={r} />
              <PathText path={r.path} />
              <span className="num caption">{formatBytes(r.size, lang)}</span>
              <span className="caption">{formatDate(r.mtime, lang)}</span>
            </button>
          ))}
        </div>
      )}

      <div className="split">
        <div className="card" style={{ padding: 16 }}>
          <div className="row-between">
            <nav className="crumbs" aria-label="breadcrumb">
              <button aria-current={trail.length === 0 ? "page" : undefined} onClick={() => setTrail([])}>
                {t("common.allDrives")}
              </button>
              {trail.map((e, i) => (
                <span key={e.id} className="row" style={{ gap: 2 }}>
                  <span className="caption">›</span>
                  <button aria-current={i === trail.length - 1 ? "page" : undefined} onClick={() => setTrail(trail.slice(0, i + 1))}>
                    {e.name}
                  </button>
                </span>
              ))}
            </nav>
            <div className="segmented" role="group">
              <button aria-pressed={view === "map"} onClick={() => setView("map")}>
                {t("explorer.map")}
              </button>
              <button aria-pressed={view === "list"} onClick={() => setView("list")}>
                {t("explorer.list")}
              </button>
            </div>
          </div>
          {items.length === 0 && <p className="muted">{t("explorer.emptyFolder")}</p>}
          {view === "map" && items.length > 0 && <Treemap items={items} label={current?.name ?? t("common.allDrives")} onOpen={(it) => open(items.find((x) => x.id === it.id)!)} />}
          {view === "list" && items.length > 0 && (
            <VirtualList
              items={items}
              rowHeight={40}
              height={Math.min(520, items.length * 40)}
              label={current?.name ?? t("common.allDrives")}
              render={(e) => (
                <button className="list-row" aria-selected={selected?.id === e.id} onClick={() => setSelected(e)} onDoubleClick={() => open(e)} onKeyDown={(k) => k.key === "Enter" && open(e)}>
                  <EntryIcon e={e} />
                  <span style={{ minWidth: 0 }}>
                    <PathText path={e.name} />
                    <div className="bar-inline" style={{ width: `${Math.max(1, (e.size / maxSize) * 100)}%` }} />
                  </span>
                  <span className="num caption" style={{ textAlign: "right" }}>
                    {formatBytes(e.size, lang)}
                  </span>
                  <span className="caption">{formatDate(e.isDir ? e.newestMtime : e.mtime, lang)}</span>
                </button>
              )}
            />
          )}
          {items.length < total && (
            <button className="btn btn-sm" style={{ marginTop: 8 }} onClick={loadMore}>
              {t("explorer.more")} ({formatNumber(total - items.length, lang)})
            </button>
          )}
        </div>
        <div>{selected ? <Properties e={selected} /> : <div className="card muted">{t("explorer.selectHint")}</div>}</div>
      </div>

      <div className="grid" style={{ gridTemplateColumns: "1fr 1fr", marginTop: 16 }}>
        <div className="card">
          <h2 className="section-title">{t("explorer.byCategory")}</h2>
          <BucketBars buckets={(overview.data?.categories ?? []).map((b) => ({ ...b, label: t(`cat.${b.key}` as Key) }))} />
        </div>
        <div className="card">
          <h2 className="section-title">{t("explorer.byExtension")}</h2>
          <BucketBars buckets={(overview.data?.extensions ?? []).map((b) => ({ ...b, label: b.key ? `.${b.key}` : "—" }))} />
        </div>
        <TopList title={t("explorer.largestFolders")} items={largest.data?.[1] ?? []} onSelect={setSelected} />
        <TopList title={t("explorer.largestFiles")} items={largest.data?.[0] ?? []} onSelect={setSelected} />
      </div>
    </>
  );
}

function BucketBars({ buckets }: { buckets: { label: string; bytes: number; files: number }[] }) {
  const lang = useLang();
  const max = buckets[0]?.bytes || 1;
  return (
    <table className="table">
      <tbody>
        {buckets.slice(0, 12).map((b) => (
          <tr key={b.label}>
            <td style={{ width: "40%" }}>{b.label}</td>
            <td>
              <div className="bar-inline" style={{ width: `${Math.max(1, (b.bytes / max) * 100)}%`, height: 8 }} />
            </td>
            <td className="shrink num">{formatBytes(b.bytes, lang)}</td>
          </tr>
        ))}
      </tbody>
    </table>
  );
}

function TopList({ title, items, onSelect }: { title: string; items: EntryView[]; onSelect: (e: EntryView) => void }) {
  const lang = useLang();
  return (
    <div className="card">
      <h2 className="section-title">{title}</h2>
      <div style={{ maxHeight: 360, overflow: "auto" }}>
        {items.map((e) => (
          <button key={e.id} className="list-row" style={{ gridTemplateColumns: "24px minmax(0,1fr) 100px" }} onClick={() => onSelect(e)}>
            <EntryIcon e={e} />
            <PathText path={e.path} />
            <span className="num caption" style={{ textAlign: "right" }}>
              {formatBytes(e.size, lang)}
            </span>
          </button>
        ))}
      </div>
    </div>
  );
}
