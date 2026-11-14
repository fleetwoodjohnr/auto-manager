import { useState } from "react";
import { Plus, Users, Wrench } from "lucide-react";
import { useCmd } from "../../api/queries";
import type { Job } from "../../api/types";
import { useApp } from "../../store";
import { Badge, DataTable, Empty, Loaded, PageHead, Tabs } from "../../ui";
import { date } from "../../lib/format";
import { STATUS } from "../vehicles/VehicleService";
import { JobDetails } from "./JobPage";
import { MaintenanceOverview } from "./Maintenance";
import { TroubleCodes } from "./TroubleCodes";
import { ObdScanner } from "./ObdScanner";

export default function Service() {
  const { route, setTab, go } = useApp();
  const tab = route.tab === "new" ? "jobs" : (route.tab ?? "jobs");
  const [creating, setCreating] = useState(route.tab === "new");
  const [status, setStatus] = useState("open");
  const jobs = useCmd<Job[]>("job_list", { query: { status: status || null, vehicle_id: null, customer_id: null } });
  return (
    <div className="page wide">
      <PageHead title="Maintenance & Service" sub="Estimates, service jobs and invoices; what's due on your vehicles; trouble codes.">
        <button className="btn" onClick={() => go("customers")}>
          <Users size={16} aria-hidden /> Customers
        </button>
        <button className="btn primary" onClick={() => setCreating(true)}>
          <Plus size={16} aria-hidden /> New estimate
        </button>
      </PageHead>
      <Tabs
        label="Service"
        value={tab}
        onChange={setTab}
        tabs={[
          { id: "jobs", label: "Estimates & jobs" },
          { id: "maintenance", label: "Maintenance due" },
          { id: "codes", label: "Trouble codes" },
          { id: "obd", label: "OBD-II scanner" },
        ]}
      />
      {tab === "jobs" && (
        <div className="stack">
          <select value={status} onChange={(e) => setStatus(e.target.value)} aria-label="Status" style={{ alignSelf: "flex-start" }}>
            <option value="open">Open (estimates, approved, in progress)</option>
            <option value="">All</option>
            {Object.entries(STATUS).map(([k, s]) => (
              <option key={k} value={k}>
                {s.label}
              </option>
            ))}
          </select>
          <Loaded q={jobs}>
            {(list) =>
              list.length === 0 ? (
                <Empty icon={<Wrench size={20} />} title="No jobs here" action={<button className="btn" onClick={() => setCreating(true)}>New estimate</button>}>
                  An estimate becomes a job when approved, then an invoice when the work is done.
                </Empty>
              ) : (
                <DataTable
                  label="Jobs"
                  rows={list}
                  rowKey={(j) => j.id}
                  onOpen={(j) => go({ page: "job", id: j.id })}
                  height="calc(100vh - 330px)"
                  columns={[
                    { id: "n", label: "#", width: "70px", render: (j) => j.number, sort: (j) => j.number },
                    { id: "title", label: "Job", width: "minmax(160px, 2fr)", render: (j) => j.title || <span className="muted">Untitled</span>, sort: (j) => j.title },
                    { id: "vehicle", label: "Vehicle", width: "minmax(140px, 1.5fr)", render: (j) => j.vehicle_title ?? "—", sort: (j) => j.vehicle_title ?? "" },
                    { id: "customer", label: "Customer", width: "minmax(120px, 1fr)", render: (j) => j.customer_name ?? "—", sort: (j) => j.customer_name ?? "" },
                    { id: "status", label: "Status", width: "130px", render: (j) => <Badge tone={STATUS[j.status].tone}>{STATUS[j.status].label}</Badge>, sort: (j) => j.status },
                    { id: "date", label: "Updated", width: "110px", render: (j) => date(j.updated_at), sort: (j) => j.updated_at },
                  ]}
                />
              )
            }
          </Loaded>
        </div>
      )}
      {tab === "maintenance" && <MaintenanceOverview />}
      {tab === "codes" && <TroubleCodes />}
      {tab === "obd" && <ObdScanner />}
      {creating && <JobDetails d={null} onClose={() => setCreating(false)} />}
    </div>
  );
}
