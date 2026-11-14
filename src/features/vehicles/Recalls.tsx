import { useState } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { CloudOff, ExternalLink, FileText, RefreshCw, ShieldAlert, Star } from "lucide-react";
import { call, errorMessage } from "../../api/ipc";
import type { Bulletin, BulletinDocument, Bulletins as BulletinList, Manual, Ratings, Recall, Vehicle, VehicleRecalls } from "../../api/types";
import { toast } from "../../store";
import { Badge, Empty, ErrorNote, ExtLink, Spinner } from "../../ui";
import { date, dateTime } from "../../lib/format";
import { indexManual } from "../../lib/pdf";

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
      <Bulletins vehicle={vehicle} />
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

function BulletinDocs({ vehicle, b }: { vehicle: Vehicle; b: Bulletin }) {
  const docs = useQuery<BulletinDocument[]>({ queryKey: ["bulletin_documents", b.nhtsa_id], queryFn: () => call("bulletin_documents", { nhtsaId: b.nhtsa_id }), retry: false });
  const [busy, setBusy] = useState<string | null>(null);
  const add = async (d: BulletinDocument) => {
    setBusy(d.url);
    try {
      const m = await call<Manual>("bulletin_import", { vehicleId: vehicle.id, url: d.url, title: `${b.number || `NHTSA ${b.nhtsa_id}`}: ${b.title}` });
      toast(`Added “${m.title}” to Manuals`);
      void indexManual(m.id).catch(() => undefined);
    } catch (e) {
      toast(errorMessage(e), { tone: "bad" });
    } finally {
      setBusy(null);
    }
  };
  if (docs.isPending) return <Spinner label="Loading documents" />;
  if (docs.isError) return <ErrorNote error={docs.error} />;
  if (docs.data.length === 0) return <p className="small muted">NHTSA has no document for this bulletin.</p>;
  return (
    <ul className="stack" style={{ gap: 4, margin: 0, paddingLeft: 0, listStyle: "none" }}>
      {docs.data.map((d) => (
        <li key={d.url} className="row small">
          <FileText size={14} aria-hidden /> {d.description || d.file_name}
          <ExtLink href={d.url}>
            Open on NHTSA <ExternalLink size={12} aria-hidden />
          </ExtLink>
          <button className="btn ghost small" disabled={busy !== null} onClick={() => add(d)}>
            {busy === d.url ? "Adding…" : "Add to library"}
          </button>
        </li>
      ))}
    </ul>
  );
}

/** Technical service bulletins and other manufacturer communications NHTSA lists for the model. */
function Bulletins({ vehicle }: { vehicle: Vehicle }) {
  const q = useQuery<BulletinList>({ queryKey: ["bulletins", vehicle.id], queryFn: () => call("bulletins_fetch", { vehicleId: vehicle.id }), retry: false });
  const [filter, setFilter] = useState("");
  const [all, setAll] = useState(false);
  const [open, setOpen] = useState<number | null>(null);
  const f = filter.trim().toLowerCase();
  const list = (q.data?.bulletins ?? []).filter((b) => !f || `${b.number} ${b.title} ${b.components.join(" ")} ${b.summary}`.toLowerCase().includes(f));
  const shown = all || f ? list : list.slice(0, 15);
  return (
    <section className="card stack" style={{ gap: 8 }}>
      <h2>Technical service bulletins</h2>
      <p className="small muted" style={{ margin: 0 }}>
        Service bulletins and other communications manufacturers sent NHTSA for every {vehicle.year} {vehicle.make} {vehicle.model}. A bulletin may cover only some configurations, so check that it applies.
      </p>
      {q.isPending && <Spinner label="Loading bulletins" />}
      {q.isError && <ErrorNote error={q.error} />}
      {q.data && (
        <>
          <div className="spread">
            <span className="small muted">
              {q.data.bulletins.length} from NHTSA{q.data.freshness ? ` · ${q.data.freshness.stale ? "offline, saved" : "updated"} ${dateTime(q.data.freshness.fetched_at)}` : ""}
            </span>
            {q.data.bulletins.length > 0 && <input type="search" value={filter} onChange={(e) => setFilter(e.target.value)} placeholder="Filter by number, component or words" aria-label="Filter bulletins" style={{ width: 260 }} />}
          </div>
          {q.data.bulletins.length === 0 ? (
            <p className="muted">NHTSA lists no manufacturer communications for this model year, make and model.</p>
          ) : (
            <div className="stack" style={{ gap: 0 }}>
              {shown.map((b) => (
                <div key={b.nhtsa_id} style={{ borderTop: "1px solid var(--border)", padding: "8px 0" }}>
                  <button className="btn ghost" style={{ width: "100%", justifyContent: "space-between", textAlign: "left", height: "auto", padding: "4px 0" }} aria-expanded={open === b.nhtsa_id} onClick={() => setOpen(open === b.nhtsa_id ? null : b.nhtsa_id)}>
                    <span>
                      <strong className="mono">{b.number || `NHTSA ${b.nhtsa_id}`}</strong> {b.title}
                      <span className="small muted"> · {b.date ? date(b.date) : "undated"}{b.components.length ? ` · ${b.components.join(", ")}` : ""}</span>
                    </span>
                  </button>
                  {open === b.nhtsa_id && (
                    <div className="stack" style={{ gap: 6, paddingTop: 4 }}>
                      <p className="small" style={{ margin: 0, whiteSpace: "pre-line" }}>{b.summary}</p>
                      <BulletinDocs vehicle={vehicle} b={b} />
                    </div>
                  )}
                </div>
              ))}
              {list.length > shown.length && (
                <button className="btn ghost" style={{ alignSelf: "flex-start" }} onClick={() => setAll(true)}>
                  Show all {list.length}
                </button>
              )}
              {f && list.length === 0 && <p className="small muted">No bulletin matches “{filter}”.</p>}
            </div>
          )}
        </>
      )}
    </section>
  );
}
