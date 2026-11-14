//! Stored OBD-II scan sessions (the reading itself happens in the auto-manager-obd crate).

use crate::db;
use crate::error::AppResult;
use rusqlite::{Connection, params};
use serde::Serialize;
use ts_rs::TS;

#[derive(Serialize, TS, Clone, Debug)]
#[ts(export)]
pub struct ObdReading {
    pub kind: String,
    pub code: String,
    pub value: String,
    pub unit: String,
}

#[derive(Serialize, TS, Clone, Debug)]
#[ts(export)]
pub struct ObdSession {
    pub id: String,
    pub vehicle_id: Option<String>,
    pub adapter: String,
    pub protocol: String,
    pub vin_read: Option<String>,
    pub started_at: String,
    pub readings: Vec<ObdReading>,
}

pub fn save(
    conn: &Connection,
    vehicle_id: Option<&str>,
    adapter: &str,
    protocol: &str,
    vin: Option<&str>,
    readings: &[ObdReading],
) -> AppResult<String> {
    let id = db::new_id();
    conn.execute(
        "INSERT INTO obd_sessions (id, vehicle_id, adapter, protocol, vin_read) VALUES (?1, ?2, ?3, ?4, ?5)",
        params![id, vehicle_id, adapter, protocol, vin],
    )?;
    let mut ins = conn.prepare("INSERT INTO obd_readings (session_id, kind, code, value, unit) VALUES (?1, ?2, ?3, ?4, ?5)")?;
    for r in readings {
        ins.execute(params![id, r.kind, r.code, r.value, r.unit])?;
    }
    Ok(id)
}

pub fn sessions(conn: &Connection, vehicle_id: Option<&str>) -> AppResult<Vec<ObdSession>> {
    let mut st = conn.prepare("SELECT id, vehicle_id, adapter, protocol, vin_read, started_at FROM obd_sessions WHERE ?1 IS NULL OR vehicle_id = ?1 ORDER BY started_at DESC LIMIT 100")?;
    let mut list: Vec<ObdSession> = st
        .query_map([vehicle_id], |r| {
            Ok(ObdSession {
                id: r.get(0)?,
                vehicle_id: r.get(1)?,
                adapter: r.get(2)?,
                protocol: r.get(3)?,
                vin_read: r.get(4)?,
                started_at: r.get(5)?,
                readings: vec![],
            })
        })?
        .collect::<Result<_, _>>()?;
    let mut rd = conn.prepare("SELECT kind, code, value, unit FROM obd_readings WHERE session_id = ?1 ORDER BY id")?;
    for s in &mut list {
        s.readings = rd
            .query_map([&s.id], |r| Ok(ObdReading { kind: r.get(0)?, code: r.get(1)?, value: r.get(2)?, unit: r.get(3)? }))?
            .collect::<Result<_, _>>()?;
    }
    Ok(list)
}

#[cfg(test)]
mod tests {
    #[test]
    fn stores_sessions() {
        let conn = crate::db::open_memory().unwrap();
        let r = super::ObdReading { kind: "stored_dtc".into(), code: "P0420".into(), value: String::new(), unit: String::new() };
        super::save(&conn, None, "ELM327 v1.5", "CAN", Some("1HGCM82633A004352"), &[r]).unwrap();
        let s = super::sessions(&conn, None).unwrap();
        assert_eq!(s[0].readings[0].code, "P0420");
    }
}
