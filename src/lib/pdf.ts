// pdf.js wrapper. The legacy build runs on older system webviews (macOS 11 WKWebView) too.
// Fonts, character maps and decoders load from the app's own files (public/pdfjs), never the
// network; scripting inside PDFs is off.
import * as pdfjs from "pdfjs-dist/legacy/build/pdf.mjs";
import workerUrl from "pdfjs-dist/legacy/build/pdf.worker.min.mjs?url";
import { call } from "../api/ipc";

pdfjs.GlobalWorkerOptions.workerSrc = workerUrl;

export type PdfDoc = Awaited<ReturnType<typeof openPdf>>;

const tasks = new WeakMap<object, { destroy: () => Promise<void> }>();

export async function openPdf(bytes: ArrayBuffer) {
  const task = pdfjs.getDocument({
    data: new Uint8Array(bytes),
    enableXfa: false,
    standardFontDataUrl: "/pdfjs/standard_fonts/",
    cMapUrl: "/pdfjs/cmaps/",
    cMapPacked: true,
    wasmUrl: "/pdfjs/wasm/",
    iccUrl: "/pdfjs/iccs/",
  });
  const doc = await task.promise;
  tasks.set(doc, task);
  return doc;
}

/** Free the worker and memory of an open document. */
export function closePdf(doc: object) {
  void tasks.get(doc)?.destroy();
}

export async function loadManual(id: string) {
  const bytes = await call<ArrayBuffer>("manual_bytes", { id });
  return openPdf(bytes);
}

/** Extract every page's text and store it for search, in batches. */
export async function indexManual(id: string, onProgress?: (done: number, total: number) => void) {
  const doc = await loadManual(id);
  const total = doc.numPages;
  let batch: [number, string][] = [];
  for (let n = 1; n <= total; n++) {
    const page = await doc.getPage(n);
    const tc = await page.getTextContent();
    const text = tc.items.map((i) => ("str" in i ? i.str + (i.hasEOL ? "\n" : " ") : "")).join("");
    batch.push([n, text]);
    page.cleanup();
    if (batch.length === 25 || n === total) {
      await call("manual_set_pages", { id, pageCount: total, pages: batch, done: n === total });
      batch = [];
      onProgress?.(n, total);
    }
  }
  closePdf(doc);
  return total;
}

/** Outline entries resolved to page numbers (1-based). */
export async function outline(doc: PdfDoc): Promise<{ title: string; page: number; depth: number }[]> {
  const out: { title: string; page: number; depth: number }[] = [];
  const walk = async (items: Awaited<ReturnType<PdfDoc["getOutline"]>>, depth: number) => {
    for (const it of items ?? []) {
      let page = 0;
      try {
        const dest = typeof it.dest === "string" ? await doc.getDestination(it.dest) : it.dest;
        if (Array.isArray(dest) && dest[0]) page = (await doc.getPageIndex(dest[0])) + 1;
      } catch {
        /* a broken entry is listed without a page */
      }
      out.push({ title: it.title, page, depth });
      if (it.items?.length && depth < 3) await walk(it.items, depth + 1);
    }
  };
  await walk(await doc.getOutline(), 0);
  return out;
}
