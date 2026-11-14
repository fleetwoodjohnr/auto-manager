//! Saved parts lists ("shopping lists") for a repair or project.

use crate::db::{self, clean, expect_one};
use crate::error::{AppError, AppResult};
use rusqlite::{Connection, OptionalExtension, params};
use serde::Serialize;
use ts_rs::TS;

#[derive(Serialize, TS, Clone, Debug)]
#[ts(export)]
pub struct PartList {
    pub id: String,
    pub name: String,
    pub vehicle_id: Option<String>,
    pub vehicle_title: Option<String>,
    pub notes: String,
    pub created_at: String,
    pub items: Vec<PartListItem>,
}

#[derive(Serialize, TS, Clone, Debug)]
#[ts(export)]
pub struct PartListItem {
    pub part_id: String,
    pub name: String,
    pub brand: String,
    pub numbers: String,
    pub qty: i64,
    pub note: String,
    pub available: i64,
}

pub fn get(conn: &Connection, id: &str) -> AppResult<PartList> {
    let mut l = conn
        .query_row(
            "SELECT l.id, l.name, l.vehicle_id, CASE WHEN v.id IS NULL THEN NULL ELSE trim(coalesce(v.year, '') || ' ' || v.make || ' ' || v.model) END, l.notes, l.created_at
             FROM part_lists l LEFT JOIN vehicles v ON v.id = l.vehicle_id WHERE l.id = ?1",
            [id],
            |r| Ok(PartList { id: r.get(0)?, name: r.get(1)?, vehicle_id: r.get(2)?, vehicle_title: r.get(3)?, notes: r.get(4)?, created_at: r.get(5)?, items: vec![] }),
        )
        .optional()?
        .ok_or_else(|| AppError::not_found("That list"))?;
    let mut st = conn.prepare(
        "SELECT p.id, p.name, p.brand, coalesce((SELECT group_concat(value, ' · ') FROM part_identifiers WHERE part_id = p.id AND kind <> 'barcode'), ''), x.qty, x.note,
           coalesce((SELECT sum(s.on_hand - s.reserved) FROM inventory_items i JOIN stock_levels s ON s.item_id = i.id JOIN inventory_locations loc ON loc.id = s.location_id
                     WHERE i.part_id = p.id AND loc.kind = 'warehouse'), 0)
         FROM part_list_items x JOIN parts p ON p.id = x.part_id WHERE x.list_id = ?1 ORDER BY x.added_at",
    )?;
    l.items = st
        .query_map([id], |r| {
            Ok(PartListItem {
                part_id: r.get(0)?,
                name: r.get(1)?,
                brand: r.get(2)?,
                numbers: r.get(3)?,
                qty: r.get(4)?,
                note: r.get(5)?,
                available: r.get(6)?,
            })
        })?
        .collect::<Result<_, _>>()?;
    Ok(l)
}

pub fn list(conn: &Connection, vehicle_id: Option<&str>) -> AppResult<Vec<PartList>> {
    let mut st = conn.prepare("SELECT id FROM part_lists WHERE ?1 IS NULL OR vehicle_id = ?1 ORDER BY created_at DESC")?;
    let ids: Vec<String> = st.query_map([vehicle_id], |r| r.get(0))?.collect::<Result<_, _>>()?;
    ids.iter().map(|i| get(conn, i)).collect()
}

pub fn create(conn: &Connection, name: &str, vehicle_id: Option<&str>) -> AppResult<PartList> {
    let name = clean(name);
    if name.is_empty() {
        return Err(AppError::invalid("name", "Name the list."));
    }
    let id = db::new_id();
    conn.execute("INSERT INTO part_lists (id, name, vehicle_id) VALUES (?1, ?2, ?3)", params![id, name, vehicle_id])?;
    get(conn, &id)
}

pub fn add(conn: &Connection, list_id: &str, part_id: &str, qty: i64, note: &str) -> AppResult<PartList> {
    if qty < 1 {
        return Err(AppError::invalid("qty", "Quantity is at least 1."));
    }
    get(conn, list_id)?;
    conn.execute(
        "INSERT INTO part_list_items (list_id, part_id, qty, note) VALUES (?1, ?2, ?3, ?4)
         ON CONFLICT (list_id, part_id) DO UPDATE SET qty = part_list_items.qty + excluded.qty",
        params![list_id, part_id, qty, note.trim()],
    )?;
    get(conn, list_id)
}

pub fn remove(conn: &Connection, list_id: &str, part_id: &str) -> AppResult<PartList> {
    conn.execute("DELETE FROM part_list_items WHERE list_id = ?1 AND part_id = ?2", params![list_id, part_id])?;
    get(conn, list_id)
}

pub fn delete(conn: &Connection, id: &str) -> AppResult<()> {
    let n = conn.execute("DELETE FROM part_lists WHERE id = ?1", [id])?;
    expect_one(n, "That list")
}

pub const RESTOCK_LIST: &str = "Restocking requests";

/// Add a part to the shared restocking list (created on first use).
pub fn restock_request(conn: &Connection, part_id: &str) -> AppResult<PartList> {
    let id: Option<String> =
        conn.query_row("SELECT id FROM part_lists WHERE name = ?1 AND vehicle_id IS NULL", [RESTOCK_LIST], |r| r.get(0)).optional()?;
    let id = match id {
        Some(id) => id,
        None => create(conn, RESTOCK_LIST, None)?.id,
    };
    add(conn, &id, part_id, 1, "")
}
