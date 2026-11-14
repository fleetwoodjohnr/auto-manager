// Build latest.json (the updater manifest) and SHA256SUMS from the per-platform build artifacts.
// Every updater package's signature is verified against the public key built into the app, every
// platform/bundle must be present, and the release notes come from CHANGELOG.md.
// (Adapted from Simple Connect's release script.)
import { createHash, createPublicKey, verify } from "node:crypto";
import { readFile, readdir, writeFile, rename } from "node:fs/promises";
import path from "node:path";
import { pathToFileURL } from "node:url";

export function verifyArtifact(bytes, encodedSignature, encodedKey, expectedVersion) {
  const lines = Buffer.from(encodedSignature.trim(), "base64").toString().trim().split("\n");
  const keyLines = Buffer.from(encodedKey.trim(), "base64").toString().trim().split("\n");
  const sig = Buffer.from(lines[1] ?? "", "base64");
  const key = Buffer.from(keyLines[1] ?? "", "base64");
  if (sig.length !== 74 || key.length !== 42 || !sig.subarray(2, 10).equals(key.subarray(2, 10))) throw new Error("Invalid signing key or signature");
  const algorithm = sig.subarray(0, 2).toString();
  if (!["Ed", "ED"].includes(algorithm)) throw new Error("Unsupported signature algorithm");
  const data = algorithm === "ED" ? createHash("blake2b512").update(bytes).digest() : bytes;
  const publicKey = createPublicKey({ key: Buffer.concat([Buffer.from("302a300506032b6570032100", "hex"), key.subarray(10)]), format: "der", type: "spki" });
  if (!verify(null, data, publicKey, sig.subarray(10))) throw new Error("Artifact signature verification failed");
  if (!lines[0]?.startsWith("untrusted comment: ") || !lines[2]?.startsWith("trusted comment: ") || key.subarray(0, 2).toString() !== "Ed") throw new Error("Invalid minisign encoding");
  const globalSignature = Buffer.from(lines[3] ?? "", "base64");
  const globalMessage = Buffer.concat([sig.subarray(10), Buffer.from(lines[2].slice(17))]);
  if (globalSignature.length !== 64 || !verify(null, globalMessage, publicKey, globalSignature)) throw new Error("Trusted comment signature verification failed");
  if (expectedVersion) {
    const versions = lines[2].slice(17).split("\t").filter((f) => f.startsWith("version:"));
    if (versions.length !== 1 || versions[0].slice(8) !== expectedVersion) throw new Error("Signed artifact version does not match release");
  }
}

async function files(dir) {
  const entries = await readdir(dir, { withFileTypes: true });
  return (await Promise.all(entries.map((e) => (e.isDirectory() ? files(path.join(dir, e.name)) : path.join(dir, e.name))))).flat();
}

/** The CHANGELOG.md section for a version ("## [0.2.0] …" up to the next "## "). */
export function notesFor(changelog, version) {
  const lines = changelog.split("\n");
  const start = lines.findIndex((l) => l.startsWith(`## [${version}]`));
  if (start < 0) return null;
  const end = lines.findIndex((l, i) => i > start && l.startsWith("## "));
  return lines.slice(start + 1, end < 0 ? undefined : end).join("\n").trim();
}

export async function manifest(root, tag, repository, pubkey, changelog = "") {
  if (!/^v\d+\.\d+\.\d+(-(alpha|beta|rc)\.\d+)?$/.test(tag)) throw new Error("Expected vMAJOR.MINOR.PATCH or vMAJOR.MINOR.PATCH-beta.N tag");
  const version = tag.slice(1);
  const notes = notesFor(changelog, version);
  if (!notes) throw new Error(`CHANGELOG.md has no "## [${version}]" section`);
  const platforms = {};
  const sums = [];
  const names = new Set();
  const diskImages = new Set();
  for (const file of await files(root)) {
    const target = path.relative(root, file).split(path.sep)[0];
    const originalName = path.basename(file);
    // GitHub replaces spaces with dots in asset names; normalize first so URLs and sums match.
    const name = (originalName.startsWith(`${target}-`) ? originalName : `${target}-${originalName}`).replaceAll(" ", ".");
    const normalizedPath = path.join(path.dirname(file), name);
    const type = name.endsWith(".app.tar.gz") ? "app" : name.endsWith(".AppImage") ? "appimage" : name.endsWith(".deb") ? "deb" : name.endsWith(".rpm") ? "rpm" : name.endsWith("-setup.exe") ? "nsis" : null;
    const diskImage = name.endsWith(".dmg");
    if (!type && !diskImage) continue;
    if (names.has(name)) throw new Error(`Duplicate asset: ${name}`);
    names.add(name);
    const bytes = await readFile(file);
    sums.push(`${createHash("sha256").update(bytes).digest("hex")}  ${name}`);
    if (diskImage) {
      if (!/^darwin-(x86_64|aarch64)$/.test(target) || diskImages.has(target)) throw new Error(`Invalid or duplicate DMG target: ${target}`);
      diskImages.add(target);
      if (file !== normalizedPath) await rename(file, normalizedPath);
      continue;
    }
    const signature = (await readFile(`${file}.sig`, "utf8")).trim();
    verifyArtifact(bytes, signature, pubkey, version);
    if (file !== normalizedPath) {
      await rename(file, normalizedPath);
      await rename(`${file}.sig`, `${normalizedPath}.sig`);
    }
    const key = `${target}-${type}`;
    if (platforms[key]) throw new Error(`Duplicate target: ${key}`);
    platforms[key] = { signature, url: `https://github.com/${repository}/releases/download/${tag}/${encodeURIComponent(name)}` };
  }
  const expected = ["x86_64", "aarch64"].flatMap((arch) => [`windows-${arch}-nsis`, `darwin-${arch}-app`, ...["deb", "rpm", "appimage"].map((t) => `linux-${arch}-${t}`)]);
  if (Object.keys(platforms).length !== expected.length || expected.some((k) => !platforms[k])) throw new Error(`Incomplete targets: expected ${expected.join(", ")}, got ${Object.keys(platforms).join(", ")}`);
  if (diskImages.size !== 2) throw new Error("Missing macOS DMG installers");
  await writeFile(path.join(root, "latest.json"), JSON.stringify({ version, notes, pub_date: new Date().toISOString(), platforms }, null, 2) + "\n");
  await writeFile(path.join(root, "SHA256SUMS"), sums.sort().join("\n") + "\n");
  await writeFile(path.join(root, "RELEASE_NOTES.md"), `${notes}\n\nVerify downloads with SHA256SUMS. Installed copies update themselves from this release (signed packages only).\n`);
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  const config = JSON.parse(await readFile("src-tauri/tauri.conf.json", "utf8"));
  const changelog = await readFile("CHANGELOG.md", "utf8");
  await manifest(process.argv[2], process.env.RELEASE_TAG || `v${config.version}`, process.env.GITHUB_REPOSITORY || "fleetwoodjohnr/auto-manager", config.plugins.updater.pubkey, changelog);
}
