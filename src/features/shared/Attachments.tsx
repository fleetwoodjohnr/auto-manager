import { useEffect, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { FileText, Paperclip, Trash2 } from "lucide-react";
import { call, errorMessage } from "../../api/ipc";
import { useAction, useCmd } from "../../api/queries";
import type { Attachment } from "../../api/types";
import { toast } from "../../store";
import { bytes as size } from "../../lib/format";

function Thumb({ a }: { a: Attachment }) {
  const [url, setUrl] = useState<string | null>(null);
  useEffect(() => {
    let u: string | null = null;
    void call<ArrayBuffer>("attachment_bytes", { id: a.id }).then((b) => {
      u = URL.createObjectURL(new Blob([b], { type: a.mime }));
      setUrl(u);
    });
    return () => {
      if (u) URL.revokeObjectURL(u);
    };
  }, [a.id, a.mime]);
  return url ? <img src={url} alt={a.caption || a.file_name} style={{ width: 120, height: 90, objectFit: "cover", borderRadius: 8, border: "1px solid var(--border)" }} /> : <span className="spinner" />;
}

/** Photos and PDF documents (receipts, invoices, references) attached to a record. */
export function Attachments({ ownerKind, ownerId, title = "Photos and files" }: { ownerKind: string; ownerId: string; title?: string }) {
  const q = useCmd<Attachment[]>("attachment_list", { ownerKind, ownerId });
  const del = useAction<{ id: string }>("attachment_delete", { invalidate: ["attachment_list"] });
  const add = async () => {
    const picked = await open({ multiple: true, filters: [{ name: "Photos and PDFs", extensions: ["png", "jpg", "jpeg", "webp", "pdf"] }] });
    const paths = Array.isArray(picked) ? picked : picked ? [picked] : [];
    for (const path of paths) {
      try {
        await call("attachment_add", { ownerKind, ownerId, path, caption: null });
      } catch (e) {
        toast(errorMessage(e), { tone: "bad" });
      }
    }
    void q.refetch();
  };
  const list = q.data ?? [];
  return (
    <section className="card">
      <div className="spread">
        <h2>
          <Paperclip size={16} aria-hidden /> {title}
        </h2>
        <button className="btn" onClick={add}>
          Attach…
        </button>
      </div>
      {list.length === 0 ? (
        <p className="small muted">Photos (PNG, JPEG, WebP) and PDFs such as receipts or invoices.</p>
      ) : (
        <div className="row" style={{ alignItems: "flex-start" }}>
          {list.map((a) => (
            <figure key={a.id} className="stack" style={{ gap: 4, margin: 0, width: 120 }}>
              {a.mime.startsWith("image/") ? (
                <button className="btn ghost" style={{ padding: 0, minHeight: 0 }} onClick={() => call("attachment_open", { id: a.id }).catch((e) => toast(errorMessage(e), { tone: "bad" }))} aria-label={`Open ${a.file_name}`}>
                  <Thumb a={a} />
                </button>
              ) : (
                <button className="btn" style={{ width: 120, height: 90, flexDirection: "column" }} onClick={() => call("attachment_open", { id: a.id }).catch((e) => toast(errorMessage(e), { tone: "bad" }))} aria-label={`Open ${a.file_name}`}>
                  <FileText size={22} aria-hidden /> PDF
                </button>
              )}
              <figcaption className="small muted truncate" title={a.file_name}>
                {a.file_name} · {size(a.size)}
              </figcaption>
              <button className="btn link small" style={{ padding: 0, alignSelf: "flex-start" }} onClick={() => del.mutate({ id: a.id })} aria-label={`Remove ${a.file_name}`}>
                <Trash2 size={12} aria-hidden /> Remove
              </button>
            </figure>
          ))}
        </div>
      )}
    </section>
  );
}
