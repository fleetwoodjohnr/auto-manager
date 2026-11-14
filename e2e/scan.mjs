// VIN and barcode scanning in the real app. WebKit's mock camera stands in for a real one, so
// this proves the camera opens and frames reach the decoder; the bundled text-recognition models
// are proven on a photo of a VIN plate.
import assert from "node:assert/strict";
import { mkdtempSync, readFileSync } from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";
import { click, invoke, launch, nav, root, stopDisplay, waitText } from "./harness.mjs";

const step = (s) => console.log(`  • ${s}`);
const app = await launch({ dataDir: mkdtempSync(path.join(tmpdir(), "am-scan-")), env: { AUTO_MANAGER_MOCK_CAMERA: "1" } });
const b = app.browser;
try {
  console.log("Scanning");
  await waitText(b, "Dashboard");
  await nav(b, "Vehicle Lookup");
  await click(b, "Scan…");
  await b.waitUntil(() => b.execute(() => (document.querySelector('video[aria-label="Camera picture"]')?.videoWidth ?? 0) > 0), {
    timeout: 20000,
    timeoutMsg: "the camera picture never appeared",
  });
  step("the camera opens inside the app");
  await click(b, "Read", { exact: true });
  await waitText(b, "No VIN with a correct check digit was found in this picture", 60000);
  step("a camera frame is read (WebKit's test pattern holds no VIN, and none is invented)");
  await app.shot("S1-scan-dialog");
  const AXE = readFileSync(path.join(root, "node_modules/axe-core/axe.min.js"), "utf8");
  await b.execute(AXE);
  const v = await b.execute(async () => (await window.axe.run(document, { runOnly: { type: "tag", values: ["wcag2a", "wcag2aa", "wcag21aa", "wcag22aa"] } })).violations.map((x) => x.id));
  assert.deepEqual(v, [], "scan dialog accessibility");
  const photo = path.join(root, "crates/core/tests/fixtures/scan/vin-plate.jpg");
  const r = await invoke(b, "scan_image", { path: photo });
  assert.ok(r.ocr, "the app found its text-recognition models");
  assert.equal(r.vins[0]?.vin, "1HGCM82633A004352", JSON.stringify(r));
  step("a photo of a VIN plate is read by the bundled text recognition");
  console.log("Scanning passed");
} finally {
  await app.close();
  stopDisplay();
}
