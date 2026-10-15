// Small shared building blocks. Kept in one file on purpose: each is a few
// lines and they are always used together.
import { AlertTriangle, CheckCircle2, Copy, FolderOpen, Info, XCircle } from "lucide-react";
import { useEffect, useRef, useState, type ReactNode } from "react";
import { api, errorText } from "../api";
import { useT } from "../i18n";
import { useApp } from "../state";

export function PageHeader({ title, subtitle, actions }: { title: string; subtitle?: string; actions?: ReactNode }) {
  return (
    <header className="page-header">
      <div>
        <h1>{title}</h1>
        {subtitle && <p>{subtitle}</p>}
      </div>
      {actions && <div className="actions">{actions}</div>}
    </header>
  );
}

export type Tone = "neutral" | "success" | "warning" | "danger" | "accent";

export function Badge({ tone = "neutral", children, title }: { tone?: Tone; children: ReactNode; title?: string }) {
  return (
    <span className={`badge ${tone === "neutral" ? "" : tone}`} title={title}>
      {children}
    </span>
  );
}

/** Determinate when `value` is a number in 0..1, otherwise indeterminate (no fake %). */
export function Meter({ value, label, warn }: { value?: number | null; label: string; warn?: boolean }) {
  const known = typeof value === "number" && Number.isFinite(value);
  return (
    <div
      className={`meter ${known ? "" : "indeterminate"} ${warn ? "warn" : ""}`}
      role="progressbar"
      aria-label={label}
      aria-valuemin={known ? 0 : undefined}
      aria-valuemax={known ? 100 : undefined}
      aria-valuenow={known ? Math.round((value as number) * 100) : undefined}
    >
      <span style={known ? { width: `${Math.min(100, Math.max(0, (value as number) * 100))}%` } : undefined} />
    </div>
  );
}

export function Notice({ tone = "info", children }: { tone?: "info" | "warning" | "danger" | "success"; children: ReactNode }) {
  const Icon = tone === "warning" ? AlertTriangle : tone === "danger" ? XCircle : tone === "success" ? CheckCircle2 : Info;
  return (
    <div className={`notice ${tone}`} role={tone === "danger" ? "alert" : undefined}>
      <Icon size={18} aria-hidden />
      <div>{children}</div>
    </div>
  );
}

export function EmptyState({ icon, title, body, action }: { icon: ReactNode; title: string; body?: string; action?: ReactNode }) {
  return (
    <div className="empty">
      {icon}
      <h2>{title}</h2>
      {body && <p style={{ margin: 0, maxWidth: 460 }}>{body}</p>}
      {action}
    </div>
  );
}

export function Stat({ value, label }: { value: ReactNode; label: string }) {
  return (
    <div className="stat">
      <span className="value">{value}</span>
      <span className="label">{label}</span>
    </div>
  );
}

/** A path that truncates cleanly, with the full path as tooltip and an accessible label. */
export function PathText({ path, className }: { path: string; className?: string }) {
  return (
    <span className={`path ${className ?? ""}`} title={path}>
      {path}
    </span>
  );
}

export function PathActions({ path }: { path: string }) {
  const t = useT();
  const { toast } = useApp();
  return (
    <span className="row">
      <button
        className="btn btn-subtle btn-sm icon-btn"
        title={t("common.openLocation")}
        aria-label={t("common.openLocation")}
        onClick={(e) => {
          e.stopPropagation();
          api.revealPath(path).catch((err) => toast(errorText(err)));
        }}
      >
        <FolderOpen size={16} aria-hidden />
      </button>
      <button
        className="btn btn-subtle btn-sm icon-btn"
        title={t("common.copyPath")}
        aria-label={t("common.copyPath")}
        onClick={(e) => {
          e.stopPropagation();
          navigator.clipboard?.writeText(path).then(() => toast(t("common.copied")));
        }}
      >
        <Copy size={16} aria-hidden />
      </button>
    </span>
  );
}

/** Native <dialog>: focus trapping, Esc to close and inert background for free. */
export function ConfirmDialog(props: {
  open: boolean;
  title: string;
  body: ReactNode;
  confirmLabel: string;
  danger?: boolean;
  busy?: boolean;
  onConfirm: () => void;
  onClose: () => void;
}) {
  const t = useT();
  const ref = useRef<HTMLDialogElement>(null);
  useEffect(() => {
    const d = ref.current;
    if (!d) return;
    if (props.open && !d.open) {
      if (typeof d.showModal === "function") d.showModal();
      else d.setAttribute("open", "");
    } else if (!props.open && d.open) {
      if (typeof d.close === "function") d.close();
      else d.removeAttribute("open");
    }
  }, [props.open]);
  return (
    <dialog
      ref={ref}
      className="dialog"
      aria-labelledby="dlg-title"
      onCancel={(e) => {
        e.preventDefault();
        props.onClose();
      }}
    >
      {props.open && (
        <>
          <div className="dialog-body">
            <h2 id="dlg-title">{props.title}</h2>
            <div className="muted">{props.body}</div>
          </div>
          <div className="dialog-footer">
            <button className="btn" onClick={props.onClose} disabled={props.busy} autoFocus>
              {t("common.cancel")}
            </button>
            <button className={`btn ${props.danger ? "btn-danger" : "btn-primary"}`} onClick={props.onConfirm} disabled={props.busy}>
              {props.confirmLabel}
            </button>
          </div>
        </>
      )}
    </dialog>
  );
}

/** Loads data when deps change; keeps the previous value while reloading. */
export function useAsync<T>(fn: () => Promise<T>, deps: unknown[]) {
  const [state, setState] = useState<{ data: T | null; error: string | null; loading: boolean }>({
    data: null,
    error: null,
    loading: true,
  });
  const [nonce, setNonce] = useState(0);
  useEffect(() => {
    let alive = true;
    Promise.resolve()
      .then(() => {
        if (alive) setState((s) => ({ ...s, loading: true, error: null }));
        return fn();
      })
      .then((data) => alive && setState({ data, error: null, loading: false }))
      .catch((e) => alive && setState((s) => ({ ...s, error: errorText(e), loading: false })));
    return () => {
      alive = false;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [...deps, nonce]);
  return { ...state, reload: () => setNonce((n) => n + 1) };
}

/** Fixed-row-height virtual list: renders only what is visible. */
export function VirtualList<T>({
  items,
  rowHeight,
  height,
  render,
  label,
}: {
  items: T[];
  rowHeight: number;
  height: number;
  render: (item: T, index: number) => ReactNode;
  label: string;
}) {
  const [top, setTop] = useState(0);
  const first = Math.max(0, Math.floor(top / rowHeight) - 5);
  const count = Math.ceil(height / rowHeight) + 10;
  const slice = items.slice(first, first + count);
  return (
    <div className="scroll" style={{ height }} onScroll={(e) => setTop(e.currentTarget.scrollTop)} role="list" aria-label={label}>
      <div style={{ height: items.length * rowHeight, position: "relative" }}>
        {slice.map((it, i) => (
          <div key={first + i} role="listitem" style={{ position: "absolute", top: (first + i) * rowHeight, left: 0, right: 0, height: rowHeight }}>
            {render(it, first + i)}
          </div>
        ))}
      </div>
    </div>
  );
}
