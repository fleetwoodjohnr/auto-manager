// Every interface text to translate: the first argument of t()/tx() in src/, plus fixed messages
// from the Rust core (AppError::msg / invalid / Conflict with a plain string). Prints JSON.
import { readFileSync, readdirSync, statSync } from "node:fs";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import ts from "typescript";

// Under a test runner import.meta.url may not be a file: URL; the project root is then the cwd.
const self = (() => {
  try {
    return fileURLToPath(import.meta.url);
  } catch {
    return null;
  }
})();
const root = self ? join(self, "..", "..") : process.cwd();
const walk = (d, ok) => readdirSync(d).flatMap((f) => (statSync(join(d, f)).isDirectory() ? (/bindings|node_modules|target/.test(f) ? [] : walk(join(d, f), ok)) : ok(f) ? [join(d, f)] : []));

export function uiKeys() {
  const keys = new Set();
  for (const f of walk(join(root, "src"), (f) => /\.tsx?$/.test(f) && !/\.test\.|\.d\.ts/.test(f))) {
    const sf = ts.createSourceFile(f, readFileSync(f, "utf8"), ts.ScriptTarget.Latest, true, f.endsWith("x") ? ts.ScriptKind.TSX : ts.ScriptKind.TS);
    (function v(n) {
      if (ts.isCallExpression(n) && /^(t|tx)$/.test(n.expression.getText()) && n.arguments[0] && ts.isStringLiteralLike(n.arguments[0])) keys.add(n.arguments[0].text);
      ts.forEachChild(n, v);
    })(sf);
  }
  return keys;
}

export function coreKeys() {
  const keys = new Set();
  const re = /AppError::(?:msg|Conflict|invalid)\(\s*(?:"[a-z_]+",\s*)?"((?:[^"\\]|\\.)*)"(?:\.into\(\)|\.to_string\(\))?\s*\)/g;
  for (const f of walk(join(root, "crates"), (f) => f.endsWith(".rs"))) {
    for (const m of readFileSync(f, "utf8").matchAll(re)) if (!m[1].includes("{")) keys.add(m[1].replace(/\\"/g, '"'));
  }
  return keys;
}

if (self && process.argv[1] === self) {
  const ui = uiKeys(), core = coreKeys();
  console.log(JSON.stringify([...new Set([...ui, ...core])].sort(), null, 1));
  console.error(`${ui.size} interface texts, ${core.size} core messages`);
}
