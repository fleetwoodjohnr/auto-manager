import { useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { FileUp } from "lucide-react";
import { call, errorField, errorMessage } from "../../api/ipc";
import { useRefresh } from "../../api/queries";
import type { Manual, ManualLink, ManualMeta, Vehicle } from "../../api/types";
import { toast, useApp } from "../../store";
import { Field, Modal, Spinner } from "../../ui";
import { indexManual } from "../../lib/pdf";
import { t } from "../../i18n";

export const KIND_LABEL: Record<string, string> = {
  owner: t("Owner's manual"),
  maintenance: t("Maintenance schedule"),
  service: t("Service manual"),
  repair: t("Repair manual"),
  tsb: t("Technical service bulletin"),
  wiring: t("Wiring diagram"),
  fuse: t("Fuse box diagram"),
  fluids: t("Fluid specifications"),
  procedure: t("Maintenance procedure"),
  component_location: t("Component locations"),
  parts_diagram: t("Parts diagram"),
  troubleshooting: t("Troubleshooting guide"),
  installation: t("Installation instructions"),
  other: "Other",
};

/** Make a document searchable in the background, with a toast when done. */
export function indexInBackground(m: Manual, refresh: () => void) {
  void indexManual(m.id)
    .then((pages) => {
      toast(t("“{0}” is searchable ({1} page{2})", m.title, pages, pages === 1 ? "" : "s"));
      refresh();
    })
    .catch((e) => toast(t("Couldn't read the text of “{0}”: {1}. It can still be viewed.", m.title, errorMessage(e)), { tone: "bad" }));
}

export function ImportManual({ vehicle, onClose }: { vehicle?: Vehicle; onClose: () => void }) {
  const go = useApp((s) => s.go);
  const refresh = useRefresh();
  const [path, setPath] = useState<string | null>(null);
  const [meta, setMeta] = useState<ManualMeta>({ title: "", kind: "owner", publisher: "", source_url: "", license_note: "" });
  const [link, setLink] = useState<"vehicle" | "model" | "none">(vehicle ? "vehicle" : "none");
  const [model, setModel] = useState({ make: vehicle?.make ?? "", model: vehicle?.model ?? "", from: vehicle?.year?.toString() ?? "", to: vehicle?.year?.toString() ?? "" });
  const [ok, setOk] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<unknown>(null);
  const pick = async () => {
    const p = await open({ filters: [{ name: "PDF", extensions: ["pdf"] }] });
    if (typeof p === "string") {
      setPath(p);
      if (!meta.title) setMeta({ ...meta, title: p.split(/[\\/]/).pop()!.replace(/\.pdf$/i, "").replace(/[_-]+/g, " ") });
    }
  };
  const submit = async () => {
    setBusy(true);
    setError(null);
    const links: ManualLink[] =
      link === "vehicle" && vehicle
        ? [{ vehicle_id: vehicle.id, vehicle_title: null, make: null, model: null, year_from: null, year_to: null }]
        : link === "model"
          ? [{ vehicle_id: null, vehicle_title: null, make: model.make, model: model.model || null, year_from: model.from ? Number(model.from) : null, year_to: model.to ? Number(model.to) : null }]
          : [];
    try {
      const m = await call<Manual>("manual_import", { path, meta, links });
      refresh();
      onClose();
      toast(t("Imported “{0}”. Making it searchable…", m.title), { action: { label: t("Open"), run: () => go({ page: "manual", id: m.id }) } });
      indexInBackground(m, refresh);
    } catch (e) {
      setError(e);
    } finally {
      setBusy(false);
    }
  };
  const fe = (f: string) => (error && errorField(error) === f ? errorMessage(error) : null);
  return (
    <Modal
      open
      title={t("Import a document")}
      onClose={onClose}
      footer={
        <>
          <button className="btn" onClick={onClose}>{t("Cancel")}</button>
          <button className="btn primary" disabled={!path || !ok || busy} onClick={submit}>
            {busy && <Spinner />}{" "}{t("Import")}</button>
        </>
      }
    >
      <div className="row">
        <button className="btn" onClick={pick}>
          <FileUp size={16} aria-hidden />{" "}{t("Choose PDF…")}</button>
        <span className="small muted truncate" style={{ maxWidth: 360 }}>
          {path ?? t("No file chosen")}
        </span>
      </div>
      <Field label={t("Title")} error={fe("title")}>
        <input type="text" value={meta.title} onChange={(e) => setMeta({ ...meta, title: e.target.value })} />
      </Field>
      <div className="grid two">
        <Field label={t("Type")} error={fe("kind")}>
          <select value={meta.kind} onChange={(e) => setMeta({ ...meta, kind: e.target.value })}>
            {Object.entries(KIND_LABEL).map(([k, l]) => (
              <option key={k} value={k}>
                {l}
              </option>
            ))}
          </select>
        </Field>
        <Field label={t("Publisher")}>
          <input type="text" value={meta.publisher} onChange={(e) => setMeta({ ...meta, publisher: e.target.value })} placeholder={t("e.g. Nissan North America")} />
        </Field>
      </div>
      <Field label={t("Where it came from (link)")} error={fe("source_url")}>
        <input type="url" value={meta.source_url} onChange={(e) => setMeta({ ...meta, source_url: e.target.value })} placeholder="https://" />
      </Field>
      <fieldset>
        <legend className="label">{t("Applies to")}</legend>
        <div className="segmented" role="radiogroup" aria-label={t("Applies to")}>
          {vehicle && (
            <label>
              <input type="radio" checked={link === "vehicle"} onChange={() => setLink("vehicle")} />{" "}{t("This vehicle")}</label>
          )}
          <label>
            <input type="radio" checked={link === "model"} onChange={() => setLink("model")} />{" "}{t("A make and model")}</label>
          <label>
            <input type="radio" checked={link === "none"} onChange={() => setLink("none")} />{" "}{t("Nothing yet")}</label>
        </div>
        {link === "model" && (
          <div className="grid mt-2" style={{ gridTemplateColumns: "1fr 1fr 100px 100px" }}>
            <Field label={t("Make")}>
              <input type="text" value={model.make} onChange={(e) => setModel({ ...model, make: e.target.value })} />
            </Field>
            <Field label={t("Model (optional)")}>
              <input type="text" value={model.model} onChange={(e) => setModel({ ...model, model: e.target.value })} />
            </Field>
            <Field label={t("From year")}>
              <input type="number" value={model.from} onChange={(e) => setModel({ ...model, from: e.target.value })} />
            </Field>
            <Field label={t("To year")}>
              <input type="number" value={model.to} onChange={(e) => setModel({ ...model, to: e.target.value })} />
            </Field>
          </div>
        )}
      </fieldset>
      <label className="check">
        <input type="checkbox" checked={ok} onChange={(e) => setOk(e.target.checked)} />{" "}{t("I'm allowed to keep this copy (for example, I downloaded it from the publisher or bought it).")}</label>
      {error != null && !errorField(error) && <div className="notice bad">{errorMessage(error)}</div>}
    </Modal>
  );
}
