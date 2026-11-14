import { useState } from "react";
import { BookOpenText } from "lucide-react";
import { useAction, useCmd } from "../../api/queries";
import type { IntervalFound, IntervalInput, SpecFound, SpecInput } from "../../api/types";
import { useApp } from "../../store";
import { Empty, Loaded, Modal } from "../../ui";
import { t, tx } from "../../i18n";

function Where({ title, page, id }: { title: string; page: number; id: string }) {
  const go = useApp((s) => s.go);
  return (
    <button className="btn link small" style={{ padding: 0 }} onClick={() => go({ page: "manual", id, arg: String(page) })}>
      {tx("{0}, page {1}", title, page)}
    </button>
  );
}

/** Specification values found in this vehicle's documents; each is saved only when chosen. */
export function SpecsFromManuals({ vehicleId }: { vehicleId: string }) {
  const [open, setOpen] = useState(false);
  const found = useCmd<SpecFound[]>("manual_spec_suggestions", { vehicleId }, open);
  const save = useAction<{ id: string; spec: SpecInput }>("vehicle_set_spec", { success: t("Saved") });
  const [used, setUsed] = useState<string[]>([]);
  return (
    <>
      <button className="btn" onClick={() => setOpen(true)}>
        <BookOpenText size={16} aria-hidden /> {t("Find values in your manuals")}
      </button>
      <Modal open={open} onClose={() => setOpen(false)} title={t("Values found in your manuals")} wide>
        <p className="small muted" style={{ marginTop: 0 }}>
          {t("Read from the searchable text of the documents linked to this vehicle. Check each against the page before using it; it's saved as “From a manual” with the page cited.")}
        </p>
        <Loaded q={found}>
          {(list) =>
            list.length === 0 ? (
              <Empty icon={<BookOpenText size={20} />} title={t("Nothing found in this vehicle's documents")}>
                {t("Import the owner's or service manual on the Manuals tab (and let it be made searchable), then look again.")}
              </Empty>
            ) : (
              <div className="stack" style={{ gap: 8 }}>
                {list.map((f, i) => {
                  const k = `${f.key}|${f.value}|${f.unit}`;
                  return (
                    <div key={i} className="card spread" style={{ padding: 10, alignItems: "flex-start" }}>
                      <div className="stack" style={{ gap: 2 }}>
                        <span>
                          <strong>{f.label}</strong>: {f.value}
                          {f.unit ? ` ${f.unit.replace("_", "·")}` : ""}
                        </span>
                        <span className="small muted">“{f.context}”</span>
                        <Where title={f.manual_title} page={f.page} id={f.manual_id} />
                      </div>
                      <button
                        className="btn"
                        disabled={used.includes(k) || save.isPending}
                        onClick={() =>
                          save.mutate(
                            { id: vehicleId, spec: { key: f.key, value: f.value, unit: f.unit, source_ref: t("{0}, page {1}", f.manual_title, f.page) } },
                            { onSuccess: () => setUsed([...used, k]) },
                          )
                        }
                      >
                        {used.includes(k) ? t("Saved") : t("Use this value")}
                      </button>
                    </div>
                  );
                })}
              </div>
            )
          }
        </Loaded>
      </Modal>
    </>
  );
}

/** Service intervals found in this vehicle's documents; each is added only when chosen. */
export function IntervalsFromManuals({ vehicleId }: { vehicleId: string }) {
  const [open, setOpen] = useState(false);
  const found = useCmd<IntervalFound[]>("manual_interval_suggestions", { vehicleId }, open);
  const save = useAction<{ vehicleId: string; id: null; input: IntervalInput }>("maintenance_interval_save", { invalidate: ["maintenance_intervals", "maintenance_due"], success: t("Interval added") });
  const [used, setUsed] = useState<number[]>([]);
  return (
    <>
      <button className="btn" onClick={() => setOpen(true)}>
        <BookOpenText size={16} aria-hidden /> {t("Find intervals in your manuals")}
      </button>
      <Modal open={open} onClose={() => setOpen(false)} title={t("Service intervals found in your manuals")} wide>
        <p className="small muted" style={{ marginTop: 0 }}>{t("Schedules differ for normal and severe driving; add the ones that apply to how this vehicle is used.")}</p>
        <Loaded q={found}>
          {(list) =>
            list.length === 0 ? (
              <Empty icon={<BookOpenText size={20} />} title={t("No intervals found in this vehicle's documents")}>
                {t("Import the owner's manual or maintenance guide on the Manuals tab, then look again.")}
              </Empty>
            ) : (
              <div className="stack" style={{ gap: 8 }}>
                {list.map((f, i) => (
                  <div key={i} className="card spread" style={{ padding: 10, alignItems: "flex-start" }}>
                    <div className="stack" style={{ gap: 2 }}>
                      <strong>{f.service}</strong>
                      <span className="small">
                        {[f.every_miles != null ? t("every {0} miles", f.every_miles.toLocaleString()) : null, f.every_months != null ? t("or {0} months", f.every_months) : null].filter(Boolean).join(" ")}
                      </span>
                      <span className="small muted">“{f.context}”</span>
                      <Where title={f.manual_title} page={f.page} id={f.manual_id} />
                    </div>
                    <button
                      className="btn"
                      disabled={used.includes(i) || save.isPending}
                      onClick={() =>
                        save.mutate(
                          { vehicleId, id: null, input: { service: f.service, every_miles: f.every_miles, every_months: f.every_months, source_ref: t("{0}, page {1}", f.manual_title, f.page), notes: "" } },
                          { onSuccess: () => setUsed([...used, i]) },
                        )
                      }
                    >
                      {used.includes(i) ? t("Added") : t("Add interval")}
                    </button>
                  </div>
                ))}
              </div>
            )
          }
        </Loaded>
      </Modal>
    </>
  );
}
