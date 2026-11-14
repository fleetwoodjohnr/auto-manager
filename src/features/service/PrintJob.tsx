import { Printer } from "lucide-react";
import { useCmd } from "../../api/queries";
import type { Customer, JobDetail } from "../../api/types";
import { useApp } from "../../store";
import { Loaded } from "../../ui";
import { date, money, num } from "../../lib/format";

/** A printable estimate or invoice. "Save as PDF" / "Print to File" in the print dialog makes the PDF. */
export default function PrintJob() {
  const { route, back } = useApp();
  const q = useCmd<JobDetail>("job_get", { id: route.id });
  const customer = useCmd<Customer>("customer_get", { id: q.data?.job.customer_id }, !!q.data?.job.customer_id);
  return (
    <div className="page">
      <div className="row no-print mb-3">
        <button className="btn" onClick={back}>
          ← Back
        </button>
        <button className="btn primary" onClick={() => window.print()}>
          <Printer size={16} aria-hidden /> Print or save as PDF
        </button>
        <span className="small muted">To get a PDF, choose “Save as PDF” or “Print to File” as the printer.</span>
      </div>
      <Loaded q={q}>
        {(d) => {
          const j = d.job;
          const kind = j.invoice_number ? `Invoice ${j.invoice_number}` : j.status === "estimate" || j.status === "approved" ? `Estimate #${j.number}` : `Work order #${j.number}`;
          const c = customer.data;
          return (
            <article className="print-sheet stack">
              <div className="spread" style={{ alignItems: "flex-start" }}>
                <div>
                  <h1 style={{ color: "#111" }}>{kind}</h1>
                  <p style={{ color: "#444" }}>{date(j.invoiced_at ?? j.completed_at ?? j.created_at)}</p>
                </div>
                <div style={{ textAlign: "right", color: "#333" }}>
                  {c ? (
                    <>
                      <strong>{c.name}</strong>
                      {c.address && <div style={{ whiteSpace: "pre-wrap" }}>{c.address}</div>}
                      {c.phone && <div>{c.phone}</div>}
                      {c.email && <div>{c.email}</div>}
                    </>
                  ) : (
                    j.customer_name
                  )}
                </div>
              </div>
              {(j.vehicle_title || j.mileage != null) && (
                <p style={{ color: "#333" }}>
                  <strong>Vehicle:</strong> {j.vehicle_title} {j.mileage != null && `· ${j.mileage.toLocaleString()} miles`}
                </p>
              )}
              {j.complaint && (
                <p style={{ color: "#333", whiteSpace: "pre-wrap" }}>
                  <strong>Request:</strong> {j.complaint}
                </p>
              )}
              <table>
                <thead>
                  <tr>
                    <th>Description</th>
                    <th className="num">Qty</th>
                    <th className="num">Price</th>
                    <th className="num">Amount</th>
                  </tr>
                </thead>
                <tbody>
                  {d.lines.map((l) => (
                    <tr key={l.id}>
                      <td>
                        {l.description}
                        {!l.taxable && <span style={{ color: "#666" }}> (not taxed)</span>}
                      </td>
                      <td className="num">{num(l.qty)}</td>
                      <td className="num">{money(l.unit_price)}</td>
                      <td className="num">{money(l.amount)}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
              <dl className="kv" style={{ alignSelf: "flex-end", minWidth: 260, color: "#111" }}>
                <dt>Parts</dt>
                <dd className="num">{money(d.totals.parts)}</dd>
                <dt>Labor</dt>
                <dd className="num">{money(d.totals.labor)}</dd>
                {Number(d.totals.fees) !== 0 && (
                  <>
                    <dt>Fees</dt>
                    <dd className="num">{money(d.totals.fees)}</dd>
                  </>
                )}
                {Number(d.totals.discount) !== 0 && (
                  <>
                    <dt>Discount</dt>
                    <dd className="num">−{money(d.totals.discount)}</dd>
                  </>
                )}
                <dt>Tax ({num(j.tax_rate_pct, 3)}%{j.prices_include_tax ? " included" : ""})</dt>
                <dd className="num">{money(d.totals.tax)}</dd>
                <dt>
                  <strong>Total</strong>
                </dt>
                <dd className="num">
                  <strong>{money(d.totals.total)}</strong>
                </dd>
              </dl>
              {(j.status === "estimate" || j.status === "approved") && <p style={{ color: "#555", fontSize: 12 }}>This is an estimate. Prices may change if more work or parts are needed; we'll ask before going ahead.</p>}
            </article>
          );
        }}
      </Loaded>
    </div>
  );
}
