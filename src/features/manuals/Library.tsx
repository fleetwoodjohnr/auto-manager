import { useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { BookOpen, FileUp, Pencil, RefreshCw, Search, Trash2 } from "lucide-react";
import { call } from "../../api/ipc";
import { useAction, useCmd, useRefresh } from "../../api/queries";
import type { Manual, ManualMeta, PageHit, Vehicle } from "../../api/types";
import { useApp } from "../../store";
import { ActionsMenu, Badge, DataTable, Empty, Field, Loaded, Modal, PageHead, Spinner, Tabs, useConfirm } from "../../ui";
import { bytes, date } from "../../lib/format";
import { ImportManual, KIND_LABEL, indexInBackground } from "./ImportManual";
import { Snippet, WhereIs } from "../vehicles/VehicleManuals";
import { vehicleTitle } from "../vehicles/Garage";
import { t, tx } from "../../i18n";

export default function Library() {
  const { route, setTab, go } = useApp();
  const tab = route.tab ?? "library";
  const q = useCmd<Manual[]>("manual_list", { vehicleId: null });
  const [importing, setImporting] = useState(false);
  const [editing, setEditing] = useState<Manual | null>(null);
  const [kind, setKind] = useState("");
  const refresh = useRefresh();
  const del = useAction<{ id: string }>("manual_delete", { success: t("Document deleted") });
  const { ask, dialog } = useConfirm();
  const links = (m: Manual) =>
    m.links.map((l) => l.vehicle_title ?? [l.year_from && l.year_to ? (l.year_from === l.year_to ? l.year_from : `${l.year_from}–${l.year_to}`) : l.year_from, l.make, l.model].filter(Boolean).join(" ")).join("; ");
  return (
    <div className="page wide">
      {dialog}
      <PageHead title={t("Manuals & Documentation")} sub={t("Owner's and service manuals, bulletins and diagrams you have, searchable and linked to your vehicles.")}>
        <button className="btn primary" onClick={() => setImporting(true)}>
          <FileUp size={16} aria-hidden />{" "}{t("Import PDF")}</button>
      </PageHead>
      <Tabs
        label={t("Documentation")}
        value={tab}
        onChange={setTab}
        tabs={[
          { id: "library", label: t("Library"), count: q.data?.length },
          { id: "search", label: t("Search inside documents") },
          { id: "where", label: t("Where Is It?") },
        ]}
      />
      {tab === "library" && (
        <Loaded q={q}>
          {(list) =>
            list.length === 0 ? (
              <Empty icon={<BookOpen size={20} />} title={t("No documents yet")} action={<button className="btn primary" onClick={() => setImporting(true)}>{t("Import a PDF")}</button>}>{t("Import manuals you own or downloaded from the manufacturer. Auto Manager never downloads or shares copyrighted manuals itself.")}</Empty>
            ) : (
              <div className="stack">
                <select value={kind} onChange={(e) => setKind(e.target.value)} aria-label={t("Document type")} style={{ alignSelf: "flex-start" }}>
                  <option value="">{t("All types")}</option>
                  {Object.entries(KIND_LABEL).map(([k, l]) => (
                    <option key={k} value={k}>
                      {l}
                    </option>
                  ))}
                </select>
                <DataTable
                  label={t("Documents")}
                  rows={list.filter((m) => !kind || m.kind === kind)}
                  rowKey={(m) => m.id}
                  onOpen={(m) => go({ page: "manual", id: m.id })}
                  height="calc(100vh - 330px)"
                  menu={(m) => [
                    { label: t("Open"), run: () => go({ page: "manual", id: m.id }) },
                    { label: t("Edit details"), run: () => setEditing(m) },
                    !m.indexed ? { label: t("Make searchable"), run: () => indexInBackground(m, refresh) } : null,
                  ]}
                  columns={[
                    { id: "title", label: t("Title"), width: "minmax(200px, 2fr)", render: (m) => m.title, sort: (m) => m.title },
                    { id: "kind", label: t("Type"), width: "160px", render: (m) => KIND_LABEL[m.kind] ?? m.kind, sort: (m) => m.kind },
                    { id: "for", label: t("For"), width: "minmax(140px, 1.5fr)", render: (m) => <span className="small">{links(m) || "—"}</span> },
                    { id: "pages", label: t("Pages"), width: "70px", render: (m) => m.page_count ?? "", align: "right", sort: (m) => m.page_count ?? 0 },
                    { id: "search", label: t("Search"), width: "110px", render: (m) => (m.indexed ? <Badge tone="ok">searchable</Badge> : <Badge tone="warn">{t("not yet")}</Badge>) },
                    { id: "size", label: t("Size"), width: "80px", render: (m) => bytes(m.file_size), align: "right", sort: (m) => m.file_size },
                    {
                      id: "act",
                      label: "",
                      width: "44px",
                      render: (m) => (
                        <ActionsMenu
                          items={[
                            { label: t("Edit details"), icon: <Pencil size={14} />, run: () => setEditing(m) },
                            { label: t("Rebuild search text"), icon: <RefreshCw size={14} />, run: () => indexInBackground(m, refresh) },
                            null,
                            { label: t("Delete"), icon: <Trash2 size={14} />, danger: true, run: async () => (await ask(t("Delete this document?"), t("The stored PDF, its search text, bookmarks and notes are removed."))) && del.mutate({ id: m.id }) },
                          ]}
                        />
                      ),
                    },
                  ]}
                />
              </div>
            )
          }
        </Loaded>
      )}
      {tab === "search" && <SearchDocs />}
      {tab === "where" && <WhereAny />}
      {importing && <ImportManual onClose={() => setImporting(false)} />}
      {editing && <EditManual m={editing} onClose={() => setEditing(null)} />}
    </div>
  );
}

function SearchDocs() {
  const go = useApp((s) => s.go);
  const [text, setText] = useState("");
  const [query, setQuery] = useState("");
  const q = useQuery<PageHit[]>({ queryKey: ["manual_search", query], queryFn: () => call<PageHit[]>("manual_search", { query, manualIds: null }), enabled: !!query });
  return (
    <div className="stack">
      <form
        className="row"
        onSubmit={(e) => {
          e.preventDefault();
          setQuery(text.trim());
        }}
      >
        <input type="search" value={text} onChange={(e) => setText(e.target.value)} placeholder={t("Words to find in every searchable document")} style={{ flex: 1 }} aria-label={t("Search documents")} />
        <button className="btn primary" disabled={!text.trim()}>
          <Search size={16} aria-hidden />{" "}{t("Search")}</button>
      </form>
      {q.isFetching && <Spinner label={t("Searching")} />}
      {q.data &&
        (q.data.length === 0 ? (
          <p className="muted">{t("No page contains all of those words.")}</p>
        ) : (
          <ul className="items" style={{ maxHeight: "none" }}>
            {q.data.map((h) => (
              <li key={`${h.manual_id}:${h.page}`} style={{ display: "block" }}>
                <button className="btn link" style={{ padding: 0 }} onClick={() => go({ page: "manual", id: h.manual_id, arg: String(h.page) })}>{tx("{0}, page {1}", h.title, h.page)}</button>
                <div className="small muted">
                  <Snippet text={h.snippet} />
                </div>
              </li>
            ))}
          </ul>
        ))}
    </div>
  );
}

function WhereAny() {
  const vehicles = useCmd<Vehicle[]>("vehicle_list", { saved: true });
  const [vid, setVid] = useState("");
  const v = vehicles.data?.find((x) => x.id === vid);
  return (
    <div className="stack">
      <select value={vid} onChange={(e) => setVid(e.target.value)} aria-label={t("Vehicle")} style={{ alignSelf: "flex-start" }}>
        <option value="">{t("Choose a saved vehicle…")}</option>
        {vehicles.data?.map((x) => (
          <option key={x.id} value={x.id}>
            {vehicleTitle(x)}
          </option>
        ))}
      </select>
      {v ? <WhereIs vehicle={v} /> : <p className="muted">{t("Locations are looked up in the documents and notes linked to one vehicle.")}</p>}
    </div>
  );
}

function EditManual({ m, onClose }: { m: Manual; onClose: () => void }) {
  const [meta, setMeta] = useState<ManualMeta>({ title: m.title, kind: m.kind, publisher: m.publisher, source_url: m.source_url, license_note: m.license_note });
  const [links, setLinks] = useState(m.links);
  const vehicles = useCmd<Vehicle[]>("vehicle_list", { saved: true });
  const save = useAction<{ id: string; meta: ManualMeta }>("manual_update", { silentError: true });
  const saveLinks = useAction<{ id: string; links: typeof links }>("manual_set_links", { success: t("Saved"), silentError: true });
  const err = save.error ?? saveLinks.error;
  return (
    <Modal
      open
      title={t("Document details")}
      onClose={onClose}
      footer={
        <>
          <button className="btn" onClick={onClose}>{t("Cancel")}</button>
          <button className="btn primary" onClick={() => save.mutate({ id: m.id, meta }, { onSuccess: () => saveLinks.mutate({ id: m.id, links }, { onSuccess: onClose }) })}>{t("Save")}</button>
        </>
      }
    >
      <Field label={t("Title")}>
        <input type="text" value={meta.title} onChange={(e) => setMeta({ ...meta, title: e.target.value })} />
      </Field>
      <div className="grid two">
        <Field label={t("Type")}>
          <select value={meta.kind} onChange={(e) => setMeta({ ...meta, kind: e.target.value })}>
            {Object.entries(KIND_LABEL).map(([k, l]) => (
              <option key={k} value={k}>
                {l}
              </option>
            ))}
          </select>
        </Field>
        <Field label={t("Publisher")}>
          <input type="text" value={meta.publisher} onChange={(e) => setMeta({ ...meta, publisher: e.target.value })} />
        </Field>
      </div>
      <Field label={t("Source link")}>
        <input type="url" value={meta.source_url} onChange={(e) => setMeta({ ...meta, source_url: e.target.value })} />
      </Field>
      <p className="label">{t("Applies to")}</p>
      {links.map((l, i) => (
        <div key={i} className="row">
          <span className="flex-1">{l.vehicle_id ? (vehicles.data?.find((v) => v.id === l.vehicle_id) ? vehicleTitle(vehicles.data.find((v) => v.id === l.vehicle_id)!) : t("A vehicle")) : [l.year_from, l.year_to && l.year_to !== l.year_from ? `–${l.year_to}` : "", l.make, l.model].filter(Boolean).join(" ")}</span>
          <button className="btn link" onClick={() => setLinks(links.filter((_, j) => j !== i))}>{t("Remove")}</button>
        </div>
      ))}
      <select
        value=""
        aria-label={t("Link to a vehicle")}
        onChange={(e) => e.target.value && setLinks([...links, { vehicle_id: e.target.value, vehicle_title: null, make: null, model: null, year_from: null, year_to: null }])}
      >
        <option value="">{t("Link to a saved vehicle…")}</option>
        {vehicles.data?.map((v) => (
          <option key={v.id} value={v.id}>
            {vehicleTitle(v)}
          </option>
        ))}
      </select>
      <p className="small muted">{tx("Imported {0} · {1}", date(m.created_at), m.file_name)}</p>
      {err != null && <div className="notice bad">{String((err as { message?: string }).message ?? err)}</div>}
    </Modal>
  );
}
