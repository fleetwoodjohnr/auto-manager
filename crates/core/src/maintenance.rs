//! Maintenance: service intervals the person records for a vehicle (ideally citing the owner's
//! manual), service records, and what is coming due. No generic schedule is shipped: intervals
//! differ by vehicle and only the manufacturer's schedule is authoritative.

use crate::db::{self, clean, expect_one};
use crate::error::{AppError, AppResult};
use crate::money;
use chrono::{Datelike, NaiveDate};
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Serialize, TS, Clone, Debug)]
#[ts(export)]
pub struct Interval {
    pub id: String,
    pub vehicle_id: String,
    pub service: String,
    pub every_miles: Option<i64>,
    pub every_months: Option<i64>,
    pub source_ref: String,
    pub notes: String,
    pub last_date: Option<String>,
    pub last_mileage: Option<i64>,
    pub due_date: Option<String>,
    pub due_mileage: Option<i64>,
    /// "overdue", "due_soon", "ok" or "never_done"
    pub state: String,
}

#[derive(Deserialize, TS, Clone, Debug)]
#[ts(export)]
pub struct IntervalInput {
    pub service: String,
    pub every_miles: Option<i64>,
    pub every_months: Option<i64>,
    #[serde(default)]
    pub source_ref: String,
    #[serde(default)]
    pub notes: String,
}

#[derive(Serialize, TS, Clone, Debug)]
#[ts(export, rename = "MaintenanceRecord")]
pub struct Record {
    pub id: String,
    pub vehicle_id: String,
    pub interval_id: Option<String>,
    pub job_id: Option<String>,
    pub date: String,
    pub mileage: Option<i64>,
    pub service: String,
    pub parts_used: String,
    pub labor: String,
    pub cost: Option<String>,
    pub provider: String,
    pub notes: String,
}

#[derive(Deserialize, TS, Clone, Debug)]
#[ts(export)]
pub struct RecordInput {
    pub interval_id: Option<String>,
    pub date: String,
    pub mileage: Option<i64>,
    pub service: String,
    #[serde(default)]
    pub parts_used: String,
    #[serde(default)]
    pub labor: String,
    pub cost: Option<String>,
    #[serde(default)]
    pub provider: String,
    #[serde(default)]
    pub notes: String,
}

const SOON_DAYS: i64 = 30;
const SOON_MILES: i64 = 1000;

fn add_months(d: NaiveDate, months: i64) -> NaiveDate {
    let total = d.year() as i64 * 12 + d.month0() as i64 + months;
    let (y, m) = ((total / 12) as i32, (total % 12) as u32 + 1);
    let last_day = (28..=31).rev().find(|day| NaiveDate::from_ymd_opt(y, m, *day).is_some()).unwrap_or(28);
    NaiveDate::from_ymd_opt(y, m, d.day().min(last_day)).unwrap_or(d)
}

fn validate_interval(i: &IntervalInput) -> AppResult<String> {
    let s = clean(&i.service);
    if s.is_empty() {
        return Err(AppError::invalid("service", "Name the service, e.g. Engine oil and filter."));
    }
    if i.every_miles.is_none() && i.every_months.is_none() {
        return Err(AppError::invalid("every_miles", "Enter a distance, a time, or both."));
    }
    if i.every_miles.is_some_and(|m| m <= 0) || i.every_months.is_some_and(|m| m <= 0) {
        return Err(AppError::invalid("every_miles", "Intervals must be more than zero."));
    }
    Ok(s)
}

pub fn add_interval(conn: &Connection, vehicle_id: &str, i: &IntervalInput) -> AppResult<String> {
    let s = validate_interval(i)?;
    let id = db::new_id();
    conn.execute(
        "INSERT INTO maintenance_intervals (id, vehicle_id, service, every_miles, every_months, source_ref, notes) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![id, vehicle_id, s, i.every_miles, i.every_months, i.source_ref.trim(), i.notes.trim()],
    )?;
    Ok(id)
}

pub fn update_interval(conn: &Connection, id: &str, i: &IntervalInput) -> AppResult<()> {
    let s = validate_interval(i)?;
    let n = conn.execute(
        "UPDATE maintenance_intervals SET service = ?2, every_miles = ?3, every_months = ?4, source_ref = ?5, notes = ?6 WHERE id = ?1",
        params![id, s, i.every_miles, i.every_months, i.source_ref.trim(), i.notes.trim()],
    )?;
    expect_one(n, "That interval")
}

pub fn delete_interval(conn: &Connection, id: &str) -> AppResult<()> {
    expect_one(conn.execute("DELETE FROM maintenance_intervals WHERE id = ?1", [id])?, "That interval")
}

/// Copy another vehicle's intervals (e.g. a fleet of the same model).
pub fn copy_intervals(conn: &Connection, from_vehicle: &str, to_vehicle: &str) -> AppResult<usize> {
    let n = conn.execute(
        "INSERT INTO maintenance_intervals (id, vehicle_id, service, every_miles, every_months, source_ref, notes)
         SELECT lower(hex(randomblob(16))), ?2, service, every_miles, every_months, source_ref, notes FROM maintenance_intervals WHERE vehicle_id = ?1",
        params![from_vehicle, to_vehicle],
    )?;
    Ok(n)
}

pub fn intervals(conn: &Connection, vehicle_id: &str, today: NaiveDate) -> AppResult<Vec<Interval>> {
    let current_mileage: Option<i64> =
        conn.query_row("SELECT mileage FROM vehicles WHERE id = ?1", [vehicle_id], |r| r.get(0)).optional()?.flatten();
    let mut st = conn.prepare("SELECT id, vehicle_id, service, every_miles, every_months, source_ref, notes FROM maintenance_intervals WHERE vehicle_id = ?1 ORDER BY service")?;
    type Row = (String, String, String, Option<i64>, Option<i64>, String, String);
    let base: Vec<Row> = st
        .query_map([vehicle_id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?, r.get(6)?)))?
        .collect::<Result<_, _>>()?;
    let mut out = Vec::new();
    for (id, vid, service, miles, months, source_ref, notes) in base {
        // The last time it was done: a record tied to this interval or with the same service name.
        let last: Option<(String, Option<i64>)> = conn
            .query_row(
                "SELECT date, mileage FROM maintenance_records WHERE vehicle_id = ?1 AND (interval_id = ?2 OR service = ?3 COLLATE NOCASE) ORDER BY date DESC, mileage DESC LIMIT 1",
                params![vid, id, service],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        let (due_date, due_mileage, state) = match &last {
            None => (None, None, "never_done".to_string()),
            Some((d, m)) => {
                let last_date = NaiveDate::parse_from_str(d, "%Y-%m-%d").ok();
                let due_date = months.zip(last_date).map(|(mo, ld)| add_months(ld, mo));
                let due_mileage = miles.zip(*m).map(|(mi, lm)| lm + mi);
                let overdue = due_date.is_some_and(|dd| dd <= today) || due_mileage.zip(current_mileage).is_some_and(|(dm, cm)| cm >= dm);
                let soon = due_date.is_some_and(|dd| (dd - today).num_days() <= SOON_DAYS)
                    || due_mileage.zip(current_mileage).is_some_and(|(dm, cm)| dm - cm <= SOON_MILES);
                (
                    due_date.map(|d| d.to_string()),
                    due_mileage,
                    if overdue {
                        "overdue"
                    } else if soon {
                        "due_soon"
                    } else {
                        "ok"
                    }
                    .to_string(),
                )
            }
        };
        out.push(Interval {
            id,
            vehicle_id: vid,
            service,
            every_miles: miles,
            every_months: months,
            source_ref,
            notes,
            last_date: last.as_ref().map(|l| l.0.clone()),
            last_mileage: last.and_then(|l| l.1),
            due_date,
            due_mileage,
            state,
        });
    }
    Ok(out)
}

fn validate_record(r: &RecordInput) -> AppResult<(String, Option<String>)> {
    if NaiveDate::parse_from_str(r.date.trim(), "%Y-%m-%d").is_err() {
        return Err(AppError::invalid("date", "Enter the date as YYYY-MM-DD."));
    }
    let s = clean(&r.service);
    if s.is_empty() {
        return Err(AppError::invalid("service", "Describe the service performed."));
    }
    if r.mileage.is_some_and(|m| m < 0) {
        return Err(AppError::invalid("mileage", "Mileage can't be negative."));
    }
    let cost = r
        .cost
        .as_deref()
        .map(str::trim)
        .filter(|c| !c.is_empty())
        .map(|c| money::parse("cost", "Cost", c).map(|d| d.to_string()))
        .transpose()?;
    Ok((s, cost))
}

pub fn add_record(conn: &Connection, vehicle_id: &str, r: &RecordInput) -> AppResult<String> {
    let (s, cost) = validate_record(r)?;
    let id = db::new_id();
    conn.execute(
        "INSERT INTO maintenance_records (id, vehicle_id, interval_id, date, mileage, service, parts_used, labor, cost, provider, notes) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
        params![id, vehicle_id, r.interval_id, r.date.trim(), r.mileage, s, r.parts_used.trim(), r.labor.trim(), cost, clean(&r.provider), r.notes.trim()],
    )?;
    // Keep the vehicle's odometer reading current.
    if let Some(m) = r.mileage {
        conn.execute("UPDATE vehicles SET mileage = ?2 WHERE id = ?1 AND (mileage IS NULL OR mileage < ?2)", params![vehicle_id, m])?;
    }
    Ok(id)
}

pub fn update_record(conn: &Connection, id: &str, r: &RecordInput) -> AppResult<()> {
    let (s, cost) = validate_record(r)?;
    let n = conn.execute(
        "UPDATE maintenance_records SET interval_id = ?2, date = ?3, mileage = ?4, service = ?5, parts_used = ?6, labor = ?7, cost = ?8, provider = ?9, notes = ?10 WHERE id = ?1",
        params![id, r.interval_id, r.date.trim(), r.mileage, s, r.parts_used.trim(), r.labor.trim(), cost, clean(&r.provider), r.notes.trim()],
    )?;
    expect_one(n, "That record")
}

pub fn delete_record(conn: &Connection, id: &str) -> AppResult<()> {
    expect_one(conn.execute("DELETE FROM maintenance_records WHERE id = ?1", [id])?, "That record")
}

pub fn records(conn: &Connection, vehicle_id: &str) -> AppResult<Vec<Record>> {
    let mut st = conn.prepare(
        "SELECT id, vehicle_id, interval_id, job_id, date, mileage, service, parts_used, labor, cost, provider, notes FROM maintenance_records WHERE vehicle_id = ?1 ORDER BY date DESC, mileage DESC",
    )?;
    let rows = st.query_map([vehicle_id], |r| {
        Ok(Record {
            id: r.get(0)?,
            vehicle_id: r.get(1)?,
            interval_id: r.get(2)?,
            job_id: r.get(3)?,
            date: r.get(4)?,
            mileage: r.get(5)?,
            service: r.get(6)?,
            parts_used: r.get(7)?,
            labor: r.get(8)?,
            cost: r.get(9)?,
            provider: r.get(10)?,
            notes: r.get(11)?,
        })
    })?;
    Ok(rows.collect::<Result<_, _>>()?)
}

/// Record the work on a completed job in the vehicle's maintenance history.
pub fn record_from_job(conn: &Connection, job_id: &str) -> AppResult<Option<String>> {
    let j = crate::jobs::detail(conn, job_id)?;
    let Some(vid) = &j.job.vehicle_id else { return Ok(None) };
    if db::exists(conn, "SELECT 1 FROM maintenance_records WHERE job_id = ?1", [job_id])? {
        return Ok(None);
    }
    let parts: Vec<String> =
        j.lines.iter().filter(|l| l.kind == "part").map(|l| format!("{} × {}", l.qty.normalize(), l.description)).collect();
    let labor: Vec<String> =
        j.lines.iter().filter(|l| l.kind == "labor").map(|l| format!("{} ({} h)", l.description, l.qty.normalize())).collect();
    let date = j.job.completed_at.as_deref().unwrap_or(&j.job.created_at).get(..10).unwrap_or_default().to_string();
    let id = db::new_id();
    conn.execute(
        "INSERT INTO maintenance_records (id, vehicle_id, job_id, date, mileage, service, parts_used, labor, cost, provider, notes) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, '', ?10)",
        params![
            id,
            vid,
            job_id,
            date,
            j.job.mileage,
            if j.job.title.is_empty() { format!("Service job #{}", j.job.number) } else { j.job.title.clone() },
            parts.join("; "),
            labor.join("; "),
            j.totals.total.to_string(),
            j.job.complaint
        ],
    )?;
    Ok(Some(id))
}

#[derive(Serialize, TS, Clone, Debug)]
#[ts(export)]
pub struct DueItem {
    pub vehicle_id: String,
    pub vehicle_title: String,
    pub service: String,
    pub due_date: Option<String>,
    pub due_mileage: Option<i64>,
    pub overdue: bool,
}

/// Saved vehicles' services that are overdue or due within 30 days / 1,000 miles.
pub fn due(conn: &Connection, today: NaiveDate) -> AppResult<Vec<DueItem>> {
    let mut st = conn.prepare("SELECT DISTINCT v.id, trim(coalesce(v.year, '') || ' ' || v.make || ' ' || v.model) FROM vehicles v JOIN maintenance_intervals i ON i.vehicle_id = v.id WHERE v.saved = 1")?;
    let vs: Vec<(String, String)> = st.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?.collect::<Result<_, _>>()?;
    let mut out = Vec::new();
    for (vid, title) in vs {
        for i in intervals(conn, &vid, today)? {
            if i.state == "overdue" || i.state == "due_soon" {
                out.push(DueItem {
                    vehicle_id: vid.clone(),
                    vehicle_title: title.clone(),
                    service: i.service,
                    due_date: i.due_date,
                    due_mileage: i.due_mileage,
                    overdue: i.state == "overdue",
                });
            }
        }
    }
    out.sort_by(|a, b| b.overdue.cmp(&a.overdue).then(a.due_date.cmp(&b.due_date)));
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn due_by_time_or_distance() {
        let conn = db::open_memory().unwrap();
        conn.execute("INSERT INTO vehicles (id, year, make, model, saved, mileage) VALUES ('v1', 2019, 'Nissan', 'Sentra', 1, 40000)", [])
            .unwrap();
        let oil = add_interval(
            &conn,
            "v1",
            &IntervalInput {
                service: "Engine oil and filter".into(),
                every_miles: Some(5000),
                every_months: Some(6),
                source_ref: "Owner's manual p. 300".into(),
                notes: String::new(),
            },
        )
        .unwrap();
        let today = NaiveDate::from_ymd_opt(2026, 11, 14).unwrap();
        assert_eq!(intervals(&conn, "v1", today).unwrap()[0].state, "never_done");
        add_record(
            &conn,
            "v1",
            &RecordInput {
                interval_id: Some(oil.clone()),
                date: "2026-08-31".into(),
                mileage: Some(36000),
                service: "Oil change".into(),
                parts_used: String::new(),
                labor: String::new(),
                cost: Some("49.95".into()),
                provider: "Me".into(),
                notes: String::new(),
            },
        )
        .unwrap();
        let i = &intervals(&conn, "v1", today).unwrap()[0];
        assert_eq!(i.due_date.as_deref(), Some("2027-02-28"), "month-end is clamped");
        assert_eq!(i.due_mileage, Some(41000));
        assert_eq!(i.state, "due_soon", "within 1,000 miles");
        conn.execute("UPDATE vehicles SET mileage = 41500 WHERE id = 'v1'", []).unwrap();
        assert_eq!(intervals(&conn, "v1", today).unwrap()[0].state, "overdue");
        assert_eq!(due(&conn, today).unwrap().len(), 1);
        assert!(
            add_interval(
                &conn,
                "v1",
                &IntervalInput {
                    service: "X".into(),
                    every_miles: None,
                    every_months: None,
                    source_ref: String::new(),
                    notes: String::new()
                }
            )
            .is_err()
        );
    }

    #[test]
    fn records_raise_odometer() {
        let conn = db::open_memory().unwrap();
        conn.execute("INSERT INTO vehicles (id, mileage) VALUES ('v1', 1000)", []).unwrap();
        add_record(
            &conn,
            "v1",
            &RecordInput {
                interval_id: None,
                date: "2026-01-01".into(),
                mileage: Some(1500),
                service: "Tires rotated".into(),
                parts_used: String::new(),
                labor: String::new(),
                cost: None,
                provider: String::new(),
                notes: String::new(),
            },
        )
        .unwrap();
        let m: i64 = conn.query_row("SELECT mileage FROM vehicles", [], |r| r.get(0)).unwrap();
        assert_eq!(m, 1500);
        assert!(
            add_record(
                &conn,
                "v1",
                &RecordInput {
                    interval_id: None,
                    date: "01/01/2026".into(),
                    mileage: None,
                    service: "x".into(),
                    parts_used: String::new(),
                    labor: String::new(),
                    cost: None,
                    provider: String::new(),
                    notes: String::new()
                }
            )
            .is_err()
        );
    }
}
