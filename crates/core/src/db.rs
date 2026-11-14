//! SQLite access: connection setup, migrations, and small row helpers.

use crate::error::{AppError, AppResult};
use rusqlite::{Connection, OptionalExtension, Row};
use rust_decimal::Decimal;
use std::path::Path;

pub const MIGRATIONS: &[&str] = &[
    include_str!("../migrations/001_foundation.sql"),
    include_str!("../migrations/002_intelligence.sql"),
    include_str!("../migrations/003_business.sql"),
    include_str!("../migrations/004_advanced.sql"),
    include_str!("../migrations/005_quantities_sync.sql"),
];

pub fn schema_version() -> i64 {
    MIGRATIONS.len() as i64
}

pub fn configure(conn: &Connection) -> AppResult<()> {
    conn.pragma_update(None, "foreign_keys", true)?;
    conn.pragma_update(None, "busy_timeout", 5000)?;
    // WAL for crash safety; FULL sync because inventory and money live here.
    conn.query_row("PRAGMA journal_mode = WAL", [], |_| Ok(()))?;
    conn.pragma_update(None, "synchronous", "FULL")?;
    Ok(())
}

pub fn user_version(conn: &Connection) -> AppResult<i64> {
    Ok(conn.query_row("PRAGMA user_version", [], |r| r.get(0))?)
}

/// Apply pending migrations, each in its own transaction. `before_migrate` runs once (with the
/// current version) when an existing database needs upgrading; it takes a backup first.
pub fn migrate(conn: &mut Connection, before_migrate: impl FnOnce(&Connection, i64) -> AppResult<()>) -> AppResult<()> {
    let current = user_version(conn)?;
    let target = schema_version();
    if current > target {
        return Err(AppError::Other(format!(
            "Your data was saved by a newer version of Auto Manager (data version {current}; this version supports {target}). Update Auto Manager to open it."
        )));
    }
    if current == target {
        return Ok(());
    }
    if current > 0 {
        before_migrate(conn, current)?;
    }
    for (i, sql) in MIGRATIONS.iter().enumerate() {
        let v = i as i64 + 1;
        if v <= current {
            continue;
        }
        let tx = conn.transaction()?;
        tx.execute_batch(sql)?;
        tx.pragma_update(None, "user_version", v)?;
        tx.commit()?;
    }
    let bad: Vec<String> = {
        let mut st = conn.prepare("PRAGMA foreign_key_check")?;
        let rows = st.query_map([], |r| r.get::<_, String>(0))?;
        rows.collect::<Result<_, _>>()?
    };
    if !bad.is_empty() {
        return Err(AppError::Other(format!("Foreign-key check failed after migration in: {}", bad.join(", "))));
    }
    Ok(())
}

pub fn open(path: &Path, before_migrate: impl FnOnce(&Connection, i64) -> AppResult<()>) -> AppResult<Connection> {
    let mut conn = Connection::open(path)?;
    configure(&conn)?;
    migrate(&mut conn, before_migrate)?;
    Ok(conn)
}

/// A migrated in-memory database (tests and fixtures).
pub fn open_memory() -> AppResult<Connection> {
    let mut conn = Connection::open_in_memory()?;
    conn.pragma_update(None, "foreign_keys", true)?;
    migrate(&mut conn, |_, _| Ok(()))?;
    Ok(conn)
}

pub fn now_utc() -> String {
    chrono::Utc::now().format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string()
}

/// A new time-ordered identifier.
pub fn new_id() -> String {
    uuid::Uuid::now_v7().to_string()
}

/// Read a TEXT decimal column.
pub fn dec(row: &Row, idx: &str) -> rusqlite::Result<Decimal> {
    let s: String = row.get(idx)?;
    s.parse::<Decimal>().map_err(|e| rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(e)))
}

pub fn opt_dec(row: &Row, idx: &str) -> rusqlite::Result<Option<Decimal>> {
    let s: Option<String> = row.get(idx)?;
    s.filter(|s| !s.is_empty())
        .map(|s| s.parse::<Decimal>().map_err(|e| rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(e))))
        .transpose()
}

/// Append an audit-trail entry. Call inside the same transaction as the change it describes.
pub fn audit(
    conn: &Connection,
    entity: &str,
    entity_id: Option<&str>,
    action: &str,
    summary: &str,
    detail: Option<serde_json::Value>,
) -> AppResult<()> {
    conn.execute(
        "INSERT INTO audit_events (entity, entity_id, action, summary, detail) VALUES (?1, ?2, ?3, ?4, ?5)",
        rusqlite::params![entity, entity_id, action, summary, detail.map(|d| d.to_string())],
    )?;
    Ok(())
}

/// Fail with NotFound when a single-row UPDATE/DELETE touched nothing.
pub fn expect_one(changed: usize, what: &str) -> AppResult<()> {
    if changed == 0 { Err(AppError::not_found(what)) } else { Ok(()) }
}

pub fn exists(conn: &Connection, sql: &str, params: impl rusqlite::Params) -> AppResult<bool> {
    Ok(conn.query_row(sql, params, |_| Ok(())).optional()?.is_some())
}

/// Normalize free text input: trim, collapse inner whitespace.
pub fn clean(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrates_empty_db_to_latest() {
        let conn = open_memory().unwrap();
        assert_eq!(user_version(&conn).unwrap(), schema_version());
    }

    #[test]
    fn rejects_newer_schema() {
        let mut conn = Connection::open_in_memory().unwrap();
        conn.pragma_update(None, "user_version", schema_version() + 1).unwrap();
        assert!(migrate(&mut conn, |_, _| Ok(())).is_err());
    }

    #[test]
    fn upgrades_each_prior_version_and_calls_backup_hook() {
        for start in 1..schema_version() {
            let mut conn = Connection::open_in_memory().unwrap();
            conn.pragma_update(None, "foreign_keys", true).unwrap();
            for (i, sql) in MIGRATIONS.iter().take(start as usize).enumerate() {
                conn.execute_batch(sql).unwrap();
                conn.pragma_update(None, "user_version", i as i64 + 1).unwrap();
            }
            let mut called = None;
            migrate(&mut conn, |_, v| {
                called = Some(v);
                Ok(())
            })
            .unwrap();
            assert_eq!(called, Some(start));
            assert_eq!(user_version(&conn).unwrap(), schema_version());
        }
    }

    #[test]
    fn search_index_follows_entities() {
        let conn = open_memory().unwrap();
        conn.execute("INSERT INTO parts (id, name, brand) VALUES ('p1', 'Front brake pad set', 'Acme')", []).unwrap();
        conn.execute("INSERT INTO part_identifiers (part_id, kind, value, value_norm) VALUES ('p1', 'oem', 'D1234-X', 'D1234X')", [])
            .unwrap();
        let hit = |q: &str| -> i64 {
            conn.query_row(
                "SELECT count(*) FROM search_index JOIN search_docs d ON d.id = search_index.rowid WHERE search_index MATCH ?1 AND d.ref_id = 'p1'",
                [q],
                |r| r.get(0),
            )
            .unwrap()
        };
        assert_eq!(hit("\"brake\""), 1);
        assert_eq!(hit("\"1234\""), 1, "identifier text is indexed with the part");
        conn.execute("UPDATE parts SET name = 'Rear shoe kit' WHERE id = 'p1'", []).unwrap();
        assert_eq!(hit("\"brake\""), 0);
        conn.execute("DELETE FROM parts WHERE id = 'p1'", []).unwrap();
        assert_eq!(hit("\"shoe\""), 0);
    }

    #[test]
    fn stock_checks_reject_negative_and_over_reserved() {
        let conn = open_memory().unwrap();
        conn.execute_batch(
            "INSERT INTO parts (id, name) VALUES ('p1', 'Filter');
             INSERT INTO inventory_items (id, part_id, sku) VALUES ('i1', 'p1', 'F-1');
             INSERT INTO stock_levels (item_id, location_id, on_hand, reserved) VALUES ('i1', 'loc-main', 2, 1);",
        )
        .unwrap();
        assert!(conn.execute("UPDATE stock_levels SET on_hand = -1", []).is_err());
        assert!(conn.execute("UPDATE stock_levels SET reserved = 3", []).is_err());
        assert!(conn.execute("UPDATE stock_levels SET on_hand = 0", []).is_err(), "reserved 1 > on_hand 0");
    }
}
