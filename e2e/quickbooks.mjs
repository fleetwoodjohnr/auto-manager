// QuickBooks in the real app.
//  1. A QuickBooks Online "Products and Services" export imported, then imported again after
//     QuickBooks changed.
//  2. The live QuickBooks Online connection against a local stand-in for Intuit: Connect → the
//     sign-in answer comes back to the app's one-time listener → review → apply → QuickBooks
//     changes → Bring in again → update → Disconnect (the sign-in revoked).
// Screens audited with axe. WebDriver can't drive the native file picker, so the export
// goes in through the commands the picker's path feeds (as in workflows.mjs); the review window is
// exercised through the live connection's Refresh, which opens it without a picker.
import assert from "node:assert/strict";
import { mkdtempSync, readFileSync, writeFileSync } from "node:fs";
import http from "node:http";
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

// ---------------------------------------------------------------- a stand-in for Intuit
const intuit = { price: 89.99, revoked: [], tokens: [], apiMethods: new Set() };
const ITEMS = () => [
  { Id: "11", Name: "Front pads", FullyQualifiedName: "Brakes:Front pads", Sku: "QB-PADS", Type: "Inventory", UnitPrice: intuit.price, PurchaseCost: 41.5, QtyOnHand: 6, ReorderPoint: 2, Active: true, PrefVendorRef: { value: "56", name: "Parts Plus" } },
  { Id: "12", Name: "Diagnostic", FullyQualifiedName: "Diagnostic", Type: "Service", UnitPrice: 120, Active: true },
  { Id: "13", Name: "Brakes", FullyQualifiedName: "Brakes", Type: "Category", Active: true },
];
const json = (res, status, body) => res.writeHead(status, { "Content-Type": "application/json" }).end(JSON.stringify(body));
const server = http
  .createServer((req, res) => {
    const url = new URL(req.url, "http://127.0.0.1");
    let body = "";
    req.on("data", (d) => (body += d));
    req.on("end", () => {
      if (url.pathname === "/authorize") {
        // Intuit after the person signs in and picks the company.
        const back = new URL(url.searchParams.get("redirect_uri"));
        back.search = new URLSearchParams({ code: "C0DE", state: url.searchParams.get("state"), realmId: "9130" }).toString();
        return res.writeHead(302, { Location: back.toString() }).end();
      }
      const auth = req.headers.authorization ?? "";
      if (url.pathname === "/token") {
        assert.equal(auth, "Basic " + Buffer.from("e2e-client:e2e-secret").toString("base64"));
        intuit.tokens.push(new URLSearchParams(body).get("grant_type"));
        return json(res, 200, { access_token: "access-1", refresh_token: "refresh-1", token_type: "bearer", expires_in: 3600 });
      }
      if (url.pathname === "/revoke") {
        intuit.revoked.push(JSON.parse(body).token);
        return res.writeHead(200).end();
      }
      if (url.pathname.startsWith("/v3/company/9130/")) {
        intuit.apiMethods.add(req.method);
        if (auth !== "Bearer access-1") return res.writeHead(401).end();
        if (url.pathname.endsWith("/companyinfo/9130")) return json(res, 200, { CompanyInfo: { CompanyName: "Sandbox Garage" } });
        const q = url.searchParams.get("query") ?? "";
        const entity = /from (\w+)/.exec(q)?.[1];
        const rows = {
          Item: ITEMS(),
          Customer: [{ Id: "58", DisplayName: "Ada Lovelace", PrimaryPhone: { FreeFormNumber: "555-0100" }, Active: true }],
          Vendor: [{ Id: "56", DisplayName: "Parts Plus", AcctNum: "A-77", Active: true }],
        }[entity];
        return json(res, 200, { QueryResponse: { [entity]: rows } });
      }
      res.writeHead(404).end();
    });
  })
  .listen(0, "127.0.0.1");
await new Promise((r) => server.once("listening", r));
const base = `http://127.0.0.1:${server.address().port}`;

const app = await launch({
  dataDir: path.join(scratch, "data"),
  env: {
    AUTO_MANAGER_QBO_CLIENT_ID: "e2e-client",
    AUTO_MANAGER_QBO_CLIENT_SECRET: "e2e-secret",
    AUTO_MANAGER_QBO_AUTH_URL: `${base}/authorize`,
    AUTO_MANAGER_QBO_TOKEN_URL: `${base}/token`,
    AUTO_MANAGER_QBO_REVOKE_URL: `${base}/revoke`,
    AUTO_MANAGER_QBO_API_BASE: base,
    // Straight back to the app (Intuit sandbox allows localhost); live sign-ins go through the HTTPS page.
    AUTO_MANAGER_QBO_REDIRECT: "http://127.0.0.1:{port}/quickbooks/callback",
    AUTO_MANAGER_QBO_NO_BROWSER: "1",
    AUTO_MANAGER_SECRET_STORE: "memory",
    AUTO_MANAGER_ALLOW_HTTP_LOCALHOST: "1",
  },
});
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

  // ---------------------------------------------------------------- QuickBooks Online
  await nav(b, "Settings");
  await click(b, "QuickBooks", { exact: true });
  await waitText(b, "Sign in with your QuickBooks account");
  await click(b, "Connect to QuickBooks");
  await waitText(b, "Finish signing in to QuickBooks in your browser");
  await audit(b, "QuickBooks sign-in dialog");
  let signIn = null;
  await b.waitUntil(async () => (signIn = await invoke(b, "quickbooks_sign_in_url")), { timeout: 10000 });
  assert.ok(signIn.includes("scope=com.intuit.quickbooks.accounting"), signIn);
  // The browser: Intuit's page redirects back to the app's listener on this computer.
  const back = await fetch(signIn, { redirect: "follow" });
  assert.match(await back.text(), /connected to QuickBooks/);
  await waitText(b, "Bring in from Sandbox Garage", 20000);
  await waitText(b, "3 to add");
  await waitText(b, "Service: Auto Manager prices labor");
  await audit(b, "QuickBooks Online review");
  await app.shot("qb-3-online-review");
  await click(b, "Apply changes");
  await waitText(b, "QuickBooks: 3 added, 0 updated");
  await waitText(b, "Connected to Sandbox Garage");
  let qbItem = (await invoke(b, "item_list", { query: {} })).find((i) => i.sku === "QB-PADS");
  assert.deepEqual([qbItem.price_override, qbItem.on_hand, qbItem.supplier_name], ["89.99", 6, "Parts Plus"]);
  console.log("  • QuickBooks Online: signed in through the local listener, 3 records added (item, customer, vendor)");

  intuit.price = 92.5;
  await click(b, "Bring in from QuickBooks…");
  await waitText(b, "Price: 89.99 → 92.5", 20000);
  await click(b, "Apply changes");
  await waitText(b, "QuickBooks: 0 added, 1 updated");
  qbItem = (await invoke(b, "item_list", { query: {} })).find((i) => i.sku === "QB-PADS");
  assert.equal(qbItem.price_override, "92.5");
  console.log("  • brought in again: the price change updated the same item");

  await b.waitUntil(() => b.execute(() => !document.querySelector("dialog[open]")), { timeout: 10000 });
  await click(b, "Disconnect");
  await waitText(b, "Disconnect QuickBooks?");
  await click(b, "Disconnect", { exact: true });
  await waitText(b, "Sign in with your QuickBooks account");
  assert.deepEqual(intuit.revoked, ["refresh-1"], "the sign-in was revoked at Intuit");
  assert.deepEqual([...intuit.apiMethods], ["GET"], "only reads reached the QuickBooks API");
  assert.equal((await invoke(b, "item_list", { query: {} })).length, 3, "imported records stay");
  console.log("  • disconnected: sign-in revoked, imported records kept");
  console.log("quickbooks: file import and QuickBooks Online ok");
} finally {
  await app.close();
  server.close();
  stopDisplay();
}
