//! NHTSA recalls by model year, make and model (https://api.nhtsa.gov/recalls/).
//!
//! These are campaigns that *potentially* apply: NHTSA's public API answers by model, not by
//! VIN. Whether a campaign is still open for one vehicle is only known from a per-VIN check on
//! nhtsa.gov/recalls (or with the manufacturer); the person records that answer here.

use super::{Freshness, seg};
use crate::db;
use crate::error::{AppError, AppResult};
use crate::state::Core;
use crate::vehicles;
use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

pub const SOURCE: &str = "nhtsa_recalls";
const WHO: &str = "NHTSA Recalls";
const TTL_HOURS: i64 = 24 * 7;
pub const VIN_CHECK_URL: &str = "https://www.nhtsa.gov/recalls";

#[derive(Serialize, TS, Clone, Debug, PartialEq)]
#[ts(export)]
pub struct Recall {
    pub campaign: String,
    pub manufacturer: String,
    pub component: String,
    pub summary: String,
    pub consequence: String,
    pub remedy: String,
    pub notes: String,
    /// YYYY-MM-DD
    pub report_date: Option<String>,
    pub park_it: bool,
    pub park_outside: bool,
    pub over_the_air: bool,
    /// What the person recorded after checking this VIN: outstanding, completed or not_applicable.
    pub status: Option<String>,
    pub status_note: String,
}

#[derive(Serialize, TS, Clone, Debug)]
#[ts(export)]
pub struct VehicleRecalls {
    pub vehicle_key: String,
    pub recalls: Vec<Recall>,
    pub freshness: Option<Freshness>,
    pub check_url: String,
}

#[derive(Deserialize)]
struct Raw {
    #[serde(rename = "NHTSACampaignNumber", default)]
    campaign: String,
    #[serde(rename = "Manufacturer", default)]
    manufacturer: String,
    #[serde(rename = "Component", default)]
    component: String,
    #[serde(rename = "Summary", default)]
    summary: String,
    #[serde(rename = "Consequence", default)]
    consequence: String,
    #[serde(rename = "Remedy", default)]
    remedy: String,
    #[serde(rename = "Notes", default)]
    notes: String,
    #[serde(rename = "ReportReceivedDate", default)]
    date: String,
    #[serde(rename = "parkIt", default)]
    park_it: bool,
    #[serde(rename = "parkOutSide", default)]
    park_outside: bool,
    #[serde(rename = "overTheAirUpdate", default)]
    ota: bool,
}

#[derive(Deserialize)]
struct Envelope {
    #[serde(default)]
    results: Vec<Raw>,
}

/// NHTSA's recall dates are DD/MM/YYYY.
fn iso(d: &str) -> Option<String> {
    let p: Vec<&str> = d.trim().split('/').collect();
    match p.as_slice() {
        [dd, mm, yyyy] if yyyy.len() == 4 => Some(format!("{yyyy}-{mm:0>2}-{dd:0>2}")),
        _ => None,
    }
}

pub fn parse(body: &str) -> AppResult<Vec<Recall>> {
    let env: Envelope = serde_json::from_str(body)
        .map_err(|_| AppError::Provider { message: format!("{WHO} sent a response Auto Manager couldn't read."), offline: false })?;
    let mut out: Vec<Recall> = env
        .results
        .into_iter()
        .filter(|r| !r.campaign.is_empty())
        .map(|r| Recall {
            campaign: r.campaign,
            manufacturer: r.manufacturer,
            component: r.component,
            summary: r.summary,
            consequence: r.consequence,
            remedy: r.remedy,
            notes: r.notes,
            report_date: iso(&r.date),
            park_it: r.park_it,
            park_outside: r.park_outside,
            over_the_air: r.ota,
            status: None,
            status_note: String::new(),
        })
        .collect();
    out.sort_by(|a, b| b.report_date.cmp(&a.report_date));
    out.dedup_by(|a, b| a.campaign == b.campaign);
    Ok(out)
}

/// The year/make/model a vehicle's recalls are filed under, from its effective specs.
pub fn vehicle_key(conn: &Connection, vehicle_id: &str) -> AppResult<Option<(i32, String, String)>> {
    let v = vehicles::get(conn, vehicle_id)?;
    let eff = vehicles::effective(conn, vehicle_id)?;
    let make = eff.get("make").cloned().unwrap_or(v.make);
    let model = eff.get("model").cloned().unwrap_or(v.model);
    Ok(match v.year {
        Some(y) if !make.is_empty() && !model.is_empty() => Some((y, make, model)),
        _ => None,
    })
}

fn key(y: i32, make: &str, model: &str) -> String {
    format!("{y}|{}|{}", make.trim().to_uppercase(), model.trim().to_uppercase())
}

fn stored(conn: &Connection, vehicle_id: &str, k: &str) -> AppResult<Vec<Recall>> {
    let mut st = conn.prepare(
        "SELECT r.campaign, r.manufacturer, r.component, r.summary, r.consequence, r.remedy, r.notes, r.report_date, r.park_it, r.park_outside, r.over_the_air,
                s.status, coalesce(s.note, '')
         FROM recalls r LEFT JOIN recall_status s ON s.campaign = r.campaign AND s.vehicle_id = ?2
         WHERE r.vehicle_key = ?1 ORDER BY r.report_date DESC",
    )?;
    let rows = st.query_map(params![k, vehicle_id], |r| {
        Ok(Recall {
            campaign: r.get(0)?,
            manufacturer: r.get(1)?,
            component: r.get(2)?,
            summary: r.get(3)?,
            consequence: r.get(4)?,
            remedy: r.get(5)?,
            notes: r.get(6)?,
            report_date: r.get(7)?,
            park_it: r.get::<_, i64>(8)? == 1,
            park_outside: r.get::<_, i64>(9)? == 1,
            over_the_air: r.get::<_, i64>(10)? == 1,
            status: r.get(11)?,
            status_note: r.get(12)?,
        })
    })?;
    Ok(rows.collect::<Result<_, _>>()?)
}

/// Recalls saved earlier (offline-capable), without contacting NHTSA.
pub fn saved_for_vehicle(conn: &Connection, vehicle_id: &str) -> AppResult<VehicleRecalls> {
    let k = vehicle_key(conn, vehicle_id)?.map(|(y, m, mo)| key(y, &m, &mo)).unwrap_or_default();
    Ok(VehicleRecalls { recalls: stored(conn, vehicle_id, &k)?, vehicle_key: k, freshness: None, check_url: VIN_CHECK_URL.into() })
}

pub async fn for_vehicle(core: &Core, vehicle_id: &str) -> AppResult<VehicleRecalls> {
    let Some((y, make, model)) = core.read(|c| vehicle_key(c, vehicle_id))? else {
        return Err(AppError::msg("Recalls are listed by model year, make and model; add those to this vehicle first."));
    };
    let url = format!("{}/recalls/recallsByVehicle?make={}&model={}&modelYear={y}", core.endpoints.nhtsa, seg(&make), seg(&model));
    let f = core.cached_text(SOURCE, &url, WHO, TTL_HOURS).await?;
    let list = parse(&f.value)?;
    let k = key(y, &make, &model);
    core.tx(|c| {
        c.execute("DELETE FROM recalls WHERE vehicle_key = ?1", [&k])?;
        let mut ins = c.prepare(
            "INSERT OR REPLACE INTO recalls (vehicle_key, campaign, manufacturer, component, summary, consequence, remedy, notes, report_date, park_it, park_outside, over_the_air, fetched_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
        )?;
        for r in &list {
            ins.execute(params![k, r.campaign, r.manufacturer, r.component, r.summary, r.consequence, r.remedy, r.notes, r.report_date, r.park_it, r.park_outside, r.over_the_air, f.freshness.fetched_at])?;
        }
        Ok(())
    })?;
    let recalls = core.read(|c| stored(c, vehicle_id, &k))?;
    Ok(VehicleRecalls { vehicle_key: k, recalls, freshness: Some(f.freshness), check_url: VIN_CHECK_URL.into() })
}

/// Record the result of a per-VIN check for one campaign (None clears it).
pub fn set_status(conn: &Connection, vehicle_id: &str, campaign: &str, status: Option<&str>, note: &str) -> AppResult<()> {
    match status {
        None => {
            conn.execute("DELETE FROM recall_status WHERE vehicle_id = ?1 AND campaign = ?2", params![vehicle_id, campaign])?;
        }
        Some(s) => {
            if !matches!(s, "outstanding" | "completed" | "not_applicable") {
                return Err(AppError::invalid("status", "Choose outstanding, completed or not applicable."));
            }
            conn.execute(
                "INSERT INTO recall_status (vehicle_id, campaign, status, note, updated_at) VALUES (?1, ?2, ?3, ?4, ?5)
                 ON CONFLICT (vehicle_id, campaign) DO UPDATE SET status = excluded.status, note = excluded.note, updated_at = excluded.updated_at",
                params![vehicle_id, campaign, s, note.trim(), db::now_utc()],
            )?;
        }
    }
    Ok(())
}

#[derive(Serialize, TS, Clone, Debug)]
#[ts(export)]
pub struct RecallNotice {
    pub vehicle_id: String,
    pub vehicle_title: String,
    /// Campaigns not yet recorded as completed or not applicable.
    pub count: i64,
}

/// Saved vehicles with recall campaigns that haven't been checked off.
pub fn notices(conn: &Connection) -> AppResult<Vec<RecallNotice>> {
    let mut st = conn.prepare(
        "SELECT v.id, trim(coalesce(v.year, '') || ' ' || v.make || ' ' || v.model || ' ' || v.trim),
           (SELECT count(*) FROM recalls r WHERE r.vehicle_key = v.year || '|' || upper(coalesce(m.value, v.make)) || '|' || upper(coalesce(mo.value, v.model))
              AND NOT EXISTS (SELECT 1 FROM recall_status s WHERE s.vehicle_id = v.id AND s.campaign = r.campaign AND s.status IN ('completed', 'not_applicable')))
         FROM vehicles v
         LEFT JOIN vehicle_effective_specs m ON m.vehicle_id = v.id AND m.key = 'make'
         LEFT JOIN vehicle_effective_specs mo ON mo.vehicle_id = v.id AND mo.key = 'model'
         WHERE v.saved = 1",
    )?;
    let rows = st.query_map([], |r| Ok(RecallNotice { vehicle_id: r.get(0)?, vehicle_title: r.get(1)?, count: r.get(2)? }))?;
    let mut out: Vec<RecallNotice> = rows.collect::<Result<_, _>>()?;
    out.retain(|n| n.count > 0);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(n: &str) -> String {
        std::fs::read_to_string(format!("{}/tests/fixtures/nhtsa/{n}", env!("CARGO_MANIFEST_DIR"))).unwrap()
    }

    #[test]
    fn parses_and_converts_dates() {
        let r = parse(&fixture("recalls_2019_nissan_sentra.json")).unwrap();
        assert_eq!(r.len(), 2);
        assert_eq!(r[0].campaign, "21V135000");
        assert_eq!(r[0].report_date.as_deref(), Some("2021-03-03"));
        let honda = parse(&fixture("recalls_2003_honda_accord.json")).unwrap();
        assert!(honda.iter().any(|x| x.report_date.as_deref() == Some("2019-06-27")), "27/06/2019 is day-first");
        assert!(parse("<html>").is_err());
    }

    #[test]
    fn statuses_and_notices() {
        let conn = db::open_memory().unwrap();
        conn.execute("INSERT INTO vehicles (id, year, make, model, saved) VALUES ('v1', 2019, 'Nissan', 'Sentra', 1)", []).unwrap();
        let k = key(2019, "Nissan", "Sentra");
        for r in parse(&fixture("recalls_2019_nissan_sentra.json")).unwrap() {
            conn.execute("INSERT INTO recalls (vehicle_key, campaign, fetched_at) VALUES (?1, ?2, 'x')", params![k, r.campaign]).unwrap();
        }
        assert_eq!(notices(&conn).unwrap()[0].count, 2);
        set_status(&conn, "v1", "21V135000", Some("completed"), "Dealer 2022").unwrap();
        assert_eq!(notices(&conn).unwrap()[0].count, 1);
        let saved = saved_for_vehicle(&conn, "v1").unwrap();
        assert_eq!(saved.recalls.iter().find(|r| r.campaign == "21V135000").unwrap().status.as_deref(), Some("completed"));
        assert!(set_status(&conn, "v1", "x", Some("maybe"), "").is_err());
    }
}
