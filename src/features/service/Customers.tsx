import { useState } from "react";
import { Plus, Users } from "lucide-react";
import { errorField, errorMessage } from "../../api/ipc";
import { useAction, useCmd } from "../../api/queries";
import type { Customer, CustomerInput, Job, Profile, Vehicle } from "../../api/types";
import { useApp } from "../../store";
import { ActionsMenu, Badge, DataTable, Empty, Field, Loaded, Modal, PageHead, useConfirm } from "../../ui";
import { STATUS } from "../vehicles/VehicleService";
import { vehicleTitle } from "../vehicles/Garage";

export default function Customers() {
  const { route } = useApp();
  const [q, setQ] = useState("");
  const list = useCmd<Customer[]>("customer_list", { q: q || null });
  const [editing, setEditing] = useState<Customer | "new" | null>(null);
  const [open, setOpen] = useState<string | null>(route.id ?? null);
  const del = useAction<{ id: string }>("customer_delete", { success: "Customer deleted" });
  const { ask, dialog } = useConfirm();
  return (
    <div className="page wide">
      {dialog}
      <PageHead back title="Customers" sub="Kept on this computer only. Vehicles, estimates, jobs and invoices link to them.">
        <button className="btn primary" onClick={() => setEditing("new")}>
          <Plus size={16} aria-hidden /> Add customer
        </button>
      </PageHead>
      <input type="search" placeholder="Search name, phone or email" value={q} onChange={(e) => setQ(e.target.value)} aria-label="Search customers" className="mb-3" style={{ width: 340 }} />
      <Loaded q={list}>
        {(rows) =>
          rows.length === 0 ? (
            <Empty icon={<Users size={20} />} title={q ? "No customers match" : "No customers yet"} />
          ) : (
            <DataTable
              label="Customers"
              rows={rows}
              rowKey={(c) => c.id}
              onOpen={(c) => setOpen(c.id)}
              height="calc(100vh - 300px)"
              menu={(c) => [
                { label: "Open", run: () => setOpen(c.id) },
                { label: "Edit", run: () => setEditing(c) },
                null,
                { label: "Delete", danger: true, run: async () => (await ask(`Delete ${c.name}?`, "Their vehicles and jobs stay, without the customer link.")) && del.mutate({ id: c.id }) },
              ]}
              columns={[
                { id: "name", label: "Name", width: "minmax(160px, 2fr)", render: (c) => c.name, sort: (c) => c.name },
                { id: "phone", label: "Phone", width: "140px", render: (c) => c.phone },
                { id: "email", label: "Email", width: "minmax(160px, 1.5fr)", render: (c) => c.email },
                { id: "v", label: "Vehicles", width: "90px", render: (c) => c.vehicle_count, align: "right", sort: (c) => c.vehicle_count },
                { id: "j", label: "Jobs", width: "70px", render: (c) => c.job_count, align: "right", sort: (c) => c.job_count },
                { id: "a", label: "", width: "44px", render: (c) => <ActionsMenu items={[{ label: "Edit", run: () => setEditing(c) }]} /> },
              ]}
            />
          )
        }
      </Loaded>
      {editing && <EditCustomer c={editing === "new" ? null : editing} onClose={() => setEditing(null)} />}
      {open && <CustomerCard id={open} onClose={() => setOpen(null)} onEdit={(c) => setEditing(c)} />}
    </div>
  );
}

function CustomerCard({ id, onClose, onEdit }: { id: string; onClose: () => void; onEdit: (c: Customer) => void }) {
  const go = useApp((s) => s.go);
  const c = useCmd<Customer>("customer_get", { id });
  const jobs = useCmd<Job[]>("job_list", { query: { status: null, vehicle_id: null, customer_id: id } });
  const vehicles = useCmd<Vehicle[]>("vehicle_list", { saved: true });
  const mine = vehicles.data?.filter((v) => v.customer_id === id) ?? [];
  return (
    <Modal open wide title={c.data?.name ?? "Customer"} onClose={onClose} footer={c.data && <button className="btn" onClick={() => (onClose(), onEdit(c.data!))}>Edit</button>}>
      {c.data && (
        <dl className="kv">
          {c.data.phone && (
            <>
              <dt>Phone</dt>
              <dd>{c.data.phone}</dd>
            </>
          )}
          {c.data.email && (
            <>
              <dt>Email</dt>
              <dd>{c.data.email}</dd>
            </>
          )}
          {c.data.address && (
            <>
              <dt>Address</dt>
              <dd style={{ whiteSpace: "pre-wrap" }}>{c.data.address}</dd>
            </>
          )}
          {c.data.discount_pct && (
            <>
              <dt>Discount</dt>
              <dd>{c.data.discount_pct}%</dd>
            </>
          )}
          {c.data.notes && (
            <>
              <dt>Notes</dt>
              <dd style={{ whiteSpace: "pre-wrap" }}>{c.data.notes}</dd>
            </>
          )}
        </dl>
      )}
      <p className="label">Vehicles</p>
      {mine.length === 0 ? (
        <p className="small muted">None. Set the owner on a vehicle's “Edit details”.</p>
      ) : (
        <ul className="items" style={{ maxHeight: "none" }}>
          {mine.map((v) => (
            <li key={v.id}>
              <button className="btn link" style={{ padding: 0 }} onClick={() => go({ page: "vehicle", id: v.id })}>
                {vehicleTitle(v)}
              </button>
              <span className="small muted">{v.plate || v.vin}</span>
            </li>
          ))}
        </ul>
      )}
      <p className="label">Estimates and jobs</p>
      {jobs.data?.length ? (
        <ul className="items" style={{ maxHeight: "none" }}>
          {jobs.data.map((j) => (
            <li key={j.id}>
              <button className="btn link" style={{ padding: 0 }} onClick={() => go({ page: "job", id: j.id })}>
                #{j.number} {j.title}
              </button>
              <span className="ml-auto">
                <Badge tone={STATUS[j.status].tone}>{STATUS[j.status].label}</Badge>
              </span>
            </li>
          ))}
        </ul>
      ) : (
        <p className="small muted">None yet.</p>
      )}
    </Modal>
  );
}

function EditCustomer({ c, onClose }: { c: Customer | null; onClose: () => void }) {
  const profiles = useCmd<Profile[]>("profile_list");
  const [f, setF] = useState<CustomerInput>({ name: c?.name ?? "", phone: c?.phone ?? "", email: c?.email ?? "", address: c?.address ?? "", notes: c?.notes ?? "", pricing_profile_id: c?.pricing_profile_id ?? null, discount_pct: c?.discount_pct ?? null });
  const save = useAction<{ id: string | null; input: CustomerInput }>("customer_save", { success: "Saved", silentError: true });
  const fe = (k: string) => (save.error && errorField(save.error) === k ? errorMessage(save.error) : null);
  const set = <K extends keyof CustomerInput>(k: K, v: CustomerInput[K]) => setF({ ...f, [k]: v });
  return (
    <Modal
      open
      title={c ? "Edit customer" : "Add customer"}
      onClose={onClose}
      footer={
        <>
          <button className="btn" onClick={onClose}>
            Cancel
          </button>
          <button className="btn primary" onClick={() => save.mutate({ id: c?.id ?? null, input: f }, { onSuccess: onClose })}>
            Save
          </button>
        </>
      }
    >
      <Field label="Name" error={fe("name")}>
        <input type="text" value={f.name} onChange={(e) => set("name", e.target.value)} autoFocus />
      </Field>
      <div className="grid two">
        <Field label="Phone">
          <input type="tel" value={f.phone} onChange={(e) => set("phone", e.target.value)} />
        </Field>
        <Field label="Email" error={fe("email")}>
          <input type="email" value={f.email} onChange={(e) => set("email", e.target.value)} />
        </Field>
      </div>
      <Field label="Address">
        <textarea value={f.address} onChange={(e) => set("address", e.target.value)} />
      </Field>
      <div className="grid two">
        <Field label="Pricing profile" error={fe("pricing_profile_id")}>
          <select value={f.pricing_profile_id ?? ""} onChange={(e) => set("pricing_profile_id", e.target.value || null)}>
            <option value="">Default</option>
            {profiles.data?.map((p) => (
              <option key={p.id} value={p.id}>
                {p.name}
              </option>
            ))}
          </select>
        </Field>
        <Field label="Standing discount (%)" error={fe("discount_pct")}>
          <input type="text" inputMode="decimal" value={f.discount_pct ?? ""} onChange={(e) => set("discount_pct", e.target.value || null)} />
        </Field>
      </div>
      <Field label="Notes">
        <textarea value={f.notes} onChange={(e) => set("notes", e.target.value)} />
      </Field>
      {save.error != null && !errorField(save.error) && <div className="notice bad">{errorMessage(save.error)}</div>}
    </Modal>
  );
}
