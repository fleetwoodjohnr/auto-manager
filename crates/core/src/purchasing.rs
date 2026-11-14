//! Purchasing: supplier prices and part numbers, the reorder assistant, purchase orders and
//! receiving against them (receipts go through the inventory ledger like any other).

use crate::db::{self, clean, dec, expect_one};
use crate::error::{AppError, AppResult};
use crate::{inventory, money};
use rusqlite::{Connection, OptionalExtension, params};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Serialize, TS, Clone, Debug)]
#[ts(export)]
pub struct SupplierPrice {
    pub supplier_id: String,
    pub supplier: String,
    pub item_id: String,
    pub sku: String,
    pub name: String,
    pub supplier_sku: String,
    #[ts(type = "string | null")]
    pub cost: Option<Decimal>,
    pub lead_time_days: Option<i64>,
    pub preferred: bool,
    pub updated_at: String,
}

const SP: &str = "SELECT sp.supplier_id, s.name, sp.item_id, i.sku, p.name, sp.supplier_sku, sp.cost, coalesce(sp.lead_time_days, s.lead_time_days),
  (i.supplier_id = sp.supplier_id), sp.updated_at
  FROM supplier_parts sp JOIN suppliers s ON s.id = sp.supplier_id JOIN inventory_items i ON i.id = sp.item_id JOIN parts p ON p.id = i.part_id";

fn sp_row(r: &rusqlite::Row) -> rusqlite::Result<SupplierPrice> {
    Ok(SupplierPrice {
        supplier_id: r.get(0)?,
        supplier: r.get(1)?,
        item_id: r.get(2)?,
        sku: r.get(3)?,
        name: r.get(4)?,
        supplier_sku: r.get(5)?,
        cost: db::opt_dec(r, "cost")?,
        lead_time_days: r.get(7)?,
        preferred: r.get::<_, Option<bool>>(8)?.unwrap_or(false),
        updated_at: r.get(9)?,
    })
}

/// Every supplier's price for an item, cheapest first (supplier price comparison).
pub fn prices_for_item(conn: &Connection, item_id: &str) -> AppResult<Vec<SupplierPrice>> {
    let mut st = conn.prepare(&format!("{SP} WHERE sp.item_id = ?1 ORDER BY sp.cost IS NULL, CAST(sp.cost AS REAL)"))?;
    let rows = st.query_map([item_id], sp_row)?;
    Ok(rows.collect::<Result<_, _>>()?)
}

pub fn prices_for_supplier(conn: &Connection, supplier_id: &str) -> AppResult<Vec<SupplierPrice>> {
    let mut st = conn.prepare(&format!("{SP} WHERE sp.supplier_id = ?1 ORDER BY p.name"))?;
    let rows = st.query_map([supplier_id], sp_row)?;
    Ok(rows.collect::<Result<_, _>>()?)
}

#[derive(Deserialize, TS, Clone, Debug)]
#[ts(export)]
pub struct SupplierPriceInput {
    pub supplier_id: String,
    pub item_id: String,
    #[serde(default)]
    pub supplier_sku: String,
    pub cost: Option<String>,
    pub lead_time_days: Option<i64>,
}

pub fn set_price(conn: &Connection, p: &SupplierPriceInput) -> AppResult<()> {
    let cost = p
        .cost
        .as_deref()
        .map(str::trim)
        .filter(|c| !c.is_empty())
        .map(|c| money::parse("cost", "Cost", c).and_then(|d| money::non_negative("cost", "Cost", d)))
        .transpose()?;
    conn.execute(
        "INSERT INTO supplier_parts (supplier_id, item_id, supplier_sku, cost, lead_time_days, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)
         ON CONFLICT (supplier_id, item_id) DO UPDATE SET supplier_sku = excluded.supplier_sku, cost = excluded.cost, lead_time_days = excluded.lead_time_days, updated_at = excluded.updated_at",
        params![p.supplier_id, p.item_id, clean(&p.supplier_sku), cost.map(|c| c.to_string()), p.lead_time_days, db::now_utc()],
    )?;
    Ok(())
}

pub fn remove_price(conn: &Connection, supplier_id: &str, item_id: &str) -> AppResult<()> {
    conn.execute("DELETE FROM supplier_parts WHERE supplier_id = ?1 AND item_id = ?2", params![supplier_id, item_id])?;
    Ok(())
}

/// A supplier price list (CSV/XLSX with SKU or supplier part number, and cost) applied to the
/// items it names. Returns (updated, not found).
pub fn import_price_list(conn: &Connection, supplier_id: &str, table: &crate::csvio::Table) -> AppResult<(usize, Vec<String>)> {
    let col = |names: &[&str]| table.headers.iter().position(|h| names.iter().any(|n| h.to_lowercase().contains(n)));
    let sku_col = col(&["sku", "our"]);
    let ssku_col = col(&["supplier", "vendor", "part number", "part no", "mpn", "item"]);
    let cost_col = col(&["cost", "price", "net"]).ok_or_else(|| AppError::msg("The price list needs a cost or price column."))?;
    if sku_col.is_none() && ssku_col.is_none() {
        return Err(AppError::msg("The price list needs a SKU or supplier part number column."));
    }
    let (mut updated, mut missing) = (0, Vec::new());
    for r in &table.rows {
        let get = |c: Option<usize>| c.and_then(|i| r.get(i)).map(|s| s.trim().to_string()).unwrap_or_default();
        let (sku, ssku, cost) = (get(sku_col), get(ssku_col), get(Some(cost_col)));
        let item: Option<String> = if !sku.is_empty() {
            inventory::by_sku(conn, &sku)?.map(|i| i.id)
        } else {
            conn.query_row(
                "SELECT item_id FROM supplier_parts WHERE supplier_id = ?1 AND supplier_sku = ?2 COLLATE NOCASE",
                params![supplier_id, ssku],
                |x| x.get(0),
            )
            .optional()?
            .or(conn
                .query_row(
                    "SELECT i.id FROM inventory_items i JOIN part_identifiers x ON x.part_id = i.part_id WHERE x.value_norm = ?1 LIMIT 1",
                    [crate::parts::norm(&ssku)],
                    |x| x.get(0),
                )
                .optional()?)
        };
        match item {
            Some(item_id) => {
                set_price(
                    conn,
                    &SupplierPriceInput {
                        supplier_id: supplier_id.into(),
                        item_id,
                        supplier_sku: ssku,
                        cost: Some(cost),
                        lead_time_days: None,
                    },
                )?;
                updated += 1;
            }
            None => missing.push(if sku.is_empty() { ssku } else { sku }),
        }
    }
    Ok((updated, missing))
}

// ---------------------------------------------------------------- reorder assistant

#[derive(Serialize, TS, Clone, Debug)]
#[ts(export)]
pub struct Suggestion {
    pub item_id: String,
    pub sku: String,
    pub name: String,
    pub available: i64,
    pub min_stock: i64,
    pub on_order: i64,
    /// Units issued or used in the last 90 days.
    pub used_90d: i64,
    pub suggested_qty: i64,
    pub supplier_id: Option<String>,
    pub supplier: Option<String>,
    #[ts(type = "string")]
    pub unit_cost: Decimal,
    /// Why it's suggested: "below minimum", "restocking request", …
    pub reason: String,
}

pub fn on_order(conn: &Connection, item_id: &str) -> AppResult<i64> {
    Ok(conn.query_row(
        "SELECT coalesce(sum(l.qty_ordered - l.qty_received), 0) FROM po_lines l JOIN purchase_orders o ON o.id = l.po_id WHERE l.item_id = ?1 AND o.status IN ('draft', 'ordered', 'partial')",
        [item_id],
        |r| r.get(0),
    )?)
}

fn used_90d(conn: &Connection, item_id: &str) -> AppResult<i64> {
    Ok(conn.query_row(
        "SELECT coalesce(-sum(qty), 0) FROM inventory_movements WHERE item_id = ?1 AND kind IN ('issue', 'consume') AND created_at >= strftime('%Y-%m-%dT%H:%M:%fZ', 'now', '-90 days')
           AND id NOT IN (SELECT reverses_id FROM inventory_movements WHERE reverses_id IS NOT NULL)",
        [item_id],
        |r| r.get(0),
    )?)
}

/// What to order: items at or below minimum stock (after what's already on order), and parts
/// on the restocking list. Quantity covers the shortfall plus expected use over the lead time,
/// and at least the item's reorder quantity.
pub fn suggestions(conn: &Connection) -> AppResult<Vec<Suggestion>> {
    let restock: Vec<String> = conn
        .prepare("SELECT x.part_id FROM part_list_items x JOIN part_lists l ON l.id = x.list_id WHERE l.name = ?1")?
        .query_map([crate::lists::RESTOCK_LIST], |r| r.get(0))?
        .collect::<Result<_, _>>()?;
    let mut out = Vec::new();
    for i in inventory::list(conn, &inventory::ItemQuery::default())? {
        let ordered = on_order(conn, &i.id)?;
        let below = i.min_stock > 0 && i.available + ordered <= i.min_stock;
        let requested = restock.contains(&i.part_id) && i.available + ordered <= 0;
        if !below && !requested {
            continue;
        }
        let used = used_90d(conn, &i.id)?;
        let best = prices_for_item(conn, &i.id)?.into_iter().find(|p| p.cost.is_some());
        let supplier_id = i.supplier_id.clone().or(best.as_ref().map(|b| b.supplier_id.clone()));
        let lead: i64 = match &supplier_id {
            Some(s) => conn.query_row("SELECT coalesce(lead_time_days, 7) FROM suppliers WHERE id = ?1", [s], |r| r.get(0))?,
            None => 7,
        };
        let shortfall = (i.min_stock - i.available - ordered).max(0);
        let during_lead = (used * lead + 89) / 90;
        let qty = (shortfall + during_lead).max(i.reorder_qty).max(1);
        let unit_cost = match &supplier_id {
            Some(s) => prices_for_item(conn, &i.id)?.into_iter().find(|p| &p.supplier_id == s).and_then(|p| p.cost).unwrap_or(i.cost),
            None => i.cost,
        };
        let supplier = match &supplier_id {
            Some(s) => conn.query_row("SELECT name FROM suppliers WHERE id = ?1", [s], |r| r.get(0)).optional()?,
            None => None,
        };
        out.push(Suggestion {
            item_id: i.id,
            sku: i.sku,
            name: i.name,
            available: i.available,
            min_stock: i.min_stock,
            on_order: ordered,
            used_90d: used,
            suggested_qty: qty,
            supplier_id,
            supplier,
            unit_cost,
            reason: if below { format!("{} available, minimum {}", i.available, i.min_stock) } else { "Restocking request".into() },
        });
    }
    Ok(out)
}

// ---------------------------------------------------------------- purchase orders

#[derive(Serialize, TS, Clone, Debug)]
#[ts(export)]
pub struct PoLine {
    pub id: String,
    pub item_id: String,
    pub sku: String,
    pub name: String,
    pub supplier_sku: String,
    pub qty_ordered: i64,
    pub qty_received: i64,
    #[ts(type = "string")]
    pub unit_cost: Decimal,
    #[ts(type = "string")]
    pub amount: Decimal,
}

#[derive(Serialize, TS, Clone, Debug)]
#[ts(export)]
pub struct PurchaseOrder {
    pub id: String,
    pub number: i64,
    pub supplier_id: String,
    pub supplier: String,
    pub status: String,
    pub expected_date: Option<String>,
    pub notes: String,
    pub created_at: String,
    pub ordered_at: Option<String>,
    pub closed_at: Option<String>,
    pub lines: Vec<PoLine>,
    #[ts(type = "string")]
    pub total: Decimal,
    /// Ordered, not yet received, and past the expected date.
    pub late: bool,
}

pub fn get_po(conn: &Connection, id: &str) -> AppResult<PurchaseOrder> {
    let mut po = conn
        .query_row(
            "SELECT o.id, o.number, o.supplier_id, s.name, o.status, o.expected_date, o.notes, o.created_at, o.ordered_at, o.closed_at
             FROM purchase_orders o JOIN suppliers s ON s.id = o.supplier_id WHERE o.id = ?1",
            [id],
            |r| {
                Ok(PurchaseOrder {
                    id: r.get(0)?,
                    number: r.get(1)?,
                    supplier_id: r.get(2)?,
                    supplier: r.get(3)?,
                    status: r.get(4)?,
                    expected_date: r.get(5)?,
                    notes: r.get(6)?,
                    created_at: r.get(7)?,
                    ordered_at: r.get(8)?,
                    closed_at: r.get(9)?,
                    lines: vec![],
                    total: Decimal::ZERO,
                    late: false,
                })
            },
        )
        .optional()?
        .ok_or_else(|| AppError::not_found("That purchase order"))?;
    let mut st = conn.prepare(
        "SELECT l.id, l.item_id, i.sku, p.name, coalesce(sp.supplier_sku, ''), l.qty_ordered, l.qty_received, l.unit_cost
         FROM po_lines l JOIN inventory_items i ON i.id = l.item_id JOIN parts p ON p.id = i.part_id
         LEFT JOIN supplier_parts sp ON sp.item_id = l.item_id AND sp.supplier_id = ?2
         WHERE l.po_id = ?1 ORDER BY l.sort, p.name",
    )?;
    po.lines = st
        .query_map(params![id, po.supplier_id], |r| {
            let unit_cost = dec(r, "unit_cost")?;
            let q: i64 = r.get(5)?;
            Ok(PoLine {
                id: r.get(0)?,
                item_id: r.get(1)?,
                sku: r.get(2)?,
                name: r.get(3)?,
                supplier_sku: r.get(4)?,
                qty_ordered: q,
                qty_received: r.get(6)?,
                unit_cost,
                amount: money::round_money(unit_cost * Decimal::from(q)),
            })
        })?
        .collect::<Result<_, _>>()?;
    po.total = po.lines.iter().map(|l| l.amount).sum();
    let today = chrono::Utc::now().date_naive().to_string();
    po.late = matches!(po.status.as_str(), "ordered" | "partial") && po.expected_date.as_ref().is_some_and(|d| *d < today);
    Ok(po)
}

pub fn list_pos(conn: &Connection, open_only: bool) -> AppResult<Vec<PurchaseOrder>> {
    let ids: Vec<String> = conn
        .prepare("SELECT id FROM purchase_orders WHERE NOT ?1 OR status IN ('draft', 'ordered', 'partial') ORDER BY number DESC LIMIT 500")?
        .query_map([open_only], |r| r.get(0))?
        .collect::<Result<_, _>>()?;
    ids.iter().map(|i| get_po(conn, i)).collect()
}

pub fn create_po(conn: &Connection, supplier_id: &str) -> AppResult<PurchaseOrder> {
    if !db::exists(conn, "SELECT 1 FROM suppliers WHERE id = ?1", [supplier_id])? {
        return Err(AppError::invalid("supplier_id", "Choose a supplier."));
    }
    let id = db::new_id();
    let n: i64 = conn.query_row("SELECT coalesce(max(number), 0) + 1 FROM purchase_orders", [], |r| r.get(0))?;
    conn.execute("INSERT INTO purchase_orders (id, number, supplier_id) VALUES (?1, ?2, ?3)", params![id, n, supplier_id])?;
    get_po(conn, &id)
}

fn draft(conn: &Connection, po_id: &str) -> AppResult<PurchaseOrder> {
    let po = get_po(conn, po_id)?;
    if po.status != "draft" {
        return Err(AppError::Conflict(format!("PO {} has been sent; only drafts can be changed.", po.number)));
    }
    Ok(po)
}

pub fn add_line(conn: &Connection, po_id: &str, item_id: &str, qty: i64, unit_cost: Option<&str>) -> AppResult<PurchaseOrder> {
    let po = draft(conn, po_id)?;
    if qty < 1 {
        return Err(AppError::invalid("qty", "Order at least 1."));
    }
    let item = inventory::get(conn, item_id)?;
    let cost = match unit_cost.map(str::trim).filter(|c| !c.is_empty()) {
        Some(c) => money::non_negative("unit_cost", "Cost", money::parse("unit_cost", "Cost", c)?)?,
        None => {
            prices_for_item(conn, item_id)?.into_iter().find(|p| p.supplier_id == po.supplier_id).and_then(|p| p.cost).unwrap_or(item.cost)
        }
    };
    conn.execute(
        "INSERT INTO po_lines (id, po_id, item_id, qty_ordered, unit_cost, sort) VALUES (?1, ?2, ?3, ?4, ?5, (SELECT coalesce(max(sort), 0) + 1 FROM po_lines WHERE po_id = ?2))
         ON CONFLICT (po_id, item_id) DO UPDATE SET qty_ordered = po_lines.qty_ordered + excluded.qty_ordered",
        params![db::new_id(), po_id, item_id, qty, cost.to_string()],
    )?;
    get_po(conn, po_id)
}

pub fn update_line(conn: &Connection, line_id: &str, qty: i64, unit_cost: &str) -> AppResult<PurchaseOrder> {
    let po_id: String = conn
        .query_row("SELECT po_id FROM po_lines WHERE id = ?1", [line_id], |r| r.get(0))
        .optional()?
        .ok_or_else(|| AppError::not_found("That line"))?;
    draft(conn, &po_id)?;
    if qty < 1 {
        return Err(AppError::invalid("qty", "Order at least 1."));
    }
    let cost = money::non_negative("unit_cost", "Cost", money::parse("unit_cost", "Cost", unit_cost)?)?;
    conn.execute("UPDATE po_lines SET qty_ordered = ?2, unit_cost = ?3 WHERE id = ?1", params![line_id, qty, cost.to_string()])?;
    get_po(conn, &po_id)
}

pub fn remove_line(conn: &Connection, line_id: &str) -> AppResult<PurchaseOrder> {
    let po_id: String = conn
        .query_row("SELECT po_id FROM po_lines WHERE id = ?1", [line_id], |r| r.get(0))
        .optional()?
        .ok_or_else(|| AppError::not_found("That line"))?;
    draft(conn, &po_id)?;
    conn.execute("DELETE FROM po_lines WHERE id = ?1", [line_id])?;
    get_po(conn, &po_id)
}

#[derive(Deserialize, TS, Clone, Debug)]
#[ts(export)]
pub struct PoUpdate {
    pub expected_date: Option<String>,
    #[serde(default)]
    pub notes: String,
}

pub fn update_po(conn: &Connection, id: &str, u: &PoUpdate) -> AppResult<PurchaseOrder> {
    let date = u.expected_date.as_deref().map(str::trim).filter(|d| !d.is_empty());
    if let Some(d) = date
        && chrono::NaiveDate::parse_from_str(d, "%Y-%m-%d").is_err()
    {
        return Err(AppError::invalid("expected_date", "Enter the date as YYYY-MM-DD."));
    }
    expect_one(
        conn.execute("UPDATE purchase_orders SET expected_date = ?2, notes = ?3 WHERE id = ?1", params![id, date, u.notes.trim()])?,
        "That purchase order",
    )?;
    get_po(conn, id)
}

pub fn set_po_status(conn: &Connection, id: &str, status: &str) -> AppResult<PurchaseOrder> {
    let po = get_po(conn, id)?;
    let now = db::now_utc();
    match (po.status.as_str(), status) {
        ("draft", "ordered") => {
            if po.lines.is_empty() {
                return Err(AppError::msg("Add at least one line before sending the order."));
            }
            conn.execute("UPDATE purchase_orders SET status = 'ordered', ordered_at = ?2 WHERE id = ?1", params![id, now])?;
        }
        ("draft" | "ordered" | "partial", "cancelled") => {
            conn.execute("UPDATE purchase_orders SET status = 'cancelled', closed_at = ?2 WHERE id = ?1", params![id, now])?;
        }
        ("ordered" | "partial", "received") => {
            // Close a PO whose remaining items won't arrive.
            conn.execute("UPDATE purchase_orders SET status = 'received', closed_at = ?2 WHERE id = ?1", params![id, now])?;
        }
        (from, to) => return Err(AppError::Conflict(format!("A {from} order can't be marked {to}."))),
    }
    db::audit(conn, "po", Some(id), "status", &format!("PO {} → {status}", po.number), None)?;
    get_po(conn, id)
}

pub fn delete_po(conn: &Connection, id: &str) -> AppResult<()> {
    let po = get_po(conn, id)?;
    if !matches!(po.status.as_str(), "draft" | "cancelled") || po.lines.iter().any(|l| l.qty_received > 0) {
        return Err(AppError::Conflict("Only drafts and cancelled orders with nothing received can be deleted.".into()));
    }
    conn.execute("DELETE FROM purchase_orders WHERE id = ?1", [id])?;
    Ok(())
}

#[derive(Deserialize, TS, Clone, Debug)]
#[ts(export)]
pub struct ReceiveLine {
    pub line_id: String,
    pub qty: i64,
}

/// Receive (part of) an order into a location. Each line goes through the inventory ledger;
/// the order becomes partial or received.
pub fn receive(conn: &Connection, po_id: &str, location_id: &str, lines: &[ReceiveLine]) -> AppResult<PurchaseOrder> {
    let po = get_po(conn, po_id)?;
    if !matches!(po.status.as_str(), "ordered" | "partial") {
        return Err(AppError::Conflict("Mark the order as sent before receiving it.".into()));
    }
    for r in lines.iter().filter(|r| r.qty != 0) {
        let line = po.lines.iter().find(|l| l.id == r.line_id).ok_or_else(|| AppError::not_found("That order line"))?;
        if r.qty < 0 || line.qty_received + r.qty > line.qty_ordered {
            return Err(AppError::invalid(
                "qty",
                format!("{}: you can receive up to {} more.", line.sku, line.qty_ordered - line.qty_received),
            ));
        }
        inventory::receive(
            conn,
            &inventory::Receive {
                item_id: line.item_id.clone(),
                location_id: location_id.into(),
                qty: r.qty,
                unit_cost: Some(line.unit_cost.to_string()),
                bin: None,
                reason: format!("PO {}", po.number),
                ref_kind: Some("po".into()),
                ref_id: Some(po_id.into()),
            },
        )?;
        conn.execute("UPDATE po_lines SET qty_received = qty_received + ?2 WHERE id = ?1", params![r.line_id, r.qty])?;
    }
    let left: i64 =
        conn.query_row("SELECT coalesce(sum(qty_ordered - qty_received), 0) FROM po_lines WHERE po_id = ?1", [po_id], |r| r.get(0))?;
    let status = if left == 0 { "received" } else { "partial" };
    conn.execute(
        "UPDATE purchase_orders SET status = ?2, closed_at = CASE WHEN ?2 = 'received' THEN ?3 ELSE NULL END WHERE id = ?1",
        params![po_id, status, db::now_utc()],
    )?;
    get_po(conn, po_id)
}

/// Turn reorder suggestions into draft orders, one per supplier. Items without a supplier are
/// left out (returned) so the person can choose one.
pub fn orders_from_suggestions(conn: &Connection, picks: &[(String, i64)]) -> AppResult<(Vec<PurchaseOrder>, Vec<String>)> {
    let all = suggestions(conn)?;
    let mut by_supplier: std::collections::BTreeMap<String, Vec<(String, i64, Decimal)>> = Default::default();
    let mut skipped = Vec::new();
    for (item_id, qty) in picks {
        let s = all.iter().find(|s| &s.item_id == item_id);
        match s.and_then(|s| s.supplier_id.clone()) {
            Some(sup) => by_supplier.entry(sup).or_default().push((item_id.clone(), *qty, s.map(|x| x.unit_cost).unwrap_or_default())),
            None => skipped.push(s.map(|x| x.sku.clone()).unwrap_or_else(|| item_id.clone())),
        }
    }
    let mut out = Vec::new();
    for (sup, lines) in by_supplier {
        let po = create_po(conn, &sup)?;
        for (item, qty, cost) in lines {
            add_line(conn, &po.id, &item, qty, Some(&cost.to_string()))?;
        }
        out.push(get_po(conn, &po.id)?);
    }
    Ok((out, skipped))
}

#[derive(Serialize, TS, Clone, Debug)]
#[ts(export)]
pub struct CostPoint {
    pub date: String,
    #[ts(type = "string")]
    pub unit_cost: Decimal,
    pub qty: i64,
    pub reference: String,
}

/// Purchase cost history of an item, from its receipts in the ledger.
pub fn cost_history(conn: &Connection, item_id: &str) -> AppResult<Vec<CostPoint>> {
    let mut st = conn.prepare(
        "SELECT m.created_at, m.unit_cost, m.qty, coalesce(CASE WHEN m.ref_kind = 'po' THEN 'PO ' || o.number END, m.reason)
         FROM inventory_movements m LEFT JOIN purchase_orders o ON o.id = m.ref_id
         WHERE m.item_id = ?1 AND m.kind = 'receive' AND m.unit_cost IS NOT NULL ORDER BY m.created_at DESC LIMIT 200",
    )?;
    let rows = st.query_map([item_id], |r| {
        Ok(CostPoint { date: r.get(0)?, unit_cost: dec(r, "unit_cost")?, qty: r.get(2)?, reference: r.get(3)? })
    })?;
    Ok(rows.collect::<Result<_, _>>()?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{inventory as inv, parts, suppliers};

    fn setup() -> (Connection, String, String) {
        let conn = db::open_memory().unwrap();
        let sup =
            suppliers::create(&conn, &suppliers::SupplierInput { name: "Parts Co".into(), lead_time_days: Some(9), ..Default::default() })
                .unwrap();
        let p = parts::create(&conn, &parts::PartInput { name: "Oil filter".into(), ..Default::default() }, "user").unwrap();
        let i = inv::create(
            &conn,
            &p.id,
            &inv::ItemInput {
                sku: "OF".into(),
                condition: "new".into(),
                unit: "each".into(),
                cost: "4".into(),
                price_override: None,
                pricing_profile_id: None,
                min_stock: 5,
                reorder_qty: 10,
                supplier_id: Some(sup.id.clone()),
                warranty: String::new(),
                notes: String::new(),
            },
        )
        .unwrap();
        (conn, sup.id, i.id)
    }

    #[test]
    fn suggestions_account_for_open_orders_and_usage() {
        let (conn, sup, item) = setup();
        inv::receive(
            &conn,
            &inv::Receive {
                item_id: item.clone(),
                location_id: "loc-main".into(),
                qty: 20,
                unit_cost: None,
                bin: None,
                reason: String::new(),
                ref_kind: None,
                ref_id: None,
            },
        )
        .unwrap();
        inv::issue(
            &conn,
            &inv::Change {
                item_id: item.clone(),
                location_id: "loc-main".into(),
                qty: 18,
                reason: String::new(),
                ref_kind: None,
                ref_id: None,
            },
        )
        .unwrap();
        set_price(
            &conn,
            &SupplierPriceInput {
                supplier_id: sup.clone(),
                item_id: item.clone(),
                supplier_sku: "PC-1".into(),
                cost: Some("3.50".into()),
                lead_time_days: None,
            },
        )
        .unwrap();
        let s = suggestions(&conn).unwrap();
        assert_eq!(s.len(), 1);
        // shortfall 3 + 18 used in 90 days × 9 days lead ÷ 90 = 2 → 5, but at least reorder qty 10.
        assert_eq!(s[0].suggested_qty, 10);
        assert_eq!(s[0].unit_cost.to_string(), "3.50");
        let (pos, skipped) = orders_from_suggestions(&conn, &[(item.clone(), 10)]).unwrap();
        assert!(skipped.is_empty());
        assert!(suggestions(&conn).unwrap().is_empty(), "already on order");
        let po = set_po_status(&conn, &pos[0].id, "ordered").unwrap();
        let po = receive(&conn, &po.id, "loc-main", &[ReceiveLine { line_id: po.lines[0].id.clone(), qty: 4 }]).unwrap();
        assert_eq!(po.status, "partial");
        assert!(
            receive(&conn, &po.id, "loc-main", &[ReceiveLine { line_id: po.lines[0].id.clone(), qty: 7 }]).is_err(),
            "can't receive more than ordered"
        );
        let po = receive(&conn, &po.id, "loc-main", &[ReceiveLine { line_id: po.lines[0].id.clone(), qty: 6 }]).unwrap();
        assert_eq!(po.status, "received");
        assert_eq!(inv::get(&conn, &item).unwrap().on_hand, 12);
        assert_eq!(cost_history(&conn, &item).unwrap()[0].reference, format!("PO {}", po.number));
        assert!(add_line(&conn, &po.id, &item, 1, None).is_err(), "sent orders are locked");
    }

    #[test]
    fn price_list_import() {
        let (conn, sup, item) = setup();
        let t = crate::csvio::Table {
            headers: vec!["SKU".into(), "Net cost".into()],
            rows: vec![vec!["OF".into(), "3.10".into()], vec!["NOPE".into(), "1".into()]],
        };
        let (n, missing) = import_price_list(&conn, &sup, &t).unwrap();
        assert_eq!((n, missing), (1, vec!["NOPE".to_string()]));
        assert_eq!(prices_for_item(&conn, &item).unwrap()[0].cost.unwrap().to_string(), "3.10");
    }
}
