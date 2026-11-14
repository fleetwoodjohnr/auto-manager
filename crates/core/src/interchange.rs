//! Part supersession (an old number replaced by a new one) and interchange (parts that can be
//! used in place of each other), recorded by the person or imported. Fitment of a superseded
//! part also considers its replacement.

use crate::db;
use crate::error::{AppError, AppResult};
use rusqlite::{Connection, params};
use serde::Serialize;
use ts_rs::TS;

#[derive(Serialize, TS, Clone, Debug)]
#[ts(export)]
pub struct Related {
    pub part_id: String,
    pub name: String,
    pub brand: String,
    pub numbers: String,
    pub note: String,
    pub source_name: String,
    pub available: i64,
}

#[derive(Serialize, TS, Clone, Debug)]
#[ts(export)]
pub struct Interchange {
    /// Newer parts that replace this one.
    pub superseded_by: Vec<Related>,
    /// Older parts this one replaces.
    pub supersedes: Vec<Related>,
    pub equivalents: Vec<Related>,
}

const REL: &str = "SELECT p.id, p.name, p.brand,
    coalesce((SELECT group_concat(value, ' · ') FROM part_identifiers WHERE part_id = p.id AND kind <> 'barcode'), ''),
    x.note, d.name,
    coalesce((SELECT sum(s.on_hand - s.reserved) FROM inventory_items i JOIN stock_levels s ON s.item_id = i.id JOIN inventory_locations l ON l.id = s.location_id
              WHERE i.part_id = p.id AND l.kind = 'warehouse' AND i.archived = 0), 0)";

fn rel(r: &rusqlite::Row) -> rusqlite::Result<Related> {
    Ok(Related {
        part_id: r.get(0)?,
        name: r.get(1)?,
        brand: r.get(2)?,
        numbers: r.get(3)?,
        note: r.get(4)?,
        source_name: r.get(5)?,
        available: r.get(6)?,
    })
}

pub fn get(conn: &Connection, part_id: &str) -> AppResult<Interchange> {
    let q = |sql: String| -> AppResult<Vec<Related>> {
        let mut st = conn.prepare(&sql)?;
        let rows = st.query_map([part_id], rel)?;
        Ok(rows.collect::<Result<_, _>>()?)
    };
    Ok(Interchange {
        superseded_by: q(format!("{REL} FROM part_supersessions x JOIN parts p ON p.id = x.new_part_id JOIN data_sources d ON d.id = x.source_id WHERE x.old_part_id = ?1"))?,
        supersedes: q(format!("{REL} FROM part_supersessions x JOIN parts p ON p.id = x.old_part_id JOIN data_sources d ON d.id = x.source_id WHERE x.new_part_id = ?1"))?,
        equivalents: q(format!(
            "{REL} FROM part_interchanges x JOIN parts p ON p.id = CASE WHEN x.part_a = ?1 THEN x.part_b ELSE x.part_a END JOIN data_sources d ON d.id = x.source_id
             WHERE x.part_a = ?1 OR x.part_b = ?1"
        ))?,
    })
}

pub fn add_supersession(conn: &Connection, old_part: &str, new_part: &str, note: &str, source_id: &str) -> AppResult<()> {
    if old_part == new_part {
        return Err(AppError::msg("A part can't replace itself."));
    }
    // Refuse cycles: new_part must not (transitively) be replaced by old_part.
    let cycle: bool = conn.query_row(
        "WITH RECURSIVE chain(id) AS (SELECT ?1 UNION SELECT s.new_part_id FROM part_supersessions s JOIN chain c ON s.old_part_id = c.id) SELECT EXISTS (SELECT 1 FROM chain WHERE id = ?2)",
        params![new_part, old_part],
        |r| r.get(0),
    )?;
    if cycle {
        return Err(AppError::msg("That would make a loop of replacements."));
    }
    conn.execute(
        "INSERT OR REPLACE INTO part_supersessions (old_part_id, new_part_id, note, source_id) VALUES (?1, ?2, ?3, ?4)",
        params![old_part, new_part, note.trim(), source_id],
    )?;
    db::audit(conn, "part", Some(old_part), "supersede", "Supersession recorded", None)?;
    Ok(())
}

pub fn remove_supersession(conn: &Connection, old_part: &str, new_part: &str) -> AppResult<()> {
    conn.execute("DELETE FROM part_supersessions WHERE old_part_id = ?1 AND new_part_id = ?2", params![old_part, new_part])?;
    Ok(())
}

pub fn add_equivalent(conn: &Connection, a: &str, b: &str, note: &str, source_id: &str) -> AppResult<()> {
    if a == b {
        return Err(AppError::msg("Choose a different part."));
    }
    let (x, y) = if a < b { (a, b) } else { (b, a) };
    conn.execute(
        "INSERT OR REPLACE INTO part_interchanges (part_a, part_b, note, source_id) VALUES (?1, ?2, ?3, ?4)",
        params![x, y, note.trim(), source_id],
    )?;
    Ok(())
}

pub fn remove_equivalent(conn: &Connection, a: &str, b: &str) -> AppResult<()> {
    let (x, y) = if a < b { (a, b) } else { (b, a) };
    conn.execute("DELETE FROM part_interchanges WHERE part_a = ?1 AND part_b = ?2", params![x, y])?;
    Ok(())
}

/// The newest part in a supersession chain (the part itself if nothing replaces it).
pub fn latest(conn: &Connection, part_id: &str) -> AppResult<String> {
    Ok(conn.query_row(
        "WITH RECURSIVE chain(id, depth) AS (SELECT ?1, 0 UNION SELECT s.new_part_id, c.depth + 1 FROM part_supersessions s JOIN chain c ON s.old_part_id = c.id WHERE c.depth < 20)
         SELECT id FROM chain ORDER BY depth DESC LIMIT 1",
        [part_id],
        |r| r.get(0),
    )?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn supersession_chain_and_cycles() {
        let conn = db::open_memory().unwrap();
        conn.execute_batch("INSERT INTO parts (id, name) VALUES ('a', 'Old'), ('b', 'Newer'), ('c', 'Newest'), ('d', 'Other brand');")
            .unwrap();
        add_supersession(&conn, "a", "b", "", "user").unwrap();
        add_supersession(&conn, "b", "c", "", "user").unwrap();
        assert_eq!(latest(&conn, "a").unwrap(), "c");
        assert!(add_supersession(&conn, "c", "a", "", "user").is_err(), "no loops");
        add_equivalent(&conn, "d", "c", "same pad, different box", "user").unwrap();
        let i = get(&conn, "c").unwrap();
        assert_eq!(i.supersedes[0].part_id, "b");
        assert_eq!(i.equivalents[0].part_id, "d");
        assert_eq!(get(&conn, "d").unwrap().equivalents[0].part_id, "c", "symmetric");
    }
}
