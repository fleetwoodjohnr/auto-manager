// Screenshots for the README (docs/screenshots/), taken from the real app with a small sample shop.
// The sample stock comes in through the QuickBooks import, vehicles are decoded from NHTSA's
// recorded answers (e2e fixtures), and QuickBooks Online is a local stand-in for Intuit with a
// made-up "Sample Auto Shop" company. Names of people and the company are invented.
//   node e2e/readme-shots.mjs
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import http from "node:http";
import { tmpdir } from "node:os";
import path from "node:path";
import { choose, click, fill, invoke, launch, nav, root, stopDisplay, waitText } from "./harness.mjs";
import { startFixtures } from "./fixture-server.mjs";

const OUT = path.join(root, "docs/screenshots");
mkdirSync(OUT, { recursive: true });
const scratch = mkdtempSync(path.join(tmpdir(), "am-shots-"));

// ---------------------------------------------------------------- sample QuickBooks data
// [QuickBooks name, SKU, sales price, cost, on hand, reorder point, preferred vendor]
const PARTS = [
  ["Brake pads:Ceramic front brake pad set", "BP-2041", 79.99, 38.5, 6, 2, "Metro Parts Supply"],
  ["Rotors:Front brake rotor", "RT-5530", 64.99, 31.2, 4, 2, "Metro Parts Supply"],
  ["Oil filters:Spin-on oil filter", "OF-1001", 9.49, 3.85, 24, 10, "Metro Parts Supply"],
  ["Engine oil:Full synthetic 0W-20 (quart)", "OIL-0W20", 8.99, 4.1, 42.5, 20, "Lubricant Wholesale Co."],
  ["Air filters and intake:Engine air filter", "AF-2207", 21.99, 9.6, 1, 3, "Metro Parts Supply"],
  ["Cabin air filters:Cabin air filter", "CF-0912", 24.99, 10.25, 8, 3, "Metro Parts Supply"],
  ["Wiper blades:Wiper blade 22 in", "WB-22", 18.99, 7.4, 12, 6, "Metro Parts Supply"],
  ["Spark plugs:Iridium spark plug", "SP-IR16", 12.99, 6.2, 16, 8, "Metro Parts Supply"],
  ["Batteries:Battery group 35", "BAT-35", 189.99, 112.0, 3, 2, "Metro Parts Supply"],
];
const intuit = {
  Item: [
    ...PARTS.map(([name, sku, price, cost, qty, reorder, vendor], i) => ({
      Id: String(100 + i),
      Name: name.split(":")[1],
      FullyQualifiedName: name,
      Sku: sku,
      Type: "Inventory",
      UnitPrice: price,
      PurchaseCost: cost,
      QtyOnHand: qty,
      ReorderPoint: reorder,
      Active: true,
      PrefVendorRef: { value: vendor === "Metro Parts Supply" ? "56" : "57", name: vendor },
    })),
    { Id: "200", Name: "Diagnostic inspection", FullyQualifiedName: "Diagnostic inspection", Type: "Service", UnitPrice: 129, Active: true },
    { Id: "201", Name: "Brake pads", FullyQualifiedName: "Brake pads", Type: "Category", Active: true },
  ],
  Customer: [
    { Id: "1", DisplayName: "Maria Lopez", PrimaryPhone: { FreeFormNumber: "(555) 010-2231" }, PrimaryEmailAddr: { Address: "maria.lopez@example.com" }, Active: true },
    { Id: "2", DisplayName: "Dan Whitfield", PrimaryPhone: { FreeFormNumber: "(555) 010-7781" }, Active: true },
    { Id: "3", DisplayName: "Priya Shah", PrimaryEmailAddr: { Address: "priya.shah@example.com" }, Active: true },
  ],
  Vendor: [
    { Id: "56", DisplayName: "Metro Parts Supply", AcctNum: "SAS-4410", PrimaryPhone: { FreeFormNumber: "(555) 010-4400" }, Active: true },
    { Id: "57", DisplayName: "Lubricant Wholesale Co.", AcctNum: "77-102", Active: true },
  ],
};
const json = (res, body) => res.writeHead(200, { "Content-Type": "application/json" }).end(JSON.stringify(body));
const server = http
  .createServer((req, res) => {
    const url = new URL(req.url, "http://127.0.0.1");
    req.resume();
    req.on("end", () => {
      if (url.pathname === "/authorize") {
        const back = new URL(url.searchParams.get("redirect_uri"));
        back.search = new URLSearchParams({ code: "C", state: url.searchParams.get("state"), realmId: "4620816365" }).toString();
        return res.writeHead(302, { Location: back.toString() }).end();
      }
      if (url.pathname === "/token") return json(res, { access_token: "a", refresh_token: "r", token_type: "bearer", expires_in: 3600 });
      if (url.pathname === "/revoke") return res.writeHead(200).end();
      if (url.pathname.endsWith("/companyinfo/4620816365")) return json(res, { CompanyInfo: { CompanyName: "Sample Auto Shop" } });
      const entity = /from (\w+)/.exec(url.searchParams.get("query") ?? "")?.[1];
      if (entity) return json(res, { QueryResponse: { [entity]: intuit[entity] } });
      res.writeHead(404).end();
    });
  })
  .listen(0, "127.0.0.1");
await new Promise((r) => server.once("listening", r));
const base = `http://127.0.0.1:${server.address().port}`;

const fx = startFixtures();
const app = await launch({
  dataDir: path.join(scratch, "data"),
  env: {
    ...fx.env,
    AUTO_MANAGER_QBO_AUTH_URL: `${base}/authorize`,
    AUTO_MANAGER_QBO_TOKEN_URL: `${base}/token`,
    AUTO_MANAGER_QBO_REVOKE_URL: `${base}/revoke`,
    AUTO_MANAGER_QBO_API_BASE: base,
    AUTO_MANAGER_QBO_REDIRECT: "http://127.0.0.1:{port}/quickbooks/callback",
    AUTO_MANAGER_QBO_NO_BROWSER: "1",
    AUTO_MANAGER_SECRET_STORE: "memory",
    AUTO_MANAGER_ALLOW_HTTP_LOCALHOST: "1",
  },
});
const b = app.browser;
const shot = async (name) => {
  // Dismiss leftover toasts (they'd cover the bottom of the page).
  await b.execute(() => document.querySelectorAll(".toast button").forEach((x) => x.click()));
  await b.pause(600);
  await b.saveScreenshot(path.join(OUT, `${name}.png`));
  console.log(`  • ${name}.png`);
};
const settle = () => b.waitUntil(() => b.execute(() => !document.querySelector("dialog[open]")), { timeout: 10000 });

try {
  await waitText(b, "Dashboard", 30000);
  const theme = async (t) => {
    await invoke(b, "settings_save", { input: { ...(await invoke(b, "settings_get")), theme: t, onboarded: true } });
    await b.execute(() => location.reload());
    await waitText(b, "Dashboard", 30000);
    await b.waitUntil(() => b.execute((x) => document.documentElement.dataset.theme === x, t), { timeout: 10000 });
  };
  await theme("light");

  // QuickBooks Online: the Intuit app's keys, connect, review, apply. The redirect address shown
  // is the real one (the stand-in's goes to the app directly).
  await nav(b, "Settings");
  await click(b, "QuickBooks", { exact: true });
  await waitText(b, "needs the keys of an Intuit developer app");
  await choose(b, "Keys for", "real company");
  await fill(b, "Client ID", "ABsampleClientId0000000000000000000000000000000");
  await fill(b, "Client Secret", "sample-secret");
  await b.execute(() => {
    document.querySelector('input[aria-label="Redirect URI"]').value = "https://fleetwoodjohnr.github.io/auto-manager/quickbooks/";
  });
  await shot("quickbooks-keys");
  await click(b, "Save keys");
  await waitText(b, "Sign in with your QuickBooks account");
  await click(b, "Connect to QuickBooks");
  let url = null;
  await b.waitUntil(async () => (url = await invoke(b, "quickbooks_sign_in_url")), { timeout: 10000 });
  await (await fetch(url)).text();
  await waitText(b, "Bring in from Sample Auto Shop", 20000);
  await waitText(b, "14 to add");
  await b.execute(() => document.querySelector("dialog .table-wrap")?.scrollTo(0, 0));
  await shot("quickbooks-review");
  await click(b, "Apply changes");
  await waitText(b, "QuickBooks: 14 added");
  await settle();

  // Vehicles (NHTSA's recorded answers), each with its customer.
  const customers = await invoke(b, "customer_list", {});
  const who = (name) => customers.find((c) => c.name === name).id;
  for (const [vin, owner, mileage, plate] of [
    ["3N1AB7AP4KY250873", "Maria Lopez", 48210, "8KLM302"],
    ["1HGCM82633A004352", "Dan Whitfield", 162400, "4TRX118"],
  ]) {
    const d = await invoke(b, "decode_vin", { vin, modelYear: null, refresh: null });
    await invoke(b, "vehicle_set_saved", { id: d.vehicle.id, saved: true });
    await invoke(b, "vehicle_update", {
      id: d.vehicle.id,
      input: { nickname: "", color: "", plate, mileage, mileage_unit: "mi", purchase_date: null, purchase_price: null, notes: "", customer_id: who(owner) },
    });
  }
  const sentra = (await invoke(b, "vehicle_list", { saved: true })).find((v) => v.model === "Sentra");

  // Fitment for the pads and the rotor, so the Sentra's Parts in Stock has confirmed matches.
  const items = await invoke(b, "item_list", { query: {} });
  const item = (sku) => items.find((i) => i.sku === sku);
  for (const sku of ["BP-2041", "RT-5530"]) {
    await invoke(b, "fitment_add", { partId: item(sku).part_id, input: { kind: "fits", year_from: 2016, year_to: 2019, make: "Nissan", model: "Sentra", qualifiers: [], verified: true } });
  }
  await invoke(b, "fitment_add", { partId: item("OF-1001").part_id, input: { kind: "fits", year_from: 2013, year_to: 2019, make: "Nissan", model: "Sentra", qualifiers: [{ key: "engine_displacement_l", value: "1.8" }], verified: true } });

  // An estimate for the Sentra.
  const job = await invoke(b, "job_create", {
    input: { customer_id: who("Maria Lopez"), vehicle_id: sentra.id, title: "Front brakes", complaint: "Squealing from the front when braking", notes: "", pricing_profile_id: null, mileage: 48210, technician: "Sam" },
  });
  await invoke(b, "job_add_part", { id: job.job.id, input: { item_id: item("BP-2041").id, qty: "1", reserve: true } });
  await invoke(b, "job_add_part", { id: job.job.id, input: { item_id: item("RT-5530").id, qty: "2", reserve: true } });
  await invoke(b, "job_add_other", { id: job.job.id, input: { kind: "labor", description: "Replace front pads and rotors", qty: "1.5", unit_price: "120.00", taxable: true } });
  await invoke(b, "job_add_other", { id: job.job.id, input: { kind: "fee", description: "Shop supplies", qty: "1", unit_price: "12.50", taxable: true } });

  // Screens.
  await nav(b, "Vehicle Lookup");
  await fill(b, "VIN", "3N1AB7AP4KY250873");
  await click(b, "Decode", { exact: true });
  await waitText(b, "2019 Nissan Sentra");
  await shot("vehicle-lookup");

  await click(b, "Open full record");
  await click(b, "Parts in Stock");
  await waitText(b, "Ceramic front brake pad set");
  await shot("parts-in-stock");

  await nav(b, "Inventory");
  await waitText(b, "OIL-0W20");
  await shot("inventory");

  await nav(b, "Dashboard");
  await click(b, "#1 Front brakes");
  await waitText(b, "Replace front pads and rotors");
  await shot("estimate");

  await nav(b, "Dashboard");
  await waitText(b, "Sentra");
  await shot("dashboard");

  await theme("dark");
  await waitText(b, "Sentra");
  await shot("dashboard-dark");
} finally {
  await app.close();
  fx.close();
  server.close();
  stopDisplay();
  rmSync(scratch, { recursive: true, force: true });
}
