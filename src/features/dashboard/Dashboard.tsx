import type { ReactNode } from "react";
import { AlertTriangle, Barcode, Boxes, Calculator, Car, CarFront, CircleCheck, Cog, PackagePlus, ScanSearch, ShieldAlert, Wrench, CalendarClock } from "lucide-react";
import { useCmd } from "../../api/queries";
import type { AppInfo, Dashboard as D, Settings } from "../../api/types";
import { useApp } from "../../store";
import { useSettings } from "../../settings";
import { Badge, Empty, Loaded } from "../../ui";
import { ago, money } from "../../lib/format";
import { useUpdateStore } from "../settings/updates";

function Card({ title, children, action }: { title: string; children: ReactNode; action?: ReactNode }) {
  return (
    <section className="card">
      <div className="spread">
        <h2>{title}</h2>
        {action}
      </div>
      {children}
    </section>
  );
}

const MOVE: Record<string, string> = {
  receive: "Received",
  issue: "Issued",
  transfer_out: "Moved out",
  transfer_in: "Moved in",
  adjust: "Adjusted",
  count: "Counted",
  damage: "Marked damaged",
  return_in: "Returned by customer",
  return_out: "Returned to supplier",
  consume: "Used on a job",
  reverse: "Undone",
};
export const movementLabel = (k: string) => MOVE[k] ?? k;

export default function Dashboard() {
  const go = useApp((s) => s.go);
  const q = useCmd<D>("dashboard");
  const { settings } = useSettings();
  const widgets = settings?.dashboard_widgets ?? [];
  const show = (w: string) => widgets.includes(w);
  return (
    <div className="page wide">
      <div className="page-head">
        <div>
          <h1>Dashboard</h1>
          <p>Find a vehicle, understand it, find its parts and manuals, check stock, then price and service it.</p>
        </div>
      </div>
      {show("shortcuts") && (
        <div className="starts mb-4">
          <Shortcut icon={<ScanSearch size={18} />} title="Look Up Vehicle" text="Decode a VIN or pick year, make and model" onClick={() => go("lookup")} />
          <Shortcut icon={<Car size={18} />} title="Add Vehicle" text="Save an older or non-standard vehicle by hand" onClick={() => go({ page: "lookup", tab: "manual" })} />
          <Shortcut icon={<PackagePlus size={18} />} title="Add Inventory" text="Stock a part and set its price" onClick={() => go({ page: "inventory", tab: "new" })} />
          <Shortcut icon={<Barcode size={18} />} title="Scan Barcode" text="Use a USB scanner or type the number" onClick={() => go({ page: "inventory", tab: "scan" })} />
          <Shortcut icon={<Cog size={18} />} title="Find Compatible Parts" text="Parts with fitment for a saved vehicle" onClick={() => go("vehicles")} />
          <Shortcut icon={<Calculator size={18} />} title="Create Estimate" text="Parts, labor and tax, with your pricing" onClick={() => go({ page: "service", tab: "new" })} />
        </div>
      )}
      <Loaded q={q}>
        {(d) => (
          <div className="grid two">
            {show("inventory") && (
              <Card title="Inventory" action={<button className="btn link" onClick={() => go("inventory")}>Open</button>}>
                {d.item_count === 0 ? (
                  <Empty icon={<Boxes size={20} />} title="No inventory yet" action={<button className="btn" onClick={() => go({ page: "inventory", tab: "new" })}>Add an item</button>}>
                    Add items one by one or import a spreadsheet.
                  </Empty>
                ) : (
                  <div className="tiles">
                    <Tile n={String(d.item_count)} what="items stocked" />
                    <Tile n={String(d.units_on_hand)} what="units on hand" />
                    <Tile n={money(d.value_at_cost)} what={d.valuation_method === "standard" ? "value at last cost" : "value at average cost"} />
                    <Tile n={String(d.out_of_stock_count)} what="out of stock" tone={d.out_of_stock_count ? "bad" : undefined} />
                  </div>
                )}
              </Card>
            )}
            {show("low_stock") && d.item_count > 0 && (
              <Card title="Low stock and restocking" action={<button className="btn link" onClick={() => go({ page: "inventory", tab: "low" })}>See all</button>}>
                {d.low_stock.length === 0 ? (
                  <p className="muted flex items-center gap-2">
                    <CircleCheck size={16} aria-hidden style={{ color: "var(--success)" }} /> Everything is above its minimum.
                  </p>
                ) : (
                  <>
                    <p className="small muted">
                      {d.low_stock_count} at or below minimum · {d.to_reorder} with a reorder quantity set
                    </p>
                    <ul className="items" style={{ maxHeight: "none" }}>
                      {d.low_stock.map((i) => (
                        <li key={i.id}>
                          <button className="btn link" style={{ padding: 0 }} onClick={() => go({ page: "item", id: i.id })}>
                            {i.name}
                          </button>
                          <span className="small muted">{i.sku}</span>
                          <span className="ml-auto">
                            <Badge tone={i.available <= 0 ? "bad" : "warn"}>
                              {i.available} / min {i.min_stock}
                            </Badge>
                          </span>
                        </li>
                      ))}
                    </ul>
                  </>
                )}
              </Card>
            )}
            {show("recent_vehicles") && (
              <Card title="Recently searched vehicles" action={<button className="btn link" onClick={() => go("vehicles")}>Garage ({d.saved_vehicles})</button>}>
                {d.recent_vehicles.length === 0 ? (
                  <Empty icon={<CarFront size={20} />} title="No vehicles yet" action={<button className="btn primary" onClick={() => go("lookup")}>Look up a VIN</button>}>
                    Enter a VIN to see what NHTSA has on the vehicle.
                  </Empty>
                ) : (
                  <ul className="items" style={{ maxHeight: "none" }}>
                    {d.recent_vehicles.map((v) => (
                      <li key={v.id}>
                        <button className="btn link" style={{ padding: 0 }} onClick={() => go({ page: "vehicle", id: v.id })}>
                          {[v.year, v.make, v.model, v.trim].filter(Boolean).join(" ") || v.vin || "Unidentified vehicle"}
                        </button>
                        <span className="small muted">{v.nickname || v.vin}</span>
                        <span className="ml-auto row">
                          {v.saved && <Badge tone="info">Saved</Badge>}
                          <span className="small muted">{ago(v.last_viewed_at)}</span>
                        </span>
                      </li>
                    ))}
                  </ul>
                )}
              </Card>
            )}
            {show("jobs") && (
              <Card title="Open service jobs" action={<button className="btn link" onClick={() => go("service")}>All jobs</button>}>
                {d.open_jobs.length === 0 ? (
                  <p className="muted">No open estimates or jobs.</p>
                ) : (
                  <ul className="items" style={{ maxHeight: "none" }}>
                    {d.open_jobs.map((j) => (
                      <li key={j.id}>
                        <button className="btn link" style={{ padding: 0 }} onClick={() => go({ page: "job", id: j.id })}>
                          #{j.number} {j.title || j.vehicle_title || "Untitled"}
                        </button>
                        <span className="small muted">{j.customer_name}</span>
                        <span className="ml-auto">
                          <Badge tone={j.status === "estimate" ? "neutral" : "info"}>{j.status.replace("_", " ")}</Badge>
                        </span>
                      </li>
                    ))}
                  </ul>
                )}
              </Card>
            )}
            {show("maintenance") && <MaintenanceDue />}
            {show("recalls") && <RecallNotices />}
            {show("activity") && d.recent_movements.length > 0 && (
              <Card title="Recent inventory activity">
                <ul className="items" style={{ maxHeight: "none" }}>
                  {d.recent_movements.map((m) => (
                    <li key={m.id}>
                      <span style={{ minWidth: 120 }}>{movementLabel(m.kind)}</span>
                      <button className="btn link" style={{ padding: 0 }} onClick={() => go({ page: "item", id: m.item_id })}>
                        {m.name}
                      </button>
                      <span className="ml-auto row">
                        <span className="mono" style={{ color: m.qty < 0 ? "var(--danger)" : "var(--success)" }}>
                          {m.qty > 0 ? "+" : "−"}
                          {Math.abs(m.qty)}
                        </span>
                        <span className="small muted">{ago(m.created_at)}</span>
                      </span>
                    </li>
                  ))}
                </ul>
              </Card>
            )}
            {show("updates") && <StatusCard />}
          </div>
        )}
      </Loaded>
    </div>
  );
}

function Shortcut({ icon, title, text, onClick }: { icon: ReactNode; title: string; text: string; onClick: () => void }) {
  return (
    <button className="start" onClick={onClick}>
      <span className="icon-chip" aria-hidden>
        {icon}
      </span>
      <span>
        <strong>{title}</strong>
        <span>{text}</span>
      </span>
    </button>
  );
}

function Tile({ n, what, tone }: { n: string; what: string; tone?: "bad" }) {
  return (
    <div className="card" style={{ padding: 12, boxShadow: "none" }}>
      <div className="stat" style={{ fontSize: 22, color: tone === "bad" ? "var(--danger)" : undefined }}>
        {n}
      </div>
      <div className="small muted">{what}</div>
    </div>
  );
}

interface DueItem {
  vehicle_id: string;
  vehicle_title: string;
  service: string;
  due_date: string | null;
  due_mileage: number | null;
  overdue: boolean;
}

function MaintenanceDue() {
  const go = useApp((s) => s.go);
  const q = useCmd<DueItem[]>("maintenance_due");
  if (!q.data || q.data.length === 0) return null;
  return (
    <Card title="Upcoming maintenance" action={<button className="btn link" onClick={() => go({ page: "service", tab: "maintenance" })}>All</button>}>
      <ul className="items" style={{ maxHeight: "none" }}>
        {q.data.slice(0, 8).map((m, i) => (
          <li key={i}>
            <CalendarClock size={15} aria-hidden />
            <button className="btn link" style={{ padding: 0 }} onClick={() => go({ page: "vehicle", id: m.vehicle_id, tab: "maintenance" })}>
              {m.vehicle_title}
            </button>
            <span>{m.service}</span>
            <span className="ml-auto">
              <Badge tone={m.overdue ? "bad" : "warn"}>{[m.due_date, m.due_mileage != null ? `${m.due_mileage.toLocaleString()} mi` : null].filter(Boolean).join(" or ")}</Badge>
            </span>
          </li>
        ))}
      </ul>
    </Card>
  );
}

interface RecallNotice {
  vehicle_id: string;
  vehicle_title: string;
  count: number;
}

function RecallNotices() {
  const go = useApp((s) => s.go);
  const q = useCmd<RecallNotice[]>("recall_notices");
  if (!q.data || q.data.length === 0) return null;
  return (
    <Card title="Recall notices">
      <p className="small muted">Campaigns NHTSA lists for these vehicles' year, make and model. Check each VIN to see whether the repair is still outstanding.</p>
      <ul className="items" style={{ maxHeight: "none" }}>
        {q.data.map((r) => (
          <li key={r.vehicle_id}>
            <ShieldAlert size={15} aria-hidden style={{ color: "var(--warn)" }} />
            <button className="btn link" style={{ padding: 0 }} onClick={() => go({ page: "vehicle", id: r.vehicle_id, tab: "recalls" })}>
              {r.vehicle_title}
            </button>
            <span className="ml-auto">
              <Badge tone="warn">
                {r.count} campaign{r.count === 1 ? "" : "s"}
              </Badge>
            </span>
          </li>
        ))}
      </ul>
    </Card>
  );
}

function StatusCard() {
  const go = useApp((s) => s.go);
  const info = useCmd<AppInfo>("app_info");
  const settings = useCmd<Settings>("settings_get");
  const u = useUpdateStore();
  return (
    <Card title="Application and data" action={<button className="btn link" onClick={() => go({ page: "settings", tab: "updates" })}>Updates</button>}>
      <dl className="kv">
        <dt>Version</dt>
        <dd>
          {info.data?.version ?? "…"} {u.available && <Badge tone="info">{u.available.version} available</Badge>}
        </dd>
        <dt>Last update check</dt>
        <dd>{settings.data?.last_update_check ? ago(settings.data.last_update_check) : "Never"}</dd>
        <dt>Data</dt>
        <dd>Stored on this computer {settings.data?.backup_auto ? "· backed up daily" : <span style={{ color: "var(--warn)" }}>· automatic backups are off <AlertTriangle size={13} aria-hidden /></span>}</dd>
      </dl>
      <p className="small muted mt-2">
        <Wrench size={12} aria-hidden /> Vehicle data comes from NHTSA when online and is saved for offline use.
      </p>
    </Card>
  );
}
