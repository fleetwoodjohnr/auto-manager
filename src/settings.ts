import { useEffect } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { call, errorMessage } from "./api/ipc";
import type { Settings } from "./api/types";
import { applyTheme } from "./theme";
import { setFormatPrefs } from "./lib/format";
import { toast } from "./store";

/** The app settings, loaded once and kept in the query cache. Theme and number/date formats
 * follow them as soon as they load or change. */
export function useSettings() {
  const qc = useQueryClient();
  const q = useQuery<Settings>({ queryKey: ["settings_get"], queryFn: () => call<Settings>("settings_get"), staleTime: Infinity });
  useEffect(() => {
    if (q.data) {
      applyTheme(q.data.theme as "system" | "light" | "dark");
      setFormatPrefs(q.data);
    }
  }, [q.data]);
  const m = useMutation({
    mutationFn: (s: Settings) => call<Settings>("settings_save", { input: s }),
    onSuccess: (s) => qc.setQueryData(["settings_get"], s),
    onError: (e) => toast(errorMessage(e), { tone: "bad" }),
  });
  const save = (patch: Partial<Settings>) => q.data && m.mutate({ ...q.data, ...patch });
  return { settings: q.data, save, saving: m.isPending };
}
