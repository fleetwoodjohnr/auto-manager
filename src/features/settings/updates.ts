import { create } from "zustand";
import { check, type Update } from "@tauri-apps/plugin-updater";
import { relaunch } from "@tauri-apps/plugin-process";
import { call, errorMessage } from "../../api/ipc";
import { useApp } from "../../store";
import { t } from "../../i18n";

// The same update flow as the other Simple Suite apps: check GitHub at start-up and daily,
// show a notice, and install only when the person chooses "Install and restart".

export function scheduleUpdateChecks(checkNow: () => void): () => void {
  checkNow();
  const timer = setInterval(checkNow, 24 * 60 * 60 * 1000);
  return () => clearInterval(timer);
}

function busyReason(): string | null {
  return useApp.getState().dirty ? t("Save or discard your unsaved changes before installing an update.") : null;
}

interface UpdateState {
  update: Update | null;
  state: "idle" | "checking" | "none" | "downloading" | "installing" | "error";
  error: string | null;
  downloaded: number;
  total: number | null;
  checkNow: () => Promise<void>;
  install: () => Promise<void>;
}

/** Updates come from GitHub Releases and are installed only if their signature matches
 *  Auto Manager's release key. */
export const useUpdates = create<UpdateState>((set, get) => ({
  update: null,
  state: "idle",
  error: null,
  downloaded: 0,
  total: null,
  checkNow: async () => {
    if (["checking", "downloading", "installing"].includes(get().state)) return;
    set({ state: "checking", error: null });
    try {
      const update = await check({ timeout: 15000 });
      const old = get().update;
      set({ update, state: update ? "idle" : "none" });
      if (old && old !== update) await old.close().catch(() => undefined);
    } catch (e) {
      set({ state: "error", error: errorMessage(e) });
    }
  },
  install: async () => {
    const { update, state } = get();
    if (!update || ["checking", "downloading", "installing"].includes(state)) return;
    const busy = busyReason();
    if (busy) return set({ state: "error", error: busy });
    set({ state: "downloading", error: null, downloaded: 0, total: null });
    try {
      // download() verifies the signature before anything is installed.
      await update.download((event) => {
        if (event.event === "Started") set({ total: event.data.contentLength ?? null });
        if (event.event === "Progress") set((s) => ({ downloaded: s.downloaded + event.data.chunkLength }));
      });
      const nowBusy = busyReason();
      if (nowBusy) throw new Error(nowBusy);
      await call("update_prepare");
      set({ state: "installing" });
      await update.install();
      // Windows exits inside install(); the installer restarts the application.
      await relaunch();
    } catch (e) {
      set({ state: "error", error: errorMessage(e) });
    }
  },
}));
