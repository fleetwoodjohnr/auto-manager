// The OBD-II scanner in the real app, against the stand-in ELM327 on a pseudo-terminal
// (crates/obd/tests/elm327_sim.py): the person types the port, scans, and sees the codes, VIN,
// freeze frame and live values; the "car" never receives anything but reading requests.
import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { mkdtempSync } from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";
import { createInterface } from "node:readline";
import { bodyText, click, fill, launch, nav, root, stopDisplay, waitText } from "./harness.mjs";

const step = (s) => console.log(`  • ${s}`);
const sim = spawn("python3", [path.join(root, "crates/obd/tests/elm327_sim.py")], { stdio: ["pipe", "pipe", "inherit"] });
const lines = createInterface({ input: sim.stdout });
const sent = [];
const port = await new Promise((resolve) => lines.once("line", resolve));
lines.on("line", (l) => l.startsWith("CMD ") && sent.push(l.slice(4)));

const app = await launch({ dataDir: mkdtempSync(path.join(tmpdir(), "am-obd-")) });
const b = app.browser;
try {
  console.log("OBD-II scanner");
  await waitText(b, "Dashboard");
  await nav(b, "Maintenance & Service");
  await click(b, "OBD-II scanner", { exact: true });
  await fill(b, "Adapter port", port);
  await click(b, "Scan", { exact: true });
  await waitText(b, "P0133", 60000);
  const text = await bodyText(b);
  for (const want of ["P0300", "1HGCM82633A004352", "ELM327 v1.5", "ISO 15765-4 CAN", "1726"]) assert.ok(text.includes(want), `missing ${want}`);
  step("scanned through a serial port: stored codes, VIN, protocol and live values shown");
  await app.shot("O1-obd-scan");
  assert.ok(sent.length > 10, "the scan talked to the adapter");
  assert.ok(!sent.some((c) => c.replace(/ /g, "") === "04"), `codes were never cleared: ${sent}`);
  assert.ok(sent.every((c) => /^(AT|01|02)/.test(c) || ["03", "07", "0902"].includes(c)), `only reading requests: ${sent}`);
  step(`the car received only setup and reading requests (${sent.length} commands, never “clear codes”)`);
  console.log("OBD-II scanner passed");
} finally {
  await app.close();
  stopDisplay();
  sim.stdin.end();
}
