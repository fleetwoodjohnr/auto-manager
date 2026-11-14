import { useEffect, useState } from "react";
import { errorField, errorMessage } from "../../api/ipc";
import { useAction, useCmd, useRefresh } from "../../api/queries";
import type { Item, ItemInput, Location, PartInput, PartRow, Profile, Supplier } from "../../api/types";
import { useApp } from "../../store";
import { useSettings } from "../../settings";
import { ErrorNote, Field, Modal } from "../../ui";
import { emptyPart, PartFields } from "../parts/PartForm";
import { call } from "../../api/ipc";
import { t } from "../../i18n";

export function itemInput(i?: Item, minStock = 0): ItemInput {
  return {
    sku: i?.sku ?? "",
    condition: i?.condition ?? "new",
    unit: i?.unit ?? "each",
    fractional: i?.fractional ?? false,
    cost: i ? String(i.cost) : "",
    price_override: i?.price_override ?? null,
    pricing_profile_id: i?.pricing_profile_id ?? null,
    min_stock: i?.min_stock ?? minStock,
    reorder_qty: i?.reorder_qty ?? 0,
    supplier_id: i?.supplier_id ?? null,
    warranty: i?.warranty ?? "",
    notes: i?.notes ?? "",
  };
}

export function ItemFields({ value, onChange, error }: { value: ItemInput; onChange: (v: ItemInput) => void; error?: unknown }) {
  const suppliers = useCmd<Supplier[]>("supplier_list");
  const profiles = useCmd<Profile[]>("profile_list");
  const fe = (f: string) => (error && errorField(error) === f ? errorMessage(error) : null);
  const set = <K extends keyof ItemInput>(k: K, v: ItemInput[K]) => onChange({ ...value, [k]: v });
  return (
    <div className="grid" style={{ gridTemplateColumns: "repeat(auto-fit, minmax(180px, 1fr))" }}>
      <Field label={t("SKU")} hint={t("Your own stock code; must be unique.")} error={fe("sku")}>
        <input type="text" className="mono" value={value.sku} onChange={(e) => set("sku", e.target.value)} />
      </Field>
      <Field label={t("Condition")} error={fe("condition")}>
        <select value={value.condition} onChange={(e) => set("condition", e.target.value)}>
          <option value="new">{t("New")}</option>
          <option value="used">{t("Used")}</option>
          <option value="refurbished">{t("Refurbished / reman")}</option>
        </select>
      </Field>
      <Field label={t("Purchase cost per unit")} error={fe("cost")}>
        <input type="text" inputMode="decimal" value={value.cost} onChange={(e) => set("cost", e.target.value)} placeholder="0.00" />
      </Field>
      <Field label={t("Fixed selling price (optional)")} hint={t("Leave empty to price with your pricing rules.")} error={fe("price_override")}>
        <input type="text" inputMode="decimal" value={value.price_override ?? ""} onChange={(e) => set("price_override", e.target.value || null)} />
      </Field>
      <Field label={t("Pricing profile")} error={fe("pricing_profile_id")}>
        <select value={value.pricing_profile_id ?? ""} onChange={(e) => set("pricing_profile_id", e.target.value || null)}>
          <option value="">{t("Default profile")}</option>
          {profiles.data?.map((p) => (
            <option key={p.id} value={p.id}>
              {p.name}
            </option>
          ))}
        </select>
      </Field>
      <Field label={t("Minimum stock")} hint={t("Low-stock alert at or below this.")} error={fe("min_stock")}>
        <input type="number" min={0} step="any" value={value.min_stock} onChange={(e) => set("min_stock", Number(e.target.value))} />
      </Field>
      <Field label={t("Reorder quantity")} error={fe("reorder_qty")}>
        <input type="number" min={0} step="any" value={value.reorder_qty} onChange={(e) => set("reorder_qty", Number(e.target.value))} />
      </Field>
      <Field label={t("Supplier")}>
        <select value={value.supplier_id ?? ""} onChange={(e) => set("supplier_id", e.target.value || null)}>
          <option value="">{t("— None —")}</option>
          {suppliers.data?.map((s) => (
            <option key={s.id} value={s.id}>
              {s.name}
            </option>
          ))}
        </select>
      </Field>
      <Field label={t("Unit")} hint={t("What one quantity means: each, set, L, qt, gal, ft, m, kg…")}>
        <input type="text" list="am-units" value={value.unit} onChange={(e) => set("unit", e.target.value)} placeholder="each" />
        <datalist id="am-units">
          {["each", "pair", "set", "kit", "bottle", "case", "L", "mL", "qt", "gal", "ft", "m", "kg", "lb", "oz"].map((u) => (
            <option key={u} value={u} />
          ))}
        </datalist>
      </Field>
      <label className="row small" style={{ alignSelf: "end", minHeight: 36 }}>
        <input type="checkbox" checked={value.fractional} onChange={(e) => set("fractional", e.target.checked)} />{" "}{t("Sold in parts of a unit (e.g. 4.5 L of oil)")}</label>
      <Field label={t("Warranty")}>
        <input type="text" value={value.warranty} onChange={(e) => set("warranty", e.target.value)} />
      </Field>
      <Field label={t("Notes")} className="col-span-2">
        <input type="text" value={value.notes} onChange={(e) => set("notes", e.target.value)} />
      </Field>
    </div>
  );
}

/** Add an inventory item: for an existing catalog part, or describe a new one. Optionally
 * receive opening stock in the same step. */
export function NewItem({ partId, partName, onClose }: { partId?: string; partName?: string; onClose: () => void }) {
  const go = useApp((s) => s.go);
  const { settings } = useSettings();
  const locs = useCmd<Location[]>("location_list");
  const [mode, setMode] = useState<"existing" | "new">(partId ? "existing" : "new");
  const [pid, setPid] = useState(partId ?? "");
  const [partQ, setPartQ] = useState("");
  const parts = useCmd<PartRow[]>("part_list", { query: { q: partQ || null, category_id: null, brand: null, limit: 30 } }, mode === "existing" && !partId);
  const [part, setPart] = useState<PartInput>(emptyPart());
  const [item, setItem] = useState<ItemInput>(itemInput(undefined, settings?.default_min_stock ?? 0));
  const [qty, setQty] = useState("");
  const [loc, setLoc] = useState("");
  const [bin, setBin] = useState("");
  const [error, setError] = useState<unknown>(null);
  useEffect(() => {
    if (!loc && locs.data) setLoc(locs.data.find((l) => l.is_default)?.id ?? "");
  }, [locs.data, loc]);
  const refresh = useRefresh();
  const create = async () => {
    setError(null);
    try {
      const created = await call<Item>("item_create", { partId: mode === "existing" ? pid || null : null, newPart: mode === "new" ? part : null, input: item });
      if (Number(qty) > 0) {
        await call("stock_receive", { input: { item_id: created.id, location_id: loc, qty: Number(qty), unit_cost: item.cost || null, bin: bin || null, reason: t("Opening stock"), ref_kind: null, ref_id: null } });
      }
      refresh();
      onClose();
      go({ page: "item", id: created.id });
    } catch (e) {
      setError(e);
    }
  };
  const generalError = error && !errorField(error);
  return (
    <Modal
      open
      wide
      title={partName ? t("Stock {0}", partName) : t("Add inventory item")}
      onClose={onClose}
      footer={
        <>
          <button className="btn" onClick={onClose}>{t("Cancel")}</button>
          <button className="btn primary" onClick={create} disabled={mode === "existing" && !pid}>{t("Add to inventory")}</button>
        </>
      }
    >
      {!partId && (
        <div className="segmented" role="radiogroup" aria-label={t("Part")}>
          <label>
            <input type="radio" checked={mode === "new"} onChange={() => setMode("new")} />{" "}{t("New part")}</label>
          <label>
            <input type="radio" checked={mode === "existing"} onChange={() => setMode("existing")} />{" "}{t("Part already in the catalog")}</label>
        </div>
      )}
      {mode === "new" && <PartFields value={part} onChange={setPart} error={error} />}
      {mode === "existing" && !partId && (
        <Field label={t("Catalog part")}>
          <input type="search" placeholder={t("Search by name or number")} value={partQ} onChange={(e) => setPartQ(e.target.value)} />
          <select size={6} value={pid} onChange={(e) => setPid(e.target.value)} aria-label={t("Matching parts")}>
            {parts.data?.map((p) => (
              <option key={p.id} value={p.id}>
                {p.name} {p.brand && `· ${p.brand}`} {p.numbers && `· ${p.numbers}`}
              </option>
            ))}
          </select>
        </Field>
      )}
      <p className="label mt-2">{t("Stock record")}</p>
      <ItemFields value={item} onChange={setItem} error={error} />
      <p className="label mt-2">{t("Opening stock (optional)")}</p>
      <div className="grid" style={{ gridTemplateColumns: "120px 1fr 1fr" }}>
        <Field label={t("Quantity")}>
          <input type="number" min={0} step="any" value={qty} onChange={(e) => setQty(e.target.value)} />
        </Field>
        <Field label={t("Location")}>
          <select value={loc} onChange={(e) => setLoc(e.target.value)}>
            {locs.data
              ?.filter((l) => l.kind === "warehouse")
              .map((l) => (
                <option key={l.id} value={l.id}>
                  {l.name}
                </option>
              ))}
          </select>
        </Field>
        <Field label={t("Shelf / bin")}>
          <input type="text" value={bin} onChange={(e) => setBin(e.target.value)} placeholder="e.g. A-3" />
        </Field>
      </div>
      {generalError ? <ErrorNote error={error} /> : null}
    </Modal>
  );
}

export function EditItem({ item, onClose }: { item: Item; onClose: () => void }) {
  const [v, setV] = useState(itemInput(item));
  const save = useAction<{ id: string; version: number; input: ItemInput }>("item_update", { success: t("Saved"), silentError: true });
  const { setDirty } = useApp();
  useEffect(() => {
    setDirty(true);
    return () => setDirty(false);
  }, [setDirty]);
  return (
    <Modal
      open
      wide
      title={t("Edit {0}", item.sku)}
      onClose={onClose}
      footer={
        <>
          <button className="btn" onClick={onClose}>{t("Cancel")}</button>
          <button className="btn primary" onClick={() => save.mutate({ id: item.id, version: item.version, input: v }, { onSuccess: onClose })}>{t("Save")}</button>
        </>
      }
    >
      <ItemFields value={v} onChange={setV} error={save.error} />
      {save.error != null && !errorField(save.error) && <ErrorNote error={save.error} />}
    </Modal>
  );
}
