//! Saved and recently looked-up vehicles, and their specifications with provenance.

use crate::db::{self, clean, expect_one};
use crate::error::{AppError, AppResult};
use crate::providers::vpic::Decoded;
use crate::specs;
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use ts_rs::TS;

/// Recently looked-up (unsaved) vehicles kept for the dashboard; older ones are removed.
const RECENT_KEEP: i64 = 50;

#[derive(Serialize, TS, Clone, Debug)]
#[ts(export)]
pub struct Vehicle {
    pub id: String,
    pub vin: Option<String>,
    pub year: Option<i32>,
    pub make: String,
    pub model: String,
    pub trim: String,
    pub engine: String,
    pub transmission: String,
    pub nickname: String,
    pub color: String,
    pub plate: String,
    pub mileage: Option<i64>,
    pub mileage_unit: String,
    pub purchase_date: Option<String>,
    pub purchase_price: Option<String>,
    pub notes: String,
    pub customer_id: Option<String>,
    pub saved: bool,
    pub decode_note: String,
    pub last_viewed_at: String,
    pub created_at: String,
}

impl Vehicle {
    pub fn title(&self) -> String {
        let y = self.year.map(|y| y.to_string()).unwrap_or_default();
        clean(&format!("{y} {} {} {}", self.make, self.model, self.trim))
    }
}

const COLS: &str = "id, vin, year, make, model, trim, engine, transmission, nickname, color, plate, mileage, mileage_unit, purchase_date, purchase_price, notes, customer_id, saved, decode_note, last_viewed_at, created_at";

fn row(r: &rusqlite::Row) -> rusqlite::Result<Vehicle> {
    Ok(Vehicle {
        id: r.get("id")?,
        vin: r.get("vin")?,
        year: r.get("year")?,
        make: r.get("make")?,
        model: r.get("model")?,
        trim: r.get("trim")?,
        engine: r.get("engine")?,
        transmission: r.get("transmission")?,
        nickname: r.get("nickname")?,
        color: r.get("color")?,
        plate: r.get("plate")?,
        mileage: r.get("mileage")?,
        mileage_unit: r.get("mileage_unit")?,
        purchase_date: r.get("purchase_date")?,
        purchase_price: r.get("purchase_price")?,
        notes: r.get("notes")?,
        customer_id: r.get("customer_id")?,
        saved: r.get::<_, i64>("saved")? == 1,
        decode_note: r.get("decode_note")?,
        last_viewed_at: r.get("last_viewed_at")?,
        created_at: r.get("created_at")?,
    })
}

#[derive(Serialize, TS, Clone, Debug)]
#[ts(export)]
pub struct SpecRow {
    pub key: String,
    pub value: String,
    pub unit: Option<String>,
    pub source_id: String,
    pub source_name: String,
    pub confidence: String,
    pub source_ref: Option<String>,
    pub fetched_at: String,
    /// This row is the one in effect for its key.
    pub effective: bool,
}

#[derive(Serialize, TS, Clone, Debug)]
#[ts(export)]
pub struct VehicleDetail {
    pub vehicle: Vehicle,
    pub specs: Vec<SpecRow>,
    /// Keys where two sources give different values.
    pub conflicts: Vec<String>,
}

pub fn get(conn: &Connection, id: &str) -> AppResult<Vehicle> {
    conn.query_row(&format!("SELECT {COLS} FROM vehicles WHERE id = ?1"), [id], row)
        .optional()?
        .ok_or_else(|| AppError::not_found("That vehicle"))
}

pub fn by_vin(conn: &Connection, vin: &str) -> AppResult<Option<Vehicle>> {
    Ok(conn.query_row(&format!("SELECT {COLS} FROM vehicles WHERE vin = ?1"), [vin], row).optional()?)
}

pub fn detail(conn: &Connection, id: &str) -> AppResult<VehicleDetail> {
    let vehicle = get(conn, id)?;
    let mut st = conn.prepare(
        "SELECT s.key, s.value, s.unit, s.source_id, d.name, s.confidence, s.source_ref, s.fetched_at,
                EXISTS (SELECT 1 FROM vehicle_effective_specs e WHERE e.vehicle_id = s.vehicle_id AND e.key = s.key AND e.source_id = s.source_id)
         FROM vehicle_specifications s JOIN data_sources d ON d.id = s.source_id
         WHERE s.vehicle_id = ?1 ORDER BY s.key, d.priority",
    )?;
    let specs: Vec<SpecRow> = st
        .query_map([id], |r| {
            Ok(SpecRow {
                key: r.get(0)?,
                value: r.get(1)?,
                unit: r.get(2)?,
                source_id: r.get(3)?,
                source_name: r.get(4)?,
                confidence: r.get(5)?,
                source_ref: r.get(6)?,
                fetched_at: r.get(7)?,
                effective: r.get(8)?,
            })
        })?
        .collect::<Result<_, _>>()?;
    let mut by_key: HashMap<&str, Vec<&SpecRow>> = HashMap::new();
    for s in &specs {
        by_key.entry(&s.key).or_default().push(s);
    }
    let mut conflicts: Vec<String> = by_key
        .into_iter()
        .filter(|(_, rows)| {
            // A user's own correction is a decision, not a conflict; disagreeing providers are.
            let provider: Vec<&&SpecRow> = rows.iter().filter(|r| r.source_id != "user").collect();
            provider.windows(2).any(|w| !same_value(&w[0].value, &w[1].value))
        })
        .map(|(k, _)| k.to_string())
        .collect();
    conflicts.sort();
    Ok(VehicleDetail { vehicle, specs, conflicts })
}

fn same_value(a: &str, b: &str) -> bool {
    match (a.parse::<f64>(), b.parse::<f64>()) {
        (Ok(x), Ok(y)) => (x - y).abs() < 0.05,
        _ => {
            let (al, bl) = (a.trim().to_lowercase(), b.trim().to_lowercase());
            // "Gasoline" and "Regular Gasoline" say the same thing.
            if al == bl || (!al.is_empty() && !bl.is_empty() && (al.contains(&bl) || bl.contains(&al))) {
                return true;
            }
            // "4x2" and "Front-Wheel Drive" don't contradict each other.
            let (x, y) = (crate::fitment::drive_set(a), crate::fitment::drive_set(b));
            !x.is_empty() && !y.is_empty() && (x.iter().all(|d| y.contains(d)) || y.iter().all(|d| x.contains(d)))
        }
    }
}

/// The effective value per key for one vehicle.
pub fn effective(conn: &Connection, id: &str) -> AppResult<HashMap<String, String>> {
    let mut st = conn.prepare("SELECT key, value FROM vehicle_effective_specs WHERE vehicle_id = ?1")?;
    let rows = st.query_map([id], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
    Ok(rows.collect::<Result<_, _>>()?)
}

/// Make names come back from NHTSA in capitals; show "Honda", keep "BMW" and "GMC".
pub fn display_make(make: &str) -> String {
    make.split(' ')
        .map(|w| {
            if w.len() <= 3 && !w.contains('-') {
                return w.to_string();
            }
            w.split('-')
                .map(|p| {
                    let mut c = p.chars();
                    match c.next() {
                        Some(f) => f.to_uppercase().collect::<String>() + &c.as_str().to_lowercase(),
                        None => String::new(),
                    }
                })
                .collect::<Vec<_>>()
                .join("-")
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Refresh the identity columns cached on `vehicles` from the effective specifications.
pub fn refresh_identity(conn: &Connection, id: &str) -> AppResult<()> {
    let eff = effective(conn, id)?;
    let get = |k: &str| eff.get(k).cloned();
    let year: Option<i32> = get("model_year").and_then(|y| y.trim().parse().ok());
    conn.execute(
        "UPDATE vehicles SET year = ?2, make = ?3, model = ?4, trim = ?5, engine = ?6, transmission = ?7, updated_at = ?8 WHERE id = ?1",
        params![
            id,
            year,
            display_make(&get("make").unwrap_or_default()),
            get("model").unwrap_or_default(),
            get("trim").unwrap_or_default(),
            specs::engine_summary(&get),
            specs::transmission_summary(&get),
            db::now_utc()
        ],
    )?;
    Ok(())
}

/// Store a decode result: find or create the vehicle by VIN, replace this source's rows, and
/// leave the user's own corrections untouched.
pub fn store_decoded(conn: &Connection, d: &Decoded, source_id: &str) -> AppResult<Vehicle> {
    let now = db::now_utc();
    let vin = d.vin.to_uppercase();
    let full_vin = vin.len() == 17 && !vin.contains('*');
    let existing = if full_vin { by_vin(conn, &vin)? } else { None };
    let id = match existing {
        Some(v) => {
            conn.execute("UPDATE vehicles SET last_viewed_at = ?2, decode_note = ?3 WHERE id = ?1", params![v.id, now, d.message])?;
            v.id
        }
        None => {
            let id = db::new_id();
            conn.execute(
                "INSERT INTO vehicles (id, vin, decode_note, last_viewed_at) VALUES (?1, ?2, ?3, ?4)",
                params![id, full_vin.then_some(&vin), d.message, now],
            )?;
            id
        }
    };
    store_values(conn, &id, source_id, "vin_decoded", &d.values)?;
    prune_recent(conn)?;
    get(conn, &id)
}

/// Replace one source's values for a vehicle (the user's own rows are never touched).
pub fn store_values(
    conn: &Connection,
    vehicle_id: &str,
    source_id: &str,
    confidence: &str,
    values: &[crate::providers::vpic::DecodedValue],
) -> AppResult<()> {
    if source_id == "user" {
        return Err(AppError::Other("Provider values can't be stored as the user's own.".into()));
    }
    let now = db::now_utc();
    conn.execute("DELETE FROM vehicle_specifications WHERE vehicle_id = ?1 AND source_id = ?2", params![vehicle_id, source_id])?;
    let mut ins = conn.prepare(
        "INSERT OR REPLACE INTO vehicle_specifications (vehicle_id, key, value, unit, source_id, confidence, source_ref, fetched_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
    )?;
    for v in values {
        ins.execute(params![vehicle_id, v.key, v.value, v.unit, source_id, confidence, v.source_ref, now])?;
    }
    refresh_identity(conn, vehicle_id)?;
    Ok(())
}

fn prune_recent(conn: &Connection) -> AppResult<()> {
    conn.execute(
        "DELETE FROM vehicles WHERE saved = 0 AND customer_id IS NULL AND id NOT IN (SELECT vehicle_id FROM service_jobs WHERE vehicle_id IS NOT NULL)
           AND id NOT IN (SELECT id FROM vehicles WHERE saved = 0 ORDER BY last_viewed_at DESC LIMIT ?1)",
        [RECENT_KEEP],
    )?;
    Ok(())
}

#[derive(Deserialize, TS, Clone, Debug, Default)]
#[ts(export)]
pub struct ManualVehicle {
    pub vin: Option<String>,
    pub year: Option<i32>,
    pub make: String,
    pub model: String,
    pub trim: Option<String>,
    /// Further specifications the person knows, e.g. engine_displacement_l = "2.0".
    #[serde(default)]
    pub specs: Vec<(String, String)>,
}

/// A vehicle identified by hand (older or non-standard vehicles, or no network).
pub fn create_manual(conn: &Connection, m: &ManualVehicle) -> AppResult<Vehicle> {
    let make = clean(&m.make);
    let model = clean(&m.model);
    if make.is_empty() {
        return Err(AppError::invalid("make", "Enter the make."));
    }
    if model.is_empty() {
        return Err(AppError::invalid("model", "Enter the model."));
    }
    if let Some(y) = m.year
        && !(1886..=2100).contains(&y)
    {
        return Err(AppError::invalid("year", "Enter a model year between 1886 and 2100."));
    }
    let vin = m.vin.as_deref().map(crate::vin::normalize).filter(|v| !v.is_empty());
    if let Some(v) = &vin {
        if v.len() > 17 || !v.chars().all(|c| c.is_ascii_alphanumeric()) {
            return Err(AppError::invalid("vin", "That VIN has characters a VIN can't contain."));
        }
        if by_vin(conn, v)?.is_some() {
            return Err(AppError::Conflict("A vehicle with that VIN is already saved.".into()));
        }
    }
    let id = db::new_id();
    conn.execute("INSERT INTO vehicles (id, vin, saved) VALUES (?1, ?2, 1)", params![id, vin])?;
    let mut all: Vec<(String, String)> = vec![("make".into(), make), ("model".into(), model)];
    if let Some(y) = m.year {
        all.push(("model_year".into(), y.to_string()));
    }
    if let Some(t) = m.trim.as_deref().map(clean).filter(|t| !t.is_empty()) {
        all.push(("trim".into(), t));
    }
    all.extend(m.specs.iter().cloned());
    for (k, v) in all {
        set_spec_inner(conn, &id, &k, &v, None, "user_entered", None)?;
    }
    refresh_identity(conn, &id)?;
    db::audit(conn, "vehicle", Some(&id), "create", "Vehicle added by hand", None)?;
    get(conn, &id)
}

fn set_spec_inner(
    conn: &Connection,
    id: &str,
    key: &str,
    value: &str,
    unit: Option<&str>,
    confidence: &str,
    source_ref: Option<&str>,
) -> AppResult<()> {
    if !specs::valid_key(key) {
        return Err(AppError::invalid("key", "That specification name isn't valid."));
    }
    let value = value.trim();
    if value.is_empty() {
        return Err(AppError::invalid("value", "Enter a value."));
    }
    if value.len() > 500 {
        return Err(AppError::invalid("value", "That value is too long."));
    }
    let unit = unit.or_else(|| specs::field(key).and_then(|f| f.unit));
    conn.execute(
        "INSERT INTO vehicle_specifications (vehicle_id, key, value, unit, source_id, confidence, source_ref, fetched_at)
         VALUES (?1, ?2, ?3, ?4, 'user', ?5, ?6, ?7)
         ON CONFLICT (vehicle_id, key, source_id) DO UPDATE SET value = excluded.value, unit = excluded.unit,
           confidence = excluded.confidence, source_ref = excluded.source_ref, fetched_at = excluded.fetched_at",
        params![id, key, value, unit, confidence, source_ref, db::now_utc()],
    )?;
    Ok(())
}

#[derive(Deserialize, TS, Clone, Debug)]
#[ts(export)]
pub struct SpecInput {
    pub key: String,
    pub value: String,
    pub unit: Option<String>,
    /// e.g. "Owner's manual p. 212": stored with the value so it can be checked later.
    pub source_ref: Option<String>,
}

/// Add or correct a specification. Stored as the user's own row; provider rows are kept.
pub fn set_spec(conn: &Connection, vehicle_id: &str, s: &SpecInput) -> AppResult<VehicleDetail> {
    get(conn, vehicle_id)?;
    let reference = s.source_ref.as_deref().map(str::trim).filter(|r| !r.is_empty());
    let confidence = if reference.is_some() { "manual_ref" } else { "user_entered" };
    set_spec_inner(conn, vehicle_id, &s.key, &s.value, s.unit.as_deref(), confidence, reference)?;
    refresh_identity(conn, vehicle_id)?;
    db::audit(
        conn,
        "vehicle",
        Some(vehicle_id),
        "set_spec",
        &format!("Set {}", s.key),
        Some(serde_json::json!({"key": s.key, "value": s.value})),
    )?;
    detail(conn, vehicle_id)
}

/// Remove the user's own value for a key, so the provider's value (if any) applies again.
pub fn clear_user_spec(conn: &Connection, vehicle_id: &str, key: &str) -> AppResult<VehicleDetail> {
    conn.execute("DELETE FROM vehicle_specifications WHERE vehicle_id = ?1 AND key = ?2 AND source_id = 'user'", params![vehicle_id, key])?;
    refresh_identity(conn, vehicle_id)?;
    detail(conn, vehicle_id)
}

#[derive(Deserialize, TS, Clone, Debug)]
#[ts(export)]
pub struct VehicleUpdate {
    pub nickname: String,
    pub color: String,
    pub plate: String,
    pub mileage: Option<i64>,
    pub mileage_unit: String,
    pub purchase_date: Option<String>,
    pub purchase_price: Option<String>,
    pub notes: String,
    pub customer_id: Option<String>,
}

pub fn update(conn: &Connection, id: &str, u: &VehicleUpdate) -> AppResult<Vehicle> {
    if !matches!(u.mileage_unit.as_str(), "mi" | "km") {
        return Err(AppError::invalid("mileage_unit", "Choose miles or kilometres."));
    }
    if u.mileage.is_some_and(|m| m < 0) {
        return Err(AppError::invalid("mileage", "Mileage can't be negative."));
    }
    let price = u.purchase_price.as_deref().map(str::trim).filter(|p| !p.is_empty());
    let price = price.map(|p| crate::money::parse("purchase_price", "Purchase price", p)).transpose()?;
    let date = u.purchase_date.as_deref().map(str::trim).filter(|d| !d.is_empty());
    if let Some(d) = date
        && chrono::NaiveDate::parse_from_str(d, "%Y-%m-%d").is_err()
    {
        return Err(AppError::invalid("purchase_date", "Enter the date as YYYY-MM-DD."));
    }
    let n = conn.execute(
        "UPDATE vehicles SET nickname = ?2, color = ?3, plate = ?4, mileage = ?5, mileage_unit = ?6, purchase_date = ?7,
           purchase_price = ?8, notes = ?9, customer_id = ?10, updated_at = ?11 WHERE id = ?1",
        params![
            id,
            clean(&u.nickname),
            clean(&u.color),
            clean(&u.plate).to_uppercase(),
            u.mileage,
            u.mileage_unit,
            date,
            price.map(|p| p.to_string()),
            u.notes.trim(),
            u.customer_id,
            db::now_utc()
        ],
    )?;
    expect_one(n, "That vehicle")?;
    get(conn, id)
}

pub fn set_saved(conn: &Connection, id: &str, saved: bool) -> AppResult<Vehicle> {
    let n = conn.execute("UPDATE vehicles SET saved = ?2, updated_at = ?3 WHERE id = ?1", params![id, saved as i64, db::now_utc()])?;
    expect_one(n, "That vehicle")?;
    db::audit(
        conn,
        "vehicle",
        Some(id),
        if saved { "save" } else { "unsave" },
        if saved { "Saved to garage" } else { "Removed from garage" },
        None,
    )?;
    get(conn, id)
}

pub fn touch(conn: &Connection, id: &str) -> AppResult<()> {
    conn.execute("UPDATE vehicles SET last_viewed_at = ?2 WHERE id = ?1", params![id, db::now_utc()])?;
    Ok(())
}

pub fn delete(conn: &Connection, id: &str) -> AppResult<()> {
    let v = get(conn, id)?;
    conn.execute("DELETE FROM vehicles WHERE id = ?1", [id])?;
    db::audit(conn, "vehicle", Some(id), "delete", &format!("Deleted {}", v.title()), None)?;
    Ok(())
}

pub fn list(conn: &Connection, saved: bool, limit: i64) -> AppResult<Vec<Vehicle>> {
    let order = if saved { "make, model, year DESC" } else { "last_viewed_at DESC" };
    let mut st = conn.prepare(&format!("SELECT {COLS} FROM vehicles WHERE saved = ?1 ORDER BY {order} LIMIT ?2"))?;
    let rows = st.query_map(params![saved as i64, limit], row)?;
    Ok(rows.collect::<Result<_, _>>()?)
}

/// Recently viewed vehicles, saved or not.
pub fn recent(conn: &Connection, limit: i64) -> AppResult<Vec<Vehicle>> {
    let mut st = conn.prepare(&format!("SELECT {COLS} FROM vehicles ORDER BY last_viewed_at DESC LIMIT ?1"))?;
    let rows = st.query_map([limit], row)?;
    Ok(rows.collect::<Result<_, _>>()?)
}

pub fn by_customer(conn: &Connection, customer_id: &str) -> AppResult<Vec<Vehicle>> {
    let mut st = conn.prepare(&format!("SELECT {COLS} FROM vehicles WHERE customer_id = ?1 ORDER BY make, model"))?;
    let rows = st.query_map([customer_id], row)?;
    Ok(rows.collect::<Result<_, _>>()?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::vpic;

    fn fixture(name: &str) -> Decoded {
        vpic::parse_decode(&std::fs::read_to_string(format!("{}/tests/fixtures/vpic/{name}", env!("CARGO_MANIFEST_DIR"))).unwrap()).unwrap()
    }

    #[test]
    fn stores_decode_and_keeps_user_corrections() {
        let conn = db::open_memory().unwrap();
        let d = fixture("decode_1HGCM82633A004352.json");
        let v = store_decoded(&conn, &d, vpic::SOURCE).unwrap();
        assert_eq!((v.year, v.make.as_str(), v.model.as_str(), v.trim.as_str()), (Some(2003), "Honda", "Accord", "EX-V6"));
        assert_eq!(v.engine, "3.0L V6");
        assert_eq!(v.transmission, "5-speed Automatic");
        assert!(!v.saved);

        // The user corrects the trim; the provider's original stays on record.
        set_spec(&conn, &v.id, &SpecInput { key: "trim".into(), value: "EX-V6 Coupe".into(), unit: None, source_ref: None }).unwrap();
        // Decoding again (e.g. refresh) must not overwrite the correction or duplicate the vehicle.
        let again = store_decoded(&conn, &d, vpic::SOURCE).unwrap();
        assert_eq!(again.id, v.id);
        assert_eq!(again.trim, "EX-V6 Coupe");
        let det = detail(&conn, &v.id).unwrap();
        let trims: Vec<_> = det.specs.iter().filter(|s| s.key == "trim").collect();
        assert_eq!(trims.len(), 2);
        assert!(trims.iter().any(|s| s.source_id == "user" && s.effective));
        assert!(trims.iter().any(|s| s.source_id == vpic::SOURCE && !s.effective && s.value == "EX-V6"));
        assert!(det.conflicts.is_empty(), "a user correction isn't a source conflict");

        clear_user_spec(&conn, &v.id, "trim").unwrap();
        assert_eq!(get(&conn, &v.id).unwrap().trim, "EX-V6");
    }

    #[test]
    fn manual_entry_and_references() {
        let conn = db::open_memory().unwrap();
        let v = create_manual(
            &conn,
            &ManualVehicle { year: Some(1972), make: "Volkswagen".into(), model: "Beetle".into(), ..Default::default() },
        )
        .unwrap();
        assert!(v.saved);
        let d = set_spec(
            &conn,
            &v.id,
            &SpecInput { key: "oil_capacity".into(), value: "2.5".into(), unit: None, source_ref: Some("Owner's manual p. 88".into()) },
        )
        .unwrap();
        let oil = d.specs.iter().find(|s| s.key == "oil_capacity").unwrap();
        assert_eq!(oil.confidence, "manual_ref");
        assert_eq!(oil.unit.as_deref(), Some("qt"));
        assert!(create_manual(&conn, &ManualVehicle { make: "".into(), model: "X".into(), ..Default::default() }).is_err());
        assert!(set_spec(&conn, &v.id, &SpecInput { key: "Bad Key".into(), value: "1".into(), unit: None, source_ref: None }).is_err());
    }

    #[test]
    fn display_make_rules() {
        assert_eq!(display_make("HONDA"), "Honda");
        assert_eq!(display_make("BMW"), "BMW");
        assert_eq!(display_make("MERCEDES-BENZ"), "Mercedes-Benz");
        assert_eq!(display_make("LAND ROVER"), "Land Rover");
    }

    #[test]
    fn recent_lookups_are_pruned_but_saved_kept() {
        let conn = db::open_memory().unwrap();
        for i in 0..(RECENT_KEEP + 5) {
            conn.execute(
                "INSERT INTO vehicles (id, saved, last_viewed_at) VALUES (?1, 0, ?2)",
                params![format!("v{i}"), format!("2026-01-01T00:00:{:02}.000Z", i % 60)],
            )
            .unwrap();
        }
        conn.execute("UPDATE vehicles SET saved = 1 WHERE id = 'v0'", []).unwrap();
        prune_recent(&conn).unwrap();
        let unsaved: i64 = conn.query_row("SELECT count(*) FROM vehicles WHERE saved = 0", [], |r| r.get(0)).unwrap();
        assert_eq!(unsaved, RECENT_KEEP);
        assert!(get(&conn, "v0").is_ok());
    }
}
