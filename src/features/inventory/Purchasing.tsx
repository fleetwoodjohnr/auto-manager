import { useState } from "react";
import { PackageCheck, Send, ShoppingCart, Trash2, Truck, XCircle } from "lucide-react";
import { call, errorMessage } from "../../api/ipc";
import { useAction, useCmd, useRefresh } from "../../api/queries";
import type { Item, Location, PurchaseOrder, Suggestion, Supplier } from "../../api/types";
import { toast } from "../../store";
import { ActionsMenu, Badge, Empty, Field, Loaded, Modal, useConfirm } from "../../ui";
import { date, money } from "../../lib/format";

const PO_STATUS: Record<string, { label: string; tone: "neutral" | "info" | "warn" | "ok" | "bad" }> = {
  draft: { label: "Draft", tone: "neutral" },
  ordered: { label: "Sent", tone: "info" },
  partial: { label: "Partly received", tone: "warn" },
  received: { label: "Received", tone: "ok" },
  cancelled: { label: "Cancelled", tone: "bad" },
};

/** Reorder assistant, purchase orders and receiving. */
export function Purchasing() {
  const s = useCmd<Suggestion[]>("reorder_suggestions");
  const pos = useCmd<PurchaseOrder[]>("po_list", { openOnly: false });
  const suppliers = useCmd<Supplier[]>("supplier_list");
  const refresh = useRefresh();
  const [qty, setQty] = useState<Record<string, number>>({});
  const [picked, setPicked] = useState<Record<string, boolean>>({});
  const [open, setOpen] = useState<PurchaseOrder | null>(null);
  const [newFor, setNewFor] = useState("");
  const create = async () => {
    const picks = (s.data ?? []).filter((x) => picked[x.item_id] ?? true).map((x) => [x.item_id, qty[x.item_id] ?? x.suggested_qty] as [string, number]);
    try {
      const r = await call<{ orders: PurchaseOrder[]; without_supplier: string[] }>("reorder_create", { picks });
      toast(`Created ${r.orders.length} draft order${r.orders.length === 1 ? "" : "s"}${r.without_supplier.length ? `; ${r.without_supplier.length} item(s) need a supplier first` : ""}`);
      refresh();
    } catch (e) {
      toast(errorMessage(e), { tone: "bad" });
    }
  };
  return (
    <div className="stack">
      <section className="card">
        <div className="spread">
          <h2>
            <ShoppingCart size={16} aria-hidden /> Reorder assistant
          </h2>
          {s.data && s.data.length > 0 && (
            <button className="btn primary" onClick={create}>
              Create draft orders
            </button>
          )}
        </div>
        <p className="small muted">Items at or below their minimum (counting what's already on order) and restocking requests. The quantity covers the shortfall plus expected use over the supplier's lead time (from the last 90 days), and at least the item's reorder quantity.</p>
        <Loaded q={s}>
          {(list) =>
            list.length === 0 ? (
              <p className="muted">Nothing needs reordering.</p>
            ) : (
              <table>
                <thead>
                  <tr>
                    <th />
                    <th>Item</th>
                    <th>Why</th>
                    <th className="num">On order</th>
                    <th className="num">Used (90 days)</th>
                    <th>Supplier</th>
                    <th className="num">Unit cost</th>
                    <th>Order qty</th>
                  </tr>
                </thead>
                <tbody>
                  {list.map((x) => (
                    <tr key={x.item_id}>
                      <td>
                        <input type="checkbox" checked={picked[x.item_id] ?? true} onChange={(e) => setPicked({ ...picked, [x.item_id]: e.target.checked })} aria-label={`Order ${x.sku}`} />
                      </td>
                      <td>
                        {x.name} <span className="mono small muted">{x.sku}</span>
                      </td>
                      <td className="small">{x.reason}</td>
                      <td className="num">{x.on_order}</td>
                      <td className="num">{x.used_90d}</td>
                      <td>{x.supplier ?? <Badge tone="warn">choose a supplier on the item</Badge>}</td>
                      <td className="num">{money(x.unit_cost)}</td>
                      <td>
                        <input type="number" min={0} step="any" value={qty[x.item_id] ?? x.suggested_qty} onChange={(e) => setQty({ ...qty, [x.item_id]: Number(e.target.value) })} style={{ width: 80 }} aria-label={`Quantity for ${x.sku}`} />
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            )
          }
        </Loaded>
      </section>
      <section className="card">
        <div className="spread">
          <h2>
            <Truck size={16} aria-hidden /> Purchase orders
          </h2>
          <div className="row">
            <select value={newFor} onChange={(e) => setNewFor(e.target.value)} aria-label="Supplier for a new order">
              <option value="">New order for…</option>
              {suppliers.data?.map((x) => (
                <option key={x.id} value={x.id}>
                  {x.name}
                </option>
              ))}
            </select>
            <button
              className="btn"
              disabled={!newFor}
              onClick={() =>
                call<PurchaseOrder>("po_create", { supplierId: newFor })
                  .then((po) => {
                    refresh();
                    setOpen(po);
                  })
                  .catch((e) => toast(errorMessage(e), { tone: "bad" }))
              }
            >
              Create
            </button>
          </div>
        </div>
        <Loaded q={pos}>
          {(list) =>
            list.length === 0 ? (
              <Empty icon={<Truck size={20} />} title="No purchase orders yet" />
            ) : (
              <table>
                <thead>
                  <tr>
                    <th>PO</th>
                    <th>Supplier</th>
                    <th>Status</th>
                    <th>Expected</th>
                    <th className="num">Lines</th>
                    <th className="num">Total</th>
                  </tr>
                </thead>
                <tbody>
                  {list.map((po) => (
                    <tr key={po.id} style={{ cursor: "pointer" }} onClick={() => setOpen(po)}>
                      <td>PO {po.number}</td>
                      <td>{po.supplier}</td>
                      <td>
                        <Badge tone={PO_STATUS[po.status].tone}>{PO_STATUS[po.status].label}</Badge> {po.late && <Badge tone="bad">late</Badge>}
                      </td>
                      <td>{po.expected_date ? date(po.expected_date) : "—"}</td>
                      <td className="num">{po.lines.length}</td>
                      <td className="num">{money(po.total)}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            )
          }
        </Loaded>
      </section>
      {open && <PoDialog id={open.id} onClose={() => setOpen(null)} />}
    </div>
  );
}

function PoDialog({ id, onClose }: { id: string; onClose: () => void }) {
  const q = useCmd<PurchaseOrder>("po_get", { id });
  const items = useCmd<Item[]>("item_list", { query: { q: null, category_id: null, location_id: null, stock: null, include_archived: false } });
  const locs = useCmd<Location[]>("location_list");
  const inv = ["po_get", "po_list", "reorder_suggestions", "item_list", "item_get", "dashboard", "movement_list"];
  const status = useAction<{ id: string; status: string }>("po_set_status", { invalidate: inv, success: "Saved" });
  const addLine = useAction<{ id: string; itemId: string; qty: number; unitCost: string | null }>("po_add_line", { invalidate: inv });
  const updLine = useAction<{ lineId: string; qty: number; unitCost: string }>("po_update_line", { invalidate: inv });
  const rmLine = useAction<{ lineId: string }>("po_remove_line", { invalidate: inv });
  const upd = useAction<{ id: string; input: { expected_date: string | null; notes: string } }>("po_update", { invalidate: inv });
  const receive = useAction<{ id: string; locationId: string; lines: { line_id: string; qty: number }[] }>("po_receive", { invalidate: inv, success: "Received into stock" });
  const del = useAction<{ id: string }>("po_delete", { invalidate: inv, success: "Order deleted" });
  const { ask, dialog } = useConfirm();
  const [item, setItem] = useState("");
  const [recv, setRecv] = useState<Record<string, number>>({});
  const [loc, setLoc] = useState("");
  const po = q.data;
  if (!po) return null;
  const draft = po.status === "draft";
  const receiving = po.status === "ordered" || po.status === "partial";
  const defaultLoc = loc || locs.data?.find((l) => l.is_default)?.id || "";
  return (
    <Modal
      open
      wide
      title={`PO ${po.number} · ${po.supplier}`}
      onClose={onClose}
      footer={
        <>
          {draft && (
            <button className="btn danger" onClick={async () => (await ask("Delete this draft order?", "Nothing has been received on it.")) && del.mutate({ id }, { onSuccess: onClose })}>
              <Trash2 size={14} aria-hidden /> Delete
            </button>
          )}
          <span className="flex-1" />
          {(draft || receiving) && (
            <button className="btn" onClick={() => status.mutate({ id, status: "cancelled" })}>
              <XCircle size={14} aria-hidden /> Cancel order
            </button>
          )}
          {draft && (
            <button className="btn primary" disabled={po.lines.length === 0} onClick={() => status.mutate({ id, status: "ordered" })}>
              <Send size={14} aria-hidden /> Mark as sent
            </button>
          )}
          {receiving && (
            <button
              className="btn primary"
              disabled={!Object.values(recv).some((n) => n > 0)}
              onClick={() => receive.mutate({ id, locationId: defaultLoc, lines: Object.entries(recv).map(([line_id, qty]) => ({ line_id, qty })) }, { onSuccess: () => setRecv({}) })}
            >
              <PackageCheck size={14} aria-hidden /> Receive
            </button>
          )}
        </>
      }
    >
      {dialog}
      <div className="row">
        <Badge tone={PO_STATUS[po.status].tone}>{PO_STATUS[po.status].label}</Badge>
        {po.late && <Badge tone="bad">Past expected date</Badge>}
        <label className="row small">
          Expected
          <input type="date" defaultValue={po.expected_date ?? ""} onBlur={(e) => upd.mutate({ id, input: { expected_date: e.target.value || null, notes: po.notes } })} />
        </label>
        {receiving && (
          <label className="row small">
            Receive into
            <select value={defaultLoc} onChange={(e) => setLoc(e.target.value)}>
              {locs.data
                ?.filter((l) => l.kind === "warehouse")
                .map((l) => (
                  <option key={l.id} value={l.id}>
                    {l.name}
                  </option>
                ))}
            </select>
          </label>
        )}
      </div>
      <table>
        <thead>
          <tr>
            <th>Item</th>
            <th>Supplier no.</th>
            <th className="num">Ordered</th>
            <th className="num">Received</th>
            <th className="num">Unit cost</th>
            <th className="num">Amount</th>
            <th />
          </tr>
        </thead>
        <tbody>
          {po.lines.map((l) => (
            <tr key={l.id}>
              <td>
                {l.name} <span className="mono small muted">{l.sku}</span>
              </td>
              <td className="mono small">{l.supplier_sku}</td>
              <td className="num">
                {draft ? <input type="number" min={0} step="any" defaultValue={l.qty_ordered} style={{ width: 70 }} onBlur={(e) => Number(e.target.value) !== l.qty_ordered && updLine.mutate({ lineId: l.id, qty: Number(e.target.value), unitCost: String(l.unit_cost) })} aria-label="Quantity" /> : l.qty_ordered}
              </td>
              <td className="num">
                {receiving && l.qty_received < l.qty_ordered ? (
                  <span className="row" style={{ justifyContent: "flex-end", flexWrap: "nowrap" }}>
                    {l.qty_received} +
                    <input type="number" min={0} step="any" max={l.qty_ordered - l.qty_received} value={recv[l.id] ?? 0} onChange={(e) => setRecv({ ...recv, [l.id]: Number(e.target.value) })} style={{ width: 70 }} aria-label={`Receive ${l.sku}`} />
                  </span>
                ) : (
                  l.qty_received
                )}
              </td>
              <td className="num">{draft ? <input type="text" defaultValue={String(l.unit_cost)} style={{ width: 90 }} onBlur={(e) => e.target.value !== String(l.unit_cost) && updLine.mutate({ lineId: l.id, qty: l.qty_ordered, unitCost: e.target.value })} aria-label="Unit cost" /> : money(l.unit_cost)}</td>
              <td className="num">{money(l.amount)}</td>
              <td>{draft && <ActionsMenu items={[{ label: "Remove line", danger: true, run: () => rmLine.mutate({ lineId: l.id }) }]} />}</td>
            </tr>
          ))}
          <tr>
            <td colSpan={5} className="num">
              <strong>Total</strong>
            </td>
            <td className="num">
              <strong>{money(po.total)}</strong>
            </td>
            <td />
          </tr>
        </tbody>
      </table>
      {draft && (
        <div className="row">
          <Field label="Add item">
            <select value={item} onChange={(e) => setItem(e.target.value)}>
              <option value="">Choose…</option>
              {items.data?.map((i) => (
                <option key={i.id} value={i.id}>
                  {i.name} ({i.sku})
                </option>
              ))}
            </select>
          </Field>
          <button className="btn" disabled={!item} onClick={() => addLine.mutate({ id, itemId: item, qty: 1, unitCost: null }, { onSuccess: () => setItem("") })} style={{ alignSelf: "flex-end" }}>
            Add
          </button>
        </div>
      )}
      {addLine.error != null && <div className="notice bad">{errorMessage(addLine.error)}</div>}
    </Modal>
  );
}

/** Supplier prices for one item (comparison) and its purchase cost history. */
export function SupplierPricing({ itemId }: { itemId: string }) {
  const prices = useCmd<import("../../api/types").SupplierPrice[]>("supplier_prices", { itemId, supplierId: null });
  const history = useCmd<import("../../api/types").CostPoint[]>("cost_history", { itemId });
  const suppliers = useCmd<Supplier[]>("supplier_list");
  const [f, setF] = useState({ supplier_id: "", supplier_sku: "", cost: "", lead_time_days: "" });
  const save = useAction<{ input: unknown }>("supplier_price_set", { invalidate: ["supplier_prices", "reorder_suggestions"], success: "Price saved" });
  const rm = useAction<{ supplierId: string; itemId: string }>("supplier_price_remove", { invalidate: ["supplier_prices"] });
  return (
    <section className="card">
      <h2>Supplier prices</h2>
      {prices.data && prices.data.length > 0 ? (
        <table>
          <tbody>
            {prices.data.map((p, i) => (
              <tr key={p.supplier_id}>
                <td>
                  {p.supplier} {i === 0 && p.cost && prices.data!.length > 1 && <Badge tone="ok">lowest</Badge>} {p.preferred && <Badge tone="info">main</Badge>}
                </td>
                <td className="mono small">{p.supplier_sku}</td>
                <td className="num">{p.cost ? money(p.cost) : "—"}</td>
                <td className="small muted">{p.lead_time_days != null ? `${p.lead_time_days} d` : ""}</td>
                <td>
                  <button className="btn link" onClick={() => rm.mutate({ supplierId: p.supplier_id, itemId })}>
                    Remove
                  </button>
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      ) : (
        <p className="muted small">No supplier prices yet.</p>
      )}
      <form
        className="row mt-2"
        onSubmit={(e) => {
          e.preventDefault();
          save.mutate({ input: { item_id: itemId, supplier_id: f.supplier_id, supplier_sku: f.supplier_sku, cost: f.cost || null, lead_time_days: f.lead_time_days ? Number(f.lead_time_days) : null } });
        }}
      >
        <select value={f.supplier_id} onChange={(e) => setF({ ...f, supplier_id: e.target.value })} aria-label="Supplier">
          <option value="">Supplier…</option>
          {suppliers.data?.map((s) => (
            <option key={s.id} value={s.id}>
              {s.name}
            </option>
          ))}
        </select>
        <input type="text" placeholder="Their part no." value={f.supplier_sku} onChange={(e) => setF({ ...f, supplier_sku: e.target.value })} style={{ width: 130 }} aria-label="Supplier part number" />
        <input type="text" inputMode="decimal" placeholder="Cost" value={f.cost} onChange={(e) => setF({ ...f, cost: e.target.value })} style={{ width: 90 }} aria-label="Cost" />
        <button className="btn" disabled={!f.supplier_id}>
          Save
        </button>
      </form>
      {history.data && history.data.length > 0 && (
        <details className="mt-2">
          <summary className="small">Purchase cost history ({history.data.length})</summary>
          <table className="mt-1">
            <tbody>
              {history.data.map((h, i) => (
                <tr key={i}>
                  <td className="small">{date(h.date)}</td>
                  <td className="small">{h.reference}</td>
                  <td className="num">{h.qty}</td>
                  <td className="num">{money(h.unit_cost)}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </details>
      )}
    </section>
  );
}
