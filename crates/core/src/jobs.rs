//! Estimates, service jobs and invoices: one record that moves
//! estimate → approved → in progress → completed → invoiced (or cancelled).
//!
//! Part lines can reserve stock. Completing a job consumes each line's reservation (or issues
//! the stock if it was never reserved) exactly once: the movement id is stored on the line, so
//! completing again changes nothing.

use crate::db::{self, clean, dec};
use crate::error::{AppError, AppResult};
use crate::inventory;
use crate::money::{self, HUNDRED};
use crate::pricing::{self, PriceResult};
use crate::qty::Qty;
use rusqlite::{Connection, OptionalExtension, params};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Serialize, TS, Clone, Debug)]
#[ts(export)]
pub struct Job {
    pub id: String,
    pub number: i64,
    pub status: String,
    pub customer_id: Option<String>,
    pub customer_name: Option<String>,
    pub vehicle_id: Option<String>,
    pub vehicle_title: Option<String>,
    pub title: String,
    pub complaint: String,
    pub notes: String,
    pub pricing_profile_id: Option<String>,
    pub mileage: Option<i64>,
    pub technician: String,
    #[ts(type = "string")]
    pub discount_pct: Decimal,
    #[ts(type = "string")]
    pub tax_rate_pct: Decimal,
    pub prices_include_tax: bool,
    pub invoice_number: Option<i64>,
    pub created_at: String,
    pub updated_at: String,
    pub approved_at: Option<String>,
    pub completed_at: Option<String>,
    pub invoiced_at: Option<String>,
}

#[derive(Serialize, TS, Clone, Debug)]
#[ts(export)]
pub struct Line {
    pub id: String,
    pub job_id: String,
    pub sort: i64,
    pub kind: String,
    pub item_id: Option<String>,
    pub part_id: Option<String>,
    pub sku: Option<String>,
    pub description: String,
    #[ts(type = "string")]
    pub qty: Decimal,
    #[ts(type = "string")]
    pub unit_price: Decimal,
    #[ts(type = "string")]
    pub unit_cost: Decimal,
    pub taxable: bool,
    #[ts(type = "string")]
    pub amount: Decimal,
    pub explanation: Option<PriceResult>,
    pub reservation_id: Option<String>,
    pub reservation_status: Option<String>,
    pub consumed: bool,
}

#[derive(Serialize, TS, Clone, Debug, Default, PartialEq)]
#[ts(export)]
pub struct Totals {
    #[ts(type = "string")]
    pub parts: Decimal,
    #[ts(type = "string")]
    pub labor: Decimal,
    #[ts(type = "string")]
    pub fees: Decimal,
    #[ts(type = "string")]
    pub subtotal: Decimal,
    #[ts(type = "string")]
    pub discount: Decimal,
    #[ts(type = "string")]
    pub tax: Decimal,
    #[ts(type = "string")]
    pub total: Decimal,
    #[ts(type = "string")]
    pub cost: Decimal,
    #[ts(type = "string")]
    pub profit: Decimal,
}

#[derive(Serialize, TS, Clone, Debug)]
#[ts(export)]
pub struct JobDetail {
    pub job: Job,
    pub lines: Vec<Line>,
    pub totals: Totals,
    pub profile_name: String,
}

const JOB_SELECT: &str = "SELECT j.id, j.number, j.status, j.customer_id, c.name AS customer_name, j.vehicle_id,
  CASE WHEN v.id IS NULL THEN NULL ELSE trim(coalesce(v.year, '') || ' ' || v.make || ' ' || v.model || ' ' || v.trim) END AS vehicle_title,
  j.title, j.complaint, j.notes, j.pricing_profile_id, j.mileage, j.technician, j.discount_pct, j.tax_rate_pct, j.prices_include_tax, j.invoice_number,
  j.created_at, j.updated_at, j.approved_at, j.completed_at, j.invoiced_at
  FROM service_jobs j LEFT JOIN customers c ON c.id = j.customer_id LEFT JOIN vehicles v ON v.id = j.vehicle_id";

fn job_row(r: &rusqlite::Row) -> rusqlite::Result<Job> {
    Ok(Job {
        id: r.get("id")?,
        number: r.get("number")?,
        status: r.get("status")?,
        customer_id: r.get("customer_id")?,
        customer_name: r.get("customer_name")?,
        vehicle_id: r.get("vehicle_id")?,
        vehicle_title: r.get("vehicle_title")?,
        title: r.get("title")?,
        complaint: r.get("complaint")?,
        notes: r.get("notes")?,
        pricing_profile_id: r.get("pricing_profile_id")?,
        mileage: r.get("mileage")?,
        technician: r.get("technician")?,
        discount_pct: dec(r, "discount_pct")?,
        tax_rate_pct: dec(r, "tax_rate_pct")?,
        prices_include_tax: r.get::<_, i64>("prices_include_tax")? == 1,
        invoice_number: r.get("invoice_number")?,
        created_at: r.get("created_at")?,
        updated_at: r.get("updated_at")?,
        approved_at: r.get("approved_at")?,
        completed_at: r.get("completed_at")?,
        invoiced_at: r.get("invoiced_at")?,
    })
}

pub fn get_job(conn: &Connection, id: &str) -> AppResult<Job> {
    conn.query_row(&format!("{JOB_SELECT} WHERE j.id = ?1"), [id], job_row).optional()?.ok_or_else(|| AppError::not_found("That job"))
}

pub fn lines(conn: &Connection, job_id: &str) -> AppResult<Vec<Line>> {
    let mut st = conn.prepare(
        "SELECT l.id, l.job_id, l.sort, l.kind, l.item_id, l.part_id, i.sku, l.description, l.qty, l.unit_price, l.unit_cost, l.taxable, l.price_explanation,
                l.reservation_id, r.status, l.consumed_movement_id
         FROM service_lines l LEFT JOIN inventory_items i ON i.id = l.item_id LEFT JOIN stock_reservations r ON r.id = l.reservation_id
         WHERE l.job_id = ?1 ORDER BY l.sort, l.id",
    )?;
    let rows = st.query_map([job_id], |r| {
        let qty = dec(r, "qty")?;
        let unit_price = dec(r, "unit_price")?;
        let expl: Option<String> = r.get(12)?;
        Ok(Line {
            id: r.get(0)?,
            job_id: r.get(1)?,
            sort: r.get(2)?,
            kind: r.get(3)?,
            item_id: r.get(4)?,
            part_id: r.get(5)?,
            sku: r.get(6)?,
            description: r.get(7)?,
            qty,
            unit_price,
            unit_cost: dec(r, "unit_cost")?,
            taxable: r.get::<_, i64>(11)? == 1,
            amount: money::round_money(qty * unit_price),
            explanation: expl.and_then(|e| serde_json::from_str(&e).ok()),
            reservation_id: r.get(13)?,
            reservation_status: r.get(14)?,
            consumed: r.get::<_, Option<String>>(15)?.is_some(),
        })
    })?;
    Ok(rows.collect::<Result<_, _>>()?)
}

/// Totals for a set of lines. Discount applies to the subtotal; tax to the taxable share
/// after discount. With tax-inclusive prices the tax is the part already inside the price.
pub fn totals(job: &Job, lines: &[Line]) -> Totals {
    let mut t = Totals::default();
    let mut taxable = Decimal::ZERO;
    for l in lines {
        match l.kind.as_str() {
            "part" => t.parts += l.amount,
            "labor" => t.labor += l.amount,
            _ => t.fees += l.amount,
        }
        if l.taxable {
            taxable += l.amount;
        }
        t.cost += money::round_money(l.qty * l.unit_cost);
    }
    t.subtotal = t.parts + t.labor + t.fees;
    t.discount = money::round_money(t.subtotal * money::frac(job.discount_pct));
    let taxable_after =
        if t.subtotal.is_zero() { Decimal::ZERO } else { taxable - money::round_money(taxable * money::frac(job.discount_pct)) };
    let net_goods = t.subtotal - t.discount;
    if job.prices_include_tax {
        t.tax = money::round_money(taxable_after * job.tax_rate_pct / (HUNDRED + job.tax_rate_pct));
        t.total = net_goods;
        t.profit = net_goods - t.tax - t.cost;
    } else {
        t.tax = money::round_money(taxable_after * money::frac(job.tax_rate_pct));
        t.total = net_goods + t.tax;
        t.profit = net_goods - t.cost;
    }
    t
}

pub fn detail(conn: &Connection, id: &str) -> AppResult<JobDetail> {
    let job = get_job(conn, id)?;
    let lines = lines(conn, id)?;
    let profile = pricing::resolve_profile(conn, &[job.pricing_profile_id.as_deref()])?;
    Ok(JobDetail { totals: totals(&job, &lines), profile_name: profile.name, job, lines })
}

#[derive(Deserialize, TS, Clone, Debug, Default)]
#[ts(export)]
pub struct JobQuery {
    /// "open" (estimate/approved/in progress), a single status, or empty for all.
    pub status: Option<String>,
    pub vehicle_id: Option<String>,
    pub customer_id: Option<String>,
}

pub fn list(conn: &Connection, q: &JobQuery) -> AppResult<Vec<Job>> {
    let status = q.status.as_deref().filter(|s| !s.is_empty());
    let mut st = conn.prepare(&format!(
        "{JOB_SELECT} WHERE (?1 IS NULL OR (?1 = 'open' AND j.status IN ('estimate', 'approved', 'in_progress')) OR j.status = ?1)
           AND (?2 IS NULL OR j.vehicle_id = ?2) AND (?3 IS NULL OR j.customer_id = ?3)
         ORDER BY j.number DESC LIMIT 1000"
    ))?;
    let rows = st.query_map(params![status, q.vehicle_id, q.customer_id], job_row)?;
    Ok(rows.collect::<Result<_, _>>()?)
}

#[derive(Deserialize, TS, Clone, Debug, Default)]
#[ts(export)]
pub struct JobInput {
    pub customer_id: Option<String>,
    pub vehicle_id: Option<String>,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub complaint: String,
    #[serde(default)]
    pub notes: String,
    pub pricing_profile_id: Option<String>,
    pub mileage: Option<i64>,
    #[serde(default)]
    pub technician: String,
    pub discount_pct: Option<String>,
}

fn editable(job: &Job) -> AppResult<()> {
    if matches!(job.status.as_str(), "completed" | "invoiced" | "cancelled") {
        return Err(AppError::Conflict(format!("Job #{} is {} and can't be changed.", job.number, job.status.replace('_', " "))));
    }
    Ok(())
}

pub fn create(conn: &Connection, j: &JobInput) -> AppResult<JobDetail> {
    let id = db::new_id();
    let number: i64 = conn.query_row("SELECT coalesce(max(number), 0) + 1 FROM service_jobs", [], |r| r.get(0))?;
    let customer_profile: Option<String> = match &j.customer_id {
        Some(c) => conn
            .query_row("SELECT pricing_profile_id FROM customers WHERE id = ?1", [c], |r| r.get(0))
            .optional()?
            .ok_or_else(|| AppError::invalid("customer_id", "Choose a customer from the list."))?,
        None => None,
    };
    let profile = pricing::resolve_profile(conn, &[j.pricing_profile_id.as_deref(), customer_profile.as_deref()])?;
    let customer_discount: Option<String> = match &j.customer_id {
        Some(c) => conn.query_row("SELECT discount_pct FROM customers WHERE id = ?1", [c], |r| r.get(0))?,
        None => None,
    };
    let discount = j.discount_pct.clone().or(customer_discount).unwrap_or_else(|| "0".into());
    let discount = money::pct_0_100("discount_pct", "Discount", money::parse("discount_pct", "Discount", &discount)?)?;
    conn.execute(
        "INSERT INTO service_jobs (id, number, customer_id, vehicle_id, title, complaint, notes, pricing_profile_id, mileage, technician, discount_pct, tax_rate_pct, prices_include_tax)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
        params![
            id,
            number,
            j.customer_id,
            j.vehicle_id,
            clean(&j.title),
            j.complaint.trim(),
            j.notes.trim(),
            profile.id,
            j.mileage,
            clean(&j.technician),
            discount.to_string(),
            profile.tax_rate_pct.to_string(),
            profile.prices_include_tax
        ],
    )?;
    db::audit(conn, "job", Some(&id), "create", &format!("Created estimate #{number}"), None)?;
    detail(conn, &id)
}

pub fn update(conn: &Connection, id: &str, j: &JobInput) -> AppResult<JobDetail> {
    let job = get_job(conn, id)?;
    editable(&job)?;
    let discount =
        money::pct_0_100("discount_pct", "Discount", money::parse("discount_pct", "Discount", j.discount_pct.as_deref().unwrap_or("0"))?)?;
    let profile = pricing::resolve_profile(conn, &[j.pricing_profile_id.as_deref()])?;
    conn.execute(
        "UPDATE service_jobs SET customer_id = ?2, vehicle_id = ?3, title = ?4, complaint = ?5, notes = ?6, pricing_profile_id = ?7, mileage = ?8, technician = ?9,
           discount_pct = ?10, tax_rate_pct = ?11, prices_include_tax = ?12, updated_at = ?13 WHERE id = ?1",
        params![
            id,
            j.customer_id,
            j.vehicle_id,
            clean(&j.title),
            j.complaint.trim(),
            j.notes.trim(),
            profile.id,
            j.mileage,
            clean(&j.technician),
            discount.to_string(),
            profile.tax_rate_pct.to_string(),
            profile.prices_include_tax,
            db::now_utc()
        ],
    )?;
    detail(conn, id)
}

fn next_sort(conn: &Connection, job_id: &str) -> AppResult<i64> {
    Ok(conn.query_row("SELECT coalesce(max(sort), 0) + 1 FROM service_lines WHERE job_id = ?1", [job_id], |r| r.get(0))?)
}

/// A part quantity: above zero, whole units unless the item is sold in parts of a unit.
fn part_qty(conn: &Connection, item_id: &str, field: &str, qty: &str) -> AppResult<Qty> {
    let q = Qty::parse(field, qty)?;
    if q <= 0 || q > 100_000 {
        return Err(AppError::invalid(field, "Enter a quantity above zero."));
    }
    inventory::check_whole(conn, item_id, q)?;
    Ok(q)
}

/// A part line's quantity as stock (lines store quantities as decimals).
fn line_qty(l: &Line) -> Qty {
    Qty::from_decimal(l.qty).unwrap_or_default()
}

#[derive(Deserialize, TS, Clone, Debug)]
#[ts(export)]
pub struct PartLineInput {
    pub item_id: String,
    pub qty: String,
    /// Reserve the stock for this job now.
    #[serde(default)]
    pub reserve: bool,
}

pub fn add_part_line(conn: &Connection, job_id: &str, p: &PartLineInput, valuation: &str) -> AppResult<JobDetail> {
    let job = get_job(conn, job_id)?;
    editable(&job)?;
    let qty = part_qty(conn, &p.item_id, "qty", &p.qty)?;
    let item = inventory::get(conn, &p.item_id)?;
    let customer_profile: Option<String> = match &job.customer_id {
        Some(c) => conn.query_row("SELECT pricing_profile_id FROM customers WHERE id = ?1", [c], |r| r.get(0)).optional()?.flatten(),
        None => None,
    };
    let price = pricing::price_item(
        conn,
        &item.id,
        qty.milli() / 1000,
        &[job.pricing_profile_id.as_deref(), customer_profile.as_deref()],
        valuation,
    )?;
    let unit_price = price.price.ok_or_else(|| {
        AppError::Conflict(format!(
            "{} has no selling price under “{}”. Add a pricing rule or set a fixed price on the item first.",
            item.name, price.profile_name
        ))
    })?;
    let line_id = db::new_id();
    conn.execute(
        "INSERT INTO service_lines (id, job_id, sort, kind, item_id, part_id, description, qty, unit_price, unit_cost, taxable, price_explanation)
         VALUES (?1, ?2, ?3, 'part', ?4, ?5, ?6, ?7, ?8, ?9, 1, ?10)",
        params![line_id, job_id, next_sort(conn, job_id)?, item.id, item.part_id, format!("{} ({})", item.name, item.sku), qty.to_string(), unit_price.to_string(), price.cost.to_string(), serde_json::to_string(&price)?],
    )?;
    if p.reserve {
        reserve_line(conn, &line_id)?;
    }
    touch(conn, job_id)?;
    detail(conn, job_id)
}

#[derive(Deserialize, TS, Clone, Debug)]
#[ts(export)]
pub struct OtherLineInput {
    /// "labor" or "fee"
    pub kind: String,
    pub description: String,
    /// Hours for labor, count for fees.
    pub qty: String,
    /// Empty for labor uses the profile's labor rate.
    pub unit_price: Option<String>,
    #[serde(default = "yes")]
    pub taxable: bool,
}
fn yes() -> bool {
    true
}

pub fn add_other_line(conn: &Connection, job_id: &str, l: &OtherLineInput) -> AppResult<JobDetail> {
    let job = get_job(conn, job_id)?;
    editable(&job)?;
    if !matches!(l.kind.as_str(), "labor" | "fee") {
        return Err(AppError::invalid("kind", "Choose labor or a fee."));
    }
    let description = clean(&l.description);
    if description.is_empty() {
        return Err(AppError::invalid("description", "Describe the work or fee."));
    }
    let qty = money::parse("qty", "Quantity", &l.qty)?;
    if qty <= Decimal::ZERO {
        return Err(AppError::invalid("qty", "Enter a quantity above zero."));
    }
    let price = match l.unit_price.as_deref().map(str::trim).filter(|p| !p.is_empty()) {
        Some(p) => money::non_negative("unit_price", "Price", money::parse("unit_price", "Price", p)?)?,
        None if l.kind == "labor" => {
            let profile = pricing::resolve_profile(conn, &[job.pricing_profile_id.as_deref()])?;
            profile.labor_rate.ok_or_else(|| {
                AppError::invalid("unit_price", format!("Enter a rate, or set a labor rate on the “{}” profile.", profile.name))
            })?
        }
        None => return Err(AppError::invalid("unit_price", "Enter the fee amount.")),
    };
    conn.execute(
        "INSERT INTO service_lines (id, job_id, sort, kind, description, qty, unit_price, taxable) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![
            db::new_id(),
            job_id,
            next_sort(conn, job_id)?,
            l.kind,
            description,
            qty.normalize().to_string(),
            price.to_string(),
            l.taxable
        ],
    )?;
    touch(conn, job_id)?;
    detail(conn, job_id)
}

fn line(conn: &Connection, line_id: &str) -> AppResult<Line> {
    let job_id: String = conn
        .query_row("SELECT job_id FROM service_lines WHERE id = ?1", [line_id], |r| r.get(0))
        .optional()?
        .ok_or_else(|| AppError::not_found("That line"))?;
    lines(conn, &job_id)?.into_iter().find(|l| l.id == line_id).ok_or_else(|| AppError::not_found("That line"))
}

#[derive(Deserialize, TS, Clone, Debug)]
#[ts(export)]
pub struct LineUpdate {
    pub description: String,
    pub qty: String,
    pub unit_price: String,
    pub taxable: bool,
}

pub fn update_line(conn: &Connection, line_id: &str, u: &LineUpdate) -> AppResult<JobDetail> {
    let l = line(conn, line_id)?;
    editable(&get_job(conn, &l.job_id)?)?;
    let qty = match (&l.kind[..], &l.item_id) {
        ("part", Some(item)) => part_qty(conn, item, "qty", &u.qty)?.dec(),
        _ => money::parse("qty", "Quantity", &u.qty)?,
    };
    if qty <= Decimal::ZERO {
        return Err(AppError::invalid("qty", "Enter a quantity above zero."));
    }
    let price = money::non_negative("unit_price", "Price", money::parse("unit_price", "Price", &u.unit_price)?)?;
    let mut explanation = l.explanation.clone();
    if price != l.unit_price
        && let Some(e) = &mut explanation
    {
        e.steps.push(format!("Price changed by hand to ${}", money::round_money(price)));
        e.price = Some(price);
    }
    conn.execute(
        "UPDATE service_lines SET description = ?2, qty = ?3, unit_price = ?4, taxable = ?5, price_explanation = ?6 WHERE id = ?1",
        params![
            line_id,
            clean(&u.description),
            qty.normalize().to_string(),
            price.to_string(),
            u.taxable,
            explanation.map(|e| serde_json::to_string(&e)).transpose()?
        ],
    )?;
    // A reservation follows the new quantity.
    if l.reservation_status.as_deref() == Some("active") && qty != l.qty {
        release_line(conn, line_id)?;
        reserve_line(conn, line_id)?;
    }
    touch(conn, &l.job_id)?;
    detail(conn, &l.job_id)
}

pub fn delete_line(conn: &Connection, line_id: &str) -> AppResult<JobDetail> {
    let l = line(conn, line_id)?;
    editable(&get_job(conn, &l.job_id)?)?;
    if let Some(r) = &l.reservation_id {
        inventory::release(conn, r)?;
    }
    conn.execute("DELETE FROM service_lines WHERE id = ?1", [line_id])?;
    touch(conn, &l.job_id)?;
    detail(conn, &l.job_id)
}

pub fn reserve_line(conn: &Connection, line_id: &str) -> AppResult<JobDetail> {
    let l = line(conn, line_id)?;
    let job = get_job(conn, &l.job_id)?;
    editable(&job)?;
    let item_id = l.item_id.clone().ok_or_else(|| AppError::msg("Only inventory parts can be reserved."))?;
    if l.reservation_status.as_deref() == Some("active") {
        return detail(conn, &l.job_id);
    }
    let qty = line_qty(&l);
    let r = inventory::reserve(
        conn,
        &inventory::Reserve {
            item_id,
            location_id: None,
            qty,
            job_id: Some(job.id.clone()),
            line_id: Some(line_id.into()),
            vehicle_id: job.vehicle_id.clone(),
            note: format!("Job #{}", job.number),
        },
    )?;
    conn.execute("UPDATE service_lines SET reservation_id = ?2 WHERE id = ?1", params![line_id, r.id])?;
    detail(conn, &l.job_id)
}

pub fn release_line(conn: &Connection, line_id: &str) -> AppResult<JobDetail> {
    let l = line(conn, line_id)?;
    if let Some(r) = &l.reservation_id {
        inventory::release(conn, r)?;
    }
    conn.execute("UPDATE service_lines SET reservation_id = NULL WHERE id = ?1", [line_id])?;
    detail(conn, &l.job_id)
}

fn touch(conn: &Connection, job_id: &str) -> AppResult<()> {
    conn.execute("UPDATE service_jobs SET updated_at = ?2 WHERE id = ?1", params![job_id, db::now_utc()])?;
    Ok(())
}

/// Move a job to the next status. Completing consumes or issues stock for every part line, all
/// or nothing; invoicing assigns the next invoice number.
pub fn set_status(conn: &Connection, id: &str, to: &str) -> AppResult<JobDetail> {
    let job = get_job(conn, id)?;
    let now = db::now_utc();
    let allowed = matches!(
        (job.status.as_str(), to),
        ("estimate", "approved" | "in_progress")
            | ("approved", "in_progress" | "estimate")
            | ("estimate" | "approved" | "in_progress", "completed" | "cancelled")
            | ("completed", "invoiced")
    );
    if !allowed {
        return Err(AppError::Conflict(format!("A {} job can't be marked {}.", job.status.replace('_', " "), to.replace('_', " "))));
    }
    match to {
        "approved" => {
            conn.execute("UPDATE service_jobs SET status = 'approved', approved_at = ?2, updated_at = ?2 WHERE id = ?1", params![id, now])?;
        }
        "completed" => {
            for l in lines(conn, id)?.iter().filter(|l| l.kind == "part" && !l.consumed) {
                let Some(item_id) = &l.item_id else { continue };
                let movement = match (&l.reservation_id, l.reservation_status.as_deref()) {
                    (Some(r), Some("active" | "consumed")) => inventory::consume(conn, r)?,
                    _ => {
                        let qty = line_qty(l);
                        let loc: Option<String> = conn
                            .query_row(
                                "SELECT s.location_id FROM stock_levels s JOIN inventory_locations x ON x.id = s.location_id
                                 WHERE s.item_id = ?1 AND x.kind = 'warehouse' AND s.on_hand - s.reserved >= ?2 ORDER BY x.is_default DESC LIMIT 1",
                                params![item_id, qty],
                                |r| r.get(0),
                            )
                            .optional()?;
                        let loc = loc.ok_or_else(|| {
                            AppError::Conflict(format!(
                                "Not enough {} in one location to complete the job. Receive or transfer stock first.",
                                l.description
                            ))
                        })?;
                        inventory::issue(
                            conn,
                            &inventory::Change {
                                item_id: item_id.clone(),
                                location_id: loc,
                                qty,
                                reason: format!("Job #{}", job.number),
                                ref_kind: Some("job".into()),
                                ref_id: Some(id.into()),
                            },
                        )?
                    }
                };
                conn.execute("UPDATE service_lines SET consumed_movement_id = ?2 WHERE id = ?1", params![l.id, movement])?;
            }
            conn.execute(
                "UPDATE service_jobs SET status = 'completed', completed_at = ?2, updated_at = ?2 WHERE id = ?1",
                params![id, now],
            )?;
        }
        "invoiced" => {
            let n: i64 = conn.query_row("SELECT coalesce(max(invoice_number), 1000) + 1 FROM service_jobs", [], |r| r.get(0))?;
            conn.execute(
                "UPDATE service_jobs SET status = 'invoiced', invoice_number = ?2, invoiced_at = ?3, updated_at = ?3 WHERE id = ?1",
                params![id, n, now],
            )?;
        }
        "cancelled" => {
            for l in lines(conn, id)? {
                if let Some(r) = &l.reservation_id {
                    inventory::release(conn, r)?;
                }
            }
            conn.execute("UPDATE service_jobs SET status = 'cancelled', updated_at = ?2 WHERE id = ?1", params![id, now])?;
        }
        other => {
            conn.execute("UPDATE service_jobs SET status = ?2, updated_at = ?3 WHERE id = ?1", params![id, other, now])?;
        }
    }
    db::audit(conn, "job", Some(id), "status", &format!("Job #{} → {}", job.number, to), None)?;
    detail(conn, id)
}

pub fn delete(conn: &Connection, id: &str) -> AppResult<()> {
    let job = get_job(conn, id)?;
    if !matches!(job.status.as_str(), "estimate" | "cancelled") {
        return Err(AppError::Conflict("Only estimates and cancelled jobs can be deleted; completed work stays in the history.".into()));
    }
    for l in lines(conn, id)? {
        if let Some(r) = &l.reservation_id {
            inventory::release(conn, r)?;
        }
    }
    conn.execute("DELETE FROM service_jobs WHERE id = ?1", [id])?;
    db::audit(conn, "job", Some(id), "delete", &format!("Deleted job #{}", job.number), None)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{inventory as inv, parts};
    use rust_decimal_macros::dec;

    fn setup() -> (Connection, String) {
        let conn = db::open_memory().unwrap();
        let p = parts::create(
            &conn,
            &parts::PartInput { name: "Brake pads".into(), category_id: Some("brakes.pads".into()), ..Default::default() },
            "user",
        )
        .unwrap();
        let i = inv::create(
            &conn,
            &p.id,
            &inv::ItemInput {
                sku: "BP-1".into(),
                condition: "new".into(),
                unit: "each".into(),
                cost: "50".into(),
                price_override: None,
                pricing_profile_id: None,
                fractional: false,
                min_stock: Qty::units(0),
                reorder_qty: Qty::units(0),
                supplier_id: None,
                warranty: String::new(),
                notes: String::new(),
            },
        )
        .unwrap();
        inv::receive(
            &conn,
            &inv::Receive {
                item_id: i.id.clone(),
                location_id: "loc-main".into(),
                qty: Qty::units(4),
                unit_cost: Some("50".into()),
                bin: None,
                reason: String::new(),
                ref_kind: None,
                ref_id: None,
            },
        )
        .unwrap();
        let d = pricing::default_profile(&conn).unwrap();
        pricing::save_rule(
            &conn,
            &d.id,
            None,
            &pricing::RuleInput {
                priority: 10,
                scope: "all".into(),
                scope_value: None,
                method: "markup".into(),
                value: "40".into(),
                min_qty: 1,
                min_price: None,
                enabled: true,
                note: String::new(),
            },
        )
        .unwrap();
        pricing::save_profile(
            &conn,
            Some(d.id.as_str()),
            &pricing::ProfileInput {
                name: "Default Retail".into(),
                kind: "retail".into(),
                tax_rate_pct: "10".into(),
                prices_include_tax: false,
                rounding_increment: "0.01".into(),
                rounding_mode: "nearest".into(),
                rounding_ending: None,
                labor_rate: Some("120".into()),
                notes: String::new(),
            },
        )
        .unwrap();
        (conn, i.id)
    }

    #[test]
    fn estimate_totals_and_explanations() {
        let (conn, item) = setup();
        let j = create(&conn, &JobInput { title: "Front brakes".into(), ..Default::default() }).unwrap();
        assert_eq!(j.job.status, "estimate");
        let d =
            add_part_line(&conn, &j.job.id, &PartLineInput { item_id: item.clone(), qty: "2".into(), reserve: false }, "average").unwrap();
        assert_eq!(d.lines[0].unit_price, dec!(70.00));
        assert!(d.lines[0].explanation.as_ref().unwrap().steps.iter().any(|s| s.contains("40% markup")));
        let d = add_other_line(
            &conn,
            &j.job.id,
            &OtherLineInput {
                kind: "labor".into(),
                description: "Replace pads".into(),
                qty: "1.5".into(),
                unit_price: None,
                taxable: false,
            },
        )
        .unwrap();
        assert_eq!(d.totals.parts, dec!(140.00));
        assert_eq!(d.totals.labor, dec!(180.00));
        assert_eq!(d.totals.tax, dec!(14.00), "tax only on the taxable parts");
        assert_eq!(d.totals.total, dec!(334.00));
        assert_eq!(d.totals.profit, dec!(220.00));
        assert!(add_part_line(&conn, &j.job.id, &PartLineInput { item_id: item, qty: "1.5".into(), reserve: false }, "average").is_err());
    }

    #[test]
    fn workflow_never_double_deducts() {
        let (conn, item) = setup();
        let j = create(&conn, &JobInput::default()).unwrap();
        let d =
            add_part_line(&conn, &j.job.id, &PartLineInput { item_id: item.clone(), qty: "2".into(), reserve: true }, "average").unwrap();
        assert_eq!(d.lines[0].reservation_status.as_deref(), Some("active"));
        assert_eq!(inv::get(&conn, &item).unwrap().available, 2);
        add_part_line(&conn, &j.job.id, &PartLineInput { item_id: item.clone(), qty: "1".into(), reserve: false }, "average").unwrap();
        set_status(&conn, &j.job.id, "approved").unwrap();
        let done = set_status(&conn, &j.job.id, "completed").unwrap();
        assert!(done.lines.iter().all(|l| l.consumed));
        let i = inv::get(&conn, &item).unwrap();
        assert_eq!((i.on_hand, i.reserved), (Qty::units(1), Qty::units(0)), "2 reserved + 1 issued, each once");
        assert!(set_status(&conn, &j.job.id, "completed").is_err(), "can't complete twice");
        let inv1 = set_status(&conn, &j.job.id, "invoiced").unwrap();
        assert_eq!(inv1.job.invoice_number, Some(1001));
        assert!(
            add_other_line(
                &conn,
                &j.job.id,
                &OtherLineInput {
                    kind: "fee".into(),
                    description: "x".into(),
                    qty: "1".into(),
                    unit_price: Some("1".into()),
                    taxable: true
                }
            )
            .is_err()
        );
        assert_eq!(inv::get(&conn, &item).unwrap().on_hand, 1);
    }

    #[test]
    fn completing_without_stock_rolls_back() {
        let (mut conn, item) = setup();
        let j = create(&conn, &JobInput::default()).unwrap();
        add_part_line(&conn, &j.job.id, &PartLineInput { item_id: item.clone(), qty: "3".into(), reserve: true }, "average").unwrap();
        add_part_line(&conn, &j.job.id, &PartLineInput { item_id: item.clone(), qty: "3".into(), reserve: false }, "average").unwrap();
        let tx = conn.transaction().unwrap();
        assert!(set_status(&tx, &j.job.id, "completed").is_err());
        drop(tx);
        let i = inv::get(&conn, &item).unwrap();
        assert_eq!((i.on_hand, i.reserved), (Qty::units(4), Qty::units(3)), "the failed completion changed nothing");
    }

    #[test]
    fn cancelling_releases_reservations() {
        let (conn, item) = setup();
        let j = create(&conn, &JobInput::default()).unwrap();
        add_part_line(&conn, &j.job.id, &PartLineInput { item_id: item.clone(), qty: "2".into(), reserve: true }, "average").unwrap();
        set_status(&conn, &j.job.id, "cancelled").unwrap();
        assert_eq!(inv::get(&conn, &item).unwrap().reserved, 0);
    }
}
