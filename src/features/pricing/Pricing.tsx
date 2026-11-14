import { useEffect, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { Calculator, Plus, Star, Trash2 } from "lucide-react";
import { call, errorField, errorMessage } from "../../api/ipc";
import { useAction, useCmd } from "../../api/queries";
import type { Category, Item, PriceResult, Profile, ProfileInput, Rule, RuleInput } from "../../api/types";
import { useApp } from "../../store";
import { ActionsMenu, Badge, cx, Field, Loaded, Modal, PageHead, useConfirm } from "../../ui";
import { money, pct } from "../../lib/format";

const KIND: Record<string, string> = { retail: "Retail", wholesale: "Wholesale", internal: "Internal cost", fleet: "Fleet", customer: "Customer-specific", custom: "Custom" };

export default function Pricing() {
  const go = useApp((s) => s.go);
  const q = useCmd<Profile[]>("profile_list");
  const [sel, setSel] = useState<string | null>(null);
  const [editing, setEditing] = useState<Profile | "new" | null>(null);
  useEffect(() => {
    if (!sel && q.data?.length) setSel(q.data.find((p) => p.is_default)?.id ?? q.data[0].id);
  }, [q.data, sel]);
  const p = q.data?.find((x) => x.id === sel);
  return (
    <div className="page wide">
      <PageHead title="Pricing & Estimates" sub="How selling prices are worked out from cost: markup, target margin or fixed prices, by category, brand or item.">
        <button className="btn" onClick={() => go({ page: "service", tab: "new" })}>
          New estimate
        </button>
        <button className="btn primary" onClick={() => setEditing("new")}>
          <Plus size={16} aria-hidden /> New profile
        </button>
      </PageHead>
      <Loaded q={q}>
        {(list) => (
          <div className="flex gap-4" style={{ alignItems: "flex-start" }}>
            <nav className="card" style={{ width: 240, flex: "none", padding: 8 }} aria-label="Pricing profiles">
              {list.map((x) => (
                <button key={x.id} className={cx("btn ghost w-full", x.id === sel && "soft")} style={{ justifyContent: "space-between" }} aria-current={x.id === sel} onClick={() => setSel(x.id)}>
                  <span className="truncate">{x.name}</span>
                  {x.is_default && <Star size={13} aria-label="Default" />}
                </button>
              ))}
            </nav>
            {p && <ProfileView p={p} onEdit={() => setEditing(p)} onDeleted={() => setSel(null)} />}
          </div>
        )}
      </Loaded>
      {editing && <EditProfile p={editing === "new" ? null : editing} onClose={(id) => (setEditing(null), id && setSel(id))} />}
    </div>
  );
}

function ProfileView({ p, onEdit, onDeleted }: { p: Profile; onEdit: () => void; onDeleted: () => void }) {
  const [rule, setRule] = useState<Rule | "new" | null>(null);
  const setDefault = useAction<{ id: string }>("profile_set_default", { success: "Default profile changed" });
  const del = useAction<{ id: string }>("profile_delete", { success: "Profile deleted" });
  const delRule = useAction<{ id: string }>("rule_delete", { success: "Rule removed" });
  const toggle = useAction<{ profileId: string; id: string; input: RuleInput }>("rule_save", {});
  const { ask, dialog } = useConfirm();
  const method = (r: Rule) => (r.method === "fixed" ? `Fixed ${money(r.value)}` : r.method === "markup" ? `${pct(r.value)} markup on cost` : `${pct(r.value)} margin`);
  const scope = (r: Rule) => (r.scope === "all" ? "All products" : r.scope === "category" ? `Category: ${r.scope_label}` : r.scope === "brand" ? `Brand: ${r.scope_label}` : `Item: ${r.scope_label}`);
  return (
    <div className="flex-1 min-w-0 stack">
      {dialog}
      <section className="card">
        <div className="spread">
          <div>
            <h2 style={{ fontSize: 18 }}>{p.name}</h2>
            <p className="row small muted">
              <Badge>{KIND[p.kind]}</Badge> Tax {pct(p.tax_rate_pct, 3)}
              {p.prices_include_tax ? " (included in prices)" : " (added to prices)"} · Rounding: {p.rounding_mode === "ending" ? `prices end in .${String(p.rounding_ending ?? "0.99").split(".")[1] ?? "99"}` : `${p.rounding_mode} ${money(p.rounding_increment)}`} · Labor {p.labor_rate ? `${money(p.labor_rate)}/h` : "not set"}
            </p>
          </div>
          <div className="row">
            {!p.is_default && (
              <button className="btn" onClick={() => setDefault.mutate({ id: p.id })}>
                <Star size={14} aria-hidden /> Make default
              </button>
            )}
            <button className="btn" onClick={onEdit}>
              Edit settings
            </button>
            {!p.is_default && (
              <ActionsMenu items={[{ label: "Delete profile", danger: true, icon: <Trash2 size={14} />, run: async () => (await ask(`Delete ${p.name}?`, "Items, customers and jobs using it fall back to the default profile.")) && del.mutate({ id: p.id }, { onSuccess: onDeleted }) }]} />
            )}
          </div>
        </div>
      </section>
      <section className="card">
        <div className="spread">
          <h2>Rules</h2>
          <button className="btn" onClick={() => setRule("new")}>
            <Plus size={14} aria-hidden /> Add rule
          </button>
        </div>
        <p className="small muted">The rule with the lowest priority number that applies is used; at equal priority the most specific wins (item, then brand, then category, then all). A fixed price set on an item beats every rule. With no applicable rule there's no price: nothing is assumed.</p>
        {p.rules.length === 0 ? (
          <div className="notice warn">This profile has no rules yet, so items without a fixed price have no selling price. Add a rule such as “All products: 40% markup on cost”.</div>
        ) : (
          <table>
            <thead>
              <tr>
                <th>Priority</th>
                <th>Applies to</th>
                <th>Price</th>
                <th>Min qty</th>
                <th>Min price</th>
                <th>On</th>
                <th />
              </tr>
            </thead>
            <tbody>
              {p.rules.map((r) => (
                <tr key={r.id} style={!r.enabled ? { opacity: 0.55 } : undefined}>
                  <td>{r.priority}</td>
                  <td>
                    {scope(r)}
                    {r.note && <div className="small muted">{r.note}</div>}
                  </td>
                  <td>{method(r)}</td>
                  <td>{r.min_qty > 1 ? `${r.min_qty}+` : "—"}</td>
                  <td>{r.min_price ? money(r.min_price) : "—"}</td>
                  <td>
                    <input
                      type="checkbox"
                      role="switch"
                      className="switch"
                      checked={r.enabled}
                      aria-label="Rule enabled"
                      onChange={(e) =>
                        toggle.mutate({ profileId: p.id, id: r.id, input: { priority: r.priority, scope: r.scope, scope_value: r.scope_value, method: r.method, value: r.value, min_qty: r.min_qty, min_price: r.min_price, enabled: e.target.checked, note: r.note } })
                      }
                    />
                  </td>
                  <td>
                    <ActionsMenu
                      items={[
                        { label: "Edit", run: () => setRule(r) },
                        { label: "Remove", danger: true, run: () => delRule.mutate({ id: r.id }) },
                      ]}
                    />
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
      </section>
      <TryIt profileId={p.id} />
      {rule && <EditRule profile={p} r={rule === "new" ? null : rule} onClose={() => setRule(null)} />}
    </div>
  );
}

/** Calculate a price for a cost, category and brand, with the full working. */
function TryIt({ profileId }: { profileId: string }) {
  const cats = useCmd<Category[]>("categories");
  const [f, setF] = useState({ cost: "50", category: "", brand: "", qty: "1" });
  const r = useQuery<PriceResult>({
    queryKey: ["price_preview", profileId, f],
    queryFn: () => call<PriceResult>("price_preview", { profileId, cost: f.cost, categoryId: f.category || null, brand: f.brand || null, qty: Number(f.qty) || 1 }),
    enabled: !!f.cost.trim(),
    retry: false,
  });
  return (
    <section className="card">
      <h2>
        <Calculator size={16} aria-hidden /> Try it
      </h2>
      <div className="grid" style={{ gridTemplateColumns: "120px 1fr 1fr 90px" }}>
        <Field label="Cost">
          <input type="text" inputMode="decimal" value={f.cost} onChange={(e) => setF({ ...f, cost: e.target.value })} />
        </Field>
        <Field label="Category">
          <select value={f.category} onChange={(e) => setF({ ...f, category: e.target.value })}>
            <option value="">—</option>
            {cats.data?.map((c) => (
              <option key={c.id} value={c.id}>
                {c.parent_id ? "   " : ""}
                {c.name}
              </option>
            ))}
          </select>
        </Field>
        <Field label="Brand">
          <input type="text" value={f.brand} onChange={(e) => setF({ ...f, brand: e.target.value })} />
        </Field>
        <Field label="Qty">
          <input type="number" min={1} value={f.qty} onChange={(e) => setF({ ...f, qty: e.target.value })} />
        </Field>
      </div>
      {r.isError && <div className="notice bad mt-2">{errorMessage(r.error)}</div>}
      {r.data && (
        <div className="mt-2">
          <ol className="small" style={{ paddingLeft: 18 }}>
            {r.data.steps.map((s, i) => (
              <li key={i}>{s}</li>
            ))}
          </ol>
          {r.data.price && (
            <p>
              <strong style={{ fontSize: 18 }}>{money(r.data.price)}</strong> <span className="muted">· profit {money(r.data.profit)} · margin {pct(r.data.margin_pct)} · markup {pct(r.data.markup_pct)}</span>
            </p>
          )}
        </div>
      )}
    </section>
  );
}

function EditProfile({ p, onClose }: { p: Profile | null; onClose: (id?: string) => void }) {
  const [f, setF] = useState<ProfileInput>({
    name: p?.name ?? "",
    kind: p?.kind ?? "custom",
    tax_rate_pct: p ? String(p.tax_rate_pct) : "0",
    prices_include_tax: p?.prices_include_tax ?? false,
    rounding_increment: p ? String(p.rounding_increment) : "0.01",
    rounding_mode: p?.rounding_mode ?? "nearest",
    rounding_ending: p?.rounding_ending ?? null,
    labor_rate: p?.labor_rate ?? null,
    notes: p?.notes ?? "",
  });
  const save = useAction<{ id: string | null; input: ProfileInput }, Profile>("profile_save", { success: "Saved", silentError: true });
  const fe = (k: string) => (save.error && errorField(save.error) === k ? errorMessage(save.error) : null);
  const set = <K extends keyof ProfileInput>(k: K, v: ProfileInput[K]) => setF({ ...f, [k]: v });
  return (
    <Modal
      open
      title={p ? "Profile settings" : "New pricing profile"}
      onClose={() => onClose()}
      footer={
        <>
          <button className="btn" onClick={() => onClose()}>
            Cancel
          </button>
          <button className="btn primary" onClick={() => save.mutate({ id: p?.id ?? null, input: f }, { onSuccess: (np) => onClose(np.id) })}>
            Save
          </button>
        </>
      }
    >
      <div className="grid two">
        <Field label="Name" error={fe("name")}>
          <input type="text" value={f.name} onChange={(e) => set("name", e.target.value)} autoFocus />
        </Field>
        <Field label="Type">
          <select value={f.kind} onChange={(e) => set("kind", e.target.value)}>
            {Object.entries(KIND).map(([k, l]) => (
              <option key={k} value={k}>
                {l}
              </option>
            ))}
          </select>
        </Field>
        <Field label="Tax rate (%)" error={fe("tax_rate_pct")}>
          <input type="text" inputMode="decimal" value={f.tax_rate_pct} onChange={(e) => set("tax_rate_pct", e.target.value)} />
        </Field>
        <Field label="Labor rate per hour" error={fe("labor_rate")}>
          <input type="text" inputMode="decimal" value={f.labor_rate ?? ""} onChange={(e) => set("labor_rate", e.target.value || null)} />
        </Field>
        <Field label="Rounding" error={fe("rounding_mode")}>
          <select value={f.rounding_mode} onChange={(e) => set("rounding_mode", e.target.value)}>
            <option value="nearest">To the nearest…</option>
            <option value="up">Up to the next…</option>
            <option value="down">Down to…</option>
            <option value="ending">Price ending (e.g. .99)</option>
          </select>
        </Field>
        {f.rounding_mode === "ending" ? (
          <Field label="Ending" error={fe("rounding_ending")}>
            <input type="text" inputMode="decimal" value={f.rounding_ending ?? "0.99"} onChange={(e) => set("rounding_ending", e.target.value)} />
          </Field>
        ) : (
          <Field label="Increment" hint="0.01 = to the cent, 0.05, 1.00 …" error={fe("rounding_increment")}>
            <input type="text" inputMode="decimal" value={f.rounding_increment} onChange={(e) => set("rounding_increment", e.target.value)} />
          </Field>
        )}
      </div>
      <label className="check">
        <input type="checkbox" checked={f.prices_include_tax} onChange={(e) => set("prices_include_tax", e.target.checked)} /> Prices include tax (tax is worked out of the price instead of added)
      </label>
      <Field label="Notes">
        <input type="text" value={f.notes} onChange={(e) => set("notes", e.target.value)} />
      </Field>
      {save.error != null && !errorField(save.error) && <div className="notice bad">{errorMessage(save.error)}</div>}
    </Modal>
  );
}

function EditRule({ profile, r, onClose }: { profile: Profile; r: Rule | null; onClose: () => void }) {
  const cats = useCmd<Category[]>("categories");
  const brands = useCmd<string[]>("part_brands");
  const items = useCmd<Item[]>("item_list", { query: { q: null, category_id: null, location_id: null, stock: null, include_archived: false } });
  const [f, setF] = useState<RuleInput>({ priority: r?.priority ?? 100, scope: r?.scope ?? "all", scope_value: r?.scope_value ?? null, method: r?.method ?? "markup", value: r ? String(r.value) : "", min_qty: r?.min_qty ?? 1, min_price: r?.min_price ?? null, enabled: r?.enabled ?? true, note: r?.note ?? "" });
  const save = useAction<{ profileId: string; id: string | null; input: RuleInput }>("rule_save", { success: "Rule saved", silentError: true });
  const fe = (k: string) => (save.error && errorField(save.error) === k ? errorMessage(save.error) : null);
  const set = <K extends keyof RuleInput>(k: K, v: RuleInput[K]) => setF({ ...f, [k]: v });
  return (
    <Modal
      open
      title={r ? "Edit rule" : "Add rule"}
      onClose={onClose}
      footer={
        <>
          <button className="btn" onClick={onClose}>
            Cancel
          </button>
          <button className="btn primary" onClick={() => save.mutate({ profileId: profile.id, id: r?.id ?? null, input: f }, { onSuccess: onClose })}>
            Save
          </button>
        </>
      }
    >
      <div className="grid two">
        <Field label="Applies to" error={fe("scope")}>
          <select value={f.scope} onChange={(e) => setF({ ...f, scope: e.target.value, scope_value: null })}>
            <option value="all">All products</option>
            <option value="category">A category</option>
            <option value="brand">A brand</option>
            <option value="item">One item</option>
          </select>
        </Field>
        {f.scope !== "all" && (
          <Field label={f.scope === "category" ? "Category" : f.scope === "brand" ? "Brand" : "Item"} error={fe("scope_value")}>
            {f.scope === "brand" ? (
              <>
                <input type="text" list="rule-brands" value={f.scope_value ?? ""} onChange={(e) => set("scope_value", e.target.value)} />
                <datalist id="rule-brands">{brands.data?.map((b) => <option key={b} value={b} />)}</datalist>
              </>
            ) : (
              <select value={f.scope_value ?? ""} onChange={(e) => set("scope_value", e.target.value || null)}>
                <option value="">Choose…</option>
                {f.scope === "category"
                  ? cats.data?.map((c) => (
                      <option key={c.id} value={c.id}>
                        {c.parent_id ? "   " : ""}
                        {c.name}
                      </option>
                    ))
                  : items.data?.map((i) => (
                      <option key={i.id} value={i.id}>
                        {i.sku} · {i.name}
                      </option>
                    ))}
              </select>
            )}
          </Field>
        )}
        <Field label="Price by">
          <select value={f.method} onChange={(e) => set("method", e.target.value)}>
            <option value="markup">Markup on cost (%)</option>
            <option value="margin">Target margin (%)</option>
            <option value="fixed">Fixed price</option>
          </select>
        </Field>
        <Field label={f.method === "fixed" ? "Price" : "Percent"} error={fe("value")} hint={f.method === "markup" ? "40 → cost × 1.40" : f.method === "margin" ? "40 → profit is 40% of the price (cost ÷ 0.60)" : undefined}>
          <input type="text" inputMode="decimal" value={f.value} onChange={(e) => set("value", e.target.value)} />
        </Field>
        <Field label="Priority" hint="Lower numbers are tried first.">
          <input type="number" value={f.priority} onChange={(e) => set("priority", Number(e.target.value))} />
        </Field>
        <Field label="Only from quantity" error={fe("min_qty")} hint="For quantity breaks.">
          <input type="number" min={1} value={f.min_qty} onChange={(e) => set("min_qty", Number(e.target.value))} />
        </Field>
        <Field label="Minimum price (optional)" error={fe("min_price")}>
          <input type="text" inputMode="decimal" value={f.min_price ?? ""} onChange={(e) => set("min_price", e.target.value || null)} />
        </Field>
        <Field label="Note">
          <input type="text" value={f.note} onChange={(e) => set("note", e.target.value)} />
        </Field>
      </div>
      {save.error != null && !errorField(save.error) && <div className="notice bad">{errorMessage(save.error)}</div>}
    </Modal>
  );
}
