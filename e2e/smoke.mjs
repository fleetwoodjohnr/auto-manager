// Launch the app on an empty data folder, visit every page in both themes, screenshot it, and
// fail on any error shown on screen.
import assert from "node:assert/strict";
import { mkdtempSync } from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";
import { bodyText, click, launch, nav, stopDisplay, waitText } from "./harness.mjs";

const PAGES = ["Dashboard", "Vehicle Lookup", "My Vehicles", "Parts Catalog", "Inventory", "Manuals & Docs", "Maintenance & Service", "Pricing & Estimates", "Reports", "Settings"];
const dataDir = mkdtempSync(path.join(tmpdir(), "am-smoke-"));
const app = await launch({ dataDir });
const b = app.browser;
try {
  await waitText(b, "Find a vehicle");
  for (const theme of ["light", "dark"]) {
    await b.execute((t) => (document.documentElement.dataset.theme = t), theme);
    for (const [i, p] of PAGES.entries()) {
      await nav(b, p);
      await b.pause(700);
      const text = await bodyText(b);
      assert.ok(!/Something went wrong|isn't allowed|not allowed|Unhandled/i.test(text), `${p}: ${text.slice(0, 400)}`);
      await app.shot(`${theme}-${String(i + 1).padStart(2, "0")}-${p.replace(/\W+/g, "-").toLowerCase()}`);
    }
  }
  await nav(b, "Settings");
  await click(b, "Updates");
  await waitText(b, "Check for Updates");
  console.log("smoke: every page rendered in both themes");
} finally {
  await app.close();
  stopDisplay();
}
