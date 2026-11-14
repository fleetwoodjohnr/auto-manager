//! Customers, for shops: their vehicles, estimates, jobs and invoices hang off this record.
//! Stored locally only.

use crate::db::{self, clean, expect_one};
use crate::error::{AppError, AppResult};
use crate::money;
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Serialize, TS, Clone, Debug)]
#[ts(export)]
pub struct Customer {
    pub id: String,
    pub name: String,
    pub phone: String,
    pub email: String,
    pub address: String,
    pub notes: String,
    pub pricing_profile_id: Option<String>,
    pub discount_pct: Option<String>,
    pub vehicle_count: i64,
    pub job_count: i64,
    pub created_at: String,
}

#[derive(Deserialize, TS, Clone, Debug, Default)]
#[ts(export)]
pub struct CustomerInput {
    pub name: String,
    #[serde(default)]
    pub phone: String,
    #[serde(default)]
    pub email: String,
    #[serde(default)]
    pub address: String,
    #[serde(default)]
    pub notes: String,
    pub pricing_profile_id: Option<String>,
    pub discount_pct: Option<String>,
}

const SELECT: &str = "SELECT c.id, c.name, c.phone, c.email, c.address, c.notes, c.pricing_profile_id, c.discount_pct,
  (SELECT count(*) FROM vehicles v WHERE v.customer_id = c.id), (SELECT count(*) FROM service_jobs j WHERE j.customer_id = c.id), c.created_at FROM customers c";

fn row(r: &rusqlite::Row) -> rusqlite::Result<Customer> {
    Ok(Customer {
        id: r.get(0)?,
        name: r.get(1)?,
        phone: r.get(2)?,
        email: r.get(3)?,
        address: r.get(4)?,
        notes: r.get(5)?,
        pricing_profile_id: r.get(6)?,
        discount_pct: r.get(7)?,
        vehicle_count: r.get(8)?,
        job_count: r.get(9)?,
        created_at: r.get(10)?,
    })
}

pub fn get(conn: &Connection, id: &str) -> AppResult<Customer> {
    conn.query_row(&format!("{SELECT} WHERE c.id = ?1"), [id], row).optional()?.ok_or_else(|| AppError::not_found("That customer"))
}

pub fn list(conn: &Connection, q: Option<&str>) -> AppResult<Vec<Customer>> {
    let like = q.map(str::trim).filter(|q| !q.is_empty()).map(|q| format!("%{}%", q.replace('%', "\\%")));
    let mut st = conn.prepare(&format!(
        "{SELECT} WHERE ?1 IS NULL OR c.name LIKE ?1 ESCAPE '\\' OR c.phone LIKE ?1 ESCAPE '\\' OR c.email LIKE ?1 ESCAPE '\\' ORDER BY c.name COLLATE NOCASE LIMIT 2000"
    ))?;
    let rows = st.query_map([like], row)?;
    Ok(rows.collect::<Result<_, _>>()?)
}

fn validate(conn: &Connection, c: &CustomerInput) -> AppResult<(String, Option<String>)> {
    let name = clean(&c.name);
    if name.is_empty() {
        return Err(AppError::invalid("name", "Enter the customer's name."));
    }
    if !c.email.trim().is_empty() && !c.email.contains('@') {
        return Err(AppError::invalid("email", "That email address doesn't look right."));
    }
    if let Some(p) = &c.pricing_profile_id
        && !db::exists(conn, "SELECT 1 FROM pricing_profiles WHERE id = ?1", [p])?
    {
        return Err(AppError::invalid("pricing_profile_id", "Choose a pricing profile from the list."));
    }
    let discount = c
        .discount_pct
        .as_deref()
        .map(str::trim)
        .filter(|d| !d.is_empty())
        .map(|d| {
            money::parse("discount_pct", "Discount", d).and_then(|x| money::pct_0_100("discount_pct", "Discount", x)).map(|x| x.to_string())
        })
        .transpose()?;
    Ok((name, discount))
}

pub fn create(conn: &Connection, c: &CustomerInput) -> AppResult<Customer> {
    let (name, discount) = validate(conn, c)?;
    let id = db::new_id();
    conn.execute(
        "INSERT INTO customers (id, name, phone, email, address, notes, pricing_profile_id, discount_pct) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![id, name, clean(&c.phone), c.email.trim(), c.address.trim(), c.notes.trim(), c.pricing_profile_id, discount],
    )?;
    db::audit(conn, "customer", Some(&id), "create", "Added customer", None)?;
    get(conn, &id)
}

pub fn update(conn: &Connection, id: &str, c: &CustomerInput) -> AppResult<Customer> {
    let (name, discount) = validate(conn, c)?;
    let n = conn.execute(
        "UPDATE customers SET name = ?2, phone = ?3, email = ?4, address = ?5, notes = ?6, pricing_profile_id = ?7, discount_pct = ?8, updated_at = ?9 WHERE id = ?1",
        params![id, name, clean(&c.phone), c.email.trim(), c.address.trim(), c.notes.trim(), c.pricing_profile_id, discount, db::now_utc()],
    )?;
    expect_one(n, "That customer")?;
    get(conn, id)
}

/// Delete a customer. Their vehicles and jobs stay, without the customer link.
pub fn delete(conn: &Connection, id: &str) -> AppResult<()> {
    let n = conn.execute("DELETE FROM customers WHERE id = ?1", [id])?;
    expect_one(n, "That customer")?;
    db::audit(conn, "customer", Some(id), "delete", "Deleted customer", None)?;
    Ok(())
}
