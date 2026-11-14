//! Generic OBD-II trouble codes. Bundled from OBDex (data CC0 1.0,
//! https://github.com/foerbsnavi/OBDex), English fields only. Possible causes are shown as
//! possibilities with their likelihood, never as a diagnosis. Manufacturer-specific codes need
//! licensed data and aren't included.

use crate::error::{AppError, AppResult};
use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};
use std::io::Read;
use ts_rs::TS;

static DATA: &[u8] = include_bytes!("../data/dtc.json.gz");

#[derive(Deserialize)]
struct Raw {
    code: String,
    category: String,
    title: String,
    description: String,
    causes: Vec<(String, String)>,
    symptoms: Vec<String>,
    related: Vec<String>,
    sources: Vec<String>,
    mil: bool,
}

#[derive(Serialize, Deserialize, TS, Clone, Debug)]
#[ts(export)]
pub struct DtcDetail {
    /// (likelihood, cause), e.g. ("high", "Catalyst aged or poisoned")
    pub causes: Vec<(String, String)>,
    pub symptoms: Vec<String>,
    pub related: Vec<String>,
    pub sources: Vec<String>,
    pub mil: bool,
}

#[derive(Serialize, TS, Clone, Debug)]
#[ts(export)]
pub struct Dtc {
    pub code: String,
    /// powertrain, body, chassis or network
    pub system: String,
    /// Generic (SAE) or manufacturer-defined range
    pub scope: String,
    pub title: String,
    pub description: String,
    pub detail: DtcDetail,
}

/// Load the bundled codes into the database once.
pub fn ensure_loaded(conn: &Connection) -> AppResult<()> {
    let n: i64 = conn.query_row("SELECT count(*) FROM dtc_codes", [], |r| r.get(0))?;
    if n > 0 {
        return Ok(());
    }
    let mut json = String::new();
    flate2::read::GzDecoder::new(DATA)
        .read_to_string(&mut json)
        .map_err(|e| AppError::Other(format!("Bundled trouble-code data is unreadable: {e}")))?;
    let raw: Vec<Raw> = serde_json::from_str(&json)?;
    let mut ins =
        conn.prepare("INSERT OR REPLACE INTO dtc_codes (code, category, title, description, detail) VALUES (?1, ?2, ?3, ?4, ?5)")?;
    for r in raw {
        let detail = DtcDetail { causes: r.causes, symptoms: r.symptoms, related: r.related, sources: r.sources, mil: r.mil };
        ins.execute(params![r.code, r.category, r.title, r.description, serde_json::to_string(&detail)?])?;
    }
    Ok(())
}

/// Second character 0, 2 or 3 (some 3s) is SAE-defined; 1 is manufacturer-defined.
pub fn scope(code: &str) -> &'static str {
    match code.chars().nth(1) {
        Some('0') | Some('2') => "Generic (SAE)",
        Some('1') => "Manufacturer-specific",
        Some('3') => "Generic or manufacturer-specific",
        _ => "Unknown",
    }
}

pub fn system_of(code: &str) -> &'static str {
    match code.chars().next().map(|c| c.to_ascii_uppercase()) {
        Some('P') => "Powertrain",
        Some('B') => "Body",
        Some('C') => "Chassis",
        Some('U') => "Network",
        _ => "Unknown",
    }
}

pub fn valid(code: &str) -> bool {
    let c: Vec<char> = code.chars().collect();
    c.len() == 5 && matches!(c[0].to_ascii_uppercase(), 'P' | 'B' | 'C' | 'U') && c[1..].iter().all(|x| x.is_ascii_hexdigit())
}

fn row(r: &rusqlite::Row) -> rusqlite::Result<Dtc> {
    let code: String = r.get(0)?;
    let detail: String = r.get(4)?;
    Ok(Dtc {
        scope: scope(&code).into(),
        code,
        system: r.get::<_, String>(1)?,
        title: r.get(2)?,
        description: r.get(3)?,
        detail: serde_json::from_str(&detail).unwrap_or(DtcDetail {
            causes: vec![],
            symptoms: vec![],
            related: vec![],
            sources: vec![],
            mil: false,
        }),
    })
}

/// Look up one code exactly, or search titles and descriptions.
pub fn search(conn: &Connection, q: &str, limit: i64) -> AppResult<Vec<Dtc>> {
    ensure_loaded(conn)?;
    let q = q.trim();
    if q.is_empty() {
        return Ok(vec![]);
    }
    let up = q.to_uppercase();
    let mut st = conn.prepare(
        "SELECT code, category, title, description, detail FROM dtc_codes
         WHERE code = ?1 OR code LIKE ?2 OR title LIKE ?3 OR description LIKE ?3
         ORDER BY code = ?1 DESC, code LIKE ?2 DESC, code LIMIT ?4",
    )?;
    let like = format!("%{}%", q.replace('%', ""));
    let rows = st.query_map(params![up, format!("{up}%"), like, limit.clamp(1, 200)], row)?;
    Ok(rows.collect::<Result<_, _>>()?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_codes_load_and_search() {
        let conn = crate::db::open_memory().unwrap();
        let r = search(&conn, "p0420", 5).unwrap();
        assert_eq!(r[0].code, "P0420");
        assert!(r[0].title.contains("Catalyst"));
        assert!(!r[0].detail.causes.is_empty());
        assert_eq!(r[0].scope, "Generic (SAE)");
        assert!(search(&conn, "oxygen sensor", 50).unwrap().len() > 5);
        assert!(valid("P0420") && valid("u0100") && !valid("P042") && !valid("X0420"));
        assert_eq!(system_of("C1234"), "Chassis");
    }
}
