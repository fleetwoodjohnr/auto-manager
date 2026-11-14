import { useState } from "react";
import { Calculator, Pencil, RefreshCw, Star, StarOff, Trash2 } from "lucide-react";
import { call, errorMessage } from "../../api/ipc";
import { useAction, useCmd, useRefresh } from "../../api/queries";
import type { Customer, DecodeOutcome, JobDetail, Vehicle, VehicleDetail, VehicleUpdate } from "../../api/types";
import { useApp, toast } from "../../store";
import { ActionsMenu, Badge, Field, Loaded, Modal, Spinner, Tabs, useConfirm } from "../../ui";
import { vehicleTitle } from "./Garage";
import Overview from "./Overview";
import Specs from "./Specs";
import Matches from "./Matches";
import VehicleManuals from "./VehicleManuals";
import VehicleService from "./VehicleService";
import { Maintenance } from "../service/Maintenance";
import Recalls from "./Recalls";

export default function VehiclePage() {
  const { route, setTab, go } = useApp();
  const id = route.id!;
  const q = useCmd<VehicleDetail>("vehicle_detail", { id });
  const tab = route.tab ?? "overview";
  const refresh = useRefresh();
  const save = useAction<{ id: string; saved: boolean }, Vehicle>("vehicle_set_saved", { success: (v) => (v.saved ? "Saved to My Vehicles" : "Removed from My Vehicles") });
  const del = useAction<{ id: string }>("vehicle_delete", { success: "Vehicle deleted" });
  const { ask, dialog } = useConfirm();
  const [editing, setEditing] = useState(false);
  const [refreshing, setRefreshing] = useState(false);

  const reDecode = async (vin: string) => {
    setRefreshing(true);
    try {
      const r = await call<DecodeOutcome>("decode_vin", { vin, modelYear: null, refresh: true });
      toast(r.freshness.stale ? "NHTSA couldn't be reached; the saved data is unchanged." : "Updated from NHTSA. Your own entries were kept.");
      refresh();
    } catch (e) {
      toast(errorMessage(e), { tone: "bad" });
    } finally {
      setRefreshing(false);
    }
  };
  const newEstimate = async (v: Vehicle) => {
    try {
      const d = await call<JobDetail>("job_create", { input: { vehicle_id: v.id, customer_id: v.customer_id, title: "" } });
      go({ page: "job", id: d.job.id });
    } catch (e) {
      toast(errorMessage(e), { tone: "bad" });
    }
  };

  return (
    <div className="page wide">
      {dialog}
      <Loaded q={q}>
        {(d) => {
          const v = d.vehicle;
          return (
            <>
              <div className="page-head">
                <div className="min-w-0">
                  <button className="btn link mb-1" style={{ paddingLeft: 0 }} onClick={() => go("vehicles")}>
                    ← My Vehicles
                  </button>
                  <h1>{vehicleTitle(v)}</h1>
                  <p className="row">
                    {v.nickname && <span>“{v.nickname}”</span>}
                    {v.vin ? <span className="mono">{v.vin}</span> : <Badge>No VIN</Badge>}
                    {[v.engine, v.transmission].filter(Boolean).join(" · ")}
                    {v.saved ? <Badge tone="info">Saved</Badge> : <Badge>Recent lookup</Badge>}
                    {d.conflicts.length > 0 && <Badge tone="warn">{d.conflicts.length} source conflict{d.conflicts.length > 1 ? "s" : ""}</Badge>}
                  </p>
                </div>
                <div className="row">
                  {v.saved ? (
                    <button className="btn" onClick={() => save.mutate({ id, saved: false })}>
                      <StarOff size={16} aria-hidden /> Unsave
                    </button>
                  ) : (
                    <button className="btn primary" onClick={() => save.mutate({ id, saved: true })}>
                      <Star size={16} aria-hidden /> Save vehicle
                    </button>
                  )}
                  <button className="btn" onClick={() => newEstimate(v)}>
                    <Calculator size={16} aria-hidden /> New estimate
                  </button>
                  <ActionsMenu
                    items={[
                      { label: "Edit details", icon: <Pencil size={14} />, run: () => setEditing(true) },
                      v.vin && v.vin.length === 17 ? { label: refreshing ? "Refreshing…" : "Refresh from NHTSA", icon: refreshing ? <Spinner /> : <RefreshCw size={14} />, run: () => reDecode(v.vin!), disabled: refreshing } : null,
                      null,
                      {
                        label: "Delete vehicle",
                        icon: <Trash2 size={14} />,
                        danger: true,
                        run: async () => {
                          if (await ask("Delete this vehicle?", <>Its specifications, notes and links are removed. Estimates and service jobs stay in your history without the vehicle.</>)) {
                            del.mutate({ id }, { onSuccess: () => go("vehicles") });
                          }
                        },
                      },
                    ].filter((x) => x !== null)}
                  />
                </div>
              </div>
              <Tabs
                label="Vehicle information"
                value={tab}
                onChange={setTab}
                tabs={[
                  { id: "overview", label: "Overview" },
                  { id: "specs", label: "Full Specifications" },
                  { id: "parts", label: "Parts" },
                  { id: "manuals", label: "Manuals" },
                  { id: "maintenance", label: "Maintenance" },
                  { id: "recalls", label: "Recalls & Safety" },
                  { id: "stock", label: "Parts in Stock" },
                  { id: "service", label: "Service History" },
                ]}
              />
              {tab === "overview" && <Overview d={d} />}
              {tab === "specs" && <Specs d={d} />}
              {tab === "parts" && <Matches vehicle={v} inStockOnly={false} />}
              {tab === "stock" && <Matches vehicle={v} inStockOnly />}
              {tab === "manuals" && <VehicleManuals vehicle={v} />}
              {tab === "maintenance" && <Maintenance vehicle={v} />}
              {tab === "recalls" && <Recalls vehicle={v} />}
              {tab === "service" && <VehicleService vehicle={v} />}
              {editing && <EditVehicle v={v} onClose={() => setEditing(false)} />}
            </>
          );
        }}
      </Loaded>
    </div>
  );
}

function EditVehicle({ v, onClose }: { v: Vehicle; onClose: () => void }) {
  const customers = useCmd<Customer[]>("customer_list", { q: null });
  const [f, setF] = useState<VehicleUpdate>({
    nickname: v.nickname,
    color: v.color,
    plate: v.plate,
    mileage: v.mileage,
    mileage_unit: v.mileage_unit,
    purchase_date: v.purchase_date,
    purchase_price: v.purchase_price,
    notes: v.notes,
    customer_id: v.customer_id,
  });
  const save = useAction<{ id: string; input: VehicleUpdate }>("vehicle_update", { success: "Saved", silentError: true });
  const err = save.error;
  const fe = (k: string) => (err && (err as { field?: string }).field === k ? errorMessage(err) : null);
  const set = <K extends keyof VehicleUpdate>(k: K, val: VehicleUpdate[K]) => setF({ ...f, [k]: val });
  return (
    <Modal
      open
      title="Vehicle details"
      onClose={onClose}
      footer={
        <>
          <button className="btn" onClick={onClose}>
            Cancel
          </button>
          <button className="btn primary" onClick={() => save.mutate({ id: v.id, input: f }, { onSuccess: onClose })}>
            Save
          </button>
        </>
      }
    >
      <div className="grid two">
        <Field label="Nickname">
          <input type="text" value={f.nickname} onChange={(e) => set("nickname", e.target.value)} />
        </Field>
        <Field label="Colour">
          <input type="text" value={f.color} onChange={(e) => set("color", e.target.value)} />
        </Field>
        <Field label="License plate (optional)">
          <input type="text" value={f.plate} onChange={(e) => set("plate", e.target.value)} />
        </Field>
        <Field label="Mileage" error={fe("mileage")}>
          <div className="row" style={{ flexWrap: "nowrap" }}>
            <input type="number" min={0} value={f.mileage ?? ""} onChange={(e) => set("mileage", e.target.value === "" ? null : Number(e.target.value))} style={{ flex: 1 }} />
            <select value={f.mileage_unit} onChange={(e) => set("mileage_unit", e.target.value)} aria-label="Mileage unit">
              <option value="mi">mi</option>
              <option value="km">km</option>
            </select>
          </div>
        </Field>
        <Field label="Purchase date" error={fe("purchase_date")}>
          <input type="date" value={f.purchase_date ?? ""} onChange={(e) => set("purchase_date", e.target.value || null)} />
        </Field>
        <Field label="Purchase price" error={fe("purchase_price")}>
          <input type="text" inputMode="decimal" value={f.purchase_price ?? ""} onChange={(e) => set("purchase_price", e.target.value || null)} />
        </Field>
        <Field label="Owner (customer)">
          <select value={f.customer_id ?? ""} onChange={(e) => set("customer_id", e.target.value || null)}>
            <option value="">— None —</option>
            {customers.data?.map((c) => (
              <option key={c.id} value={c.id}>
                {c.name}
              </option>
            ))}
          </select>
        </Field>
      </div>
      <Field label="Notes">
        <textarea value={f.notes} onChange={(e) => set("notes", e.target.value)} />
      </Field>
      {err != null && !fe("mileage") && !fe("purchase_date") && !fe("purchase_price") && <div className="notice bad">{errorMessage(err)}</div>}
    </Modal>
  );
}
