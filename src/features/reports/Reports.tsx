import { useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { Download, Printer, ShieldCheck } from "lucide-react";
import { save as saveFile } from "@tauri-apps/plugin-dialog";
import { call, errorMessage } from "../../api/ipc";
import { useCmd } from "../../api/queries";
import type { IssueGroup, Report, ReportInfo, ReportQuery, Vehicle } from "../../api/types";
import { useApp, toast } from "../../store";
import { Badge, cx, ErrorNote, PageHead, Spinner, Tabs } from "../../ui";
import { money } from "../../lib/format";
import { vehicleTitle } from "../vehicles/Garage";

const today = () => new Date().toISOString().slice(0, 10);
const yearStart = () => `${new Date().getFullYear()}-01-01`;

export default function Reports() {
  const { route, setTab } = useApp();
  const tab = route.tab ?? "reports";
  return (
    <div className="page wide">
      <PageHead title="Reports" sub="Computed from your stored records. Print to PDF, or export to CSV or Excel." />
      <Tabs
        label="Reports"
        value={tab}
        onChange={setTab}
        tabs={[
          { id: "reports", label: "Reports" },
          { id: "quality", label: "Data Quality Center" },
        ]}
      />
      {tab === "reports" ? <ReportBrowser /> : <Quality />}
    </div>
  );
}

function ReportBrowser() {
  const list = useCmd<ReportInfo[]>("report_list");
  const vehicles = useCmd<Vehicle[]>("vehicle_list", { saved: true });
  const [id, setId] = useState("inventory_value");
  const [from, setFrom] = useState(yearStart());
  const [to, setTo] = useState(today());
  const [vid, setVid] = useState("");
  const info = list.data?.find((r) => r.id === id);
  const query: ReportQuery = { id, from, to, vehicle_id: vid || null };
  const r = useQuery<Report>({ queryKey: ["report", query], queryFn: () => call<Report>("report", { query }), enabled: !info?.needs_vehicle || !!vid, retry: false });
  const groups = [...new Set((list.data ?? []).map((x) => x.group))];
  const exportTo = async (ext: "csv" | "xlsx") => {
    const path = await saveFile({ defaultPath: `${id}-${to}.${ext}`, filters: [{ name: ext.toUpperCase(), extensions: [ext] }] });
    if (!path) return;
    try {
      const n = await call<number>("report_export", { query, path });
      toast(`Exported ${n} rows`);
    } catch (e) {
      toast(errorMessage(e), { tone: "bad" });
    }
  };
  const isMoney = (h: string) => /value|cost|price|revenue|profit|total|amount|parts|labor|tax|spend/i.test(h) && !/units|count|qty/i.test(h);
  return (
    <div className="flex gap-4" style={{ alignItems: "flex-start" }}>
      <nav className="card no-print" style={{ width: 250, flex: "none", padding: 8 }} aria-label="Reports">
        {groups.map((g) => (
          <div key={g} className="mb-2">
            <p className="label" style={{ margin: "6px 8px" }}>
              {g}
            </p>
            {list.data
              ?.filter((x) => x.group === g)
              .map((x) => (
                <button key={x.id} className={cx("btn ghost w-full", x.id === id && "soft")} style={{ justifyContent: "flex-start" }} aria-current={x.id === id} onClick={() => setId(x.id)}>
                  {x.title}
                </button>
              ))}
          </div>
        ))}
      </nav>
      <div className="flex-1 min-w-0 stack">
        <div className="row no-print">
          {info?.uses_dates && (
            <>
              <label className="row small">
                From <input type="date" value={from} onChange={(e) => setFrom(e.target.value)} />
              </label>
              <label className="row small">
                To <input type="date" value={to} onChange={(e) => setTo(e.target.value)} />
              </label>
            </>
          )}
          {info?.needs_vehicle && (
            <select value={vid} onChange={(e) => setVid(e.target.value)} aria-label="Vehicle">
              <option value="">Choose a vehicle…</option>
              {vehicles.data?.map((v) => (
                <option key={v.id} value={v.id}>
                  {vehicleTitle(v)}
                </option>
              ))}
            </select>
          )}
          <span className="flex-1" />
          <button className="btn" onClick={() => window.print()} disabled={!r.data}>
            <Printer size={16} aria-hidden /> Print / PDF
          </button>
          <button className="btn" onClick={() => exportTo("csv")} disabled={!r.data}>
            <Download size={16} aria-hidden /> CSV
          </button>
          <button className="btn" onClick={() => exportTo("xlsx")} disabled={!r.data}>
            <Download size={16} aria-hidden /> Excel
          </button>
        </div>
        {info?.needs_vehicle && !vid && <p className="muted">Choose a vehicle.</p>}
        {r.isFetching && <Spinner label="Working out the report" />}
        {r.isError && <ErrorNote error={r.error} />}
        {r.data && (
          <article className="print-sheet stack" style={{ background: "var(--panel)", color: "var(--text)" }}>
            <div>
              <h2 style={{ fontSize: 18 }}>{r.data.title}</h2>
              <p className="small muted">
                {info?.uses_dates && `${from} to ${to} · `}
                {r.data.note}
              </p>
            </div>
            {r.data.summary.length > 0 && (
              <dl className="kv">
                {r.data.summary.map(([k, v]) => (
                  <div key={k} style={{ display: "contents" }}>
                    <dt>{k}</dt>
                    <dd>
                      <strong>{/^-?\d+(\.\d+)?$/.test(v) && isMoney(k) ? money(v) : v}</strong>
                    </dd>
                  </div>
                ))}
              </dl>
            )}
            {r.data.rows.length === 0 ? (
              <p className="muted">No records for this report{info?.uses_dates ? " in this period" : ""}.</p>
            ) : (
              <div className="table-wrap" style={{ maxHeight: "none", boxShadow: "none" }}>
                <table>
                  <thead>
                    <tr>
                      {r.data.headers.map((h, i) => (
                        <th key={h} className={r.data!.numeric[i] ? "num" : undefined}>
                          {h}
                        </th>
                      ))}
                    </tr>
                  </thead>
                  <tbody>
                    {r.data.rows.map((row, k) => (
                      <tr key={k}>
                        {row.map((c, i) => (
                          <td key={i} className={r.data!.numeric[i] ? "num" : undefined}>
                            {r.data!.numeric[i] && isMoney(r.data!.headers[i]) && /^-?\d+(\.\d+)?$/.test(c) ? money(c) : c}
                          </td>
                        ))}
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
            )}
          </article>
        )}
      </div>
    </div>
  );
}

function Quality() {
  const go = useApp((s) => s.go);
  const q = useCmd<IssueGroup[]>("data_quality");
  if (q.isPending) return <Spinner label="Checking your data" />;
  if (q.isError) return <ErrorNote error={q.error} />;
  const total = q.data!.reduce((n, g) => n + g.issues.length, 0);
  return (
    <div className="stack">
      <div className={`notice ${total ? "warn" : "ok"} row`}>
        <ShieldCheck size={16} aria-hidden /> {total ? `${total} things to look at. Fixing them makes parts matching, prices and reports more reliable.` : "No gaps or conflicts found."}
      </div>
      {q.data!
        .filter((g) => g.issues.length > 0)
        .map((g) => (
          <details key={g.id} className="card">
            <summary className="spread">
              <strong>{g.title}</strong> <Badge tone="warn">{g.issues.length}</Badge>
            </summary>
            <p className="small muted">{g.help}</p>
            <ul className="items" style={{ maxHeight: 320 }}>
              {g.issues.map((i) => (
                <li key={i.id}>
                  <button className="btn link" style={{ padding: 0 }} onClick={() => go({ page: i.kind as "vehicle" | "part" | "item", id: i.id, tab: i.kind === "vehicle" ? "specs" : undefined })}>
                    {i.title || "(untitled)"}
                  </button>
                  <span className="small muted">{i.detail}</span>
                </li>
              ))}
            </ul>
          </details>
        ))}
    </div>
  );
}
