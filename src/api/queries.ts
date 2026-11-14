// React Query helpers. Query keys are the command name plus arguments, so invalidating a
// command name refreshes every view of it.
import { useMutation, useQuery, useQueryClient, type QueryClient, type QueryKey } from "@tanstack/react-query";
import { call, errorMessage } from "./ipc";
import { toast } from "../store";

export function useCmd<T>(cmd: string, args?: Record<string, unknown>, enabled = true) {
  return useQuery<T>({ queryKey: [cmd, args ?? {}], queryFn: () => call<T>(cmd, args), enabled });
}

/** Queries that must not be re-run after an edit: open documents, and lookups that go to
 * the network (they refresh on their own schedule or when the person asks). */
const KEEP = new Set(["pdf", "outline", "settings_get", "vpic_makes", "vpic_models", "recalls", "safety_ratings", "canada", "epa", "dtc_search", "app_info", "obd_ports"]);

/** Refresh every view of stored data (cheap: everything is local). */
export function invalidateData(qc: QueryClient) {
  return qc.invalidateQueries({ predicate: (q) => !KEEP.has(String(q.queryKey[0])) });
}


/**
 * Mutation that refreshes data views on success and shows a toast. Errors surface as a red
 * toast unless `silentError` (forms show field errors inline instead).
 */
export function useAction<TArgs extends Record<string, unknown>, TOut = unknown>(
  cmd: string,
  opts: { invalidate?: string[]; success?: string | ((out: TOut) => string); silentError?: boolean; undo?: (out: TOut) => (() => void) | null } = {},
) {
  const qc = useQueryClient();
  return useMutation<TOut, unknown, TArgs>({
    mutationFn: (args) => call<TOut>(cmd, args),
    onSuccess: (out) => {
      if (opts.invalidate) for (const k of opts.invalidate) qc.invalidateQueries({ queryKey: [k] as QueryKey });
      else void invalidateData(qc);
      const msg = typeof opts.success === "function" ? opts.success(out) : opts.success;
      const undo = opts.undo?.(out);
      if (msg) toast(msg, undo ? { action: { label: "Undo", run: undo } } : undefined);
    },
    onError: (e) => {
      if (!opts.silentError) toast(errorMessage(e), { tone: "bad" });
    },
  });
}

export function useRefresh() {
  const qc = useQueryClient();
  return () => void invalidateData(qc);
}
