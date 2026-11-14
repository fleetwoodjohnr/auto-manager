// Interface translations. English text is the key; anything without a translation shows in
// English. The language is a per-computer preference (like the theme) read before the app's
// modules load, so changing it reloads the window.
import { createElement, Fragment, type ReactNode } from "react";
import es from "./es.json";
import fr from "./fr.json";

export const LANGUAGES = [
  { id: "en", label: "English" },
  { id: "es", label: "Español" },
  { id: "fr", label: "Français" },
] as const;
export type Lang = (typeof LANGUAGES)[number]["id"];

const CATALOGS: Record<string, Record<string, string>> = { es, fr };
const KEY = "auto-manager.language";

function initial(): Lang {
  try {
    const saved = localStorage.getItem(KEY);
    if (saved && LANGUAGES.some((l) => l.id === saved)) return saved as Lang;
  } catch {
    /* storage unavailable: use the system language */
  }
  const sys = (typeof navigator !== "undefined" ? navigator.language : "en").slice(0, 2);
  return LANGUAGES.some((l) => l.id === sys) ? (sys as Lang) : "en";
}

export const lang: Lang = initial();
const catalog = CATALOGS[lang] ?? {};
if (typeof document !== "undefined") document.documentElement.lang = lang;

export function setLanguage(l: Lang) {
  try {
    localStorage.setItem(KEY, l);
  } catch {
    /* only a preference */
  }
  location.reload();
}

const SLOT = /\{(\d+)\}/g;

/** Translated text, with {0}, {1}… filled in. */
export function t(text: string, ...args: unknown[]): string {
  const s = catalog[text] ?? text;
  return args.length ? s.replace(SLOT, (_, i) => fmt(args[Number(i)])) : s;
}

function fmt(v: unknown): string {
  return v === null || v === undefined || typeof v === "boolean" ? "" : String(v);
}

/** As `t`, for text that has elements (links, badges) in its slots. */
export function tx(text: string, ...args: ReactNode[]): ReactNode {
  if (args.every((a) => a === null || a === undefined || typeof a !== "object")) return t(text, ...args);
  const s = catalog[text] ?? text;
  const out: ReactNode[] = [];
  let last = 0;
  for (const m of s.matchAll(SLOT)) {
    out.push(s.slice(last, m.index), args[Number(m[1])]);
    last = (m.index ?? 0) + m[0].length;
  }
  out.push(s.slice(last));
  return createElement(Fragment, null, ...out.map((p, i) => createElement(Fragment, { key: i }, p)));
}

/** Translate a message that came from the app's core (static messages only). */
export const tm = (message: string) => catalog[message] ?? message;
