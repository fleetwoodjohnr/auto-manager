// Set the version everywhere it lives: node scripts/bump-version.mjs 0.2.0   (or 0.2.0-beta.1)
import { readFileSync, writeFileSync } from "node:fs";
const v = process.argv[2];
if (!/^\d+\.\d+\.\d+(-(alpha|beta|rc)\.\d+)?$/.test(v ?? "")) throw new Error("Usage: bump-version.mjs MAJOR.MINOR.PATCH[-beta.N]");
const json = (f, edit) => {
  const d = JSON.parse(readFileSync(f, "utf8"));
  edit(d);
  writeFileSync(f, JSON.stringify(d, null, 2) + "\n");
};
json("package.json", (d) => (d.version = v));
json("src-tauri/tauri.conf.json", (d) => (d.version = v));
writeFileSync("Cargo.toml", readFileSync("Cargo.toml", "utf8").replace(/^version = "[^"]+"/m, `version = "${v}"`));
console.log(`Version set to ${v}. Run cargo check to refresh Cargo.lock, add a CHANGELOG entry, then tag v${v}.`);
