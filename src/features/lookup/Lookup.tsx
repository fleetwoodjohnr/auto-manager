import { useEffect, useMemo, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { CircleAlert, CircleCheck, CloudOff, ScanSearch, Star, TriangleAlert, Camera } from "lucide-react";
import { call, errorField, errorMessage } from "../../api/ipc";
import { useAction, useCmd } from "../../api/queries";
import type { DecodeOutcome, NameList, SpecField, Vehicle, VinCheck } from "../../api/types";
import { useApp } from "../../store";
import { useSettings } from "../../settings";
import { Badge, ErrorNote, Field, PageHead, Spinner, Tabs } from "../../ui";
import { dateTime, humanKey } from "../../lib/format";
import { showValue, type UnitSystem } from "../../lib/units";
import BulkDecode from "./BulkDecode";
import { ScanDialog } from "../shared/ScanDialog";
import { t, tx } from "../../i18n";

export default function Lookup() {
  const { route, setTab } = useApp();
  const tab = route.tab ?? "vin";
  return (
    <div className="page">
      <PageHead title={t("Vehicle Lookup")} sub={t("Enter a VIN to decode it with NHTSA's free vPIC service, or identify an older or non-standard vehicle yourself.")} />
      <Tabs
        label={t("How to identify the vehicle")}
        value={tab}
        onChange={setTab}
        tabs={[
          { id: "vin", label: t("By VIN") },
          { id: "manual", label: t("Without a VIN") },
          { id: "bulk", label: t("Many VINs (CSV)") },
        ]}
      />
      {tab === "vin" && <VinLookup initial={route.arg} />}
      {tab === "manual" && <ManualEntry />}
      {tab === "bulk" && <BulkDecode />}
    </div>
  );
}

function checkBadge(c: VinCheck | undefined) {
  if (!c) return null;
  switch (c.status) {
    case "valid":
      return (
        <Badge tone="ok">
          <CircleCheck size={12} aria-hidden />{" "}{t("Check digit correct")}</Badge>
      );
    case "check_digit_mismatch":
      return (
        <Badge tone="warn" title={t("Position 9 is {0}; the calculation gives {1}.", c.found, c.expected)}>
          <TriangleAlert size={12} aria-hidden />{" "}{t("Check digit doesn't match")}</Badge>
      );
    case "partial":
      return <Badge tone="info">{t("Partial VIN")}</Badge>;
    case "invalid":
      return (
        <Badge tone="bad">
          <CircleAlert size={12} aria-hidden /> {c.reason}
        </Badge>
      );
  }
}

function VinLookup({ initial }: { initial?: string }) {
  const go = useApp((s) => s.go);
  const { settings } = useSettings();
  const [vin, setVin] = useState(initial ?? "");
  const [scanning, setScanning] = useState(false);
  const [year, setYear] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<unknown>(null);
  const [out, setOut] = useState<DecodeOutcome | null>(null);
  const check = useQuery<VinCheck>({ queryKey: ["vin_check", vin], queryFn: () => call<VinCheck>("vin_check", { vin }), enabled: vin.trim().length > 0 });
  const save = useAction<{ id: string; saved: boolean }, Vehicle>("vehicle_set_saved", { success: (v) => (v.saved ? t("Saved to My Vehicles") : t("Removed from My Vehicles")) });

  const decode = async (v = vin) => {
    setBusy(true);
    setError(null);
    setOut(null);
    try {
      const r = await call<DecodeOutcome>("decode_vin", { vin: v, modelYear: year ? Number(year) : null });
      setOut(r);
      void call("remember_search", { query: v.trim().toUpperCase() }).catch(() => undefined);
    } catch (e) {
      setError(e);
    } finally {
      setBusy(false);
    }
  };
  useEffect(() => {
    if (initial) void decode(initial);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [initial]);

  const c = check.data;
  const canDecode = !!vin.trim() && c?.status !== "invalid" && !busy;
  return (
    <div className="stack">
      <form
        className="card stack"
        onSubmit={(e) => {
          e.preventDefault();
          if (canDecode) void decode();
        }}
      >
        <div className="row" style={{ alignItems: "flex-end" }}>
          <Field label={t("VIN")} className="flex-1" hint={t("17 characters, usually at the base of the windshield or on the driver's door jamb. Use * for unknown characters.")} error={errorField(error) === "vin" ? errorMessage(error) : null}>
            <input
              type="text"
              value={vin}
              autoFocus
              spellCheck={false}
              autoComplete="off"
              maxLength={24}
              className="mono"
              style={{ fontSize: 16, letterSpacing: "0.08em", textTransform: "uppercase" }}
              placeholder={t("1HGCM82633A004352")}
              onChange={(e) => setVin(e.target.value)}
              aria-describedby="vin-status"
            />
          </Field>
          <Field label={t("Model year (optional)")} hint={t("Helps with partial VINs.")}>
            <input type="number" min={1981} max={2100} value={year} onChange={(e) => setYear(e.target.value)} style={{ width: 140 }} />
          </Field>
          <button className="btn primary big" disabled={!canDecode}>
            {busy ? <Spinner /> : <ScanSearch size={18} aria-hidden />}{" "}{t("Decode")}</button>
          <button type="button" className="btn big" onClick={() => setScanning(true)}>
            <Camera size={18} aria-hidden />{" "}{t("Scan…")}</button>
        </div>
        <div id="vin-status" className="row" aria-live="polite">
          {checkBadge(c)}
          {c?.status === "check_digit_mismatch" && <span className="small muted">{t("Common for vehicles built for markets outside North America. You can still decode it; check for a typing mistake first.")}</span>}
          {c?.status === "invalid" && vin.length >= 1 && <span className="small muted">{t("Fix the VIN, or identify the vehicle without one.")}</span>}
        </div>
      </form>
      <ScanDialog open={scanning} want="vin" onClose={() => setScanning(false)} onVin={setVin} />
      {error != null && errorField(error) !== "vin" ? <ErrorNote error={error} /> : null}
      {out && <DecodeResult out={out} system={(settings?.units ?? "imperial") as UnitSystem} onSave={(id) => save.mutate({ id, saved: true }, { onSuccess: () => setOut({ ...out, vehicle: out.vehicle && { ...out.vehicle, saved: true } }) })} onOpen={(id) => go({ page: "vehicle", id })} onManual={() => go({ page: "lookup", tab: "manual" })} />}
    </div>
  );
}

function DecodeResult({ out, system, onSave, onOpen, onManual }: { out: DecodeOutcome; system: UnitSystem; onSave: (id: string) => void; onOpen: (id: string) => void; onManual: () => void }) {
  const fields = useCmd<SpecField[]>("spec_fields");
  const d = out.decoded;
  const byKey = useMemo(() => new Map(d.values.map((v) => [v.key, v])), [d.values]);
  const clean = d.error_codes.length === 0 || (d.error_codes.length === 1 && d.error_codes[0] === "0");
  const essential = (fields.data ?? []).filter((f) => f.essential);
  const missing = essential.filter((f) => !byKey.has(f.key));
  const v = out.vehicle;
  const title = v ? [v.year, v.make, v.model, v.trim].filter(Boolean).join(" ") : [byKey.get("model_year")?.value, byKey.get("make")?.value, byKey.get("model")?.value].filter(Boolean).join(" ");
  return (
    <section className="card stack">
      <div className="spread">
        <div>
          <p className="label">{t("Decoded vehicle")}</p>
          <h2 style={{ fontSize: 20, margin: 0 }}>{d.identified ? title : t("Not identified")}</h2>
          {d.vin && <span className="mono small muted">{d.vin}</span>}
        </div>
        {v && (
          <div className="row">
            {v.saved ? (
              <Badge tone="info">
                <Star size={12} aria-hidden />{" "}{t("In My Vehicles")}</Badge>
            ) : (
              <button className="btn primary" onClick={() => onSave(v.id)}>
                <Star size={16} aria-hidden />{" "}{t("Save vehicle")}</button>
            )}
            <button className="btn" onClick={() => onOpen(v.id)}>{t("Open full record")}</button>
          </div>
        )}
      </div>
      {d.message && !clean && (
        <div className={`notice ${d.identified ? "warn" : "bad"}`}>
          <strong>{t("NHTSA says:")}</strong> {d.message}
          {d.suggested_vin && <div className="small">{t("Suggested VIN:")}{" "}<span className="mono">{d.suggested_vin}</span></div>}
        </div>
      )}
      {!d.identified && (
        <div className="row">
          <span className="muted">{t("NHTSA couldn't identify this vehicle from its VIN. Older and non-US vehicles often aren't in its database.")}</span>
          <button className="btn" onClick={onManual}>{t("Identify it without the VIN")}</button>
        </div>
      )}
      {d.identified && (
        <>
          <dl className="kv">
            {essential
              .filter((f) => byKey.has(f.key))
              .map((f) => (
                <div key={f.key} style={{ display: "contents" }}>
                  <dt>{f.label}</dt>
                  <dd>{showValue(byKey.get(f.key)!.value, f.unit, system)}</dd>
                </div>
              ))}
          </dl>
          {missing.length > 0 && (
            <p className="small muted">
              <strong>{t("Not provided by NHTSA for this VIN:")}</strong> {missing.map((f) => f.label.toLowerCase()).join(", ")}. {v ? t("Add them from your manual on the vehicle's record.") : ""}
            </p>
          )}
          {!v && <p className="small muted">{t("Partial VINs aren't saved. Enter the full 17-character VIN to save the vehicle, or add it by hand.")}</p>}
          <details>
            <summary>{tx("All {0} values NHTSA returned", d.values.length)}</summary>
            <div className="table-wrap mt-2" style={{ maxHeight: 360 }}>
              <table>
                <tbody>
                  {d.values.map((x) => (
                    <tr key={x.key}>
                      <td className="muted">{fields.data?.find((f) => f.key === x.key)?.label ?? humanKey(x.key)}</td>
                      <td>{showValue(x.value, x.unit, system)}</td>
                      <td className="small muted mono">{x.source_ref}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          </details>
        </>
      )}
      <p className="small muted flex items-center gap-1">
        {out.freshness.stale && <CloudOff size={13} aria-hidden />}{t("Source: NHTSA vPIC ·")}{" "}{out.freshness.stale ? t("offline, showing the copy saved {0}", dateTime(out.freshness.fetched_at)) : out.freshness.from_cache ? t("saved {0}", dateTime(out.freshness.fetched_at)) : t("just now")}
      </p>
    </section>
  );
}

const QUICK_SPECS: { key: string; label: string; placeholder: string }[] = [
  { key: "engine_displacement_l", label: t("Engine size (litres)"), placeholder: "2.0" },
  { key: "engine_cylinders", label: t("Cylinders"), placeholder: "4" },
  { key: "fuel_type", label: t("Fuel"), placeholder: t("Gasoline") },
  { key: "transmission_style", label: t("Transmission"), placeholder: t("Manual") },
  { key: "drive_type", label: t("Drivetrain"), placeholder: t("RWD") },
  { key: "body_class", label: t("Body"), placeholder: t("Coupe") },
];

function ManualEntry() {
  const go = useApp((s) => s.go);
  const [form, setForm] = useState({ year: "", make: "", model: "", trim: "", vin: "" });
  const [extra, setExtra] = useState<Record<string, string>>({});
  const [error, setError] = useState<unknown>(null);
  const [busy, setBusy] = useState(false);
  const makes = useQuery<NameList>({ queryKey: ["vpic_makes"], queryFn: () => call<NameList>("vpic_makes"), staleTime: Infinity, retry: false });
  const yearNum = Number(form.year);
  const models = useQuery<NameList>({
    queryKey: ["vpic_models", form.make, yearNum],
    queryFn: () => call<NameList>("vpic_models", { make: form.make, year: yearNum }),
    enabled: !!form.make.trim() && yearNum >= 1981 && yearNum <= 2100,
    retry: false,
  });
  const set = (k: keyof typeof form) => (e: React.ChangeEvent<HTMLInputElement>) => setForm({ ...form, [k]: e.target.value });
  const fieldErr = (f: string) => (errorField(error) === f ? errorMessage(error) : null);
  const submit = async () => {
    setBusy(true);
    setError(null);
    try {
      const v = await call<Vehicle>("vehicle_create_manual", {
        input: {
          year: form.year ? Number(form.year) : null,
          make: form.make,
          model: form.model,
          trim: form.trim || null,
          vin: form.vin || null,
          specs: Object.entries(extra).filter(([, val]) => val.trim()),
        },
      });
      go({ page: "vehicle", id: v.id });
    } catch (e) {
      setError(e);
    } finally {
      setBusy(false);
    }
  };
  return (
    <form
      className="card stack"
      onSubmit={(e) => {
        e.preventDefault();
        void submit();
      }}
    >
      <p className="muted">{t("For vehicles before 1981, vehicles not sold in the US, or when you're offline. What you enter is marked as entered by you; nothing is filled in automatically.")}</p>
      <div className="grid" style={{ gridTemplateColumns: "120px 1fr 1fr 1fr" }}>
        <Field label={t("Year")} error={fieldErr("year")}>
          <input type="number" min={1886} max={2100} value={form.year} onChange={set("year")} />
        </Field>
        <Field label={t("Make")} error={fieldErr("make")} hint={makes.isError ? t("Offline: type the make.") : undefined}>
          <input type="text" list="makes" value={form.make} onChange={set("make")} autoComplete="off" />
        </Field>
        <Field label={t("Model")} error={fieldErr("model")}>
          <input type="text" list="models" value={form.model} onChange={set("model")} autoComplete="off" />
        </Field>
        <Field label={t("Trim (optional)")}>
          <input type="text" value={form.trim} onChange={set("trim")} />
        </Field>
      </div>
      <datalist id="makes">{makes.data?.names.map((m) => <option key={m} value={m} />)}</datalist>
      <datalist id="models">{models.data?.names.map((m) => <option key={m} value={m} />)}</datalist>
      <Field label={t("VIN or serial number (optional)")} error={fieldErr("vin")} hint={t("Older vehicles have shorter serial numbers; they're stored as given.")}>
        <input type="text" className="mono" value={form.vin} onChange={set("vin")} style={{ textTransform: "uppercase" }} />
      </Field>
      <details>
        <summary>{t("More details you know (optional)")}</summary>
        <div className="grid mt-2">
          {QUICK_SPECS.map((s) => (
            <Field key={s.key} label={s.label}>
              <input type="text" placeholder={s.placeholder} value={extra[s.key] ?? ""} onChange={(e) => setExtra({ ...extra, [s.key]: e.target.value })} />
            </Field>
          ))}
        </div>
      </details>
      {error != null && !errorField(error) ? <ErrorNote error={error} /> : null}
      <div className="row">
        <button className="btn primary" disabled={busy}>
          {busy && <Spinner />}{" "}{t("Save vehicle")}</button>
      </div>
    </form>
  );
}

