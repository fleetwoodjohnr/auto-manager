import { useRef, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { FileSpreadsheet } from "lucide-react";
import { call, errorMessage } from "../../api/ipc";
import { useRefresh } from "../../api/queries";
import type { Table } from "../../api/types";
import { useApp } from "../../store";
import { Badge, ErrorNote, Field, ProgressBar } from "../../ui";

interface BulkRow {
  vin: string;
  status: "decoded" | "not_identified" | "invalid";
  title: string;
  vehicle_id: string | null;
  message: string;
}

const CHUNK = 50;

/** Decode a spreadsheet of VINs in batches of 50 (NHTSA's limit), one batch at a time. */
export default function BulkDecode() {
  const go = useApp((s) => s.go);
  const refresh = useRefresh();
  const [table, setTable] = useState<Table | null>(null);
  const [col, setCol] = useState(0);
  const [save, setSave] = useState(true);
  const [rows, setRows] = useState<BulkRow[]>([]);
  const [progress, setProgress] = useState<{ done: number; total: number } | null>(null);
  const [error, setError] = useState<unknown>(null);
  const stop = useRef(false);

  const pick = async () => {
    setError(null);
    const path = await open({ filters: [{ name: "Spreadsheet", extensions: ["csv", "xlsx", "xls", "ods"] }] });
    if (typeof path !== "string") return;
    try {
      const t = await call<Table>("table_read", { path });
      setTable(t);
      setRows([]);
      const guess = t.headers.findIndex((h) => /vin/i.test(h));
      setCol(guess >= 0 ? guess : 0);
    } catch (e) {
      setError(e);
    }
  };
  const vins = table ? [...new Set(table.rows.map((r) => (r[col] ?? "").trim().toUpperCase()).filter(Boolean))] : [];
  const run = async () => {
    setRows([]);
    setError(null);
    stop.current = false;
    setProgress({ done: 0, total: vins.length });
    const out: BulkRow[] = [];
    for (let i = 0; i < vins.length; i += CHUNK) {
      if (stop.current) break;
      try {
        out.push(...(await call<BulkRow[]>("bulk_decode", { vins: vins.slice(i, i + CHUNK), save })));
      } catch (e) {
        setError(`Stopped after ${i} VINs: ${errorMessage(e)}`);
        break;
      }
      setRows([...out]);
      setProgress({ done: Math.min(vins.length, i + CHUNK), total: vins.length });
    }
    refresh();
  };
  const count = (s: BulkRow["status"]) => rows.filter((r) => r.status === s).length;
  return (
    <div className="stack">
      <div className="card stack">
        <p className="muted">Choose a CSV or Excel file with a column of VINs. Each VIN is decoded with NHTSA vPIC, 50 at a time.</p>
        <div className="row">
          <button className="btn" onClick={pick}>
            <FileSpreadsheet size={16} aria-hidden /> Choose file…
          </button>
          {table && (
            <>
              <Field label="VIN column">
                <select value={col} onChange={(e) => setCol(Number(e.target.value))}>
                  {table.headers.map((h, i) => (
                    <option key={i} value={i}>
                      {h || `Column ${i + 1}`}
                    </option>
                  ))}
                </select>
              </Field>
              <label className="check">
                <input type="checkbox" checked={save} onChange={(e) => setSave(e.target.checked)} /> Save decoded vehicles to My Vehicles
              </label>
              <button className="btn primary" disabled={!vins.length || (progress != null && progress.done < progress.total && !error)} onClick={run}>
                Decode {vins.length} VIN{vins.length === 1 ? "" : "s"}
              </button>
            </>
          )}
        </div>
        {progress && (
          <div>
            <ProgressBar value={progress.done} max={progress.total} label="Decoding progress" />
            <div className="row small muted mt-1">
              {progress.done} of {progress.total} · {count("decoded")} decoded · {count("not_identified")} not identified · {count("invalid")} invalid
              {progress.done < progress.total && !error && (
                <button className="btn link" onClick={() => (stop.current = true)}>
                  Stop
                </button>
              )}
            </div>
          </div>
        )}
      </div>
      {error != null && <ErrorNote error={error} />}
      {rows.length > 0 && (
        <div className="table-wrap">
          <table>
            <thead>
              <tr>
                <th>VIN</th>
                <th>Result</th>
                <th>Vehicle</th>
                <th>Note</th>
              </tr>
            </thead>
            <tbody>
              {rows.map((r) => (
                <tr key={r.vin}>
                  <td className="mono">{r.vin}</td>
                  <td>
                    <Badge tone={r.status === "decoded" ? "ok" : r.status === "invalid" ? "bad" : "warn"}>{r.status === "decoded" ? "Decoded" : r.status === "invalid" ? "Invalid" : "Not identified"}</Badge>
                  </td>
                  <td>
                    {r.vehicle_id ? (
                      <button className="btn link" style={{ padding: 0 }} onClick={() => go({ page: "vehicle", id: r.vehicle_id! })}>
                        {r.title}
                      </button>
                    ) : (
                      "—"
                    )}
                  </td>
                  <td className="small muted">{r.message}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
    </div>
  );
}
