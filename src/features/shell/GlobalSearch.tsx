import { useEffect, useRef, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { BookOpen, Boxes, Car, Cog, Search, Truck, User, Wrench, ScanSearch, History } from "lucide-react";
import { call } from "../../api/ipc";
import type { Hit, SearchResult } from "../../api/types";
import { useApp, type Route } from "../../store";
import { useSettings } from "../../settings";

const KIND: Record<string, { label: string; icon: typeof Car; page: Route["page"] }> = {
  vehicle: { label: "Vehicles", icon: Car, page: "vehicle" },
  part: { label: "Parts", icon: Cog, page: "part" },
  item: { label: "Inventory", icon: Boxes, page: "item" },
  manual: { label: "Manuals", icon: BookOpen, page: "manual" },
  supplier: { label: "Suppliers", icon: Truck, page: "suppliers" },
  customer: { label: "Customers", icon: User, page: "customers" },
  job: { label: "Service jobs", icon: Wrench, page: "job" },
};

export function routeFor(hit: Hit): Route {
  const k = KIND[hit.kind];
  return { page: k?.page ?? "dashboard", id: hit.id };
}

function useDebounced<T>(v: T, ms: number): T {
  const [d, setD] = useState(v);
  useEffect(() => {
    const t = setTimeout(() => setD(v), ms);
    return () => clearTimeout(t);
  }, [v, ms]);
  return d;
}

/** Search everything from the title bar. Ctrl+K (⌘K) focuses it. A VIN opens the lookup, a
 * SKU, barcode or part number opens that record, words list matches by type. */
export default function GlobalSearch() {
  const go = useApp((s) => s.go);
  const { settings } = useSettings();
  const [q, setQ] = useState("");
  const [open, setOpen] = useState(false);
  const [active, setActive] = useState(0);
  const input = useRef<HTMLInputElement>(null);
  const dq = useDebounced(q.trim(), 140);
  const res = useQuery<SearchResult>({ queryKey: ["search", dq], queryFn: () => call<SearchResult>("search", { query: dq, limit: 24 }), enabled: dq.length > 0 });
  const mac = navigator.platform.toLowerCase().includes("mac");

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "k") {
        e.preventDefault();
        input.current?.focus();
        input.current?.select();
      }
    };
    addEventListener("keydown", onKey);
    return () => removeEventListener("keydown", onKey);
  }, []);
  useEffect(() => setActive(0), [dq]);

  const data = dq && res.data?.query === dq ? res.data : null;
  const hits = data?.hits ?? [];
  const recents = !q.trim() ? (settings?.recent_searches ?? []) : [];

  const finish = (r: Route, remember = q.trim()) => {
    if (remember) void call("remember_search", { query: remember }).catch(() => undefined);
    setOpen(false);
    setQ("");
    input.current?.blur();
    go(r);
  };
  const routeOf = (s: SearchResult): Route | null => {
    const r = s.route;
    if (!r) return null;
    if (r.kind === "vin") return r.vehicle_id ? { page: "vehicle", id: r.vehicle_id } : { page: "lookup", arg: r.vin };
    return { page: r.kind === "part" ? "part" : "item", id: r.id };
  };
  const direct = data ? routeOf(data) : null;
  const options: { key: string; label: string; sub?: string; icon: typeof Car; run: () => void }[] = [];
  if (direct && data?.route) {
    const r = data.route;
    options.push({
      key: "direct",
      label: r.kind === "vin" ? (r.vehicle_id ? `Open vehicle ${r.vin}` : `Look up VIN ${r.vin}`) : r.kind === "part" ? "Open this part number" : "Open this stock item",
      sub: "Exact match",
      icon: r.kind === "vin" ? ScanSearch : r.kind === "part" ? Cog : Boxes,
      run: () => finish(direct),
    });
  }
  hits.forEach((h) => options.push({ key: `${h.kind}:${h.id}`, label: h.title, sub: h.subtitle, icon: KIND[h.kind]?.icon ?? Search, run: () => finish(routeFor(h)) }));
  recents.forEach((r) => options.push({ key: `recent:${r}`, label: r, sub: "Recent search", icon: History, run: () => setQ(r) }));

  const onKey = (e: React.KeyboardEvent) => {
    if (e.key === "ArrowDown") {
      e.preventDefault();
      setActive((a) => Math.min(options.length - 1, a + 1));
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      setActive((a) => Math.max(0, a - 1));
    } else if (e.key === "Enter") {
      e.preventDefault();
      if (options[active]) options[active].run();
      else if (q.trim()) finish({ page: "lookup", arg: q.trim() });
    } else if (e.key === "Escape") {
      setOpen(false);
      input.current?.blur();
    }
  };

  let lastGroup = "";
  const showing = open && (options.length > 0 || (!!dq && !!data));
  return (
    <div className="searchbox" role="search">
      <Search size={15} aria-hidden />
      <input
        ref={input}
        type="search"
        value={q}
        placeholder="Search VIN, vehicle, part number, SKU…"
        aria-label="Search everything"
        aria-expanded={showing}
        aria-controls={showing ? "search-results" : undefined}
        aria-activedescendant={showing && options[active] ? `sr-${active}` : undefined}
        role="combobox"
        aria-autocomplete="list"
        onChange={(e) => {
          setQ(e.target.value);
          setOpen(true);
        }}
        onFocus={() => setOpen(true)}
        onBlur={() => setTimeout(() => setOpen(false), 150)}
        onKeyDown={onKey}
      />
      {!q && <kbd aria-hidden>{mac ? "⌘K" : "Ctrl K"}</kbd>}
      {showing && (
        <div className="results" id="search-results" role="listbox" aria-label="Search results">
          {options.map((o, i) => {
            const group = o.key.split(":")[0];
            const header = group !== lastGroup && group !== "direct" ? (KIND[group]?.label ?? (group === "recent" ? "Recent searches" : "")) : "";
            lastGroup = group;
            const Icon = o.icon;
            return (
              <div key={o.key}>
                {header && <p className="label">{header}</p>}
                <button id={`sr-${i}`} role="option" aria-selected={i === active} onMouseDown={(e) => e.preventDefault()} onClick={o.run} onMouseEnter={() => setActive(i)}>
                  <Icon size={16} aria-hidden />
                  <span className="min-w-0 flex-1 truncate">{o.label}</span>
                  {o.sub && <span className="small muted truncate" style={{ maxWidth: "45%" }}>{o.sub}</span>}
                </button>
              </div>
            );
          })}
          {data && options.length === 0 && (
            <div className="p-3 small muted">
              Nothing found for “{dq}”.
              {data.suggestion && (
                <>
                  {" "}
                  Did you mean{" "}
                  <button className="btn link" style={{ display: "inline", width: "auto", padding: 0 }} onMouseDown={(e) => e.preventDefault()} onClick={() => setQ(data.suggestion!)}>
                    {data.suggestion}
                  </button>
                  ?
                </>
              )}
            </div>
          )}
        </div>
      )}
    </div>
  );
}
