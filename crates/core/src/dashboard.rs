//! The dashboard: every number comes from a query over stored records.

use crate::error::AppResult;
use crate::{inventory, jobs, settings, vehicles};
use rusqlite::Connection;
use rust_decimal::Decimal;
use serde::Serialize;
use ts_rs::TS;

#[derive(Serialize, TS, Clone, Debug)]
#[ts(export)]
pub struct Dashboard {
    pub recent_vehicles: Vec<vehicles::Vehicle>,
    pub saved_vehicles: i64,
    pub item_count: i64,
    pub units_on_hand: crate::qty::Qty,
    #[ts(type = "string")]
    pub value_at_cost: Decimal,
    pub valuation_method: String,
    pub low_stock: Vec<inventory::Item>,
    pub low_stock_count: i64,
    pub out_of_stock_count: i64,
    /// Items at or below minimum stock with a reorder quantity set.
    pub to_reorder: i64,
    pub open_jobs: Vec<jobs::Job>,
    pub open_job_count: i64,
    pub recent_movements: Vec<inventory::Movement>,
    pub manuals: i64,
    pub parts: i64,
}

pub fn summary(conn: &Connection) -> AppResult<Dashboard> {
    let s = settings::load(conn);
    let val = inventory::valuation(conn, &s.valuation_method)?;
    let all = inventory::list(conn, &inventory::ItemQuery::default())?;
    let low: Vec<inventory::Item> = all.iter().filter(|i| i.min_stock > 0 && i.available <= i.min_stock).cloned().collect();
    let out_count = all.iter().filter(|i| i.available <= 0).count() as i64;
    let to_reorder = low.iter().filter(|i| i.reorder_qty > 0).count() as i64;
    let open = jobs::list(conn, &jobs::JobQuery { status: Some("open".into()), ..Default::default() })?;
    let count = |sql: &str| -> AppResult<i64> { Ok(conn.query_row(sql, [], |r| r.get(0))?) };
    Ok(Dashboard {
        recent_vehicles: vehicles::recent(conn, 6)?,
        saved_vehicles: count("SELECT count(*) FROM vehicles WHERE saved = 1")?,
        item_count: val.items,
        units_on_hand: val.units,
        value_at_cost: val.at_cost,
        valuation_method: val.method,
        low_stock_count: low.len() as i64,
        low_stock: low.into_iter().take(8).collect(),
        out_of_stock_count: out_count,
        to_reorder,
        open_job_count: open.len() as i64,
        open_jobs: open.into_iter().take(6).collect(),
        recent_movements: inventory::movements(conn, None, 8)?,
        manuals: count("SELECT count(*) FROM manuals")?,
        parts: count("SELECT count(*) FROM parts")?,
    })
}
