import { useState } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { CloudOff, ExternalLink, RefreshCw, ShieldAlert, Star } from "lucide-react";
import { call, errorMessage } from "../../api/ipc";
import type { Ratings, Recall, Vehicle, VehicleRecalls } from "../../api/types";
import { toast } from "../../store";
import { Badge, Empty, ErrorNote, ExtLink, Spinner } from "../../ui";
import { date, dateTime } from "../../lib/format";

const STATUS: Record<string, { label: string; tone: "bad" | "ok" | "neutral" }> = {
  outstanding: { label: "Outstanding for this VIN", tone: "bad" },
  completed: { label: "Repair completed", tone: "ok" },
  not_applicable: { label: "Doesn't apply to this VIN", tone: "neutral" },
};

/** Recall campaigns for the vehicle's model year, make and model, and NHTSA crash ratings. */
export default function Recalls({ vehicle }: { vehicle: Vehicle }) {
  const qc = useQueryClient();
  const [online, setOnline] = useState(true);
  const q = useQuery<VehicleRecalls>({
    queryKey: ["recalls", vehicle.id, online],
    queryFn: () => (online ? call<VehicleRecalls>("recalls_fetch", { vehicleId: vehicle.id }) : call<VehicleRecalls>("recalls_saved", { vehicleId: vehicle.id })),
    retry: false,
  });
  const ratings = useQuery<Ratings>({ queryKey: ["safety_ratings", vehicle.id], queryFn: () => call<Ratings>("safety_ratings", { vehicleId: vehicle.id }), retry: false });
  const setStatus = async (r: Recall, status: string | null) => {
    try {
      await call("recall_set_status", { vehicleId: vehicle.id, campaign: r.campaign, status, note: null });
      qc.invalidateQueries({ queryKey: ["recalls"] });
      qc.invalidateQueries({ queryKey: ["recall_notices"] });
    } catch (e) {
      toast(errorMessage(e), { tone: "bad" });
    }
  };
  return (
    <div className="stack">
      <div className="notice info">
        <strong>These campaigns are listed for every {vehicle.year} {vehicle.make} {vehicle.model}.</strong> NHTSA's public data is by model, so a campaign may not cover this particular vehicle, and its repair may already be done.
        {vehicle.vin ? (
          <> Check this VIN on <ExtLink href="https://www.nhtsa.gov/recalls">nhtsa.gov/recalls</ExtLink> (or with the manufacturer), then record the answer for each campaign below.</>
        ) : (
          " Add the VIN to check which campaigns are outstanding."
        )}
      </div>
      {q.isPending && <Spinner label="Loading recalls" />}
      {q.isError && (
        <>
          <ErrorNote error={q.error} />
          {online && (
            <button className="btn" onClick={() => setOnline(false)} style={{ alignSelf: "flex-start" }}>
              <CloudOff size={16} aria-hidden /> Show saved recalls
            </button>
          )}
        </>
      )}
      {q.data && (
        <>
          <div className="spread">
            <span className="small muted">
              {q.data.freshness ? `NHTSA Recalls · ${q.data.freshness.stale ? "offline, saved" : "updated"} ${dateTime(q.data.freshness.fetched_at)}` : "Saved copy (not refreshed)"}
            </span>
            <button className="btn" onClick={() => (setOnline(true), q.refetch())}>
              <RefreshCw size={16} aria-hidden /> Refresh
            </button>
          </div>
          {q.data.recalls.length === 0 ? (
            <Empty icon={<ShieldAlert size={20} />} title="NHTSA lists no recalls for this model year, make and model" />
          ) : (
            q.data.recalls.map((r) => (
              <section key={r.campaign} className="card stack" style={{ gap: 6 }}>
                <div className="spread">
                  <div>
                    <strong className="mono">{r.campaign}</strong> <span className="muted">· {date(r.report_date)}</span>
                    <div className="small">{r.component}</div>
                  </div>
                  <div className="row">
                    {r.park_it && <Badge tone="bad">Don't drive</Badge>}
                    {r.park_outside && <Badge tone="bad">Park outside</Badge>}
                    {r.over_the_air && <Badge tone="info">Over-the-air fix</Badge>}
                    {r.status ? <Badge tone={STATUS[r.status].tone}>{STATUS[r.status].label}</Badge> : <Badge tone="warn">Not checked for this VIN</Badge>}
                  </div>
                </div>
                <p style={{ margin: 0 }}>{r.summary}</p>
                {r.consequence && (
                  <p className="small" style={{ margin: 0 }}>
                    <strong>Risk:</strong> {r.consequence}
                  </p>
                )}
                {r.remedy && (
                  <p className="small" style={{ margin: 0 }}>
                    <strong>Remedy:</strong> {r.remedy}
                  </p>
                )}
                <div className="row small">
                  <span className="muted">Per-VIN result:</span>
                  {Object.entries(STATUS).map(([k, s]) => (
                    <button key={k} className={`btn ${r.status === k ? "soft" : "ghost"}`} style={{ minHeight: 26 }} onClick={() => setStatus(r, r.status === k ? null : k)}>
                      {s.label}
                    </button>
                  ))}
                  <ExtLink href={`https://www.nhtsa.gov/recalls?nhtsaId=${encodeURIComponent(r.campaign)}`}>
                    NHTSA record <ExternalLink size={12} aria-hidden />
                  </ExtLink>
                </div>
              </section>
            ))
          )}
        </>
      )}
      <section className="card">
        <h2>Technical service bulletins</h2>
        <p className="small" style={{ margin: 0 }}>
          NHTSA publishes manufacturers' service bulletins and other communications only as a bulk download, with no per-vehicle lookup Auto Manager can use, so they aren't listed here. Search them for this vehicle on{" "}
          <ExtLink href="https://www.nhtsa.gov/recalls">nhtsa.gov</ExtLink> (choose the vehicle, then “Manufacturer Communications”). Bulletins you have as PDFs can be imported under Manuals with the type “Technical service bulletin”.
        </p>
      </section>
      <section className="card">
        <h2>
          <Star size={16} aria-hidden /> NHTSA 5-Star Safety Ratings
        </h2>
        {ratings.isPending && <Spinner label="Loading ratings" />}
        {ratings.isError && <ErrorNote error={ratings.error} />}
        {ratings.data &&
          (ratings.data.ratings.length === 0 ? (
            <p className="muted">NHTSA hasn't rated this model year, make and model.</p>
          ) : (
            <div className="table-wrap" style={{ boxShadow: "none" }}>
              <table>
                <thead>
                  <tr>
                    <th>Variant</th>
                    <th>Overall</th>
                    <th>Frontal</th>
                    <th>Side</th>
                    <th>Rollover</th>
                    <th>Complaints</th>
                  </tr>
                </thead>
                <tbody>
                  {ratings.data.ratings.map((r) => (
                    <tr key={r.vehicle_id}>
                      <td>{r.description}</td>
                      <td>{r.overall ? `${r.overall} ★` : "Not rated"}</td>
                      <td>{r.front ?? "—"}</td>
                      <td>{r.side ?? "—"}</td>
                      <td>
                        {r.rollover ?? "—"}
                        {r.rollover_risk && <span className="small muted"> ({r.rollover_risk} risk)</span>}
                      </td>
                      <td>{r.complaints ?? "—"}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          ))}
        <p className="small muted mt-1">Ratings are for the model variant shown, not specifically this VIN.</p>
      </section>
    </div>
  );
}
