import { Attachments } from "../shared/Attachments";
import { useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { BadgeCheck, Boxes, ListPlus, Pencil, Plus, ShieldQuestion, Trash2 } from "lucide-react";
import { call, errorMessage } from "../../api/ipc";
import { useAction, useCmd } from "../../api/queries";
import type { FitResult, Fitment, FitmentInput, NameList, PartDetail, PartInput, Qualifier, SpecField, Vehicle } from "../../api/types";
import { useApp } from "../../store";
import { ActionsMenu, Badge, Empty, ErrorNote, Field, FitBadge, Loaded, Modal, StockBadge, useConfirm } from "../../ui";
import { date, humanKey, money } from "../../lib/format";
import { ID_KIND_LABEL, PartFields, partToInput } from "./PartForm";
import { AddToList } from "../shared/actions";
import { NewItem } from "../inventory/ItemForm";
import { vehicleTitle } from "../vehicles/Garage";
import { Interchange } from "./Interchange";
import { t, tx } from "../../i18n";

export default function PartPage() {
  const { route, go } = useApp();
  const id = route.id!;
  const q = useCmd<PartDetail>("part_get", { id });
  const [editing, setEditing] = useState(false);
  const [fitEdit, setFitEdit] = useState<Fitment | "new" | null>(null);
  const [listing, setListing] = useState(false);
  const [stocking, setStocking] = useState(false);
  const del = useAction<{ id: string }>("part_delete", { success: t("Part deleted") });
  const verify = useAction<{ id: string; verified: boolean }>("fitment_verify", { success: (f) => ((f as Fitment).verified_by ? t("Marked as verified") : t("Verification removed")) });
  const delFit = useAction<{ id: string }>("fitment_delete", { success: t("Fitment record deleted") });
  const { ask, dialog } = useConfirm();
  return (
    <div className="page wide">
      {dialog}
      <Loaded q={q}>
        {(d) => {
          const p = d.part;
          return (
            <>
              <div className="page-head">
                <div className="min-w-0">
                  <button className="btn link mb-1" style={{ paddingLeft: 0 }} onClick={() => go({ page: "parts", arg: p.category_id ?? undefined })}>
                    ← {p.category_path || t("Parts Catalog")}
                  </button>
                  <h1>{p.name}</h1>
                  <p className="row">
                    {p.brand && <strong>{p.brand}</strong>}
                    {p.manufacturer && p.manufacturer !== p.brand && <span>{tx("made by {0}", p.manufacturer)}</span>}
                    <Badge>{p.source_name}</Badge>
                  </p>
                </div>
                <div className="row">
                  <button className="btn" onClick={() => setListing(true)}>
                    <ListPlus size={16} aria-hidden />{" "}{t("Save to list")}</button>
                  <button className="btn primary" onClick={() => setStocking(true)}>
                    <Boxes size={16} aria-hidden />{" "}{t("Stock this part")}</button>
                  <ActionsMenu
                    items={[
                      { label: t("Edit part"), icon: <Pencil size={14} />, run: () => setEditing(true) },
                      null,
                      {
                        label: t("Delete part"),
                        icon: <Trash2 size={14} />,
                        danger: true,
                        run: async () => {
                          if (await ask(t("Delete this part?"), t("Its numbers and fitment records are deleted too. Parts that are stocked in inventory can't be deleted."))) del.mutate({ id }, { onSuccess: () => go("parts") });
                        },
                      },
                    ]}
                  />
                </div>
              </div>
              <div className="grid two" style={{ alignItems: "start" }}>
                <div className="stack">
                  <section className="card">
                    <h2>{t("Part numbers")}</h2>
                    {p.identifiers.length === 0 ? (
                      <p className="muted">{t("No numbers recorded.")}</p>
                    ) : (
                      <dl className="kv">
                        {p.identifiers.map((i) => (
                          <div key={i.kind + i.value} style={{ display: "contents" }}>
                            <dt>{ID_KIND_LABEL[i.kind] ?? i.kind}</dt>
                            <dd className="mono">
                              {i.value} {i.brand && <span className="muted">({i.brand})</span>}
                            </dd>
                          </div>
                        ))}
                      </dl>
                    )}
                  </section>
                  {(p.description || p.specs.length > 0 || p.install_notes || p.warranty) && (
                    <section className="card">
                      <h2>{t("Details")}</h2>
                      {p.description && <p style={{ whiteSpace: "pre-wrap" }}>{p.description}</p>}
                      {p.specs.length > 0 && (
                        <dl className="kv mb-2">
                          {p.specs.map(([k, v]) => (
                            <div key={k} style={{ display: "contents" }}>
                              <dt>{k}</dt>
                              <dd>{v}</dd>
                            </div>
                          ))}
                        </dl>
                      )}
                      {p.install_notes && (
                        <>
                          <p className="label">{t("Installation")}</p>
                          <p style={{ whiteSpace: "pre-wrap" }}>{p.install_notes}</p>
                        </>
                      )}
                      {p.warranty && (
                        <p>
                          <span className="label">{t("Warranty")}</span> {p.warranty}
                        </p>
                      )}
                    </section>
                  )}
                  <section className="card">
                    <div className="spread">
                      <h2>{t("In inventory")}</h2>
                    </div>
                    {d.items.length === 0 ? (
                      <p className="muted">{t("Not stocked. Use “Stock this part” to add it to inventory.")}</p>
                    ) : (
                      <table>
                        <tbody>
                          {d.items.map((i) => (
                            <tr key={i.id} style={{ cursor: "pointer" }} onClick={() => go({ page: "item", id: i.id })}>
                              <td className="mono">{i.sku}</td>
                              <td>{i.condition}</td>
                              <td>
                                <StockBadge available={i.available} min={i.min_stock} />
                              </td>
                              <td className="small muted">{i.bins}</td>
                              <td className="num">{tx("{0} cost", money(i.avg_cost))}</td>
                            </tr>
                          ))}
                        </tbody>
                      </table>
                    )}
                  </section>
                  <Interchange partId={p.id} />
                  <Attachments ownerKind="part" ownerId={p.id} title={t("Photos and diagrams")} />
                </div>
                <div className="stack">
                  <FitCheck partId={p.id} />
                  <section className="card">
                    <div className="spread">
                      <h2>{t("Fitment records")}</h2>
                      <button className="btn" onClick={() => setFitEdit("new")}>
                        <Plus size={16} aria-hidden />{" "}{t("Add")}</button>
                    </div>
                    <p className="small muted">{t("Which vehicles this part fits (or doesn't), as stated by a source. Only verified records can produce a confirmed fit.")}</p>
                    {d.fitments.length === 0 ? (
                      <Empty icon={<ShieldQuestion size={20} />} title={t("No fitment records")}>{t("Without records, Auto Manager won't suggest this part for any vehicle.")}</Empty>
                    ) : (
                      <ul className="items" style={{ maxHeight: "none" }}>
                        {d.fitments.map((f) => (
                          <li key={f.id} style={{ alignItems: "flex-start" }}>
                            <div className="flex-1 min-w-0">
                              <div>
                                {f.kind === "excludes" && <Badge tone="bad">{t("Does not fit")}</Badge>} <strong style={{ fontWeight: 600 }}>
                                  {f.year_from === f.year_to ? f.year_from : `${f.year_from}–${f.year_to}`} {f.make} {f.model}
                                </strong>
                                {f.position && <span className="muted"> · {f.position}</span>}
                                {f.quantity != null && <span className="muted">{" "}{tx("· qty {0}", f.quantity)}</span>}
                              </div>
                              {f.qualifiers.length > 0 && <div className="small">{f.qualifiers.map((x) => `${humanKey(x.key)}: ${x.value}`).join(" · ")}</div>}
                              {f.notes && <div className="small muted">{f.notes}</div>}
                              <div className="small muted">
                                {f.source_name}
                                {f.source_ref && ` · ${f.source_ref}`}
                              </div>
                            </div>
                            <div className="row" style={{ flexWrap: "nowrap" }}>
                              {f.verified_by ? (
                                <Badge tone="ok" title={t("Verified {0} {1}", f.verified_by === "source" ? "by its source" : "by you", date(f.verified_at))}>
                                  <BadgeCheck size={12} aria-hidden />{" "}{t("Verified")}</Badge>
                              ) : (
                                <Badge tone="warn">{t("Unverified")}</Badge>
                              )}
                              <ActionsMenu
                                items={[
                                  f.verified_by ? { label: t("Remove verification"), run: () => verify.mutate({ id: f.id, verified: false }) } : { label: t("Mark as verified"), icon: <BadgeCheck size={14} />, run: () => verify.mutate({ id: f.id, verified: true }) },
                                  { label: t("Edit"), icon: <Pencil size={14} />, run: () => setFitEdit(f) },
                                  null,
                                  { label: t("Delete"), icon: <Trash2 size={14} />, danger: true, run: () => delFit.mutate({ id: f.id }) },
                                ]}
                              />
                            </div>
                          </li>
                        ))}
                      </ul>
                    )}
                  </section>
                </div>
              </div>
              {editing && <EditPart id={p.id} initial={partToInput(p)} onClose={() => setEditing(false)} />}
              {fitEdit && <FitmentForm partId={p.id} existing={fitEdit === "new" ? null : fitEdit} onClose={() => setFitEdit(null)} />}
              <AddToList part={listing ? { id: p.id, name: p.name } : null} onClose={() => setListing(false)} />
              {stocking && <NewItem partId={p.id} partName={p.name} onClose={() => setStocking(false)} />}
            </>
          );
        }}
      </Loaded>
    </div>
  );
}

function EditPart({ id, initial, onClose }: { id: string; initial: PartInput; onClose: () => void }) {
  const [p, setP] = useState(initial);
  const save = useAction<{ id: string; input: PartInput }>("part_update", { success: t("Saved"), silentError: true });
  return (
    <Modal
      open
      wide
      title={t("Edit part")}
      onClose={onClose}
      footer={
        <>
          <button className="btn" onClick={onClose}>{t("Cancel")}</button>
          <button className="btn primary" onClick={() => save.mutate({ id, input: p }, { onSuccess: onClose })}>{t("Save")}</button>
        </>
      }
    >
      <PartFields value={p} onChange={setP} error={save.error} />
      {save.error != null && !(save.error as { field?: string }).field && <ErrorNote error={save.error} />}
    </Modal>
  );
}

/** Check this part against one saved vehicle. */
function FitCheck({ partId }: { partId: string }) {
  const vehicles = useCmd<Vehicle[]>("vehicle_list", { saved: true });
  const [vid, setVid] = useState("");
  const fit = useQuery<FitResult>({ queryKey: ["part_fit", partId, vid], queryFn: () => call<FitResult>("part_fit", { partId, vehicleId: vid }), enabled: !!vid });
  if (!vehicles.data?.length) return null;
  return (
    <section className="card">
      <h2>{t("Does it fit?")}</h2>
      <select value={vid} onChange={(e) => setVid(e.target.value)} aria-label={t("Vehicle to check")} className="w-full">
        <option value="">{t("Choose a saved vehicle…")}</option>
        {vehicles.data.map((v) => (
          <option key={v.id} value={v.id}>
            {vehicleTitle(v)} {v.nickname && `(${v.nickname})`}
          </option>
        ))}
      </select>
      {fit.data && (
        <div className="stack mt-2" style={{ gap: 6 }}>
          <FitBadge status={fit.data.status} />
          {fit.data.reasons.map((r, i) => (
            <p key={i} className="small" style={{ margin: 0 }}>
              {r}
            </p>
          ))}
          {fit.data.requirements.length > 0 && <p className="small">{tx("Needs: {0}", fit.data.requirements.map((r) => `${r.label} = ${r.value}`).join("; "))}</p>}
        </div>
      )}
      {fit.isError && <ErrorNote error={fit.error} />}
    </section>
  );
}

const QUALIFIER_HINTS = ["engine_displacement_l", "engine_cylinders", "engine_code", "trim", "series", "body_class", "drive_type", "fuel_type", "transmission_style", "transmission_code", "brake_package", "production_from", "production_to"];

export function FitmentForm({ partId, existing, onClose }: { partId: string; existing: Fitment | null; onClose: () => void }) {
  const fields = useCmd<SpecField[]>("spec_fields");
  const makes = useQuery<NameList>({ queryKey: ["vpic_makes"], queryFn: () => call<NameList>("vpic_makes"), staleTime: Infinity, retry: false });
  const [f, setF] = useState<FitmentInput>(
    existing
      ? { kind: existing.kind, year_from: existing.year_from, year_to: existing.year_to, make: existing.make, model: existing.model, position: existing.position, quantity: existing.quantity, notes: existing.notes, qualifiers: existing.qualifiers, verified: !!existing.verified_by, source_ref: existing.source_ref }
      : { kind: "fits", year_from: new Date().getFullYear(), year_to: new Date().getFullYear(), make: "", model: "", position: "", quantity: null, notes: "", qualifiers: [], verified: false, source_ref: null },
  );
  const add = useAction<{ partId: string; input: FitmentInput }>("fitment_add", { success: t("Fitment record added"), silentError: true });
  const upd = useAction<{ id: string; input: FitmentInput }>("fitment_update", { success: t("Fitment record saved"), silentError: true });
  const err = add.error ?? upd.error;
  const set = <K extends keyof FitmentInput>(k: K, v: FitmentInput[K]) => setF({ ...f, [k]: v });
  const label = (k: string) => fields.data?.find((x) => x.key === k)?.label ?? humanKey(k);
  const setQ = (i: number, patch: Partial<Qualifier>) => set("qualifiers", f.qualifiers.map((q, j) => (j === i ? { ...q, ...patch } : q)));
  return (
    <Modal
      open
      wide
      title={existing ? t("Edit fitment record") : t("Add fitment record")}
      onClose={onClose}
      footer={
        <>
          <button className="btn" onClick={onClose}>{t("Cancel")}</button>
          <button className="btn primary" onClick={() => (existing ? upd.mutate({ id: existing.id, input: f }, { onSuccess: onClose }) : add.mutate({ partId, input: f }, { onSuccess: onClose }))}>{t("Save")}</button>
        </>
      }
    >
      <div className="segmented" role="radiogroup" aria-label={t("Record type")}>
        <label>
          <input type="radio" checked={f.kind === "fits"} onChange={() => set("kind", "fits")} />{" "}{t("Fits")}</label>
        <label>
          <input type="radio" checked={f.kind === "excludes"} onChange={() => set("kind", "excludes")} />{" "}{t("Does not fit")}</label>
      </div>
      <div className="grid" style={{ gridTemplateColumns: "110px 110px 1fr 1fr" }}>
        <Field label={t("From year")}>
          <input type="number" value={f.year_from} onChange={(e) => set("year_from", Number(e.target.value))} />
        </Field>
        <Field label={t("To year")}>
          <input type="number" value={f.year_to} onChange={(e) => set("year_to", Number(e.target.value))} />
        </Field>
        <Field label={t("Make")}>
          <input type="text" list="fit-makes" value={f.make} onChange={(e) => set("make", e.target.value)} />
        </Field>
        <Field label={t("Model")}>
          <input type="text" value={f.model} onChange={(e) => set("model", e.target.value)} />
        </Field>
      </div>
      <datalist id="fit-makes">{makes.data?.names.map((m) => <option key={m} value={m} />)}</datalist>
      <fieldset className="card" style={{ padding: 12 }}>
        <legend className="label">{t("Conditions (all must match)")}</legend>
        <p className="small muted">{t("e.g. engine 1.8 L, drivetrain FWD, brake package Sport. Separate alternatives with “|” (S|SV). A vehicle without the detail gets a conditional fit until it's added.")}</p>
        {f.qualifiers.map((q, i) => (
          <div key={i} className="row mb-1">
            <input type="text" list="qual-keys" value={q.key} onChange={(e) => setQ(i, { key: e.target.value.trim().toLowerCase().replace(/[^a-z0-9_.]+/g, "_") })} aria-label={t("Condition")} style={{ width: 240 }} />
            <span className="small muted" style={{ width: 160 }}>
              {label(q.key)}
            </span>
            <input type="text" value={q.value} onChange={(e) => setQ(i, { value: e.target.value })} aria-label={t("Value")} style={{ flex: 1 }} />
            <button type="button" className="icon-btn" aria-label={t("Remove condition")} onClick={() => set("qualifiers", f.qualifiers.filter((_, j) => j !== i))}>
              <Trash2 size={14} aria-hidden />
            </button>
          </div>
        ))}
        <datalist id="qual-keys">{QUALIFIER_HINTS.map((k) => <option key={k} value={k} label={label(k)} />)}</datalist>
        <button type="button" className="btn link" style={{ paddingLeft: 0 }} onClick={() => set("qualifiers", [...f.qualifiers, { key: "engine_displacement_l", value: "" }])}>
          <Plus size={14} aria-hidden />{" "}{t("Add condition")}</button>
      </fieldset>
      <div className="grid" style={{ gridTemplateColumns: "1fr 120px" }}>
        <Field label={t("Position (optional)")}>
          <input type="text" value={f.position} onChange={(e) => set("position", e.target.value)} placeholder={t("Front, Rear left…")} />
        </Field>
        <Field label={t("Qty per vehicle")}>
          <input type="number" min={1} value={f.quantity ?? ""} onChange={(e) => set("quantity", e.target.value ? Number(e.target.value) : null)} />
        </Field>
      </div>
      <Field label={t("Notes")}>
        <input type="text" value={f.notes} onChange={(e) => set("notes", e.target.value)} />
      </Field>
      <Field label={t("Source of this information")} hint={t("e.g. Manufacturer catalog 2026, page 14; supplier website; checked on the vehicle.")}>
        <input type="text" value={f.source_ref ?? ""} onChange={(e) => set("source_ref", e.target.value || null)} />
      </Field>
      <label className="check">
        <input type="checkbox" checked={f.verified} onChange={(e) => set("verified", e.target.checked)} />{" "}{t("I've verified this against an authoritative source (allows a “Confirmed fit”)")}</label>
      {err != null && <div className="notice bad">{errorMessage(err)}</div>}
    </Modal>
  );
}
