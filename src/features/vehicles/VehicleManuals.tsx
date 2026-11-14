import { useState } from "react";
import { BookOpen, MapPin, Search, Upload } from "lucide-react";
import { useQuery } from "@tanstack/react-query";
import { call } from "../../api/ipc";
import { useCmd } from "../../api/queries";
import type { Manual, Vehicle, WhereIsIt } from "../../api/types";
import { useApp } from "../../store";
import { Badge, Empty, ErrorNote, Loaded, Spinner } from "../../ui";
import { ImportManual } from "../manuals/ImportManual";
import { OemLinks } from "../manuals/OemLinks";

function Snippet({ text }: { text: string }) {
  const parts = text.split(/(\[\[.*?\]\])/g);
  return (
    <span>
      {parts.map((p, i) => (p.startsWith("[[") ? <mark key={i}>{p.slice(2, -2)}</mark> : <span key={i}>{p}</span>))}
    </span>
  );
}

export default function VehicleManuals({ vehicle }: { vehicle: Vehicle }) {
  const go = useApp((s) => s.go);
  const q = useCmd<Manual[]>("manual_list", { vehicleId: vehicle.id });
  const [importing, setImporting] = useState(false);
  return (
    <div className="grid two" style={{ alignItems: "start" }}>
      <section className="card">
        <div className="spread">
          <h2>Documents for this vehicle</h2>
          <button className="btn" onClick={() => setImporting(true)}>
            <Upload size={16} aria-hidden /> Import PDF
          </button>
        </div>
        <Loaded q={q}>
          {(list) =>
            list.length === 0 ? (
              <Empty icon={<BookOpen size={20} />} title="No documents linked yet">
                Import an owner's or service manual you have, or find one on the manufacturer's site below.
              </Empty>
            ) : (
              <ul className="items" style={{ maxHeight: "none" }}>
                {list.map((m) => (
                  <li key={m.id}>
                    <BookOpen size={15} aria-hidden />
                    <button className="btn link" style={{ padding: 0 }} onClick={() => go({ page: "manual", id: m.id })}>
                      {m.title}
                    </button>
                    <span className="ml-auto row">
                      <Badge>{m.kind.replace("_", " ")}</Badge>
                      {!m.indexed && <Badge tone="warn">not searchable yet</Badge>}
                    </span>
                  </li>
                ))}
              </ul>
            )
          }
        </Loaded>
        <OemLinks make={vehicle.make} />
      </section>
      <WhereIs vehicle={vehicle} />
      {importing && <ImportManual vehicle={vehicle} onClose={() => setImporting(false)} />}
    </div>
  );
}

/** "Where Is It?": find a component in this vehicle's own documents and saved references. */
export function WhereIs({ vehicle }: { vehicle: Vehicle }) {
  const go = useApp((s) => s.go);
  const [text, setText] = useState("");
  const [asked, setAsked] = useState("");
  const q = useQuery<WhereIsIt>({ queryKey: ["where_is", vehicle.id, asked], queryFn: () => call<WhereIsIt>("where_is", { vehicleId: vehicle.id, component: asked }), enabled: !!asked });
  const w = q.data;
  const nothing = w && w.saved.length === 0 && w.pages.length === 0 && w.install_notes.length === 0;
  return (
    <section className="card">
      <h2>
        <MapPin size={16} aria-hidden /> Where Is It?
      </h2>
      <form
        className="row"
        onSubmit={(e) => {
          e.preventDefault();
          setAsked(text.trim());
        }}
      >
        <input type="search" value={text} onChange={(e) => setText(e.target.value)} placeholder="e.g. starter motor, cabin filter, fuse box" aria-label="Component" style={{ flex: 1 }} />
        <button className="btn primary" disabled={!text.trim()}>
          <Search size={16} aria-hidden /> Find
        </button>
      </form>
      <p className="small muted mt-1">Searches only your documents and notes for this vehicle. Auto Manager never makes up locations or procedures.</p>
      {q.isFetching && <Spinner label="Searching" />}
      {q.isError && <ErrorNote error={q.error} />}
      {w && (
        <div className="stack mt-2">
          {w.saved.map((a) => (
            <div key={a.id} className="notice ok">
              <strong>{a.label}</strong>: {a.body || "location recorded"}
              <div className="small">
                <button className="btn link" style={{ padding: 0 }} onClick={() => go({ page: "manual", id: a.manual_id, arg: String(a.page) })}>
                  {a.manual_title}, page {a.page}
                </button>
              </div>
            </div>
          ))}
          {w.pages.length > 0 && (
            <div>
              <p className="label">Mentioned in your documents</p>
              <ul className="items" style={{ maxHeight: "none" }}>
                {w.pages.map((p) => (
                  <li key={`${p.manual_id}:${p.page}`} style={{ display: "block" }}>
                    <button className="btn link" style={{ padding: 0 }} onClick={() => go({ page: "manual", id: p.manual_id, arg: String(p.page) })}>
                      {p.title}, page {p.page}
                    </button>
                    <div className="small muted">
                      <Snippet text={p.snippet} />
                    </div>
                  </li>
                ))}
              </ul>
            </div>
          )}
          {w.install_notes.map((n) => (
            <div key={n.part_id} className="notice">
              <strong>Installation notes on {n.part_name}:</strong> <span style={{ whiteSpace: "pre-wrap" }}>{n.notes}</span>
            </div>
          ))}
          {nothing && (
            <div className="notice warn">
              No location for “{w.component}” is recorded for this vehicle{w.searched_documents === 0 ? ", and none of its documents are searchable yet" : ` in ${w.searched_documents} searchable document${w.searched_documents === 1 ? "" : "s"}`}. Import the service manual, or open a document
              and save the page as a component location once you find it.
            </div>
          )}
        </div>
      )}
    </section>
  );
}

export { Snippet };
