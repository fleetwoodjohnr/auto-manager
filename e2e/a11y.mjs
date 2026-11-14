// Accessibility audit with axe-core (WCAG 2.0–2.2 A and AA) on every page, light and dark,
// against the real app. Fails on any violation. (The approach of Simple Setup's e2e/a11y.py.)
import { mkdtempSync, readFileSync } from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";
import { launch, nav, root, stopDisplay, waitText } from "./harness.mjs";

const AXE = readFileSync(path.join(root, "node_modules/axe-core/axe.min.js"), "utf8");
const PAGES = ["Dashboard", "Vehicle Lookup", "My Vehicles", "Parts Catalog", "Inventory", "Manuals & Docs", "Maintenance & Service", "Pricing & Estimates", "Reports", "Settings"];
const app = await launch({ dataDir: mkdtempSync(path.join(tmpdir(), "am-a11y-")) });
const b = app.browser;
const problems = [];
try {
  await waitText(b, "Dashboard");
  await b.execute(AXE);
  for (const theme of ["light", "dark"]) {
    await b.execute((t) => {
      localStorage.setItem("auto-manager-theme", t);
      document.documentElement.dataset.theme = t;
    }, theme);
    for (const p of PAGES) {
      await nav(b, p);
      await b.pause(500);
      const result = await b.execute(async () => {
        const r = await window.axe.run(document, { runOnly: { type: "tag", values: ["wcag2a", "wcag2aa", "wcag21a", "wcag21aa", "wcag22aa"] } });
        return r.violations.map((v) => ({ id: v.id, impact: v.impact, help: v.help, nodes: v.nodes.slice(0, 3).map((n) => n.target.join(" ")) }));
      });
      for (const v of result) problems.push(`${theme} · ${p}: [${v.impact}] ${v.id} — ${v.help} (${v.nodes.join(" | ")})`);
    }
  }
} finally {
  await app.close();
  stopDisplay();
}
if (problems.length) {
  console.error(problems.join("\n"));
  process.exit(1);
}
console.log("a11y: no WCAG A/AA violations on any page in either theme");
