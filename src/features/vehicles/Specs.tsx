import { useMemo, useState } from "react";
import { BookOpen, CircleAlert, Pencil, Plus, Undo2 } from "lucide-react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { call, errorMessage } from "../../api/ipc";
import { invalidateData, useAction, useCmd } from "../../api/queries";
import type { CanadaOptions, EpaOptions, SpecField, SpecInput, SpecRow, VehicleDetail } from "../../api/types";
import { useSettings } from "../../settings";
import { Badge, ErrorNote, Field, Modal } from "../../ui";
import { date, humanKey } from "../../lib/format";
import { inputUnit, showValue, toStored, type UnitSystem } from "../../lib/units";
import { SpecsFromManuals } from "../shared/FromManuals";
import { t, tx } from "../../i18n";

export const CONFIDENCE: Record<string, { label: string; tone: "ok" | "info" | "neutral" | "warn" }> = {
  vin_decoded: { label: t("Decoded from VIN"), tone: "ok" },
  model_level: { label: t("Model-level"), tone: "warn" },
  user_entered: { label: t("Entered by you"), tone: "info" },
  manual_ref: { label: t("From a manual"), tone: "info" },
};

export function SourceChip({ s }: { s: SpecRow }) {
  const c = CONFIDENCE[s.confidence] ?? { label: s.confidence, tone: "neutral" as const };
  const title = [s.source_name, s.source_ref, s.confidence === "model_level" ? t("Applies to the model in general, not confirmed for this VIN") : null, `recorded ${date(s.fetched_at)}`].filter(Boolean).join(" · ");
  return (
    <Badge tone={c.tone} title={title}>
      {s.confidence === "manual_ref" && <BookOpen size={11} aria-hidden />}
      {c.label}
      {s.source_id !== "user" && ` · ${s.source_name}`}
    </Badge>
  );
}

/** Every specification, grouped, with where each value came from. Corrections are stored as
 * your own values beside the original, which stays on record and can be restored. */
export default function Specs({ d }: { d: VehicleDetail }) {
  const fields = useCmd<SpecField[]>("spec_fields");
  const { settings } = useSettings();
  const system = (settings?.units ?? "imperial") as UnitSystem;
  const [editing, setEditing] = useState<{ key: string; label: string; unit: string | null; current?: SpecRow } | null>(null);
  const clear = useAction<{ id: string; key: string }>("vehicle_clear_spec", { success: t("Restored the source's value") });
  const byKey = useMemo(() => {
    const m = new Map<string, SpecRow[]>();
    d.specs.forEach((s) => m.set(s.key, [...(m.get(s.key) ?? []), s]));
    return m;
  }, [d.specs]);
  if (!fields.data) return null;
  const groups = [...new Set(fields.data.map((f) => f.group))];
  const known = new Set(fields.data.map((f) => f.key));
  const extra = [...byKey.keys()].filter((k) => !known.has(k) && !k.startsWith("vpic."));
  const other = [...byKey.keys()].filter((k) => k.startsWith("vpic."));

  const row = (key: string, label: string, unit: string | null) => {
    const rows = byKey.get(key) ?? [];
    const eff = rows.find((r) => r.effective);
    const overridden = rows.filter((r) => !r.effective);
    const conflict = d.conflicts.includes(key);
    return (
      <div key={key} style={{ display: "contents" }}>
        <div className="k">{label}</div>
        <div className="v">
          {eff ? (
            <>
              {showValue(eff.value, eff.unit ?? unit, system)}
              {overridden.length > 0 && (
                <div className="small muted">
                  {overridden.map((o) => (
                    <span key={o.source_id}>
                      {o.source_name}: {showValue(o.value, o.unit ?? unit, system)}{" "}
                    </span>
                  ))}
                </div>
              )}
              {conflict && (
                <div className="small" style={{ color: "var(--warn)" }}>
                  <CircleAlert size={12} aria-hidden />{" "}{t("Sources disagree; check the vehicle or its documentation.")}</div>
              )}
            </>
          ) : (
            <span className="missing">{t("Not available from free sources")}</span>
          )}
        </div>
        <div className="s row" style={{ justifyContent: "flex-end", flexWrap: "nowrap" }}>
          {eff && <SourceChip s={eff} />}
          {eff?.source_id === "user" && overridden.length > 0 && (
            <button className="icon-btn" title={t("Use the source's value again")} aria-label={t("Restore {0} from source", label)} onClick={() => clear.mutate({ id: d.vehicle.id, key })}>
              <Undo2 size={14} aria-hidden />
            </button>
          )}
          <button className="icon-btn" title={eff ? t("Correct this value") : t("Add from your manual")} aria-label={`${eff ? "Edit" : "Add"} ${label}`} onClick={() => setEditing({ key, label, unit, current: eff })}>
            {eff ? <Pencil size={14} aria-hidden /> : <Plus size={14} aria-hidden />}
          </button>
        </div>
      </div>
    );
  };

  return (
    <div className="stack">
      <p className="small muted">{t("NHTSA notes that a missing value means it has no data, not that the feature is absent. Values marked “Entered by you” or “From a manual” were added here; the original source values stay on record.")}</p>
      {groups.map((g) => (
        <section key={g} className="card">
          <h2>{g}</h2>
          <div className="specs">{fields.data!.filter((f) => f.group === g).map((f) => row(f.key, f.label, f.unit))}</div>
        </section>
      ))}
      <section className="card">
        <div className="spread">
          <h2>{t("Your additional details")}</h2>
          <button className="btn" onClick={() => setEditing({ key: "", label: "", unit: null })}>
            <Plus size={16} aria-hidden />{" "}{t("Add a detail")}</button>
        </div>
        <p className="small muted">{t("Details such as brake package or production date that some fitment records need, recorded from the vehicle or its paperwork.")}</p>
        {extra.length > 0 ? <div className="specs">{extra.map((k) => row(k, humanKey(k), null))}</div> : <p className="muted">{t("None yet.")}</p>}
      </section>
      <div className="row">
        <SpecsFromManuals vehicleId={d.vehicle.id} />
      </div>
      <MoreSources vehicleId={d.vehicle.id} />
      {other.length > 0 && (
        <details className="card">
          <summary>
            <strong>{tx("Other data NHTSA returned ({0})", other.length)}</strong>
          </summary>
          <div className="specs mt-2">{other.map((k) => row(k, humanKey(k), null))}</div>
        </details>
      )}
      {editing && <EditSpec vehicleId={d.vehicle.id} target={editing} system={system} onClose={() => setEditing(null)} />}
    </div>
  );
}

function EditSpec({ vehicleId, target, system, onClose }: { vehicleId: string; target: { key: string; label: string; unit: string | null; current?: SpecRow }; system: UnitSystem; onClose: () => void }) {
  const [key, setKey] = useState(target.key);
  const unitShown = inputUnit(target.unit, system);
  const [value, setValue] = useState(target.current ? showValue(target.current.value, null, system) : "");
  const [ref, setRef] = useState(target.current?.source_id === "user" ? (target.current.source_ref ?? "") : "");
  const save = useAction<{ id: string; spec: SpecInput }>("vehicle_set_spec", { success: t("Saved"), silentError: true });
  const custom = !target.key;
  const cleanKey = key.trim().toLowerCase().replace(/[^a-z0-9]+/g, "_").replace(/^_|_$/g, "");
  return (
    <Modal
      open
      title={custom ? t("Add a detail") : target.current ? t("Correct {0}", target.label.toLowerCase()) : t("Add {0}", target.label.toLowerCase())}
      onClose={onClose}
      footer={
        <>
          <button className="btn" onClick={onClose}>{t("Cancel")}</button>
          <button
            className="btn primary"
            disabled={!value.trim() || !cleanKey}
            onClick={() => save.mutate({ id: vehicleId, spec: { key: cleanKey, value: toStored(value, target.unit, system), unit: target.unit, source_ref: ref || null } }, { onSuccess: onClose })}
          >{t("Save")}</button>
        </>
      }
    >
      {custom && (
        <Field label={t("Detail")} hint={cleanKey ? t("Saved as “{0}”, the name fitment records use.", cleanKey) : t("e.g. Brake package, Production date, Axle ratio")}>
          <input type="text" value={key} onChange={(e) => setKey(e.target.value)} autoFocus />
        </Field>
      )}
      <Field label={unitShown ? t("Value ({0})", unitShown) : t("Value")}>
        <input type="text" value={value} onChange={(e) => setValue(e.target.value)} autoFocus={!custom} />
      </Field>
      <Field label={t("Where it comes from (recommended)")} hint={t("e.g. Owner's manual p. 212, door-jamb sticker, window sticker. Saved with the value so it can be checked later.")}>
        <input type="text" value={ref} onChange={(e) => setRef(e.target.value)} />
      </Field>
      {target.current && target.current.source_id !== "user" && (
        <p className="small muted">{tx("{0} said “{1}”. That value is kept; yours is shown instead, and you can switch back at any time.", target.current.source_name, target.current.value)}</p>
      )}
      {save.error != null && <div className="notice bad">{errorMessage(save.error)}</div>}
    </Modal>
  );
}

/** Fill gaps from model-level free sources: the person picks the matching configuration. */
export function MoreSources({ vehicleId }: { vehicleId: string }) {
  const [which, setWhich] = useState<"canada" | "epa" | null>(null);
  return (
    <section className="card">
      <h2>{t("Add data from other free sources")}</h2>
      <p className="small muted">{t("These describe a model or configuration, not this exact VIN, so you pick the one that matches and the values are marked “Model-level”. Values from NHTSA's VIN decode still take priority.")}</p>
      <div className="row">
        <button className="btn" onClick={() => setWhich("canada")}>{t("Dimensions and weight (vPIC Canadian specifications)")}</button>
        <button className="btn" onClick={() => setWhich("epa")}>{t("Engine, transmission and fuel economy (EPA)")}</button>
      </div>
      {which && <SourcePicker vehicleId={vehicleId} which={which} onClose={() => setWhich(null)} />}
    </section>
  );
}

function SourcePicker({ vehicleId, which, onClose }: { vehicleId: string; which: "canada" | "epa"; onClose: () => void }) {
  const qc = useQueryClient();
  const q = useQuery<CanadaOptions | EpaOptions>({
    queryKey: [which, vehicleId],
    queryFn: () => call(which === "canada" ? "canada_options" : "epa_options", { vehicleId }),
    retry: false,
  });
  const [pick, setPick] = useState<string>("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<unknown>(null);
  const apply = async () => {
    setBusy(true);
    setError(null);
    try {
      await call(which === "canada" ? "canada_apply" : "epa_apply", which === "canada" ? { vehicleId, index: Number(pick) } : { vehicleId, optionId: pick });
      void invalidateData(qc);
      onClose();
    } catch (e) {
      setError(e);
    } finally {
      setBusy(false);
    }
  };
  const options: { id: string; label: string }[] = !q.data
    ? []
    : "rows" in q.data
      ? q.data.rows.map((r) => ({ id: String(r.index), label: `${r.model}: ${r.values.map((v) => `${v.key.replace("canada.", "")} ${v.value}`).join(", ")}` }))
      : q.data.options.map((o) => ({ id: o.id, label: o.text }));
  return (
    <Modal
      open
      wide
      title={which === "canada" ? t("Choose the matching body style") : t("Choose the matching configuration")}
      onClose={onClose}
      footer={
        <>
          <button className="btn" onClick={onClose}>{t("Cancel")}</button>
          <button className="btn primary" disabled={!pick || busy} onClick={apply}>{t("Add these values")}</button>
        </>
      }
    >
      {q.isPending && <p>{t("Looking up…")}</p>}
      {q.isError && <ErrorNote error={q.error} />}
      {q.data && options.length === 0 && <p className="muted">{t("This source has nothing for this model year, make and model.")}</p>}
      <div className="stack" role="radiogroup" aria-label={t("Options")}>
        {options.map((o) => (
          <label key={o.id} className="check" style={{ alignItems: "flex-start" }}>
            <input type="radio" name={t("src")} checked={pick === o.id} onChange={() => setPick(o.id)} /> <span>{o.label}</span>
          </label>
        ))}
      </div>
      {error != null && <ErrorNote error={error} />}
    </Modal>
  );
}
