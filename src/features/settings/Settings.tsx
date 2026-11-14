import { useState, type ReactNode } from "react";
import { Monitor, Moon, Sun, RefreshCw, HardDriveDownload, FolderOpen, ShieldCheck, Upload, Download } from "lucide-react";
import { open as openFile, save as saveFile } from "@tauri-apps/plugin-dialog";
import { revealItemInDir } from "@tauri-apps/plugin-opener";
import { useSettings } from "../../settings";
import { useAction, useCmd, useRefresh } from "../../api/queries";
import { call, errorMessage } from "../../api/ipc";
import type { AppInfo, BackupInfo, CacheStats, Location, Profile, Settings as S, Source } from "../../api/types";
import { useApp, toast } from "../../store";
import { Badge, ExtLink, Loaded, PageHead, ProgressBar, Spinner, Tabs, useConfirm } from "../../ui";
import { bytes, dateTime, ago } from "../../lib/format";
import { useUpdateStore } from "./updates";
import { FolderSync } from "./FolderSync";

const WIDGET_LABEL: Record<string, string> = {
  shortcuts: "Shortcuts",
  recent_vehicles: "Recently searched vehicles",
  garage: "Saved vehicles",
  inventory: "Inventory value and counts",
  low_stock: "Low stock and restocking",
  jobs: "Open service jobs",
  maintenance: "Upcoming maintenance",
  recalls: "Recall notices",
  activity: "Recent inventory activity",
  updates: "Application and data status",
};

function Setting({ label, hint, children }: { label: string; hint?: ReactNode; children: ReactNode }) {
  return (
    <div className="setting">
      <div>
        {label}
        {hint && <p className="hint">{hint}</p>}
      </div>
      <div className="flex-none">{children}</div>
    </div>
  );
}

function Segmented<T extends string>({ name, value, options, onChange }: { name: string; value: T; options: { v: T; label: string; icon?: ReactNode }[]; onChange: (v: T) => void }) {
  return (
    <div className="segmented" role="radiogroup" aria-label={name}>
      {options.map((o) => (
        <label key={o.v}>
          <input type="radio" name={name} checked={value === o.v} onChange={() => onChange(o.v)} />
          {o.icon}
          {o.label}
        </label>
      ))}
    </div>
  );
}

function Switch({ checked, onChange, label, disabled }: { checked: boolean; onChange: (v: boolean) => void; label: string; disabled?: boolean }) {
  return <input type="checkbox" role="switch" className="switch" aria-label={label} checked={checked} disabled={disabled} onChange={(e) => onChange(e.target.checked)} />;
}

export default function Settings() {
  const { route, setTab } = useApp();
  const { settings, save } = useSettings();
  const tab = route.tab ?? "general";
  return (
    <div className="page settings">
      <PageHead title="Settings" sub="Saved as you change them." />
      <Tabs
        label="Settings sections"
        value={tab}
        onChange={setTab}
        tabs={[
          { id: "general", label: "General" },
          { id: "data_sources", label: "Vehicle data" },
          { id: "inventory", label: "Inventory" },
          { id: "pricing", label: "Pricing" },
          { id: "data", label: "Data and backups" },
          { id: "updates", label: "Updates" },
        ]}
      />
      {settings ? (
        <>
          {tab === "general" && <General s={settings} save={save} />}
          {tab === "data_sources" && <DataSources s={settings} save={save} />}
          {tab === "inventory" && <InventorySettings s={settings} save={save} />}
          {tab === "pricing" && <PricingSettings />}
          {tab === "data" && <DataSettings s={settings} save={save} />}
          {tab === "updates" && <Updates s={settings} save={save} />}
        </>
      ) : (
        <Spinner label="Loading settings" />
      )}
    </div>
  );
}

type Props = { s: S; save: (p: Partial<S>) => void };

function General({ s, save }: Props) {
  return (
    <div className="stack">
      <section className="card">
        <h2>Appearance</h2>
        <Setting label="Theme">
          <Segmented
            name="Theme"
            value={s.theme as "system" | "light" | "dark"}
            onChange={(theme) => save({ theme })}
            options={[
              { v: "system", label: "System", icon: <Monitor size={14} aria-hidden /> },
              { v: "light", label: "Light", icon: <Sun size={14} aria-hidden /> },
              { v: "dark", label: "Dark", icon: <Moon size={14} aria-hidden /> },
            ]}
          />
        </Setting>
        <Setting label="Start on" hint="The page Auto Manager opens to.">
          <select value={s.landing_page} onChange={(e) => save({ landing_page: e.target.value })} aria-label="Start page">
            <option value="dashboard">Dashboard</option>
            <option value="lookup">Vehicle Lookup</option>
            <option value="vehicles">My Vehicles</option>
            <option value="parts">Parts Catalog</option>
            <option value="inventory">Inventory</option>
            <option value="service">Maintenance &amp; Service</option>
          </select>
        </Setting>
      </section>
      <section className="card">
        <h2>Formats</h2>
        <Setting label="Measurement units" hint="Specifications keep the units their source gave; this only changes how they're shown.">
          <Segmented
            name="Units"
            value={s.units as "imperial" | "metric"}
            onChange={(units) => save({ units })}
            options={[
              { v: "imperial", label: "Miles, lb, gal" },
              { v: "metric", label: "Km, kg, litres" },
            ]}
          />
        </Setting>
        <Setting label="Date format">
          <select value={s.date_format} onChange={(e) => save({ date_format: e.target.value })} aria-label="Date format">
            <option value="system">As this computer</option>
            <option value="iso">2026-11-14</option>
            <option value="us">11/14/2026</option>
            <option value="eu">14/11/2026</option>
          </select>
        </Setting>
        <Setting label="Currency" hint="Three-letter code, e.g. USD, CAD, EUR, GBP. All amounts are in this one currency.">
          <input
            type="text"
            defaultValue={s.currency}
            maxLength={3}
            style={{ width: 80, textTransform: "uppercase" }}
            aria-label="Currency code"
            onBlur={(e) => {
              const c = e.target.value.trim().toUpperCase();
              if (c !== s.currency) save({ currency: c });
            }}
          />
        </Setting>
        <Setting label="Language" hint="English only for now; other languages need translations first.">
          <span className="muted">English</span>
        </Setting>
      </section>
      <section className="card">
        <h2>Dashboard</h2>
        <p className="small muted">Choose what the dashboard shows. Hidden sections can be turned back on here.</p>
        <div className="toggles">
          {Object.entries(WIDGET_LABEL).map(([id, label]) => (
            <label className="toggle" key={id}>
              <input
                type="checkbox"
                checked={s.dashboard_widgets.includes(id)}
                onChange={(e) => save({ dashboard_widgets: e.target.checked ? [...s.dashboard_widgets, id] : s.dashboard_widgets.filter((w) => w !== id) })}
              />
              {label}
            </label>
          ))}
        </div>
      </section>
    </div>
  );
}

function DataSources({ s, save }: Props) {
  const sources = useCmd<Source[]>("data_sources");
  const cache = useCmd<CacheStats>("cache_stats");
  const refresh = useRefresh();
  const setPriority = useAction<{ id: string; priority: number }>("data_source_priority", { invalidate: ["data_sources", "vehicle_detail"], success: "Priority saved" });
  const clear = useAction("cache_clear", { invalidate: ["cache_stats"], success: (n) => `Removed ${n} saved answers` });
  return (
    <div className="stack">
      <section className="card">
        <h2>Data providers</h2>
        <p className="small muted">
          When two sources disagree about a value, the one with the lower priority number is shown and the disagreement is flagged. Your own entries always come first. None of the
          built-in providers needs an account or an API key.
        </p>
        <Loaded q={sources}>
          {(list) => (
            <div className="table-wrap" style={{ maxHeight: "none" }}>
              <table>
                <thead>
                  <tr>
                    <th>Source</th>
                    <th>Type</th>
                    <th>Records</th>
                    <th>Terms</th>
                    <th>Priority</th>
                  </tr>
                </thead>
                <tbody>
                  {list.map((d) => (
                    <tr key={d.id}>
                      <td>
                        {d.name}
                        {d.url && <div className="small muted">{d.url}</div>}
                      </td>
                      <td>
                        <Badge tone={d.kind === "provider" ? "info" : d.kind === "user" ? "ok" : "neutral"}>{d.kind === "provider" ? "Online provider" : d.kind === "user" ? "You" : "Import"}</Badge>
                      </td>
                      <td>{d.records}</td>
                      <td className="small">{d.license ?? "—"}</td>
                      <td>
                        {d.id === "user" ? (
                          <span className="muted">Always first</span>
                        ) : (
                          <input
                            type="number"
                            min={1}
                            max={1000}
                            defaultValue={d.priority}
                            style={{ width: 80 }}
                            aria-label={`Priority of ${d.name}`}
                            onBlur={(e) => Number(e.target.value) !== d.priority && setPriority.mutate({ id: d.id, priority: Number(e.target.value) })}
                          />
                        )}
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          )}
        </Loaded>
        <p className="small muted mt-2">What each provider can supply, and its terms, is listed in the provider documentation (docs/PROVIDERS.md).</p>
      </section>
      <section className="card">
        <h2>Saved answers (offline cache)</h2>
        <Setting label="Refresh saved answers after" hint="Lookups that worked once keep working offline; after this many days they are refreshed when online.">
          <select value={s.provider_refresh_days} onChange={(e) => save({ provider_refresh_days: Number(e.target.value) })} aria-label="Refresh days">
            {[1, 7, 30, 90, 365].map((d) => (
              <option key={d} value={d}>
                {d} day{d > 1 ? "s" : ""}
              </option>
            ))}
          </select>
        </Setting>
        <Setting label="Stored" hint={cache.data?.oldest ? `Oldest from ${dateTime(cache.data.oldest)}` : undefined}>
          <div className="row">
            <span className="muted">{cache.data ? `${cache.data.entries} answers · ${bytes(cache.data.bytes)}` : "…"}</span>
            <button
              className="btn"
              onClick={() => {
                clear.mutate({});
                refresh();
              }}
              disabled={!cache.data?.entries}
            >
              Clear
            </button>
          </div>
        </Setting>
      </section>
    </div>
  );
}

function InventorySettings({ s, save }: Props) {
  const locs = useCmd<Location[]>("location_list");
  const add = useAction<{ name: string }>("location_add", { invalidate: ["location_list"], success: "Location added" });
  const rename = useAction<{ id: string; name: string }>("location_rename", { invalidate: ["location_list", "item_list"], success: "Renamed" });
  const setDefault = useAction<{ id: string }>("location_set_default", { invalidate: ["location_list"], success: "Default location changed" });
  const [name, setName] = useState("");
  return (
    <div className="stack">
      <section className="card">
        <h2>Locations</h2>
        <p className="small muted">Warehouses, shelves, vans: anywhere stock is kept. Damaged stock goes to the quarantine location, where it can't be sold or reserved.</p>
        <Loaded q={locs}>
          {(list) => (
            <ul className="items" style={{ maxHeight: "none" }}>
              {list.map((l) => (
                <li key={l.id} className="spread">
                  <input
                    type="text"
                    defaultValue={l.name}
                    aria-label="Location name"
                    onBlur={(e) => e.target.value.trim() !== l.name && rename.mutate({ id: l.id, name: e.target.value })}
                    style={{ flex: 1, maxWidth: 320 }}
                  />
                  <span className="row">
                    <span className="small muted">{l.item_count} items</span>
                    {l.is_default ? (
                      <Badge tone="info">Default</Badge>
                    ) : l.kind === "damaged" ? (
                      <Badge tone="warn">Quarantine</Badge>
                    ) : (
                      <button className="btn link" onClick={() => setDefault.mutate({ id: l.id })}>
                        Make default
                      </button>
                    )}
                  </span>
                </li>
              ))}
            </ul>
          )}
        </Loaded>
        <form
          className="row mt-2"
          onSubmit={(e) => {
            e.preventDefault();
            add.mutate({ name }, { onSuccess: () => setName("") });
          }}
        >
          <input type="text" placeholder="New location, e.g. Back shelf" value={name} onChange={(e) => setName(e.target.value)} aria-label="New location name" />
          <button className="btn" disabled={!name.trim()}>
            Add location
          </button>
        </form>
      </section>
      <section className="card">
        <h2>Stock</h2>
        <Setting label="Inventory valuation" hint="Average cost follows every delivery (weighted average). Standard uses each item's last purchase cost.">
          <Segmented
            name="Valuation"
            value={s.valuation_method as "average" | "standard"}
            onChange={(valuation_method) => save({ valuation_method })}
            options={[
              { v: "average", label: "Average cost" },
              { v: "standard", label: "Last cost" },
            ]}
          />
        </Setting>
        <Setting label="Suggested minimum stock" hint="Pre-filled on new items; low-stock alerts start at or below it.">
          <input type="number" min={0} defaultValue={s.default_min_stock} style={{ width: 90 }} aria-label="Default minimum stock" onBlur={(e) => save({ default_min_stock: Math.max(0, Number(e.target.value) || 0) })} />
        </Setting>
        <Setting label="Label barcodes" hint="Barcode type printed on inventory labels. USB scanners work with both.">
          <Segmented
            name="Label barcode"
            value={s.label_symbology as "code128" | "qr"}
            onChange={(label_symbology) => save({ label_symbology })}
            options={[
              { v: "code128", label: "Code 128" },
              { v: "qr", label: "QR code" },
            ]}
          />
        </Setting>
      </section>
    </div>
  );
}

function PricingSettings() {
  const go = useApp((s) => s.go);
  const profiles = useCmd<Profile[]>("profile_list");
  const setDefault = useAction<{ id: string }>("profile_set_default", { invalidate: ["profile_list", "item_list", "vehicle_matches"], success: "Default profile changed" });
  return (
    <section className="card">
      <h2>Pricing</h2>
      <Loaded q={profiles}>
        {(list) => (
          <>
            <Setting label="Default pricing profile" hint="Used when neither the job, the customer nor the item names a profile.">
              <select value={list.find((p) => p.is_default)?.id} onChange={(e) => setDefault.mutate({ id: e.target.value })} aria-label="Default pricing profile">
                {list.map((p) => (
                  <option key={p.id} value={p.id}>
                    {p.name}
                  </option>
                ))}
              </select>
            </Setting>
            <Setting label="Profiles, rules, labor rates and tax">
              <button className="btn" onClick={() => go("pricing")}>
                Open Pricing
              </button>
            </Setting>
          </>
        )}
      </Loaded>
    </section>
  );
}

function DataSettings({ s, save }: Props) {
  const info = useCmd<AppInfo>("app_info");
  const backups = useCmd<BackupInfo[]>("backup_list");
  const refresh = useRefresh();
  const { ask, dialog } = useConfirm();
  const [busy, setBusy] = useState<string | null>(null);
  const run = async (what: string, f: () => Promise<unknown>, ok: string) => {
    setBusy(what);
    try {
      await f();
      toast(ok);
      refresh();
      void backups.refetch();
    } catch (e) {
      toast(errorMessage(e), { tone: "bad" });
    } finally {
      setBusy(null);
    }
  };
  const restore = async (path: string) => {
    let label = path;
    try {
      const m = await call<{ created_at: string; app_version: string }>("backup_inspect", { path });
      label = `the backup from ${dateTime(m.created_at)} (Auto Manager ${m.app_version})`;
    } catch (e) {
      return toast(errorMessage(e), { tone: "bad" });
    }
    if (!(await ask("Restore this backup?", <>Your current data will be replaced by {label}. A backup of the current data is made first, and the previous files are kept in the data folder.</>, "Restore"))) return;
    await run("restore", () => call("backup_restore", { path }), "Backup restored");
  };
  return (
    <div className="stack">
      {dialog}
      <section className="card">
        <h2>Backups</h2>
        <Setting label="Automatic daily backup" hint="Kept for 14 days. A backup is also made before every update, data upgrade and restore.">
          <Switch checked={s.backup_auto} onChange={(backup_auto) => save({ backup_auto })} label="Automatic daily backup" />
        </Setting>
        <div className="row">
          <button className="btn primary" disabled={!!busy} onClick={() => run("now", () => call("backup_now"), "Backup made")}>
            {busy === "now" ? <Spinner /> : <HardDriveDownload size={16} aria-hidden />} Back up now
          </button>
          <button
            className="btn"
            disabled={!!busy}
            onClick={async () => {
              const path = await saveFile({ defaultPath: `auto-manager-backup-${new Date().toISOString().slice(0, 10)}.zip`, filters: [{ name: "Backup", extensions: ["zip"] }] });
              if (path) await run("to", () => call("backup_to", { path }), "Backup saved");
            }}
          >
            <Download size={16} aria-hidden /> Save a backup to…
          </button>
          <button
            className="btn"
            disabled={!!busy}
            onClick={async () => {
              const path = await openFile({ filters: [{ name: "Backup", extensions: ["zip"] }] });
              if (typeof path === "string") await restore(path);
            }}
          >
            <Upload size={16} aria-hidden /> Restore from file…
          </button>
        </div>
        <Loaded q={backups}>
          {(list) =>
            list.length === 0 ? (
              <p className="small muted mt-2">No backups yet.</p>
            ) : (
              <div className="table-wrap mt-2" style={{ maxHeight: 280 }}>
                <table>
                  <thead>
                    <tr>
                      <th>Made</th>
                      <th>Why</th>
                      <th>Size</th>
                      <th />
                    </tr>
                  </thead>
                  <tbody>
                    {list.map((b) => (
                      <tr key={b.path}>
                        <td>{b.manifest ? dateTime(b.manifest.created_at) : b.file_name}</td>
                        <td>{b.manifest ? b.manifest.reason.replace(/-/g, " ") : <span style={{ color: "var(--danger)" }}>{b.error}</span>}</td>
                        <td>{bytes(b.size)}</td>
                        <td>
                          {b.manifest && (
                            <button className="btn link" disabled={!!busy} onClick={() => restore(b.path)}>
                              Restore
                            </button>
                          )}
                        </td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
            )
          }
        </Loaded>
      </section>
      <FolderSync />
      <SyncBundles busy={busy} run={run} />
      <section className="card">
        <h2>Export</h2>
        <div className="row">
          {(["csv", "xlsx"] as const).map((ext) => (
            <button
              key={ext}
              className="btn"
              onClick={async () => {
                const path = await saveFile({ defaultPath: `inventory.${ext}`, filters: [{ name: ext.toUpperCase(), extensions: [ext] }] });
                if (path) await run("export", () => call("export_inventory", { path }), "Inventory exported");
              }}
            >
              Export inventory ({ext.toUpperCase()})
            </button>
          ))}
          <button
            className="btn"
            onClick={async () => {
              const path = await saveFile({ defaultPath: "auto-manager-data.json", filters: [{ name: "JSON", extensions: ["json"] }] });
              if (path) await run("json", () => call("export_json", { path }), "All data exported");
            }}
          >
            Export everything (JSON)
          </button>
        </div>
      </section>
      <section className="card">
        <h2>Storage</h2>
        {info.data && (
          <dl className="kv">
            <dt>Data folder</dt>
            <dd className="mono">{info.data.data_dir}</dd>
            <dt>Data version</dt>
            <dd>{info.data.schema_version}</dd>
          </dl>
        )}
        <div className="row mt-2">
          <button className="btn" onClick={() => info.data && revealItemInDir(info.data.data_dir + "/auto-manager.db").catch((e) => toast(errorMessage(e), { tone: "bad" }))}>
            <FolderOpen size={16} aria-hidden /> Show in folder
          </button>
          <button
            className="btn"
            disabled={!!busy}
            onClick={async () => {
              setBusy("check");
              try {
                const r = await call<string>("integrity_check");
                toast(r === "ok" ? "The database is healthy." : `Problems found: ${r}`, r === "ok" ? undefined : { tone: "bad" });
              } catch (e) {
                toast(errorMessage(e), { tone: "bad" });
              } finally {
                setBusy(null);
              }
            }}
          >
            {busy === "check" ? <Spinner /> : <ShieldCheck size={16} aria-hidden />} Check database
          </button>
        </div>
        <p className="small muted mt-2">Your data stays on this computer. Nothing is uploaded, and there is no account or telemetry.</p>
      </section>
    </div>
  );
}

function Updates({ s, save }: Props) {
  const info = useCmd<AppInfo>("app_info");
  const u = useUpdateStore();
  const managed = info.data?.os === "linux" && !info.data.bundle;
  return (
    <div className="stack">
      <section className="card">
        <h2>Version</h2>
        <dl className="kv">
          <dt>Installed</dt>
          <dd>
            {info.data?.version ?? "…"} {info.data?.debug && <Badge tone="warn">development build</Badge>}
          </dd>
          <dt>Latest</dt>
          <dd>
            {u.available ? `${u.available.version}` : u.checked && !u.error ? "You're up to date" : "—"}
            {s.last_update_check && <span className="muted"> · checked {ago(s.last_update_check)}</span>}
          </dd>
          <dt>Installed as</dt>
          <dd>{info.data ? `${info.data.bundle ?? "unpackaged build"} · ${info.data.os} ${info.data.arch}` : "…"}</dd>
        </dl>
        {managed && (
          <div className="notice info mt-2">This copy wasn't installed from an Auto Manager package (for example a Flatpak or a build from source), so it is updated by the way it was installed, not from here.</div>
        )}
        <div className="row mt-2">
          <button className="btn" onClick={() => u.check(s.update_channel)} disabled={u.checking || u.downloading}>
            {u.checking ? <Spinner /> : <RefreshCw size={16} aria-hidden />} {u.checking ? "Checking…" : "Check for Updates"}
          </button>
        </div>
        {u.error && <div className="notice bad mt-2">{u.error}</div>}
        {u.available && (
          <div className="stack mt-2">
            <details open>
              <summary>What's new in {u.available.version}</summary>
              <pre className="notes mt-2">{u.available.notes || "No release notes."}</pre>
            </details>
            {!u.ready && (
              <button className="btn primary" onClick={u.download} disabled={u.downloading}>
                {u.downloading ? <Spinner /> : <Download size={16} aria-hidden />} Download update
              </button>
            )}
            {u.progress && !u.ready && (
              <div>
                <ProgressBar value={u.progress.downloaded} max={u.progress.total ?? 0} label="Download progress" />
                <p className="small muted">
                  {bytes(u.progress.downloaded)}
                  {u.progress.total ? ` of ${bytes(u.progress.total)}` : ""}
                </p>
              </div>
            )}
            {u.ready && (
              <div className="notice ok spread">
                <span>The update is downloaded and its signature checked. Your data will be backed up before it installs.</span>
                <button className="btn primary" onClick={u.installAndRestart}>
                  Restart and install
                </button>
              </div>
            )}
          </div>
        )}
      </section>
      <section className="card">
        <h2>Preferences</h2>
        <Setting label="Check for updates when Auto Manager starts" hint="At most once a day.">
          <Switch checked={s.update_check} onChange={(update_check) => save({ update_check })} label="Check for updates automatically" />
        </Setting>
        <Setting label="Download updates automatically" hint="Installing always waits for you.">
          <Switch checked={s.update_download} disabled={!s.update_check} onChange={(update_download) => save({ update_download })} label="Download updates automatically" />
        </Setting>
        <Setting label="Release channel" hint="Beta gets new features first and may have rough edges. Switching back to stable never installs an older version.">
          <Segmented
            name="Release channel"
            value={s.update_channel as "stable" | "beta"}
            onChange={(update_channel) => save({ update_channel })}
            options={[
              { v: "stable", label: "Stable" },
              { v: "beta", label: "Beta" },
            ]}
          />
        </Setting>
        <Setting label="Release notes and downloads">
          <ExtLink href="https://github.com/fleetwoodjohnr/auto-manager/releases">All releases</ExtLink>
        </Setting>
      </section>
    </div>
  );
}

/** Move a complete, encrypted copy of the data to another computer. */
function SyncBundles({ busy, run }: { busy: string | null; run: (what: string, f: () => Promise<unknown>, ok: string) => Promise<void> }) {
  const [mode, setMode] = useState<"export" | "import" | null>(null);
  const [pass, setPass] = useState("");
  const { ask, dialog } = useConfirm();
  const go = async () => {
    if (mode === "export") {
      const path = await saveFile({ defaultPath: `auto-manager-${new Date().toISOString().slice(0, 10)}.amsync`, filters: [{ name: "Auto Manager sync bundle", extensions: ["amsync"] }] });
      if (path) await run("sync", () => call("sync_export", { path, passphrase: pass }), "Encrypted bundle saved");
    } else {
      const path = await openFile({ filters: [{ name: "Auto Manager sync bundle", extensions: ["amsync"] }] });
      if (typeof path !== "string") return;
      if (!(await ask("Replace this computer's data?", "Everything here is replaced by the bundle's contents. A backup of the current data is made first.", "Replace"))) return;
      await run("sync", () => call("sync_import", { path, passphrase: pass }), "Data replaced from the bundle");
    }
    setMode(null);
    setPass("");
  };
  return (
    <section className="card">
      {dialog}
      <h2>Move to another computer</h2>
      <p className="small muted">An encrypted bundle (a full backup locked with your passphrase) that you can carry on a USB stick or put in a synced folder, then open on another computer. It replaces everything on the other computer. To keep computers in step with each other, use Sync between computers above.</p>
      <div className="row">
        <button className="btn" onClick={() => setMode("export")}>
          Create encrypted bundle…
        </button>
        <button className="btn" onClick={() => setMode("import")}>
          Open a bundle…
        </button>
      </div>
      {mode && (
        <form
          className="row mt-2"
          onSubmit={(e) => {
            e.preventDefault();
            void go();
          }}
        >
          <input type="password" value={pass} onChange={(e) => setPass(e.target.value)} placeholder="Passphrase (at least 10 characters)" autoFocus aria-label="Passphrase" style={{ width: 300 }} />
          <button className="btn primary" disabled={pass.length < 10 || !!busy}>
            {mode === "export" ? "Choose where to save" : "Choose the bundle"}
          </button>
          <button type="button" className="btn ghost" onClick={() => setMode(null)}>
            Cancel
          </button>
        </form>
      )}
      <p className="small muted mt-1">Without the passphrase a bundle can't be opened, by anyone. Keep it somewhere safe.</p>
    </section>
  );
}
