// Add translations to the catalogs: node scripts/i18n-add.mjs file.json, where the file maps
// English text to [Spanish, French]. Existing entries are kept unless listed.
import { readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { fileURLToPath } from "node:url";

const dir = join(fileURLToPath(new URL(".", import.meta.url)), "..", "src", "i18n");
const add = JSON.parse(readFileSync(process.argv[2], "utf8"));
["es", "fr"].forEach((lang, i) => {
  const file = join(dir, `${lang}.json`);
  const cat = JSON.parse(readFileSync(file, "utf8"));
  for (const [en, tr] of Object.entries(add)) cat[en] = tr[i];
  const sorted = Object.fromEntries(Object.entries(cat).sort(([a], [b]) => (a < b ? -1 : a > b ? 1 : 0)));
  writeFileSync(file, JSON.stringify(sorted, null, 1) + "\n");
});
console.log(`Added ${Object.keys(add).length} texts`);
