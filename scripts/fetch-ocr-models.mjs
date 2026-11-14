// Fetch the text-recognition models used to read printed VINs (ocrs, CC BY-SA 4.0, trained on
// the HierText dataset) into src-tauri/resources/ocr, where the app bundles them. Each file is
// checked against a pinned SHA-256, so a changed model is refused rather than shipped.
import { createHash } from "node:crypto";
import { existsSync, mkdirSync, readFileSync, renameSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const BASE = "https://ocrs-models.s3-accelerate.amazonaws.com";
const MODELS = {
  "text-detection.onnx": "a917b23dbd9524b465df7e922641b3ff2981623df4ded5a0234004ef2fee7cfe",
  "text-recognition.onnx": "86c145c2edb96c8caed5b1ebb8f44d706408922211451c309c625157dd6061c5",
};
const out = join(dirname(fileURLToPath(import.meta.url)), "..", "src-tauri", "resources", "ocr");
mkdirSync(out, { recursive: true });
const sha = (b) => createHash("sha256").update(b).digest("hex");

for (const [name, want] of Object.entries(MODELS)) {
  const file = join(out, name);
  if (existsSync(file) && sha(readFileSync(file)) === want) continue;
  const res = await fetch(`${BASE}/${name}`);
  if (!res.ok) throw new Error(`Couldn't download ${name}: HTTP ${res.status}`);
  const bytes = Buffer.from(await res.arrayBuffer());
  const got = sha(bytes);
  if (got !== want) throw new Error(`${name} doesn't match its pinned SHA-256 (got ${got}). Not using it.`);
  writeFileSync(`${file}.partial`, bytes);
  renameSync(`${file}.partial`, file);
  console.log(`Fetched ${name}`);
}
writeFileSync(
  join(out, "LICENSE.txt"),
  "Text detection and recognition models from the ocrs project (https://github.com/robertknight/ocrs-models),\n" +
    "by Robert Knight, trained on the HierText dataset (Google Research).\n" +
    "Licensed under the Creative Commons Attribution-ShareAlike 4.0 International license:\n" +
    "https://creativecommons.org/licenses/by-sa/4.0/\n",
);
