//! Supplier directory.

use crate::db::{self, clean, expect_one};
use crate::error::{AppError, AppResult};
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Serialize, TS, Clone, Debug)]
#[ts(export)]
pub struct Supplier {
    pub id: String,
    pub name: String,
    pub contact: String,
    pub phone: String,
    pub email: String,
    pub website: String,
    pub account_no: String,
    pub lead_time_days: Option<i64>,
    pub preferred: bool,
    pub notes: String,
    pub item_count: i64,
}

#[derive(Deserialize, TS, Clone, Debug, Default)]
#[ts(export)]
pub struct SupplierInput {
    pub name: String,
    #[serde(default)]
    pub contact: String,
    #[serde(default)]
    pub phone: String,
    #[serde(default)]
    pub email: String,
    #[serde(default)]
    pub website: String,
    #[serde(default)]
    pub account_no: String,
    pub lead_time_days: Option<i64>,
    #[serde(default)]
    pub preferred: bool,
    #[serde(default)]
    pub notes: String,
}

const SELECT: &str = "SELECT s.id, s.name, s.contact, s.phone, s.email, s.website, s.account_no, s.lead_time_days, s.preferred, s.notes,
  (SELECT count(*) FROM inventory_items i WHERE i.supplier_id = s.id AND i.archived = 0) FROM suppliers s";

fn row(r: &rusqlite::Row) -> rusqlite::Result<Supplier> {
    Ok(Supplier {
        id: r.get(0)?,
        name: r.get(1)?,
        contact: r.get(2)?,
        phone: r.get(3)?,
        email: r.get(4)?,
        website: r.get(5)?,
        account_no: r.get(6)?,
        lead_time_days: r.get(7)?,
        preferred: r.get::<_, i64>(8)? == 1,
        notes: r.get(9)?,
        item_count: r.get(10)?,
    })
}

pub fn get(conn: &Connection, id: &str) -> AppResult<Supplier> {
    conn.query_row(&format!("{SELECT} WHERE s.id = ?1"), [id], row).optional()?.ok_or_else(|| AppError::not_found("That supplier"))
}

pub fn list(conn: &Connection) -> AppResult<Vec<Supplier>> {
    let mut st = conn.prepare(&format!("{SELECT} ORDER BY s.preferred DESC, s.name COLLATE NOCASE"))?;
    let rows = st.query_map([], row)?;
    Ok(rows.collect::<Result<_, _>>()?)
}

fn validate(s: &SupplierInput) -> AppResult<String> {
    let name = clean(&s.name);
    if name.is_empty() {
        return Err(AppError::invalid("name", "Enter the supplier's name."));
    }
    if !s.email.trim().is_empty() && !s.email.contains('@') {
        return Err(AppError::invalid("email", "That email address doesn't look right."));
    }
    if s.lead_time_days.is_some_and(|d| !(0..=365).contains(&d)) {
        return Err(AppError::invalid("lead_time_days", "Lead time is between 0 and 365 days."));
    }
    Ok(name)
}

pub fn create(conn: &Connection, s: &SupplierInput) -> AppResult<Supplier> {
    let name = validate(s)?;
    let id = db::new_id();
    conn.execute(
        "INSERT INTO suppliers (id, name, contact, phone, email, website, account_no, lead_time_days, preferred, notes) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
        params![id, name, clean(&s.contact), clean(&s.phone), s.email.trim(), s.website.trim(), clean(&s.account_no), s.lead_time_days, s.preferred, s.notes.trim()],
    )?;
    get(conn, &id)
}

pub fn update(conn: &Connection, id: &str, s: &SupplierInput) -> AppResult<Supplier> {
    let name = validate(s)?;
    let n = conn.execute(
        "UPDATE suppliers SET name = ?2, contact = ?3, phone = ?4, email = ?5, website = ?6, account_no = ?7, lead_time_days = ?8, preferred = ?9, notes = ?10 WHERE id = ?1",
        params![id, name, clean(&s.contact), clean(&s.phone), s.email.trim(), s.website.trim(), clean(&s.account_no), s.lead_time_days, s.preferred, s.notes.trim()],
    )?;
    expect_one(n, "That supplier")?;
    get(conn, id)
}

pub fn delete(conn: &Connection, id: &str) -> AppResult<()> {
    let n = conn.execute("DELETE FROM suppliers WHERE id = ?1", [id])?;
    expect_one(n, "That supplier")
}
