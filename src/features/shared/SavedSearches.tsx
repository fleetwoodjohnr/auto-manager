import { useState } from "react";
import { Bookmark, X } from "lucide-react";
import { useAction, useCmd } from "../../api/queries";
import type { SavedSearch } from "../../api/types";

/** Save the current filters of a list under a name, and bring them back with one click. */
export function SavedSearches<T extends Record<string, unknown>>({ page, filters, onApply }: { page: string; filters: T; onApply: (f: T) => void }) {
  const list = useCmd<SavedSearch[]>("saved_search_list", { page });
  const add = useAction<{ name: string; page: string; filters: T }>("saved_search_add", { invalidate: ["saved_search_list"], success: "Search saved" });
  const del = useAction<{ id: string }>("saved_search_delete", { invalidate: ["saved_search_list"] });
  const [naming, setNaming] = useState(false);
  const [name, setName] = useState("");
  return (
    <div className="row small">
      {list.data?.map((s) => (
        <span key={s.id} className="toggle" style={{ paddingRight: 2 }}>
          <button className="btn link small" style={{ padding: 0 }} onClick={() => onApply(s.filters as T)}>
            {s.name}
          </button>
          <button className="icon-btn" style={{ minWidth: 24, height: 24 }} aria-label={`Forget saved search ${s.name}`} onClick={() => del.mutate({ id: s.id })}>
            <X size={12} aria-hidden />
          </button>
        </span>
      ))}
      {naming ? (
        <form
          className="row"
          onSubmit={(e) => {
            e.preventDefault();
            add.mutate({ name, page, filters }, { onSuccess: () => (setNaming(false), setName("")) });
          }}
        >
          <input type="text" value={name} onChange={(e) => setName(e.target.value)} placeholder="Name this search" autoFocus aria-label="Saved search name" style={{ width: 180 }} />
          <button className="btn" disabled={!name.trim()}>
            Save
          </button>
          <button type="button" className="btn ghost" onClick={() => setNaming(false)}>
            Cancel
          </button>
        </form>
      ) : (
        <button className="btn ghost small" onClick={() => setNaming(true)}>
          <Bookmark size={13} aria-hidden /> Save this search
        </button>
      )}
    </div>
  );
}
