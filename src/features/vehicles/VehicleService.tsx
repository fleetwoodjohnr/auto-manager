import { ClipboardList, Plus, Wrench } from "lucide-react";
import { call, errorMessage } from "../../api/ipc";
import { useAction, useCmd } from "../../api/queries";
import type { Job, JobDetail, PartList, Vehicle } from "../../api/types";
import { useApp, toast } from "../../store";
import { Badge, Empty, Loaded } from "../../ui";
import { date, money } from "../../lib/format";
import { useQueries } from "@tanstack/react-query";
import { t, tx } from "../../i18n";

export const STATUS: Record<string, { label: string; tone: "neutral" | "info" | "ok" | "warn" | "bad" }> = {
  estimate: { label: t("Estimate"), tone: "neutral" },
  approved: { label: t("Approved"), tone: "info" },
  in_progress: { label: t("In progress"), tone: "warn" },
  completed: { label: t("Completed"), tone: "ok" },
  invoiced: { label: t("Invoiced"), tone: "ok" },
  cancelled: { label: t("Cancelled"), tone: "bad" },
};

export default function VehicleService({ vehicle }: { vehicle: Vehicle }) {
  const go = useApp((s) => s.go);
  const jobs = useCmd<Job[]>("job_list", { query: { status: null, vehicle_id: vehicle.id, customer_id: null } });
  const lists = useCmd<PartList[]>("plist_list", { vehicleId: vehicle.id });
  const totals = useQueries({ queries: (jobs.data ?? []).map((j) => ({ queryKey: ["job_get", { id: j.id }], queryFn: () => call<JobDetail>("job_get", { id: j.id }) })) });
  const remove = useAction<{ listId: string; partId: string }>("plist_remove", { invalidate: ["plist_list"] });
  const create = async () => {
    try {
      const d = await call<JobDetail>("job_create", { input: { vehicle_id: vehicle.id, customer_id: vehicle.customer_id, title: "" } });
      go({ page: "job", id: d.job.id });
    } catch (e) {
      toast(errorMessage(e), { tone: "bad" });
    }
  };
  return (
    <div className="grid two" style={{ alignItems: "start" }}>
      <section className="card">
        <div className="spread">
          <h2>{t("Estimates and jobs")}</h2>
          <button className="btn" onClick={create}>
            <Plus size={16} aria-hidden />{" "}{t("New estimate")}</button>
        </div>
        <Loaded q={jobs}>
          {(list) =>
            list.length === 0 ? (
              <Empty icon={<Wrench size={20} />} title={t("No service history yet")} />
            ) : (
              <table>
                <thead>
                  <tr>
                    <th>{t("Job")}</th>
                    <th>{t("Date")}</th>
                    <th>{t("Status")}</th>
                    <th className="num">{t("Total")}</th>
                  </tr>
                </thead>
                <tbody>
                  {list.map((j, i) => (
                    <tr key={j.id} style={{ cursor: "pointer" }} onClick={() => go({ page: "job", id: j.id })}>
                      <td>
                        #{j.number} {j.title}
                      </td>
                      <td>{date(j.completed_at ?? j.created_at)}</td>
                      <td>
                        <Badge tone={STATUS[j.status]?.tone}>{STATUS[j.status]?.label}</Badge>
                      </td>
                      <td className="num">{totals[i]?.data ? money(totals[i].data!.totals.total) : "…"}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            )
          }
        </Loaded>
      </section>
      <section className="card">
        <h2>
          <ClipboardList size={16} aria-hidden />{" "}{t("Parts lists")}</h2>
        <Loaded q={lists}>
          {(ls) =>
            ls.length === 0 ? (
              <p className="muted">{t("Save parts to a list from the Parts tab to plan a repair.")}</p>
            ) : (
              <div className="stack">
                {ls.map((l) => (
                  <div key={l.id}>
                    <p className="label">{l.name}</p>
                    <ul className="items" style={{ maxHeight: "none" }}>
                      {l.items.map((it) => (
                        <li key={it.part_id}>
                          <button className="btn link" style={{ padding: 0 }} onClick={() => go({ page: "part", id: it.part_id })}>
                            {it.qty} × {it.name}
                          </button>
                          <span className="small muted">{it.numbers}</span>
                          <span className="ml-auto row">
                            <Badge tone={it.available >= it.qty ? "ok" : "warn"}>{tx("{0} available", it.available)}</Badge>
                            <button className="btn link" onClick={() => remove.mutate({ listId: l.id, partId: it.part_id })}>{t("Remove")}</button>
                          </span>
                        </li>
                      ))}
                    </ul>
                  </div>
                ))}
              </div>
            )
          }
        </Loaded>
      </section>
    </div>
  );
}
