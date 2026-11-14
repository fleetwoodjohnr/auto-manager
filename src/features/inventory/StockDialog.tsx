import { useEffect, useState } from "react";
import { call, errorField, errorMessage } from "../../api/ipc";
import { useAction, useCmd } from "../../api/queries";
import type { Item, Location, StockLevel } from "../../api/types";
import { toast } from "../../store";
import { Field, Modal } from "../../ui";

export type StockOp = "receive" | "issue" | "transfer" | "adjust" | "damage" | "return_in" | "return_out";

const META: Record<StockOp, { title: string; cmd: string; verb: string; help: string }> = {
  receive: { title: "Receive stock", cmd: "stock_receive", verb: "Received", help: "Stock arriving from a supplier. The cost updates the item's average cost." },
  issue: { title: "Issue stock", cmd: "stock_issue", verb: "Issued", help: "Stock leaving: sold over the counter, used, or scrapped. Reserved stock can't be issued." },
  transfer: { title: "Move stock", cmd: "stock_transfer", verb: "Moved", help: "Move stock between locations." },
  adjust: { title: "Adjust quantity", cmd: "stock_adjust", verb: "Adjusted", help: "Set the quantity at a location to what's actually there. The difference is recorded with your reason." },
  damage: { title: "Mark damaged", cmd: "stock_damage", verb: "Marked damaged", help: "Moves stock to quarantine. It stays on record but can't be sold or reserved." },
  return_in: { title: "Customer return", cmd: "stock_return_in", verb: "Returned", help: "A customer brought a part back in sellable condition. Faulty returns: receive, then mark damaged." },
  return_out: { title: "Return to supplier", cmd: "stock_return_out", verb: "Returned to supplier", help: "Send stock back to the supplier." },
};

/** One dialog for every stock movement. Each change can be undone from the toast. */
export function StockDialog({ op, item, levels, onClose }: { op: StockOp | null; item: Item; levels: StockLevel[]; onClose: () => void }) {
  const locs = useCmd<Location[]>("location_list");
  const warehouses = (locs.data ?? []).filter((l) => l.kind === "warehouse" && !l.archived);
  const [loc, setLoc] = useState("");
  const [to, setTo] = useState("");
  const [qty, setQty] = useState("1");
  const [cost, setCost] = useState(String(item.cost));
  const [bin, setBin] = useState("");
  const [reason, setReason] = useState("");
  const withStock = levels.filter((l) => l.kind === "warehouse" && l.on_hand > 0);
  useEffect(() => {
    if (!op) return;
    const first = op === "receive" || op === "return_in" ? warehouses.find((l) => l.is_default)?.id : withStock[0]?.location_id;
    setLoc(first ?? warehouses[0]?.id ?? "");
    setTo("");
    setQty(op === "adjust" ? String(withStock[0]?.on_hand ?? 0) : "1");
    setReason("");
    setBin("");
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [op, locs.data]);
  const m = op ? META[op] : null;
  const act = useAction<{ input: unknown }, string | string[] | null>(m?.cmd ?? "stock_issue", {
    silentError: true,
    success: () => `${m?.verb ?? "Done"}: ${item.sku}`,
    undo: (out) => {
      const id = typeof out === "string" ? out : Array.isArray(out) ? out[0] : null;
      if (!id) return null;
      return () =>
        void call("movement_reverse", { id })
          .then(() => toast("Undone"))
          .catch((e) => toast(errorMessage(e), { tone: "bad" }));
    },
  });
  if (!op || !m) return null;
  const level = levels.find((l) => l.location_id === loc);
  const submit = () => {
    const base = { item_id: item.id, location_id: loc, qty: Number(qty), reason, ref_kind: null, ref_id: null };
    const input =
      op === "receive"
        ? { ...base, unit_cost: cost || null, bin: bin || null }
        : op === "transfer"
          ? { item_id: item.id, from_location_id: loc, to_location_id: to, qty: Number(qty), bin: bin || null, reason }
          : op === "adjust"
            ? { item_id: item.id, location_id: loc, counted: Number(qty), reason }
            : base;
    act.mutate({ input }, { onSuccess: onClose });
  };
  const err = act.error;
  const fe = (f: string) => (err && errorField(err) === f ? errorMessage(err) : null);
  const sourceLocs = op === "receive" || op === "return_in" ? warehouses : warehouses.filter((l) => withStock.some((w) => w.location_id === l.id) || op === "adjust");
  return (
    <Modal
      open
      title={`${m.title}: ${item.sku}`}
      onClose={onClose}
      footer={
        <>
          <button className="btn" onClick={onClose}>
            Cancel
          </button>
          <button className="btn primary" onClick={submit} disabled={act.isPending || !loc || (op === "transfer" && !to)}>
            {m.title}
          </button>
        </>
      }
    >
      <p className="muted">{m.help}</p>
      <div className="grid two">
        <Field label={op === "transfer" ? "From" : "Location"} error={fe("location_id")}>
          <select value={loc} onChange={(e) => setLoc(e.target.value)}>
            {sourceLocs.map((l) => {
              const lv = levels.find((x) => x.location_id === l.id);
              return (
                <option key={l.id} value={l.id}>
                  {l.name}
                  {lv ? ` (${lv.available} available${lv.reserved ? `, ${lv.reserved} reserved` : ""})` : ""}
                </option>
              );
            })}
          </select>
        </Field>
        {op === "transfer" && (
          <Field label="To" error={fe("to_location_id")}>
            <select value={to} onChange={(e) => setTo(e.target.value)}>
              <option value="">Choose…</option>
              {warehouses
                .filter((l) => l.id !== loc)
                .map((l) => (
                  <option key={l.id} value={l.id}>
                    {l.name}
                  </option>
                ))}
            </select>
          </Field>
        )}
        <Field label={op === "adjust" ? "Quantity actually there" : "Quantity"} error={fe("qty")} hint={op === "adjust" && level ? `Recorded now: ${level.on_hand}` : undefined}>
          <input type="number" min={op === "adjust" ? 0 : 1} value={qty} onChange={(e) => setQty(e.target.value)} autoFocus />
        </Field>
        {op === "receive" && (
          <Field label="Cost per unit" error={fe("unit_cost")}>
            <input type="text" inputMode="decimal" value={cost} onChange={(e) => setCost(e.target.value)} />
          </Field>
        )}
        {(op === "receive" || op === "transfer") && (
          <Field label="Shelf / bin (optional)">
            <input type="text" value={bin} onChange={(e) => setBin(e.target.value)} />
          </Field>
        )}
      </div>
      <Field label={op === "adjust" || op === "damage" ? "Reason" : "Note (optional)"} error={fe("reason")}>
        <input type="text" value={reason} onChange={(e) => setReason(e.target.value)} placeholder={op === "damage" ? "e.g. Cracked housing" : op === "adjust" ? "e.g. Found two on the wrong shelf" : ""} />
      </Field>
      {err != null && !errorField(err) && <div className="notice bad">{errorMessage(err)}</div>}
    </Modal>
  );
}
