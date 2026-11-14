// The Simple Suite component kit for Auto Manager: native <dialog> modals, badges, empty and
// error states (as in Simple Setup), Radix menus (as in the Simple Office apps), and a
// virtualized, sortable data table for large inventories.
import { useEffect, useId, useMemo, useRef, useState, type KeyboardEvent, type ReactNode } from "react";
import { ContextMenu, DropdownMenu } from "radix-ui";
import { useVirtualizer } from "@tanstack/react-virtual";
import { ArrowDown, ArrowUp, CircleAlert, CloudOff, MoreHorizontal, X } from "lucide-react";
import type { UseQueryResult } from "@tanstack/react-query";
import { errorMessage, isAppError } from "../api/ipc";
import { useApp } from "../store";
import type { FitStatus } from "../api/bindings/FitStatus";

export const cx = (...c: (string | false | null | undefined)[]) => c.filter(Boolean).join(" ");

export function Badge({ tone = "neutral", children, title }: { tone?: "neutral" | "ok" | "warn" | "bad" | "info"; children: ReactNode; title?: string }) {
  return (
    <span className={`badge ${tone}`} title={title}>
      {children}
    </span>
  );
}

const FIT_LABEL: Record<FitStatus, string> = {
  confirmed: "Confirmed fit",
  conditional: "Conditional fit",
  possible: "Possible fit",
  not_compatible: "Not compatible",
  unknown: "Unknown",
};
export const fitLabel = (s: FitStatus) => FIT_LABEL[s];

export function FitBadge({ status }: { status: FitStatus }) {
  return <span className={`fit ${status}`}>{FIT_LABEL[status]}</span>;
}

export function StockBadge({ available, min }: { available: number; min?: number }) {
  if (available <= 0) return <Badge tone="bad">Out of stock</Badge>;
  if (min && available <= min) return <Badge tone="warn">Low · {available}</Badge>;
  return <Badge tone="ok">In stock · {available}</Badge>;
}

export function Spinner({ label }: { label?: string }) {
  return <span className="spinner" role={label ? "status" : undefined} aria-label={label} />;
}

/** An empty list: what is missing and what to do about it. */
export function Empty({ icon, title, children, action }: { icon: ReactNode; title: string; children?: ReactNode; action?: ReactNode }) {
  return (
    <div className="empty">
      <span className="tile" aria-hidden>
        {icon}
      </span>
      <strong>{title}</strong>
      {children && <p>{children}</p>}
      {action && <div className="mt-2">{action}</div>}
    </div>
  );
}

export function ErrorNote({ error }: { error: unknown }) {
  if (!error) return null;
  const offline = isAppError(error) && error.kind === "offline";
  return (
    <div className={`notice ${offline ? "warn" : "bad"} flex gap-2 items-start`} role="alert">
      {offline ? <CloudOff size={16} aria-hidden className="mt-0.5 flex-none" /> : <CircleAlert size={16} aria-hidden className="mt-0.5 flex-none" />}
      <span>{errorMessage(error)}</span>
    </div>
  );
}

/** Loading, error and data states for one query. */
export function Loaded<T>({ q, children, label = "Loading" }: { q: UseQueryResult<T>; children: (data: T) => ReactNode; label?: string }) {
  if (q.isPending) {
    return (
      <div className="flex items-center gap-2 text-text-2 py-6">
        <Spinner label={label} /> {label}…
      </div>
    );
  }
  if (q.isError) return <ErrorNote error={q.error} />;
  return <>{children(q.data as T)}</>;
}

export function PageHead({ title, sub, children, back }: { title: ReactNode; sub?: ReactNode; children?: ReactNode; back?: boolean }) {
  const goBack = useApp((s) => s.back);
  return (
    <div className="page-head">
      <div className="min-w-0">
        {back && (
          <button className="btn link mb-1" style={{ paddingLeft: 0 }} onClick={goBack}>
            ← Back
          </button>
        )}
        <h1>{title}</h1>
        {sub && <p>{sub}</p>}
      </div>
      {children && <div className="row">{children}</div>}
    </div>
  );
}

/** Native modal dialog: focus trapping, Escape to close and the backdrop come from the browser. */
export function Modal({ open, title, onClose, children, footer, wide }: { open: boolean; title: string; onClose: () => void; children: ReactNode; footer?: ReactNode; wide?: boolean }) {
  const ref = useRef<HTMLDialogElement>(null);
  const id = useId();
  useEffect(() => {
    const d = ref.current;
    if (!d) return;
    if (open && !d.open) d.showModal?.();
    if (!open && d.open) d.close();
  }, [open]);
  useEffect(() => () => ref.current?.close?.(), []);
  return (
    <dialog ref={ref} onClose={onClose} aria-labelledby={id} className={wide ? "wide" : undefined}>
      {open && (
        <>
          <div className="dialog-head">
            <h2 id={id}>{title}</h2>
            <button className="icon-btn" aria-label="Close" title="Close" onClick={() => ref.current?.close()}>
              <X size={16} aria-hidden />
            </button>
          </div>
          <div className="dialog-body">{children}</div>
          {footer && <div className="dialog-foot">{footer}</div>}
        </>
      )}
    </dialog>
  );
}

/** A labelled field. The error (from the core's `field`) appears under it. */
export function Field({ label, hint, error, children, className }: { label: string; hint?: ReactNode; error?: string | null; children: ReactNode; className?: string }) {
  return (
    <label className={cx("field", className)}>
      <span>{label}</span>
      {children}
      {hint && !error && <small className="small muted">{hint}</small>}
      {error && (
        <small className="small" style={{ color: "var(--danger)" }} role="alert">
          {error}
        </small>
      )}
    </label>
  );
}

export interface TabDef {
  id: string;
  label: string;
  count?: number;
}

/** A tab row; arrow keys move between tabs. */
export function Tabs({ tabs, value, onChange, label }: { tabs: TabDef[]; value: string; onChange: (id: string) => void; label: string }) {
  const onKey = (e: KeyboardEvent<HTMLDivElement>) => {
    const i = tabs.findIndex((t) => t.id === value);
    const next = e.key === "ArrowRight" ? i + 1 : e.key === "ArrowLeft" ? i - 1 : e.key === "Home" ? 0 : e.key === "End" ? tabs.length - 1 : null;
    if (next == null) return;
    e.preventDefault();
    const t = tabs[(next + tabs.length) % tabs.length];
    onChange(t.id);
    (e.currentTarget.querySelector(`[data-tab="${t.id}"]`) as HTMLElement | null)?.focus();
  };
  return (
    <div className="tabs" role="tablist" aria-label={label} onKeyDown={onKey}>
      {tabs.map((t) => (
        <button key={t.id} data-tab={t.id} role="tab" aria-selected={t.id === value} tabIndex={t.id === value ? 0 : -1} onClick={() => onChange(t.id)}>
          {t.label}
          {t.count != null && <span className="count">{t.count}</span>}
        </button>
      ))}
    </div>
  );
}

export function Toasts() {
  const { toasts, dismiss } = useApp();
  return (
    <div className="toasts" role="status" aria-live="polite">
      {toasts.map((t) => (
        <div key={t.id} className={cx("toast", t.tone === "bad" && "bad")}>
          <span>{t.message}</span>
          {t.action && (
            <button
              onClick={() => {
                t.action?.run();
                dismiss(t.id);
              }}
            >
              {t.action.label}
            </button>
          )}
          <button aria-label="Dismiss" onClick={() => dismiss(t.id)}>
            <X size={14} aria-hidden />
          </button>
        </div>
      ))}
    </div>
  );
}

export interface MenuItem {
  label: string;
  icon?: ReactNode;
  run: () => void;
  danger?: boolean;
  disabled?: boolean;
}

export function ActionsMenu({ items, label = "More actions" }: { items: (MenuItem | null)[]; label?: string }) {
  return (
    <DropdownMenu.Root modal={false}>
      <DropdownMenu.Trigger asChild>
        <button className="icon-btn" aria-label={label} title={label}>
          <MoreHorizontal size={16} aria-hidden />
        </button>
      </DropdownMenu.Trigger>
      <DropdownMenu.Portal>
        <DropdownMenu.Content className="menu" sideOffset={6} align="end" collisionPadding={8}>
          {items.map((it, i) =>
            it ? (
              <DropdownMenu.Item key={i} className={it.danger ? "danger" : undefined} disabled={it.disabled} onSelect={it.run}>
                {it.icon}
                {it.label}
              </DropdownMenu.Item>
            ) : (
              <DropdownMenu.Separator key={i} className="sep" />
            ),
          )}
        </DropdownMenu.Content>
      </DropdownMenu.Portal>
    </DropdownMenu.Root>
  );
}

export function RowMenu({ items, children }: { items: (MenuItem | null)[]; children: ReactNode }) {
  return (
    <ContextMenu.Root modal={false}>
      <ContextMenu.Trigger asChild>{children}</ContextMenu.Trigger>
      <ContextMenu.Portal>
        <ContextMenu.Content className="menu" collisionPadding={8}>
          {items.map((it, i) =>
            it ? (
              <ContextMenu.Item key={i} className={it.danger ? "danger" : undefined} disabled={it.disabled} onSelect={it.run}>
                {it.icon}
                {it.label}
              </ContextMenu.Item>
            ) : (
              <ContextMenu.Separator key={i} className="sep" />
            ),
          )}
        </ContextMenu.Content>
      </ContextMenu.Portal>
    </ContextMenu.Root>
  );
}

export interface Column<T> {
  id: string;
  label: string;
  width: string;
  render: (row: T) => ReactNode;
  /** Sort key; columns without one aren't sortable. */
  sort?: (row: T) => string | number;
  align?: "right";
}

/**
 * A virtualized table: only visible rows are in the DOM, so tens of thousands of items scroll
 * smoothly. Click a header to sort; Enter or double-click opens a row; right-click for actions.
 */
export function DataTable<T>({
  rows,
  columns,
  rowKey,
  onOpen,
  menu,
  height = "calc(100vh - 290px)",
  label,
  footer,
  rowHeight = 40,
}: {
  rows: T[];
  columns: Column<T>[];
  rowKey: (r: T) => string;
  onOpen?: (r: T) => void;
  menu?: (r: T) => (MenuItem | null)[];
  height?: string;
  label: string;
  footer?: ReactNode;
  rowHeight?: number;
}) {
  const [sort, setSort] = useState<{ id: string; dir: 1 | -1 } | null>(null);
  const [active, setActive] = useState(0);
  const scroller = useRef<HTMLDivElement>(null);
  const sorted = useMemo(() => {
    const col = columns.find((c) => c.id === sort?.id);
    if (!col?.sort || !sort) return rows;
    const key = col.sort;
    return [...rows].sort((a, b) => {
      const x = key(a);
      const y = key(b);
      return (typeof x === "number" && typeof y === "number" ? x - y : String(x).localeCompare(String(y), undefined, { numeric: true, sensitivity: "base" })) * sort.dir;
    });
  }, [rows, columns, sort]);
  const v = useVirtualizer({ count: sorted.length, getScrollElement: () => scroller.current, estimateSize: () => rowHeight, overscan: 12 });
  const template = columns.map((c) => c.width).join(" ");
  const onKey = (e: KeyboardEvent) => {
    if (e.key === "ArrowDown" || e.key === "ArrowUp") {
      e.preventDefault();
      const n = Math.max(0, Math.min(sorted.length - 1, active + (e.key === "ArrowDown" ? 1 : -1)));
      setActive(n);
      v.scrollToIndex(n);
    } else if (e.key === "Enter" && sorted[active] && onOpen) {
      onOpen(sorted[active]);
    }
  };
  return (
    <div className="grid-table" style={{ height }}>
      <div className="scroller" ref={scroller} role="grid" aria-label={label} aria-rowcount={sorted.length} tabIndex={0} onKeyDown={onKey}>
        <div className="gt-row gt-head" role="row" style={{ gridTemplateColumns: template }}>
          {columns.map((c) => (
            <div key={c.id} role="columnheader" className={c.align === "right" ? "num" : undefined} aria-sort={sort?.id === c.id ? (sort.dir === 1 ? "ascending" : "descending") : undefined}>
              {c.sort ? (
                <button onClick={() => setSort(sort?.id === c.id ? (sort.dir === 1 ? { id: c.id, dir: -1 } : null) : { id: c.id, dir: 1 })}>
                  {c.label}
                  {sort?.id === c.id && (sort.dir === 1 ? <ArrowUp size={12} aria-hidden /> : <ArrowDown size={12} aria-hidden />)}
                </button>
              ) : (
                c.label
              )}
            </div>
          ))}
        </div>
        <div className="gt-body" style={{ height: v.getTotalSize(), position: "relative" }}>
          {v.getVirtualItems().map((vi) => {
            const r = sorted[vi.index];
            const row = (
              <div
                key={rowKey(r)}
                role="row"
                aria-rowindex={vi.index + 2}
                aria-selected={vi.index === active}
                className="gt-row"
                style={{ gridTemplateColumns: template, top: vi.start, height: vi.size }}
                onClick={() => setActive(vi.index)}
                onDoubleClick={() => onOpen?.(r)}
              >
                {columns.map((c) => (
                  <div key={c.id} role="gridcell" className={c.align === "right" ? "num" : undefined}>
                    {c.render(r)}
                  </div>
                ))}
              </div>
            );
            return menu ? (
              <RowMenu key={rowKey(r)} items={menu(r)}>
                {row}
              </RowMenu>
            ) : (
              row
            );
          })}
        </div>
      </div>
      {footer !== undefined && <div className="gt-foot">{footer}</div>}
    </div>
  );
}

/** The app icon, simplified for small sizes (src-tauri/icons/icon.svg is the full one). */
export function Logo({ size = 22 }: { size?: number }) {
  return (
    <svg width={size} height={size} viewBox="64 64 896 896" aria-hidden focusable="false">
      <defs>
        <linearGradient id="logo-tile" x1="0" y1="0" x2="1" y2="1">
          <stop offset="0" stopColor="#22b8cf" />
          <stop offset=".5" stopColor="#0e7490" />
          <stop offset="1" stopColor="#164e63" />
        </linearGradient>
      </defs>
      <rect x="64" y="64" width="896" height="896" rx="200" fill="url(#logo-tile)" />
      <rect x="224" y="232" width="576" height="560" rx="52" fill="#fff" />
      <g fill="none" stroke="#0e7490" strokeLinecap="round">
        <circle cx="512" cy="512" r="186" strokeWidth="58" />
        <path d="M338 486H448M576 486H686M512 560V690" strokeWidth="54" />
      </g>
      <circle cx="512" cy="500" r="62" fill="#0e7490" />
    </svg>
  );
}

/** Ask before something that can't be undone. Resolves true when confirmed. */
export function useConfirm() {
  const [state, setState] = useState<{ title: string; body: ReactNode; action: string; resolve: (ok: boolean) => void } | null>(null);
  const ask = (title: string, body: ReactNode, action = "Delete") => new Promise<boolean>((resolve) => setState({ title, body, action, resolve }));
  const close = (ok: boolean) => {
    state?.resolve(ok);
    setState(null);
  };
  const dialog = (
    <Modal
      open={!!state}
      title={state?.title ?? ""}
      onClose={() => close(false)}
      footer={
        <>
          <button className="btn" onClick={() => close(false)}>
            Cancel
          </button>
          <button className="btn danger" onClick={() => close(true)} autoFocus>
            {state?.action}
          </button>
        </>
      }
    >
      <div>{state?.body}</div>
    </Modal>
  );
  return { ask, dialog };
}

export function ProgressBar({ value, max, label }: { value: number; max: number; label: string }) {
  const pct = max > 0 ? Math.min(100, Math.round((value / max) * 100)) : 0;
  return (
    <div className="progress" role="progressbar" aria-label={label} aria-valuemin={0} aria-valuemax={100} aria-valuenow={pct}>
      <div style={{ width: `${pct}%` }} />
    </div>
  );
}

/** A link that opens in the system browser (the app window never navigates away). */
export function ExtLink({ href, children, className = "btn link" }: { href: string; children: ReactNode; className?: string }) {
  return (
    <a
      className={className}
      href={href}
      onClick={(e) => {
        e.preventDefault();
        void import("@tauri-apps/plugin-opener").then((m) => m.openUrl(href)).catch(() => undefined);
      }}
    >
      {children}
    </a>
  );
}
