import { useEffect, useState } from "react";
import { useQueryClient } from "@tanstack/react-query";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { FolderSync as SyncIcon, RefreshCw } from "lucide-react";
import { call, errorMessage } from "../../api/ipc";
import { invalidateData, useCmd } from "../../api/queries";
import type { SyncReport, SyncStatus } from "../../api/types";
import { toast } from "../../store";
import { Field, useConfirm } from "../../ui";
import { ago, dateTime } from "../../lib/format";
import { t, tx } from "../../i18n";

const EVERY_MS = 5 * 60 * 1000;

/** Sync on start and every few minutes while automatic sync is on (mounted once, in App). */
export function useAutoSync() {
  const qc = useQueryClient();
  useEffect(() => {
    let stopped = false;
    const tick = async () => {
      try {
        const s = await call<SyncStatus>("folder_sync_status");
        if (stopped || !s.enabled || !s.auto) return;
        const r = await call<SyncReport>("folder_sync_now");
        if (r.received > 0) void invalidateData(qc);
        if (r.notes.length) toast(t("Sync: {0}{1}", r.notes[0], r.notes.length > 1 ? ` (+${r.notes.length - 1} more in Settings)` : ""));
      } catch {
        /* shown in Settings → Data; a folder that's briefly unavailable shouldn't nag */
      }
    };
    const first = setTimeout(tick, 4000);
    const timer = setInterval(tick, EVERY_MS);
    return () => {
      stopped = true;
      clearTimeout(first);
      clearInterval(timer);
    };
  }, [qc]);
}

function ReportView({ r }: { r: SyncReport }) {
  return (
    <div className="stack small" style={{ gap: 4 }}>
      <span className="muted">{tx("Last synced {0} ({1}): {2} change{3} received{4}.", ago(r.at), dateTime(r.at), r.received, r.received === 1 ? "" : "s", r.from.length ? t(" from {0}", r.from.join(", ")) : t(" (no other computer has synced yet)"))}</span>
      {r.notes.map((n, i) => (
        <div key={`n${i}`} className="notice warn" style={{ margin: 0 }}>
          {n}
        </div>
      ))}
      {r.skipped.map((n, i) => (
        <div key={`s${i}`} className="notice info" style={{ margin: 0 }}>
          {n}
        </div>
      ))}
    </div>
  );
}

/** Two-way sync with other computers through a shared folder. */
export function FolderSync() {
  const qc = useQueryClient();
  const status = useCmd<SyncStatus>("folder_sync_status");
  const defaultName = useCmd<string>("folder_sync_default_name");
  const [folder, setFolder] = useState("");
  const [pass, setPass] = useState("");
  const [name, setName] = useState("");
  const [busy, setBusy] = useState(false);
  const [err, setErr] = useState<string | null>(null);
  const { ask, dialog } = useConfirm();
  const s = status.data;
  const run = async (f: () => Promise<SyncReport | void>) => {
    setBusy(true);
    setErr(null);
    try {
      const r = await f();
      if (r && r.received > 0) void invalidateData(qc);
      void status.refetch();
    } catch (e) {
      setErr(errorMessage(e));
    } finally {
      setBusy(false);
    }
  };
  const pick = async () => {
    const d = await openDialog({ directory: true });
    if (typeof d === "string") setFolder(d);
  };
  return (
    <section className="card stack" style={{ gap: 10 }}>
      {dialog}
      <h2>
        <SyncIcon size={16} aria-hidden />{" "}{t("Sync between computers")}</h2>
      {!s?.enabled ? (
        <>
          <p className="small muted" style={{ margin: 0 }}>{t("Keep two or more computers up to date with each other through a folder they all see: one synced by Syncthing, Dropbox, OneDrive or Google Drive, a network share, or a USB stick you move between them. Everything in the folder is encrypted with your passphrase. Use the same folder and passphrase on every computer.")}</p>
          <form
            className="stack"
            style={{ gap: 8 }}
            onSubmit={(e) => {
              e.preventDefault();
              void run(() => call<SyncReport>("folder_sync_setup", { folder, passphrase: pass, deviceName: name || defaultName.data || "" }));
            }}
          >
            <div className="row">
              <input type="text" value={folder} onChange={(e) => setFolder(e.target.value)} placeholder={t("Shared folder")} aria-label={t("Shared folder")} style={{ flex: 1, minWidth: 240 }} />
              <button type="button" className="btn" onClick={pick}>{t("Choose…")}</button>
            </div>
            <div className="grid" style={{ gridTemplateColumns: "repeat(auto-fit, minmax(220px, 1fr))" }}>
              <Field label={t("Passphrase")} hint={t("At least 10 characters. The same on every computer; without it the folder can't be read.")}>
                <input type="password" value={pass} onChange={(e) => setPass(e.target.value)} autoComplete="new-password" />
              </Field>
              <Field label={t("This computer's name")} hint={t("How the other computers list it.")}>
                <input type="text" value={name} onChange={(e) => setName(e.target.value)} placeholder={defaultName.data ?? ""} />
              </Field>
            </div>
            <div className="row">
              <button className="btn primary" disabled={busy || !folder || pass.length < 10}>
                {busy ? t("Setting up…") : t("Start syncing")}
              </button>
            </div>
          </form>
        </>
      ) : (
        <>
          <div className="small">
            <strong>{s.device_name}</strong>{" "}{t("syncs with")}{" "}<span className="mono">{s.folder}</span>
          </div>
          {s.peers.length > 0 && (
            <div className="small muted">{tx("Other computers: {0}", s.peers.map((p) => `${p.name} (last synced ${ago(p.written_at)})`).join(" · "))}</div>
          )}
          {s.last && <ReportView r={s.last} />}
          <div className="row">
            <button className="btn primary" disabled={busy} onClick={() => run(() => call<SyncReport>("folder_sync_now"))}>
              <RefreshCw size={16} aria-hidden /> {busy ? t("Syncing…") : t("Sync now")}
            </button>
            <label className="row small">
              <input type="checkbox" checked={s.auto} onChange={(e) => run(() => call("folder_sync_set_auto", { auto: e.target.checked }))} />{" "}{t("Sync automatically (on start and every 5 minutes)")}</label>
            <button
              className="btn ghost"
              onClick={async () => {
                if (await ask(t("Stop syncing on this computer?"), t("This computer keeps its data and stops exchanging changes. The shared folder and the other computers aren't changed."), t("Stop syncing"))) {
                  void run(() => call("folder_sync_stop"));
                }
              }}
            >{t("Stop syncing on this computer")}</button>
          </div>
        </>
      )}
      {err && <div className="notice bad">{err}</div>}
      <p className="small muted" style={{ margin: 0 }}>{t("Each record keeps its latest change. When two computers change the same record before syncing, the later change wins. If they both use the same stock, or the same invoice number, Auto Manager corrects it and lists what it did here.")}</p>
    </section>
  );
}
