// Light/dark theme, as in the rest of Simple Suite: a data-theme attribute on <html>. The choice
// is kept in localStorage as well as in the settings, so it applies before the first paint (the
// settings arrive a moment later over IPC).

export type ThemePref = "system" | "light" | "dark";

const KEY = "auto-manager-theme";
const DARK = "(prefers-color-scheme: dark)";

export function resolveTheme(pref: ThemePref, systemDark: boolean): "light" | "dark" {
  return pref === "system" ? (systemDark ? "dark" : "light") : pref;
}

export function storedTheme(): ThemePref {
  try {
    const v = localStorage.getItem(KEY);
    return v === "light" || v === "dark" ? v : "system";
  } catch {
    return "system";
  }
}

let current: ThemePref = "system";

export function applyTheme(pref: ThemePref) {
  current = pref;
  try {
    localStorage.setItem(KEY, pref);
  } catch {
    /* only a convenience */
  }
  document.documentElement.dataset.theme = resolveTheme(pref, matchMedia(DARK).matches);
}

export function currentTheme(): "light" | "dark" {
  return resolveTheme(current, matchMedia(DARK).matches);
}

export function startTheme() {
  applyTheme(storedTheme());
  matchMedia(DARK).addEventListener("change", () => {
    if (current === "system") applyTheme("system");
  });
}
