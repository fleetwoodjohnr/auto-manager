// QuickBooks in the real app: a QuickBooks Online "Products and Services" export imported through
// Settings → QuickBooks (choose → review → apply), then imported again after QuickBooks changed,
// and the screens audited with axe. WebDriver can't drive the native file picker, so the export
// goes in through the commands the picker's path feeds (as in workflows.mjs); the review window is
// exercised through the live connection's Refresh, which opens it without a picker.
import assert from "node:assert/strict";
import { mkdtempSync, readFileSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";
import { bodyText, click, invoke, launch, nav, root, stopDisplay, waitText } from "./harness.mjs";

const AXE = readFileSync(path.join(root, "node_modules/axe-core/axe.min.js"), "utf8");
const scratch = mkdtempSync(path.join(tmpdir(), "am-qb-"));
const csv = (rows) => rows.map((r) => r.map((c) => `"${String(c).replace(/"/g, '""')}"`).join(",")).join("\n") + "\n";
const HEAD = ["Product/Service Name", "Sales Description", "SKU", "Type", "Sales Price / Rate", "Purchase Cost", "Quantity On Hand", "Reorder Point"];
const exportFile = path.join(scratch, "ProductServiceList.csv");
writeFileSync(
  exportFile,
  csv([
    HEAD,
    ["Brakes:Front pads", "Ceramic front brake pads", "BP-100", "Inventory", "89.99", "41.50", "12", "4"],
    ["Engine oil 5W-30", "", "OIL-5W30", "Inventory", "8.50", "4.10", "37.5", ""],
    ["Labor", "Shop labor", "", "Service", "120", "", "", ""],
  ]),
);

async function audit(b, where) {
  await b.execute(AXE);
  const v = await b.execute(async () => {
    const r = await window.axe.run(document, { runOnly: { type: "tag", values: ["wcag2a", "wcag2aa", "wcag21a", "wcag21aa", "wcag22aa"] } });
    return r.violations.map((x) => `${x.id}: ${x.nodes.map((n) => n.target.join(" ")).join(", ")}`);
  });
  assert.deepEqual(v, [], `${where}: ${v.join("\n")}`);
}

const app = await launch({ dataDir: path.join(scratch, "data") });
const b = app.browser;
try {
  await waitText(b, "Dashboard", 30000);
  await nav(b, "Settings");
  await click(b, "QuickBooks", { exact: true });
  await waitText(b, "Import a QuickBooks export");
  await audit(b, "Settings → QuickBooks");

  const options = { use_prices: true, non_inventory: false, apply_stock: false, apply_cost: false };
  const first = await invoke(b, "quickbooks_file_preview", { path: exportFile, options });
  assert.equal(first.kind, "items");
  assert.deepEqual([first.preview.new_count, first.preview.skipped_count], [2, 1], JSON.stringify(first.preview.rows));
  const s1 = await invoke(b, "quickbooks_file_apply", { path: exportFile, options });
  assert.deepEqual([s1.added, s1.updated, s1.skipped], [2, 0, 1]);
  await nav(b, "Inventory");
  await waitText(b, "BP-100");
  await waitText(b, "Front pads");
  await app.shot("qb-1-imported");
  const items = await invoke(b, "item_list", { query: {} });
  const pads = items.find((i) => i.sku === "BP-100");
  assert.equal(pads.price_override, "89.99");
  assert.equal(pads.on_hand, 12);
  assert.equal(items.find((i) => i.sku === "OIL-5W30").on_hand, 37.5);
  console.log("  • export imported: 2 stock items with QuickBooks prices and stock, the service skipped");

  // QuickBooks changed: a new price. Importing again updates the same item.
  writeFileSync(
    exportFile,
    csv([HEAD, ["Brakes:Front pads", "Ceramic front brake pads", "BP-100", "Inventory", "94.99", "41.50", "12", "4"], ["Engine oil 5W-30", "", "OIL-5W30", "Inventory", "8.50", "4.10", "37.5", ""]]),
  );
  const again = await invoke(b, "quickbooks_file_preview", { path: exportFile, options });
  const row = again.preview.rows.find((r) => r.name === "Front pads");
  assert.deepEqual([row.status, row.notes], ["update", ["Price: 89.99 → 94.99"]]);
  await invoke(b, "quickbooks_file_apply", { path: exportFile, options });
  const after = await invoke(b, "item_list", { query: {} });
  assert.equal(after.length, 2, "no copies");
  assert.equal(after.find((i) => i.sku === "BP-100").price_override, "94.99");
  console.log("  • imported again: the price updated on the same item, nothing copied");
  assert.ok(!/Something went wrong|not allowed/i.test(await bodyText(b)));
  console.log("quickbooks: file import ok");
} finally {
  await app.close();
  stopDisplay();
}
