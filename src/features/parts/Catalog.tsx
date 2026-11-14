import { useMemo, useState } from "react";
import { ChevronDown, ChevronRight, Cog, FileUp, FolderPlus, Plus } from "lucide-react";
import { useAction, useCmd } from "../../api/queries";
import type { Category, Part, PartInput, PartRow } from "../../api/types";
import { useApp } from "../../store";
import { cx, DataTable, Empty, Loaded, Modal, PageHead, ErrorNote } from "../../ui";
import { emptyPart, PartFields } from "./PartForm";
import { FitmentImport } from "./FitmentImport";
import { SavedSearches } from "../shared/SavedSearches";

export default function Catalog() {
  const { go, route } = useApp();
  const [cat, setCat] = useState<string | null>(route.arg ?? null);
  const [q, setQ] = useState("");
  const [brand, setBrand] = useState("");
  const [creating, setCreating] = useState(false);
  const [importing, setImporting] = useState(false);
  const cats = useCmd<Category[]>("categories");
  const brands = useCmd<string[]>("part_brands");
  const parts = useCmd<PartRow[]>("part_list", { query: { q: q || null, category_id: cat, brand: brand || null, limit: 5000 } });
  return (
    <div className="page wide">
      <PageHead title="Parts Catalog" sub="Every part you track, by vehicle system, with its numbers and the vehicles it fits.">
        <button className="btn" onClick={() => setImporting(true)}>
          <FileUp size={16} aria-hidden /> Import fitment
        </button>
        <button className="btn primary" onClick={() => setCreating(true)}>
          <Plus size={16} aria-hidden /> New part
        </button>
      </PageHead>
      <div className="flex gap-4" style={{ alignItems: "flex-start" }}>
        <aside className="card" style={{ width: 260, flex: "none", padding: 8, maxHeight: "calc(100vh - 200px)", overflow: "auto" }}>
          <Loaded q={cats}>{(list) => <CategoryTree list={list} value={cat} onChange={setCat} />}</Loaded>
        </aside>
        <div className="flex-1 min-w-0 stack">
          <div className="row">
            <input type="search" placeholder="Search name, brand or any part number" value={q} onChange={(e) => setQ(e.target.value)} style={{ flex: 1 }} aria-label="Search parts" />
            <select value={brand} onChange={(e) => setBrand(e.target.value)} aria-label="Brand">
              <option value="">All brands</option>
              {brands.data?.map((b) => (
                <option key={b} value={b}>
                  {b}
                </option>
              ))}
            </select>
          </div>
          <SavedSearches
            page="parts"
            filters={{ q, brand, cat }}
            onApply={(f) => {
              setQ(String(f.q ?? ""));
              setBrand(String(f.brand ?? ""));
              setCat((f.cat as string | null) ?? null);
            }}
          />
          <Loaded q={parts}>
            {(rows) =>
              rows.length === 0 ? (
                <Empty icon={<Cog size={20} />} title={q || cat || brand ? "No parts match" : "The catalog is empty"} action={<button className="btn" onClick={() => setCreating(true)}>Add a part</button>}>
                  Add parts by hand, import an inventory spreadsheet, or import a fitment file from your supplier.
                </Empty>
              ) : (
                <DataTable
                  label="Parts"
                  rows={rows}
                  rowKey={(r) => r.id}
                  onOpen={(r) => go({ page: "part", id: r.id })}
                  footer={`${rows.length} part${rows.length === 1 ? "" : "s"}`}
                  menu={(r) => [{ label: "Open", run: () => go({ page: "part", id: r.id }) }]}
                  columns={[
                    { id: "name", label: "Part", width: "minmax(200px, 2fr)", render: (r) => <strong style={{ fontWeight: 500 }}>{r.name}</strong>, sort: (r) => r.name },
                    { id: "brand", label: "Brand", width: "minmax(90px, 1fr)", render: (r) => r.brand, sort: (r) => r.brand },
                    { id: "numbers", label: "Numbers", width: "minmax(140px, 2fr)", render: (r) => <span className="mono small">{r.numbers}</span> },
                    { id: "cat", label: "Category", width: "minmax(110px, 1fr)", render: (r) => r.category, sort: (r) => r.category },
                    { id: "fit", label: "Fitment", width: "80px", render: (r) => (r.fitment_count ? r.fitment_count : <span className="muted">none</span>), sort: (r) => r.fitment_count, align: "right" },
                    { id: "stock", label: "Available", width: "90px", render: (r) => r.available, sort: (r) => r.available, align: "right" },
                  ]}
                />
              )
            }
          </Loaded>
        </div>
      </div>
      {creating && <NewPart categoryId={cat} onClose={() => setCreating(false)} />}
      {importing && <FitmentImport onClose={() => setImporting(false)} />}
    </div>
  );
}

function CategoryTree({ list, value, onChange }: { list: Category[]; value: string | null; onChange: (id: string | null) => void }) {
  const [open, setOpen] = useState<Record<string, boolean>>({});
  const [adding, setAdding] = useState<string | null>(null);
  const [name, setName] = useState("");
  const add = useAction<{ parentId: string | null; name: string }>("category_add", { invalidate: ["categories"], success: "Category added" });
  const top = list.filter((c) => !c.parent_id);
  const kids = useMemo(() => {
    const m = new Map<string, Category[]>();
    list.forEach((c) => c.parent_id && m.set(c.parent_id, [...(m.get(c.parent_id) ?? []), c]));
    return m;
  }, [list]);
  const count = (c: Category) => c.part_count + (kids.get(c.id) ?? []).reduce((n, k) => n + k.part_count, 0);
  const item = (c: Category, depth: number) => (
    <li key={c.id}>
      <div className="flex items-center" style={{ paddingLeft: depth * 14 }}>
        {depth === 0 && kids.has(c.id) ? (
          <button className="icon-btn" style={{ minWidth: 24, height: 24 }} aria-label={open[c.id] ? `Collapse ${c.name}` : `Expand ${c.name}`} aria-expanded={!!open[c.id]} onClick={() => setOpen({ ...open, [c.id]: !open[c.id] })}>
            {open[c.id] ? <ChevronDown size={14} aria-hidden /> : <ChevronRight size={14} aria-hidden />}
          </button>
        ) : (
          <span style={{ width: 24 }} />
        )}
        <button className={cx("btn ghost flex-1", value === c.id && "soft")} style={{ justifyContent: "space-between", minHeight: 28, padding: "2px 8px" }} aria-current={value === c.id} onClick={() => onChange(c.id)}>
          <span className="truncate">{c.name}</span>
          <span className="small muted">{count(c) || ""}</span>
        </button>
      </div>
      {depth === 0 && open[c.id] && (
        <ul className="tree root">
          {(kids.get(c.id) ?? []).map((k) => item(k, 1))}
          <li style={{ paddingLeft: 36 }}>
            {adding === c.id ? (
              <form
                className="row"
                onSubmit={(e) => {
                  e.preventDefault();
                  add.mutate({ parentId: c.id, name }, { onSuccess: () => (setAdding(null), setName("")) });
                }}
              >
                <input type="text" value={name} onChange={(e) => setName(e.target.value)} autoFocus aria-label="New category name" style={{ width: 150 }} />
                <button className="btn">Add</button>
              </form>
            ) : (
              <button className="btn link small" onClick={() => setAdding(c.id)}>
                <FolderPlus size={12} aria-hidden /> Add subcategory
              </button>
            )}
          </li>
        </ul>
      )}
    </li>
  );
  return (
    <nav aria-label="Categories">
      <button className={cx("btn ghost w-full", value === null && "soft")} style={{ justifyContent: "flex-start" }} onClick={() => onChange(null)}>
        All parts
      </button>
      <ul className="tree root">{top.map((c) => item(c, 0))}</ul>
    </nav>
  );
}

function NewPart({ categoryId, onClose }: { categoryId: string | null; onClose: () => void }) {
  const go = useApp((s) => s.go);
  const [p, setP] = useState<PartInput>(emptyPart(categoryId));
  const create = useAction<{ input: PartInput }, Part>("part_create", { success: "Part added", silentError: true });
  return (
    <Modal
      open
      wide
      title="New part"
      onClose={onClose}
      footer={
        <>
          <button className="btn" onClick={onClose}>
            Cancel
          </button>
          <button className="btn primary" disabled={create.isPending} onClick={() => create.mutate({ input: p }, { onSuccess: (part) => (onClose(), go({ page: "part", id: part.id })) })}>
            Add part
          </button>
        </>
      }
    >
      <PartFields value={p} onChange={setP} error={create.error} />
      {create.error != null && !(create.error as { field?: string }).field && <ErrorNote error={create.error} />}
    </Modal>
  );
}
