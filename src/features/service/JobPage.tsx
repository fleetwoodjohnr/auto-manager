import { Attachments } from "../shared/Attachments";
import { useState } from "react";
import { CheckCircle2, ClipboardCheck, PackageCheck, PackageX, Play, Plus, Printer, Receipt, Trash2, Undo2, XCircle } from "lucide-react";
import { errorMessage } from "../../api/ipc";
import { useAction, useCmd } from "../../api/queries";
import type { Customer, Item, JobDetail, JobInput, Line, LineUpdate, OtherLineInput, PartLineInput, Profile, Vehicle } from "../../api/types";
import { useApp } from "../../store";
import { ActionsMenu, Badge, Field, Loaded, Modal, useConfirm } from "../../ui";
import { dateTime, money, num } from "../../lib/format";
import { STATUS } from "../vehicles/VehicleService";
import { vehicleTitle } from "../vehicles/Garage";

const NEXT: Record<string, { to: string; label: string; icon: typeof Play }[]> = {
  estimate: [
    { to: "approved", label: "Customer approved", icon: ClipboardCheck },
    { to: "in_progress", label: "Start work", icon: Play },
  ],
  approved: [
    { to: "in_progress", label: "Start work", icon: Play },
    { to: "estimate", label: "Back to estimate", icon: Undo2 },
  ],
  in_progress: [{ to: "completed", label: "Complete job", icon: CheckCircle2 }],
  completed: [{ to: "invoiced", label: "Create invoice", icon: Receipt }],
};

export default function JobPage() {
  const { route, go } = useApp();
  const id = route.id!;
  const q = useCmd<JobDetail>("job_get", { id });
  const [adding, setAdding] = useState<"part" | "labor" | "fee" | null>(null);
  const [editing, setEditing] = useState<Line | null>(null);
  const [details, setDetails] = useState(false);
  const status = useAction<{ id: string; status: string }, JobDetail>("job_set_status", { success: (d) => `Job #${d.job.number}: ${STATUS[d.job.status].label.toLowerCase()}` });
  const reserve = useAction<{ lineId: string }>("job_reserve_line", { success: "Stock reserved" });
  const release = useAction<{ lineId: string }>("job_release_line", { success: "Reservation released" });
  const del = useAction<{ lineId: string }>("job_delete_line", {});
  const delJob = useAction<{ id: string }>("job_delete", { success: "Deleted" });
  const { ask, dialog } = useConfirm();
  return (
    <div className="page wide">
      {dialog}
      <Loaded q={q}>
        {(d) => {
          const j = d.job;
          const locked = ["completed", "invoiced", "cancelled"].includes(j.status);
          const t = d.totals;
          return (
            <>
              <div className="page-head">
                <div className="min-w-0">
                  <button className="btn link mb-1" style={{ paddingLeft: 0 }} onClick={() => go("service")}>
                    ← Service jobs
                  </button>
                  <h1>
                    {j.invoice_number ? `Invoice ${j.invoice_number}` : j.status === "estimate" ? "Estimate" : "Job"} #{j.number} {j.title && `· ${j.title}`}
                  </h1>
                  <p className="row">
                    <Badge tone={STATUS[j.status].tone}>{STATUS[j.status].label}</Badge>
                    {j.vehicle_id ? (
                      <button className="btn link" style={{ padding: 0 }} onClick={() => go({ page: "vehicle", id: j.vehicle_id! })}>
                        {j.vehicle_title}
                      </button>
                    ) : (
                      <span className="muted">No vehicle</span>
                    )}
                    {j.customer_name && <span>· {j.customer_name}</span>}
                    <span className="muted">· {d.profile_name} pricing</span>
                  </p>
                </div>
                <div className="row">
                  {(NEXT[j.status] ?? []).map((n) => (
                    <button
                      key={n.to}
                      className={`btn ${n.to === "completed" || n.to === "invoiced" || n.to === "approved" ? "primary" : ""}`}
                      onClick={async () => {
                        if (n.to === "completed" && !(await ask("Complete this job?", "Every part on it is taken out of stock (reserved parts first). The job can't be edited afterwards.", "Complete"))) return;
                        status.mutate({ id, status: n.to });
                      }}
                    >
                      <n.icon size={16} aria-hidden /> {n.label}
                    </button>
                  ))}
                  <button className="btn" onClick={() => go({ page: "print", id })}>
                    <Printer size={16} aria-hidden /> Print / PDF
                  </button>
                  <ActionsMenu
                    items={[
                      !locked ? { label: "Edit details", run: () => setDetails(true) } : null,
                      !locked ? { label: "Cancel job", icon: <XCircle size={14} />, danger: true, run: async () => (await ask("Cancel this job?", "Reserved stock is released.", "Cancel job")) && status.mutate({ id, status: "cancelled" }) } : null,
                      j.status === "estimate" || j.status === "cancelled"
                        ? { label: "Delete", icon: <Trash2 size={14} />, danger: true, run: async () => (await ask("Delete this job?", "This can't be undone.")) && delJob.mutate({ id }, { onSuccess: () => go("service") }) }
                        : null,
                    ].filter((x) => x !== null)}
                  />
                </div>
              </div>
              {status.error != null && <div className="notice bad mb-3">{errorMessage(status.error)}</div>}
              <div className="grid" style={{ gridTemplateColumns: "minmax(0, 2.2fr) minmax(260px, 1fr)", alignItems: "start" }}>
                <section className="card">
                  <div className="spread">
                    <h2>Parts and labor</h2>
                    {!locked && (
                      <div className="row">
                        <button className="btn" onClick={() => setAdding("part")}>
                          <Plus size={14} aria-hidden /> Part
                        </button>
                        <button className="btn" onClick={() => setAdding("labor")}>
                          <Plus size={14} aria-hidden /> Labor
                        </button>
                        <button className="btn" onClick={() => setAdding("fee")}>
                          <Plus size={14} aria-hidden /> Fee
                        </button>
                      </div>
                    )}
                  </div>
                  {d.lines.length === 0 ? (
                    <p className="muted">Add parts from inventory, labor and fees. Part prices come from your pricing rules, and each line shows how its price was worked out.</p>
                  ) : (
                    <table>
                      <thead>
                        <tr>
                          <th>Description</th>
                          <th className="num">Qty</th>
                          <th className="num">Price</th>
                          <th className="num">Amount</th>
                          <th />
                        </tr>
                      </thead>
                      <tbody>
                        {d.lines.map((l) => (
                          <tr key={l.id}>
                            <td>
                              {l.description}
                              <div className="row small">
                                <Badge>{l.kind}</Badge>
                                {!l.taxable && <span className="muted">not taxed</span>}
                                {l.kind === "part" &&
                                  (l.consumed ? (
                                    <Badge tone="ok">taken from stock</Badge>
                                  ) : l.reservation_status === "active" ? (
                                    <Badge tone="info">reserved</Badge>
                                  ) : (
                                    <Badge tone="warn">not reserved</Badge>
                                  ))}
                                {l.explanation && (
                                  <details>
                                    <summary className="muted">how priced</summary>
                                    <ol className="small" style={{ paddingLeft: 18, margin: "4px 0" }}>
                                      {l.explanation.steps.map((s, k) => (
                                        <li key={k}>{s}</li>
                                      ))}
                                    </ol>
                                  </details>
                                )}
                              </div>
                            </td>
                            <td className="num">{num(l.qty, 2)}</td>
                            <td className="num">{money(l.unit_price)}</td>
                            <td className="num">{money(l.amount)}</td>
                            <td>
                              {!locked && (
                                <ActionsMenu
                                  items={[
                                    { label: "Edit", run: () => setEditing(l) },
                                    l.kind === "part" && l.item_id && l.reservation_status !== "active" ? { label: "Reserve stock", icon: <PackageCheck size={14} />, run: () => reserve.mutate({ lineId: l.id }) } : null,
                                    l.reservation_status === "active" ? { label: "Release reservation", icon: <PackageX size={14} />, run: () => release.mutate({ lineId: l.id }) } : null,
                                    { label: "Remove", icon: <Trash2 size={14} />, danger: true, run: () => del.mutate({ lineId: l.id }) },
                                  ].filter((x) => x !== null)}
                                />
                              )}
                            </td>
                          </tr>
                        ))}
                      </tbody>
                    </table>
                  )}
                  {(reserve.error ?? del.error) != null && <div className="notice bad mt-2">{errorMessage(reserve.error ?? del.error)}</div>}
                </section>
                <div className="stack">
                  <section className="card">
                    <h2>Totals</h2>
                    <dl className="kv">
                      <dt>Parts</dt>
                      <dd className="num">{money(t.parts)}</dd>
                      <dt>Labor</dt>
                      <dd className="num">{money(t.labor)}</dd>
                      {Number(t.fees) !== 0 && (
                        <>
                          <dt>Fees</dt>
                          <dd className="num">{money(t.fees)}</dd>
                        </>
                      )}
                      {Number(t.discount) !== 0 && (
                        <>
                          <dt>Discount ({num(j.discount_pct)}%)</dt>
                          <dd className="num">−{money(t.discount)}</dd>
                        </>
                      )}
                      <dt>Tax ({num(j.tax_rate_pct, 3)}%{j.prices_include_tax ? ", included" : ""})</dt>
                      <dd className="num">{money(t.tax)}</dd>
                      <dt>
                        <strong>Total</strong>
                      </dt>
                      <dd className="num">
                        <strong style={{ fontSize: 16 }}>{money(t.total)}</strong>
                      </dd>
                    </dl>
                    <p className="small muted mt-2">
                      Cost of parts {money(t.cost)} · gross profit {money(t.profit)}
                    </p>
                  </section>
                  <Attachments ownerKind="job" ownerId={j.id} title="Photos, receipts and invoices" />
                  <section className="card">
                    <h2>Details</h2>
                    <dl className="kv">
                      {j.complaint && (
                        <>
                          <dt>Request</dt>
                          <dd style={{ whiteSpace: "pre-wrap" }}>{j.complaint}</dd>
                        </>
                      )}
                      {j.mileage != null && (
                        <>
                          <dt>Mileage</dt>
                          <dd>{j.mileage.toLocaleString()}</dd>
                        </>
                      )}
                      {j.technician && (
                        <>
                          <dt>Technician</dt>
                          <dd>{j.technician}</dd>
                        </>
                      )}
                      <dt>Created</dt>
                      <dd>{dateTime(j.created_at)}</dd>
                      {j.approved_at && (
                        <>
                          <dt>Approved</dt>
                          <dd>{dateTime(j.approved_at)}</dd>
                        </>
                      )}
                      {j.completed_at && (
                        <>
                          <dt>Completed</dt>
                          <dd>{dateTime(j.completed_at)}</dd>
                        </>
                      )}
                    </dl>
                    {!locked && (
                      <button className="btn mt-2" onClick={() => setDetails(true)}>
                        Edit details
                      </button>
                    )}
                  </section>
                </div>
              </div>
              {adding === "part" && <AddPart d={d} onClose={() => setAdding(null)} />}
              {(adding === "labor" || adding === "fee") && <AddOther jobId={id} kind={adding} onClose={() => setAdding(null)} />}
              {editing && <EditLine line={editing} onClose={() => setEditing(null)} />}
              {details && <JobDetails d={d} onClose={() => setDetails(false)} />}
            </>
          );
        }}
      </Loaded>
    </div>
  );
}

function AddPart({ d, onClose }: { d: JobDetail; onClose: () => void }) {
  const [q, setQ] = useState("");
  const items = useCmd<Item[]>("item_list", { query: { q: q || null, category_id: null, location_id: null, stock: null, include_archived: false } });
  const [pick, setPick] = useState("");
  const [qty, setQty] = useState("1");
  const [res, setRes] = useState(true);
  const add = useAction<{ id: string; input: PartLineInput }>("job_add_part", { silentError: true, success: "Part added" });
  return (
    <Modal
      open
      title="Add a part from inventory"
      onClose={onClose}
      footer={
        <>
          <button className="btn" onClick={onClose}>
            Cancel
          </button>
          <button className="btn primary" disabled={!pick} onClick={() => add.mutate({ id: d.job.id, input: { item_id: pick, qty, reserve: res } }, { onSuccess: onClose })}>
            Add
          </button>
        </>
      }
    >
      <input type="search" placeholder="Search SKU, name or part number" value={q} onChange={(e) => setQ(e.target.value)} autoFocus aria-label="Search inventory" />
      <select size={8} value={pick} onChange={(e) => setPick(e.target.value)} aria-label="Items">
        {items.data?.map((i) => (
          <option key={i.id} value={i.id}>
            {i.name} · {i.sku} · {i.available} available
          </option>
        ))}
      </select>
      {d.job.vehicle_id && <p className="small muted">Tip: the vehicle's Parts in Stock tab lists only parts with fitment records for it.</p>}
      <Field label="Quantity">
        <input type="number" min={1} value={qty} onChange={(e) => setQty(e.target.value)} />
      </Field>
      <label className="check">
        <input type="checkbox" checked={res} onChange={(e) => setRes(e.target.checked)} /> Reserve the stock now
      </label>
      {add.error != null && <div className="notice bad">{errorMessage(add.error)}</div>}
    </Modal>
  );
}

function AddOther({ jobId, kind, onClose }: { jobId: string; kind: "labor" | "fee"; onClose: () => void }) {
  const [f, setF] = useState<OtherLineInput>({ kind, description: "", qty: "1", unit_price: null, taxable: kind === "fee" });
  const add = useAction<{ id: string; input: OtherLineInput }>("job_add_other", { silentError: true });
  return (
    <Modal
      open
      title={kind === "labor" ? "Add labor" : "Add a fee"}
      onClose={onClose}
      footer={
        <>
          <button className="btn" onClick={onClose}>
            Cancel
          </button>
          <button className="btn primary" onClick={() => add.mutate({ id: jobId, input: f }, { onSuccess: onClose })}>
            Add
          </button>
        </>
      }
    >
      <Field label="Description">
        <input type="text" value={f.description} onChange={(e) => setF({ ...f, description: e.target.value })} placeholder={kind === "labor" ? "e.g. Replace front brake pads" : "e.g. Shop supplies"} autoFocus />
      </Field>
      <div className="grid two">
        <Field label={kind === "labor" ? "Hours" : "Quantity"}>
          <input type="text" inputMode="decimal" value={f.qty} onChange={(e) => setF({ ...f, qty: e.target.value })} />
        </Field>
        <Field label={kind === "labor" ? "Rate per hour" : "Amount"} hint={kind === "labor" ? "Empty uses the pricing profile's labor rate." : undefined}>
          <input type="text" inputMode="decimal" value={f.unit_price ?? ""} onChange={(e) => setF({ ...f, unit_price: e.target.value || null })} />
        </Field>
      </div>
      <label className="check">
        <input type="checkbox" checked={f.taxable} onChange={(e) => setF({ ...f, taxable: e.target.checked })} /> Taxable
      </label>
      {add.error != null && <div className="notice bad">{errorMessage(add.error)}</div>}
    </Modal>
  );
}

function EditLine({ line, onClose }: { line: Line; onClose: () => void }) {
  const [f, setF] = useState<LineUpdate>({ description: line.description, qty: line.qty, unit_price: line.unit_price, taxable: line.taxable });
  const save = useAction<{ lineId: string; input: LineUpdate }>("job_update_line", { silentError: true });
  return (
    <Modal
      open
      title="Edit line"
      onClose={onClose}
      footer={
        <>
          <button className="btn" onClick={onClose}>
            Cancel
          </button>
          <button className="btn primary" onClick={() => save.mutate({ lineId: line.id, input: f }, { onSuccess: onClose })}>
            Save
          </button>
        </>
      }
    >
      <Field label="Description">
        <input type="text" value={f.description} onChange={(e) => setF({ ...f, description: e.target.value })} />
      </Field>
      <div className="grid two">
        <Field label="Quantity">
          <input type="text" inputMode="decimal" value={f.qty} onChange={(e) => setF({ ...f, qty: e.target.value })} />
        </Field>
        <Field label="Unit price" hint={line.kind === "part" ? "Changing it is noted in the price explanation." : undefined}>
          <input type="text" inputMode="decimal" value={f.unit_price} onChange={(e) => setF({ ...f, unit_price: e.target.value })} />
        </Field>
      </div>
      <label className="check">
        <input type="checkbox" checked={f.taxable} onChange={(e) => setF({ ...f, taxable: e.target.checked })} /> Taxable
      </label>
      {save.error != null && <div className="notice bad">{errorMessage(save.error)}</div>}
    </Modal>
  );
}

export function JobDetails({ d, onClose }: { d: JobDetail | null; onClose: () => void }) {
  const go = useApp((s) => s.go);
  const customers = useCmd<Customer[]>("customer_list", { q: null });
  const vehicles = useCmd<Vehicle[]>("vehicle_list", { saved: true });
  const profiles = useCmd<Profile[]>("profile_list");
  const j = d?.job;
  const [f, setF] = useState<JobInput>({
    customer_id: j?.customer_id ?? null,
    vehicle_id: j?.vehicle_id ?? null,
    title: j?.title ?? "",
    complaint: j?.complaint ?? "",
    notes: j?.notes ?? "",
    pricing_profile_id: j?.pricing_profile_id ?? null,
    mileage: j?.mileage ?? null,
    technician: j?.technician ?? "",
    discount_pct: j ? String(j.discount_pct) : null,
  });
  const save = useAction<{ id: string; input: JobInput }>("job_update", { silentError: true, success: "Saved" });
  const create = useAction<{ input: JobInput }, JobDetail>("job_create", { silentError: true });
  const set = <K extends keyof JobInput>(k: K, v: JobInput[K]) => setF({ ...f, [k]: v });
  const err = save.error ?? create.error;
  return (
    <Modal
      open
      title={j ? "Job details" : "New estimate"}
      onClose={onClose}
      footer={
        <>
          <button className="btn" onClick={onClose}>
            Cancel
          </button>
          <button
            className="btn primary"
            onClick={() => (j ? save.mutate({ id: j.id, input: f }, { onSuccess: onClose }) : create.mutate({ input: f }, { onSuccess: (nd) => (onClose(), go({ page: "job", id: nd.job.id })) }))}
          >
            {j ? "Save" : "Create estimate"}
          </button>
        </>
      }
    >
      <div className="grid two">
        <Field label="Customer">
          <select value={f.customer_id ?? ""} onChange={(e) => set("customer_id", e.target.value || null)}>
            <option value="">— None —</option>
            {customers.data?.map((c) => (
              <option key={c.id} value={c.id}>
                {c.name}
              </option>
            ))}
          </select>
        </Field>
        <Field label="Vehicle">
          <select value={f.vehicle_id ?? ""} onChange={(e) => set("vehicle_id", e.target.value || null)}>
            <option value="">— None —</option>
            {vehicles.data?.map((v) => (
              <option key={v.id} value={v.id}>
                {vehicleTitle(v)}
              </option>
            ))}
          </select>
        </Field>
      </div>
      <Field label="Title">
        <input type="text" value={f.title} onChange={(e) => set("title", e.target.value)} placeholder="e.g. Front brakes" />
      </Field>
      <Field label="Customer's request / complaint">
        <textarea value={f.complaint} onChange={(e) => set("complaint", e.target.value)} />
      </Field>
      <div className="grid two">
        <Field label="Pricing profile">
          <select value={f.pricing_profile_id ?? ""} onChange={(e) => set("pricing_profile_id", e.target.value || null)}>
            <option value="">Customer's or default</option>
            {profiles.data?.map((p) => (
              <option key={p.id} value={p.id}>
                {p.name}
              </option>
            ))}
          </select>
        </Field>
        <Field label="Discount (%)">
          <input type="text" inputMode="decimal" value={f.discount_pct ?? ""} onChange={(e) => set("discount_pct", e.target.value || null)} />
        </Field>
        <Field label="Mileage">
          <input type="number" min={0} value={f.mileage ?? ""} onChange={(e) => set("mileage", e.target.value ? Number(e.target.value) : null)} />
        </Field>
        <Field label="Technician">
          <input type="text" value={f.technician} onChange={(e) => set("technician", e.target.value)} />
        </Field>
      </div>
      <Field label="Internal notes">
        <textarea value={f.notes} onChange={(e) => set("notes", e.target.value)} />
      </Field>
      {err != null && <div className="notice bad">{errorMessage(err)}</div>}
    </Modal>
  );
}
