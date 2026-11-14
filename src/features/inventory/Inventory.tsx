import { useEffect, useRef, useState } from "react";
import { Barcode, Boxes, Download, FileUp, ListChecks, PackagePlus, Printer, Truck, Camera } from "lucide-react";
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
import { ScanDialog } from "../shared/ScanDialog";
import { t, tx } from "../../i18n";

export default function Inventory() {
  const { route, setTab, go } = useApp();
  const tab = route.tab === "new" ? "all" : (route.tab ?? "all");
  const [adding, setAdding] = useState(route.tab === "new");
  const [importing, setImporting] = useState(false);
  const [labels, setLabels] = useState<Item[] | null>(null);
  return (
    <div className="page wide">
      <PageHead title={t("Inventory")} sub={t("What you have, where it is, and every movement in and out.")}>
        <button className="btn" onClick={() => go("suppliers")}>
          <Truck size={16} aria-hidden />{" "}{t("Suppliers")}</button>
        <button className="btn" onClick={() => setImporting(true)}>
          <FileUp size={16} aria-hidden />{" "}{t("Import")}</button>
        <button
          className="btn"
          onClick={async () => {
            const path = await saveFile({ defaultPath: "inventory.csv", filters: [{ name: "CSV", extensions: ["csv"] }, { name: "Excel", extensions: ["xlsx"] }] });
            if (path)
              call<number>("export_inventory", { path })
                .then((n) => toast(t("Exported {0} items", n)))
                .catch((e) => toast(errorMessage(e), { tone: "bad" }));
          }}
        >
          <Download size={16} aria-hidden />{" "}{t("Export")}</button>
        <button className="btn primary" onClick={() => setAdding(true)}>
          <PackagePlus size={16} aria-hidden />{" "}{t("Add item")}</button>
      </PageHead>
      <Tabs
        label={t("Inventory views")}
        value={tab}
        onChange={setTab}
        tabs={[
          { id: "all", label: t("All items") },
          { id: "low", label: t("Low stock") },
          { id: "reorder", label: t("Reorder & purchasing") },
          { id: "reservations", label: t("Reservations") },
          { id: "history", label: t("History") },
          { id: "count", label: t("Physical count") },
          { id: "scan", label: t("Scan") },
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
        <input type="search" placeholder={t("Search SKU, name, brand or part number")} value={q} onChange={(e) => setQ(e.target.value)} style={{ flex: 1, minWidth: 220 }} aria-label={t("Search inventory")} />
        <select value={cat} onChange={(e) => setCat(e.target.value)} aria-label={t("Category")}>
          <option value="">{t("All categories")}</option>
          {cats.data
            ?.filter((c) => !c.parent_id)
            .map((c) => (
              <option key={c.id} value={c.id}>
                {c.name}
              </option>
            ))}
        </select>
        <select value={loc} onChange={(e) => setLoc(e.target.value)} aria-label={t("Location")}>
          <option value="">{t("All locations")}</option>
          {locs.data?.map((l) => (
            <option key={l.id} value={l.id}>
              {l.name}
            </option>
          ))}
        </select>
        <label className="check small">
          <input type="checkbox" checked={archived} onChange={(e) => setArchived(e.target.checked)} />{" "}{t("Archived")}</label>
        {items.data && items.data.length > 0 && (
          <button className="btn" onClick={() => onLabels(items.data!)}>
            <Printer size={16} aria-hidden />{" "}{t("Labels")}</button>
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
            <Empty icon={<Boxes size={20} />} title={low ? t("Nothing is low on stock") : q || cat || loc ? t("No items match") : t("No inventory yet")} action={!low && !q ? <button className="btn primary" onClick={onAdd}>{t("Add an item")}</button> : undefined}>
              {low ? t("Items at or below their minimum stock appear here.") : t("Add items one at a time, or import a spreadsheet you already have.")}
            </Empty>
          ) : (
            <DataTable
              label={t("Inventory items")}
              rows={rows}
              rowKey={(r) => r.id}
              onOpen={(r) => go({ page: "item", id: r.id })}
              menu={(r) => [
                { label: t("Open"), run: () => go({ page: "item", id: r.id }) },
                { label: t("Open catalog part"), run: () => go({ page: "part", id: r.part_id }) },
                { label: t("Print label"), run: () => onLabels([r]) },
              ]}
              footer={`${rows.length} items · ${qty(rows.reduce((n, r) => n + r.on_hand, 0))} units on hand`}
              columns={[
                { id: "sku", label: t("SKU"), width: "minmax(90px, 1fr)", render: (r) => <span className="mono">{r.sku}</span>, sort: (r) => r.sku },
                { id: "name", label: t("Product"), width: "minmax(180px, 2.5fr)", render: (r) => (<>{r.name}{r.archived && <Badge>archived</Badge>}</>), sort: (r) => r.name },
                { id: "brand", label: t("Brand"), width: "minmax(80px, 1fr)", render: (r) => r.brand, sort: (r) => r.brand },
                { id: "numbers", label: t("Part no."), width: "minmax(100px, 1.2fr)", render: (r) => <span className="mono small">{r.numbers}</span> },
                { id: "where", label: t("Location"), width: "minmax(100px, 1.2fr)", render: (r) => <span className="small">{r.bins || "—"}</span>, sort: (r) => r.bins },
                { id: "avail", label: t("Available"), width: "130px", render: (r) => <StockBadge available={r.available} min={r.min_stock} />, sort: (r) => r.available },
                { id: "res", label: t("Reserved"), width: "80px", render: (r) => r.reserved || "", sort: (r) => r.reserved, align: "right" },
                { id: "cost", label: t("Avg cost"), width: "90px", render: (r) => money(r.avg_cost), sort: (r) => Number(r.avg_cost), align: "right" },
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
  const release = useAction<{ id: string }>("stock_release", { success: t("Reservation released") });
  return (
    <Loaded q={q}>
      {(rows) =>
        rows.length === 0 ? (
          <Empty icon={<ListChecks size={20} />} title={t("No active reservations")}>{t("Reserve stock from a vehicle's Parts in Stock tab, an estimate, or an item's page.")}</Empty>
        ) : (
          <div className="table-wrap">
            <table>
              <thead>
                <tr>
                  <th>{t("Item")}</th>
                  <th>{t("Qty")}</th>
                  <th>{t("Location")}</th>
                  <th>{t("For")}</th>
                  <th>{t("Since")}</th>
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
                        <button className="btn link" style={{ padding: 0 }} onClick={() => go({ page: "job", id: r.job_id! })}>{tx("Job #{0}", r.job_number)}</button>
                      ) : r.vehicle_id ? (
                        <button className="btn link" style={{ padding: 0 }} onClick={() => go({ page: "vehicle", id: r.vehicle_id! })}>{t("Vehicle")}</button>
                      ) : (
                        "—"
                      )}
                      {r.note && <div className="small muted">{r.note}</div>}
                    </td>
                    <td>{ago(r.created_at)}</td>
                    <td>
                      <button className="btn link" onClick={() => release.mutate({ id: r.id })}>{t("Release")}</button>
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
          <Empty icon={<ListChecks size={20} />} title={t("No stock movements yet")} />
        ) : (
          <DataTable
            label={t("Stock movements")}
            rows={rows}
            rowKey={(r) => r.id}
            onOpen={(r) => go({ page: "item", id: r.item_id })}
            footer={`${rows.length} most recent movements`}
            columns={[
              { id: "when", label: t("When"), width: "160px", render: (r) => dateTime(r.created_at), sort: (r) => r.created_at },
              { id: "what", label: t("Change"), width: "150px", render: (r) => movementLabel(r.kind), sort: (r) => r.kind },
              { id: "item", label: t("Item"), width: "minmax(180px, 2fr)", render: (r) => `${r.name} (${r.sku})`, sort: (r) => r.name },
              { id: "loc", label: t("Location"), width: "minmax(100px, 1fr)", render: (r) => r.location },
              { id: "qty", label: t("Qty"), width: "70px", render: (r) => <span style={{ color: r.qty < 0 ? "var(--danger)" : "var(--success)" }}>{r.qty > 0 ? `+${r.qty}` : r.qty}</span>, align: "right", sort: (r) => r.qty },
              { id: "why", label: t("Note"), width: "minmax(120px, 2fr)", render: (r) => <span className="small muted">{[r.reason, r.reversed && "undone"].filter(Boolean).join(" · ")}</span> },
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
          <span>{t("Location")}</span>
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
        <span className="small muted">{t("Enter what you count. Leave a row empty to skip it. Differences are recorded as count adjustments.")}</span>
        <button className="btn primary ml-auto" disabled={!lines.length || post.isPending} onClick={() => post.mutate({ locationId: loc, lines }, { onSuccess: () => setCounts({}) })}>{tx("Post count ({0})", lines.length)}</button>
      </div>
      <Loaded q={items}>
        {(rows) => (
          <div className="table-wrap" style={{ maxHeight: "calc(100vh - 330px)" }}>
            <table>
              <thead>
                <tr>
                  <th>{t("SKU")}</th>
                  <th>{t("Product")}</th>
                  <th>{t("Bin")}</th>
                  <th className="num">{t("Recorded")}</th>
                  <th>{t("Counted")}</th>
                  <th className="num">{t("Difference")}</th>
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
                        <input type="number" min={0} step="any" value={c ?? ""} onChange={(e) => setCounts({ ...counts, [r.id]: e.target.value })} style={{ width: 90 }} aria-label={t("Counted {0}", r.sku)} />
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
  const [scanning, setScanning] = useState(false);
  const lookup = async (raw = code) => {
    const c = raw.trim();
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
        <input ref={input} type="text" className="mono" value={code} onChange={(e) => setCode(e.target.value)} placeholder={t("Scan or type a barcode, SKU or part number")} style={{ flex: 1, fontSize: 16 }} aria-label={t("Barcode")} />
        <button className="btn primary">{t("Find")}</button>
        <button type="button" className="btn" onClick={() => setScanning(true)}>
          <Camera size={16} aria-hidden />{" "}{t("Camera…")}</button>
      </form>
      <ScanDialog open={scanning} want="barcode" onClose={() => setScanning(false)} onBarcode={(b) => void lookup(b)} />
      <p className="small muted">{t("Plug in any USB barcode scanner (it works like a keyboard), or use the camera or a photo.")}</p>
      {miss && (
        <div className="notice warn spread">
          <span>{t("Nothing in inventory or the catalog has the code")}{" "}<span className="mono">{miss}</span>.
          </span>
          <button className="btn" onClick={onAdd}>{t("Add a new item")}</button>
        </div>
      )}
    </div>
  );
}
