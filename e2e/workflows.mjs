// Acceptance workflows A–D from the specification, driven through the real app UI against a
// local fixture server that serves recorded NHTSA/EPA responses.
//
//   A  Look up a vehicle, see what's known and what isn't, save it, reopen it after a restart.
//   D  Create an inventory part with fitment, cost and a pricing rule; receive it; find it.
//   B  Decode a VIN, open Parts in Stock, see confirmed and conditional matches apart,
//      reserve a part, and check the available stock went down.
//   C  Research a component: Where Is It? in the vehicle's manual, the part's fitment, save it.
import assert from "node:assert/strict";
import { mkdtempSync, readFileSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";
import { bodyText, choose, click, fill, invoke, launch, nav, root, stopDisplay, waitText } from "./harness.mjs";
import { startFixtures } from "./fixture-server.mjs";

const dataDir = mkdtempSync(path.join(tmpdir(), "am-flows-"));
const fx = startFixtures();
let app = await launch({ dataDir, env: fx.env });
let b = app.browser;
const step = (s) => console.log(`  • ${s}`);

/** A one-page PDF whose text layer says `text` (enough for pdf.js and the search index). */
function tinyPdf(file, text) {
  const content = `BT /F1 14 Tf 72 720 Td (${text}) Tj ET`;
  const objs = [
    "<< /Type /Catalog /Pages 2 0 R >>",
    "<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
    "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents 4 0 R /Resources << /Font << /F1 5 0 R >> >> >>",
    `<< /Length ${content.length} >>\nstream\n${content}\nendstream`,
    "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>",
  ];
  let out = "%PDF-1.4\n";
  const offsets = [];
  objs.forEach((o, i) => {
    offsets.push(out.length);
    out += `${i + 1} 0 obj\n${o}\nendobj\n`;
  });
  const xref = out.length;
  out += `xref\n0 ${objs.length + 1}\n0000000000 65535 f \n${offsets.map((o) => `${String(o).padStart(10, "0")} 00000 n \n`).join("")}`;
  out += `trailer\n<< /Size ${objs.length + 1} /Root 1 0 R >>\nstartxref\n${xref}\n%%EOF\n`;
  writeFileSync(file, out);
}

try {
  // ------------------------------------------------------------------ Workflow A
  console.log("Workflow A: look up a vehicle");
  await waitText(b, "Dashboard");
  await nav(b, "Vehicle Lookup");
  await fill(b, "VIN", "1hgcm82633a004352");
  await waitText(b, "Check digit correct");
  await click(b, "Decode", { exact: true });
  await waitText(b, "Source: NHTSA vPIC");
  await waitText(b, "Not provided by NHTSA");
  let t = await bodyText(b);
  assert.match(t, /2003 Honda Accord EX-V6/);
  assert.match(t, /Not provided by NHTSA for this VIN:.*drivetrain/s, "missing values are named, not guessed");
  step("decoded with NHTSA vPIC; unavailable details listed");
  await app.shot("A1-decoded");
  await click(b, "Save vehicle");
  await waitText(b, "In My Vehicles");
  step("saved");
  await app.close();
  fx.close();
  // Reopen without any provider: everything comes from the local database.
  app = await launch({ dataDir });
  b = app.browser;
  await nav(b, "My Vehicles");
  await click(b, "Accord");
  await waitText(b, "Key specifications");
  await click(b, "Full Specifications");
  await waitText(b, "J30A4");
  t = await bodyText(b);
  assert.match(t, /Not available from free sources/);
  assert.match(t, /Decoded from VIN/);
  step("reopened after restart, offline, with sources and gaps shown");
  await app.shot("A2-reopened-specs");
  await app.close();

  // ------------------------------------------------------------------ Workflow D
  console.log("Workflow D: inventory and pricing");
  const fx2 = startFixtures();
  app = await launch({ dataDir, env: fx2.env });
  b = app.browser;
  await nav(b, "Pricing & Estimates");
  await click(b, "Add rule");
  await fill(b, "Percent", "40");
  await click(b, "Save", { exact: true });
  await waitText(b, "40% markup on cost");
  await waitText(b, "$70.00");
  step("pricing rule: 40% markup → $50 cost sells at $70.00");
  await nav(b, "Inventory");
  await click(b, "Add item");
  await fill(b, "Part name", "Front brake pad set");
  await choose(b, "Category", "Brake pads");
  await fill(b, "Brand", "Acme");
  await click(b, "Add a number");
  await fill(b, "Number", "ACM-1234");
  await fill(b, "SKU", "BP-100");
  await fill(b, "Purchase cost per unit", "50");
  await fill(b, "Quantity", "4");
  await fill(b, "Shelf / bin", "B-2");
  await click(b, "Add to inventory");
  await waitText(b, "Cost and price");
  t = await bodyText(b);
  assert.match(t, /4 on hand/);
  assert.match(t, /\$70\.00/);
  step("created part + item, received 4 at $50, price $70.00 from the profile");
  await app.shot("D1-item");
  await click(b, "More actions");
  await click(b, "Open catalog part");
  await waitText(b, "Fitment records");
  await click(b, "Add", { exact: true });
  await fill(b, "From year", "2016");
  await fill(b, "To year", "2019");
  await fill(b, "Make", "Nissan");
  await fill(b, "Model", "Sentra");
  await click(b, "Add condition");
  await fill(b, "Value", "1.8");
  await b.execute(() => {
    const cb = [...document.querySelectorAll("dialog input[type=checkbox]")].find((c) => c.parentElement.textContent.includes("verified"));
    cb.click();
  });
  await click(b, "Save", { exact: true });
  await waitText(b, "2016–2019 Nissan Sentra");
  await waitText(b, "Verified");
  step("fitment: 2016–2019 Nissan Sentra, engine 1.8 L, verified");
  // Find it by its number from the global search.
  await fill(b, "Search everything", "acm1234");
  await waitText(b, "Open this stock item");
  await b.keys(["Enter"]);
  await waitText(b, "Front brake pad set");
  step("found by part number through global search");
  await app.close();

  // ------------------------------------------------------------------ Workflow B
  console.log("Workflow B: parts in stock for a VIN");
  app = await launch({ dataDir, env: fx2.env });
  b = app.browser;
  // A second part that fits only with a detail the vehicle record doesn't have (setup).
  const pads = await invoke(b, "part_create", { input: { name: "Sport brake pads", category_id: "brakes.pads", brand: "Acme", identifiers: [] } });
  const sportItem = await invoke(b, "item_create", { partId: pads.id, newPart: null, input: { sku: "BP-SPORT", cost: "60", min_stock: 0, reorder_qty: 0, warranty: "", notes: "" } });
  await invoke(b, "stock_receive", { input: { item_id: sportItem.id, location_id: "loc-main", qty: 2, unit_cost: "60", bin: null, reason: "", ref_kind: null, ref_id: null } });
  await invoke(b, "fitment_add", { partId: pads.id, input: { kind: "fits", year_from: 2019, year_to: 2019, make: "Nissan", model: "Sentra", qualifiers: [{ key: "brake_package", value: "Sport" }], verified: true } });
  await nav(b, "Vehicle Lookup");
  await fill(b, "VIN", "3N1AB7AP4KY250873");
  await click(b, "Decode", { exact: true });
  await waitText(b, "2019 Nissan Sentra");
  await click(b, "Open full record");
  await click(b, "Parts in Stock");
  await waitText(b, "Front brake pad set");
  t = await bodyText(b);
  const confirmedAt = t.indexOf("A verified fitment record matches");
  const conditionalAt = t.indexOf("Fits some configurations");
  assert.ok(confirmedAt >= 0 && conditionalAt > confirmedAt, "confirmed and conditional matches are listed separately");
  assert.match(t, /Brake package/, "the detail that decides the conditional match is asked for");
  assert.match(t, /\$70\.00/);
  step("confirmed and conditional matches listed separately, with quantities and prices");
  await app.shot("B1-parts-in-stock");
  const before = (await invoke(b, "item_get", { id: (await invoke(b, "item_list", { query: { q: "BP-100" } }))[0].id })).item;
  await click(b, "Reserve");
  await fill(b, "Quantity", "1");
  await b.execute(() => [...document.querySelectorAll("dialog .dialog-foot button")].find((x) => x.textContent.trim() === "Reserve").click());
  await waitText(b, "Reserved 1 × BP-100");
  const after = (await invoke(b, "item_get", { id: before.id })).item;
  assert.equal(before.available, 4);
  assert.equal(after.available, 3);
  assert.equal(after.on_hand, 4, "reserving doesn't take stock off the shelf");
  await waitText(b, "1 reserved");
  step("reserved 1: available 4 → 3, on hand still 4");
  await app.shot("B2-reserved");

  // ------------------------------------------------------------------ Workflow C
  console.log("Workflow C: research a component");
  const pdf = path.join(dataDir, "sentra-service.pdf");
  tinyPdf(pdf, "Starter motor removal: the starter motor is below the intake manifold.");
  const vehicles = await invoke(b, "vehicle_recent", {});
  const sentra = vehicles.find((v) => v.model === "Sentra");
  const manual = await invoke(b, "manual_import", { path: pdf, meta: { title: "Sentra service manual", kind: "service", publisher: "", source_url: "", license_note: "" }, links: [{ vehicle_id: sentra.id }] });
  assert.ok(manual.id, JSON.stringify(manual));
  await nav(b, "Manuals & Docs");
  await b.execute(() => [...document.querySelectorAll(".gt-row")].find((r) => r.textContent.includes("Sentra service manual"))?.dispatchEvent(new MouseEvent("dblclick", { bubbles: true })));
  await click(b, "Search", { exact: true });
  await click(b, "Make searchable");
  await waitText(b, "is searchable", 30000);
  step("imported a manual and indexed its text");
  await nav(b, "My Vehicles");
  await click(b, "Recent lookups");
  await click(b, "Sentra");
  await click(b, "Manuals", { exact: true });
  await fill(b, "Component", "starter motor");
  await click(b, "Find", { exact: true });
  await waitText(b, "Sentra service manual, page 1");
  step("Where Is It? found the page in the vehicle's manual");
  await app.shot("C1-where-is-it");
  await click(b, "Sentra service manual, page 1");
  await waitText(b, "of 1");
  await app.shot("C2-viewer");
  await click(b, "← Library");
  await nav(b, "My Vehicles");
  await click(b, "Recent lookups");
  await click(b, "Sentra");
  await click(b, "Parts", { exact: true });
  await waitText(b, "Front brake pad set");
  await click(b, "Front brake pad set");
  await waitText(b, "Fitment records");
  await click(b, "Save to list");
  await fill(b, "List name", "Front brake job");
  await b.execute(() => [...document.querySelectorAll("dialog .dialog-foot button")].find((x) => x.textContent.trim() === "Save").click());
  await waitText(b, "Added to the list");
  step("viewed the part's fitment and saved it to a parts list");
  // Workflow D's last step, and C's alternative: add to an estimate.
  await click(b, "BP-100");
  await click(b, "Add to estimate");
  await b.execute(() => [...document.querySelectorAll("dialog .dialog-foot button")].find((x) => x.textContent.trim() === "Add").click());
  await waitText(b, "Added to estimate #1");
  const job = (await invoke(b, "job_list", { query: { status: null } }))[0];
  const detail = await invoke(b, "job_get", { id: job.id });
  assert.equal(detail.lines[0].unit_price, "70.00");
  assert.ok(detail.lines[0].explanation.steps.some((s) => s.includes("40% markup")));
  step("added to an estimate at $70.00 with the price explanation");
  console.log("Workflows A–D passed");

  // Accessibility of the pages now full of data.
  const AXE = readFileSync(path.join(root, "node_modules/axe-core/axe.min.js"), "utf8");
  await b.execute(AXE);
  const audit = async (where) => {
    await b.pause(600);
    const v = await b.execute(async () => {
      const r = await window.axe.run(document, { runOnly: { type: "tag", values: ["wcag2a", "wcag2aa", "wcag21a", "wcag21aa", "wcag22aa"] } });
      return r.violations.map((x) => `${x.id}: ${x.help} (${x.nodes.slice(0, 2).map((n) => n.target.join(" ")).join(" | ")})`);
    });
    assert.deepEqual(v, [], `${where}:\n${v.join("\n")}`);
  };
  await nav(b, "My Vehicles");
  await click(b, "Recent lookups");
  await click(b, "Sentra");
  for (const tab of ["Overview", "Full Specifications", "Parts", "Manuals", "Maintenance", "Recalls & Safety", "Parts in Stock", "Service History"]) {
    await click(b, tab, { exact: true });
    await audit(`vehicle · ${tab}`);
  }
  await app.shot("E-recalls");
  await nav(b, "Dashboard");
  await click(b, "#1 Untitled");
  await waitText(b, "Parts and labor");
  await audit("job");
  await app.shot("E-job");
  step("vehicle tabs and job page pass the WCAG A/AA audit with data");

  // Attachments. WebDriver can't drive the native file picker, so the file goes in through the call it feeds.
  const photo = path.join(mkdtempSync(path.join(tmpdir(), "am-photo-")), "brake-pads.png");
  writeFileSync(photo, Buffer.from("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==", "base64"));
  await invoke(b, "attachment_add", { ownerKind: "job", ownerId: job.id, path: photo, caption: "Worn pads" });
  await nav(b, "Inventory");
  await b.pause(5500); // past the query cache's 5 s freshness, so the job page reloads its files
  await nav(b, "Dashboard");
  await click(b, "#1 Untitled");
  await waitText(b, "brake-pads.png");
  await b.waitUntil(() => b.execute(() => document.querySelector('img[alt="Worn pads"]')?.naturalWidth === 1), { timeout: 10000, timeoutMsg: "photo thumbnail didn't load" });
  await click(b, "Remove brake-pads.png");
  await b.waitUntil(async () => (await invoke(b, "attachment_list", { ownerKind: "job", ownerId: job.id })).length === 0, { timeout: 10000, timeoutMsg: "photo wasn't removed" });
  step("photo attached to the job, shown as a thumbnail, and removed");

  // Saved searches.
  await nav(b, "Inventory");
  await fill(b, "Search inventory", "BP-100");
  await click(b, "Save this search");
  await fill(b, "Saved search name", "Brake pads");
  await click(b, "Save", { exact: true });
  await waitText(b, "Search saved");
  await fill(b, "Search inventory", "");
  await click(b, "Brake pads", { exact: true });
  await b.waitUntil(() => b.execute(() => document.querySelector('[aria-label="Search inventory"]').value === "BP-100"), { timeout: 10000, timeoutMsg: "the saved search didn't restore the filter" });
  step("saved an inventory search and brought it back");
  fx2.close();
} finally {
  await app.close();
  stopDisplay();
}
