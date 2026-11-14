import { Boxes, BookOpen, Wrench, Info } from "lucide-react";
import { useCmd } from "../../api/queries";
import type { Job, Manual, SpecField, VehicleDetail, VehicleMatches } from "../../api/types";
import { useApp } from "../../store";
import { useSettings } from "../../settings";
import { showValue, type UnitSystem } from "../../lib/units";
import { SourceChip } from "./Specs";

export default function Overview({ d }: { d: VehicleDetail }) {
  const { go, setTab } = useApp();
  const v = d.vehicle;
  const fields = useCmd<SpecField[]>("spec_fields");
  const { settings } = useSettings();
  const system = (settings?.units ?? "imperial") as UnitSystem;
  const stock = useCmd<VehicleMatches>("vehicle_matches", { id: v.id, categoryId: null, inStockOnly: true });
  const manuals = useCmd<Manual[]>("manual_list", { vehicleId: v.id });
  const jobs = useCmd<Job[]>("job_list", { query: { status: null, vehicle_id: v.id, customer_id: null } });
  const eff = new Map(d.specs.filter((s) => s.effective).map((s) => [s.key, s]));
  const essential = (fields.data ?? []).filter((f) => f.essential);
  const missingFitment = (fields.data ?? []).filter((f) => f.fitment && !eff.has(f.key));
  const fitting = stock.data ? stock.data.matches.filter((m) => m.fit.status === "confirmed" || m.fit.status === "conditional" || m.fit.status === "possible").length : null;
  return (
    <div className="grid two">
      <section className="card">
        <h2>Key specifications</h2>
        <div className="specs">
          {essential.map((f) => {
            const s = eff.get(f.key);
            return (
              <div key={f.key} style={{ display: "contents" }}>
                <div className="k">{f.label}</div>
                <div className="v">{s ? showValue(s.value, s.unit ?? f.unit, system) : <span className="missing">Not available</span>}</div>
                <div className="s">{s && <SourceChip s={s} />}</div>
              </div>
            );
          })}
        </div>
        <button className="btn link mt-2" style={{ paddingLeft: 0 }} onClick={() => setTab("specs")}>
          All specifications →
        </button>
      </section>
      <div className="stack">
        <section className="card">
          <h2>At a glance</h2>
          <div className="starts">
            <button className="start" onClick={() => setTab("stock")}>
              <span className="icon-chip">
                <Boxes size={18} />
              </span>
              <span>
                <strong>{fitting ?? "…"} parts in stock</strong>
                <span>with fitment records for this vehicle</span>
              </span>
            </button>
            <button className="start" onClick={() => setTab("manuals")}>
              <span className="icon-chip">
                <BookOpen size={18} />
              </span>
              <span>
                <strong>{manuals.data?.length ?? "…"} documents</strong>
                <span>manuals and references</span>
              </span>
            </button>
            <button className="start" onClick={() => setTab("service")}>
              <span className="icon-chip">
                <Wrench size={18} />
              </span>
              <span>
                <strong>{jobs.data?.length ?? "…"} jobs</strong>
                <span>estimates and service history</span>
              </span>
            </button>
          </div>
        </section>
        {missingFitment.length > 0 && (
          <section className="notice info">
            <div className="flex gap-2">
              <Info size={16} aria-hidden className="flex-none mt-0.5" />
              <div>
                <strong>Details that help parts matching are missing:</strong> {missingFitment.map((f) => f.label.toLowerCase()).join(", ")}. Without them, some parts can only be shown as a conditional fit.{" "}
                <button className="btn link" style={{ padding: 0 }} onClick={() => setTab("specs")}>
                  Add them
                </button>
              </div>
            </div>
          </section>
        )}
        {v.decode_note && !v.decode_note.startsWith("0 -") && (
          <section className="notice warn">
            <strong>NHTSA's note on this VIN:</strong> {v.decode_note}
          </section>
        )}
        {(v.notes || v.mileage != null || v.plate || v.color) && (
          <section className="card">
            <h2>Your details</h2>
            <dl className="kv">
              {v.mileage != null && (
                <>
                  <dt>Mileage</dt>
                  <dd>
                    {v.mileage.toLocaleString()} {v.mileage_unit}
                  </dd>
                </>
              )}
              {v.plate && (
                <>
                  <dt>Plate</dt>
                  <dd>{v.plate}</dd>
                </>
              )}
              {v.color && (
                <>
                  <dt>Colour</dt>
                  <dd>{v.color}</dd>
                </>
              )}
              {v.notes && (
                <>
                  <dt>Notes</dt>
                  <dd style={{ whiteSpace: "pre-wrap" }}>{v.notes}</dd>
                </>
              )}
            </dl>
          </section>
        )}
        {v.customer_id && (
          <button className="btn" onClick={() => go({ page: "customers", id: v.customer_id! })}>
            Open owner record
          </button>
        )}
      </div>
    </div>
  );
}
