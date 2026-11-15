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
import { bytes, dateTime } from "../../lib/format";
import { useUpdates } from "./updates";
import { FolderSync } from "./FolderSync";
import { QuickBooks } from "./QuickBooks";
import { t, tx, lang, LANGUAGES, setLanguage, type Lang } from "../../i18n";

const WIDGET_LABEL: Record<string, string> = {
  shortcuts: "Shortcuts",
  recent_vehicles: t("Recently searched vehicles"),
  garage: t("Saved vehicles"),
  inventory: t("Inventory value and counts"),
  low_stock: t("Low stock and restocking"),
  jobs: t("Open service jobs"),
  maintenance: t("Upcoming maintenance"),
  recalls: t("Recall notices"),
  activity: t("Recent inventory activity"),
  updates: t("Application and data status"),
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
      <PageHead title={t("Settings")} sub={t("Saved as you change them.")} />
      <Tabs
        label={t("Settings sections")}
        value={tab}
        onChange={setTab}
        tabs={[
          { id: "general", label: t("General") },
          { id: "data_sources", label: t("Vehicle data") },
          { id: "inventory", label: t("Inventory") },
          { id: "pricing", label: t("Pricing") },
          { id: "data", label: t("Data and backups") },
          { id: "quickbooks", label: "QuickBooks" },
          { id: "updates", label: t("Updates") },
        ]}
      />
      {settings ? (
        <>
          {tab === "general" && <General s={settings} save={save} />}
          {tab === "data_sources" && <DataSources s={settings} save={save} />}
          {tab === "inventory" && <InventorySettings s={settings} save={save} />}
          {tab === "pricing" && <PricingSettings />}
          {tab === "data" && <DataSettings s={settings} save={save} />}
          {tab === "quickbooks" && <QuickBooks />}
          {tab === "updates" && <Updates s={settings} save={save} />}
        </>
      ) : (
        <Spinner label={t("Loading settings")} />
      )}
    </div>
  );
}

type Props = { s: S; save: (p: Partial<S>) => void };

function General({ s, save }: Props) {
  return (
    <div className="stack">
      <section className="card">
        <h2>{t("Appearance")}</h2>
        <Setting label={t("Theme")}>
          <Segmented
            name={t("Theme")}
            value={s.theme as "system" | "light" | "dark"}
            onChange={(theme) => save({ theme })}
            options={[
              { v: "system", label: t("System"), icon: <Monitor size={14} aria-hidden /> },
              { v: "light", label: t("Light"), icon: <Sun size={14} aria-hidden /> },
              { v: "dark", label: t("Dark"), icon: <Moon size={14} aria-hidden /> },
            ]}
          />
        </Setting>
        <Setting label={t("Start on")} hint={t("The page Auto Manager opens to.")}>
          <select value={s.landing_page} onChange={(e) => save({ landing_page: e.target.value })} aria-label={t("Start page")}>
            <option value="dashboard">{t("Dashboard")}</option>
            <option value="lookup">{t("Vehicle Lookup")}</option>
            <option value="vehicles">{t("My Vehicles")}</option>
            <option value="parts">{t("Parts Catalog")}</option>
            <option value="inventory">{t("Inventory")}</option>
            <option value="service">{t("Maintenance &amp; Service")}</option>
          </select>
        </Setting>
      </section>
      <section className="card">
        <h2>{t("Formats")}</h2>
        <Setting label={t("Measurement units")} hint={t("Specifications keep the units their source gave; this only changes how they're shown.")}>
          <Segmented
            name={t("Units")}
            value={s.units as "imperial" | "metric"}
            onChange={(units) => save({ units })}
            options={[
              { v: "imperial", label: t("Miles, lb, gal") },
              { v: "metric", label: t("Km, kg, litres") },
            ]}
          />
        </Setting>
        <Setting label={t("Date format")}>
          <select value={s.date_format} onChange={(e) => save({ date_format: e.target.value })} aria-label={t("Date format")}>
            <option value="system">{t("As this computer")}</option>
            <option value="iso">2026-11-14</option>
            <option value="us">11/14/2026</option>
            <option value="eu">14/11/2026</option>
          </select>
        </Setting>
        <Setting label={t("Currency")} hint={t("Three-letter code, e.g. USD, CAD, EUR, GBP. All amounts are in this one currency.")}>
          <input
            type="text"
            defaultValue={s.currency}
            maxLength={3}
            style={{ width: 80, textTransform: "uppercase" }}
            aria-label={t("Currency code")}
            onBlur={(e) => {
              const c = e.target.value.trim().toUpperCase();
              if (c !== s.currency) save({ currency: c });
            }}
          />
        </Setting>
        <Setting label={t("Language")} hint={t("The app restarts its window in the chosen language. Data from NHTSA and the EPA stays in English, as published.")}>
          <select value={lang} onChange={(e) => setLanguage(e.target.value as Lang)} aria-label={t("Language")}>
            {LANGUAGES.map((l) => (
              <option key={l.id} value={l.id}>
                {l.label}
              </option>
            ))}
          </select>
        </Setting>
      </section>
      <section className="card">
        <h2>{t("Dashboard")}</h2>
        <p className="small muted">{t("Choose what the dashboard shows. Hidden sections can be turned back on here.")}</p>
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
  const setPriority = useAction<{ id: string; priority: number }>("data_source_priority", { invalidate: ["data_sources", "vehicle_detail"], success: t("Priority saved") });
  const clear = useAction("cache_clear", { invalidate: ["cache_stats"], success: (n) => `Removed ${n} saved answers` });
  return (
    <div className="stack">
      <section className="card">
        <h2>{t("Data providers")}</h2>
        <p className="small muted">{t("When two sources disagree about a value, the one with the lower priority number is shown and the disagreement is flagged. Your own entries always come first. None of the built-in providers needs an account or an API key.")}</p>
        <Loaded q={sources}>
          {(list) => (
            <div className="table-wrap" style={{ maxHeight: "none" }}>
              <table>
                <thead>
                  <tr>
                    <th>{t("Source")}</th>
                    <th>{t("Type")}</th>
                    <th>{t("Records")}</th>
                    <th>{t("Terms")}</th>
                    <th>{t("Priority")}</th>
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
                        <Badge tone={d.kind === "provider" ? "info" : d.kind === "user" ? "ok" : "neutral"}>{d.kind === "provider" ? t("Online provider") : d.kind === "user" ? t("You") : t("Import")}</Badge>
                      </td>
                      <td>{d.records}</td>
                      <td className="small">{d.license ?? "—"}</td>
                      <td>
                        {d.id === "user" ? (
                          <span className="muted">{t("Always first")}</span>
                        ) : (
                          <input
                            type="number"
                            min={1}
                            max={1000}
                            defaultValue={d.priority}
                            style={{ width: 80 }}
                            aria-label={t("Priority of {0}", d.name)}
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
        <p className="small muted mt-2">{t("What each provider can supply, and its terms, is listed in the provider documentation (docs/PROVIDERS.md).")}</p>
      </section>
      <section className="card">
        <h2>{t("Saved answers (offline cache)")}</h2>
        <Setting label={t("Refresh saved answers after")} hint={t("Lookups that worked once keep working offline; after this many days they are refreshed when online.")}>
          <select value={s.provider_refresh_days} onChange={(e) => save({ provider_refresh_days: Number(e.target.value) })} aria-label={t("Refresh days")}>
            {[1, 7, 30, 90, 365].map((d) => (
              <option key={d} value={d}>{tx("{0} day{1}", d, d > 1 ? "s" : "")}</option>
            ))}
          </select>
        </Setting>
        <Setting label={t("Stored")} hint={cache.data?.oldest ? t("Oldest from {0}", dateTime(cache.data.oldest)) : undefined}>
          <div className="row">
            <span className="muted">{cache.data ? t("{0} answers · {1}", cache.data.entries, bytes(cache.data.bytes)) : "…"}</span>
            <button
              className="btn"
              onClick={() => {
                clear.mutate({});
                refresh();
              }}
              disabled={!cache.data?.entries}
            >{t("Clear")}</button>
          </div>
        </Setting>
      </section>
    </div>
  );
}

function InventorySettings({ s, save }: Props) {
  const locs = useCmd<Location[]>("location_list");
  const add = useAction<{ name: string }>("location_add", { invalidate: ["location_list"], success: t("Location added") });
  const rename = useAction<{ id: string; name: string }>("location_rename", { invalidate: ["location_list", "item_list"], success: t("Renamed") });
  const setDefault = useAction<{ id: string }>("location_set_default", { invalidate: ["location_list"], success: t("Default location changed") });
  const [name, setName] = useState("");
  return (
    <div className="stack">
      <section className="card">
        <h2>{t("Locations")}</h2>
        <p className="small muted">{t("Warehouses, shelves, vans: anywhere stock is kept. Damaged stock goes to the quarantine location, where it can't be sold or reserved.")}</p>
        <Loaded q={locs}>
          {(list) => (
            <ul className="items" style={{ maxHeight: "none" }}>
              {list.map((l) => (
                <li key={l.id} className="spread">
                  <input
                    type="text"
                    defaultValue={l.name}
                    aria-label={t("Location name")}
                    onBlur={(e) => e.target.value.trim() !== l.name && rename.mutate({ id: l.id, name: e.target.value })}
                    style={{ flex: 1, maxWidth: 320 }}
                  />
                  <span className="row">
                    <span className="small muted">{tx("{0} items", l.item_count)}</span>
                    {l.is_default ? (
                      <Badge tone="info">{t("Default")}</Badge>
                    ) : l.kind === "damaged" ? (
                      <Badge tone="warn">{t("Quarantine")}</Badge>
                    ) : (
                      <button className="btn link" onClick={() => setDefault.mutate({ id: l.id })}>{t("Make default")}</button>
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
          <input type="text" placeholder={t("New location, e.g. Back shelf")} value={name} onChange={(e) => setName(e.target.value)} aria-label={t("New location name")} />
          <button className="btn" disabled={!name.trim()}>{t("Add location")}</button>
        </form>
      </section>
      <section className="card">
        <h2>{t("Stock")}</h2>
        <Setting label={t("Inventory valuation")} hint={t("Average cost follows every delivery (weighted average). Standard uses each item's last purchase cost.")}>
          <Segmented
            name={t("Valuation")}
            value={s.valuation_method as "average" | "standard"}
            onChange={(valuation_method) => save({ valuation_method })}
            options={[
              { v: "average", label: t("Average cost") },
              { v: "standard", label: t("Last cost") },
            ]}
          />
        </Setting>
        <Setting label={t("Suggested minimum stock")} hint={t("Pre-filled on new items; low-stock alerts start at or below it.")}>
          <input type="number" min={0} defaultValue={s.default_min_stock} style={{ width: 90 }} aria-label={t("Default minimum stock")} onBlur={(e) => save({ default_min_stock: Math.max(0, Number(e.target.value) || 0) })} />
        </Setting>
        <Setting label={t("Label barcodes")} hint={t("Barcode type printed on inventory labels. USB scanners work with both.")}>
          <Segmented
            name={t("Label barcode")}
            value={s.label_symbology as "code128" | "qr"}
            onChange={(label_symbology) => save({ label_symbology })}
            options={[
              { v: "code128", label: t("Code 128") },
              { v: "qr", label: t("QR code") },
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
  const setDefault = useAction<{ id: string }>("profile_set_default", { invalidate: ["profile_list", "item_list", "vehicle_matches"], success: t("Default profile changed") });
  return (
    <section className="card">
      <h2>{t("Pricing")}</h2>
      <Loaded q={profiles}>
        {(list) => (
          <>
            <Setting label={t("Default pricing profile")} hint={t("Used when neither the job, the customer nor the item names a profile.")}>
              <select value={list.find((p) => p.is_default)?.id} onChange={(e) => setDefault.mutate({ id: e.target.value })} aria-label={t("Default pricing profile")}>
                {list.map((p) => (
                  <option key={p.id} value={p.id}>
                    {p.name}
                  </option>
                ))}
              </select>
            </Setting>
            <Setting label={t("Profiles, rules, labor rates and tax")}>
              <button className="btn" onClick={() => go("pricing")}>{t("Open Pricing")}</button>
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
    if (!(await ask(t("Restore this backup?"), <>{tx("Your current data will be replaced by {0}. A backup of the current data is made first, and the previous files are kept in the data folder.", label)}</>, t("Restore")))) return;
    await run("restore", () => call("backup_restore", { path }), t("Backup restored"));
  };
  return (
    <div className="stack">
      {dialog}
      <section className="card">
        <h2>{t("Backups")}</h2>
        <Setting label={t("Automatic daily backup")} hint={t("Kept for 14 days. A backup is also made before every update, data upgrade and restore.")}>
          <Switch checked={s.backup_auto} onChange={(backup_auto) => save({ backup_auto })} label={t("Automatic daily backup")} />
        </Setting>
        <div className="row">
          <button className="btn primary" disabled={!!busy} onClick={() => run("now", () => call("backup_now"), t("Backup made"))}>
            {busy === "now" ? <Spinner /> : <HardDriveDownload size={16} aria-hidden />}{" "}{t("Back up now")}</button>
          <button
            className="btn"
            disabled={!!busy}
            onClick={async () => {
              const path = await saveFile({ defaultPath: `auto-manager-backup-${new Date().toISOString().slice(0, 10)}.zip`, filters: [{ name: "Backup", extensions: ["zip"] }] });
              if (path) await run("to", () => call("backup_to", { path }), t("Backup saved"));
            }}
          >
            <Download size={16} aria-hidden />{" "}{t("Save a backup to…")}</button>
          <button
            className="btn"
            disabled={!!busy}
            onClick={async () => {
              const path = await openFile({ filters: [{ name: "Backup", extensions: ["zip"] }] });
              if (typeof path === "string") await restore(path);
            }}
          >
            <Upload size={16} aria-hidden />{" "}{t("Restore from file…")}</button>
        </div>
        <Loaded q={backups}>
          {(list) =>
            list.length === 0 ? (
              <p className="small muted mt-2">{t("No backups yet.")}</p>
            ) : (
              <div className="table-wrap mt-2" style={{ maxHeight: 280 }}>
                <table>
                  <thead>
                    <tr>
                      <th>{t("Made")}</th>
                      <th>{t("Why")}</th>
                      <th>{t("Size")}</th>
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
                            <button className="btn link" disabled={!!busy} onClick={() => restore(b.path)}>{t("Restore")}</button>
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
        <h2>{t("Export")}</h2>
        <div className="row">
          {(["csv", "xlsx"] as const).map((ext) => (
            <button
              key={ext}
              className="btn"
              onClick={async () => {
                const path = await saveFile({ defaultPath: `inventory.${ext}`, filters: [{ name: ext.toUpperCase(), extensions: [ext] }] });
                if (path) await run("export", () => call("export_inventory", { path }), t("Inventory exported"));
              }}
            >{tx("Export inventory ({0})", ext.toUpperCase())}</button>
          ))}
          <button
            className="btn"
            onClick={async () => {
              const path = await saveFile({ defaultPath: "auto-manager-data.json", filters: [{ name: "JSON", extensions: ["json"] }] });
              if (path) await run("json", () => call("export_json", { path }), t("All data exported"));
            }}
          >{t("Export everything (JSON)")}</button>
        </div>
      </section>
      <section className="card">
        <h2>{t("Storage")}</h2>
        {info.data && (
          <dl className="kv">
            <dt>{t("Data folder")}</dt>
            <dd className="mono">{info.data.data_dir}</dd>
            <dt>{t("Data version")}</dt>
            <dd>{info.data.schema_version}</dd>
          </dl>
        )}
        <div className="row mt-2">
          <button className="btn" onClick={() => info.data && revealItemInDir(info.data.data_dir + "/auto-manager.db").catch((e) => toast(errorMessage(e), { tone: "bad" }))}>
            <FolderOpen size={16} aria-hidden />{" "}{t("Show in folder")}</button>
          <button
            className="btn"
            disabled={!!busy}
            onClick={async () => {
              setBusy("check");
              try {
                const r = await call<string>("integrity_check");
                toast(r === "ok" ? t("The database is healthy.") : t("Problems found: {0}", r), r === "ok" ? undefined : { tone: "bad" });
              } catch (e) {
                toast(errorMessage(e), { tone: "bad" });
              } finally {
                setBusy(null);
              }
            }}
          >
            {busy === "check" ? <Spinner /> : <ShieldCheck size={16} aria-hidden />}{" "}{t("Check database")}</button>
        </div>
        <p className="small muted mt-2">{t("Your data stays on this computer. Nothing is uploaded, and there is no account or telemetry.")}</p>
      </section>
    </div>
  );
}

function Updates({ s, save }: Props) {
  const info = useCmd<AppInfo>("app_info");
  const u = useUpdates();
  const managed = info.data?.os === "linux" && !info.data.bundle;
  const busy = u.state === "downloading" || u.state === "installing";
  return (
    <div className="stack">
      <section className="card">
        <h2>{t("Version")}</h2>
        <dl className="kv">
          <dt>{t("Installed")}</dt>
          <dd>
            {info.data?.version ?? "…"} {info.data?.debug && <Badge tone="warn">{t("development build")}</Badge>}
          </dd>
          <dt>{t("Installed as")}</dt>
          <dd>{info.data ? `${info.data.bundle ?? "unpackaged build"} · ${info.data.os} ${info.data.arch}` : "…"}</dd>
        </dl>
        {managed && (
          <div className="notice info mt-2">{t("This copy wasn't installed from an Auto Manager package (for example a Flatpak or a build from source), so it is updated by the way it was installed, not from here.")}</div>
        )}
        <Setting
          label={u.update ? t("Version {0} is available", u.update.version) : u.state === "none" ? t("You're up to date") : t("Updates")}
          hint={t("Updates are installed only if their signature matches Auto Manager's release key. Your data is backed up first, then the app restarts.")}
        >
          {u.update ? (
            <button className="btn primary" disabled={busy || u.state === "checking"} onClick={() => void u.install()}>
              {busy ? <Spinner /> : <Download size={16} aria-hidden />}{" "}
              {u.state === "downloading" ? t("Downloading…") : u.state === "installing" ? t("Installing…") : t("Install and restart")}
            </button>
          ) : (
            <button className="btn" disabled={u.state === "checking"} onClick={() => void u.checkNow()}>
              {u.state === "checking" ? <Spinner /> : <RefreshCw size={16} aria-hidden />}{" "}{u.state === "checking" ? t("Checking…") : t("Check now")}
            </button>
          )}
        </Setting>
        {u.state === "downloading" && (
          <div role="status">
            <ProgressBar value={u.downloaded} max={u.total ?? 0} label={t("Download progress")} />
            <p className="small muted">
              {bytes(u.downloaded)}
              {u.total ? t(" of {0}", bytes(u.total)) : ""}
            </p>
          </div>
        )}
        {u.update?.body && (
          <details open className="mt-2">
            <summary>{tx("What's new in {0}", u.update.version)}</summary>
            <pre className="notes mt-2">{u.update.body}</pre>
          </details>
        )}
        {u.state === "error" && <div className="notice bad mt-2">{t("Update failed: {0}", u.error ?? "")}</div>}
      </section>
      <section className="card">
        <h2>{t("Preferences")}</h2>
        <Setting label={t("Check for updates automatically")} hint={t("At start-up and once a day, from GitHub. Nothing installs until you choose to.")}>
          <Switch checked={s.update_check} onChange={(update_check) => save({ update_check })} label={t("Check for updates automatically")} />
        </Setting>
        <Setting label={t("Release notes and downloads")}>
          <ExtLink href="https://github.com/fleetwoodjohnr/auto-manager/releases">{t("All releases")}</ExtLink>
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
      const path = await saveFile({ defaultPath: `auto-manager-${new Date().toISOString().slice(0, 10)}.amsync`, filters: [{ name: t("Auto Manager sync bundle"), extensions: ["amsync"] }] });
      if (path) await run("sync", () => call("sync_export", { path, passphrase: pass }), t("Encrypted bundle saved"));
    } else {
      const path = await openFile({ filters: [{ name: t("Auto Manager sync bundle"), extensions: ["amsync"] }] });
      if (typeof path !== "string") return;
      if (!(await ask(t("Replace this computer's data?"), t("Everything here is replaced by the bundle's contents. A backup of the current data is made first."), t("Replace")))) return;
      await run("sync", () => call("sync_import", { path, passphrase: pass }), t("Data replaced from the bundle"));
    }
    setMode(null);
    setPass("");
  };
  return (
    <section className="card">
      {dialog}
      <h2>{t("Move to another computer")}</h2>
      <p className="small muted">{t("An encrypted bundle (a full backup locked with your passphrase) that you can carry on a USB stick or put in a synced folder, then open on another computer. It replaces everything on the other computer. To keep computers in step with each other, use Sync between computers above.")}</p>
      <div className="row">
        <button className="btn" onClick={() => setMode("export")}>{t("Create encrypted bundle…")}</button>
        <button className="btn" onClick={() => setMode("import")}>{t("Open a bundle…")}</button>
      </div>
      {mode && (
        <form
          className="row mt-2"
          onSubmit={(e) => {
            e.preventDefault();
            void go();
          }}
        >
          <input type="password" value={pass} onChange={(e) => setPass(e.target.value)} placeholder={t("Passphrase (at least 10 characters)")} autoFocus aria-label={t("Passphrase")} style={{ width: 300 }} />
          <button className="btn primary" disabled={pass.length < 10 || !!busy}>
            {mode === "export" ? t("Choose where to save") : t("Choose the bundle")}
          </button>
          <button type="button" className="btn ghost" onClick={() => setMode(null)}>{t("Cancel")}</button>
        </form>
      )}
      <p className="small muted mt-1">{t("Without the passphrase a bundle can't be opened, by anyone. Keep it somewhere safe.")}</p>
    </section>
  );
}
