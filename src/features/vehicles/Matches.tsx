import { useState } from "react";
import { Boxes, ClipboardList, Cog, Info, ListPlus, PackageCheck, ShoppingCart } from "lucide-react";
import { call, errorMessage } from "../../api/ipc";
import { useCmd, useRefresh } from "../../api/queries";
import type { Category, FitStatus, PartMatch, StockOption, Vehicle, VehicleMatches } from "../../api/types";
import { useApp, toast } from "../../store";
import { ActionsMenu, Badge, Empty, FitBadge, Loaded, StockBadge } from "../../ui";
import { money } from "../../lib/format";
import { AddToEstimate, AddToList, ReserveDialog } from "../shared/actions";
import { t, tx } from "../../i18n";

const GROUPS: { status: FitStatus; title: string; help: string }[] = [
  { status: "confirmed", title: t("Confirmed fit"), help: t("A verified fitment record matches this vehicle's configuration.") },
  { status: "conditional", title: t("Conditional fit"), help: t("Fits some configurations; the details below decide it.") },
  { status: "possible", title: t("Possible fit"), help: t("A record matches but nobody has verified it. Check before fitting.") },
];

/** Parts with fitment records for a vehicle, grouped by how sure the match is. `inStockOnly`
 * gives the "Parts in Stock for This Vehicle" view. */
export default function Matches({ vehicle, inStockOnly }: { vehicle: Vehicle; inStockOnly: boolean }) {
  const go = useApp((s) => s.go);
  const [category, setCategory] = useState<string>("");
  const cats = useCmd<Category[]>("categories");
  const q = useCmd<VehicleMatches>("vehicle_matches", { id: vehicle.id, categoryId: category || null, inStockOnly });
  const [reserve, setReserve] = useState<StockOption | null>(null);
  const [estimate, setEstimate] = useState<{ id: string; name: string } | null>(null);
  const [list, setList] = useState<{ id: string; name: string } | null>(null);
  const refresh = useRefresh();
  const top = (cats.data ?? []).filter((c) => !c.parent_id);

  const restock = async (m: PartMatch) => {
    try {
      await call("restock_request", { partId: m.part_id, vehicleId: vehicle.id });
      toast(t("{0} added to the restocking list", m.name), { action: { label: t("Open"), run: () => go({ page: "inventory", tab: "reorder" }) } });
      refresh();
    } catch (e) {
      toast(errorMessage(e), { tone: "bad" });
    }
  };

  return (
    <div className="stack">
      <div className="row">
        <select value={category} onChange={(e) => setCategory(e.target.value)} aria-label={t("Filter by system")}>
          <option value="">{t("All systems")}</option>
          {top.map((c) => (
            <optgroup key={c.id} label={c.name}>
              <option value={c.id}>{tx("All {0}", c.name.toLowerCase())}</option>
              {(cats.data ?? [])
                .filter((x) => x.parent_id === c.id)
                .map((x) => (
                  <option key={x.id} value={x.id}>
                    {x.name}
                  </option>
                ))}
            </optgroup>
          ))}
        </select>
        <span className="small muted">{tx("{0} with fitment records for this vehicle. A part is never shown as fitting just because the make and model match.", inStockOnly ? t("Stocked parts") : t("Catalog parts"))}</span>
      </div>
      <Loaded q={q}>
        {(m) => {
          if (m.matches.length === 0) {
            return (
              <Empty
                icon={<Boxes size={20} />}
                title={inStockOnly ? t("No stocked parts have fitment records for this vehicle") : t("No parts in the catalog have fitment records for this vehicle")}
                action={
                  <button className="btn" onClick={() => go("parts")}>{t("Open the parts catalog")}</button>
                }
              >{t("Add fitment records to parts (or import a fitment file), and matching parts appear here with their stock.")}</Empty>
            );
          }
          const shown = (s: FitStatus) => m.matches.filter((x) => x.fit.status === s);
          const excluded = [...shown("not_compatible"), ...shown("unknown")];
          return (
            <>
              <div className="row">
                {GROUPS.map((g) => (
                  <span key={g.status} className="row small">
                    <FitBadge status={g.status} /> {m.counts[g.status] ?? 0}
                  </span>
                ))}
                {excluded.length > 0 && <span className="small muted">{tx("· {0} not compatible or undetermined", excluded.length)}</span>}
              </div>
              {m.missing.length > 0 && <MissingDetails vehicleId={vehicle.id} missing={m.missing} />}
              {GROUPS.map((g) =>
                shown(g.status).length === 0 ? null : (
                  <section key={g.status} className="stack" style={{ gap: 8 }}>
                    <h2 className="row" style={{ margin: "8px 0 0" }}>
                      <FitBadge status={g.status} /> <span className="small muted" style={{ fontWeight: 400 }}>{g.help}</span>
                    </h2>
                    {shown(g.status).map((x) => (
                      <MatchCard
                        key={x.part_id}
                        m={x}
                        onReserve={setReserve}
                        onEstimate={(o) => setEstimate({ id: o.item_id, name: `${x.name} (${o.sku})` })}
                        onList={() => setList({ id: x.part_id, name: x.name })}
                        onRestock={() => restock(x)}
                        onOpen={() => go({ page: "part", id: x.part_id })}
                        onItem={(o) => go({ page: "item", id: o.item_id })}
                      />
                    ))}
                  </section>
                ),
              )}
              {excluded.length > 0 && (
                <details>
                  <summary>{tx("Not compatible or undetermined ({0})", excluded.length)}</summary>
                  <div className="stack mt-2" style={{ gap: 8 }}>
                    {excluded.map((x) => (
                      <MatchCard key={x.part_id} m={x} onOpen={() => go({ page: "part", id: x.part_id })} onItem={(o) => go({ page: "item", id: o.item_id })} />
                    ))}
                  </div>
                </details>
              )}
            </>
          );
        }}
      </Loaded>
      <ReserveDialog item={reserve ? { id: reserve.item_id, sku: reserve.sku, available: reserve.available } : null} vehicleId={vehicle.id} onClose={() => setReserve(null)} />
      <AddToEstimate item={estimate} vehicleId={vehicle.id} onClose={() => setEstimate(null)} />
      <AddToList part={list} vehicleId={vehicle.id} onClose={() => setList(null)} />
    </div>
  );
}

function MatchCard({
  m,
  onReserve,
  onEstimate,
  onList,
  onRestock,
  onOpen,
  onItem,
}: {
  m: PartMatch;
  onReserve?: (o: StockOption) => void;
  onEstimate?: (o: StockOption) => void;
  onList?: () => void;
  onRestock?: () => void;
  onOpen: () => void;
  onItem: (o: StockOption) => void;
}) {
  const fits = m.fit.status !== "not_compatible" && m.fit.status !== "unknown";
  const inStock = m.items.some((i) => i.available > 0);
  return (
    <div className="card" style={{ padding: 12 }}>
      <div className="spread" style={{ alignItems: "flex-start" }}>
        <div className="min-w-0">
          <button className="btn link" style={{ padding: 0, fontWeight: 600, fontSize: 14 }} onClick={onOpen}>
            {m.name}
          </button>
          <div className="small muted">
            {[m.brand, m.numbers, m.category].filter(Boolean).join(" · ")}
            {m.fit.position && ` · ${m.fit.position}`}
            {m.fit.quantity != null && t(" · {0} per vehicle", m.fit.quantity)}
          </div>
        </div>
        <div className="row">
          <FitBadge status={m.fit.status} />
          {fits && onList && (
            <ActionsMenu
              items={[
                { label: t("Open part"), icon: <Cog size={14} />, run: onOpen },
                { label: t("Save to a parts list"), icon: <ListPlus size={14} />, run: onList },
                !inStock && onRestock ? { label: t("Request restocking"), icon: <ShoppingCart size={14} />, run: onRestock } : null,
              ].filter((x) => x !== null)}
            />
          )}
        </div>
      </div>
      {(m.fit.requirements.length > 0 || m.fit.status !== "confirmed") && (
        <div className="small mt-1" style={{ color: "var(--text-2)" }}>
          <Info size={12} aria-hidden /> {m.fit.reasons.join(" ")}
          {m.fit.requirements.length > 0 && <>{" "}{tx("Needs: {0}.", m.fit.requirements.map((r) => `${r.label} = ${r.value}`).join("; "))}</>}
        </div>
      )}
      {m.items.length > 0 ? (
        <table className="mt-2">
          <tbody>
            {m.items.map((o) => (
              <tr key={o.item_id}>
                <td style={{ width: "28%" }}>
                  <button className="btn link mono" style={{ padding: 0 }} onClick={() => onItem(o)}>
                    {o.sku}
                  </button>
                  {o.condition !== "new" && <Badge>{o.condition}</Badge>}
                </td>
                <td>
                  <StockBadge available={o.available} />
                  {o.reserved > 0 && <span className="small muted">{" "}{tx("· {0} reserved", o.reserved)}</span>}
                </td>
                <td className="small muted">{o.bins || "—"}</td>
                <td className="num">{o.price ? money(o.price) : <span className="small muted" title={t("No pricing rule applies and no fixed price is set")}>{t("no price")}</span>}</td>
                <td className="num" style={{ whiteSpace: "nowrap" }}>
                  {fits && onReserve && (
                    <button className="btn" disabled={o.available <= 0} onClick={() => onReserve(o)} title={t("Hold stock for this vehicle")}>
                      <PackageCheck size={14} aria-hidden />{" "}{t("Reserve")}</button>
                  )}{" "}
                  {fits && onEstimate && (
                    <button className="btn" onClick={() => onEstimate(o)} disabled={!o.price}>
                      <ClipboardList size={14} aria-hidden />{" "}{t("Add to estimate")}</button>
                  )}
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      ) : (
        <p className="small muted mt-1">{t("Not stocked.")}{" "}{onRestock && fits && <button className="btn link" style={{ padding: 0 }} onClick={onRestock}>{t("Request restocking")}</button>}</p>
      )}
    </div>
  );
}

function MissingDetails({ vehicleId, missing }: { vehicleId: string; missing: VehicleMatches["missing"] }) {
  const refresh = useRefresh();
  const [values, setValues] = useState<Record<string, string>>({});
  const save = async (key: string) => {
    const value = values[key]?.trim();
    if (!value) return;
    try {
      await call("vehicle_set_spec", { id: vehicleId, spec: { key, value, unit: null, source_ref: null } });
      toast(t("Saved; matches updated"));
      refresh();
    } catch (e) {
      toast(errorMessage(e), { tone: "bad" });
    }
  };
  return (
    <div className="notice warn stack" style={{ gap: 8 }}>
      <strong>{t("Some matches depend on details this vehicle's record doesn't have yet.")}</strong>
      {missing.map(([r, n]) => (
        <form
          key={r.key}
          className="row"
          onSubmit={(e) => {
            e.preventDefault();
            void save(r.key);
          }}
        >
          <span style={{ minWidth: 200 }}>
            {r.label} <span className="small muted">{tx("({0} part{1})", n, n === 1 ? "" : "s")}</span>
          </span>
          <input type="text" placeholder={t("Value from the vehicle or its paperwork")} value={values[r.key] ?? ""} onChange={(e) => setValues({ ...values, [r.key]: e.target.value })} aria-label={r.label} />
          <button className="btn" disabled={!values[r.key]?.trim()}>{t("Save")}</button>
        </form>
      ))}
    </div>
  );
}
