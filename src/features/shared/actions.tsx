import { useState } from "react";
import { call, errorField, errorMessage } from "../../api/ipc";
import { useAction, useCmd } from "../../api/queries";
import type { Job, JobDetail, PartList, Reservation } from "../../api/types";
import { useApp, toast } from "../../store";
import { Field, Modal, Spinner } from "../../ui";
import { useQueryClient } from "@tanstack/react-query";
import { invalidateData } from "../../api/queries";
import { t, tx } from "../../i18n";

/** Reserve stock of an item, optionally for a vehicle. */
export function ReserveDialog({ item, vehicleId, onClose }: { item: { id: string; sku: string; available: number } | null; vehicleId?: string; onClose: () => void }) {
  const [qty, setQty] = useState("1");
  const [note, setNote] = useState("");
  const reserve = useAction<{ input: unknown }, Reservation>("stock_reserve", {
    success: (r) => `Reserved ${r.qty} × ${r.sku}`,
    undo: (r) => () => void call("stock_release", { id: r.id }).then(() => toast(t("Reservation released"))),
    silentError: true,
  });
  const err = reserve.error;
  return (
    <Modal
      open={!!item}
      title={t("Reserve {0}", item?.sku ?? "")}
      onClose={onClose}
      footer={
        <>
          <button className="btn" onClick={onClose}>{t("Cancel")}</button>
          <button
            className="btn primary"
            disabled={reserve.isPending}
            onClick={() => item && reserve.mutate({ input: { item_id: item.id, location_id: null, qty: Number(qty), vehicle_id: vehicleId ?? null, job_id: null, line_id: null, note } }, { onSuccess: onClose })}
          >{t("Reserve")}</button>
        </>
      }
    >
      <p className="muted">{tx("{0} available. Reserved stock stays on the shelf but can't be sold or used elsewhere until you release it.", item?.available)}</p>
      <Field label={t("Quantity")} error={err && errorField(err) === "qty" ? errorMessage(err) : null}>
        <input type="number" min={0} step="any" value={qty} onChange={(e) => setQty(e.target.value)} autoFocus />
      </Field>
      <Field label={t("Note (optional)")}>
        <input type="text" value={note} onChange={(e) => setNote(e.target.value)} placeholder={t("e.g. Customer picking up Friday")} />
      </Field>
      {err != null && errorField(err) !== "qty" && <div className="notice bad">{errorMessage(err)}</div>}
    </Modal>
  );
}

/** Add an inventory item to an open estimate (or a new one) for a vehicle. */
export function AddToEstimate({ item, vehicleId, onClose }: { item: { id: string; name: string } | null; vehicleId?: string | null; onClose: () => void }) {
  const go = useApp((s) => s.go);
  const qc = useQueryClient();
  const jobs = useCmd<Job[]>("job_list", { query: { status: "open", vehicle_id: vehicleId ?? null, customer_id: null } }, !!item);
  const [jobId, setJobId] = useState<string>("new");
  const [qty, setQty] = useState("1");
  const [reserve, setReserve] = useState(true);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const add = async () => {
    if (!item) return;
    setBusy(true);
    setError(null);
    try {
      let id = jobId;
      if (id === "new") id = (await call<JobDetail>("job_create", { input: { vehicle_id: vehicleId ?? null, title: "" } })).job.id;
      const d = await call<JobDetail>("job_add_part", { id, input: { item_id: item.id, qty, reserve } });
      void invalidateData(qc);
      toast(t("Added to estimate #{0}", d.job.number), { action: { label: t("Open"), run: () => go({ page: "job", id }) } });
      onClose();
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      setBusy(false);
    }
  };
  return (
    <Modal
      open={!!item}
      title={t("Add to estimate")}
      onClose={onClose}
      footer={
        <>
          <button className="btn" onClick={onClose}>{t("Cancel")}</button>
          <button className="btn primary" disabled={busy} onClick={add}>
            {busy && <Spinner />}{" "}{t("Add")}</button>
        </>
      }
    >
      <p>
        <strong>{item?.name}</strong>
      </p>
      <Field label={t("Estimate")}>
        <select value={jobId} onChange={(e) => setJobId(e.target.value)}>
          <option value="new">{tx("New estimate{0}", vehicleId ? t(" for this vehicle") : "")}</option>
          {jobs.data?.map((j) => (
            <option key={j.id} value={j.id}>
              #{j.number} {j.title || j.vehicle_title || ""} ({j.status.replace("_", " ")})
            </option>
          ))}
        </select>
      </Field>
      <Field label={t("Quantity")}>
        <input type="number" min={0} step="any" value={qty} onChange={(e) => setQty(e.target.value)} />
      </Field>
      <label className="check">
        <input type="checkbox" checked={reserve} onChange={(e) => setReserve(e.target.checked)} />{" "}{t("Reserve the stock for this estimate now")}</label>
      {error && <div className="notice bad">{error}</div>}
    </Modal>
  );
}

/** Save a catalog part to a parts list (shopping list) for a repair or project. */
export function AddToList({ part, vehicleId, onClose }: { part: { id: string; name: string } | null; vehicleId?: string | null; onClose: () => void }) {
  const lists = useCmd<PartList[]>("plist_list", { vehicleId: null }, !!part);
  const qc = useQueryClient();
  const [listId, setListId] = useState("new");
  const [name, setName] = useState("");
  const [qty, setQty] = useState("1");
  const [error, setError] = useState<string | null>(null);
  const add = async () => {
    if (!part) return;
    setError(null);
    try {
      let id = listId;
      if (id === "new") id = (await call<PartList>("plist_create", { name: name || t("Shopping list"), vehicleId: vehicleId ?? null })).id;
      await call("plist_add", { listId: id, partId: part.id, qty: Number(qty), note: null });
      qc.invalidateQueries({ queryKey: ["plist_list"] });
      toast(t("Added to the list"));
      onClose();
    } catch (e) {
      setError(errorMessage(e));
    }
  };
  return (
    <Modal
      open={!!part}
      title={t("Save to a parts list")}
      onClose={onClose}
      footer={
        <>
          <button className="btn" onClick={onClose}>{t("Cancel")}</button>
          <button className="btn primary" onClick={add}>{t("Save")}</button>
        </>
      }
    >
      <p>
        <strong>{part?.name}</strong>
      </p>
      <Field label={t("List")}>
        <select value={listId} onChange={(e) => setListId(e.target.value)}>
          <option value="new">{t("New list…")}</option>
          {lists.data?.map((l) => (
            <option key={l.id} value={l.id}>
              {l.name} {l.vehicle_title ? `(${l.vehicle_title})` : ""}
            </option>
          ))}
        </select>
      </Field>
      {listId === "new" && (
        <Field label={t("List name")}>
          <input type="text" value={name} onChange={(e) => setName(e.target.value)} placeholder={t("e.g. Front brake job")} autoFocus />
        </Field>
      )}
      <Field label={t("Quantity")}>
        <input type="number" min={0} step="any" value={qty} onChange={(e) => setQty(e.target.value)} />
      </Field>
      {error && <div className="notice bad">{error}</div>}
    </Modal>
  );
}
