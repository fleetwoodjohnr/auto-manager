//! Inventory: stocked items, quantities per location, and the movement ledger.
//!
//! Every quantity change writes an `inventory_movements` row and updates `stock_levels` in the
//! same transaction (callers run these inside `Core::tx`). The table CHECKs make negative or
//! over-reserved stock impossible even if this code had a bug: such an update fails and the
//! whole transaction rolls back.

use crate::db::{self, clean, dec};
use crate::error::{AppError, AppResult};
use crate::money;
use crate::qty::{self, Qty};
use rusqlite::{Connection, OptionalExtension, params};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

pub const DAMAGED: &str = "loc-damaged";

#[derive(Serialize, TS, Clone, Debug)]
#[ts(export)]
pub struct Item {
    pub id: String,
    pub part_id: String,
    pub sku: String,
    pub name: String,
    pub brand: String,
    pub category_id: Option<String>,
    pub category: String,
    pub numbers: String,
    pub barcode: Option<String>,
    pub condition: String,
    pub unit: String,
    /// Sold in parts of a unit (oil by the litre); otherwise whole units only.
    pub fractional: bool,
    #[ts(type = "string")]
    pub cost: Decimal,
    #[ts(type = "string")]
    pub avg_cost: Decimal,
    #[ts(type = "string | null")]
    pub price_override: Option<Decimal>,
    pub pricing_profile_id: Option<String>,
    pub min_stock: Qty,
    pub reorder_qty: Qty,
    pub supplier_id: Option<String>,
    pub supplier_name: Option<String>,
    pub warranty: String,
    pub notes: String,
    pub archived: bool,
    pub version: i64,
    /// Sellable locations only (the damaged/quarantine location is counted separately).
    pub on_hand: Qty,
    pub reserved: Qty,
    pub available: Qty,
    pub damaged: Qty,
    /// "Main: A-3" style summary of where it is.
    pub bins: String,
    pub created_at: String,
    pub updated_at: String,
}

impl Item {
    pub fn stock_state(&self) -> &'static str {
        if self.available <= 0 {
            "out"
        } else if self.min_stock > 0 && self.available <= self.min_stock {
            "low"
        } else {
            "ok"
        }
    }
}

const ITEM_SELECT: &str = "SELECT i.id, i.part_id, i.sku, p.name, p.brand, p.category_id, coalesce(c.name, '') AS category,
  coalesce((SELECT group_concat(value, ' · ') FROM part_identifiers WHERE part_id = p.id AND kind <> 'barcode'), '') AS numbers,
  (SELECT value FROM part_identifiers WHERE part_id = p.id AND kind = 'barcode' LIMIT 1) AS barcode,
  i.condition, i.unit, i.fractional, i.cost, i.avg_cost, i.price_override, i.pricing_profile_id, i.min_stock, i.reorder_qty, i.supplier_id, s.name AS supplier_name,
  i.warranty, i.notes, i.archived, i.version,
  coalesce((SELECT sum(on_hand) FROM stock_levels sl JOIN inventory_locations l ON l.id = sl.location_id WHERE sl.item_id = i.id AND l.kind = 'warehouse'), 0) AS on_hand,
  coalesce((SELECT sum(reserved) FROM stock_levels sl JOIN inventory_locations l ON l.id = sl.location_id WHERE sl.item_id = i.id AND l.kind = 'warehouse'), 0) AS reserved,
  coalesce((SELECT sum(on_hand) FROM stock_levels sl JOIN inventory_locations l ON l.id = sl.location_id WHERE sl.item_id = i.id AND l.kind = 'damaged'), 0) AS damaged,
  coalesce((SELECT group_concat(l.name || CASE WHEN sl.bin <> '' THEN ': ' || sl.bin ELSE '' END, ', ') FROM stock_levels sl JOIN inventory_locations l ON l.id = sl.location_id
            WHERE sl.item_id = i.id AND sl.on_hand > 0), '') AS bins,
  i.created_at, i.updated_at
FROM inventory_items i JOIN parts p ON p.id = i.part_id LEFT JOIN part_categories c ON c.id = p.category_id LEFT JOIN suppliers s ON s.id = i.supplier_id";

fn item_row(r: &rusqlite::Row) -> rusqlite::Result<Item> {
    let on_hand: Qty = r.get("on_hand")?;
    let reserved: Qty = r.get("reserved")?;
    Ok(Item {
        id: r.get("id")?,
        part_id: r.get("part_id")?,
        sku: r.get("sku")?,
        name: r.get("name")?,
        brand: r.get("brand")?,
        category_id: r.get("category_id")?,
        category: r.get("category")?,
        numbers: r.get("numbers")?,
        barcode: r.get("barcode")?,
        condition: r.get("condition")?,
        unit: r.get("unit")?,
        fractional: r.get::<_, i64>("fractional")? == 1,
        cost: dec(r, "cost")?,
        avg_cost: dec(r, "avg_cost")?,
        price_override: db::opt_dec(r, "price_override")?,
        pricing_profile_id: r.get("pricing_profile_id")?,
        min_stock: r.get("min_stock")?,
        reorder_qty: r.get("reorder_qty")?,
        supplier_id: r.get("supplier_id")?,
        supplier_name: r.get("supplier_name")?,
        warranty: r.get("warranty")?,
        notes: r.get("notes")?,
        archived: r.get::<_, i64>("archived")? == 1,
        version: r.get("version")?,
        on_hand,
        reserved,
        available: on_hand - reserved,
        damaged: r.get("damaged")?,
        bins: r.get("bins")?,
        created_at: r.get("created_at")?,
        updated_at: r.get("updated_at")?,
    })
}

pub fn get(conn: &Connection, id: &str) -> AppResult<Item> {
    conn.query_row(&format!("{ITEM_SELECT} WHERE i.id = ?1"), [id], item_row)
        .optional()?
        .ok_or_else(|| AppError::not_found("That inventory item"))
}

pub fn by_sku(conn: &Connection, sku: &str) -> AppResult<Option<Item>> {
    Ok(conn.query_row(&format!("{ITEM_SELECT} WHERE i.sku = ?1 COLLATE NOCASE"), [sku.trim()], item_row).optional()?)
}

pub fn for_part(conn: &Connection, part_id: &str) -> AppResult<Vec<Item>> {
    let mut st = conn.prepare(&format!("{ITEM_SELECT} WHERE i.part_id = ?1 AND i.archived = 0 ORDER BY i.sku"))?;
    let rows = st.query_map([part_id], item_row)?;
    Ok(rows.collect::<Result<_, _>>()?)
}

#[derive(Deserialize, TS, Clone, Debug, Default)]
#[ts(export)]
pub struct ItemQuery {
    pub q: Option<String>,
    pub category_id: Option<String>,
    pub location_id: Option<String>,
    /// "low", "out", or empty for all.
    pub stock: Option<String>,
    #[serde(default)]
    pub include_archived: bool,
}

pub fn list(conn: &Connection, q: &ItemQuery) -> AppResult<Vec<Item>> {
    let text = q.q.as_deref().map(str::trim).filter(|t| !t.is_empty());
    let like = text.map(|t| format!("%{}%", t.replace('%', "\\%").replace('_', "\\_")));
    let num = text.map(crate::parts::norm).filter(|n| !n.is_empty()).unwrap_or_else(|| "\u{1}".into());
    let sql = format!(
        "WITH RECURSIVE cats(id) AS (SELECT ?1 UNION ALL SELECT c.id FROM part_categories c JOIN cats ON c.parent_id = cats.id)
         {ITEM_SELECT}
         WHERE (?1 IS NULL OR p.category_id IN (SELECT id FROM cats))
           AND (?2 OR i.archived = 0)
           AND (?3 IS NULL OR i.id IN (SELECT item_id FROM stock_levels WHERE location_id = ?3 AND on_hand > 0))
           AND (?4 IS NULL OR i.sku LIKE ?4 ESCAPE '\\' OR p.name LIKE ?4 ESCAPE '\\' OR p.brand LIKE ?4 ESCAPE '\\' OR i.notes LIKE ?4 ESCAPE '\\'
                OR EXISTS (SELECT 1 FROM part_identifiers x WHERE x.part_id = p.id AND x.value_norm LIKE '%' || ?5 || '%'))
         ORDER BY p.name COLLATE NOCASE, i.sku"
    );
    let mut st = conn.prepare(&sql)?;
    let rows = st.query_map(params![q.category_id, q.include_archived, q.location_id, like, num], item_row)?;
    let mut items: Vec<Item> = rows.collect::<Result<_, _>>()?;
    match q.stock.as_deref() {
        Some("low") => items.retain(|i| i.stock_state() == "low" || (i.stock_state() == "out" && i.min_stock > 0)),
        Some("out") => items.retain(|i| i.stock_state() == "out"),
        _ => {}
    }
    Ok(items)
}

#[derive(Serialize, TS, Clone, Debug)]
#[ts(export)]
pub struct StockLevel {
    pub location_id: String,
    pub location: String,
    pub kind: String,
    pub bin: String,
    pub on_hand: Qty,
    pub reserved: Qty,
    pub available: Qty,
}

pub fn levels(conn: &Connection, item_id: &str) -> AppResult<Vec<StockLevel>> {
    let mut st = conn.prepare(
        "SELECT l.id, l.name, l.kind, s.bin, s.on_hand, s.reserved FROM stock_levels s JOIN inventory_locations l ON l.id = s.location_id
         WHERE s.item_id = ?1 ORDER BY l.is_default DESC, l.name",
    )?;
    let rows = st.query_map([item_id], |r| {
        let on_hand: Qty = r.get(4)?;
        let reserved: Qty = r.get(5)?;
        Ok(StockLevel {
            location_id: r.get(0)?,
            location: r.get(1)?,
            kind: r.get(2)?,
            bin: r.get(3)?,
            on_hand,
            reserved,
            available: on_hand - reserved,
        })
    })?;
    Ok(rows.collect::<Result<_, _>>()?)
}

/// On-hand quantity of every item at one location (for counting sheets).
pub fn on_hand_at(conn: &Connection, location_id: &str) -> AppResult<Vec<(String, Qty)>> {
    let mut st = conn.prepare("SELECT item_id, on_hand FROM stock_levels WHERE location_id = ?1")?;
    let rows = st.query_map([location_id], |r| Ok((r.get(0)?, r.get(1)?)))?;
    Ok(rows.collect::<Result<_, _>>()?)
}

#[derive(Deserialize, TS, Clone, Debug)]
#[ts(export)]
pub struct ItemInput {
    pub sku: String,
    #[serde(default = "new_condition")]
    pub condition: String,
    #[serde(default = "each")]
    pub unit: String,
    /// Purchase cost per unit.
    pub cost: String,
    pub price_override: Option<String>,
    pub pricing_profile_id: Option<String>,
    #[serde(default)]
    pub fractional: bool,
    #[serde(default)]
    pub min_stock: Qty,
    #[serde(default)]
    pub reorder_qty: Qty,
    pub supplier_id: Option<String>,
    #[serde(default)]
    pub warranty: String,
    #[serde(default)]
    pub notes: String,
}

fn new_condition() -> String {
    "new".into()
}
fn each() -> String {
    "each".into()
}

struct CleanItem {
    sku: String,
    cost: Decimal,
    price_override: Option<Decimal>,
}

fn validate(conn: &Connection, i: &ItemInput, self_id: Option<&str>) -> AppResult<CleanItem> {
    let sku = clean(&i.sku);
    if sku.is_empty() {
        return Err(AppError::invalid("sku", "Enter a SKU (your stock code for this item)."));
    }
    if sku.len() > 64 {
        return Err(AppError::invalid("sku", "A SKU can be at most 64 characters."));
    }
    if let Some(other) = by_sku(conn, &sku)?
        && Some(other.id.as_str()) != self_id
    {
        return Err(AppError::invalid("sku", format!("SKU {sku} is already used by {}.", other.name)));
    }
    if !matches!(i.condition.as_str(), "new" | "used" | "refurbished") {
        return Err(AppError::invalid("condition", "Choose new, used or refurbished."));
    }
    let cost = money::non_negative("cost", "Cost", money::parse("cost", "Cost", &i.cost)?)?;
    let price_override = i
        .price_override
        .as_deref()
        .map(str::trim)
        .filter(|p| !p.is_empty())
        .map(|p| money::parse("price_override", "Selling price", p).and_then(|d| money::non_negative("price_override", "Selling price", d)))
        .transpose()?;
    if i.min_stock < 0 || i.reorder_qty < 0 {
        return Err(AppError::invalid("min_stock", "Stock levels can't be negative."));
    }
    if !i.fractional && !(i.min_stock.is_whole() && i.reorder_qty.is_whole()) {
        return Err(AppError::invalid("min_stock", "Use whole numbers, or allow parts of a unit for this item."));
    }
    if let Some(pp) = &i.pricing_profile_id
        && !db::exists(conn, "SELECT 1 FROM pricing_profiles WHERE id = ?1", [pp])?
    {
        return Err(AppError::invalid("pricing_profile_id", "Choose a pricing profile from the list."));
    }
    Ok(CleanItem { sku, cost, price_override })
}

pub fn create(conn: &Connection, part_id: &str, i: &ItemInput) -> AppResult<Item> {
    if !db::exists(conn, "SELECT 1 FROM parts WHERE id = ?1", [part_id])? {
        return Err(AppError::not_found("That part"));
    }
    let c = validate(conn, i, None)?;
    let id = db::new_id();
    conn.execute(
        "INSERT INTO inventory_items (id, part_id, sku, condition, unit, cost, avg_cost, price_override, pricing_profile_id, min_stock, reorder_qty, supplier_id, warranty, notes, fractional)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
        params![
            id,
            part_id,
            c.sku,
            i.condition,
            clean(&i.unit),
            c.cost.to_string(),
            c.price_override.map(|p| p.to_string()),
            i.pricing_profile_id,
            i.min_stock,
            i.reorder_qty,
            i.supplier_id,
            i.warranty.trim(),
            i.notes.trim(),
            i.fractional
        ],
    )?;
    db::audit(conn, "item", Some(&id), "create", &format!("Added inventory item {}", c.sku), None)?;
    get(conn, &id)
}

/// Save an edit made in a form opened at `version`. Fails if someone saved in between.
pub fn update(conn: &Connection, id: &str, version: i64, i: &ItemInput) -> AppResult<Item> {
    let c = validate(conn, i, Some(id))?;
    let n = conn.execute(
        "UPDATE inventory_items SET sku = ?3, condition = ?4, unit = ?5, cost = ?6, price_override = ?7, pricing_profile_id = ?8, min_stock = ?9,
           reorder_qty = ?10, supplier_id = ?11, warranty = ?12, notes = ?13, version = version + 1, updated_at = ?14, fractional = ?15
         WHERE id = ?1 AND version = ?2",
        params![
            id,
            version,
            c.sku,
            i.condition,
            clean(&i.unit),
            c.cost.to_string(),
            c.price_override.map(|p| p.to_string()),
            i.pricing_profile_id,
            i.min_stock,
            i.reorder_qty,
            i.supplier_id,
            i.warranty.trim(),
            i.notes.trim(),
            db::now_utc(),
            i.fractional
        ],
    )?;
    if n == 0 {
        get(conn, id)?;
        return Err(AppError::Conflict(
            "This item was changed somewhere else since you opened it. Reopen it to see the latest, then make your change again.".into(),
        ));
    }
    db::audit(conn, "item", Some(id), "update", &format!("Edited inventory item {}", c.sku), None)?;
    get(conn, id)
}

pub fn set_archived(conn: &Connection, id: &str, archived: bool) -> AppResult<Item> {
    let item = get(conn, id)?;
    if archived && (item.on_hand > 0 || item.reserved > 0) {
        return Err(AppError::Conflict("Issue, transfer or adjust the remaining stock to zero before archiving.".into()));
    }
    conn.execute(
        "UPDATE inventory_items SET archived = ?2, version = version + 1, updated_at = ?3 WHERE id = ?1",
        params![id, archived, db::now_utc()],
    )?;
    get(conn, id)
}

/// Delete an item that has never moved; otherwise archive it to keep history.
pub fn delete(conn: &Connection, id: &str) -> AppResult<()> {
    let item = get(conn, id)?;
    if db::exists(conn, "SELECT 1 FROM inventory_movements WHERE item_id = ?1", [id])? {
        return Err(AppError::Conflict(format!("{} has stock history. Archive it instead so the history is kept.", item.sku)));
    }
    conn.execute("DELETE FROM inventory_items WHERE id = ?1", [id])?;
    db::audit(conn, "item", Some(id), "delete", &format!("Deleted inventory item {}", item.sku), None)?;
    Ok(())
}

// ---------------------------------------------------------------- locations

#[derive(Serialize, TS, Clone, Debug)]
#[ts(export)]
pub struct Location {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub is_default: bool,
    pub archived: bool,
    pub item_count: i64,
}

pub fn locations(conn: &Connection) -> AppResult<Vec<Location>> {
    let mut st = conn.prepare(
        "SELECT l.id, l.name, l.kind, l.is_default, l.archived, (SELECT count(*) FROM stock_levels s WHERE s.location_id = l.id AND s.on_hand > 0)
         FROM inventory_locations l ORDER BY l.kind = 'damaged', l.is_default DESC, l.name",
    )?;
    let rows = st.query_map([], |r| {
        Ok(Location {
            id: r.get(0)?,
            name: r.get(1)?,
            kind: r.get(2)?,
            is_default: r.get::<_, i64>(3)? == 1,
            archived: r.get::<_, i64>(4)? == 1,
            item_count: r.get(5)?,
        })
    })?;
    Ok(rows.collect::<Result<_, _>>()?)
}

pub fn default_location(conn: &Connection) -> AppResult<String> {
    Ok(conn.query_row("SELECT id FROM inventory_locations WHERE is_default = 1", [], |r| r.get(0))?)
}

pub fn add_location(conn: &Connection, name: &str) -> AppResult<Location> {
    let name = clean(name);
    if name.is_empty() {
        return Err(AppError::invalid("name", "Enter a location name."));
    }
    let id = db::new_id();
    conn.execute("INSERT INTO inventory_locations (id, name) VALUES (?1, ?2)", params![id, name]).map_err(|e| {
        if e.to_string().contains("UNIQUE") { AppError::invalid("name", "A location with that name already exists.") } else { e.into() }
    })?;
    locations(conn)?.into_iter().find(|l| l.id == id).ok_or_else(|| AppError::not_found("That location"))
}

pub fn set_default_location(conn: &Connection, id: &str) -> AppResult<()> {
    let kind: Option<String> = conn.query_row("SELECT kind FROM inventory_locations WHERE id = ?1", [id], |r| r.get(0)).optional()?;
    match kind.as_deref() {
        None => return Err(AppError::not_found("That location")),
        Some("damaged") => return Err(AppError::msg("The damaged-stock location can't be the default.")),
        _ => {}
    }
    conn.execute("UPDATE inventory_locations SET is_default = 0 WHERE is_default = 1", [])?;
    conn.execute("UPDATE inventory_locations SET is_default = 1 WHERE id = ?1", [id])?;
    Ok(())
}

pub fn rename_location(conn: &Connection, id: &str, name: &str) -> AppResult<()> {
    let name = clean(name);
    if name.is_empty() {
        return Err(AppError::invalid("name", "Enter a location name."));
    }
    let n = conn.execute("UPDATE inventory_locations SET name = ?2 WHERE id = ?1", params![id, name])?;
    db::expect_one(n, "That location")
}

// ---------------------------------------------------------------- movements

fn location_kind(conn: &Connection, id: &str) -> AppResult<String> {
    conn.query_row("SELECT kind FROM inventory_locations WHERE id = ?1 AND archived = 0", [id], |r| r.get(0))
        .optional()?
        .ok_or_else(|| AppError::invalid("location_id", "Choose a location from the list."))
}

fn level(conn: &Connection, item: &str, loc: &str) -> AppResult<(Qty, Qty)> {
    Ok(conn
        .query_row("SELECT on_hand, reserved FROM stock_levels WHERE item_id = ?1 AND location_id = ?2", params![item, loc], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })
        .optional()?
        .unwrap_or((Qty::ZERO, Qty::ZERO)))
}

fn ensure_level(conn: &Connection, item: &str, loc: &str) -> AppResult<()> {
    conn.execute("INSERT OR IGNORE INTO stock_levels (item_id, location_id) VALUES (?1, ?2)", params![item, loc])?;
    Ok(())
}

fn bump(conn: &Connection, item: &str, loc: &str, on_hand: Qty, reserved: Qty) -> AppResult<()> {
    ensure_level(conn, item, loc)?;
    conn.execute(
        "UPDATE stock_levels SET on_hand = on_hand + ?3, reserved = reserved + ?4 WHERE item_id = ?1 AND location_id = ?2",
        params![item, loc, on_hand, reserved],
    )
    .map_err(AppError::from_stock)?;
    conn.execute("UPDATE inventory_items SET updated_at = ?2 WHERE id = ?1", params![item, db::now_utc()])?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn record(
    conn: &Connection,
    item: &str,
    loc: &str,
    kind: &str,
    qty: Qty,
    unit_cost: Option<Decimal>,
    group: Option<&str>,
    reference: (&str, &str),
    reason: &str,
    reverses: Option<&str>,
) -> AppResult<String> {
    let id = db::new_id();
    conn.execute(
        "INSERT INTO inventory_movements (id, item_id, location_id, kind, qty, unit_cost, group_id, ref_kind, ref_id, reason, reverses_id, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, nullif(?8, ''), nullif(?9, ''), ?10, ?11, ?12)",
        params![id, item, loc, kind, qty, unit_cost.map(|c| c.to_string()), group, reference.0, reference.1, reason.trim(), reverses, db::now_utc()],
    )?;
    Ok(id)
}

fn positive(qty: Qty) -> AppResult<Qty> {
    if qty <= 0 {
        return Err(AppError::invalid("qty", "Enter a quantity greater than 0."));
    }
    if qty > qty::MAX_UNITS {
        return Err(AppError::invalid("qty", "That quantity is too large."));
    }
    Ok(qty)
}

/// Items not sold in parts of a unit take whole quantities only.
pub fn check_whole(conn: &Connection, item: &str, qty: Qty) -> AppResult<()> {
    if !qty.is_whole() {
        let (fractional, unit): (i64, String) = conn
            .query_row("SELECT fractional, unit FROM inventory_items WHERE id = ?1", [item], |r| Ok((r.get(0)?, r.get(1)?)))
            .optional()?
            .ok_or_else(|| AppError::not_found("That inventory item"))?;
        if fractional == 0 {
            return Err(AppError::invalid(
                "qty",
                format!("This item is counted in whole units ({unit}). Allow parts of a unit on the item to use {qty}."),
            ));
        }
    }
    Ok(())
}

/// A positive quantity this item can take.
fn amount(conn: &Connection, item: &str, qty: Qty) -> AppResult<Qty> {
    let qty = positive(qty)?;
    check_whole(conn, item, qty)?;
    Ok(qty)
}

fn need_available(conn: &Connection, item: &str, loc: &str, qty: Qty) -> AppResult<()> {
    let (on_hand, reserved) = level(conn, item, loc)?;
    if on_hand - reserved < qty {
        let mut msg = format!("Only {} available at this location", (on_hand - reserved).max(Qty::ZERO));
        if reserved > 0 {
            msg.push_str(&format!(" ({reserved} reserved for jobs)"));
        }
        msg.push_str(". Nothing was changed.");
        return Err(AppError::Conflict(msg));
    }
    Ok(())
}

#[derive(Deserialize, TS, Clone, Debug)]
#[ts(export)]
pub struct Receive {
    pub item_id: String,
    pub location_id: String,
    pub qty: Qty,
    /// Cost per unit for this delivery; the item's cost is used when empty.
    pub unit_cost: Option<String>,
    pub bin: Option<String>,
    #[serde(default)]
    pub reason: String,
    pub ref_kind: Option<String>,
    pub ref_id: Option<String>,
}

/// Receive stock. Updates last cost and the moving weighted-average cost.
pub fn receive(conn: &Connection, r: &Receive) -> AppResult<String> {
    let qty = amount(conn, &r.item_id, r.qty)?;
    if location_kind(conn, &r.location_id)? == "damaged" {
        return Err(AppError::invalid("location_id", "Receive into a stock location, not the damaged-stock location."));
    }
    let item = get(conn, &r.item_id)?;
    let unit_cost = match r.unit_cost.as_deref().map(str::trim).filter(|c| !c.is_empty()) {
        Some(c) => money::non_negative("unit_cost", "Unit cost", money::parse("unit_cost", "Unit cost", c)?)?,
        None => item.cost,
    };
    let total_before = item.on_hand + item.damaged;
    let avg = if total_before <= 0 {
        unit_cost
    } else {
        ((item.avg_cost * total_before.dec() + unit_cost * qty.dec()) / (total_before + qty).dec()).round_dp(6)
    };
    conn.execute(
        "UPDATE inventory_items SET cost = ?2, avg_cost = ?3 WHERE id = ?1",
        params![r.item_id, unit_cost.to_string(), avg.normalize().to_string()],
    )?;
    bump(conn, &r.item_id, &r.location_id, qty, Qty::ZERO)?;
    if let Some(bin) = r.bin.as_deref().map(clean).filter(|b| !b.is_empty()) {
        conn.execute("UPDATE stock_levels SET bin = ?3 WHERE item_id = ?1 AND location_id = ?2", params![r.item_id, r.location_id, bin])?;
    }
    record(
        conn,
        &r.item_id,
        &r.location_id,
        "receive",
        qty,
        Some(unit_cost),
        None,
        (r.ref_kind.as_deref().unwrap_or(""), r.ref_id.as_deref().unwrap_or("")),
        &r.reason,
        None,
    )
}

#[derive(Deserialize, TS, Clone, Debug)]
#[ts(export)]
pub struct Change {
    pub item_id: String,
    pub location_id: String,
    pub qty: Qty,
    #[serde(default)]
    pub reason: String,
    pub ref_kind: Option<String>,
    pub ref_id: Option<String>,
}

fn reference(c: &Change) -> (&str, &str) {
    (c.ref_kind.as_deref().unwrap_or(""), c.ref_id.as_deref().unwrap_or(""))
}

/// Take stock out (sold over the counter, used, scrapped). Reserved stock can't be issued.
pub fn issue(conn: &Connection, c: &Change) -> AppResult<String> {
    let qty = amount(conn, &c.item_id, c.qty)?;
    location_kind(conn, &c.location_id)?;
    need_available(conn, &c.item_id, &c.location_id, qty)?;
    bump(conn, &c.item_id, &c.location_id, -qty, Qty::ZERO)?;
    let cost = get(conn, &c.item_id)?.avg_cost;
    record(conn, &c.item_id, &c.location_id, "issue", -qty, Some(cost), None, reference(c), &c.reason, None)
}

/// A customer brings a part back (qty > 0 into stock).
pub fn return_in(conn: &Connection, c: &Change) -> AppResult<String> {
    let qty = amount(conn, &c.item_id, c.qty)?;
    if location_kind(conn, &c.location_id)? == "damaged" {
        return Err(AppError::invalid("location_id", "Return into a stock location; use “Damaged” for faulty returns."));
    }
    bump(conn, &c.item_id, &c.location_id, qty, Qty::ZERO)?;
    record(conn, &c.item_id, &c.location_id, "return_in", qty, None, None, reference(c), &c.reason, None)
}

/// Send stock back to the supplier.
pub fn return_out(conn: &Connection, c: &Change) -> AppResult<String> {
    let qty = amount(conn, &c.item_id, c.qty)?;
    location_kind(conn, &c.location_id)?;
    need_available(conn, &c.item_id, &c.location_id, qty)?;
    bump(conn, &c.item_id, &c.location_id, -qty, Qty::ZERO)?;
    record(conn, &c.item_id, &c.location_id, "return_out", -qty, None, None, reference(c), &c.reason, None)
}

/// Set the quantity at a location to what was counted; records the difference.
pub fn adjust_to(
    conn: &Connection,
    item: &str,
    loc: &str,
    counted: Qty,
    kind: &str,
    reason: &str,
    ref_id: Option<&str>,
) -> AppResult<Option<String>> {
    if counted < 0 {
        return Err(AppError::invalid("qty", "A count can't be negative."));
    }
    check_whole(conn, item, counted)?;
    location_kind(conn, loc)?;
    let (on_hand, reserved) = level(conn, item, loc)?;
    if counted < reserved {
        return Err(AppError::Conflict(format!(
            "{reserved} of these are reserved for jobs. Release those reservations before counting fewer."
        )));
    }
    let delta = counted - on_hand;
    if delta == 0 {
        return Ok(None);
    }
    bump(conn, item, loc, delta, Qty::ZERO)?;
    let cost = get(conn, item)?.avg_cost;
    Ok(Some(record(
        conn,
        item,
        loc,
        kind,
        delta,
        Some(cost),
        None,
        (if ref_id.is_some() { "count" } else { "" }, ref_id.unwrap_or("")),
        reason,
        None,
    )?))
}

#[derive(Deserialize, TS, Clone, Debug)]
#[ts(export)]
pub struct Adjust {
    pub item_id: String,
    pub location_id: String,
    /// The new quantity on hand at this location.
    pub counted: Qty,
    pub reason: String,
}

pub fn adjust(conn: &Connection, a: &Adjust) -> AppResult<Option<String>> {
    if a.reason.trim().is_empty() {
        return Err(AppError::invalid("reason", "Say why the quantity changed (for the history)."));
    }
    adjust_to(conn, &a.item_id, &a.location_id, a.counted, "adjust", &a.reason, None)
}

#[derive(Deserialize, TS, Clone, Debug)]
#[ts(export)]
pub struct CountLine {
    pub item_id: String,
    pub counted: Qty,
}

#[derive(Serialize, TS, Clone, Debug)]
#[ts(export)]
pub struct CountResult {
    pub count_id: String,
    pub changed: i64,
    pub unchanged: i64,
}

/// Post a physical count for one location: every line in one transaction.
pub fn post_count(conn: &Connection, location_id: &str, lines: &[CountLine]) -> AppResult<CountResult> {
    let count_id = db::new_id();
    let (mut changed, mut unchanged) = (0, 0);
    for l in lines {
        match adjust_to(conn, &l.item_id, location_id, l.counted, "count", "Physical count", Some(&count_id))? {
            Some(_) => changed += 1,
            None => unchanged += 1,
        }
    }
    db::audit(conn, "count", Some(&count_id), "post", &format!("Physical count: {changed} changed, {unchanged} matched"), None)?;
    Ok(CountResult { count_id, changed, unchanged })
}

#[derive(Deserialize, TS, Clone, Debug)]
#[ts(export)]
pub struct Transfer {
    pub item_id: String,
    pub from_location_id: String,
    pub to_location_id: String,
    pub qty: Qty,
    pub bin: Option<String>,
    #[serde(default)]
    pub reason: String,
}

fn transfer_kind(conn: &Connection, t: &Transfer, kind_out: &str, kind_in: &str) -> AppResult<String> {
    let qty = amount(conn, &t.item_id, t.qty)?;
    if t.from_location_id == t.to_location_id {
        return Err(AppError::invalid("to_location_id", "Choose a different location to move to."));
    }
    location_kind(conn, &t.from_location_id)?;
    location_kind(conn, &t.to_location_id)?;
    need_available(conn, &t.item_id, &t.from_location_id, qty)?;
    let group = db::new_id();
    bump(conn, &t.item_id, &t.from_location_id, -qty, Qty::ZERO)?;
    bump(conn, &t.item_id, &t.to_location_id, qty, Qty::ZERO)?;
    if let Some(bin) = t.bin.as_deref().map(clean).filter(|b| !b.is_empty()) {
        conn.execute(
            "UPDATE stock_levels SET bin = ?3 WHERE item_id = ?1 AND location_id = ?2",
            params![t.item_id, t.to_location_id, bin],
        )?;
    }
    let cost = get(conn, &t.item_id)?.avg_cost;
    record(conn, &t.item_id, &t.from_location_id, kind_out, -qty, Some(cost), Some(&group), ("", ""), &t.reason, None)?;
    record(conn, &t.item_id, &t.to_location_id, kind_in, qty, Some(cost), Some(&group), ("", ""), &t.reason, None)?;
    Ok(group)
}

pub fn transfer(conn: &Connection, t: &Transfer) -> AppResult<String> {
    if location_kind(conn, &t.to_location_id)? == "damaged" {
        return Err(AppError::invalid("to_location_id", "Use “Mark damaged” to move stock into quarantine."));
    }
    transfer_kind(conn, t, "transfer_out", "transfer_in")
}

/// Move stock to the damaged/quarantine location. It stays on record but isn't sellable.
pub fn mark_damaged(conn: &Connection, c: &Change) -> AppResult<String> {
    if c.reason.trim().is_empty() {
        return Err(AppError::invalid("reason", "Describe the damage (for the history)."));
    }
    let t = Transfer {
        item_id: c.item_id.clone(),
        from_location_id: c.location_id.clone(),
        to_location_id: DAMAGED.into(),
        qty: c.qty,
        bin: None,
        reason: c.reason.clone(),
    };
    transfer_kind(conn, &t, "damage", "damage")
}

// ---------------------------------------------------------------- reservations

#[derive(Serialize, TS, Clone, Debug)]
#[ts(export)]
pub struct Reservation {
    pub id: String,
    pub item_id: String,
    pub sku: String,
    pub name: String,
    pub location_id: String,
    pub location: String,
    pub qty: Qty,
    pub status: String,
    pub job_id: Option<String>,
    pub job_number: Option<i64>,
    pub line_id: Option<String>,
    pub vehicle_id: Option<String>,
    pub note: String,
    pub created_at: String,
    pub closed_at: Option<String>,
}

const RES_SELECT: &str = "SELECT r.id, r.item_id, i.sku, p.name, r.location_id, l.name, r.qty, r.status, r.job_id, j.number, r.line_id, r.vehicle_id, r.note, r.created_at, r.closed_at
  FROM stock_reservations r JOIN inventory_items i ON i.id = r.item_id JOIN parts p ON p.id = i.part_id
  JOIN inventory_locations l ON l.id = r.location_id LEFT JOIN service_jobs j ON j.id = r.job_id";

fn res_row(r: &rusqlite::Row) -> rusqlite::Result<Reservation> {
    Ok(Reservation {
        id: r.get(0)?,
        item_id: r.get(1)?,
        sku: r.get(2)?,
        name: r.get(3)?,
        location_id: r.get(4)?,
        location: r.get(5)?,
        qty: r.get(6)?,
        status: r.get(7)?,
        job_id: r.get(8)?,
        job_number: r.get(9)?,
        line_id: r.get(10)?,
        vehicle_id: r.get(11)?,
        note: r.get(12)?,
        created_at: r.get(13)?,
        closed_at: r.get(14)?,
    })
}

pub fn reservation(conn: &Connection, id: &str) -> AppResult<Reservation> {
    conn.query_row(&format!("{RES_SELECT} WHERE r.id = ?1"), [id], res_row)
        .optional()?
        .ok_or_else(|| AppError::not_found("That reservation"))
}

pub fn reservations(conn: &Connection, item_id: Option<&str>, active_only: bool) -> AppResult<Vec<Reservation>> {
    let mut st = conn.prepare(&format!(
        "{RES_SELECT} WHERE (?1 IS NULL OR r.item_id = ?1) AND (NOT ?2 OR r.status = 'active') ORDER BY r.created_at DESC LIMIT 500"
    ))?;
    let rows = st.query_map(params![item_id, active_only], res_row)?;
    Ok(rows.collect::<Result<_, _>>()?)
}

#[derive(Deserialize, TS, Clone, Debug)]
#[ts(export)]
pub struct Reserve {
    pub item_id: String,
    /// Defaults to the location with the most available stock.
    pub location_id: Option<String>,
    pub qty: Qty,
    pub job_id: Option<String>,
    pub line_id: Option<String>,
    pub vehicle_id: Option<String>,
    #[serde(default)]
    pub note: String,
}

pub fn reserve(conn: &Connection, r: &Reserve) -> AppResult<Reservation> {
    let qty = amount(conn, &r.item_id, r.qty)?;
    let loc = match &r.location_id {
        Some(l) => l.clone(),
        None => conn
            .query_row(
                "SELECT s.location_id FROM stock_levels s JOIN inventory_locations l ON l.id = s.location_id
                 WHERE s.item_id = ?1 AND l.kind = 'warehouse' AND l.archived = 0 ORDER BY s.on_hand - s.reserved DESC, l.is_default DESC LIMIT 1",
                [&r.item_id],
                |row| row.get(0),
            )
            .optional()?
            .ok_or_else(|| AppError::Conflict("None of this item is in stock to reserve.".into()))?,
    };
    if location_kind(conn, &loc)? == "damaged" {
        return Err(AppError::invalid("location_id", "Damaged stock can't be reserved."));
    }
    need_available(conn, &r.item_id, &loc, qty)?;
    bump(conn, &r.item_id, &loc, Qty::ZERO, qty)?;
    let id = db::new_id();
    conn.execute(
        "INSERT INTO stock_reservations (id, item_id, location_id, qty, job_id, line_id, vehicle_id, note) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![id, r.item_id, loc, qty, r.job_id, r.line_id, r.vehicle_id, r.note.trim()],
    )?;
    db::audit(conn, "reservation", Some(&id), "reserve", &format!("Reserved {qty}"), None)?;
    reservation(conn, &id)
}

/// Put reserved stock back to available. Releasing twice is harmless.
pub fn release(conn: &Connection, id: &str) -> AppResult<Reservation> {
    let r = reservation(conn, id)?;
    if r.status != "active" {
        return Ok(r);
    }
    bump(conn, &r.item_id, &r.location_id, Qty::ZERO, -r.qty)?;
    conn.execute("UPDATE stock_reservations SET status = 'released', closed_at = ?2 WHERE id = ?1", params![id, db::now_utc()])?;
    db::audit(conn, "reservation", Some(id), "release", &format!("Released {}", r.qty), None)?;
    reservation(conn, id)
}

/// Use reserved stock (job completed): lowers on-hand and reserved together. Consuming a
/// reservation that is already consumed returns the original movement and changes nothing,
/// so stock is never deducted twice.
pub fn consume(conn: &Connection, id: &str) -> AppResult<String> {
    let r = reservation(conn, id)?;
    match r.status.as_str() {
        "consumed" => {
            return conn
                .query_row(
                    "SELECT id FROM inventory_movements WHERE ref_kind = 'reservation' AND ref_id = ?1 AND kind = 'consume'",
                    [id],
                    |row| row.get(0),
                )
                .map_err(Into::into);
        }
        "released" => return Err(AppError::Conflict("This reservation was released; reserve the part again first.".into())),
        _ => {}
    }
    bump(conn, &r.item_id, &r.location_id, -r.qty, -r.qty)?;
    let cost = get(conn, &r.item_id)?.avg_cost;
    let m = record(conn, &r.item_id, &r.location_id, "consume", -r.qty, Some(cost), None, ("reservation", id), &r.note, None)?;
    conn.execute("UPDATE stock_reservations SET status = 'consumed', closed_at = ?2 WHERE id = ?1", params![id, db::now_utc()])?;
    Ok(m)
}

// ---------------------------------------------------------------- history and undo

#[derive(Serialize, TS, Clone, Debug)]
#[ts(export)]
pub struct Movement {
    pub id: String,
    pub item_id: String,
    pub sku: String,
    pub name: String,
    pub location: String,
    pub kind: String,
    pub qty: Qty,
    #[ts(type = "string | null")]
    pub unit_cost: Option<Decimal>,
    pub group_id: Option<String>,
    pub ref_kind: Option<String>,
    pub ref_id: Option<String>,
    pub reason: String,
    pub reverses_id: Option<String>,
    pub reversed: bool,
    pub created_at: String,
}

pub fn movements(conn: &Connection, item_id: Option<&str>, limit: i64) -> AppResult<Vec<Movement>> {
    let mut st = conn.prepare(
        "SELECT m.id, m.item_id, i.sku, p.name, l.name, m.kind, m.qty, m.unit_cost, m.group_id, m.ref_kind, m.ref_id, m.reason, m.reverses_id,
                EXISTS (SELECT 1 FROM inventory_movements x WHERE x.reverses_id = m.id), m.created_at
         FROM inventory_movements m JOIN inventory_items i ON i.id = m.item_id JOIN parts p ON p.id = i.part_id JOIN inventory_locations l ON l.id = m.location_id
         WHERE (?1 IS NULL OR m.item_id = ?1) ORDER BY m.created_at DESC, m.id DESC LIMIT ?2",
    )?;
    let rows = st.query_map(params![item_id, limit.clamp(1, 10_000)], |r| {
        Ok(Movement {
            id: r.get(0)?,
            item_id: r.get(1)?,
            sku: r.get(2)?,
            name: r.get(3)?,
            location: r.get(4)?,
            kind: r.get(5)?,
            qty: r.get(6)?,
            unit_cost: db::opt_dec(r, "unit_cost").ok().flatten(),
            group_id: r.get(8)?,
            ref_kind: r.get(9)?,
            ref_id: r.get(10)?,
            reason: r.get(11)?,
            reverses_id: r.get(12)?,
            reversed: r.get(13)?,
            created_at: r.get(14)?,
        })
    })?;
    Ok(rows.collect::<Result<_, _>>()?)
}

/// Undo a movement with a compensating one (both halves of a transfer together). Reservations
/// consumed by jobs are undone by reopening the job instead.
pub fn reverse(conn: &Connection, movement_id: &str) -> AppResult<Vec<String>> {
    let (kind, group): (String, Option<String>) = conn
        .query_row("SELECT kind, group_id FROM inventory_movements WHERE id = ?1", [movement_id], |r| Ok((r.get(0)?, r.get(1)?)))
        .optional()?
        .ok_or_else(|| AppError::not_found("That stock movement"))?;
    if matches!(kind.as_str(), "consume" | "reverse") {
        return Err(AppError::msg("This change can't be undone here."));
    }
    let ids: Vec<String> = match &group {
        Some(g) => {
            let mut st = conn.prepare("SELECT id FROM inventory_movements WHERE group_id = ?1 ORDER BY qty")?;
            st.query_map([g], |r| r.get(0))?.collect::<Result<_, _>>()?
        }
        None => vec![movement_id.to_string()],
    };
    let mut out = Vec::new();
    for id in ids {
        if db::exists(conn, "SELECT 1 FROM inventory_movements WHERE reverses_id = ?1", [&id])? {
            return Err(AppError::Conflict("That change was already undone.".into()));
        }
        let (item, loc, qty): (String, String, Qty) =
            conn.query_row("SELECT item_id, location_id, qty FROM inventory_movements WHERE id = ?1", [&id], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?))
            })?;
        // Taking back stock that has since been reserved or used would go negative: the CHECKs refuse it.
        bump(conn, &item, &loc, -qty, Qty::ZERO)?;
        out.push(record(conn, &item, &loc, "reverse", -qty, None, None, ("", ""), "Undo", Some(&id))?);
    }
    db::audit(conn, "movement", Some(movement_id), "reverse", "Undid a stock change", None)?;
    Ok(out)
}

#[derive(Serialize, TS, Clone, Debug)]
#[ts(export)]
pub struct Valuation {
    pub method: String,
    #[ts(type = "string")]
    pub at_cost: Decimal,
    pub units: Qty,
    pub items: i64,
}

/// Inventory value at cost: weighted-average or standard (last purchase) cost.
pub fn valuation(conn: &Connection, method: &str) -> AppResult<Valuation> {
    let col = if method == "standard" { "cost" } else { "avg_cost" };
    let mut st = conn.prepare(&format!(
        "SELECT i.{col}, coalesce(sum(s.on_hand), 0) FROM inventory_items i
         LEFT JOIN stock_levels s ON s.item_id = i.id AND s.location_id IN (SELECT id FROM inventory_locations WHERE kind = 'warehouse')
         WHERE i.archived = 0 GROUP BY i.id"
    ))?;
    let mut at_cost = Decimal::ZERO;
    let mut units = Qty::ZERO;
    let mut items = 0;
    let rows = st.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, Qty>(1)?)))?;
    for row in rows {
        let (c, q) = row?;
        items += 1;
        units += q;
        at_cost += c.parse::<Decimal>().unwrap_or_default() * q.dec();
    }
    Ok(Valuation {
        method: if method == "standard" { "standard".into() } else { "average".into() },
        at_cost: money::round_money(at_cost),
        units,
        items,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parts;
    use crate::qty::Qty;
    use rust_decimal_macros::dec;

    fn setup() -> (Connection, String) {
        let conn = db::open_memory().unwrap();
        let p = parts::create(
            &conn,
            &parts::PartInput { name: "Oil filter".into(), category_id: Some("engine.oil_filter".into()), ..Default::default() },
            "user",
        )
        .unwrap();
        let i = create(
            &conn,
            &p.id,
            &ItemInput {
                sku: "OF-100".into(),
                condition: "new".into(),
                unit: "each".into(),
                cost: "4.00".into(),
                price_override: None,
                pricing_profile_id: None,
                fractional: false,
                min_stock: Qty::units(2),
                reorder_qty: Qty::units(10),
                supplier_id: None,
                warranty: String::new(),
                notes: String::new(),
            },
        )
        .unwrap();
        (conn, i.id)
    }

    fn rcv(conn: &Connection, item: &str, qty: impl Into<Qty>, cost: &str) -> String {
        receive(
            conn,
            &Receive {
                item_id: item.into(),
                location_id: "loc-main".into(),
                qty: qty.into(),
                unit_cost: Some(cost.into()),
                bin: Some("A-3".into()),
                reason: String::new(),
                ref_kind: None,
                ref_id: None,
            },
        )
        .unwrap()
    }
    fn chg(item: &str, qty: impl Into<Qty>) -> Change {
        Change {
            item_id: item.into(),
            location_id: "loc-main".into(),
            qty: qty.into(),
            reason: "test".into(),
            ref_kind: None,
            ref_id: None,
        }
    }

    #[test]
    fn parts_of_a_unit_only_where_allowed() {
        let (conn, id) = setup();
        let q = |s: &str| Qty::parse("qty", s).unwrap();
        let whole = Receive {
            item_id: id.clone(),
            location_id: "loc-main".into(),
            qty: q("1.5"),
            unit_cost: None,
            bin: None,
            reason: String::new(),
            ref_kind: None,
            ref_id: None,
        };
        assert!(receive(&conn, &whole).unwrap_err().to_string().contains("whole units"), "whole-unit items refuse 1.5");
        let mut item = get(&conn, &id).unwrap();
        let input = ItemInput {
            sku: item.sku.clone(),
            condition: item.condition.clone(),
            unit: "L".into(),
            cost: item.cost.to_string(),
            price_override: None,
            pricing_profile_id: None,
            fractional: true,
            min_stock: q("2.5"),
            reorder_qty: q("10"),
            supplier_id: None,
            warranty: String::new(),
            notes: String::new(),
        };
        item = update(&conn, &id, item.version, &input).unwrap();
        assert!(item.fractional);
        rcv(&conn, &id, q("4.5"), "8.00");
        rcv(&conn, &id, q("0.5"), "10.00");
        issue(&conn, &chg(&id, q("0.25"))).unwrap();
        reserve(
            &conn,
            &Reserve {
                item_id: id.clone(),
                location_id: None,
                qty: q("1.75"),
                job_id: None,
                line_id: None,
                vehicle_id: None,
                note: String::new(),
            },
        )
        .unwrap();
        let i = get(&conn, &id).unwrap();
        assert_eq!((i.on_hand.to_string(), i.reserved.to_string(), i.available.to_string()), ("4.75".into(), "1.75".into(), "3".into()));
        assert_eq!(i.avg_cost.to_string(), "8.2", "(4.5 × 8 + 0.5 × 10) / 5");
        assert_eq!(i.stock_state(), "ok");
        assert!(issue(&conn, &chg(&id, q("3.001"))).is_err(), "can't take more than is available");
        assert!(issue(&conn, &chg(&id, Qty::ZERO)).is_err(), "zero is refused");
    }

    #[test]
    fn receive_issue_and_weighted_cost() {
        let (conn, id) = setup();
        rcv(&conn, &id, 10, "4.00");
        rcv(&conn, &id, 10, "5.00");
        let i = get(&conn, &id).unwrap();
        assert_eq!((i.on_hand, i.available), (Qty::units(20), Qty::units(20)));
        assert_eq!(i.avg_cost, dec!(4.5));
        assert_eq!(i.cost, dec!(5.00));
        assert_eq!(i.bins, "Main: A-3");
        issue(&conn, &chg(&id, 5)).unwrap();
        assert_eq!(get(&conn, &id).unwrap().on_hand, 15);
        assert_eq!(valuation(&conn, "average").unwrap().at_cost, dec!(67.50));
    }

    #[test]
    fn never_negative() {
        let (conn, id) = setup();
        rcv(&conn, &id, 2, "4");
        let e = issue(&conn, &chg(&id, 3)).unwrap_err();
        assert_eq!(e.kind(), "conflict");
        assert_eq!(get(&conn, &id).unwrap().on_hand, 2, "nothing changed");
        assert!(issue(&conn, &chg(&id, 0)).is_err());
        assert!(issue(&conn, &chg(&id, -1)).is_err());
    }

    #[test]
    fn reservations_protect_stock_and_consume_once() {
        let (conn, id) = setup();
        rcv(&conn, &id, 4, "4");
        let r = reserve(
            &conn,
            &Reserve {
                item_id: id.clone(),
                location_id: None,
                qty: Qty::units(3),
                job_id: None,
                line_id: None,
                vehicle_id: None,
                note: String::new(),
            },
        )
        .unwrap();
        let i = get(&conn, &id).unwrap();
        assert_eq!((i.on_hand, i.reserved, i.available), (Qty::units(4), Qty::units(3), Qty::units(1)));
        assert!(issue(&conn, &chg(&id, 2)).is_err(), "reserved stock can't be issued");
        assert!(
            reserve(
                &conn,
                &Reserve {
                    item_id: id.clone(),
                    location_id: None,
                    qty: Qty::units(2),
                    job_id: None,
                    line_id: None,
                    vehicle_id: None,
                    note: String::new()
                }
            )
            .is_err()
        );
        let m1 = consume(&conn, &r.id).unwrap();
        let m2 = consume(&conn, &r.id).unwrap();
        assert_eq!(m1, m2, "consuming twice is the same movement");
        let i = get(&conn, &id).unwrap();
        assert_eq!((i.on_hand, i.reserved, i.available), (Qty::units(1), Qty::units(0), Qty::units(1)), "deducted exactly once");
        assert!(release(&conn, &r.id).unwrap().status == "consumed");
    }

    #[test]
    fn release_returns_availability() {
        let (conn, id) = setup();
        rcv(&conn, &id, 4, "4");
        let r = reserve(
            &conn,
            &Reserve {
                item_id: id.clone(),
                location_id: Some("loc-main".into()),
                qty: Qty::units(2),
                job_id: None,
                line_id: None,
                vehicle_id: None,
                note: String::new(),
            },
        )
        .unwrap();
        release(&conn, &r.id).unwrap();
        release(&conn, &r.id).unwrap();
        let i = get(&conn, &id).unwrap();
        assert_eq!((i.reserved, i.available), (Qty::units(0), Qty::units(4)));
        assert!(consume(&conn, &r.id).is_err(), "a released reservation can't be consumed");
    }

    #[test]
    fn transfer_damage_count_and_returns() {
        let (conn, id) = setup();
        rcv(&conn, &id, 10, "4");
        let shelf = add_location(&conn, "Van").unwrap();
        transfer(
            &conn,
            &Transfer {
                item_id: id.clone(),
                from_location_id: "loc-main".into(),
                to_location_id: shelf.id.clone(),
                qty: Qty::units(3),
                bin: None,
                reason: String::new(),
            },
        )
        .unwrap();
        mark_damaged(&conn, &chg(&id, 1)).unwrap();
        let i = get(&conn, &id).unwrap();
        assert_eq!((i.on_hand, i.damaged), (Qty::units(9), Qty::units(1)), "damaged stock is not sellable");
        post_count(&conn, "loc-main", &[CountLine { item_id: id.clone(), counted: Qty::units(5) }]).unwrap();
        assert_eq!(level(&conn, &id, "loc-main").unwrap().0, 5);
        return_in(&conn, &chg(&id, 1)).unwrap();
        return_out(&conn, &chg(&id, 2)).unwrap();
        assert_eq!(level(&conn, &id, "loc-main").unwrap().0, 4);
        assert_eq!(get(&conn, &id).unwrap().on_hand, 7);
    }

    #[test]
    fn undo_is_a_compensating_movement() {
        let (conn, id) = setup();
        let m = rcv(&conn, &id, 5, "4");
        let shelf = add_location(&conn, "Shelf").unwrap();
        let g = transfer(
            &conn,
            &Transfer {
                item_id: id.clone(),
                from_location_id: "loc-main".into(),
                to_location_id: shelf.id.clone(),
                qty: Qty::units(2),
                bin: None,
                reason: String::new(),
            },
        )
        .unwrap();
        let half: String = conn.query_row("SELECT id FROM inventory_movements WHERE group_id = ?1 LIMIT 1", [&g], |r| r.get(0)).unwrap();
        reverse(&conn, &half).unwrap();
        assert_eq!(level(&conn, &id, "loc-main").unwrap().0, 5);
        assert_eq!(level(&conn, &id, &shelf.id).unwrap().0, 0);
        assert!(reverse(&conn, &half).is_err(), "only once");
        // Undoing a receipt whose stock has been reserved would go negative: refused.
        reserve(
            &conn,
            &Reserve {
                item_id: id.clone(),
                location_id: None,
                qty: Qty::units(5),
                job_id: None,
                line_id: None,
                vehicle_id: None,
                note: String::new(),
            },
        )
        .unwrap();
        assert!(reverse(&conn, &m).is_err());
        assert_eq!(movements(&conn, Some(&id), 100).unwrap().len(), 5);
    }

    #[test]
    fn duplicate_sku_and_stale_edits() {
        let (conn, id) = setup();
        let i = get(&conn, &id).unwrap();
        let p2 = parts::create(&conn, &parts::PartInput { name: "Air filter".into(), ..Default::default() }, "user").unwrap();
        let input = ItemInput {
            sku: "of-100".into(),
            condition: "new".into(),
            unit: "each".into(),
            cost: "1".into(),
            price_override: None,
            pricing_profile_id: None,
            fractional: false,
            min_stock: Qty::units(0),
            reorder_qty: Qty::units(0),
            supplier_id: None,
            warranty: String::new(),
            notes: String::new(),
        };
        assert!(create(&conn, &p2.id, &input).is_err(), "SKUs are unique ignoring case");
        let edit = ItemInput { sku: "OF-100".into(), cost: "4.10".into(), ..input };
        update(&conn, &id, i.version, &edit).unwrap();
        let e = update(&conn, &id, i.version, &edit).unwrap_err();
        assert_eq!(e.kind(), "conflict", "a form opened before the last save is stale");
    }

    #[test]
    fn concurrent_reservations_cannot_oversell() {
        // Two writers race for the last units through separate connections to one file.
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("db.sqlite");
        let item_id = {
            let mut c = Connection::open(&path).unwrap();
            db::configure(&c).unwrap();
            db::migrate(&mut c, |_, _| Ok(())).unwrap();
            let p = parts::create(&c, &parts::PartInput { name: "Pad".into(), ..Default::default() }, "user").unwrap();
            let i = create(
                &c,
                &p.id,
                &ItemInput {
                    sku: "P1".into(),
                    condition: "new".into(),
                    unit: "each".into(),
                    cost: "1".into(),
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
            rcv(&c, &i.id, 5, "1");
            i.id
        };
        let handles: Vec<_> = (0..8)
            .map(|_| {
                let path = path.clone();
                let item_id = item_id.clone();
                std::thread::spawn(move || {
                    let mut c = Connection::open(&path).unwrap();
                    db::configure(&c).unwrap();
                    let tx = c.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate).unwrap();
                    let ok = reserve(
                        &tx,
                        &Reserve {
                            item_id,
                            location_id: None,
                            qty: Qty::units(2),
                            job_id: None,
                            line_id: None,
                            vehicle_id: None,
                            note: String::new(),
                        },
                    )
                    .is_ok();
                    if ok {
                        tx.commit().unwrap();
                    }
                    ok
                })
            })
            .collect();
        let wins = handles.into_iter().map(|h| h.join().unwrap()).filter(|ok| *ok).count();
        assert_eq!(wins, 2, "5 units allow exactly two reservations of 2");
        let c = Connection::open(&path).unwrap();
        let i = get(&c, &item_id).unwrap();
        assert_eq!((i.reserved, i.available), (Qty::units(4), Qty::units(1)));
    }

    #[test]
    fn rollback_leaves_no_trace() {
        let (mut conn, id) = setup();
        rcv(&conn, &id, 1, "4");
        let tx = conn.transaction().unwrap();
        issue(&tx, &chg(&id, 1)).unwrap();
        assert!(issue(&tx, &chg(&id, 1)).is_err());
        drop(tx); // rolled back
        assert_eq!(get(&conn, &id).unwrap().on_hand, 1);
        assert_eq!(movements(&conn, Some(&id), 10).unwrap().len(), 1);
    }
}
