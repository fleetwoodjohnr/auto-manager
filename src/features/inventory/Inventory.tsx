import { useEffect, useRef, useState } from "react";
import { Barcode, Boxes, Download, FileUp, ListChecks, PackagePlus, Printer, Truck } from "lucide-react";
import { save as saveFile } from "@tauri-apps/plugin-dialog";
import { call, errorMessage } from "../../api/ipc";
import { useAction, useCmd } from "../../api/queries";
import type { Category, CountResult, Item, Location, Movement, Reservation, SearchResult } from "../../api/types";
import { useApp, toast } from "../../store";
import { Badge, DataTable, Empty, Loaded, PageHead, StockBadge, Tabs } from "../../ui";
import { ago, dateTime, money, qty } from "../../lib/format";
import { NewItem } from "./ItemForm";
import { ImportInventory } from "./ImportInventory";
import { movementLabel } from "../dashboard/Dashboard";
import { Purchasing } from "./Purchasing";
import { Labels } from "./Labels";
import { SavedSearches } from "../shared/SavedSearches";

export default function Inventory() {
  const { route, setTab, go } = useApp();
  const tab = route.tab === "new" ? "all" : (route.tab ?? "all");
  const [adding, setAdding] = useState(route.tab === "new");
  const [importing, setImporting] = useState(false);
  const [labels, setLabels] = useState<Item[] | null>(null);
  return (
    <div className="page wide">
      <PageHead title="Inventory" sub="What you have, where it is, and every movement in and out.">
        <button className="btn" onClick={() => go("suppliers")}>
          <Truck size={16} aria-hidden /> Suppliers
        </button>
        <button className="btn" onClick={() => setImporting(true)}>
          <FileUp size={16} aria-hidden /> Import
        </button>
        <button
          className="btn"
          onClick={async () => {
            const path = await saveFile({ defaultPath: "inventory.csv", filters: [{ name: "CSV", extensions: ["csv"] }, { name: "Excel", extensions: ["xlsx"] }] });
            if (path)
              call<number>("export_inventory", { path })
                .then((n) => toast(`Exported ${n} items`))
                .catch((e) => toast(errorMessage(e), { tone: "bad" }));
          }}
        >
          <Download size={16} aria-hidden /> Export
        </button>
        <button className="btn primary" onClick={() => setAdding(true)}>
          <PackagePlus size={16} aria-hidden /> Add item
        </button>
      </PageHead>
      <Tabs
        label="Inventory views"
        value={tab}
        onChange={setTab}
        tabs={[
          { id: "all", label: "All items" },
          { id: "low", label: "Low stock" },
          { id: "reorder", label: "Reorder & purchasing" },
          { id: "reservations", label: "Reservations" },
          { id: "history", label: "History" },
          { id: "count", label: "Physical count" },
          { id: "scan", label: "Scan" },
        ]}
      />
      {(tab === "all" || tab === "low") && <ItemList low={tab === "low"} onLabels={setLabels} onAdd={() => setAdding(true)} />}
      {tab === "reorder" && <Purchasing />}
      {tab === "reservations" && <Reservations />}
      {tab === "history" && <History />}
      {tab === "count" && <Count />}
      {tab === "scan" && <Scan onAdd={() => setAdding(true)} />}
      {adding && <NewItem onClose={() => setAdding(false)} />}
      {importing && <ImportInventory onClose={() => setImporting(false)} />}
      {labels && <Labels items={labels} onClose={() => setLabels(null)} />}
    </div>
  );
}

function ItemList({ low, onLabels, onAdd }: { low: boolean; onLabels: (i: Item[]) => void; onAdd: () => void }) {
  const go = useApp((s) => s.go);
  const [q, setQ] = useState("");
  const [cat, setCat] = useState("");
  const [loc, setLoc] = useState("");
  const [archived, setArchived] = useState(false);
  const cats = useCmd<Category[]>("categories");
  const locs = useCmd<Location[]>("location_list");
  const items = useCmd<Item[]>("item_list", { query: { q: q || null, category_id: cat || null, location_id: loc || null, stock: low ? "low" : null, include_archived: archived } });
  return (
    <div className="stack">
      <div className="row">
        <input type="search" placeholder="Search SKU, name, brand or part number" value={q} onChange={(e) => setQ(e.target.value)} style={{ flex: 1, minWidth: 220 }} aria-label="Search inventory" />
        <select value={cat} onChange={(e) => setCat(e.target.value)} aria-label="Category">
          <option value="">All categories</option>
          {cats.data
            ?.filter((c) => !c.parent_id)
            .map((c) => (
              <option key={c.id} value={c.id}>
                {c.name}
              </option>
            ))}
        </select>
        <select value={loc} onChange={(e) => setLoc(e.target.value)} aria-label="Location">
          <option value="">All locations</option>
          {locs.data?.map((l) => (
            <option key={l.id} value={l.id}>
              {l.name}
            </option>
          ))}
        </select>
        <label className="check small">
          <input type="checkbox" checked={archived} onChange={(e) => setArchived(e.target.checked)} /> Archived
        </label>
        {items.data && items.data.length > 0 && (
          <button className="btn" onClick={() => onLabels(items.data!)}>
            <Printer size={16} aria-hidden /> Labels
          </button>
        )}
      </div>
      <SavedSearches
        page={low ? "inventory-low" : "inventory"}
        filters={{ q, cat, loc, archived }}
        onApply={(f) => {
          setQ(String(f.q ?? ""));
          setCat(String(f.cat ?? ""));
          setLoc(String(f.loc ?? ""));
          setArchived(!!f.archived);
        }}
      />
      <Loaded q={items}>
        {(rows) =>
          rows.length === 0 ? (
            <Empty icon={<Boxes size={20} />} title={low ? "Nothing is low on stock" : q || cat || loc ? "No items match" : "No inventory yet"} action={!low && !q ? <button className="btn primary" onClick={onAdd}>Add an item</button> : undefined}>
              {low ? "Items at or below their minimum stock appear here." : "Add items one at a time, or import a spreadsheet you already have."}
            </Empty>
          ) : (
            <DataTable
              label="Inventory items"
              rows={rows}
              rowKey={(r) => r.id}
              onOpen={(r) => go({ page: "item", id: r.id })}
              menu={(r) => [
                { label: "Open", run: () => go({ page: "item", id: r.id }) },
                { label: "Open catalog part", run: () => go({ page: "part", id: r.part_id }) },
                { label: "Print label", run: () => onLabels([r]) },
              ]}
              footer={`${rows.length} items · ${qty(rows.reduce((n, r) => n + r.on_hand, 0))} units on hand`}
              columns={[
                { id: "sku", label: "SKU", width: "minmax(90px, 1fr)", render: (r) => <span className="mono">{r.sku}</span>, sort: (r) => r.sku },
                { id: "name", label: "Product", width: "minmax(180px, 2.5fr)", render: (r) => (<>{r.name}{r.archived && <Badge>archived</Badge>}</>), sort: (r) => r.name },
                { id: "brand", label: "Brand", width: "minmax(80px, 1fr)", render: (r) => r.brand, sort: (r) => r.brand },
                { id: "numbers", label: "Part no.", width: "minmax(100px, 1.2fr)", render: (r) => <span className="mono small">{r.numbers}</span> },
                { id: "where", label: "Location", width: "minmax(100px, 1.2fr)", render: (r) => <span className="small">{r.bins || "—"}</span>, sort: (r) => r.bins },
                { id: "avail", label: "Available", width: "130px", render: (r) => <StockBadge available={r.available} min={r.min_stock} />, sort: (r) => r.available },
                { id: "res", label: "Reserved", width: "80px", render: (r) => r.reserved || "", sort: (r) => r.reserved, align: "right" },
                { id: "cost", label: "Avg cost", width: "90px", render: (r) => money(r.avg_cost), sort: (r) => Number(r.avg_cost), align: "right" },
              ]}
            />
          )
        }
      </Loaded>
    </div>
  );
}

function Reservations() {
  const go = useApp((s) => s.go);
  const q = useCmd<Reservation[]>("reservation_list");
  const release = useAction<{ id: string }>("stock_release", { success: "Reservation released" });
  return (
    <Loaded q={q}>
      {(rows) =>
        rows.length === 0 ? (
          <Empty icon={<ListChecks size={20} />} title="No active reservations">
            Reserve stock from a vehicle's Parts in Stock tab, an estimate, or an item's page.
          </Empty>
        ) : (
          <div className="table-wrap">
            <table>
              <thead>
                <tr>
                  <th>Item</th>
                  <th>Qty</th>
                  <th>Location</th>
                  <th>For</th>
                  <th>Since</th>
                  <th />
                </tr>
              </thead>
              <tbody>
                {rows.map((r) => (
                  <tr key={r.id}>
                    <td>
                      <button className="btn link" style={{ padding: 0 }} onClick={() => go({ page: "item", id: r.item_id })}>
                        {r.name}
                      </button>{" "}
                      <span className="mono small muted">{r.sku}</span>
                    </td>
                    <td>{r.qty}</td>
                    <td>{r.location}</td>
                    <td>
                      {r.job_id ? (
                        <button className="btn link" style={{ padding: 0 }} onClick={() => go({ page: "job", id: r.job_id! })}>
                          Job #{r.job_number}
                        </button>
                      ) : r.vehicle_id ? (
                        <button className="btn link" style={{ padding: 0 }} onClick={() => go({ page: "vehicle", id: r.vehicle_id! })}>
                          Vehicle
                        </button>
                      ) : (
                        "—"
                      )}
                      {r.note && <div className="small muted">{r.note}</div>}
                    </td>
                    <td>{ago(r.created_at)}</td>
                    <td>
                      <button className="btn link" onClick={() => release.mutate({ id: r.id })}>
                        Release
                      </button>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )
      }
    </Loaded>
  );
}

function History() {
  const go = useApp((s) => s.go);
  const q = useCmd<Movement[]>("movement_list", { itemId: null, limit: 2000 });
  return (
    <Loaded q={q}>
      {(rows) =>
        rows.length === 0 ? (
          <Empty icon={<ListChecks size={20} />} title="No stock movements yet" />
        ) : (
          <DataTable
            label="Stock movements"
            rows={rows}
            rowKey={(r) => r.id}
            onOpen={(r) => go({ page: "item", id: r.item_id })}
            footer={`${rows.length} most recent movements`}
            columns={[
              { id: "when", label: "When", width: "160px", render: (r) => dateTime(r.created_at), sort: (r) => r.created_at },
              { id: "what", label: "Change", width: "150px", render: (r) => movementLabel(r.kind), sort: (r) => r.kind },
              { id: "item", label: "Item", width: "minmax(180px, 2fr)", render: (r) => `${r.name} (${r.sku})`, sort: (r) => r.name },
              { id: "loc", label: "Location", width: "minmax(100px, 1fr)", render: (r) => r.location },
              { id: "qty", label: "Qty", width: "70px", render: (r) => <span style={{ color: r.qty < 0 ? "var(--danger)" : "var(--success)" }}>{r.qty > 0 ? `+${r.qty}` : r.qty}</span>, align: "right", sort: (r) => r.qty },
              { id: "why", label: "Note", width: "minmax(120px, 2fr)", render: (r) => <span className="small muted">{[r.reason, r.reversed && "undone"].filter(Boolean).join(" · ")}</span> },
            ]}
          />
        )
      }
    </Loaded>
  );
}

function Count() {
  const locs = useCmd<Location[]>("location_list");
  const [loc, setLoc] = useState("");
  const items = useCmd<Item[]>("item_list", { query: { q: null, category_id: null, location_id: null, stock: null, include_archived: false } });
  const [counts, setCounts] = useState<Record<string, string>>({});
  const onHand = useCmd<[string, number][]>("stock_on_hand_at", { locationId: loc }, !!loc);
  const levels: Record<string, number> = Object.fromEntries(onHand.data ?? []);
  const post = useAction<{ locationId: string; lines: { item_id: string; counted: number }[] }, CountResult>("stock_count", {
    success: (r) => `Count posted: ${r.changed} changed, ${r.unchanged} matched`,
    invalidate: ["stock_on_hand_at", "item_list", "dashboard", "movement_list"],
  });
  useEffect(() => {
    if (!loc && locs.data) setLoc(locs.data.find((l) => l.is_default)?.id ?? "");
  }, [locs.data, loc]);
  const lines = Object.entries(counts)
    .filter(([, v]) => v.trim() !== "")
    .map(([item_id, v]) => ({ item_id, counted: Number(v) }));
  return (
    <div className="stack">
      <div className="row">
        <label className="field" style={{ flexDirection: "row", alignItems: "center", gap: 8 }}>
          <span>Location</span>
          <select value={loc} onChange={(e) => setLoc(e.target.value)}>
            {locs.data
              ?.filter((l) => l.kind === "warehouse")
              .map((l) => (
                <option key={l.id} value={l.id}>
                  {l.name}
                </option>
              ))}
          </select>
        </label>
        <span className="small muted">Enter what you count. Leave a row empty to skip it. Differences are recorded as count adjustments.</span>
        <button className="btn primary ml-auto" disabled={!lines.length || post.isPending} onClick={() => post.mutate({ locationId: loc, lines }, { onSuccess: () => setCounts({}) })}>
          Post count ({lines.length})
        </button>
      </div>
      <Loaded q={items}>
        {(rows) => (
          <div className="table-wrap" style={{ maxHeight: "calc(100vh - 330px)" }}>
            <table>
              <thead>
                <tr>
                  <th>SKU</th>
                  <th>Product</th>
                  <th>Bin</th>
                  <th className="num">Recorded</th>
                  <th>Counted</th>
                  <th className="num">Difference</th>
                </tr>
              </thead>
              <tbody>
                {rows.map((r) => {
                  const rec = levels[r.id] ?? 0;
                  const c = counts[r.id];
                  const diff = c !== undefined && c !== "" ? Number(c) - rec : null;
                  return (
                    <tr key={r.id}>
                      <td className="mono">{r.sku}</td>
                      <td>{r.name}</td>
                      <td className="small">{r.bins}</td>
                      <td className="num">{rec}</td>
                      <td>
                        <input type="number" min={0} step="any" value={c ?? ""} onChange={(e) => setCounts({ ...counts, [r.id]: e.target.value })} style={{ width: 90 }} aria-label={`Counted ${r.sku}`} />
                      </td>
                      <td className="num" style={{ color: diff ? (diff < 0 ? "var(--danger)" : "var(--success)") : undefined }}>
                        {diff == null ? "" : diff > 0 ? `+${diff}` : diff}
                      </td>
                    </tr>
                  );
                })}
              </tbody>
            </table>
          </div>
        )}
      </Loaded>
    </div>
  );
}

/** USB barcode scanners type the code and press Enter; this field catches it. */
function Scan({ onAdd }: { onAdd: () => void }) {
  const go = useApp((s) => s.go);
  const [code, setCode] = useState("");
  const [miss, setMiss] = useState<string | null>(null);
  const input = useRef<HTMLInputElement>(null);
  useEffect(() => input.current?.focus(), []);
  const lookup = async () => {
    const c = code.trim();
    if (!c) return;
    setMiss(null);
    try {
      const r = await call<SearchResult>("search", { query: c, kinds: ["item", "part"], limit: 5 });
      if (r.route?.kind === "item") return go({ page: "item", id: r.route.id });
      if (r.route?.kind === "part") return go({ page: "part", id: r.route.id });
      if (r.hits.length === 1) return go({ page: r.hits[0].kind === "item" ? "item" : "part", id: r.hits[0].id });
      setMiss(c);
    } catch (e) {
      toast(errorMessage(e), { tone: "bad" });
    }
    setCode("");
  };
  return (
    <div className="card stack" style={{ maxWidth: 640 }}>
      <form
        className="row"
        onSubmit={(e) => {
          e.preventDefault();
          void lookup();
        }}
      >
        <Barcode size={22} aria-hidden />
        <input ref={input} type="text" className="mono" value={code} onChange={(e) => setCode(e.target.value)} placeholder="Scan or type a barcode, SKU or part number" style={{ flex: 1, fontSize: 16 }} aria-label="Barcode" />
        <button className="btn primary">Find</button>
      </form>
      <p className="small muted">Plug in any USB barcode scanner; it works like a keyboard. Camera scanning isn't available in this version.</p>
      {miss && (
        <div className="notice warn spread">
          <span>
            Nothing in inventory or the catalog has the code <span className="mono">{miss}</span>.
          </span>
          <button className="btn" onClick={onAdd}>
            Add a new item
          </button>
        </div>
      )}
    </div>
  );
}
