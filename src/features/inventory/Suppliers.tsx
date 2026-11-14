import { useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { FileSpreadsheet, Plus, Star, Truck } from "lucide-react";
import { call, errorField, errorMessage } from "../../api/ipc";
import { useAction, useCmd, useRefresh } from "../../api/queries";
import type { Supplier, SupplierInput, SupplierPrice } from "../../api/types";
import { toast } from "../../store";
import { ActionsMenu, Badge, Empty, Field, Loaded, Modal, PageHead, useConfirm } from "../../ui";
import { money } from "../../lib/format";
import { t } from "../../i18n";

export default function Suppliers() {
  const q = useCmd<Supplier[]>("supplier_list");
  const [editing, setEditing] = useState<Supplier | "new" | null>(null);
  const [prices, setPrices] = useState<Supplier | null>(null);
  const del = useAction<{ id: string }>("supplier_delete", { success: t("Supplier deleted") });
  const refresh = useRefresh();
  const { ask, dialog } = useConfirm();
  const importList = async (s: Supplier) => {
    const p = await open({ filters: [{ name: t("Price list"), extensions: ["csv", "xlsx", "xls", "ods"] }] });
    if (typeof p !== "string") return;
    try {
      const r = await call<{ updated: number; not_found: string[] }>("supplier_price_list_import", { supplierId: s.id, path: p });
      toast(t("Updated {0} prices{1}", r.updated, r.not_found.length ? `; ${r.not_found.length} lines didn't match an item` : ""));
      refresh();
    } catch (e) {
      toast(errorMessage(e), { tone: "bad" });
    }
  };
  return (
    <div className="page">
      {dialog}
      <PageHead back title={t("Suppliers")} sub={t("Who you buy from, their part numbers and prices.")}>
        <button className="btn primary" onClick={() => setEditing("new")}>
          <Plus size={16} aria-hidden />{" "}{t("Add supplier")}</button>
      </PageHead>
      <Loaded q={q}>
        {(list) =>
          list.length === 0 ? (
            <Empty icon={<Truck size={20} />} title={t("No suppliers yet")} />
          ) : (
            <div className="table-wrap">
              <table>
                <thead>
                  <tr>
                    <th>{t("Supplier")}</th>
                    <th>{t("Contact")}</th>
                    <th>{t("Account")}</th>
                    <th className="num">{t("Lead time")}</th>
                    <th className="num">{t("Items")}</th>
                    <th />
                  </tr>
                </thead>
                <tbody>
                  {list.map((s) => (
                    <tr key={s.id}>
                      <td>
                        {s.preferred && <Star size={13} aria-label={t("Preferred")} style={{ color: "var(--warn)" }} />} {s.name}
                        {s.website && <div className="small muted">{s.website}</div>}
                      </td>
                      <td className="small">{[s.contact, s.phone, s.email].filter(Boolean).join(" · ")}</td>
                      <td className="mono small">{s.account_no}</td>
                      <td className="num">{s.lead_time_days != null ? t("{0} days", s.lead_time_days) : "—"}</td>
                      <td className="num">{s.item_count}</td>
                      <td>
                        <ActionsMenu
                          items={[
                            { label: t("Edit"), run: () => setEditing(s) },
                            { label: t("Prices and part numbers"), run: () => setPrices(s) },
                            { label: t("Import a price list…"), icon: <FileSpreadsheet size={14} />, run: () => importList(s) },
                            null,
                            { label: t("Delete"), danger: true, run: async () => (await ask(t("Delete this supplier?"), t("Items keep their stock; they just lose this supplier link."))) && del.mutate({ id: s.id }) },
                          ]}
                        />
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          )
        }
      </Loaded>
      {editing && <EditSupplier s={editing === "new" ? null : editing} onClose={() => setEditing(null)} />}
      {prices && <Prices s={prices} onClose={() => setPrices(null)} />}
    </div>
  );
}

function EditSupplier({ s, onClose }: { s: Supplier | null; onClose: () => void }) {
  const [f, setF] = useState<SupplierInput>({ name: s?.name ?? "", contact: s?.contact ?? "", phone: s?.phone ?? "", email: s?.email ?? "", website: s?.website ?? "", account_no: s?.account_no ?? "", lead_time_days: s?.lead_time_days ?? null, preferred: s?.preferred ?? false, notes: s?.notes ?? "" });
  const save = useAction<{ id: string | null; input: SupplierInput }>("supplier_save", { success: t("Saved"), silentError: true });
  const fe = (k: string) => (save.error && errorField(save.error) === k ? errorMessage(save.error) : null);
  const set = <K extends keyof SupplierInput>(k: K, v: SupplierInput[K]) => setF({ ...f, [k]: v });
  return (
    <Modal
      open
      title={s ? t("Edit supplier") : t("Add supplier")}
      onClose={onClose}
      footer={
        <>
          <button className="btn" onClick={onClose}>{t("Cancel")}</button>
          <button className="btn primary" onClick={() => save.mutate({ id: s?.id ?? null, input: f }, { onSuccess: onClose })}>{t("Save")}</button>
        </>
      }
    >
      <Field label={t("Name")} error={fe("name")}>
        <input type="text" value={f.name} onChange={(e) => set("name", e.target.value)} autoFocus />
      </Field>
      <div className="grid two">
        <Field label={t("Contact person")}>
          <input type="text" value={f.contact} onChange={(e) => set("contact", e.target.value)} />
        </Field>
        <Field label={t("Phone")}>
          <input type="tel" value={f.phone} onChange={(e) => set("phone", e.target.value)} />
        </Field>
        <Field label={t("Email")} error={fe("email")}>
          <input type="email" value={f.email} onChange={(e) => set("email", e.target.value)} />
        </Field>
        <Field label={t("Website")}>
          <input type="url" value={f.website} onChange={(e) => set("website", e.target.value)} />
        </Field>
        <Field label={t("Your account number")}>
          <input type="text" value={f.account_no} onChange={(e) => set("account_no", e.target.value)} />
        </Field>
        <Field label={t("Usual lead time (days)")} error={fe("lead_time_days")}>
          <input type="number" min={0} value={f.lead_time_days ?? ""} onChange={(e) => set("lead_time_days", e.target.value ? Number(e.target.value) : null)} />
        </Field>
      </div>
      <label className="check">
        <input type="checkbox" checked={f.preferred} onChange={(e) => set("preferred", e.target.checked)} />{" "}{t("Preferred supplier")}</label>
      <Field label={t("Notes")}>
        <textarea value={f.notes} onChange={(e) => set("notes", e.target.value)} />
      </Field>
      {save.error != null && !errorField(save.error) && <div className="notice bad">{errorMessage(save.error)}</div>}
    </Modal>
  );
}

function Prices({ s, onClose }: { s: Supplier; onClose: () => void }) {
  const q = useCmd<SupplierPrice[]>("supplier_prices", { itemId: null, supplierId: s.id });
  return (
    <Modal open wide title={t("{0}: prices", s.name)} onClose={onClose}>
      <Loaded q={q}>
        {(list) =>
          list.length === 0 ? (
            <p className="muted">{t("No prices recorded. Add them on an item's page or import a price list.")}</p>
          ) : (
            <table>
              <thead>
                <tr>
                  <th>{t("Item")}</th>
                  <th>{t("Their number")}</th>
                  <th className="num">{t("Cost")}</th>
                </tr>
              </thead>
              <tbody>
                {list.map((p) => (
                  <tr key={p.item_id}>
                    <td>
                      {p.name} <span className="mono small muted">{p.sku}</span> {p.preferred && <Badge tone="info">{t("main supplier")}</Badge>}
                    </td>
                    <td className="mono">{p.supplier_sku}</td>
                    <td className="num">{p.cost ? money(p.cost) : "—"}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          )
        }
      </Loaded>
    </Modal>
  );
}
