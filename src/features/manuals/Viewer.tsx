import { useEffect, useMemo, useRef, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { useVirtualizer } from "@tanstack/react-virtual";
import { Group, Panel, Separator } from "react-resizable-panels";
import { Bookmark, ChevronLeft, ChevronRight, MapPin, Maximize2, Printer, Search, StickyNote, Trash2, ZoomIn, ZoomOut } from "lucide-react";
import { call } from "../../api/ipc";
import { useAction, useCmd, useRefresh } from "../../api/queries";
import type { Annotation, AnnotationInput, Manual, PageHit } from "../../api/types";
import { useApp } from "../../store";
import { Badge, ErrorNote, Field, Modal, Spinner, Tabs } from "../../ui";
import { closePdf, loadManual, outline, type PdfDoc } from "../../lib/pdf";
import { indexInBackground, KIND_LABEL } from "./ImportManual";
import { Snippet } from "../vehicles/VehicleManuals";
import { t, tx } from "../../i18n";

const GAP = 16;

function PageCanvas({ doc, n, scale, width, height }: { doc: PdfDoc; n: number; scale: number; width: number; height: number }) {
  const canvas = useRef<HTMLCanvasElement>(null);
  useEffect(() => {
    let cancelled = false;
    let task: { cancel: () => void; promise: Promise<unknown> } | null = null;
    void doc.getPage(n).then((page) => {
      if (cancelled || !canvas.current) return;
      const dpr = window.devicePixelRatio || 1;
      const vp = page.getViewport({ scale: scale * dpr });
      const c = canvas.current;
      c.width = Math.floor(vp.width);
      c.height = Math.floor(vp.height);
      task = page.render({ canvas: c, viewport: vp });
      task.promise.catch(() => undefined);
    });
    return () => {
      cancelled = true;
      task?.cancel();
    };
  }, [doc, n, scale]);
  return (
    <div className="pdf-page" style={{ width, height }} aria-label={t("Page {0}", n)} role="img">
      <canvas ref={canvas} />
    </div>
  );
}

export default function Viewer() {
  const { route, go } = useApp();
  const id = route.id!;
  const meta = useCmd<Manual>("manual_get", { id });
  const doc = useQuery({ queryKey: ["pdf", id], queryFn: () => loadManual(id), staleTime: Infinity, gcTime: 0, retry: false });
  const [zoom, setZoom] = useState(1.2);
  const [base, setBase] = useState<{ w: number; h: number } | null>(null);
  const [page, setPage] = useState(Number(route.arg) || 1);
  const [side, setSide] = useState("contents");
  const [printing, setPrinting] = useState<string[] | null>(null);
  const [adding, setAdding] = useState<AnnotationInput | null>(null);
  const scroller = useRef<HTMLDivElement>(null);
  const refresh = useRefresh();
  const d = doc.data;
  const total = d?.numPages ?? 0;

  useEffect(() => {
    if (!d) return;
    void d.getPage(1).then((p) => {
      const vp = p.getViewport({ scale: 1 });
      setBase({ w: vp.width, h: vp.height });
    });
    return () => closePdf(d);
  }, [d]);

  const pageH = base ? base.h * zoom : 1000;
  const v = useVirtualizer({ count: total, getScrollElement: () => scroller.current, estimateSize: () => pageH + GAP, overscan: 2 });
  useEffect(() => v.measure(), [pageH, v]);
  const jumpedTo = useRef(false);
  useEffect(() => {
    if (base && total && !jumpedTo.current) {
      jumpedTo.current = true;
      if (page > 1) v.scrollToIndex(page - 1, { align: "start" });
    }
  }, [base, total, page, v]);
  const onScroll = () => {
    const first = v.getVirtualItems().find((i) => i.end > (scroller.current?.scrollTop ?? 0) + 40);
    if (first) setPage(first.index + 1);
  };
  const goPage = (n: number) => {
    const p = Math.max(1, Math.min(total, n));
    setPage(p);
    v.scrollToIndex(p - 1, { align: "start" });
  };
  const fitWidth = () => {
    if (base && scroller.current) setZoom(Math.max(0.3, (scroller.current.clientWidth - 48) / base.w));
  };
  const printPages = async (from: number, to: number) => {
    if (!d) return;
    const urls: string[] = [];
    for (let n = from; n <= to; n++) {
      const p = await d.getPage(n);
      const vp = p.getViewport({ scale: 2 });
      const c = document.createElement("canvas");
      c.width = vp.width;
      c.height = vp.height;
      await p.render({ canvas: c, viewport: vp }).promise;
      urls.push(c.toDataURL("image/png"));
    }
    setPrinting(urls);
    setTimeout(() => {
      window.print();
      setPrinting(null);
    }, 300);
  };

  return (
    <div className="viewer" style={{ height: "100%" }}>
      <div className="toolbar no-print">
        <button className="btn ghost" onClick={() => go("manuals")}>{t("← Library")}</button>
        <strong className="truncate" style={{ maxWidth: 320 }}>
          {meta.data?.title}
        </strong>
        {meta.data && <Badge>{KIND_LABEL[meta.data.kind]}</Badge>}
        <span className="flex-1" />
        <button className="icon-btn" aria-label={t("Previous page")} title={t("Previous page")} onClick={() => goPage(page - 1)} disabled={page <= 1}>
          <ChevronLeft size={16} aria-hidden />
        </button>
        <input type="number" value={page} min={1} max={total} onChange={(e) => goPage(Number(e.target.value))} style={{ width: 70 }} aria-label={t("Page")} />
        <span className="small muted">{tx("of {0}", total || "…")}</span>
        <button className="icon-btn" aria-label={t("Next page")} title={t("Next page")} onClick={() => goPage(page + 1)} disabled={page >= total}>
          <ChevronRight size={16} aria-hidden />
        </button>
        <button className="icon-btn" aria-label={t("Zoom out")} title={t("Zoom out")} onClick={() => setZoom((z) => Math.max(0.3, z / 1.2))}>
          <ZoomOut size={16} aria-hidden />
        </button>
        <span className="small muted" style={{ minWidth: 40, textAlign: "center" }}>
          {Math.round(zoom * 100)}%
        </span>
        <button className="icon-btn" aria-label={t("Zoom in")} title={t("Zoom in")} onClick={() => setZoom((z) => Math.min(5, z * 1.2))}>
          <ZoomIn size={16} aria-hidden />
        </button>
        <button className="icon-btn" aria-label={t("Fit width")} title={t("Fit width")} onClick={fitWidth}>
          <Maximize2 size={16} aria-hidden />
        </button>
        <button className="btn" onClick={() => setAdding({ page, kind: "bookmark", label: t("Page {0}", page), body: "", vehicle_id: null })}>
          <Bookmark size={14} aria-hidden />{" "}{t("Bookmark")}</button>
        <button className="btn" onClick={() => setAdding({ page, kind: "note", label: "", body: "", vehicle_id: null })}>
          <StickyNote size={14} aria-hidden />{" "}{t("Note")}</button>
        <button className="btn" title={t("Record that a component is shown on this page, for Where Is It?")} onClick={() => setAdding({ page, kind: "component", label: "", body: "", vehicle_id: null })}>
          <MapPin size={14} aria-hidden />{" "}{t("Component here")}</button>
        <button className="icon-btn" aria-label={t("Print this page")} title={t("Print this page")} onClick={() => printPages(page, page)}>
          <Printer size={16} aria-hidden />
        </button>
      </div>
      {doc.isError && (
        <div className="p-4">
          <ErrorNote error={doc.error} />
        </div>
      )}
      {doc.isPending && (
        <div className="p-4">
          <Spinner label={t("Opening document")} />{" "}{t("Opening…")}</div>
      )}
      {d && (
        <Group orientation="horizontal" style={{ flex: 1, minHeight: 0 }} className="no-print">
          <Panel defaultSize="24" minSize={180} collapsible>
            <div className="side">
              <Tabs
                label={t("Document panels")}
                value={side}
                onChange={setSide}
                tabs={[
                  { id: "contents", label: t("Contents") },
                  { id: "notes", label: t("Notes") },
                  { id: "search", label: t("Search") },
                ]}
              />
              {side === "contents" && <Contents doc={d} onGo={goPage} />}
              {side === "notes" && <Notes manualId={id} onGo={goPage} />}
              {side === "search" && <InDocSearch manual={meta.data} onGo={goPage} onIndex={() => meta.data && indexInBackground(meta.data, refresh)} />}
            </div>
          </Panel>
          <Separator className="resize-handle" />
          <Panel>
            <div className="pages" ref={scroller} onScroll={onScroll} style={{ height: "100%" }} tabIndex={0} aria-label={t("Document pages")}>
              <div style={{ height: v.getTotalSize(), position: "relative" }}>
                {base &&
                  v.getVirtualItems().map((it) => (
                    <div key={it.index} style={{ position: "absolute", top: it.start, left: 0, right: 0 }}>
                      <PageCanvas doc={d} n={it.index + 1} scale={zoom} width={base.w * zoom} height={base.h * zoom} />
                    </div>
                  ))}
              </div>
            </div>
          </Panel>
        </Group>
      )}
      {printing && (
        <div className="print-only">
          {printing.map((u, i) => (
            <img key={i} src={u} alt={t("Page {0}", page + i)} style={{ width: "100%", pageBreakAfter: "always" }} />
          ))}
        </div>
      )}
      {adding && <AddAnnotation manualId={id} initial={adding} onClose={() => setAdding(null)} />}
    </div>
  );
}

function Contents({ doc, onGo }: { doc: PdfDoc; onGo: (n: number) => void }) {
  const q = useQuery({ queryKey: ["outline", doc.fingerprints[0]], queryFn: () => outline(doc), staleTime: Infinity });
  if (q.isPending) return <Spinner label={t("Reading contents")} />;
  if (!q.data?.length) return <p className="small muted p-2">{t("This document has no table of contents. Use page numbers, search or bookmarks.")}</p>;
  return (
    <nav aria-label={t("Contents")}>
      {q.data.map((o, i) => (
        <button key={i} style={{ paddingLeft: 8 + o.depth * 12 }} onClick={() => o.page && onGo(o.page)} disabled={!o.page}>
          {o.title} {o.page ? <span className="muted small">· {o.page}</span> : null}
        </button>
      ))}
    </nav>
  );
}

function Notes({ manualId, onGo }: { manualId: string; onGo: (n: number) => void }) {
  const q = useCmd<Annotation[]>("annotation_list", { manualId, kind: null });
  const del = useAction<{ id: string }>("annotation_delete", { invalidate: ["annotation_list"] });
  if (!q.data?.length) return <p className="small muted p-2">{t("Bookmarks, notes and component locations you add appear here.")}</p>;
  return (
    <ul className="items" style={{ maxHeight: "none" }}>
      {q.data.map((a) => (
        <li key={a.id} style={{ alignItems: "flex-start" }}>
          {a.kind === "bookmark" ? <Bookmark size={14} aria-hidden /> : a.kind === "note" ? <StickyNote size={14} aria-hidden /> : <MapPin size={14} aria-hidden />}
          <div className="flex-1 min-w-0">
            <button style={{ padding: 0 }} onClick={() => onGo(a.page)}>
              {a.label} <span className="muted small">· p. {a.page}</span>
            </button>
            {a.body && <div className="small muted">{a.body}</div>}
          </div>
          <button className="icon-btn" aria-label={t("Delete {0}", a.label)} onClick={() => del.mutate({ id: a.id })}>
            <Trash2 size={13} aria-hidden />
          </button>
        </li>
      ))}
    </ul>
  );
}

function InDocSearch({ manual, onGo, onIndex }: { manual?: Manual; onGo: (n: number) => void; onIndex: () => void }) {
  const [text, setText] = useState("");
  const [q, setQ] = useState("");
  const ids = useMemo(() => (manual ? [manual.id] : []), [manual]);
  const r = useQuery<PageHit[]>({ queryKey: ["manual_search", ids, q], queryFn: () => call<PageHit[]>("manual_search", { query: q, manualIds: ids }), enabled: !!q && ids.length > 0 });
  if (manual && !manual.indexed) {
    return (
      <div className="p-2 small">
        <p>{t("This document's text hasn't been read yet, so it can't be searched.")}</p>
        <button className="btn" onClick={onIndex}>{t("Make searchable")}</button>
        <p className="muted mt-1">{t("Scanned (image-only) PDFs have no text to search.")}</p>
      </div>
    );
  }
  return (
    <div>
      <form
        className="row p-1"
        onSubmit={(e) => {
          e.preventDefault();
          setQ(text.trim());
        }}
      >
        <input type="search" value={text} onChange={(e) => setText(e.target.value)} placeholder={t("Find in this document")} style={{ flex: 1 }} aria-label={t("Find in this document")} />
        <button className="icon-btn" aria-label={t("Find")}>
          <Search size={15} aria-hidden />
        </button>
      </form>
      {r.data?.length === 0 && <p className="small muted p-2">{t("Not found.")}</p>}
      {r.data?.map((h) => (
        <button key={h.page} onClick={() => onGo(h.page)}>
          <strong>{tx("Page {0}", h.page)}</strong>
          <div className="small muted">
            <Snippet text={h.snippet} />
          </div>
        </button>
      ))}
    </div>
  );
}

function AddAnnotation({ manualId, initial, onClose }: { manualId: string; initial: AnnotationInput; onClose: () => void }) {
  const [a, setA] = useState(initial);
  const add = useAction<{ manualId: string; input: AnnotationInput }>("annotation_add", { invalidate: ["annotation_list", "where_is"], success: t("Saved"), silentError: true });
  const title = a.kind === "bookmark" ? t("Bookmark this page") : a.kind === "note" ? t("Note on this page") : t("Component shown on this page");
  return (
    <Modal
      open
      title={title}
      onClose={onClose}
      footer={
        <>
          <button className="btn" onClick={onClose}>{t("Cancel")}</button>
          <button className="btn primary" onClick={() => add.mutate({ manualId, input: a }, { onSuccess: onClose })}>{t("Save")}</button>
        </>
      }
    >
      <p className="muted">{tx("Page {0}", a.page)}</p>
      <Field label={a.kind === "component" ? t("Component") : t("Title")} hint={a.kind === "component" ? t("e.g. Starter motor. Where Is It? will find this page for it.") : undefined}>
        <input type="text" value={a.label} onChange={(e) => setA({ ...a, label: e.target.value })} autoFocus />
      </Field>
      <Field label={a.kind === "component" ? t("Where it is / how to reach it (optional)") : t("Note")}>
        <textarea value={a.body} onChange={(e) => setA({ ...a, body: e.target.value })} />
      </Field>
      {add.error != null && <div className="notice bad">{String((add.error as { message?: string }).message ?? add.error)}</div>}
    </Modal>
  );
}
