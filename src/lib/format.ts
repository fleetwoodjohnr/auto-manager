// Display formatting. Money is never computed here: values arrive from the Rust core as exact
// decimal strings and are only rounded for display (half away from zero, as in the core) with
// string arithmetic, so binary floating point never changes a figure.

let currency = "USD";
let dateStyle = "system";

/** Called when settings load or change. */
export function setFormatPrefs(p: { currency: string; date_format: string }) {
  currency = p.currency;
  dateStyle = p.date_format;
}

const DEC_RE = /^-?\d+(\.\d+)?$/;

export function isDecimal(s: string): boolean {
  return DEC_RE.test(s.trim());
}

/** Round a decimal string to `dp` places, half away from zero. */
export function roundDec(s: string, dp: number): string {
  const t = s.trim();
  if (!isDecimal(t)) return t;
  const neg = t.startsWith("-");
  const [intPart, frac = ""] = (neg ? t.slice(1) : t).split(".");
  if (frac.length <= dp) {
    const out = dp > 0 ? `${intPart}.${frac.padEnd(dp, "0")}` : intPart;
    return neg && /[1-9]/.test(out) ? `-${out}` : out;
  }
  const keep = intPart + frac.slice(0, dp);
  const roundUp = frac.charCodeAt(dp) - 48 >= 5;
  let digits = keep.split("").map(Number);
  if (roundUp) {
    let i = digits.length - 1;
    while (i >= 0) {
      if (digits[i] === 9) {
        digits[i] = 0;
        i--;
      } else {
        digits[i]++;
        break;
      }
    }
    if (i < 0) digits = [1, ...digits];
  }
  const str = digits.join("");
  const ip = str.slice(0, str.length - dp) || "0";
  const fp = str.slice(str.length - dp);
  const out = dp > 0 ? `${ip.replace(/^0+(?=\d)/, "")}.${fp}` : ip.replace(/^0+(?=\d)/, "");
  return neg && /[1-9]/.test(out) ? `-${out}` : out;
}

function group(intPart: string): string {
  return intPart.replace(/\B(?=(\d{3})+(?!\d))/g, ",");
}

function symbol(): string {
  try {
    const parts = new Intl.NumberFormat(undefined, { style: "currency", currency }).formatToParts(0);
    return parts.find((p) => p.type === "currency")?.value ?? "$";
  } catch {
    return "$";
  }
}

/** "$1,234.50" — `dp` defaults to cents; pass 4 for unit costs. */
export function money(s: string | null | undefined, dp = 2): string {
  if (s == null || s === "") return "—";
  if (!isDecimal(s)) return s;
  const r = roundDec(s, dp);
  const neg = r.startsWith("-") && /[1-9]/.test(r);
  const [ip, fp] = r.replace("-", "").split(".");
  return `${neg ? "−" : ""}${symbol()}${group(ip)}${fp ? `.${fp}` : ""}`;
}

/** Plain number with up to `maxDp` decimals, trailing zeros trimmed: "12.5". */
export function num(s: string | number | null | undefined, maxDp = 2): string {
  if (s == null || s === "") return "—";
  const str = String(s);
  if (!isDecimal(str)) return str;
  const r = roundDec(str, maxDp);
  const neg = r.startsWith("-") && /[1-9]/.test(r);
  const [ip, fp = ""] = r.replace("-", "").split(".");
  const f = fp.replace(/0+$/, "");
  return `${neg ? "−" : ""}${group(ip)}${f ? `.${f}` : ""}`;
}

export function pct(s: string | null | undefined, maxDp = 1): string {
  return s == null || s === "" ? "—" : `${num(s, maxDp)}%`;
}

function fmtDate(d: Date): string {
  const y = d.getFullYear();
  const m = String(d.getMonth() + 1).padStart(2, "0");
  const day = String(d.getDate()).padStart(2, "0");
  switch (dateStyle) {
    case "iso":
      return `${y}-${m}-${day}`;
    case "us":
      return `${m}/${day}/${y}`;
    case "eu":
      return `${day}/${m}/${y}`;
    default:
      return d.toLocaleDateString(undefined, { year: "numeric", month: "short", day: "numeric" });
  }
}

/** A stored UTC timestamp or a YYYY-MM-DD date, in the chosen date style. */
export function date(s: string | null | undefined): string {
  if (!s) return "—";
  const d = /^\d{4}-\d{2}-\d{2}$/.test(s) ? new Date(Number(s.slice(0, 4)), Number(s.slice(5, 7)) - 1, Number(s.slice(8, 10))) : new Date(s);
  return isNaN(d.getTime()) ? s : fmtDate(d);
}

export function dateTime(s: string | null | undefined): string {
  if (!s) return "—";
  const d = new Date(s);
  return isNaN(d.getTime()) ? s : `${fmtDate(d)} ${d.toLocaleTimeString(undefined, { hour: "numeric", minute: "2-digit" })}`;
}

/** "just now", "5 min ago", "3 h ago", "2 days ago", then the date. */
export function ago(s: string | null | undefined, now = Date.now()): string {
  if (!s) return "—";
  const t = new Date(s).getTime();
  if (isNaN(t)) return s;
  const min = Math.round((now - t) / 60000);
  if (min < 1) return "just now";
  if (min < 60) return `${min} min ago`;
  const h = Math.round(min / 60);
  if (h < 24) return `${h} h ago`;
  const days = Math.round(h / 24);
  if (days < 14) return `${days} day${days === 1 ? "" : "s"} ago`;
  return date(s);
}

export function bytes(n: number | null | undefined): string {
  if (n == null) return "—";
  const u = ["B", "KB", "MB", "GB"];
  let i = 0;
  let v = n;
  while (v >= 1000 && i < u.length - 1) {
    v /= 1000;
    i++;
  }
  return i === 0 ? `${n} B` : `${v.toFixed(v < 10 ? 1 : 0)} ${u[i]}`;
}

/** "Brake pads" from "brake_package" style keys. */
export function humanKey(k: string): string {
  const s = k.replace(/^vpic\./, "").replace(/([a-z])([A-Z])/g, "$1 $2").replace(/[_.]/g, " ").trim();
  return s.charAt(0).toUpperCase() + s.slice(1).toLowerCase();
}
