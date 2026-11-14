import { useState } from "react";
import { Cable, RefreshCw } from "lucide-react";
import { call, errorMessage } from "../../api/ipc";
import { useCmd } from "../../api/queries";
import type { ObdSession, Vehicle } from "../../api/types";
import { Badge, Spinner } from "../../ui";
import { dateTime } from "../../lib/format";
import { vehicleTitle } from "../vehicles/Garage";
import { TroubleCodes } from "./TroubleCodes";
import { t, tx } from "../../i18n";

/** Read-only OBD-II scan through an ELM327-compatible adapter. */
export function ObdScanner() {
  const ports = useCmd<string[]>("obd_ports");
  const vehicles = useCmd<Vehicle[]>("vehicle_list", { saved: true });
  const sessions = useCmd<ObdSession[]>("obd_sessions", { vehicleId: null });
  const [port, setPort] = useState("");
  const [vid, setVid] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [code, setCode] = useState<string | null>(null);
  const scan = async () => {
    setBusy(true);
    setError(null);
    try {
      await call<ObdSession>("obd_scan", { port, vehicleId: vid || null });
      void sessions.refetch();
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      setBusy(false);
    }
  };
  return (
    <div className="stack">
      <div className="notice info">{t("Connect an ELM327-compatible adapter (USB, or Bluetooth paired so it appears as a serial port), turn the ignition on, then scan. Auto Manager only")}{" "}<strong>reads</strong>{t(": trouble codes, freeze-frame data, live values and the VIN. It never clears codes or sends commands that change anything in the vehicle. Tested against a simulated adapter; results with real adapters vary by adapter and vehicle.")}</div>
      <div className="row">
        <select value={port} onChange={(e) => setPort(e.target.value)} aria-label={t("Adapter port")}>
          <option value="">{ports.data?.length ? t("Choose the adapter's port…") : t("No serial ports found")}</option>
          {ports.data?.map((p) => (
            <option key={p} value={p}>
              {p}
            </option>
          ))}
        </select>
        <button className="icon-btn" aria-label={t("Look for ports again")} title={t("Look for ports again")} onClick={() => ports.refetch()}>
          <RefreshCw size={15} aria-hidden />
        </button>
        <select value={vid} onChange={(e) => setVid(e.target.value)} aria-label={t("Vehicle")}>
          <option value="">{t("Vehicle (optional)")}</option>
          {vehicles.data?.map((v) => (
            <option key={v.id} value={v.id}>
              {vehicleTitle(v)}
            </option>
          ))}
        </select>
        <button className="btn primary" disabled={!port || busy} onClick={scan}>
          {busy ? <Spinner /> : <Cable size={16} aria-hidden />}{" "}{t("Scan")}</button>
      </div>
      {error && <div className="notice bad">{error}</div>}
      {sessions.data?.map((s) => {
        const of = (k: string) => s.readings.filter((r) => r.kind === k);
        const codes = [...of("stored_dtc"), ...of("pending_dtc")];
        return (
          <section key={s.id} className="card stack" style={{ gap: 8 }}>
            <div className="spread">
              <strong>{dateTime(s.started_at)}</strong>
              <span className="small muted">
                {s.adapter} · {s.protocol} {s.vin_read && <>{t("· VIN")}{" "}<span className="mono">{s.vin_read}</span></>}
              </span>
            </div>
            <div className="row">
              {of("readiness").map((r) => (
                <Badge key={r.code} tone={r.value === "on" ? "bad" : "ok"}>{tx("Check-engine lamp {0}", r.value)}</Badge>
              ))}
              {codes.length === 0 ? (
                <Badge tone="ok">{t("No trouble codes")}</Badge>
              ) : (
                codes.map((c) => (
                  <button key={c.kind + c.code} className="btn" onClick={() => setCode(c.code)}>
                    <span className="mono">{c.code}</span> {c.kind === "pending_dtc" && <Badge tone="warn">pending</Badge>}
                  </button>
                ))
              )}
            </div>
            {of("pid").length > 0 && (
              <dl className="kv">
                {of("pid").map((r) => (
                  <div key={r.code} style={{ display: "contents" }}>
                    <dt>{r.code}</dt>
                    <dd>
                      {r.value} {r.unit}
                    </dd>
                  </div>
                ))}
              </dl>
            )}
            {of("freeze_frame").length > 0 && (
              <details>
                <summary>{t("Freeze frame")}</summary>
                <dl className="kv mt-1">
                  {of("freeze_frame").map((r) => (
                    <div key={r.code} style={{ display: "contents" }}>
                      <dt>{r.code}</dt>
                      <dd>
                        {r.value} {r.unit}
                      </dd>
                    </div>
                  ))}
                </dl>
              </details>
            )}
          </section>
        );
      })}
      {code && (
        <section className="card">
          <div className="spread">
            <h2>{tx("Code {0}", code)}</h2>
            <button className="btn link" onClick={() => setCode(null)}>{t("Close")}</button>
          </div>
          <TroubleCodes key={code} initial={code} />
        </section>
      )}
    </div>
  );
}
