import { useState } from "react";
import { ArrowRightLeft, Plus, Trash2 } from "lucide-react";
import { useAction, useCmd } from "../../api/queries";
import type { Interchange as I, PartRow, Related } from "../../api/types";
import { useApp } from "../../store";
import { Badge, Field, Modal } from "../../ui";

/** Supersession (old → new numbers) and interchangeable parts for one part. */
export function Interchange({ partId }: { partId: string }) {
  const go = useApp((s) => s.go);
  const q = useCmd<I>("interchange_get", { partId });
  const [adding, setAdding] = useState<"newer" | "equivalent" | null>(null);
  const inv = ["interchange_get", "vehicle_matches", "part_get"];
  const rmSup = useAction<{ oldPartId: string; newPartId: string }>("supersession_remove", { invalidate: inv });
  const rmEq = useAction<{ partId: string; otherId: string }>("equivalent_remove", { invalidate: inv });
  const row = (r: Related, onRemove: () => void) => (
    <li key={r.part_id}>
      <button className="btn link" style={{ padding: 0 }} onClick={() => go({ page: "part", id: r.part_id })}>
        {r.name}
      </button>
      <span className="small muted">{[r.brand, r.numbers].filter(Boolean).join(" · ")}</span>
      <span className="ml-auto row">
        <Badge tone={r.available > 0 ? "ok" : "neutral"}>{r.available} available</Badge>
        <button className="icon-btn" aria-label={`Remove link to ${r.name}`} onClick={onRemove}>
          <Trash2 size={13} aria-hidden />
        </button>
      </span>
    </li>
  );
  const d = q.data;
  return (
    <section className="card">
      <div className="spread">
        <h2>
          <ArrowRightLeft size={16} aria-hidden /> Alternatives and supersession
        </h2>
        <div className="row">
          <button className="btn" onClick={() => setAdding("equivalent")}>
            <Plus size={14} aria-hidden /> Equivalent
          </button>
          <button className="btn" onClick={() => setAdding("newer")}>
            <Plus size={14} aria-hidden /> Replaced by
          </button>
        </div>
      </div>
      {d && d.superseded_by.length + d.supersedes.length + d.equivalents.length === 0 && <p className="muted">No alternatives recorded.</p>}
      {d && d.superseded_by.length > 0 && (
        <>
          <p className="label">Replaced by (use the newer number)</p>
          <ul className="items" style={{ maxHeight: "none" }}>{d.superseded_by.map((r) => row(r, () => rmSup.mutate({ oldPartId: partId, newPartId: r.part_id })))}</ul>
        </>
      )}
      {d && d.supersedes.length > 0 && (
        <>
          <p className="label">Replaces</p>
          <ul className="items" style={{ maxHeight: "none" }}>{d.supersedes.map((r) => row(r, () => rmSup.mutate({ oldPartId: r.part_id, newPartId: partId })))}</ul>
        </>
      )}
      {d && d.equivalents.length > 0 && (
        <>
          <p className="label">Interchangeable</p>
          <ul className="items" style={{ maxHeight: "none" }}>{d.equivalents.map((r) => row(r, () => rmEq.mutate({ partId, otherId: r.part_id })))}</ul>
          <p className="small muted">Interchangeable parts still need their own fitment records to show as fitting a vehicle.</p>
        </>
      )}
      {adding && <PickPart partId={partId} mode={adding} onClose={() => setAdding(null)} />}
    </section>
  );
}

function PickPart({ partId, mode, onClose }: { partId: string; mode: "newer" | "equivalent"; onClose: () => void }) {
  const [q, setQ] = useState("");
  const [pick, setPick] = useState("");
  const [note, setNote] = useState("");
  const parts = useCmd<PartRow[]>("part_list", { query: { q: q || null, category_id: null, brand: null, limit: 40 } });
  const inv = ["interchange_get", "vehicle_matches", "part_get"];
  const sup = useAction<{ oldPartId: string; newPartId: string; note: string }>("supersession_add", { invalidate: inv, success: "Saved", silentError: true });
  const eq = useAction<{ partId: string; otherId: string; note: string }>("equivalent_add", { invalidate: inv, success: "Saved", silentError: true });
  const err = sup.error ?? eq.error;
  return (
    <Modal
      open
      title={mode === "newer" ? "This part is replaced by…" : "Interchangeable with…"}
      onClose={onClose}
      footer={
        <>
          <button className="btn" onClick={onClose}>
            Cancel
          </button>
          <button
            className="btn primary"
            disabled={!pick}
            onClick={() => (mode === "newer" ? sup.mutate({ oldPartId: partId, newPartId: pick, note }, { onSuccess: onClose }) : eq.mutate({ partId, otherId: pick, note }, { onSuccess: onClose }))}
          >
            Save
          </button>
        </>
      }
    >
      <input type="search" placeholder="Search the catalog" value={q} onChange={(e) => setQ(e.target.value)} autoFocus aria-label="Search parts" />
      <select size={8} value={pick} onChange={(e) => setPick(e.target.value)} aria-label="Parts">
        {parts.data
          ?.filter((p) => p.id !== partId)
          .map((p) => (
            <option key={p.id} value={p.id}>
              {p.name} {p.brand && `· ${p.brand}`} {p.numbers && `· ${p.numbers}`}
            </option>
          ))}
      </select>
      <Field label="Note / source (optional)">
        <input type="text" value={note} onChange={(e) => setNote(e.target.value)} placeholder="e.g. Supersession per dealer parts counter, 2026" />
      </Field>
      {err != null && <div className="notice bad">{String((err as { message?: string }).message ?? err)}</div>}
    </Modal>
  );
}
