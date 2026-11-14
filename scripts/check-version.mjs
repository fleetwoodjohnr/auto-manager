// The release tag, package.json, Cargo.toml and tauri.conf.json must agree, and the updater must
// point at this repository's releases.
import { readFileSync } from "node:fs";
const pkg = JSON.parse(readFileSync("package.json"));
const config = JSON.parse(readFileSync("src-tauri/tauri.conf.json"));
const cargo = readFileSync("Cargo.toml", "utf8").match(/^version = "([^"]+)"/m)?.[1];
const tag = process.env.RELEASE_TAG || `v${pkg.version}`;
if (!/^v\d+\.\d+\.\d+(-(alpha|beta|rc)\.\d+)?$/.test(tag)) throw new Error(`Tag ${tag} isn't vMAJOR.MINOR.PATCH or vMAJOR.MINOR.PATCH-beta.N`);
if ([pkg.version, config.version, cargo].some((v) => `v${v}` !== tag)) throw new Error(`Version mismatch: tag ${tag}, npm ${pkg.version}, tauri ${config.version}, cargo ${cargo}`);
if (!config.plugins.updater.pubkey || config.plugins.updater.endpoints.some((u) => !u.startsWith("https://github.com/fleetwoodjohnr/auto-manager/releases/"))) throw new Error("Release updater is not configured");
console.log(`Validated ${tag}`);
