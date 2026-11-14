import { useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { FileSpreadsheet } from "lucide-react";
import { call, errorMessage } from "../../api/ipc";
import { useRefresh } from "../../api/queries";
import type { ImportSummary, InventoryMapping, InventoryPreview, Table } from "../../api/types";
import { toast } from "../../store";
import { Badge, ErrorNote, Modal, Spinner } from "../../ui";
import { t, tx } from "../../i18n";

const FIELDS: { key: keyof InventoryMapping; label: string; required?: boolean }[] = [
  { key: "name", label: t("Product name"), required: true },
  { key: "sku", label: t("SKU"), required: true },
  { key: "brand", label: t("Brand") },
  { key: "category", label: t("Category") },
  { key: "part_number", label: t("Part number") },
  { key: "oem_number", label: t("OEM number") },
  { key: "barcode", label: t("Barcode") },
  { key: "description", label: t("Description") },
  { key: "cost", label: t("Cost") },
  { key: "price", label: t("Selling price") },
  { key: "qty", label: t("Quantity on hand") },
  { key: "location", label: t("Location") },
  { key: "bin", label: t("Shelf / bin") },
  { key: "min_stock", label: t("Minimum stock") },
  { key: "reorder_qty", label: t("Reorder quantity") },
  { key: "supplier", label: t("Supplier") },
  { key: "condition", label: t("Condition") },
];

/** Spreadsheet import: choose a file, match its columns, review every row, then import. */
export function ImportInventory({ onClose }: { onClose: () => void }) {
  const refresh = useRefresh();
  const [path, setPath] = useState<string | null>(null);
  const [table, setTable] = useState<Table | null>(null);
  const [mapping, setMapping] = useState<InventoryMapping | null>(null);
  const [preview, setPreview] = useState<InventoryPreview | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<unknown>(null);
  const pick = async () => {
    const p = await open({ filters: [{ name: "Spreadsheet", extensions: ["csv", "xlsx", "xls", "ods"] }] });
    if (typeof p !== "string") return;
    setError(null);
    setPreview(null);
    try {
      const t = await call<Table>("table_read", { path: p });
      setPath(p);
      setTable(t);
      setMapping(await call<InventoryMapping>("table_guess_inventory", { headers: t.headers }));
    } catch (e) {
      setError(e);
    }
  };
  const review = async () => {
    setBusy(true);
    setError(null);
    try {
      setPreview(await call<InventoryPreview>("import_inventory_preview", { path, mapping }));
    } catch (e) {
      setError(e);
    } finally {
      setBusy(false);
    }
  };
  const commit = async () => {
    setBusy(true);
    try {
      const s = await call<ImportSummary>("import_inventory_commit", { path, mapping });
      toast(t("Imported {0} items{1}", s.created, s.skipped ? `, skipped ${s.skipped}` : ""));
      refresh();
      onClose();
    } catch (e) {
      toast(errorMessage(e), { tone: "bad" });
    } finally {
      setBusy(false);
    }
  };
  return (
    <Modal
      open
      wide
      title={t("Import inventory")}
      onClose={onClose}
      footer={
        <>
          <button className="btn" onClick={onClose}>{t("Cancel")}</button>
          {mapping && !preview && (
            <button className="btn primary" onClick={review} disabled={busy}>
              {busy && <Spinner />}{" "}{t("Review rows")}</button>
          )}
          {preview && (
            <>
              <button className="btn" onClick={() => setPreview(null)}>{t("Change columns")}</button>
              <button className="btn primary" onClick={commit} disabled={busy || preview.new_count === 0}>
                {busy && <Spinner />}{" "}{t("Import")}{" "}{preview.new_count}{" "}{t("items")}</button>
            </>
          )}
        </>
      }
    >
      {!table && (
        <div className="stack">
          <p className="muted">{t("A CSV or Excel sheet with one product per row and a header row. Nothing is saved until you've reviewed every row.")}</p>
          <button className="btn primary" onClick={pick} style={{ alignSelf: "flex-start" }}>
            <FileSpreadsheet size={16} aria-hidden />{" "}{t("Choose file…")}</button>
        </div>
      )}
      {error != null && <ErrorNote error={error} />}
      {table && mapping && !preview && (
        <div className="stack">
          <p className="muted">{tx("{0} rows. Match each field to a column of your file (guessed from the headers).", table.rows.length)}</p>
          <div className="grid" style={{ gridTemplateColumns: "repeat(auto-fit, minmax(240px, 1fr))" }}>
            {FIELDS.map((f) => (
              <label key={f.key} className="field">
                <span>
                  {f.label}
                  {f.required && " *"}
                </span>
                <select value={mapping[f.key] ?? ""} onChange={(e) => setMapping({ ...mapping, [f.key]: e.target.value === "" ? null : Number(e.target.value) })}>
                  <option value="">{t("— Not in the file —")}</option>
                  {table.headers.map((h, i) => (
                    <option key={i} value={i}>
                      {h || t("Column {0}", i + 1)} {table.rows[0]?.[i] ? `(e.g. ${table.rows[0][i].slice(0, 24)})` : ""}
                    </option>
                  ))}
                </select>
              </label>
            ))}
          </div>
        </div>
      )}
      {preview && (
        <div className="stack">
          <div className="row">
            <Badge tone="ok">{tx("{0} to import", preview.new_count)}</Badge>
            <Badge tone="warn">{tx("{0} duplicates skipped", preview.duplicate_count)}</Badge>
            <Badge tone="bad">{tx("{0} with problems", preview.error_count)}</Badge>
          </div>
          <div className="table-wrap" style={{ maxHeight: "50vh" }}>
            <table>
              <thead>
                <tr>
                  <th>{t("Row")}</th>
                  <th>{t("Status")}</th>
                  <th>{t("SKU")}</th>
                  <th>{t("Product")}</th>
                  <th>{t("Category")}</th>
                  <th>{t("Qty")}</th>
                  <th>{t("Cost")}</th>
                </tr>
              </thead>
              <tbody>
                {preview.rows.map((r) => (
                  <tr key={r.line}>
                    <td>{r.line}</td>
                    <td>
                      <Badge tone={r.status === "new" ? "ok" : r.status === "duplicate" ? "warn" : "bad"}>{r.status === "new" ? t("Import") : r.status === "duplicate" ? t("Skip") : t("Problem")}</Badge>
                      {r.message && <div className="small muted">{r.message}</div>}
                    </td>
                    <td className="mono">{r.sku}</td>
                    <td>{r.name}</td>
                    <td>{r.category || "—"}</td>
                    <td>{r.qty ?? ""}</td>
                    <td>{r.cost ?? ""}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
          <p className="small muted">{t("Rows with problems aren't imported; fix them in the file and import again. Everything is imported in one step, so a failure leaves nothing half-done.")}</p>
        </div>
      )}
    </Modal>
  );
}
