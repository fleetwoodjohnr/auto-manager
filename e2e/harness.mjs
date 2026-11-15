// Drives the real desktop binary through tauri-driver + WebKitWebDriver on a private Xvfb
// display (the pattern from salon-management). Every run uses a fresh data folder.
import { spawn } from "node:child_process";
import { mkdirSync, readdirSync, readlinkSync, realpathSync } from "node:fs";
import { setTimeout as sleep } from "node:timers/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { remote } from "webdriverio";

export const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
export const BIN = process.env.AM_BIN ?? path.join(root, "target/debug/auto-manager");
export const SHOTS = process.env.AM_SHOTS ?? path.join(root, "e2e/screenshots");

let xvfb;
export async function startDisplay(n = 97) {
  if (xvfb || process.env.AM_USE_DISPLAY) return;
  xvfb = spawn("Xvfb", [`:${n}`, "-screen", "0", "1440x900x24", "-nolisten", "tcp"], { stdio: "ignore" });
  process.env.DISPLAY = `:${n}`;
  await sleep(800);
}
export function stopDisplay() {
  xvfb?.kill();
  xvfb = undefined;
}

/** Start the app. `env` adds variables, e.g. provider base URLs pointing at a fixture server. */
/** Only one copy of the app may run (single-instance): a relaunch before the last copy has
 *  exited hands over to it and quits. Wait for copies of `bin` to exit; stop them after 10 s. */
async function waitExited(bin) {
  let real;
  try {
    real = realpathSync(bin);
  } catch {
    return;
  }
  const running = () =>
    readdirSync("/proc")
      .filter((p) => /^\d+$/.test(p))
      .filter((p) => {
        try {
          return readlinkSync(`/proc/${p}/exe`) === real;
        } catch {
          return false;
        }
      });
  const deadline = Date.now() + 10000;
  while (running().length) {
    if (Date.now() > deadline) running().forEach((p) => process.kill(Number(p), "SIGKILL"));
    await sleep(100);
  }
}

export async function launch({ dataDir, port = 4544, bin = BIN, env = {} }) {
  await startDisplay();
  mkdirSync(dataDir, { recursive: true });
  const fullEnv = { ...process.env, AUTO_MANAGER_DATA_DIR: dataDir, GDK_BACKEND: "x11", WEBKIT_DISABLE_DMABUF_RENDERER: "1", ...env };
  const driver = spawn("tauri-driver", ["--port", String(port), "--native-port", String(port + 1)], { env: fullEnv, stdio: ["ignore", "ignore", "pipe"] });
  let stderr = "";
  driver.stderr.on("data", (d) => (stderr += d));
  await sleep(1000);
  let browser;
  try {
    browser = await remote({ hostname: "127.0.0.1", port, logLevel: "error", capabilities: { "tauri:options": { application: bin } } });
  } catch (e) {
    driver.kill();
    throw new Error(`Could not start app session: ${e.message}\n${stderr}`);
  }
  await browser.setWindowSize?.(1400, 880).catch(() => {});
  return {
    browser,
    async shot(name) {
      mkdirSync(SHOTS, { recursive: true });
      await browser.saveScreenshot(path.join(SHOTS, `${name}.png`));
    },
    async close() {
      await browser.deleteSession().catch(() => {});
      driver.kill();
      await waitExited(bin);
    },
  };
}

/** Rendered text of the whole page. */
export const bodyText = (b) => b.execute(() => document.body.innerText);

export async function waitText(b, text, timeout = 15000) {
  try {
    await b.waitUntil(async () => (await bodyText(b)).includes(text), { timeout });
  } catch {
    const now = (await bodyText(b).catch(() => "")).replace(/\s+/g, " ").slice(0, 1500);
    throw new Error(`Text not found: ${text}\n--- page shows ---\n${now}`);
  }
}

/** Click the first visible button/link/tab/menu item whose text (or aria-label) contains
 *  `text`, with a real pointer click (Radix menus open on pointer events). */
export async function click(b, text, { exact = false } = {}) {
  await b.waitUntil(
    () =>
      b.execute(
        (t, ex) => {
          document.querySelectorAll("[data-e2e]").forEach((e) => e.removeAttribute("data-e2e"));
          const els = [...document.querySelectorAll("button, a, [role=tab], [role=menuitem], [role=option], summary, .gt-row, tbody tr")];
          const matches = els.filter((e) => {
            const s = (e.innerText || e.textContent || "").trim().replace(/\s+/g, " ");
            const label = e.getAttribute("aria-label") ?? "";
            const visible = e.getClientRects().length > 0 && !e.disabled;
            return visible && (ex ? s === t || label === t : s.includes(t) || label.includes(t));
          });
          // While a dialog is open nothing outside it can be clicked: prefer what's inside it.
          const inDialog = matches.filter((e) => e.closest("dialog[open]"));
          const pool = inDialog.length ? inDialog : matches;
          // Prefer a real control over a table row that merely contains it.
          const el = pool.find((e) => e.tagName !== "TR" && !e.classList.contains("gt-row")) ?? pool[0];
          if (!el) return false;
          el.scrollIntoView({ block: "center" });
          el.setAttribute("data-e2e", "1");
          return true;
        },
        text,
        exact,
      ),
    { timeout: 15000, timeoutMsg: `Nothing clickable says: ${text}` },
  );
  const el = await b.$('[data-e2e="1"]');
  // WebDriver won't pointer-click a table row; rows get a DOM click instead.
  if ((await el.getTagName()).toLowerCase() === "tr") await b.execute((e) => e.click(), el);
  else await el.click();
}

/** Set the input/select/textarea inside the field labelled `label` (or with that aria-label). */
export async function fill(b, label, value) {
  await b.waitUntil(
    () =>
      b.execute(
        (l, v) => {
          const byField = [...document.querySelectorAll("label.field")].find((f) => f.querySelector(":scope > span")?.textContent.trim() === l);
          const el = byField?.querySelector("input, select, textarea") ?? document.querySelector(`[aria-label="${l}"]`);
          if (!el || el.getClientRects().length === 0) return false;
          el.focus();
          const proto = el.tagName === "SELECT" ? HTMLSelectElement.prototype : el.tagName === "TEXTAREA" ? HTMLTextAreaElement.prototype : HTMLInputElement.prototype;
          Object.getOwnPropertyDescriptor(proto, "value").set.call(el, v);
          el.dispatchEvent(new Event(el.tagName === "SELECT" ? "change" : "input", { bubbles: true }));
          el.dispatchEvent(new Event("change", { bubbles: true }));
          return true;
        },
        label,
        String(value),
      ),
    { timeout: 15000, timeoutMsg: `No field labelled: ${label}` },
  );
}

/** Choose the <select> option whose text contains `text`, in the field labelled `label`. */
export async function choose(b, label, text) {
  await b.waitUntil(
    () =>
      b.execute(
        (l, t) => {
          const byField = [...document.querySelectorAll("label.field")].find((f) => f.querySelector(":scope > span")?.textContent.trim() === l);
          const el = byField?.querySelector("select") ?? document.querySelector(`select[aria-label="${l}"]`);
          const opt = el && [...el.options].find((o) => o.textContent.includes(t));
          if (!opt) return false;
          Object.getOwnPropertyDescriptor(HTMLSelectElement.prototype, "value").set.call(el, opt.value);
          el.dispatchEvent(new Event("change", { bubbles: true }));
          return true;
        },
        label,
        text,
      ),
    { timeout: 15000, timeoutMsg: `No option “${text}” in ${label}` },
  );
}

export async function nav(b, label) {
  await b.waitUntil(
    () =>
      b.execute((l) => {
        const btn = [...document.querySelectorAll("nav[aria-label=Main] button")].find((x) => x.textContent.includes(l));
        btn?.click();
        return !!btn && btn.getAttribute("aria-current") === "page";
      }, label),
    { timeout: 15000, timeoutMsg: `Couldn't open ${label}` },
  );
  await sleep(300);
}

/** Call a Tauri command directly (for checks that aren't visible on screen). */
export async function invoke(b, cmd, args = {}) {
  return b.execute((c, a) => window.__TAURI_INTERNALS__.invoke(c, a).catch((e) => ({ __error: e })), cmd, args);
}

/** A one-page PDF whose text layer says `text` (enough for pdf.js and the search index). */
export function tinyPdf(text) {
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
  return out;
}
