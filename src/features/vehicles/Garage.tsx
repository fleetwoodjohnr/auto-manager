import { useMemo, useState } from "react";
import { useQueries } from "@tanstack/react-query";
import { Car, Columns3, Plus, ScanSearch } from "lucide-react";
import { call } from "../../api/ipc";
import { useCmd } from "../../api/queries";
import type { SpecField, Vehicle, VehicleDetail } from "../../api/types";
import { useApp } from "../../store";
import { useSettings } from "../../settings";
import { Badge, Empty, Loaded, Modal, PageHead, Tabs } from "../../ui";
import { ago } from "../../lib/format";
import { showValue, type UnitSystem } from "../../lib/units";

export const vehicleTitle = (v: Pick<Vehicle, "year" | "make" | "model" | "trim">) => [v.year, v.make, v.model, v.trim].filter(Boolean).join(" ") || "Unidentified vehicle";

export default function Garage() {
  const { route, go, setTab } = useApp();
  const tab = route.tab ?? "saved";
  const q = useCmd<Vehicle[]>("vehicle_list", { saved: tab === "saved" });
  const [filter, setFilter] = useState("");
  const [picked, setPicked] = useState<string[]>([]);
  const [comparing, setComparing] = useState(false);
  const toggle = (id: string) => setPicked((p) => (p.includes(id) ? p.filter((x) => x !== id) : [...p, id].slice(-4)));
  return (
    <div className="page wide">
      <PageHead title="My Vehicles" sub="Your garage: saved vehicles, their specifications, parts, manuals and service history.">
        {picked.length >= 2 && (
          <button className="btn" onClick={() => setComparing(true)}>
            <Columns3 size={16} aria-hidden /> Compare {picked.length}
          </button>
        )}
        <button className="btn" onClick={() => go({ page: "lookup", tab: "manual" })}>
          <Plus size={16} aria-hidden /> Add by hand
        </button>
        <button className="btn primary" onClick={() => go("lookup")}>
          <ScanSearch size={16} aria-hidden /> Look up a VIN
        </button>
      </PageHead>
      <Tabs
        label="Vehicles"
        value={tab}
        onChange={(t) => {
          setTab(t);
          setPicked([]);
        }}
        tabs={[
          { id: "saved", label: "Saved" },
          { id: "recent", label: "Recent lookups" },
        ]}
      />
      <Loaded q={q}>
        {(list) => {
          const f = filter.trim().toLowerCase();
          const shown = f ? list.filter((v) => [vehicleTitle(v), v.vin, v.nickname, v.plate, v.color].join(" ").toLowerCase().includes(f)) : list;
          if (list.length === 0) {
            return (
              <Empty icon={<Car size={20} />} title={tab === "saved" ? "No saved vehicles yet" : "No recent lookups"} action={<button className="btn primary" onClick={() => go("lookup")}>Look up a vehicle</button>}>
                {tab === "saved" ? "Decode a VIN and choose Save, or add an older vehicle by hand." : "Vehicles you decode appear here until you save them."}
              </Empty>
            );
          }
          return (
            <>
              <div className="row mb-3">
                <input type="search" placeholder="Filter by name, VIN, plate…" value={filter} onChange={(e) => setFilter(e.target.value)} aria-label="Filter vehicles" style={{ width: 320 }} />
                <span className="small muted">{shown.length} vehicles · tick up to 4 to compare</span>
              </div>
              <div className="garage">
                {shown.map((v) => (
                  <div key={v.id} className="relative">
                    <button className="vcard w-full" onClick={() => go({ page: "vehicle", id: v.id })}>
                      <span className="make">
                        {v.year} {v.make}
                      </span>
                      <span className="model">
                        {v.model} {v.trim}
                      </span>
                      {v.nickname && <span>“{v.nickname}”</span>}
                      <span className="small muted">{[v.engine, v.transmission].filter(Boolean).join(" · ") || "Engine unknown"}</span>
                      <span className="row small muted">
                        {v.vin ? <span className="mono">{v.vin}</span> : <Badge>No VIN</Badge>}
                        {v.plate && <Badge>{v.plate}</Badge>}
                        {v.mileage != null && (
                          <span>
                            {v.mileage.toLocaleString()} {v.mileage_unit}
                          </span>
                        )}
                      </span>
                      {tab === "recent" && <span className="small muted">Looked up {ago(v.last_viewed_at)}</span>}
                    </button>
                    <label className="absolute" style={{ top: 10, right: 10 }} title="Select to compare">
                      <input type="checkbox" checked={picked.includes(v.id)} onChange={() => toggle(v.id)} aria-label={`Compare ${vehicleTitle(v)}`} />
                    </label>
                  </div>
                ))}
              </div>
            </>
          );
        }}
      </Loaded>
      {comparing && <Compare ids={picked} onClose={() => setComparing(false)} />}
    </div>
  );
}

/** Specifications of 2–4 vehicles side by side; rows that differ are highlighted. */
function Compare({ ids, onClose }: { ids: string[]; onClose: () => void }) {
  const { settings } = useSettings();
  const fields = useCmd<SpecField[]>("spec_fields");
  const details = useQueries({ queries: ids.map((id) => ({ queryKey: ["vehicle_detail", { id }], queryFn: () => call<VehicleDetail>("vehicle_detail", { id }) })) });
  const system = (settings?.units ?? "imperial") as UnitSystem;
  const ready = details.every((d) => d.data);
  const rows = useMemo(() => {
    if (!ready || !fields.data) return [];
    return fields.data
      .map((f) => {
        const vals = details.map((d) => {
          const s = d.data!.specs.find((x) => x.key === f.key && x.effective);
          return s ? showValue(s.value, s.unit ?? f.unit, system) : "—";
        });
        return { f, vals, differs: new Set(vals).size > 1 };
      })
      .filter((r) => r.vals.some((v) => v !== "—"));
  }, [ready, fields.data, details, system]);
  return (
    <Modal open title="Compare vehicles" onClose={onClose} wide>
      {!ready ? (
        <p>Loading…</p>
      ) : (
        <div className="table-wrap" style={{ maxHeight: "65vh" }}>
          <table>
            <thead>
              <tr>
                <th>Specification</th>
                {details.map((d) => (
                  <th key={d.data!.vehicle.id}>{vehicleTitle(d.data!.vehicle)}</th>
                ))}
              </tr>
            </thead>
            <tbody>
              {rows.map((r) => (
                <tr key={r.f.key} style={r.differs ? { background: "var(--accent-soft)" } : undefined}>
                  <td className="muted">{r.f.label}</td>
                  {r.vals.map((v, i) => (
                    <td key={i}>{v}</td>
                  ))}
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
      <p className="small muted">Highlighted rows differ. “—” means no source has given that value.</p>
    </Modal>
  );
}
