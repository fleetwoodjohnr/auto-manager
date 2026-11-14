import { useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { FileSpreadsheet } from "lucide-react";
import { call, errorMessage } from "../../api/ipc";
import { useRefresh } from "../../api/queries";
import type { ImportSummary, InventoryMapping, InventoryPreview, Table } from "../../api/types";
import { toast } from "../../store";
import { Badge, ErrorNote, Modal, Spinner } from "../../ui";

const FIELDS: { key: keyof InventoryMapping; label: string; required?: boolean }[] = [
  { key: "name", label: "Product name", required: true },
  { key: "sku", label: "SKU", required: true },
  { key: "brand", label: "Brand" },
  { key: "category", label: "Category" },
  { key: "part_number", label: "Part number" },
  { key: "oem_number", label: "OEM number" },
  { key: "barcode", label: "Barcode" },
  { key: "description", label: "Description" },
  { key: "cost", label: "Cost" },
  { key: "price", label: "Selling price" },
  { key: "qty", label: "Quantity on hand" },
  { key: "location", label: "Location" },
  { key: "bin", label: "Shelf / bin" },
  { key: "min_stock", label: "Minimum stock" },
  { key: "reorder_qty", label: "Reorder quantity" },
  { key: "supplier", label: "Supplier" },
  { key: "condition", label: "Condition" },
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
      toast(`Imported ${s.created} items${s.skipped ? `, skipped ${s.skipped}` : ""}`);
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
      title="Import inventory"
      onClose={onClose}
      footer={
        <>
          <button className="btn" onClick={onClose}>
            Cancel
          </button>
          {mapping && !preview && (
            <button className="btn primary" onClick={review} disabled={busy}>
              {busy && <Spinner />} Review rows
            </button>
          )}
          {preview && (
            <>
              <button className="btn" onClick={() => setPreview(null)}>
                Change columns
              </button>
              <button className="btn primary" onClick={commit} disabled={busy || preview.new_count === 0}>
                {busy && <Spinner />} Import {preview.new_count} items
              </button>
            </>
          )}
        </>
      }
    >
      {!table && (
        <div className="stack">
          <p className="muted">A CSV or Excel sheet with one product per row and a header row. Nothing is saved until you've reviewed every row.</p>
          <button className="btn primary" onClick={pick} style={{ alignSelf: "flex-start" }}>
            <FileSpreadsheet size={16} aria-hidden /> Choose file…
          </button>
        </div>
      )}
      {error != null && <ErrorNote error={error} />}
      {table && mapping && !preview && (
        <div className="stack">
          <p className="muted">
            {table.rows.length} rows. Match each field to a column of your file (guessed from the headers).
          </p>
          <div className="grid" style={{ gridTemplateColumns: "repeat(auto-fit, minmax(240px, 1fr))" }}>
            {FIELDS.map((f) => (
              <label key={f.key} className="field">
                <span>
                  {f.label}
                  {f.required && " *"}
                </span>
                <select value={mapping[f.key] ?? ""} onChange={(e) => setMapping({ ...mapping, [f.key]: e.target.value === "" ? null : Number(e.target.value) })}>
                  <option value="">— Not in the file —</option>
                  {table.headers.map((h, i) => (
                    <option key={i} value={i}>
                      {h || `Column ${i + 1}`} {table.rows[0]?.[i] ? `(e.g. ${table.rows[0][i].slice(0, 24)})` : ""}
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
            <Badge tone="ok">{preview.new_count} to import</Badge>
            <Badge tone="warn">{preview.duplicate_count} duplicates skipped</Badge>
            <Badge tone="bad">{preview.error_count} with problems</Badge>
          </div>
          <div className="table-wrap" style={{ maxHeight: "50vh" }}>
            <table>
              <thead>
                <tr>
                  <th>Row</th>
                  <th>Status</th>
                  <th>SKU</th>
                  <th>Product</th>
                  <th>Category</th>
                  <th>Qty</th>
                  <th>Cost</th>
                </tr>
              </thead>
              <tbody>
                {preview.rows.map((r) => (
                  <tr key={r.line}>
                    <td>{r.line}</td>
                    <td>
                      <Badge tone={r.status === "new" ? "ok" : r.status === "duplicate" ? "warn" : "bad"}>{r.status === "new" ? "Import" : r.status === "duplicate" ? "Skip" : "Problem"}</Badge>
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
          <p className="small muted">Rows with problems aren't imported; fix them in the file and import again. Everything is imported in one step, so a failure leaves nothing half-done.</p>
        </div>
      )}
    </Modal>
  );
}
