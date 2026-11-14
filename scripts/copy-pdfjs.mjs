// Copy pdf.js's fonts, character maps, colour profiles and WebAssembly decoders into
// public/pdfjs so the viewer loads them from the app itself (never from the network).
import { cpSync, mkdirSync } from "node:fs";
import { dirname, join } from "node:path";
import { createRequire } from "node:module";
import { fileURLToPath } from "node:url";

const require = createRequire(import.meta.url);
const root = dirname(require.resolve("pdfjs-dist/package.json"));
const out = join(dirname(fileURLToPath(import.meta.url)), "..", "public", "pdfjs");
mkdirSync(out, { recursive: true });
for (const dir of ["cmaps", "standard_fonts", "wasm", "iccs"]) {
  cpSync(join(root, dir), join(out, dir), { recursive: true });
}
console.log(`pdf.js assets copied to ${out}`);
