import { useState } from "react";
import { CalendarClock, Pencil, Plus, Trash2, Wrench } from "lucide-react";
import { errorMessage } from "../../api/ipc";
import { useAction, useCmd } from "../../api/queries";
import type { DueItem, Interval, IntervalInput, MaintenanceRecord as MRecord, RecordInput, Vehicle } from "../../api/types";
import { useApp } from "../../store";
import { ActionsMenu, Badge, Empty, Field, Loaded, Modal } from "../../ui";
import { date, money } from "../../lib/format";
import { vehicleTitle } from "../vehicles/Garage";
import { IntervalsFromManuals } from "../shared/FromManuals";
import { t, tx } from "../../i18n";

const STATE: Record<string, { label: string; tone: "bad" | "warn" | "ok" | "neutral" }> = {
  overdue: { label: t("Overdue"), tone: "bad" },
  due_soon: { label: t("Due soon"), tone: "warn" },
  ok: { label: t("OK"), tone: "ok" },
  never_done: { label: t("No record yet"), tone: "neutral" },
};

const invalidate = ["maintenance_intervals", "maintenance_records", "maintenance_due", "dashboard", "vehicle_detail"];

/** One vehicle's service intervals and history. */
export function Maintenance({ vehicle }: { vehicle: Vehicle }) {
  const intervals = useCmd<Interval[]>("maintenance_intervals", { vehicleId: vehicle.id });
  const records = useCmd<MRecord[]>("maintenance_records", { vehicleId: vehicle.id });
  const others = useCmd<Vehicle[]>("vehicle_list", { saved: true });
  const [editInterval, setEditInterval] = useState<Interval | "new" | null>(null);
  const [editRecord, setEditRecord] = useState<{ record?: MRecord; interval?: Interval } | null>(null);
  const delI = useAction<{ id: string }>("maintenance_interval_delete", { invalidate, success: t("Interval removed") });
  const delR = useAction<{ id: string }>("maintenance_record_delete", { invalidate, success: t("Record removed") });
  const copy = useAction<{ fromVehicle: string; toVehicle: string }, number>("maintenance_copy", { invalidate, success: (n) => `Copied ${n} intervals` });
  return (
    <div className="stack">
      <section className="card">
        <div className="spread">
          <h2>{t("Service schedule")}</h2>
          <div className="row">
            {(intervals.data?.length ?? 0) === 0 && (others.data?.length ?? 0) > 1 && (
              <select aria-label={t("Copy schedule from")} defaultValue="" onChange={(e) => e.target.value && copy.mutate({ fromVehicle: e.target.value, toVehicle: vehicle.id })}>
                <option value="">{t("Copy from another vehicle…")}</option>
                {others.data
                  ?.filter((v) => v.id !== vehicle.id)
                  .map((v) => (
                    <option key={v.id} value={v.id}>
                      {vehicleTitle(v)}
                    </option>
                  ))}
              </select>
            )}
            <IntervalsFromManuals vehicleId={vehicle.id} />
            <button className="btn" onClick={() => setEditInterval("new")}>
              <Plus size={16} aria-hidden />{" "}{t("Add interval")}</button>
          </div>
        </div>
        <p className="small muted">{t("Enter the intervals from this vehicle's owner's manual or maintenance guide (and note the page). Auto Manager doesn't guess schedules; they vary by vehicle and driving conditions.")}</p>
        <Loaded q={intervals}>
          {(list) =>
            list.length === 0 ? (
              <Empty icon={<CalendarClock size={20} />} title={t("No intervals yet")}>{t("For example: Engine oil and filter, every 5,000 miles or 6 months.")}</Empty>
            ) : (
              <table>
                <thead>
                  <tr>
                    <th>{t("Service")}</th>
                    <th>{t("Every")}</th>
                    <th>{t("Last done")}</th>
                    <th>{t("Next due")}</th>
                    <th>{t("Status")}</th>
                    <th />
                  </tr>
                </thead>
                <tbody>
                  {list.map((i) => (
                    <tr key={i.id}>
                      <td>
                        {i.service}
                        {i.source_ref && <div className="small muted">{i.source_ref}</div>}
                      </td>
                      <td>{[i.every_miles != null && `${i.every_miles.toLocaleString()} mi`, i.every_months != null && `${i.every_months} months`].filter(Boolean).join(" or ")}</td>
                      <td>{i.last_date ? `${date(i.last_date)}${i.last_mileage != null ? ` · ${i.last_mileage.toLocaleString()} mi` : ""}` : "—"}</td>
                      <td>{[i.due_date && date(i.due_date), i.due_mileage != null && `${i.due_mileage.toLocaleString()} mi`].filter(Boolean).join(" or ") || "—"}</td>
                      <td>
                        <Badge tone={STATE[i.state].tone}>{STATE[i.state].label}</Badge>
                      </td>
                      <td>
                        <div className="row" style={{ flexWrap: "nowrap" }}>
                          <button className="btn" onClick={() => setEditRecord({ interval: i })}>{t("Mark done")}</button>
                          <ActionsMenu
                            items={[
                              { label: t("Edit"), icon: <Pencil size={14} />, run: () => setEditInterval(i) },
                              { label: t("Remove"), icon: <Trash2 size={14} />, danger: true, run: () => delI.mutate({ id: i.id }) },
                            ]}
                          />
                        </div>
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
          <h2>{t("Service history")}</h2>
          <button className="btn" onClick={() => setEditRecord({})}>
            <Plus size={16} aria-hidden />{" "}{t("Add record")}</button>
        </div>
        <Loaded q={records}>
          {(list) =>
            list.length === 0 ? (
              <p className="muted">{t("No service recorded yet. Completed service jobs are added here automatically.")}</p>
            ) : (
              <table>
                <thead>
                  <tr>
                    <th>{t("Date")}</th>
                    <th>{t("Mileage")}</th>
                    <th>{t("Service")}</th>
                    <th>{t("By")}</th>
                    <th className="num">{t("Cost")}</th>
                    <th />
                  </tr>
                </thead>
                <tbody>
                  {list.map((r) => (
                    <tr key={r.id}>
                      <td>{date(r.date)}</td>
                      <td>{r.mileage?.toLocaleString() ?? "—"}</td>
                      <td>
                        {r.service}
                        {r.parts_used && <div className="small muted">{tx("Parts: {0}", r.parts_used)}</div>}
                        {r.notes && <div className="small muted">{r.notes}</div>}
                      </td>
                      <td>{r.provider || (r.job_id ? t("Service job") : "—")}</td>
                      <td className="num">{r.cost ? money(r.cost) : "—"}</td>
                      <td>
                        <ActionsMenu
                          items={[
                            { label: t("Edit"), icon: <Pencil size={14} />, run: () => setEditRecord({ record: r }) },
                            { label: t("Remove"), icon: <Trash2 size={14} />, danger: true, run: () => delR.mutate({ id: r.id }) },
                          ]}
                        />
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            )
          }
        </Loaded>
      </section>
      {editInterval && <IntervalDialog vehicleId={vehicle.id} existing={editInterval === "new" ? null : editInterval} onClose={() => setEditInterval(null)} />}
      {editRecord && <RecordDialog vehicle={vehicle} existing={editRecord.record} interval={editRecord.interval} onClose={() => setEditRecord(null)} />}
    </div>
  );
}

function IntervalDialog({ vehicleId, existing, onClose }: { vehicleId: string; existing: Interval | null; onClose: () => void }) {
  const [f, setF] = useState<IntervalInput>({ service: existing?.service ?? "", every_miles: existing?.every_miles ?? null, every_months: existing?.every_months ?? null, source_ref: existing?.source_ref ?? "", notes: existing?.notes ?? "" });
  const save = useAction<{ vehicleId: string; id: string | null; input: IntervalInput }>("maintenance_interval_save", { invalidate, success: t("Saved"), silentError: true });
  return (
    <Modal
      open
      title={existing ? t("Edit interval") : t("Add interval")}
      onClose={onClose}
      footer={
        <>
          <button className="btn" onClick={onClose}>{t("Cancel")}</button>
          <button className="btn primary" onClick={() => save.mutate({ vehicleId, id: existing?.id ?? null, input: f }, { onSuccess: onClose })}>{t("Save")}</button>
        </>
      }
    >
      <Field label={t("Service")}>
        <input type="text" value={f.service} onChange={(e) => setF({ ...f, service: e.target.value })} placeholder={t("e.g. Engine oil and filter")} autoFocus />
      </Field>
      <div className="grid two">
        <Field label={t("Every (miles)")}>
          <input type="number" min={1} value={f.every_miles ?? ""} onChange={(e) => setF({ ...f, every_miles: e.target.value ? Number(e.target.value) : null })} />
        </Field>
        <Field label={t("Or every (months)")}>
          <input type="number" min={1} value={f.every_months ?? ""} onChange={(e) => setF({ ...f, every_months: e.target.value ? Number(e.target.value) : null })} />
        </Field>
      </div>
      <Field label={t("From (recommended)")} hint={t("e.g. Owner's manual p. 300, normal driving schedule")}>
        <input type="text" value={f.source_ref} onChange={(e) => setF({ ...f, source_ref: e.target.value })} />
      </Field>
      <Field label={t("Notes")}>
        <input type="text" value={f.notes} onChange={(e) => setF({ ...f, notes: e.target.value })} />
      </Field>
      {save.error != null && <div className="notice bad">{errorMessage(save.error)}</div>}
    </Modal>
  );
}

function RecordDialog({ vehicle, existing, interval, onClose }: { vehicle: Vehicle; existing?: MRecord; interval?: Interval; onClose: () => void }) {
  const [f, setF] = useState<RecordInput>({
    interval_id: existing?.interval_id ?? interval?.id ?? null,
    date: existing?.date ?? new Date().toISOString().slice(0, 10),
    mileage: existing?.mileage ?? vehicle.mileage ?? null,
    service: existing?.service ?? interval?.service ?? "",
    parts_used: existing?.parts_used ?? "",
    labor: existing?.labor ?? "",
    cost: existing?.cost ?? null,
    provider: existing?.provider ?? "",
    notes: existing?.notes ?? "",
  });
  const save = useAction<{ vehicleId: string; id: string | null; input: RecordInput }>("maintenance_record_save", { invalidate, success: t("Saved"), silentError: true });
  const set = <K extends keyof RecordInput>(k: K, v: RecordInput[K]) => setF({ ...f, [k]: v });
  return (
    <Modal
      open
      title={existing ? t("Edit service record") : t("Record service")}
      onClose={onClose}
      footer={
        <>
          <button className="btn" onClick={onClose}>{t("Cancel")}</button>
          <button className="btn primary" onClick={() => save.mutate({ vehicleId: vehicle.id, id: existing?.id ?? null, input: f }, { onSuccess: onClose })}>{t("Save")}</button>
        </>
      }
    >
      <div className="grid two">
        <Field label={t("Date")}>
          <input type="date" value={f.date} onChange={(e) => set("date", e.target.value)} />
        </Field>
        <Field label={t("Mileage ({0})", vehicle.mileage_unit)}>
          <input type="number" min={0} value={f.mileage ?? ""} onChange={(e) => set("mileage", e.target.value ? Number(e.target.value) : null)} />
        </Field>
      </div>
      <Field label={t("Service performed")}>
        <input type="text" value={f.service} onChange={(e) => set("service", e.target.value)} />
      </Field>
      <Field label={t("Parts used")}>
        <input type="text" value={f.parts_used} onChange={(e) => set("parts_used", e.target.value)} />
      </Field>
      <div className="grid two">
        <Field label={t("Labor")}>
          <input type="text" value={f.labor} onChange={(e) => set("labor", e.target.value)} placeholder="e.g. 0.5 h" />
        </Field>
        <Field label={t("Cost")}>
          <input type="text" inputMode="decimal" value={f.cost ?? ""} onChange={(e) => set("cost", e.target.value || null)} />
        </Field>
      </div>
      <Field label={t("Done by")}>
        <input type="text" value={f.provider} onChange={(e) => set("provider", e.target.value)} placeholder={t("Shop, technician or yourself")} />
      </Field>
      <Field label={t("Notes")}>
        <textarea value={f.notes} onChange={(e) => set("notes", e.target.value)} />
      </Field>
      {save.error != null && <div className="notice bad">{errorMessage(save.error)}</div>}
    </Modal>
  );
}

/** Everything due across the garage. */
export function MaintenanceOverview() {
  const go = useApp((s) => s.go);
  const due = useCmd<DueItem[]>("maintenance_due");
  return (
    <Loaded q={due}>
      {(list) =>
        list.length === 0 ? (
          <Empty icon={<Wrench size={20} />} title={t("Nothing is due")}>{t("Services that are overdue, or due within 30 days or 1,000 miles, appear here. Set up intervals on each vehicle's Maintenance tab.")}</Empty>
        ) : (
          <div className="table-wrap">
            <table>
              <thead>
                <tr>
                  <th>{t("Vehicle")}</th>
                  <th>{t("Service")}</th>
                  <th>{t("Due")}</th>
                  <th />
                </tr>
              </thead>
              <tbody>
                {list.map((d, i) => (
                  <tr key={i}>
                    <td>
                      <button className="btn link" style={{ padding: 0 }} onClick={() => go({ page: "vehicle", id: d.vehicle_id, tab: "maintenance" })}>
                        {d.vehicle_title}
                      </button>
                    </td>
                    <td>{d.service}</td>
                    <td>{[d.due_date && date(d.due_date), d.due_mileage != null && `${d.due_mileage.toLocaleString()} mi`].filter(Boolean).join(" or ")}</td>
                    <td>
                      <Badge tone={d.overdue ? "bad" : "warn"}>{d.overdue ? t("Overdue") : t("Due soon")}</Badge>
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
