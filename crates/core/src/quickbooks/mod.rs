//! QuickBooks → Auto Manager, one way. Records come from a QuickBooks export file ([`files`]) or
//! from QuickBooks Online, are normalized to [`QbData`], previewed, then applied in one
//! transaction. Each imported record is linked (`external_links`), so importing again updates the
//! same record instead of adding a copy. QuickBooks itself is never changed.

pub mod files;
pub mod online;

use crate::db::{self, clean};
use crate::error::{AppError, AppResult};
use crate::qty::Qty;
use crate::{customers, inventory, parts, suppliers};
use rusqlite::{Connection, OptionalExtension, params};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::str::FromStr;
use ts_rs::TS;

/// One QuickBooks product or service.
#[derive(Serialize, Deserialize, TS, Clone, Debug, Default, PartialEq)]
#[ts(export)]
pub struct QbItem {
    /// QuickBooks' id (Online) or the full item name (exports).
    pub external_id: String,
    pub name: String,
    /// The parent categories ("Brakes" for "Brakes:Pads").
    pub category: String,
    pub sku: String,
    /// "inventory", "non_inventory", "service" or "other" (bundles, charges, discounts…)
    pub kind: String,
    pub description: String,
    pub price: Option<String>,
    pub cost: Option<String>,
    pub qty: Option<Qty>,
    pub reorder_point: Option<Qty>,
    pub vendor: String,
    pub active: bool,
}

#[derive(Serialize, Deserialize, TS, Clone, Debug, Default, PartialEq)]
#[ts(export)]
pub struct QbCustomer {
    pub external_id: String,
    pub name: String,
    pub phone: String,
    pub email: String,
    pub address: String,
    pub notes: String,
    pub active: bool,
}

#[derive(Serialize, Deserialize, TS, Clone, Debug, Default, PartialEq)]
#[ts(export)]
pub struct QbVendor {
    pub external_id: String,
    pub name: String,
    pub contact: String,
    pub phone: String,
    pub email: String,
    pub website: String,
    pub account_no: String,
    pub active: bool,
}

/// Everything read from QuickBooks, plus the rows that couldn't be read.
#[derive(Serialize, Deserialize, TS, Clone, Debug, Default)]
#[ts(export)]
pub struct QbData {
    pub items: Vec<QbItem>,
    pub customers: Vec<QbCustomer>,
    pub vendors: Vec<QbVendor>,
    pub problems: Vec<QbRow>,
}

#[derive(Deserialize, TS, Clone, Debug)]
#[ts(export)]
pub struct QbOptions {
    /// Use QuickBooks' sales price as the item's price (otherwise your pricing rules decide).
    #[serde(default = "yes")]
    pub use_prices: bool,
    /// Also import non-inventory products (no stock is kept for them).
    #[serde(default)]
    pub non_inventory: bool,
    /// On items already here, set stock to the QuickBooks quantity (as a count movement).
    #[serde(default)]
    pub apply_stock: bool,
    /// On items already here, set the cost to QuickBooks' purchase cost.
    #[serde(default)]
    pub apply_cost: bool,
}

fn yes() -> bool {
    true
}

impl Default for QbOptions {
    fn default() -> Self {
        QbOptions { use_prices: true, non_inventory: false, apply_stock: false, apply_cost: false }
    }
}

#[derive(Serialize, Deserialize, TS, Clone, Debug, PartialEq)]
#[ts(export)]
pub struct QbRow {
    /// "item", "customer" or "vendor"
    pub entity: String,
    pub name: String,
    /// "new", "update", "unchanged" or "skipped"
    pub status: String,
    /// What will change ("Price 12.00 → 14.50"), or why the row is skipped.
    pub notes: Vec<String>,
}

#[derive(Serialize, TS, Clone, Debug)]
#[ts(export)]
pub struct QbPreview {
    pub rows: Vec<QbRow>,
    pub new_count: usize,
    pub update_count: usize,
    pub unchanged_count: usize,
    pub skipped_count: usize,
}

/// What the preview decided for one record, and what apply needs to do it.
enum Plan<'a> {
    Skip(String),
    NewItem(&'a QbItem),
    UpdateItem(&'a QbItem, String),
    NewCustomer(&'a QbCustomer),
    UpdateCustomer(&'a QbCustomer, String),
    NewVendor(&'a QbVendor),
    UpdateVendor(&'a QbVendor, String),
}

fn link(conn: &Connection, system: &str, entity: &str, external_id: &str) -> AppResult<Option<String>> {
    Ok(conn
        .query_row(
            "SELECT local_id FROM external_links WHERE system = ?1 AND entity = ?2 AND external_id = ?3",
            [system, entity, external_id],
            |r| r.get(0),
        )
        .optional()?)
}

fn set_link(conn: &Connection, system: &str, entity: &str, external_id: &str, local_id: &str) -> AppResult<()> {
    conn.execute(
        "INSERT INTO external_links (system, entity, external_id, local_id, updated_at) VALUES (?1, ?2, ?3, ?4, ?5)
         ON CONFLICT (system, entity, external_id) DO UPDATE SET local_id = excluded.local_id, updated_at = excluded.updated_at",
        params![system, entity, external_id, local_id, db::now_utc()],
    )?;
    Ok(())
}

fn exists(conn: &Connection, table: &str, id: &str) -> AppResult<bool> {
    db::exists(conn, &format!("SELECT 1 FROM {table} WHERE id = ?1"), [id])
}

/// The local record a QuickBooks record is linked to, if it still exists.
fn linked(conn: &Connection, system: &str, entity: &str, external_id: &str, table: &str) -> AppResult<Option<String>> {
    match link(conn, system, entity, external_id)? {
        Some(id) if exists(conn, table, &id)? => Ok(Some(id)),
        _ => Ok(None),
    }
}

fn by_name(conn: &Connection, table: &str, name: &str) -> AppResult<Option<String>> {
    Ok(conn.query_row(&format!("SELECT id FROM {table} WHERE name = ?1 COLLATE NOCASE"), [name], |r| r.get(0)).optional()?)
}

fn dec(s: &Option<String>) -> Option<Decimal> {
    s.as_deref().and_then(|v| Decimal::from_str(v).ok())
}

/// QuickBooks shows 0 for "no sales price": leave those to the pricing rules.
fn qb_price(i: &QbItem) -> Option<Decimal> {
    dec(&i.price).filter(|p| !p.is_zero())
}

fn item_sku(i: &QbItem) -> String {
    let sku = clean(&i.sku);
    if !sku.is_empty() {
        return sku;
    }
    if i.category.is_empty() { clean(&i.name) } else { clean(&format!("{}:{}", i.category, i.name)) }
}

fn changed(notes: &mut Vec<String>, what: &str, old: &str, new: &str) {
    if old != new {
        let show = |s: &str| if s.is_empty() { "—".to_string() } else { s.to_string() };
        notes.push(format!("{what}: {} → {}", show(old), show(new)));
    }
}

fn qty_text(q: Qty) -> String {
    q.dec().normalize().to_string()
}

fn sellable_on_hand(conn: &Connection, item_id: &str) -> AppResult<Qty> {
    Ok(inventory::get(conn, item_id)?.on_hand)
}

fn plan_item<'a>(
    conn: &Connection,
    system: &str,
    i: &'a QbItem,
    o: &QbOptions,
    seen: &mut std::collections::HashSet<String>,
) -> AppResult<(Plan<'a>, Vec<String>)> {
    let mut notes = Vec::new();
    match i.kind.as_str() {
        "inventory" => {}
        "non_inventory" if o.non_inventory => {}
        "non_inventory" => return Ok((Plan::Skip("Non-inventory product (turn on “Include non-inventory products”)".into()), notes)),
        "service" => return Ok((Plan::Skip("Service: Auto Manager prices labor with your labor rate".into()), notes)),
        _ => return Ok((Plan::Skip("Bundles, charges and discounts aren't stock items".into()), notes)),
    }
    let existing = match linked(conn, system, "item", &i.external_id, "inventory_items")? {
        Some(id) => Some(id),
        None => inventory::by_sku(conn, &item_sku(i))?.map(|it| it.id),
    };
    let Some(id) = existing else {
        if !i.active {
            return Ok((Plan::Skip("Inactive in QuickBooks".into()), notes));
        }
        let sku = item_sku(i);
        if sku.chars().count() > 64 {
            let why = if i.sku.trim().is_empty() {
                "No SKU, and the name is too long to use as one (give it a SKU in QuickBooks)"
            } else {
                "SKU longer than 64 characters"
            };
            return Ok((Plan::Skip(why.into()), notes));
        }
        if !seen.insert(sku.to_lowercase()) {
            return Ok((Plan::Skip(format!("SKU {sku} is used by another QuickBooks item in this import")), notes));
        }
        if i.sku.trim().is_empty() {
            notes.push(format!("No SKU in QuickBooks: “{sku}” is used"));
        }
        match i.qty {
            Some(q) if q < 0 => notes.push(format!("QuickBooks shows {} on hand: added with none", qty_text(q))),
            Some(q) if q > 0 && i.kind == "inventory" => notes.push(format!("Opening stock {}", qty_text(q))),
            _ => {}
        }
        return Ok((Plan::NewItem(i), notes));
    };
    let item = inventory::get(conn, &id)?;
    if !i.active {
        notes.push("Inactive in QuickBooks (kept here)".into());
    }
    changed(&mut notes, "Name", &item.name, &clean(&i.name));
    let part = parts::get(conn, &item.part_id)?;
    if !i.description.trim().is_empty() {
        changed(&mut notes, "Description", part.description.trim(), i.description.trim());
    }
    if o.use_prices
        && let Some(p) = qb_price(i)
        && item.price_override != Some(p)
    {
        changed(&mut notes, "Price", &item.price_override.map(|d| d.to_string()).unwrap_or_default(), &p.to_string());
    }
    if let Some(r) = i.reorder_point
        && r >= 0
        && r != item.min_stock
    {
        changed(&mut notes, "Minimum stock", &qty_text(item.min_stock), &qty_text(r));
    }
    if !i.vendor.trim().is_empty() && !item.supplier_name.as_deref().unwrap_or("").eq_ignore_ascii_case(i.vendor.trim()) {
        changed(&mut notes, "Supplier", item.supplier_name.as_deref().unwrap_or(""), i.vendor.trim());
    }
    if let Some(c) = dec(&i.cost)
        && c != item.cost
    {
        if o.apply_cost {
            changed(&mut notes, "Cost", &item.cost.to_string(), &c.to_string());
        } else {
            notes.push(format!("QuickBooks cost {c}, here {} (not changed)", item.cost));
        }
    }
    if i.kind == "inventory"
        && let Some(q) = i.qty
    {
        let here = sellable_on_hand(conn, &id)?;
        if q != here {
            if o.apply_stock && q >= 0 {
                notes.push(format!("Stock: {} → {}", qty_text(here), qty_text(q)));
            } else {
                notes.push(format!("QuickBooks has {} on hand, here {} (not changed)", qty_text(q), qty_text(here)));
            }
        }
    }
    let real_change = notes.iter().any(|n| n.contains(" → ") && !n.ends_with("(not changed)"));
    Ok((if real_change { Plan::UpdateItem(i, id) } else { Plan::Skip(String::new()) }, notes))
}

fn plan_customer<'a>(conn: &Connection, system: &str, c: &'a QbCustomer) -> AppResult<(Plan<'a>, Vec<String>)> {
    let mut notes = Vec::new();
    let existing = match linked(conn, system, "customer", &c.external_id, "customers")? {
        Some(id) => Some(id),
        None => by_name(conn, "customers", &clean(&c.name))?,
    };
    let Some(id) = existing else {
        return Ok(if c.active { (Plan::NewCustomer(c), notes) } else { (Plan::Skip("Inactive in QuickBooks".into()), notes) });
    };
    let here = customers::get(conn, &id)?;
    changed(&mut notes, "Name", &here.name, &clean(&c.name));
    for (what, old, new) in [("Phone", &here.phone, &c.phone), ("Email", &here.email, &c.email), ("Address", &here.address, &c.address)] {
        if !new.trim().is_empty() {
            changed(&mut notes, what, old.trim(), new.trim());
        }
    }
    Ok((if notes.is_empty() { Plan::Skip(String::new()) } else { Plan::UpdateCustomer(c, id) }, notes))
}

fn plan_vendor<'a>(conn: &Connection, system: &str, v: &'a QbVendor) -> AppResult<(Plan<'a>, Vec<String>)> {
    let mut notes = Vec::new();
    let existing = match linked(conn, system, "vendor", &v.external_id, "suppliers")? {
        Some(id) => Some(id),
        None => by_name(conn, "suppliers", &clean(&v.name))?,
    };
    let Some(id) = existing else {
        return Ok(if v.active { (Plan::NewVendor(v), notes) } else { (Plan::Skip("Inactive in QuickBooks".into()), notes) });
    };
    let here = suppliers::get(conn, &id)?;
    changed(&mut notes, "Name", &here.name, &clean(&v.name));
    for (what, old, new) in [
        ("Contact", &here.contact, &v.contact),
        ("Phone", &here.phone, &v.phone),
        ("Email", &here.email, &v.email),
        ("Website", &here.website, &v.website),
        ("Account number", &here.account_no, &v.account_no),
    ] {
        if !new.trim().is_empty() {
            changed(&mut notes, what, old.trim(), new.trim());
        }
    }
    Ok((if notes.is_empty() { Plan::Skip(String::new()) } else { Plan::UpdateVendor(v, id) }, notes))
}

/// Vendors first (items name their vendor), then customers, then items.
fn plan_all<'a>(conn: &Connection, system: &str, d: &'a QbData, o: &QbOptions) -> AppResult<Vec<(QbRow, Plan<'a>)>> {
    let mut out = Vec::new();
    let mut push = |entity: &str, name: &str, (plan, notes): (Plan<'a>, Vec<String>)| {
        let (status, notes) = match &plan {
            Plan::Skip(why) if why.is_empty() => ("unchanged", notes),
            Plan::Skip(why) => ("skipped", vec![why.clone()]),
            Plan::NewItem(_) | Plan::NewCustomer(_) | Plan::NewVendor(_) => ("new", notes),
            _ => ("update", notes),
        };
        out.push((QbRow { entity: entity.into(), name: name.into(), status: status.into(), notes }, plan));
    };
    for v in &d.vendors {
        push("vendor", &v.name, plan_vendor(conn, system, v)?);
    }
    for c in &d.customers {
        push("customer", &c.name, plan_customer(conn, system, c)?);
    }
    let mut seen = std::collections::HashSet::new();
    for i in &d.items {
        push("item", &i.name, plan_item(conn, system, i, o, &mut seen)?);
    }
    Ok(out)
}

fn count(rows: Vec<QbRow>) -> QbPreview {
    let n = |s: &str| rows.iter().filter(|r| r.status == s).count();
    QbPreview { new_count: n("new"), update_count: n("update"), unchanged_count: n("unchanged"), skipped_count: n("skipped"), rows }
}

/// What importing `data` would do. Nothing is written.
pub fn preview(conn: &Connection, system: &str, data: &QbData, o: &QbOptions) -> AppResult<QbPreview> {
    let mut rows: Vec<QbRow> = plan_all(conn, system, data, o)?.into_iter().map(|(r, _)| r).collect();
    rows.extend(data.problems.iter().cloned());
    Ok(count(rows))
}

#[derive(Serialize, TS, Clone, Debug)]
#[ts(export)]
pub struct QbSummary {
    pub added: usize,
    pub updated: usize,
    pub unchanged: usize,
    pub skipped: usize,
}

struct Suppliers(HashMap<String, String>);

impl Suppliers {
    /// The supplier named `name`, created if it isn't there yet.
    fn id(&mut self, conn: &Connection, name: &str) -> AppResult<Option<String>> {
        let name = clean(name);
        if name.is_empty() {
            return Ok(None);
        }
        if let Some(id) = self.0.get(&name.to_lowercase()) {
            return Ok(Some(id.clone()));
        }
        let id = match by_name(conn, "suppliers", &name)? {
            Some(id) => id,
            None => suppliers::create(conn, &suppliers::SupplierInput { name: name.clone(), ..Default::default() })?.id,
        };
        self.0.insert(name.to_lowercase(), id.clone());
        Ok(Some(id))
    }
}

fn category_id(conn: &Connection, category: &str) -> AppResult<Option<String>> {
    let last = category.rsplit(':').next().unwrap_or("").trim();
    if last.is_empty() {
        return Ok(None);
    }
    by_name(conn, "part_categories", last)
}

fn fractional(i: &QbItem) -> bool {
    [i.qty, i.reorder_point].iter().flatten().any(|q| !q.is_whole())
}

fn new_item(conn: &Connection, i: &QbItem, o: &QbOptions, sup: &mut Suppliers) -> AppResult<String> {
    let part = parts::create(
        conn,
        &parts::PartInput {
            name: clean(&i.name),
            category_id: category_id(conn, &i.category)?,
            description: i.description.trim().to_string(),
            ..Default::default()
        },
        "user",
    )?;
    let cost = dec(&i.cost).filter(|c| !c.is_sign_negative());
    let item = inventory::create(
        conn,
        &part.id,
        &inventory::ItemInput {
            sku: item_sku(i),
            condition: "new".into(),
            unit: "each".into(),
            fractional: fractional(i),
            cost: cost.map(|c| c.to_string()).unwrap_or_else(|| "0".into()),
            price_override: if o.use_prices { qb_price(i).map(|p| p.to_string()) } else { None },
            pricing_profile_id: None,
            min_stock: i.reorder_point.filter(|r| *r >= 0).unwrap_or_default(),
            reorder_qty: Qty::ZERO,
            supplier_id: sup.id(conn, &i.vendor)?,
            warranty: String::new(),
            notes: String::new(),
        },
    )?;
    if i.kind == "inventory"
        && let Some(q) = i.qty.filter(|q| *q > 0)
    {
        inventory::receive(
            conn,
            &inventory::Receive {
                item_id: item.id.clone(),
                location_id: inventory::default_location(conn)?,
                qty: q,
                unit_cost: cost.map(|c| c.to_string()),
                bin: None,
                reason: "Opening stock (QuickBooks)".into(),
                ref_kind: Some("import".into()),
                ref_id: None,
            },
        )?;
    }
    Ok(item.id)
}

fn update_item(conn: &Connection, id: &str, i: &QbItem, o: &QbOptions, sup: &mut Suppliers) -> AppResult<()> {
    let item = inventory::get(conn, id)?;
    let description =
        if i.description.trim().is_empty() { parts::get(conn, &item.part_id)?.description } else { i.description.trim().into() };
    conn.execute(
        "UPDATE parts SET name = ?2, description = ?3, category_id = coalesce(category_id, ?4), updated_at = ?5 WHERE id = ?1",
        params![item.part_id, clean(&i.name), description, category_id(conn, &i.category)?, db::now_utc()],
    )?;
    let supplier_id = if i.vendor.trim().is_empty() { item.supplier_id.clone() } else { sup.id(conn, &i.vendor)? };
    let cost = match dec(&i.cost) {
        Some(c) if o.apply_cost && !c.is_sign_negative() => c,
        _ => item.cost,
    };
    let price = match qb_price(i) {
        Some(p) if o.use_prices => Some(p),
        _ => item.price_override,
    };
    inventory::update(
        conn,
        id,
        item.version,
        &inventory::ItemInput {
            sku: item.sku.clone(),
            condition: item.condition.clone(),
            unit: item.unit.clone(),
            fractional: item.fractional || fractional(i),
            cost: cost.to_string(),
            price_override: price.map(|p| p.to_string()),
            pricing_profile_id: item.pricing_profile_id.clone(),
            min_stock: i.reorder_point.filter(|r| *r >= 0).unwrap_or(item.min_stock),
            reorder_qty: item.reorder_qty,
            supplier_id,
            warranty: item.warranty.clone(),
            notes: item.notes.clone(),
        },
    )?;
    if o.apply_stock
        && i.kind == "inventory"
        && let Some(q) = i.qty.filter(|q| *q >= 0)
    {
        let here = sellable_on_hand(conn, id)?;
        if q != here {
            // The difference goes to the default location; other locations keep what they hold.
            let loc = inventory::default_location(conn)?;
            let at_default: Qty = conn
                .query_row("SELECT on_hand FROM stock_levels WHERE item_id = ?1 AND location_id = ?2", [id, &loc], |r| r.get(0))
                .optional()?
                .unwrap_or_default();
            let target = Qty::from_milli(at_default.milli() + q.milli() - here.milli());
            if target < 0 {
                return Err(AppError::Conflict(format!(
                    "{}: QuickBooks has {} on hand, but more than that is held at your other locations. Count it by hand.",
                    item.sku,
                    qty_text(q)
                )));
            }
            inventory::adjust_to(conn, id, &loc, target, "count", "Set to the QuickBooks quantity", None)?;
        }
    }
    Ok(())
}

fn customer_input(c: &QbCustomer, here: Option<&customers::Customer>) -> customers::CustomerInput {
    let pick =
        |new: &str, old: Option<&String>| if new.trim().is_empty() { old.cloned().unwrap_or_default() } else { new.trim().to_string() };
    customers::CustomerInput {
        name: clean(&c.name),
        phone: pick(&c.phone, here.map(|h| &h.phone)),
        email: pick(&c.email, here.map(|h| &h.email)),
        address: pick(&c.address, here.map(|h| &h.address)),
        notes: pick(&c.notes, here.map(|h| &h.notes)),
        pricing_profile_id: here.and_then(|h| h.pricing_profile_id.clone()),
        discount_pct: here.and_then(|h| h.discount_pct.clone()),
    }
}

fn vendor_input(v: &QbVendor, here: Option<&suppliers::Supplier>) -> suppliers::SupplierInput {
    let pick =
        |new: &str, old: Option<&String>| if new.trim().is_empty() { old.cloned().unwrap_or_default() } else { new.trim().to_string() };
    suppliers::SupplierInput {
        name: clean(&v.name),
        contact: pick(&v.contact, here.map(|h| &h.contact)),
        phone: pick(&v.phone, here.map(|h| &h.phone)),
        email: pick(&v.email, here.map(|h| &h.email)),
        website: pick(&v.website, here.map(|h| &h.website)),
        account_no: pick(&v.account_no, here.map(|h| &h.account_no)),
        lead_time_days: here.and_then(|h| h.lead_time_days),
        preferred: here.is_some_and(|h| h.preferred),
        notes: here.map(|h| h.notes.clone()).unwrap_or_default(),
    }
}

/// Import `data`: everything the preview shows as new or update. Run inside one transaction.
pub fn apply(conn: &Connection, system: &str, data: &QbData, o: &QbOptions) -> AppResult<QbSummary> {
    let plans = plan_all(conn, system, data, o)?;
    let mut sup = Suppliers(HashMap::new());
    let (mut added, mut updated, mut unchanged, mut skipped) = (0, 0, 0, data.problems.len());
    for (row, plan) in &plans {
        match plan {
            Plan::Skip(why) if why.is_empty() => unchanged += 1,
            Plan::Skip(_) => skipped += 1,
            Plan::NewVendor(v) => {
                let id = suppliers::create(conn, &vendor_input(v, None))?.id;
                sup.0.insert(clean(&v.name).to_lowercase(), id.clone());
                set_link(conn, system, "vendor", &v.external_id, &id)?;
                added += 1;
            }
            Plan::UpdateVendor(v, id) => {
                suppliers::update(conn, id, &vendor_input(v, Some(&suppliers::get(conn, id)?)))?;
                set_link(conn, system, "vendor", &v.external_id, id)?;
                updated += 1;
            }
            Plan::NewCustomer(c) => {
                let id = customers::create(conn, &customer_input(c, None))?.id;
                set_link(conn, system, "customer", &c.external_id, &id)?;
                added += 1;
            }
            Plan::UpdateCustomer(c, id) => {
                customers::update(conn, id, &customer_input(c, Some(&customers::get(conn, id)?)))?;
                set_link(conn, system, "customer", &c.external_id, id)?;
                updated += 1;
            }
            Plan::NewItem(i) => {
                let id = new_item(conn, i, o, &mut sup).map_err(|e| AppError::msg(format!("{}: {}", row.name, e)))?;
                set_link(conn, system, "item", &i.external_id, &id)?;
                added += 1;
            }
            Plan::UpdateItem(i, id) => {
                update_item(conn, id, i, o, &mut sup).map_err(|e| AppError::msg(format!("{}: {}", row.name, e)))?;
                set_link(conn, system, "item", &i.external_id, id)?;
                updated += 1;
            }
        }
    }
    // Records that matched but needed nothing are still linked, so the next import finds them.
    link_unchanged(conn, system, data)?;
    db::audit(
        conn,
        "import",
        None,
        "quickbooks",
        &format!("QuickBooks: {added} added, {updated} updated, {unchanged} unchanged, {skipped} skipped"),
        None,
    )?;
    Ok(QbSummary { added, updated, unchanged, skipped })
}

/// Link records that were found by SKU or name and needed no change.
fn link_unchanged(conn: &Connection, system: &str, data: &QbData) -> AppResult<()> {
    for i in data.items.iter().filter(|i| matches!(i.kind.as_str(), "inventory" | "non_inventory")) {
        if link(conn, system, "item", &i.external_id)?.is_none()
            && let Some(it) = inventory::by_sku(conn, &item_sku(i))?
        {
            set_link(conn, system, "item", &i.external_id, &it.id)?;
        }
    }
    for c in &data.customers {
        if link(conn, system, "customer", &c.external_id)?.is_none()
            && let Some(id) = by_name(conn, "customers", &clean(&c.name))?
        {
            set_link(conn, system, "customer", &c.external_id, &id)?;
        }
    }
    for v in &data.vendors {
        if link(conn, system, "vendor", &v.external_id)?.is_none()
            && let Some(id) = by_name(conn, "suppliers", &clean(&v.name))?
        {
            set_link(conn, system, "vendor", &v.external_id, &id)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests;
