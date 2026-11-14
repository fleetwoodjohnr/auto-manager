// Two-way sync in the real app: two computers (two data folders, run one after the other since
// only one copy of the app runs at a time) share one folder. A vehicle saved on the first appears
// on the second; a customer added on the second appears on the first.
import assert from "node:assert/strict";
import { mkdtempSync, readdirSync, readFileSync } from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";
import { click, fill, invoke, launch, nav, stopDisplay, waitText } from "./harness.mjs";

const base = mkdtempSync(path.join(tmpdir(), "am-sync-"));
const shared = mkdtempSync(path.join(tmpdir(), "am-shared-"));
const step = (s) => console.log(`  • ${s}`);

async function setUp(b, name) {
  await nav(b, "Settings");
  await click(b, "Data and backups");
  await fill(b, "Shared folder", shared);
  await fill(b, "Passphrase", "correct horse battery");
  await fill(b, "This computer's name", name);
  await click(b, "Start syncing");
  await waitText(b, `${name} syncs with`);
}

let app;
try {
  console.log("Sync between computers");
  app = await launch({ dataDir: path.join(base, "front-desk") });
  let b = app.browser;
  await waitText(b, "Dashboard");
  await setUp(b, "Front desk");
  const v = await invoke(b, "vehicle_create_manual", { input: { vin: null, year: 2019, make: "Nissan", model: "Sentra", trim: "SV", specs: [] } });
  await invoke(b, "vehicle_set_saved", { id: v.id, saved: true });
  await click(b, "Sync now");
  await waitText(b, "Last synced");
  step("first computer set up the shared folder and synced a saved vehicle");
  await app.close();

  app = await launch({ dataDir: path.join(base, "workshop") });
  b = app.browser;
  await waitText(b, "Dashboard");
  await setUp(b, "Workshop");
  await waitText(b, "received from Front desk");
  await nav(b, "My Vehicles");
  await waitText(b, "Sentra");
  step("second computer joined with the same passphrase and received the vehicle");
  await invoke(b, "customer_save", { id: null, input: { name: "Grace Hopper", phone: "", email: "", address: "", notes: "", discount_pct: null, pricing_profile_id: null } });
  await nav(b, "Settings");
  await click(b, "Data and backups");
  await click(b, "Sync now");
  await waitText(b, "Other computers: Front desk");
  await app.close();

  app = await launch({ dataDir: path.join(base, "front-desk") });
  b = app.browser;
  await waitText(b, "Dashboard");
  await nav(b, "Settings");
  await click(b, "Data and backups");
  await click(b, "Sync now");
  await waitText(b, "received from Workshop");
  const customers = await invoke(b, "customer_list", { q: null });
  assert.ok(customers.some((c) => c.name === "Grace Hopper"), "customer from the second computer");
  step("first computer received the customer added on the second");

  // Nothing readable in the shared folder.
  const files = readdirSync(shared, { recursive: true, withFileTypes: true })
    .filter((d) => d.isFile() && !d.name.endsWith(".json"))
    .map((d) => path.join(d.parentPath, d.name));
  assert.ok(files.length >= 2, "both computers wrote their state");
  for (const f of files) {
    const bytes = readFileSync(f);
    assert.ok(!bytes.includes("Sentra") && !bytes.includes("Grace"), `${f} is readable`);
  }
  step("the shared folder holds only encrypted data");
  console.log("Sync between computers passed");
} finally {
  await app?.close();
  stopDisplay();
}
