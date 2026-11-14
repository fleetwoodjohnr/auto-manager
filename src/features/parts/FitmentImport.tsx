import { useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { FileSpreadsheet } from "lucide-react";
import { call, errorMessage } from "../../api/ipc";
import { useCmd, useRefresh } from "../../api/queries";
import type { AcesSummary, Category, FitmentPreview, ImportSummary, PiesSummary } from "../../api/types";
import { toast } from "../../store";
import { Badge, ErrorNote, Field, Modal, Spinner, Tabs } from "../../ui";
import { t, tx } from "../../i18n";

/** Catalog imports: our fitment spreadsheet, PIES product data, ACES applications. */
export function FitmentImport({ onClose }: { onClose: () => void }) {
  const [tab, setTab] = useState("csv");
  return (
    <Modal open wide title={t("Import catalog data")} onClose={onClose}>
      <Tabs
        label={t("Import type")}
        value={tab}
        onChange={setTab}
        tabs={[
          { id: "csv", label: t("Fitment spreadsheet") },
          { id: "pies", label: t("PIES (products)") },
          { id: "aces", label: t("ACES (fitment)") },
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
      toast(t("Imported {0} fitment records", s.created));
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
      <p className="muted small">{t("Columns:")}{" "}<span className="mono">part</span>{" "}{t("(SKU or part number),")}{" "}<span className="mono">kind</span>{" "}{t("(fits / excludes),")}{" "}<span className="mono">year_from</span>, <span className="mono">year_to</span>, <span className="mono">make</span>,{" "}
        <span className="mono">model</span>{t(", optional")}{" "}<span className="mono">position</span>, <span className="mono">quantity</span>, <span className="mono">notes</span>{t(". Any other column becomes a condition named after its header (e.g.")}{" "}
        <span className="mono">engine_displacement_l</span>, <span className="mono">drive_type</span>, <span className="mono">brake_package</span>).
      </p>
      <button className="btn" onClick={choose} style={{ alignSelf: "flex-start" }}>
        <FileSpreadsheet size={16} aria-hidden />{" "}{t("Choose file…")}</button>
      {error != null && <ErrorNote error={error} />}
      {pv && (
        <>
          <div className="row">
            <Badge tone="ok">{tx("{0} ready", pv.ok_count)}</Badge>
            <Badge tone="bad">{tx("{0} with problems", pv.error_count)}</Badge>
            {pv.qualifier_keys.length > 0 && <span className="small muted">{tx("Conditions: {0}", pv.qualifier_keys.join(", "))}</span>}
          </div>
          <div className="table-wrap" style={{ maxHeight: 300 }}>
            <table>
              <thead>
                <tr>
                  <th>{t("Row")}</th>
                  <th>{t("Part")}</th>
                  <th>{t("Vehicle")}</th>
                  <th>{t("Status")}</th>
                </tr>
              </thead>
              <tbody>
                {pv.rows.map((r) => (
                  <tr key={r.line}>
                    <td>{r.line}</td>
                    <td className="mono">{r.part}</td>
                    <td>
                      {r.kind === "excludes" && t("NOT ")}
                      {r.year_from}–{r.year_to} {r.make} {r.model} {r.qualifiers.map((q) => `${q.key}=${q.value}`).join(" ")}
                    </td>
                    <td>{r.status === "ok" ? <Badge tone="ok">{t("OK")}</Badge> : <span className="small" style={{ color: "var(--danger)" }}>{r.message}</span>}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
          <label className="check">
            <input type="checkbox" checked={auth} onChange={(e) => setAuth(e.target.checked)} />{" "}{t("This file comes from an authoritative catalog I trust (marks the records as verified)")}</label>
          <button className="btn primary" style={{ alignSelf: "flex-start" }} disabled={busy || pv.ok_count === 0} onClick={commit}>
            {busy && <Spinner />}{" "}{t("Import")}{" "}{pv.ok_count}{" "}{t("records")}</button>
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
    const p = await pickFile(["xml"], t("PIES XML"));
    if (!p) return;
    setBusy(true);
    try {
      const s = await call<PiesSummary>("import_pies", { path: p, categoryId: cat || null });
      toast(t("PIES: {0} parts added, {1} updated", s.created, s.updated));
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
      <p className="muted small">{t("PIES XML from a supplier or brand creates catalog parts with their names, descriptions, attributes, barcodes, list prices and interchange numbers. Parts already present (same number and brand) are updated.")}</p>
      <Field label={t("Put new parts in category (optional)")}>
        <select value={cat} onChange={(e) => setCat(e.target.value)}>
          <option value="">{t("— Leave uncategorised —")}</option>
          {cats.data?.map((c) => (
            <option key={c.id} value={c.id}>
              {c.parent_id ? "  " : ""}
              {c.name}
            </option>
          ))}
        </select>
      </Field>
      <button className="btn primary" style={{ alignSelf: "flex-start" }} onClick={run} disabled={busy}>
        {busy ? <Spinner /> : <FileSpreadsheet size={16} aria-hidden />}{" "}{t("Choose PIES file…")}</button>
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
        {v ?? t("not chosen")}
      </span>
    </div>
  );
  return (
    <div className="stack">
      <p className="muted small">{t("ACES applications name vehicles by VCdb ids, which are licensed reference data that Auto Manager doesn't include. Supply a")}{" "}<strong>{t("vehicle map")}</strong>{" "}{t("exported from your licensed VCdb (CSV with BaseVehicleID, Year, Make, Model) and, optionally, an")}{" "}
        <strong>{t("attribute map")}</strong>{" "}{t("(CSV: attribute, id, key, value; e.g.")}{" "}<span className="mono">{t("EngineBase,1234,engine_displacement_l,2.0")}</span>{t("). Applications that can't be resolved are reported and skipped. Import the parts (PIES) first.")}</p>
      {fileRow(t("ACES XML"), aces, setAces, ["xml"])}
      {fileRow(t("Vehicle map (CSV)"), vmap, setVmap, ["csv", "xlsx"])}
      {fileRow(t("Attribute map (optional)"), amap, setAmap, ["csv", "xlsx"])}
      <label className="check">
        <input type="checkbox" checked={auth} onChange={(e) => setAuth(e.target.checked)} />{" "}{t("This ACES file comes from the brand or a licensed catalog (marks records as verified)")}</label>
      <button className="btn primary" style={{ alignSelf: "flex-start" }} disabled={!aces || !vmap || busy} onClick={run}>
        {busy && <Spinner />}{" "}{t("Import")}</button>
      {sum && (
        <div className="notice">
          <strong>{sum.imported}</strong>{" "}{t("of")}{" "}{sum.apps}{" "}{t("applications imported.")}{sum.unresolved_vehicles.length > 0 && <div className="small">{tx("{0} with a BaseVehicle not in your vehicle map (skipped).", sum.unresolved_vehicles.length)}</div>}
          {sum.unknown_parts.length > 0 && <div className="small">{tx("Part numbers not in the catalog (skipped): {0}{1}", sum.unknown_parts.slice(0, 20).join(", "), sum.unknown_parts.length > 20 ? "…" : "")}</div>}
          {sum.deletes_skipped > 0 && <div className="small">{tx("{0} delete actions ignored (remove records by hand).", sum.deletes_skipped)}</div>}
          {sum.unmapped_attributes.length > 0 && <div className="small">{tx("Unmapped attributes kept as conditions vehicles can't meet yet: {0}", sum.unmapped_attributes.slice(0, 15).join(", "))}</div>}
          <button className="btn mt-2" onClick={onDone}>{t("Done")}</button>
        </div>
      )}
    </div>
  );
}
