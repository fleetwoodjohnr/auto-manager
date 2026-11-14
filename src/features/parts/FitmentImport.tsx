import { useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { FileSpreadsheet } from "lucide-react";
import { call, errorMessage } from "../../api/ipc";
import { useCmd, useRefresh } from "../../api/queries";
import type { AcesSummary, Category, FitmentPreview, ImportSummary, PiesSummary } from "../../api/types";
import { toast } from "../../store";
import { Badge, ErrorNote, Field, Modal, Spinner, Tabs } from "../../ui";

/** Catalog imports: our fitment spreadsheet, PIES product data, ACES applications. */
export function FitmentImport({ onClose }: { onClose: () => void }) {
  const [tab, setTab] = useState("csv");
  return (
    <Modal open wide title="Import catalog data" onClose={onClose}>
      <Tabs
        label="Import type"
        value={tab}
        onChange={setTab}
        tabs={[
          { id: "csv", label: "Fitment spreadsheet" },
          { id: "pies", label: "PIES (products)" },
          { id: "aces", label: "ACES (fitment)" },
        ]}
      />
      {tab === "csv" && <CsvFitment onDone={onClose} />}
      {tab === "pies" && <Pies onDone={onClose} />}
      {tab === "aces" && <Aces onDone={onClose} />}
    </Modal>
  );
}

const pickFile = async (exts: string[], name: string) => {
  const p = await open({ filters: [{ name, extensions: exts }] });
  return typeof p === "string" ? p : null;
};

function CsvFitment({ onDone }: { onDone: () => void }) {
  const refresh = useRefresh();
  const [path, setPath] = useState<string | null>(null);
  const [pv, setPv] = useState<FitmentPreview | null>(null);
  const [auth, setAuth] = useState(false);
  const [error, setError] = useState<unknown>(null);
  const [busy, setBusy] = useState(false);
  const choose = async () => {
    const p = await pickFile(["csv", "xlsx", "xls", "ods"], "Spreadsheet");
    if (!p) return;
    setError(null);
    setPath(p);
    try {
      setPv(await call<FitmentPreview>("import_fitment_preview", { path: p }));
    } catch (e) {
      setError(e);
      setPv(null);
    }
  };
  const commit = async () => {
    setBusy(true);
    try {
      const s = await call<ImportSummary>("import_fitment_commit", { path, authoritative: auth });
      toast(`Imported ${s.created} fitment records`);
      refresh();
      onDone();
    } catch (e) {
      setError(e);
    } finally {
      setBusy(false);
    }
  };
  return (
    <div className="stack">
      <p className="muted small">
        Columns: <span className="mono">part</span> (SKU or part number), <span className="mono">kind</span> (fits / excludes), <span className="mono">year_from</span>, <span className="mono">year_to</span>, <span className="mono">make</span>,{" "}
        <span className="mono">model</span>, optional <span className="mono">position</span>, <span className="mono">quantity</span>, <span className="mono">notes</span>. Any other column becomes a condition named after its header (e.g.{" "}
        <span className="mono">engine_displacement_l</span>, <span className="mono">drive_type</span>, <span className="mono">brake_package</span>).
      </p>
      <button className="btn" onClick={choose} style={{ alignSelf: "flex-start" }}>
        <FileSpreadsheet size={16} aria-hidden /> Choose file…
      </button>
      {error != null && <ErrorNote error={error} />}
      {pv && (
        <>
          <div className="row">
            <Badge tone="ok">{pv.ok_count} ready</Badge>
            <Badge tone="bad">{pv.error_count} with problems</Badge>
            {pv.qualifier_keys.length > 0 && <span className="small muted">Conditions: {pv.qualifier_keys.join(", ")}</span>}
          </div>
          <div className="table-wrap" style={{ maxHeight: 300 }}>
            <table>
              <thead>
                <tr>
                  <th>Row</th>
                  <th>Part</th>
                  <th>Vehicle</th>
                  <th>Status</th>
                </tr>
              </thead>
              <tbody>
                {pv.rows.map((r) => (
                  <tr key={r.line}>
                    <td>{r.line}</td>
                    <td className="mono">{r.part}</td>
                    <td>
                      {r.kind === "excludes" && "NOT "}
                      {r.year_from}–{r.year_to} {r.make} {r.model} {r.qualifiers.map((q) => `${q.key}=${q.value}`).join(" ")}
                    </td>
                    <td>{r.status === "ok" ? <Badge tone="ok">OK</Badge> : <span className="small" style={{ color: "var(--danger)" }}>{r.message}</span>}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
          <label className="check">
            <input type="checkbox" checked={auth} onChange={(e) => setAuth(e.target.checked)} /> This file comes from an authoritative catalog I trust (marks the records as verified)
          </label>
          <button className="btn primary" style={{ alignSelf: "flex-start" }} disabled={busy || pv.ok_count === 0} onClick={commit}>
            {busy && <Spinner />} Import {pv.ok_count} records
          </button>
        </>
      )}
    </div>
  );
}

function Pies({ onDone }: { onDone: () => void }) {
  const refresh = useRefresh();
  const cats = useCmd<Category[]>("categories");
  const [cat, setCat] = useState("");
  const [busy, setBusy] = useState(false);
  const run = async () => {
    const p = await pickFile(["xml"], "PIES XML");
    if (!p) return;
    setBusy(true);
    try {
      const s = await call<PiesSummary>("import_pies", { path: p, categoryId: cat || null });
      toast(`PIES: ${s.created} parts added, ${s.updated} updated`);
      refresh();
      onDone();
    } catch (e) {
      toast(errorMessage(e), { tone: "bad" });
    } finally {
      setBusy(false);
    }
  };
  return (
    <div className="stack">
      <p className="muted small">PIES XML from a supplier or brand creates catalog parts with their names, descriptions, attributes, barcodes, list prices and interchange numbers. Parts already present (same number and brand) are updated.</p>
      <Field label="Put new parts in category (optional)">
        <select value={cat} onChange={(e) => setCat(e.target.value)}>
          <option value="">— Leave uncategorised —</option>
          {cats.data?.map((c) => (
            <option key={c.id} value={c.id}>
              {c.parent_id ? "  " : ""}
              {c.name}
            </option>
          ))}
        </select>
      </Field>
      <button className="btn primary" style={{ alignSelf: "flex-start" }} onClick={run} disabled={busy}>
        {busy ? <Spinner /> : <FileSpreadsheet size={16} aria-hidden />} Choose PIES file…
      </button>
    </div>
  );
}

function Aces({ onDone }: { onDone: () => void }) {
  const refresh = useRefresh();
  const [aces, setAces] = useState<string | null>(null);
  const [vmap, setVmap] = useState<string | null>(null);
  const [amap, setAmap] = useState<string | null>(null);
  const [auth, setAuth] = useState(true);
  const [busy, setBusy] = useState(false);
  const [sum, setSum] = useState<AcesSummary | null>(null);
  const run = async () => {
    setBusy(true);
    try {
      const s = await call<AcesSummary>("import_aces", { path: aces, vehicleMap: vmap, attributeMap: amap, authoritative: auth });
      setSum(s);
      refresh();
    } catch (e) {
      toast(errorMessage(e), { tone: "bad" });
    } finally {
      setBusy(false);
    }
  };
  const fileRow = (label: string, v: string | null, set: (p: string | null) => void, exts: string[]) => (
    <div className="row">
      <button className="btn" onClick={async () => set(await pickFile(exts, label))}>
        {label}…
      </button>
      <span className="small muted truncate" style={{ maxWidth: 420 }}>
        {v ?? "not chosen"}
      </span>
    </div>
  );
  return (
    <div className="stack">
      <p className="muted small">
        ACES applications name vehicles by VCdb ids, which are licensed reference data that Auto Manager doesn't include. Supply a <strong>vehicle map</strong> exported from your licensed VCdb (CSV with BaseVehicleID, Year, Make, Model) and, optionally, an{" "}
        <strong>attribute map</strong> (CSV: attribute, id, key, value; e.g. <span className="mono">EngineBase,1234,engine_displacement_l,2.0</span>). Applications that can't be resolved are reported and skipped. Import the parts (PIES) first.
      </p>
      {fileRow("ACES XML", aces, setAces, ["xml"])}
      {fileRow("Vehicle map (CSV)", vmap, setVmap, ["csv", "xlsx"])}
      {fileRow("Attribute map (optional)", amap, setAmap, ["csv", "xlsx"])}
      <label className="check">
        <input type="checkbox" checked={auth} onChange={(e) => setAuth(e.target.checked)} /> This ACES file comes from the brand or a licensed catalog (marks records as verified)
      </label>
      <button className="btn primary" style={{ alignSelf: "flex-start" }} disabled={!aces || !vmap || busy} onClick={run}>
        {busy && <Spinner />} Import
      </button>
      {sum && (
        <div className="notice">
          <strong>{sum.imported}</strong> of {sum.apps} applications imported.
          {sum.unresolved_vehicles.length > 0 && <div className="small">{sum.unresolved_vehicles.length} with a BaseVehicle not in your vehicle map (skipped).</div>}
          {sum.unknown_parts.length > 0 && <div className="small">Part numbers not in the catalog (skipped): {sum.unknown_parts.slice(0, 20).join(", ")}{sum.unknown_parts.length > 20 ? "…" : ""}</div>}
          {sum.deletes_skipped > 0 && <div className="small">{sum.deletes_skipped} delete actions ignored (remove records by hand).</div>}
          {sum.unmapped_attributes.length > 0 && <div className="small">Unmapped attributes kept as conditions vehicles can't meet yet: {sum.unmapped_attributes.slice(0, 15).join(", ")}</div>}
          <button className="btn mt-2" onClick={onDone}>
            Done
          </button>
        </div>
      )}
    </div>
  );
}
