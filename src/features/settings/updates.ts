import { useEffect } from "react";
import { create } from "zustand";
import { Channel } from "@tauri-apps/api/core";
import { relaunch } from "@tauri-apps/plugin-process";
import { ask } from "@tauri-apps/plugin-dialog";
import { call, errorMessage } from "../../api/ipc";
import type { AppInfo, Settings, UpdateInfo, UpdateProgress } from "../../api/types";
import { useApp } from "../../store";

interface UpdateState {
  checking: boolean;
  checked: boolean;
  available: UpdateInfo | null;
  downloading: boolean;
  progress: { downloaded: number; total: number | null } | null;
  ready: boolean;
  error: string | null;
  check: (channel: string) => Promise<void>;
  download: () => Promise<void>;
  installAndRestart: () => Promise<void>;
}

/** Update state shared by the title-bar pill and the Settings page. */
export const useUpdateStore = create<UpdateState>((set, get) => ({
  checking: false,
  checked: false,
  available: null,
  downloading: false,
  progress: null,
  ready: false,
  error: null,
  check: async (channel) => {
    set({ checking: true, error: null });
    try {
      const available = await call<UpdateInfo | null>("update_check", { channel });
      set({ available, checked: true, ready: false, progress: null });
    } catch (e) {
      set({ error: errorMessage(e), checked: true });
    } finally {
      set({ checking: false });
    }
  },
  download: async () => {
    const onProgress = new Channel<UpdateProgress>();
    onProgress.onmessage = (m) => {
      if (m.event === "started") set({ progress: { downloaded: 0, total: m.total } });
      if (m.event === "progress") set({ progress: { downloaded: m.downloaded, total: m.total } });
    };
    set({ downloading: true, error: null });
    try {
      await call("update_download", { onProgress });
      set({ ready: true });
    } catch (e) {
      set({ error: errorMessage(e) });
    } finally {
      set({ downloading: false });
    }
  },
  installAndRestart: async () => {
    if (useApp.getState().dirty && !(await ask("You have unsaved changes. Install the update and restart anyway?", { title: "Auto Manager", kind: "warning" }))) return;
    try {
      await call("update_install");
      await relaunch();
    } catch (e) {
      set({ error: errorMessage(e), ready: false, available: get().available });
    }
  },
}));

const DAY = 24 * 3600 * 1000;

/** Checks once a day at start-up when the setting allows (never in development builds), and
 * downloads automatically if asked to. Installing always waits for the person. */
export function useUpdates(info: AppInfo | undefined, settings: Settings | undefined) {
  const s = useUpdateStore();
  useEffect(() => {
    if (!info || !settings || info.debug || !settings.update_check || s.checked) return;
    const last = settings.last_update_check ? Date.parse(settings.last_update_check) : 0;
    if (Date.now() - last < DAY) return;
    void s.check(settings.update_channel).then(() => {
      const st = useUpdateStore.getState();
      if (settings.update_download && st.available && !st.ready) void st.download();
    });
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [info, settings]);
  return s;
}
