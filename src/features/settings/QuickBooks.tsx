import { useEffect, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { FileSpreadsheet } from "lucide-react";
import { call, errorMessage } from "../../api/ipc";
import { useRefresh } from "../../api/queries";
import type { QbOptions, QbPreview, QbRow, QbSummary } from "../../api/types";
import { toast } from "../../store";
import { Badge, ErrorNote, Modal, Spinner } from "../../ui";
import { t, tm, tx } from "../../i18n";

const DEFAULTS: QbOptions = { use_prices: true, non_inventory: false, apply_stock: false, apply_cost: false };

const ENTITY: Record<string, string> = { item: t("Product"), customer: t("Customer"), vendor: t("Supplier") };
const STATUS: Record<string, { label: string; tone: "ok" | "info" | "warn" | "neutral" }> = {
  new: { label: t("Add"), tone: "ok" },
  update: { label: t("Update"), tone: "info" },
  unchanged: { label: t("No change"), tone: "neutral" },
  skipped: { label: t("Skip"), tone: "warn" },
};

function Options({ o, set }: { o: QbOptions; set: (o: QbOptions) => void }) {
  const box = (key: keyof QbOptions, label: string, hint: string) => (
    <label className="check">
      <input type="checkbox" checked={o[key]} onChange={(e) => set({ ...o, [key]: e.target.checked })} />
      <span>
        {label}
        <span className="hint">{hint}</span>
      </span>
    </label>
  );
  return (
    <div className="stack" style={{ gap: 6 }}>
      {box("use_prices", t("Use QuickBooks sales prices"), t("Otherwise your pricing rules set the price."))}
      {box("non_inventory", t("Include non-inventory products"), t("Added without stock."))}
      {box("apply_stock", t("Set stock to the QuickBooks quantity"), t("For items already here. Recorded as a count, so the history shows the change."))}
      {box("apply_cost", t("Set cost to the QuickBooks purchase cost"), t("For items already here."))}
    </div>
  );
}

/** Shows what an import would do, row by row, and applies it on request. */
export function QbReview({
  title,
  load,
  apply,
  onClose,
}: {
  title: string;
  load: (o: QbOptions) => Promise<QbPreview>;
  apply: (o: QbOptions) => Promise<QbSummary>;
  onClose: () => void;
}) {
  const refresh = useRefresh();
  const [o, setO] = useState<QbOptions>(DEFAULTS);
  const [preview, setPreview] = useState<QbPreview | null>(null);
  const [error, setError] = useState<unknown>(null);
  const [busy, setBusy] = useState(false);
  useEffect(() => {
    let live = true;
    setError(null);
    load(o)
      .then((p) => live && setPreview(p))
      .catch((e) => live && setError(e));
    return () => {
      live = false;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [o]);
  const go = async () => {
    setBusy(true);
    try {
      const s = await apply(o);
      toast(t("QuickBooks: {0} added, {1} updated, {2} skipped", s.added, s.updated, s.skipped));
      refresh();
      onClose();
    } catch (e) {
      toast(errorMessage(e), { tone: "bad" });
    } finally {
      setBusy(false);
    }
  };
  const changes = preview ? preview.new_count + preview.update_count : 0;
  return (
    <Modal
      open
      wide
      title={title}
      onClose={onClose}
      footer={
        <>
          <button className="btn" onClick={onClose}>{t("Cancel")}</button>
          <button className="btn primary" onClick={go} disabled={busy || !preview || changes === 0}>
            {busy && <Spinner />}{" "}{t("Apply changes")}</button>
        </>
      }
    >
      <div className="stack">
        <Options o={o} set={setO} />
        {error != null && <ErrorNote error={error} />}
        {!preview && error == null && <Spinner label={t("Reading QuickBooks data")} />}
        {preview && (
          <>
            <div className="row">
              <Badge tone="ok">{tx("{0} to add", preview.new_count)}</Badge>
              <Badge tone="info">{tx("{0} to update", preview.update_count)}</Badge>
              <Badge tone="neutral">{tx("{0} unchanged", preview.unchanged_count)}</Badge>
              <Badge tone="warn">{tx("{0} skipped", preview.skipped_count)}</Badge>
            </div>
            <div className="table-wrap" style={{ maxHeight: "46vh" }}>
              <table>
                <thead>
                  <tr>
                    <th>{t("Record")}</th>
                    <th>{t("Name")}</th>
                    <th>{t("What happens")}</th>
                  </tr>
                </thead>
                <tbody>
                  {preview.rows.map((r: QbRow, i) => (
                    <tr key={i}>
                      <td>{ENTITY[r.entity] ?? r.entity}</td>
                      <td>{r.name}</td>
                      <td>
                        <Badge tone={STATUS[r.status]?.tone ?? "neutral"}>{STATUS[r.status]?.label ?? r.status}</Badge>
                        {r.notes.map((n, j) => (
                          <div key={j} className="small muted">{tm(n)}</div>
                        ))}
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
            <p className="small muted">{t("QuickBooks isn't changed. Everything is applied in one step, so a failure leaves nothing half-done. Importing again later updates these same records.")}</p>
          </>
        )}
      </div>
    </Modal>
  );
}

const KIND: Record<string, string> = { items: t("products and services"), customers: t("customers"), vendors: t("vendors") };

function ImportExport() {
  const [path, setPath] = useState<string | null>(null);
  const [kind, setKind] = useState<string>("");
  const pick = async () => {
    const p = await open({ filters: [{ name: "QuickBooks export", extensions: ["xlsx", "xls", "csv"] }] });
    if (typeof p === "string") setPath(p);
  };
  return (
    <section className="card">
      <h2>{t("Import a QuickBooks export")}</h2>
      <p className="muted">{t("Works with QuickBooks Online and QuickBooks Desktop. Export a list to Excel, then choose the file here. Products, customers and vendors are each their own file.")}</p>
      <details className="mt-2">
        <summary>{t("How to export from QuickBooks")}</summary>
        <ul className="small">
          <li>{t("QuickBooks Online: Sales → Products and services → More → Export to Excel. Customers and vendors: open the list, then Export to Excel.")}</li>
          <li>{t("QuickBooks Desktop: Lists → Item List → Excel → Export all items. Customer Center or Vendor Center → Excel → Export Customer List or Vendor List.")}</li>
        </ul>
      </details>
      <div className="row mt-2">
        <button className="btn primary" onClick={pick}>
          <FileSpreadsheet size={16} aria-hidden />{" "}{t("Choose export…")}</button>
      </div>
      {path && (
        <QbReview
          title={kind ? t("Import QuickBooks {0}", KIND[kind] ?? kind) : t("Import from QuickBooks")}
          load={async (options) => {
            const r = await call<{ kind: string; preview: QbPreview }>("quickbooks_file_preview", { path, options });
            setKind(r.kind);
            return r.preview;
          }}
          apply={(options) => call<QbSummary>("quickbooks_file_apply", { path, options })}
          onClose={() => {
            setPath(null);
            setKind("");
          }}
        />
      )}
    </section>
  );
}

export function QuickBooks() {
  return (
    <div className="stack">
      <ImportExport />
    </div>
  );
}
