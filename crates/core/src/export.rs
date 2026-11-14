//! A full JSON export of every table, for moving data to other tools. (Backups are the way to
//! move data between copies of Auto Manager; they also carry documents and photos.)

use crate::error::AppResult;
use rusqlite::Connection;
use rusqlite::types::ValueRef;
use std::path::Path;

pub fn all_json(conn: &Connection) -> AppResult<serde_json::Value> {
    let tables: Vec<String> = conn
        .prepare("SELECT name FROM sqlite_schema WHERE type = 'table' AND name NOT LIKE 'sqlite_%' AND name NOT LIKE 'search_%' AND name NOT LIKE 'manual_pages%' AND name <> 'api_cache' ORDER BY name")?
        .query_map([], |r| r.get(0))?
        .collect::<Result<_, _>>()?;
    let mut out = serde_json::Map::new();
    out.insert("format".into(), "auto-manager-export".into());
    out.insert("app_version".into(), env!("CARGO_PKG_VERSION").into());
    out.insert("schema_version".into(), crate::db::user_version(conn)?.into());
    out.insert("exported_at".into(), crate::db::now_utc().into());
    for t in tables {
        let mut st = conn.prepare(&format!("SELECT * FROM \"{t}\""))?;
        let cols: Vec<String> = st.column_names().map(|c| c.to_string()).collect();
        let rows = st.query_map([], |r| {
            let mut o = serde_json::Map::new();
            for (i, c) in cols.iter().enumerate() {
                let v = match r.get_ref(i)? {
                    ValueRef::Null => serde_json::Value::Null,
                    ValueRef::Integer(n) => n.into(),
                    ValueRef::Real(f) => f.into(),
                    ValueRef::Text(s) => String::from_utf8_lossy(s).into(),
                    ValueRef::Blob(b) => format!("<{} bytes>", b.len()).into(),
                };
                o.insert(c.clone(), v);
            }
            Ok(serde_json::Value::Object(o))
        })?;
        out.insert(t, serde_json::Value::Array(rows.collect::<Result<_, _>>()?));
    }
    Ok(serde_json::Value::Object(out))
}

pub fn write_json(conn: &Connection, path: &Path) -> AppResult<()> {
    let v = all_json(conn)?;
    let tmp = path.with_extension("json.partial");
    std::fs::write(&tmp, serde_json::to_vec_pretty(&v)?)?;
    std::fs::rename(tmp, path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn exports_every_table() {
        let conn = crate::db::open_memory().unwrap();
        conn.execute("INSERT INTO vehicles (id, make, model) VALUES ('v1', 'Honda', 'Accord')", []).unwrap();
        let v = super::all_json(&conn).unwrap();
        assert_eq!(v["vehicles"][0]["model"], "Accord");
        assert!(v.get("inventory_movements").is_some());
        assert!(v.get("api_cache").is_none() && v.get("search_docs").is_none());
    }
}
