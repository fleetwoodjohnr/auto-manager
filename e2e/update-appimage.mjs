// Workflow E, locally: in-app update of a Linux AppImage against a local "GitHub" (adapted from
// Salon Resource Manager's test).
//   - an interrupted download and a badly signed package leave the installed AppImage untouched;
//   - a good, signed package replaces it after "Install and restart" (the Simple Suite flow:
//     Check now -> notice -> Install and restart), a pre-update backup is made, and the app
//     starts as the new version with the data intact.
// Build the images first:  e2e/build-update-images.sh <dir>
// Then:  APPIMAGE_EXTRACT_AND_RUN=1 node e2e/update-appimage.mjs <dir> [old] [new]
import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { createHash } from "node:crypto";
import { chmodSync, copyFileSync, mkdtempSync, readFileSync, readdirSync, readlinkSync, rmSync } from "node:fs";
import http from "node:http";
import { tmpdir } from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { bodyText, click, invoke, launch, nav, stopDisplay, waitText } from "./harness.mjs";

const sha = (f) => createHash("sha256").update(readFileSync(f)).digest("hex");

if (process.argv[2] === "--verify") {
  // Second process: start the replaced AppImage and check the version and the data.
  const [, , , bin, dataDir, version] = process.argv;
  const app = await launch({ dataDir, bin, port: 4644 });
  const b = app.browser;
  try {
    await waitText(b, "Dashboard", 30000);
    const info = await invoke(b, "app_info");
    assert.equal(info.version, version, JSON.stringify(info));
    const saved = await invoke(b, "vehicle_list", { saved: true });
    assert.equal(saved.length, 1, "the saved vehicle survived the update");
    assert.equal(saved[0].model, "Beetle");
    await app.shot("E3-restarted-new-version");
    console.log(`restarted as ${info.version} with the saved vehicle intact`);
  } finally {
    await app.close();
    stopDisplay();
  }
  process.exit(0);
}

const [dir, oldV = "0.1.0", newV = "0.1.1"] = process.argv.slice(2);
const oldImage = path.join(dir, `auto-manager-${oldV}.AppImage`);
const newImage = path.join(dir, `auto-manager-${newV}.AppImage`);
const scratch = mkdtempSync(path.join(tmpdir(), "am-update-"));
const work = path.join(scratch, "Auto-Manager.AppImage");
const dataDir = path.join(scratch, "data");
copyFileSync(oldImage, work);
chmodSync(work, 0o755);
const oldSha = sha(work);
const newBytes = readFileSync(newImage);

// One check, then three installs: the download is cut short, then it isn't the signed file,
// then it's the genuine package.
let mode = "good";
const oldBytes = readFileSync(oldImage);
const server = http
  .createServer((req, res) => {
    if (req.url === "/latest.json") {
      const signature = readFileSync(`${newImage}.sig`, "utf8").trim();
      return res
        .writeHead(200, { "Content-Type": "application/json" })
        .end(JSON.stringify({ version: newV, notes: "Test release notes", pub_date: "2026-11-14T00:00:00Z", platforms: { "linux-x86_64-appimage": { signature, url: "http://127.0.0.1:8765/new.AppImage" } } }));
    }
    if (req.url === "/new.AppImage") {
      const body = mode === "badsig" ? oldBytes : newBytes;
      res.writeHead(200, { "Content-Type": "application/octet-stream", "Content-Length": body.length });
      if (mode === "truncate") {
        res.write(body.subarray(0, Math.floor(body.length / 3)));
        return setTimeout(() => res.destroy(), 200);
      }
      return res.end(body);
    }
    res.writeHead(404).end();
  })
  .listen(8765, "127.0.0.1");

async function failedInstall(b) {
  await click(b, "Install and restart");
  // The previous attempt's error clears while downloading.
  await b.waitUntil(async () => (await bodyText(b)).includes("Downloading…"), { timeout: 30000, interval: 50 });
  await waitText(b, "Update failed", 120000);
}

const app = await launch({ dataDir, bin: work });
const b = app.browser;
try {
  await waitText(b, "Dashboard", 30000);
  assert.equal((await invoke(b, "app_info")).version, oldV);
  await invoke(b, "vehicle_create_manual", { input: { year: 1972, make: "Volkswagen", model: "Beetle", specs: [] } });

  await nav(b, "Settings");
  await click(b, "Updates", { exact: true });
  await click(b, "Check now");
  await waitText(b, `Auto Manager ${newV} is available.`, 30000);
  await waitText(b, `What's new in ${newV}`);
  console.log("  • update found, notice shown");

  mode = "truncate";
  await failedInstall(b);
  assert.equal(sha(work), oldSha, "an interrupted download changed the AppImage");
  await app.shot("E1-interrupted");
  console.log("  • interrupted download refused, nothing changed");

  mode = "badsig";
  await failedInstall(b);
  assert.equal(sha(work), oldSha, "a package that doesn't match its signature changed the AppImage");
  await app.shot("E2-bad-signature");
  console.log("  • package not matching its signature refused, nothing changed");

  mode = "good";
  await click(b, "Install and restart");
  await b.waitUntil(() => sha(work) === sha(newImage), { timeout: 120000, timeoutMsg: "AppImage not replaced" });
  const backups = readdirSync(path.join(dataDir, "backups")).filter((f) => f.startsWith("pre-update-"));
  assert.equal(backups.length, 1, "a backup is made before installing");
  console.log("  • installed: AppImage replaced, pre-update backup made");
} finally {
  await app.close().catch(() => {});
  server.close();
  stopDisplay();
}
// The relaunched app may outlive the session and only one instance may run: stop copies started
// from an extracted AppImage (matched by executable path, so a normally installed app is left
// alone). The relaunch can come a moment after the session closes, so keep looking until none
// has appeared for three seconds.
async function stopExtracted() {
  const extracted = new Set();
  let quietSince = Date.now();
  while (Date.now() - quietSince < 3000) {
    for (const pid of readdirSync("/proc").filter((p) => /^\d+$/.test(p))) {
      try {
        const exe = readlinkSync(`/proc/${pid}/exe`);
        if (/appimage_extracted_[0-9a-f]+\/usr\/bin\/auto-manager$/.test(exe)) {
          process.kill(Number(pid));
          extracted.add(exe.replace(/\/usr\/bin\/auto-manager$/, ""));
          quietSince = Date.now();
        }
      } catch {
        /* not ours, or gone */
      }
    }
    await new Promise((r) => setTimeout(r, 250));
  }
  // A killed AppImage can't remove its extracted copy (hundreds of MB in /tmp).
  for (const dir of extracted) rmSync(dir, { recursive: true, force: true });
}
await stopExtracted();
try {
  execFileSync(process.execPath, [fileURLToPath(import.meta.url), "--verify", work, dataDir, newV], { stdio: "inherit" });
  console.log(`Workflow E ok: ${oldV} → ${newV}, ${oldSha.slice(0, 12)} → ${sha(work).slice(0, 12)}`);
} finally {
  await stopExtracted();
  rmSync(scratch, { recursive: true, force: true });
}
