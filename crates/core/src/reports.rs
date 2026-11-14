//! Reports, all computed from stored records for a date range. Each report is a table the UI
//! shows, prints (to PDF through the print dialog) and exports to CSV or XLSX.

use crate::error::{AppError, AppResult};
use crate::money;
use crate::{inventory, jobs, maintenance, pricing, settings};
use rusqlite::{Connection, params};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use ts_rs::TS;

#[derive(Serialize, TS, Clone, Debug)]
#[ts(export)]
pub struct Report {
    pub id: String,
    pub title: String,
    pub note: String,
    pub headers: Vec<String>,
    /// Which columns are amounts or counts (right-aligned, summed in exports).
    pub numeric: Vec<bool>,
    pub rows: Vec<Vec<String>>,
    pub summary: Vec<(String, String)>,
}

#[derive(Deserialize, TS, Clone, Debug)]
#[ts(export)]
pub struct ReportQuery {
    pub id: String,
    /// YYYY-MM-DD, inclusive
    pub from: String,
    pub to: String,
    pub vehicle_id: Option<String>,
}

#[derive(Serialize, TS, Clone, Debug)]
#[ts(export)]
pub struct ReportInfo {
    pub id: String,
    pub group: String,
    pub title: String,
    pub needs_vehicle: bool,
    pub uses_dates: bool,
}

pub const REPORTS: &[(&str, &str, &str, bool, bool)] = &[
    ("inventory_value", "Inventory", "Inventory value", false, false),
    ("stock_status", "Inventory", "Low and out-of-stock items", false, false),
    ("movement", "Inventory", "Inventory movement", false, true),
    ("frequent", "Inventory", "Frequently used parts", false, true),
    ("slow", "Inventory", "Slow-moving inventory", false, true),
    ("turnover", "Inventory", "Inventory turnover", false, true),
    ("by_supplier", "Inventory", "Parts by supplier", false, false),
    ("sales", "Business", "Revenue, cost and gross profit", false, true),
    ("jobs", "Business", "Completed jobs", false, true),
    ("quotes", "Business", "Estimate conversion", false, true),
    ("supplier_spend", "Business", "Supplier spending", false, true),
    ("vehicle_history", "Vehicle", "Maintenance history", true, false),
    ("vehicle_cost", "Vehicle", "Cost of ownership", true, false),
    ("vehicle_parts", "Vehicle", "Parts replaced", true, false),
    ("vehicle_due", "Vehicle", "Upcoming maintenance", true, false),
];

pub fn list() -> Vec<ReportInfo> {
    REPORTS
        .iter()
        .map(|(id, g, t, v, d)| ReportInfo { id: (*id).into(), group: (*g).into(), title: (*t).into(), needs_vehicle: *v, uses_dates: *d })
        .collect()
}

fn m(d: Decimal) -> String {
    money::round_money(d).to_string()
}

fn dec(s: &str) -> Decimal {
    s.parse().unwrap_or_default()
}

fn report(
    id: &str,
    note: impl Into<String>,
    headers: &[&str],
    numeric: &[bool],
    rows: Vec<Vec<String>>,
    summary: Vec<(String, String)>,
) -> Report {
    let title = REPORTS.iter().find(|r| r.0 == id).map(|r| r.2).unwrap_or(id).to_string();
    Report {
        id: id.into(),
        title,
        note: note.into(),
        headers: headers.iter().map(|h| h.to_string()).collect(),
        numeric: numeric.to_vec(),
        rows,
        summary,
    }
}

pub fn run(conn: &Connection, q: &ReportQuery) -> AppResult<Report> {
    let (from, to) = (q.from.as_str(), format!("{}T23:59:59.999Z", q.to));
    for d in [&q.from, &q.to] {
        if chrono::NaiveDate::parse_from_str(d, "%Y-%m-%d").is_err() {
            return Err(AppError::invalid("from", "Choose the dates as YYYY-MM-DD."));
        }
    }
    let s = settings::load(conn);
    let period = format!("{} to {}", q.from, q.to);
    let vehicle = || q.vehicle_id.clone().ok_or_else(|| AppError::invalid("vehicle_id", "Choose a vehicle."));
    Ok(match q.id.as_str() {
        "inventory_value" => {
            let profile = pricing::default_profile(conn)?;
            let (mut at_cost, mut at_retail, mut unpriced) = (Decimal::ZERO, Decimal::ZERO, 0);
            let mut rows = Vec::new();
            for i in inventory::list(conn, &inventory::ItemQuery::default())? {
                let cost = if s.valuation_method == "standard" || i.avg_cost.is_zero() { i.cost } else { i.avg_cost };
                let value = cost * Decimal::from(i.on_hand);
                let price = pricing::price_item(conn, &i.id, 1, &[Some(profile.id.as_str())], &s.valuation_method)?.price;
                let retail = price.map(|p| p * Decimal::from(i.on_hand));
                at_cost += value;
                match retail {
                    Some(r) => at_retail += r,
                    None if i.on_hand > 0 => unpriced += 1,
                    None => {}
                }
                rows.push(vec![
                    i.sku,
                    i.name,
                    i.category,
                    i.on_hand.to_string(),
                    m(cost),
                    m(value),
                    price.map(m).unwrap_or_default(),
                    retail.map(m).unwrap_or_default(),
                ]);
            }
            report(
                "inventory_value",
                format!(
                    "Valued at {} cost; retail at “{}” prices. {unpriced} stocked item(s) have no price and aren't in the retail total.",
                    if s.valuation_method == "standard" { "last purchase" } else { "average" },
                    profile.name
                ),
                &["SKU", "Product", "Category", "On hand", "Unit cost", "Value at cost", "Unit price", "Retail value"],
                &[false, false, false, true, true, true, true, true],
                rows,
                vec![
                    ("Value at cost".into(), m(at_cost)),
                    ("Potential retail value".into(), m(at_retail)),
                    ("Potential gross profit".into(), m(at_retail - at_cost)),
                ],
            )
        }
        "stock_status" => {
            let rows: Vec<Vec<String>> = inventory::list(conn, &inventory::ItemQuery { stock: Some("low".into()), ..Default::default() })?
                .into_iter()
                .chain(
                    inventory::list(conn, &inventory::ItemQuery { stock: Some("out".into()), ..Default::default() })?
                        .into_iter()
                        .filter(|i| i.min_stock == 0),
                )
                .map(|i| {
                    vec![
                        i.sku,
                        i.name,
                        i.available.to_string(),
                        i.reserved.to_string(),
                        i.min_stock.to_string(),
                        i.reorder_qty.to_string(),
                        i.supplier_name.unwrap_or_default(),
                    ]
                })
                .collect();
            let n = rows.len();
            report(
                "stock_status",
                "Items at or below minimum stock, and items with none available.",
                &["SKU", "Product", "Available", "Reserved", "Minimum", "Reorder qty", "Supplier"],
                &[false, false, true, true, true, true, false],
                rows,
                vec![("Items".into(), n.to_string())],
            )
        }
        "movement" => {
            let mut st = conn.prepare(
                "SELECT m.kind, count(*), sum(m.qty), sum(abs(m.qty) * CAST(coalesce(m.unit_cost, '0') AS REAL)) FROM inventory_movements m
                 WHERE m.created_at BETWEEN ?1 AND ?2 GROUP BY m.kind ORDER BY m.kind",
            )?;
            let rows: Vec<Vec<String>> = st
                .query_map(params![from, to], |r| {
                    Ok(vec![
                        crate_label(&r.get::<_, String>(0)?),
                        r.get::<_, i64>(1)?.to_string(),
                        r.get::<_, i64>(2)?.to_string(),
                        format!("{:.2}", r.get::<_, f64>(3)?),
                    ])
                })?
                .collect::<Result<_, _>>()?;
            report(
                "movement",
                format!("Stock movements {period}, by type. Value uses the cost recorded on each movement."),
                &["Movement", "Count", "Net units", "Value at cost"],
                &[false, true, true, true],
                rows,
                vec![],
            )
        }
        "frequent" => {
            let mut st = conn.prepare(
                "SELECT i.sku, p.name, -sum(m.qty) AS used, count(DISTINCT substr(m.created_at, 1, 10)) FROM inventory_movements m JOIN inventory_items i ON i.id = m.item_id JOIN parts p ON p.id = i.part_id
                 WHERE m.kind IN ('issue', 'consume') AND m.created_at BETWEEN ?1 AND ?2 GROUP BY m.item_id ORDER BY used DESC LIMIT 100",
            )?;
            let rows = st
                .query_map(params![from, to], |r| {
                    Ok(vec![r.get(0)?, r.get(1)?, r.get::<_, i64>(2)?.to_string(), r.get::<_, i64>(3)?.to_string()])
                })?
                .collect::<Result<_, _>>()?;
            report(
                "frequent",
                format!("Units issued or used on jobs {period}."),
                &["SKU", "Product", "Units used", "Days used"],
                &[false, false, true, true],
                rows,
                vec![],
            )
        }
        "slow" => {
            let mut st = conn.prepare(
                "SELECT i.sku, p.name, sum(s.on_hand), (SELECT max(created_at) FROM inventory_movements x WHERE x.item_id = i.id AND x.kind IN ('issue', 'consume')), i.avg_cost
                 FROM inventory_items i JOIN parts p ON p.id = i.part_id JOIN stock_levels s ON s.item_id = i.id
                 WHERE i.archived = 0 AND NOT EXISTS (SELECT 1 FROM inventory_movements x WHERE x.item_id = i.id AND x.kind IN ('issue', 'consume') AND x.created_at BETWEEN ?1 AND ?2)
                 GROUP BY i.id HAVING sum(s.on_hand) > 0 ORDER BY sum(s.on_hand) * CAST(i.avg_cost AS REAL) DESC",
            )?;
            let rows: Vec<Vec<String>> = st
                .query_map(params![from, to], |r| {
                    let on: i64 = r.get(2)?;
                    let cost: String = r.get(4)?;
                    Ok(vec![
                        r.get(0)?,
                        r.get(1)?,
                        on.to_string(),
                        r.get::<_, Option<String>>(3)?.map(|d| d[..10].to_string()).unwrap_or_else(|| "never".into()),
                        m(dec(&cost) * Decimal::from(on)),
                    ])
                })?
                .collect::<Result<_, _>>()?;
            let tied: Decimal = rows.iter().map(|r| dec(&r[4])).sum();
            report(
                "slow",
                format!("Stocked items with no sales or job use {period}."),
                &["SKU", "Product", "On hand", "Last used", "Value at cost"],
                &[false, false, true, false, true],
                rows,
                vec![("Value tied up".into(), m(tied))],
            )
        }
        "turnover" => {
            let cogs: f64 = conn.query_row(
                "SELECT coalesce(sum(-qty * CAST(coalesce(unit_cost, '0') AS REAL)), 0) FROM inventory_movements WHERE kind IN ('issue', 'consume') AND created_at BETWEEN ?1 AND ?2",
                params![from, to],
                |r| r.get(0),
            )?;
            let value = inventory::valuation(conn, &s.valuation_method)?.at_cost;
            let cogs = Decimal::try_from(cogs).unwrap_or_default();
            let turns = if value.is_zero() { None } else { Some((cogs / value).round_dp(2)) };
            report(
                "turnover",
                format!(
                    "Cost of parts issued or used {period}, divided by today's inventory value at cost (an approximation: Auto Manager doesn't keep daily value snapshots)."
                ),
                &["Measure", "Value"],
                &[false, true],
                vec![
                    vec!["Cost of parts used".into(), m(cogs)],
                    vec!["Inventory value now".into(), m(value)],
                    vec!["Turns in the period".into(), turns.map(|t| t.to_string()).unwrap_or_else(|| "—".into())],
                ],
                vec![],
            )
        }
        "by_supplier" => {
            let mut st = conn.prepare(
                "SELECT coalesce(s.name, '(no supplier)'), count(i.id), coalesce(sum((SELECT sum(on_hand) FROM stock_levels x WHERE x.item_id = i.id)), 0),
                        coalesce(sum((SELECT sum(on_hand) FROM stock_levels x WHERE x.item_id = i.id) * CAST(i.avg_cost AS REAL)), 0)
                 FROM inventory_items i LEFT JOIN suppliers s ON s.id = i.supplier_id WHERE i.archived = 0 GROUP BY s.id ORDER BY 1",
            )?;
            let rows = st
                .query_map([], |r| {
                    Ok(vec![
                        r.get(0)?,
                        r.get::<_, i64>(1)?.to_string(),
                        r.get::<_, i64>(2)?.to_string(),
                        format!("{:.2}", r.get::<_, f64>(3)?),
                    ])
                })?
                .collect::<Result<_, _>>()?;
            report(
                "by_supplier",
                "Stocked items by their main supplier.",
                &["Supplier", "Items", "Units on hand", "Value at average cost"],
                &[false, true, true, true],
                rows,
                vec![],
            )
        }
        "sales" | "jobs" => {
            let done: Vec<String> = conn
                .prepare("SELECT id FROM service_jobs WHERE status IN ('completed', 'invoiced') AND completed_at BETWEEN ?1 AND ?2 ORDER BY completed_at")?
                .query_map(params![from, to], |r| r.get(0))?
                .collect::<Result<_, _>>()?;
            let mut t = jobs::Totals::default();
            let mut rows = Vec::new();
            for id in &done {
                let d = jobs::detail(conn, id)?;
                let tt = &d.totals;
                t.parts += tt.parts;
                t.labor += tt.labor;
                t.fees += tt.fees;
                t.discount += tt.discount;
                t.tax += tt.tax;
                t.total += tt.total;
                t.cost += tt.cost;
                t.profit += tt.profit;
                rows.push(vec![
                    d.job.number.to_string(),
                    d.job.completed_at.clone().unwrap_or_default().get(..10).unwrap_or_default().to_string(),
                    d.job.customer_name.clone().unwrap_or_default(),
                    d.job.vehicle_title.clone().unwrap_or_default(),
                    m(tt.parts),
                    m(tt.labor),
                    m(tt.tax),
                    m(tt.total),
                    m(tt.profit),
                ]);
            }
            let revenue = t.parts + t.labor + t.fees - t.discount;
            let margin =
                if revenue.is_zero() { "—".into() } else { format!("{}%", (t.profit / revenue * Decimal::ONE_HUNDRED).round_dp(1)) };
            let summary = vec![
                ("Jobs completed".into(), done.len().to_string()),
                ("Parts revenue".into(), m(t.parts)),
                ("Labor revenue".into(), m(t.labor)),
                ("Fees".into(), m(t.fees)),
                ("Discounts".into(), m(t.discount)),
                ("Tax collected".into(), m(t.tax)),
                ("Cost of parts".into(), m(t.cost)),
                ("Gross profit".into(), m(t.profit)),
                ("Gross margin".into(), margin),
            ];
            report(
                &q.id,
                format!("Jobs completed {period}. Revenue excludes tax."),
                &["Job", "Completed", "Customer", "Vehicle", "Parts", "Labor", "Tax", "Total", "Gross profit"],
                &[false, false, false, false, true, true, true, true, true],
                rows,
                summary,
            )
        }
        "quotes" => {
            let mut st = conn.prepare("SELECT status, count(*) FROM service_jobs WHERE created_at BETWEEN ?1 AND ?2 GROUP BY status")?;
            let counts: BTreeMap<String, i64> =
                st.query_map(params![from, to], |r| Ok((r.get(0)?, r.get(1)?)))?.collect::<Result<_, _>>()?;
            let total: i64 = counts.values().sum();
            let won: i64 = ["approved", "in_progress", "completed", "invoiced"].iter().map(|k| counts.get(*k).copied().unwrap_or(0)).sum();
            let rows = counts.iter().map(|(k, v)| vec![k.replace('_', " "), v.to_string()]).collect();
            let rate = if total == 0 { "—".into() } else { format!("{:.1}%", won as f64 * 100.0 / total as f64) };
            report(
                "quotes",
                format!("Estimates created {period} and where they are now."),
                &["Status", "Count"],
                &[false, true],
                rows,
                vec![("Estimates".into(), total.to_string()), ("Accepted".into(), won.to_string()), ("Conversion".into(), rate)],
            )
        }
        "supplier_spend" => {
            let mut st = conn.prepare(
                "SELECT coalesce(s.name, '(no supplier recorded)'), count(*), sum(m.qty), sum(m.qty * CAST(coalesce(m.unit_cost, '0') AS REAL))
                 FROM inventory_movements m JOIN inventory_items i ON i.id = m.item_id
                 LEFT JOIN purchase_orders o ON m.ref_kind = 'po' AND o.id = m.ref_id
                 LEFT JOIN suppliers s ON s.id = coalesce(o.supplier_id, i.supplier_id)
                 WHERE m.kind = 'receive' AND m.created_at BETWEEN ?1 AND ?2 AND m.id NOT IN (SELECT reverses_id FROM inventory_movements WHERE reverses_id IS NOT NULL)
                 GROUP BY s.id ORDER BY 4 DESC",
            )?;
            let rows: Vec<Vec<String>> = st
                .query_map(params![from, to], |r| {
                    Ok(vec![
                        r.get(0)?,
                        r.get::<_, i64>(1)?.to_string(),
                        r.get::<_, i64>(2)?.to_string(),
                        format!("{:.2}", r.get::<_, f64>(3)?),
                    ])
                })?
                .collect::<Result<_, _>>()?;
            let total: Decimal = rows.iter().map(|r| dec(&r[3])).sum();
            report(
                "supplier_spend",
                format!("Stock received {period}, at the cost recorded on receipt."),
                &["Supplier", "Receipts", "Units", "Purchase cost"],
                &[false, true, true, true],
                rows,
                vec![("Total purchase cost".into(), m(total))],
            )
        }
        "vehicle_history" => {
            let vid = vehicle()?;
            let rows = maintenance::records(conn, &vid)?
                .into_iter()
                .map(|r| {
                    vec![
                        r.date,
                        r.mileage.map(|x| x.to_string()).unwrap_or_default(),
                        r.service,
                        r.parts_used,
                        r.provider,
                        r.cost.unwrap_or_default(),
                    ]
                })
                .collect();
            report(
                "vehicle_history",
                "Every recorded service.",
                &["Date", "Mileage", "Service", "Parts", "By", "Cost"],
                &[false, true, false, false, false, true],
                rows,
                vec![],
            )
        }
        "vehicle_cost" => {
            let vid = vehicle()?;
            let records = maintenance::records(conn, &vid)?;
            let maint: Decimal = records.iter().filter(|r| r.job_id.is_none()).filter_map(|r| r.cost.as_deref()).map(dec).sum();
            let mut jobs_total = Decimal::ZERO;
            for j in jobs::list(conn, &jobs::JobQuery { vehicle_id: Some(vid.clone()), ..Default::default() })?
                .into_iter()
                .filter(|j| matches!(j.status.as_str(), "completed" | "invoiced"))
            {
                jobs_total += jobs::detail(conn, &j.id)?.totals.total;
            }
            let purchase: Option<String> = conn.query_row("SELECT purchase_price FROM vehicles WHERE id = ?1", [&vid], |r| r.get(0))?;
            let p = purchase.as_deref().map(dec).unwrap_or_default();
            report(
                "vehicle_cost",
                "Purchase price, recorded maintenance costs and completed jobs. Fuel, insurance and depreciation aren't tracked.",
                &["Item", "Amount"],
                &[false, true],
                vec![
                    vec!["Purchase price".into(), m(p)],
                    vec!["Maintenance records".into(), m(maint)],
                    vec!["Service jobs".into(), m(jobs_total)],
                ],
                vec![("Total".into(), m(p + maint + jobs_total))],
            )
        }
        "vehicle_parts" => {
            let vid = vehicle()?;
            let mut st = conn.prepare(
                "SELECT substr(coalesce(j.completed_at, j.created_at), 1, 10), j.number, l.description, l.qty FROM service_lines l JOIN service_jobs j ON j.id = l.job_id
                 WHERE j.vehicle_id = ?1 AND l.kind = 'part' AND j.status IN ('completed', 'invoiced') ORDER BY 1 DESC",
            )?;
            let rows = st
                .query_map([&vid], |r| Ok(vec![r.get(0)?, format!("#{}", r.get::<_, i64>(1)?), r.get(2)?, r.get(3)?]))?
                .collect::<Result<_, _>>()?;
            report(
                "vehicle_parts",
                "Parts fitted on completed jobs.",
                &["Date", "Job", "Part", "Qty"],
                &[false, false, false, true],
                rows,
                vec![],
            )
        }
        "vehicle_due" => {
            let vid = vehicle()?;
            let today = chrono::Utc::now().date_naive();
            let rows = maintenance::intervals(conn, &vid, today)?
                .into_iter()
                .map(|i| {
                    vec![
                        i.service,
                        [i.every_miles.map(|x| format!("{x} mi")), i.every_months.map(|x| format!("{x} months"))]
                            .into_iter()
                            .flatten()
                            .collect::<Vec<_>>()
                            .join(" / "),
                        i.last_date.unwrap_or_else(|| "never".into()),
                        i.due_date.unwrap_or_default(),
                        i.due_mileage.map(|x| x.to_string()).unwrap_or_default(),
                        i.state.replace('_', " "),
                    ]
                })
                .collect();
            report(
                "vehicle_due",
                "From the intervals recorded for this vehicle.",
                &["Service", "Every", "Last done", "Due date", "Due mileage", "Status"],
                &[false, false, false, false, true, false],
                rows,
                vec![],
            )
        }
        other => return Err(AppError::msg(format!("Unknown report “{other}”."))),
    })
}

fn crate_label(kind: &str) -> String {
    match kind {
        "receive" => "Received",
        "issue" => "Issued",
        "transfer_out" => "Moved out",
        "transfer_in" => "Moved in",
        "adjust" => "Adjusted",
        "count" => "Counted",
        "damage" => "Damaged",
        "return_in" => "Customer returns",
        "return_out" => "Returned to supplier",
        "consume" => "Used on jobs",
        "reverse" => "Undone",
        k => k,
    }
    .to_string()
}

pub fn export(path: &std::path::Path, r: &Report) -> AppResult<usize> {
    let mut rows = r.rows.clone();
    if !r.summary.is_empty() {
        rows.push(vec![]);
        rows.extend(r.summary.iter().map(|(k, v)| vec![k.clone(), v.clone()]));
    }
    let headers: Vec<&str> = r.headers.iter().map(String::as_str).collect();
    match path.extension().and_then(|e| e.to_str()) {
        Some("xlsx") => crate::csvio::write_xlsx(path, &r.title.chars().take(31).collect::<String>(), &headers, rows),
        _ => crate::csvio::write_csv(path, &headers, rows),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_report_runs_on_an_empty_database() {
        let conn = crate::db::open_memory().unwrap();
        conn.execute("INSERT INTO vehicles (id) VALUES ('v1')", []).unwrap();
        for info in list() {
            let r = run(
                &conn,
                &ReportQuery { id: info.id.clone(), from: "2026-01-01".into(), to: "2026-12-31".into(), vehicle_id: Some("v1".into()) },
            )
            .unwrap();
            assert_eq!(r.headers.len(), r.numeric.len(), "{}", info.id);
        }
        assert!(run(&conn, &ReportQuery { id: "sales".into(), from: "bad".into(), to: "2026-12-31".into(), vehicle_id: None }).is_err());
    }
}
