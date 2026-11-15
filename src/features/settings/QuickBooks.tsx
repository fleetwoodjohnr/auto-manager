import { useEffect, useRef, useState, type FormEvent } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { Copy, FileSpreadsheet, KeyRound, Link2, RefreshCw, Unlink } from "lucide-react";
import { call, errorMessage } from "../../api/ipc";
import { useCmd, useRefresh } from "../../api/queries";
import type { Connection, KeySource, QbOptions, QbPreview, QbRow, QbSummary } from "../../api/types";
import { ago } from "../../lib/format";
import { toast } from "../../store";
import { Badge, ErrorNote, ExtLink, Field, Modal, Spinner, useConfirm } from "../../ui";
import { t, tm, tx } from "../../i18n";

const DEFAULTS: QbOptions = { use_prices: true, non_inventory: false, apply_stock: false, apply_cost: false };

const ENTITY: Record<string, string> = { item: t("Product"), customer: t("Customer"), vendor: t("Supplier") };
const STATUS: Record<string, { label: string; tone: "ok" | "info" | "warn" | "neutral" }> = {
  new: { label: t("Add"), tone: "ok" },
  update: { label: t("Update"), tone: "info" },
  unchanged: { label: t("No change"), tone: "neutral" },
  skipped: { label: t("Skip"), tone: "warn" },
};

function Options({ o, set }: { o: QbOptions; set: (o: QbOptions) => void }) {
  const box = (key: keyof QbOptions, label: string, hint: string) => (
    <label className="check">
      <input type="checkbox" checked={o[key]} onChange={(e) => set({ ...o, [key]: e.target.checked })} />
      <span>
        {label}
        <span className="hint">{hint}</span>
      </span>
    </label>
  );
  return (
    <div className="stack" style={{ gap: 6 }}>
      {box("use_prices", t("Use QuickBooks sales prices"), t("Otherwise your pricing rules set the price."))}
      {box("non_inventory", t("Include non-inventory products"), t("Added without stock."))}
      {box("apply_stock", t("Set stock to the QuickBooks quantity"), t("For items already here. Recorded as a count, so the history shows the change."))}
      {box("apply_cost", t("Set cost to the QuickBooks purchase cost"), t("For items already here."))}
    </div>
  );
}

/** Shows what an import would do, row by row, and applies it on request. */
export function QbReview({
  title,
  load,
  apply,
  onClose,
}: {
  title: string;
  load: (o: QbOptions) => Promise<QbPreview>;
  apply: (o: QbOptions) => Promise<QbSummary>;
  onClose: () => void;
}) {
  const refresh = useRefresh();
  const [o, setO] = useState<QbOptions>(DEFAULTS);
  const [preview, setPreview] = useState<QbPreview | null>(null);
  const [error, setError] = useState<unknown>(null);
  const [busy, setBusy] = useState(false);
  useEffect(() => {
    let live = true;
    setError(null);
    load(o)
      .then((p) => live && setPreview(p))
      .catch((e) => live && setError(e));
    return () => {
      live = false;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [o]);
  const go = async () => {
    setBusy(true);
    try {
      const s = await apply(o);
      toast(t("QuickBooks: {0} added, {1} updated, {2} skipped", s.added, s.updated, s.skipped));
      refresh();
      onClose();
    } catch (e) {
      toast(errorMessage(e), { tone: "bad" });
    } finally {
      setBusy(false);
    }
  };
  const changes = preview ? preview.new_count + preview.update_count : 0;
  return (
    <Modal
      open
      wide
      title={title}
      onClose={onClose}
      footer={
        <>
          <button className="btn" onClick={onClose}>{t("Cancel")}</button>
          <button className="btn primary" onClick={go} disabled={busy || !preview || changes === 0}>
            {busy && <Spinner />}{" "}{t("Apply changes")}</button>
        </>
      }
    >
      <div className="stack">
        <Options o={o} set={setO} />
        {error != null && <ErrorNote error={error} />}
        {!preview && error == null && <Spinner label={t("Reading QuickBooks data")} />}
        {preview && (
          <>
            <div className="row">
              <Badge tone="ok">{tx("{0} to add", preview.new_count)}</Badge>
              <Badge tone="info">{tx("{0} to update", preview.update_count)}</Badge>
              <Badge tone="neutral">{tx("{0} unchanged", preview.unchanged_count)}</Badge>
              <Badge tone="warn">{tx("{0} skipped", preview.skipped_count)}</Badge>
            </div>
            <div className="table-wrap" style={{ maxHeight: "46vh" }}>
              <table>
                <thead>
                  <tr>
                    <th>{t("Record")}</th>
                    <th>{t("Name")}</th>
                    <th>{t("What happens")}</th>
                  </tr>
                </thead>
                <tbody>
                  {preview.rows.map((r: QbRow, i) => (
                    <tr key={i}>
                      <td>{ENTITY[r.entity] ?? r.entity}</td>
                      <td>{r.name}</td>
                      <td>
                        <Badge tone={STATUS[r.status]?.tone ?? "neutral"}>{STATUS[r.status]?.label ?? r.status}</Badge>
                        {r.notes.map((n, j) => (
                          <div key={j} className="small muted">{tm(n)}</div>
                        ))}
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
            <p className="small muted">{t("QuickBooks isn't changed. Everything is applied in one step, so a failure leaves nothing half-done. Importing again later updates these same records.")}</p>
          </>
        )}
      </div>
    </Modal>
  );
}

const KIND: Record<string, string> = { items: t("products and services"), customers: t("customers"), vendors: t("vendors") };

function ImportExport() {
  const [path, setPath] = useState<string | null>(null);
  const [kind, setKind] = useState<string>("");
  const pick = async () => {
    const p = await open({ filters: [{ name: "QuickBooks export", extensions: ["xlsx", "xls", "csv"] }] });
    if (typeof p === "string") setPath(p);
  };
  return (
    <section className="card">
      <h2>{t("Import a QuickBooks export")}</h2>
      <p className="muted">{t("Works with QuickBooks Online and QuickBooks Desktop. Export a list to Excel, then choose the file here. Products, customers and vendors are each their own file.")}</p>
      <details className="mt-2">
        <summary>{t("How to export from QuickBooks")}</summary>
        <ul className="small">
          <li>{t("QuickBooks Online: ⚙ Settings → Products and services → More → Run report → Export to Excel. Customers and vendors: open the list, then Export to Excel.")}</li>
          <li>{t("QuickBooks Desktop: Lists → Item List → Excel → Export all items. Customer Center or Vendor Center → Excel → Export Customer List or Vendor List.")}</li>
        </ul>
      </details>
      <div className="row mt-2">
        <button className="btn primary" onClick={pick}>
          <FileSpreadsheet size={16} aria-hidden />{" "}{t("Choose export…")}</button>
      </div>
      {path && (
        <QbReview
          title={kind ? t("Import QuickBooks {0}", KIND[kind] ?? kind) : t("Import from QuickBooks")}
          load={async (options) => {
            const r = await call<{ kind: string; preview: QbPreview }>("quickbooks_file_preview", { path, options });
            setKind(r.kind);
            return r.preview;
          }}
          apply={(options) => call<QbSummary>("quickbooks_file_apply", { path, options })}
          onClose={() => {
            setPath(null);
            setKind("");
          }}
        />
      )}
    </section>
  );
}

type Status = {
  available: boolean;
  environment: string | null;
  source: KeySource | null;
  client_id: string | null;
  redirect: string;
  connection: Connection | null;
  signing_in: boolean;
};

const short = (id: string) => (id.length > 12 ? `${id.slice(0, 4)}…${id.slice(-4)}` : id);

/** The person's own Intuit developer app: where to get its keys, and the boxes to enter them. */
function KeysForm({ redirect, onSaved, onCancel }: { redirect: string; onSaved: () => void; onCancel?: () => void }) {
  const [clientId, setClientId] = useState("");
  const [secret, setSecret] = useState("");
  const [production, setProduction] = useState(false);
  const [error, setError] = useState<unknown>(null);
  const [busy, setBusy] = useState(false);
  const copy = async () => {
    try {
      await navigator.clipboard.writeText(redirect);
      toast(t("Copied"));
    } catch {
      toast(t("Select the address and copy it."), { tone: "bad" });
    }
  };
  const save = async (e: FormEvent) => {
    e.preventDefault();
    setBusy(true);
    setError(null);
    try {
      await call("quickbooks_keys_save", { clientId, clientSecret: secret, production });
      setSecret("");
      toast(t("QuickBooks keys saved"));
      onSaved();
    } catch (err) {
      setError(err);
    } finally {
      setBusy(false);
    }
  };
  return (
    <form className="stack mt-2" onSubmit={save}>
      <ol className="small stack" style={{ gap: 6, paddingLeft: 20, listStyle: "decimal" }}>
        <li>{tx("Sign in at {0} and create an app for QuickBooks Online with the Accounting scope.", <ExtLink key="l" href="https://developer.intuit.com/app/developer/dashboard">developer.intuit.com</ExtLink>)}</li>
        <li>
          {t("In the app's settings, add this redirect URI: under Development for a sandbox company, under Production for your real company.")}
          <div className="row mt-2">
            <input type="text" className="grow" readOnly value={redirect} aria-label={t("Redirect URI")} onFocus={(e) => e.currentTarget.select()} />
            <button type="button" className="btn" onClick={copy}>
              <Copy size={16} aria-hidden />{" "}{t("Copy")}</button>
          </div>
        </li>
        <li>{t("From the app's Keys and credentials, copy the Client ID and Client Secret into the boxes below.")}</li>
      </ol>
      <div className="grid" style={{ gridTemplateColumns: "repeat(auto-fit, minmax(220px, 1fr))" }}>
        <Field label={t("Keys for")}>
          <select value={production ? "production" : "sandbox"} onChange={(e) => setProduction(e.target.value === "production")}>
            <option value="sandbox">{t("A sandbox company (Development keys)")}</option>
            <option value="production">{t("A real company (Production keys)")}</option>
          </select>
        </Field>
        <Field label={t("Client ID")}>
          <input type="text" value={clientId} onChange={(e) => setClientId(e.target.value)} autoComplete="off" spellCheck={false} />
        </Field>
        <Field label={t("Client Secret")}>
          <input type="password" value={secret} onChange={(e) => setSecret(e.target.value)} autoComplete="new-password" />
        </Field>
      </div>
      {error != null && <ErrorNote error={error} />}
      <div className="row">
        <button type="submit" className="btn primary" disabled={busy || !clientId.trim() || !secret.trim()}>
          {busy ? <Spinner /> : <KeyRound size={16} aria-hidden />}{" "}{t("Save keys")}</button>
        {onCancel && <button type="button" className="btn" onClick={onCancel}>{t("Cancel")}</button>}
      </div>
      <p className="small muted">{t("The keys stay on this computer, in the system keyring. The Client Secret isn't shown again.")}</p>
    </form>
  );
}

function SignInDialog({ onClose }: { onClose: () => void }) {
  const [pasted, setPasted] = useState("");
  const [error, setError] = useState<unknown>(null);
  const finish = async () => {
    setError(null);
    try {
      await call("quickbooks_finish", { url: pasted });
    } catch (e) {
      setError(e);
    }
  };
  return (
    <Modal
      open
      title={t("Connect to QuickBooks")}
      onClose={onClose}
      footer={<button className="btn" onClick={onClose}>{t("Cancel")}</button>}
    >
      <div className="stack">
        <p>
          <Spinner />{" "}{t("Finish signing in to QuickBooks in your browser, then choose the company to connect.")}</p>
        <p className="small muted">{t("If your browser shows a page asking you to paste its address, copy that address here:")}</p>
        <div className="row">
          <input className="grow" aria-label={t("Address from the browser")} value={pasted} onChange={(e) => setPasted(e.target.value)} placeholder="https://…" />
          <button className="btn" onClick={finish} disabled={!pasted.trim()}>{t("Continue")}</button>
        </div>
        {error != null && <ErrorNote error={error} />}
      </div>
    </Modal>
  );
}

function Online() {
  const status = useCmd<Status>("quickbooks_status");
  const { ask, dialog } = useConfirm();
  const [signingIn, setSigningIn] = useState(false);
  const [reviewing, setReviewing] = useState(false);
  const [error, setError] = useState<unknown>(null);
  const [editingKeys, setEditingKeys] = useState(false);
  const first = useRef(true);
  const s = status.data;
  const keysSaved = async () => {
    setEditingKeys(false);
    await status.refetch();
  };
  const removeKeys = async () => {
    if (!(await ask(t("Remove the QuickBooks keys?"), t("Auto Manager forgets your Intuit app's keys on this computer. You can enter them again at any time."), t("Remove keys")))) return;
    try {
      await call("quickbooks_keys_forget");
      await status.refetch();
    } catch (e) {
      setError(e);
    }
  };
  const connect = async () => {
    setError(null);
    setSigningIn(true);
    try {
      const c = await call<Connection>("quickbooks_connect");
      toast(t("Connected to {0}", c.company));
      await status.refetch();
      first.current = true;
      setReviewing(true);
    } catch (e) {
      if (!/cancelled/.test(errorMessage(e))) setError(e);
    } finally {
      setSigningIn(false);
    }
  };
  const cancel = async () => {
    await call("quickbooks_cancel");
    setSigningIn(false);
  };
  const disconnect = async () => {
    if (!(await ask(t("Disconnect QuickBooks?"), t("Auto Manager stops reading from this company. What was imported stays, and connecting again later updates the same records."), t("Disconnect")))) return;
    try {
      const note = await call<string | null>("quickbooks_disconnect");
      if (note) toast(note, { tone: "bad" });
      await status.refetch();
    } catch (e) {
      setError(e);
    }
  };
  if (!s) return <Spinner label={t("Loading")} />;
  const c = s.connection;
  return (
    <section className="card">
      <h2>{t("QuickBooks Online")}</h2>
      {!s.available ? (
        <>
          <p className="muted">{t("To connect, Auto Manager needs the keys of an Intuit developer app. Creating one is free; then enter its keys here. Or import an export below: it brings in the same products, prices, stock, customers and vendors.")}</p>
          <KeysForm redirect={s.redirect} onSaved={keysSaved} />
        </>
      ) : c ? (
        <>
          <p>
            {tx("Connected to {0}", <strong key="c">{c.company}</strong>)}{" "}
            {c.environment === "sandbox" && <Badge tone="warn">{t("sandbox")}</Badge>}
          </p>
          <p className="small muted">{c.last_refresh ? tx("Last brought in {0}.", ago(c.last_refresh)) : t("Nothing brought in yet.")}{" "}{t("Auto Manager only reads from QuickBooks.")}</p>
          {s.source === "settings" && s.client_id && (
            <p className="small muted">{tx("Using your Intuit app's keys (Client ID {0}). Disconnect to change them.", <code key="i">{short(s.client_id)}</code>)}</p>
          )}
          <div className="row mt-2">
            <button
              className="btn primary"
              onClick={() => {
                first.current = true;
                setReviewing(true);
              }}
            >
              <RefreshCw size={16} aria-hidden />{" "}{t("Bring in from QuickBooks…")}</button>
            <button className="btn" onClick={disconnect}>
              <Unlink size={16} aria-hidden />{" "}{t("Disconnect")}</button>
          </div>
        </>
      ) : (
        <>
          <p className="muted">{t("Sign in with your QuickBooks account and choose the company. Auto Manager then reads your products and services, prices, stock, customers and vendors, and shows what it will bring in before saving anything. It never changes QuickBooks.")}</p>
          <div className="row mt-2">
            <button className="btn primary" onClick={connect} disabled={signingIn}>
              <Link2 size={16} aria-hidden />{" "}{t("Connect to QuickBooks")}</button>
            {s.environment === "sandbox" && <Badge tone="warn">{t("sandbox")}</Badge>}
          </div>
          {s.source === "settings" && s.client_id &&
            (editingKeys ? (
              <KeysForm redirect={s.redirect} onSaved={keysSaved} onCancel={() => setEditingKeys(false)} />
            ) : (
              <p className="small muted mt-2">
                {tx("Using your Intuit app's keys (Client ID {0}).", <code key="i">{short(s.client_id)}</code>)}{" "}
                <button className="btn link" onClick={() => setEditingKeys(true)}>{t("Change keys")}</button>
                <button className="btn link" onClick={removeKeys}>{t("Remove keys")}</button>
              </p>
            ))}
          {s.source === "built_in" && (
            <details className="mt-2">
              <summary>{t("Use your own Intuit app instead")}</summary>
              <KeysForm redirect={s.redirect} onSaved={keysSaved} />
            </details>
          )}
          {s.source === "environment" && <p className="small muted mt-2">{t("Using the Intuit app keys from the environment (AUTO_MANAGER_QBO_CLIENT_ID).")}</p>}
        </>
      )}
      {error != null && <div className="mt-2"><ErrorNote error={error} /></div>}
      {signingIn && <SignInDialog onClose={cancel} />}
      {reviewing && c && (
        <QbReview
          title={t("Bring in from {0}", c.company)}
          load={(options) => {
            const reload = first.current;
            first.current = false;
            return call<QbPreview>("quickbooks_preview", { options, reload });
          }}
          apply={async (options) => {
            const r = await call<QbSummary>("quickbooks_apply", { options });
            await status.refetch();
            return r;
          }}
          onClose={() => setReviewing(false)}
        />
      )}
      {dialog}
    </section>
  );
}

export function QuickBooks() {
  return (
    <div className="stack">
      <Online />
      <ImportExport />
    </div>
  );
}
