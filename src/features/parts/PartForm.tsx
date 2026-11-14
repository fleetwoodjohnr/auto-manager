import { useState } from "react";
import { Plus, Trash2 } from "lucide-react";
import { errorField, errorMessage } from "../../api/ipc";
import { useCmd } from "../../api/queries";
import type { Category, Identifier, Part, PartInput } from "../../api/types";
import { Field } from "../../ui";

export const ID_KIND_LABEL: Record<string, string> = { oem: "OEM number", aftermarket: "Aftermarket number", barcode: "Barcode (UPC/EAN)", interchange: "Interchange / cross-reference", supplier: "Supplier number" };

export function emptyPart(categoryId?: string | null): PartInput {
  return { name: "", category_id: categoryId ?? null, brand: "", manufacturer: "", description: "", specs: [], install_notes: "", warranty: "", identifiers: [] };
}

export function partToInput(p: Part): PartInput {
  return { name: p.name, category_id: p.category_id, brand: p.brand, manufacturer: p.manufacturer, description: p.description, specs: p.specs, install_notes: p.install_notes, warranty: p.warranty, identifiers: p.identifiers };
}

/** The editable fields of a catalog part. Used on its own and inside "Add inventory". */
export function PartFields({ value, onChange, error }: { value: PartInput; onChange: (p: PartInput) => void; error?: unknown }) {
  const cats = useCmd<Category[]>("categories");
  const fe = (f: string) => (error && errorField(error) === f ? errorMessage(error) : null);
  const set = <K extends keyof PartInput>(k: K, v: PartInput[K]) => onChange({ ...value, [k]: v });
  const top = (cats.data ?? []).filter((c) => !c.parent_id);
  const ids = value.identifiers ?? [];
  const setId = (i: number, patch: Partial<Identifier>) => set("identifiers", ids.map((x, j) => (j === i ? { ...x, ...patch } : x)));
  const specs = value.specs ?? [];
  return (
    <div className="stack">
      <div className="grid two">
        <Field label="Part name" error={fe("name")}>
          <input type="text" value={value.name} onChange={(e) => set("name", e.target.value)} placeholder="e.g. Front brake pad set" autoFocus />
        </Field>
        <Field label="Category" error={fe("category_id")}>
          <select value={value.category_id ?? ""} onChange={(e) => set("category_id", e.target.value || null)}>
            <option value="">— Choose —</option>
            {top.map((c) => (
              <optgroup key={c.id} label={c.name}>
                <option value={c.id}>{c.name} (general)</option>
                {(cats.data ?? [])
                  .filter((x) => x.parent_id === c.id)
                  .map((x) => (
                    <option key={x.id} value={x.id}>
                      {x.name}
                    </option>
                  ))}
              </optgroup>
            ))}
          </select>
        </Field>
        <Field label="Brand">
          <input type="text" value={value.brand ?? ""} onChange={(e) => set("brand", e.target.value)} />
        </Field>
        <Field label="Manufacturer (if different)">
          <input type="text" value={value.manufacturer ?? ""} onChange={(e) => set("manufacturer", e.target.value)} />
        </Field>
      </div>
      <fieldset className="card" style={{ padding: 12 }}>
        <legend className="label">Part numbers</legend>
        {fe("identifiers") && <p className="small" style={{ color: "var(--danger)" }}>{fe("identifiers")}</p>}
        {ids.map((id, i) => (
          <div key={i} className="row mb-1">
            <select value={id.kind} onChange={(e) => setId(i, { kind: e.target.value })} aria-label="Number type">
              {Object.entries(ID_KIND_LABEL).map(([k, l]) => (
                <option key={k} value={k}>
                  {l}
                </option>
              ))}
            </select>
            <input type="text" className="mono" value={id.value} onChange={(e) => setId(i, { value: e.target.value })} aria-label="Number" style={{ flex: 1 }} />
            <input type="text" value={id.brand} onChange={(e) => setId(i, { brand: e.target.value })} placeholder="Brand (optional)" aria-label="Brand of this number" style={{ width: 150 }} />
            <button type="button" className="icon-btn" aria-label="Remove number" onClick={() => set("identifiers", ids.filter((_, j) => j !== i))}>
              <Trash2 size={14} aria-hidden />
            </button>
          </div>
        ))}
        <button type="button" className="btn link" style={{ paddingLeft: 0 }} onClick={() => set("identifiers", [...ids, { kind: ids.length ? "aftermarket" : "oem", value: "", brand: "" }])}>
          <Plus size={14} aria-hidden /> Add a number
        </button>
      </fieldset>
      <Field label="Description">
        <textarea value={value.description ?? ""} onChange={(e) => set("description", e.target.value)} />
      </Field>
      <details>
        <summary>Technical specifications, installation and warranty</summary>
        <div className="stack mt-2">
          {specs.map(([k, v], i) => (
            <div key={i} className="row">
              <input type="text" value={k} placeholder="e.g. Thickness" onChange={(e) => set("specs", specs.map((s, j) => (j === i ? [e.target.value, s[1]] : s)))} aria-label="Specification name" />
              <input type="text" value={v} placeholder="e.g. 17 mm" onChange={(e) => set("specs", specs.map((s, j) => (j === i ? [s[0], e.target.value] : s)))} aria-label="Specification value" style={{ flex: 1 }} />
              <button type="button" className="icon-btn" aria-label="Remove specification" onClick={() => set("specs", specs.filter((_, j) => j !== i))}>
                <Trash2 size={14} aria-hidden />
              </button>
            </div>
          ))}
          <button type="button" className="btn link" style={{ paddingLeft: 0 }} onClick={() => set("specs", [...specs, ["", ""]])}>
            <Plus size={14} aria-hidden /> Add a specification
          </button>
          <Field label="Installation information" hint="Copy only from instructions you're allowed to use; note the source.">
            <textarea value={value.install_notes ?? ""} onChange={(e) => set("install_notes", e.target.value)} />
          </Field>
          <Field label="Warranty">
            <input type="text" value={value.warranty ?? ""} onChange={(e) => set("warranty", e.target.value)} placeholder="e.g. 2 years / 24,000 miles" />
          </Field>
        </div>
      </details>
    </div>
  );
}

export function usePartDraft(initial: PartInput) {
  return useState<PartInput>(initial);
}
