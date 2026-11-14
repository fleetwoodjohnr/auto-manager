//! App settings, stored as one JSON document. Unknown or missing fields take defaults, so
//! older and newer versions can share a database.

use crate::error::{AppError, AppResult};
use rusqlite::{Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

pub const WIDGETS: &[&str] =
    &["shortcuts", "recent_vehicles", "garage", "inventory", "low_stock", "jobs", "maintenance", "recalls", "activity", "updates"];

#[derive(Serialize, Deserialize, TS, Clone, Debug, PartialEq)]
#[serde(default)]
#[ts(export)]
pub struct Settings {
    /// "system", "light" or "dark"
    pub theme: String,
    /// "system", "iso" (2026-11-14), "us" (11/14/2026) or "eu" (14/11/2026)
    pub date_format: String,
    /// "imperial" or "metric", for displaying specifications
    pub units: String,
    /// ISO 4217 code used when formatting money
    pub currency: String,
    pub landing_page: String,
    /// "average" (moving weighted average) or "standard" (last purchase cost)
    pub valuation_method: String,
    /// Minimum stock suggested for new items
    pub default_min_stock: i64,
    /// "code128" or "qr"
    pub label_symbology: String,
    pub update_check: bool,
    pub update_download: bool,
    /// "stable" or "beta"
    pub update_channel: String,
    pub last_update_check: Option<String>,
    pub backup_auto: bool,
    /// Days before a saved provider answer is refreshed
    pub provider_refresh_days: i64,
    pub dashboard_widgets: Vec<String>,
    pub recent_searches: Vec<String>,
    pub onboarded: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            theme: "system".into(),
            date_format: "system".into(),
            units: "imperial".into(),
            currency: "USD".into(),
            landing_page: "dashboard".into(),
            valuation_method: "average".into(),
            default_min_stock: 0,
            label_symbology: "code128".into(),
            update_check: true,
            update_download: false,
            update_channel: "stable".into(),
            last_update_check: None,
            backup_auto: true,
            provider_refresh_days: 30,
            dashboard_widgets: WIDGETS.iter().map(|w| w.to_string()).collect(),
            recent_searches: vec![],
            onboarded: false,
        }
    }
}

pub fn load(conn: &Connection) -> Settings {
    conn.query_row("SELECT value FROM settings WHERE key = 'app'", [], |r| r.get::<_, String>(0))
        .optional()
        .ok()
        .flatten()
        .and_then(|v| serde_json::from_str(&v).ok())
        .unwrap_or_default()
}

fn one_of(field: &str, v: &str, allowed: &[&str]) -> AppResult<()> {
    if allowed.contains(&v) { Ok(()) } else { Err(AppError::invalid(field, format!("“{v}” isn't a valid choice."))) }
}

pub fn save(conn: &Connection, s: &Settings) -> AppResult<Settings> {
    one_of("theme", &s.theme, &["system", "light", "dark"])?;
    one_of("date_format", &s.date_format, &["system", "iso", "us", "eu"])?;
    one_of("units", &s.units, &["imperial", "metric"])?;
    one_of("valuation_method", &s.valuation_method, &["average", "standard"])?;
    one_of("label_symbology", &s.label_symbology, &["code128", "qr"])?;
    one_of("update_channel", &s.update_channel, &["stable", "beta"])?;
    one_of(
        "landing_page",
        &s.landing_page,
        &["dashboard", "lookup", "vehicles", "parts", "inventory", "manuals", "service", "pricing", "reports"],
    )?;
    if s.currency.len() != 3 || !s.currency.chars().all(|c| c.is_ascii_uppercase()) {
        return Err(AppError::invalid("currency", "Use a three-letter currency code such as USD."));
    }
    if !(1..=365).contains(&s.provider_refresh_days) {
        return Err(AppError::invalid("provider_refresh_days", "Choose between 1 and 365 days."));
    }
    let mut clean = s.clone();
    clean.dashboard_widgets.retain(|w| WIDGETS.contains(&w.as_str()));
    clean.recent_searches.truncate(12);
    conn.execute(
        "INSERT INTO settings (key, value) VALUES ('app', ?1) ON CONFLICT (key) DO UPDATE SET value = excluded.value",
        [serde_json::to_string(&clean)?],
    )?;
    Ok(clean)
}

pub fn remember_search(conn: &Connection, q: &str) -> AppResult<()> {
    let q = q.trim();
    if q.is_empty() {
        return Ok(());
    }
    let mut s = load(conn);
    s.recent_searches.retain(|x| !x.eq_ignore_ascii_case(q));
    s.recent_searches.insert(0, q.chars().take(80).collect());
    save(conn, &s).map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_round_trip_and_validation() {
        let conn = crate::db::open_memory().unwrap();
        assert_eq!(load(&conn), Settings::default());
        let s = Settings { theme: "dark".into(), ..Default::default() };
        save(&conn, &s).unwrap();
        assert_eq!(load(&conn).theme, "dark");
        assert!(save(&conn, &Settings { theme: "neon".into(), ..Default::default() }).is_err());
        assert!(save(&conn, &Settings { currency: "usd".into(), ..Default::default() }).is_err());
        // Settings written by a newer version with extra fields still load.
        conn.execute("UPDATE settings SET value = '{\"theme\":\"light\",\"future\":1}' WHERE key = 'app'", []).unwrap();
        assert_eq!(load(&conn).theme, "light");
        for q in ["a", "b", "A"] {
            remember_search(&conn, q).unwrap();
        }
        assert_eq!(load(&conn).recent_searches, vec!["A", "b"]);
    }
}
