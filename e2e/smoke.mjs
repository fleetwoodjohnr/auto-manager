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
  // The webview's updater call must be permitted: "Check now" reaches GitHub (up to date, or an
  // error if offline) and never "not allowed".
  await click(b, "Check now");
  await b.waitUntil(async () => /You're up to date|Update failed|is available/.test(await bodyText(b)), { timeout: 30000 });
  assert.ok(!/not allowed|isn't allowed/i.test(await bodyText(b)), "the updater plugin isn't permitted");
  console.log("smoke: every page rendered in both themes");

  // The other languages: every page renders, with its navigation translated.
  const setLang = (l) => b.execute((x) => (localStorage.setItem("auto-manager.language", x), location.reload()), l);
  for (const [lang, expect] of [["es", ["Panel", "Buscar vehículo", "Mis vehículos", "Configuración"]], ["fr", ["Tableau de bord", "Mes véhicules", "Paramètres"]]]) {
    await setLang(lang);
    await b.waitUntil(() => b.execute((l) => document.documentElement.lang === l && !!document.querySelector("nav button"), lang), { timeout: 20000 });
    for (let i = 0; i < PAGES.length; i++) {
      await b.execute((n) => document.querySelectorAll("nav")[0].querySelectorAll("button")[n].click(), i);
      await b.pause(600);
      const text = await bodyText(b);
      assert.ok(!/Something went wrong|Unhandled|undefined|\[object Object\]|\{\d\}/.test(text), `${lang} page ${i + 1}: ${text.slice(0, 400)}`);
      await app.shot(`${lang}-${String(i + 1).padStart(2, "0")}`);
    }
    const text = await bodyText(b);
    for (const w of expect) assert.ok(text.includes(w), `${lang}: “${w}” missing`);
  }
  console.log("smoke: every page rendered in Spanish and French");
} finally {
  // The language is a per-computer preference kept by the web view; leave it in English.
  await b.execute(() => localStorage.removeItem("auto-manager.language")).catch(() => {});
  await app.close();
  stopDisplay();
}
