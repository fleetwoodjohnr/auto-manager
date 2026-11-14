import { create } from "zustand";

export type Page = "dashboard" | "lookup" | "vehicles" | "vehicle" | "parts" | "part" | "inventory" | "item" | "manuals" | "manual" | "service" | "job" | "pricing" | "reports" | "settings" | "customers" | "suppliers" | "print";

export interface Route {
  page: Page;
  id?: string;
  tab?: string;
  /** Extra state, e.g. a page number for the manual viewer. */
  arg?: string;
}

export interface Toast {
  id: number;
  message: string;
  tone?: "bad";
  action?: { label: string; run: () => void };
}

interface AppState {
  route: Route;
  history: Route[];
  go: (r: Route | Page) => void;
  back: () => void;
  setTab: (tab: string) => void;
  toasts: Toast[];
  toast: (message: string, opts?: { tone?: "bad"; action?: Toast["action"] }) => void;
  dismiss: (id: number) => void;
  /** A form with unsaved changes; the updater asks before restarting. */
  dirty: boolean;
  setDirty: (d: boolean) => void;
}

let nextToast = 1;

export const useApp = create<AppState>((set, get) => ({
  route: { page: "dashboard" },
  history: [],
  go: (r) => {
    const route = typeof r === "string" ? { page: r } : r;
    set((s) => ({ route, history: [...s.history.slice(-30), s.route] }));
    document.getElementById("main")?.scrollTo?.(0, 0);
  },
  back: () => {
    const h = get().history;
    if (h.length === 0) return set({ route: { page: "dashboard" } });
    set({ route: h[h.length - 1], history: h.slice(0, -1) });
  },
  setTab: (tab) => set((s) => ({ route: { ...s.route, tab } })),
  toasts: [],
  toast: (message, opts) => {
    const id = nextToast++;
    set((s) => ({ toasts: [...s.toasts.slice(-3), { id, message, ...opts }] }));
    setTimeout(() => get().dismiss(id), opts?.tone === "bad" ? 8000 : opts?.action ? 7000 : 3500);
  },
  dismiss: (id) => set((s) => ({ toasts: s.toasts.filter((t) => t.id !== id) })),
  dirty: false,
  setDirty: (dirty) => set({ dirty }),
}));

export const toast = (message: string, opts?: { tone?: "bad"; action?: Toast["action"] }) => useApp.getState().toast(message, opts);
