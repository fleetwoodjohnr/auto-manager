import { Attachments } from "../shared/Attachments";
import { useState } from "react";
import { Archive, ArchiveRestore, ArrowDownToLine, ArrowLeftRight, ArrowUpFromLine, ClipboardList, Cog, PackageCheck, Pencil, Printer, RotateCcw, SlidersHorizontal, Trash2, TriangleAlert, Undo2 } from "lucide-react";
import { call, errorMessage } from "../../api/ipc";
import { useAction, useCmd } from "../../api/queries";
import type { ItemDetail } from "../../api/types";
import { useApp, toast } from "../../store";
import { ActionsMenu, Badge, Loaded, StockBadge, useConfirm } from "../../ui";
import { dateTime, money, pct, qty } from "../../lib/format";
import { EditItem } from "./ItemForm";
import { StockDialog, type StockOp } from "./StockDialog";
import { AddToEstimate, ReserveDialog } from "../shared/actions";
import { movementLabel } from "../dashboard/Dashboard";
import { Labels } from "./Labels";
import { SupplierPricing } from "./Purchasing";
import { t, tx } from "../../i18n";

export default function ItemPage() {
  const { route, go } = useApp();
  const id = route.id!;
  const q = useCmd<ItemDetail>("item_get", { id });
  const [op, setOp] = useState<StockOp | null>(null);
  const [editing, setEditing] = useState(false);
  const [reserving, setReserving] = useState(false);
  const [estimating, setEstimating] = useState(false);
  const [labels, setLabels] = useState(false);
  const archive = useAction<{ id: string; archived: boolean }>("item_archive", { success: t("Saved") });
  const del = useAction<{ id: string }>("item_delete", { success: t("Item deleted") });
  const release = useAction<{ id: string }>("stock_release", { success: t("Reservation released") });
  const { ask, dialog } = useConfirm();
  const undo = async (mid: string) => {
    try {
      await call("movement_reverse", { id: mid });
      toast(t("Undone"));
      void q.refetch();
    } catch (e) {
      toast(errorMessage(e), { tone: "bad" });
    }
  };
  return (
    <div className="page wide">
      {dialog}
      <Loaded q={q}>
        {(d) => {
          const i = d.item;
          return (
            <>
              <div className="page-head">
                <div className="min-w-0">
                  <button className="btn link mb-1" style={{ paddingLeft: 0 }} onClick={() => go("inventory")}>{t("← Inventory")}</button>
                  <h1>{i.name}</h1>
                  <p className="row">
                    <span className="mono">{i.sku}</span>
                    {i.brand && <strong>{i.brand}</strong>}
                    {i.numbers && <span className="mono small">{i.numbers}</span>}
                    {i.condition !== "new" && <Badge>{i.condition}</Badge>}
                    {i.archived && <Badge tone="warn">{t("Archived")}</Badge>}
                  </p>
                </div>
                <div className="row">
                  <button className="btn primary" onClick={() => setOp("receive")}>
                    <ArrowDownToLine size={16} aria-hidden />{" "}{t("Receive")}</button>
                  <button className="btn" onClick={() => setOp("issue")} disabled={i.available <= 0}>
                    <ArrowUpFromLine size={16} aria-hidden />{" "}{t("Issue")}</button>
                  <button className="btn" onClick={() => setReserving(true)} disabled={i.available <= 0}>
                    <PackageCheck size={16} aria-hidden />{" "}{t("Reserve")}</button>
                  <button className="btn" onClick={() => setEstimating(true)}>
                    <ClipboardList size={16} aria-hidden />{" "}{t("Add to estimate")}</button>
                  <ActionsMenu
                    items={[
                      { label: t("Move between locations"), icon: <ArrowLeftRight size={14} />, run: () => setOp("transfer"), disabled: i.on_hand <= 0 },
                      { label: t("Adjust quantity"), icon: <SlidersHorizontal size={14} />, run: () => setOp("adjust") },
                      { label: t("Customer return"), icon: <RotateCcw size={14} />, run: () => setOp("return_in") },
                      { label: t("Return to supplier"), icon: <Undo2 size={14} />, run: () => setOp("return_out"), disabled: i.available <= 0 },
                      { label: t("Mark damaged"), icon: <TriangleAlert size={14} />, run: () => setOp("damage"), disabled: i.available <= 0 },
                      null,
                      { label: t("Edit item"), icon: <Pencil size={14} />, run: () => setEditing(true) },
                      { label: t("Open catalog part"), icon: <Cog size={14} />, run: () => go({ page: "part", id: i.part_id }) },
                      { label: t("Print label"), icon: <Printer size={14} />, run: () => setLabels(true) },
                      i.archived
                        ? { label: t("Unarchive"), icon: <ArchiveRestore size={14} />, run: () => archive.mutate({ id, archived: false }) }
                        : { label: t("Archive"), icon: <Archive size={14} />, run: () => archive.mutate({ id, archived: true }) },
                      {
                        label: t("Delete"),
                        icon: <Trash2 size={14} />,
                        danger: true,
                        run: async () => {
                          if (await ask(t("Delete this item?"), t("Only items that never had stock movements can be deleted; others can be archived."))) del.mutate({ id }, { onSuccess: () => go("inventory") });
                        },
                      },
                    ]}
                  />
                </div>
              </div>
              <div className="grid two" style={{ alignItems: "start" }}>
                <div className="stack">
                  <section className="card">
                    <h2>{t("Stock")}</h2>
                    <div className="row mb-2">
                      <StockBadge available={i.available} min={i.min_stock} />
                      <span className="muted">{tx("{0} on hand · {1} reserved{2} · minimum {3}{4}", qty(i.on_hand, i.unit), qty(i.reserved), i.damaged ? t(" · {0} damaged", qty(i.damaged)) : "", qty(i.min_stock), i.reorder_qty ? t(" · reorder {0}", i.reorder_qty) : "")}</span>
                    </div>
                    {d.levels.length === 0 ? (
                      <p className="muted">{t("No stock recorded yet. Use Receive to add some.")}</p>
                    ) : (
                      <table>
                        <thead>
                          <tr>
                            <th>{t("Location")}</th>
                            <th>{t("Bin")}</th>
                            <th className="num">{t("On hand")}</th>
                            <th className="num">{t("Reserved")}</th>
                            <th className="num">{t("Available")}</th>
                          </tr>
                        </thead>
                        <tbody>
                          {d.levels.map((l) => (
                            <tr key={l.location_id}>
                              <td>
                                {l.location} {l.kind === "damaged" && <Badge tone="warn">quarantine</Badge>}
                              </td>
                              <td>{l.bin || "—"}</td>
                              <td className="num">{qty(l.on_hand)}</td>
                              <td className="num">{l.reserved ? qty(l.reserved) : ""}</td>
                              <td className="num">{l.kind === "damaged" ? "—" : qty(l.available)}</td>
                            </tr>
                          ))}
                        </tbody>
                      </table>
                    )}
                  </section>
                  {d.reservations.length > 0 && (
                    <section className="card">
                      <h2>{t("Reserved")}</h2>
                      <ul className="items" style={{ maxHeight: "none" }}>
                        {d.reservations.map((r) => (
                          <li key={r.id}>
                            <span>{tx("{0} at {1}", r.qty, r.location)}</span>
                            {r.job_id ? (
                              <button className="btn link" style={{ padding: 0 }} onClick={() => go({ page: "job", id: r.job_id! })}>{tx("Job #{0}", r.job_number)}</button>
                            ) : r.vehicle_id ? (
                              <button className="btn link" style={{ padding: 0 }} onClick={() => go({ page: "vehicle", id: r.vehicle_id! })}>{t("for a vehicle")}</button>
                            ) : null}
                            <span className="small muted">{r.note}</span>
                            <button className="btn link ml-auto" onClick={() => release.mutate({ id: r.id })}>{t("Release")}</button>
                          </li>
                        ))}
                      </ul>
                    </section>
                  )}
                </div>
                <div className="stack">
                  <section className="card">
                    <h2>{t("Cost and price")}</h2>
                    <dl className="kv">
                      <dt>{t("Last purchase cost")}</dt>
                      <dd>{money(i.cost, 4).replace(/(\.\d\d)00$/, "$1")}</dd>
                      <dt>{t("Average cost")}</dt>
                      <dd>{money(i.avg_cost, 4).replace(/(\.\d\d)00$/, "$1")}</dd>
                      <dt>{t("Selling price")}</dt>
                      <dd>{d.price.price ? <strong>{money(d.price.price)}</strong> : <span style={{ color: "var(--warn)" }}>{t("Not set")}</span>}</dd>
                      {d.price.profit && (
                        <>
                          <dt>{t("Profit")}</dt>
                          <dd>{tx("{0} · {1} margin · {2} markup", money(d.price.profit), pct(d.price.margin_pct), pct(d.price.markup_pct))}</dd>
                        </>
                      )}
                      {i.supplier_name && (
                        <>
                          <dt>{t("Supplier")}</dt>
                          <dd>{i.supplier_name}</dd>
                        </>
                      )}
                      {i.warranty && (
                        <>
                          <dt>{t("Warranty")}</dt>
                          <dd>{i.warranty}</dd>
                        </>
                      )}
                    </dl>
                    <details className="mt-2" open={!d.price.price}>
                      <summary className="small">{tx("How this price was calculated ({0})", d.price.profile_name)}</summary>
                      <ol className="small mt-1" style={{ paddingLeft: 18 }}>
                        {d.price.steps.map((s, k) => (
                          <li key={k}>{s}</li>
                        ))}
                      </ol>
                    </details>
                  </section>
                  <SupplierPricing itemId={i.id} />
                  <Attachments ownerKind="item" ownerId={i.id} />
                  <section className="card">
                    <h2>{t("History")}</h2>
                    {d.movements.length === 0 ? (
                      <p className="muted">{t("No movements yet.")}</p>
                    ) : (
                      <div className="table-wrap" style={{ maxHeight: 360, boxShadow: "none" }}>
                        <table>
                          <tbody>
                            {d.movements.map((m) => (
                              <tr key={m.id}>
                                <td className="small">{dateTime(m.created_at)}</td>
                                <td>
                                  {movementLabel(m.kind)}
                                  <div className="small muted">{[m.location, m.reason].filter(Boolean).join(" · ")}</div>
                                </td>
                                <td className="num" style={{ color: m.qty < 0 ? "var(--danger)" : "var(--success)" }}>
                                  {m.qty > 0 ? `+${m.qty}` : m.qty}
                                </td>
                                <td>
                                  {m.reversed ? (
                                    <Badge>undone</Badge>
                                  ) : m.kind !== "consume" && m.kind !== "reverse" ? (
                                    <button className="btn link small" onClick={() => undo(m.id)}>{t("Undo")}</button>
                                  ) : null}
                                </td>
                              </tr>
                            ))}
                          </tbody>
                        </table>
                      </div>
                    )}
                  </section>
                </div>
              </div>
              <StockDialog op={op} item={i} levels={d.levels} onClose={() => setOp(null)} />
              {editing && <EditItem item={i} onClose={() => setEditing(false)} />}
              <ReserveDialog item={reserving ? { id: i.id, sku: i.sku, available: i.available } : null} onClose={() => setReserving(false)} />
              <AddToEstimate item={estimating ? { id: i.id, name: `${i.name} (${i.sku})` } : null} onClose={() => setEstimating(false)} />
              {labels && <Labels items={[i]} onClose={() => setLabels(false)} />}
            </>
          );
        }}
      </Loaded>
    </div>
  );
}
