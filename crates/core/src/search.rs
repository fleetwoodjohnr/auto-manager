//! Global search. Works out what was typed (a VIN, a part number or SKU, or words), routes
//! exact identifiers straight to their record, and otherwise searches every entity through the
//! trigram index, with a "did you mean" fallback for typos.

use crate::db;
use crate::error::AppResult;
use crate::{parts, vin};
use rusqlite::{Connection, OptionalExtension, params};
use serde::Serialize;
use ts_rs::TS;

#[derive(Serialize, TS, Clone, Debug, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[ts(export)]
pub enum Route {
    /// A VIN: open the vehicle lookup (or the saved vehicle).
    Vin {
        vin: String,
        vehicle_id: Option<String>,
    },
    Part {
        id: String,
    },
    Item {
        id: String,
    },
}

#[derive(Serialize, TS, Clone, Debug)]
#[ts(export)]
pub struct Hit {
    pub kind: String,
    pub id: String,
    pub title: String,
    pub subtitle: String,
}

#[derive(Serialize, TS, Clone, Debug)]
#[ts(export)]
pub struct SearchResult {
    pub query: String,
    pub route: Option<Route>,
    pub hits: Vec<Hit>,
    pub suggestion: Option<String>,
}

fn route(conn: &Connection, q: &str) -> AppResult<Option<Route>> {
    if vin::looks_like_vin(q) {
        let v = vin::normalize(q);
        let vehicle_id = conn.query_row("SELECT id FROM vehicles WHERE vin = ?1", [&v], |r| r.get(0)).optional()?;
        return Ok(Some(Route::Vin { vin: v, vehicle_id }));
    }
    if let Some(item) =
        conn.query_row("SELECT id FROM inventory_items WHERE sku = ?1 COLLATE NOCASE", [q.trim()], |r| r.get::<_, String>(0)).optional()?
    {
        return Ok(Some(Route::Item { id: item }));
    }
    let found = parts::by_identifier(conn, q)?;
    if found.len() == 1 && parts::norm(q).len() >= 4 {
        // A barcode scan of a stocked product opens the stock record.
        let items: Vec<String> = conn
            .prepare("SELECT id FROM inventory_items WHERE part_id = ?1 AND archived = 0")?
            .query_map([&found[0]], |r| r.get(0))?
            .collect::<Result<_, _>>()?;
        if items.len() == 1 {
            return Ok(Some(Route::Item { id: items[0].clone() }));
        }
        return Ok(Some(Route::Part { id: found[0].clone() }));
    }
    Ok(None)
}

fn subtitle(conn: &Connection, kind: &str, id: &str, body: &str) -> AppResult<String> {
    Ok(match kind {
        "item" => {
            let (sku, avail): (String, i64) = conn.query_row(
                "SELECT i.sku, coalesce((SELECT sum(on_hand - reserved) FROM stock_levels s JOIN inventory_locations l ON l.id = s.location_id WHERE s.item_id = i.id AND l.kind = 'warehouse'), 0)
                 FROM inventory_items i WHERE i.id = ?1",
                [id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )?;
            format!("SKU {sku} · {avail} available")
        }
        "vehicle" => body.split_whitespace().next().filter(|w| w.len() == 17).map(|v| format!("VIN {v}")).unwrap_or_default(),
        _ => body.chars().take(80).collect(),
    })
}

pub fn search(conn: &Connection, query: &str, kinds: Option<&[String]>, limit: i64) -> AppResult<SearchResult> {
    let q = db::clean(query);
    let mut out = SearchResult { query: q.clone(), route: None, hits: vec![], suggestion: None };
    if q.is_empty() {
        return Ok(out);
    }
    out.route = route(conn, &q)?;
    let allowed = |k: &str| kinds.is_none_or(|ks| ks.iter().any(|x| x == k));
    let rows: Vec<(String, String, String, String)> = if q.chars().count() >= 3 {
        // Each word must appear (trigram matching finds substrings, so "brak" finds "brake").
        let fts: String = q
            .split_whitespace()
            .filter(|w| w.chars().count() >= 3)
            .map(|w| format!("\"{}\"", w.replace('"', "")))
            .collect::<Vec<_>>()
            .join(" AND ");
        if fts.is_empty() {
            vec![]
        } else {
            let mut st = conn.prepare("SELECT d.kind, d.ref_id, d.title, d.body FROM search_index JOIN search_docs d ON d.id = search_index.rowid WHERE search_index MATCH ?1 ORDER BY rank LIMIT ?2")?;
            st.query_map(params![fts, limit * 3], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?.collect::<Result<_, _>>()?
        }
    } else {
        let like = format!("{}%", q.replace('%', "\\%").replace('_', "\\_"));
        let mut st =
            conn.prepare("SELECT kind, ref_id, title, body FROM search_docs WHERE title LIKE ?1 ESCAPE '\\' ORDER BY title LIMIT ?2")?;
        st.query_map(params![like, limit * 3], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?.collect::<Result<_, _>>()?
    };
    for (kind, id, title, body) in rows {
        if !allowed(&kind) || out.hits.len() as i64 >= limit {
            continue;
        }
        let subtitle = subtitle(conn, &kind, &id, &body)?;
        out.hits.push(Hit { kind, id, title, subtitle });
    }
    if out.hits.is_empty() && out.route.is_none() {
        out.suggestion = suggest(conn, &q)?;
    }
    Ok(out)
}

/// The closest title by Jaro-Winkler similarity, when it is close enough to be a typo.
fn suggest(conn: &Connection, q: &str) -> AppResult<Option<String>> {
    // ponytail: linear scan over at most 20k titles; switch to a spelling index if catalogs get bigger.
    let ql = q.to_lowercase();
    let mut st = conn.prepare("SELECT title FROM search_docs LIMIT 20000")?;
    let mut best: Option<(f64, String)> = None;
    for t in st.query_map([], |r| r.get::<_, String>(0))? {
        let t = t?;
        let tl = t.to_lowercase();
        // Compare with the whole title and with each word, so "alternatr" finds "Alternator, 120A".
        let score = std::iter::once(tl.as_str()).chain(tl.split_whitespace()).map(|w| strsim::jaro_winkler(&ql, w)).fold(0.0, f64::max);
        if score >= 0.88 && best.as_ref().is_none_or(|(s, _)| score > *s) {
            best = Some((score, t));
        }
    }
    Ok(best.map(|(_, t)| t))
}

#[derive(Serialize, TS, Clone, Debug)]
#[ts(export)]
pub struct SavedSearch {
    pub id: String,
    pub name: String,
    pub page: String,
    /// The page's filters, as the page stored them (JSON).
    pub filters: serde_json::Value,
}

pub fn save_search(conn: &Connection, name: &str, page: &str, filters: &serde_json::Value) -> AppResult<SavedSearch> {
    let name = db::clean(name);
    if name.is_empty() {
        return Err(crate::AppError::invalid("name", "Name the search."));
    }
    if !filters.is_object() || filters.to_string().len() > 4000 {
        return Err(crate::AppError::msg("Those filters can't be saved."));
    }
    let id = db::new_id();
    conn.execute(
        "INSERT INTO saved_searches (id, name, page, filters) VALUES (?1, ?2, ?3, ?4)",
        params![id, name, page, filters.to_string()],
    )?;
    Ok(SavedSearch { id, name, page: page.into(), filters: filters.clone() })
}

pub fn saved_searches(conn: &Connection, page: &str) -> AppResult<Vec<SavedSearch>> {
    let mut st = conn.prepare("SELECT id, name, page, filters FROM saved_searches WHERE page = ?1 ORDER BY name")?;
    let rows = st.query_map([page], |r| {
        let f: String = r.get(3)?;
        Ok(SavedSearch { id: r.get(0)?, name: r.get(1)?, page: r.get(2)?, filters: serde_json::from_str(&f).unwrap_or_default() })
    })?;
    Ok(rows.collect::<Result<_, _>>()?)
}

pub fn delete_saved_search(conn: &Connection, id: &str) -> AppResult<()> {
    conn.execute("DELETE FROM saved_searches WHERE id = ?1", [id])?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seeded() -> Connection {
        let conn = db::open_memory().unwrap();
        conn.execute_batch(
            "INSERT INTO parts (id, name, brand) VALUES ('p1', 'Alternator 120A', 'Bosch'), ('p2', 'Front brake pad set', 'Acme');
             INSERT INTO part_identifiers (part_id, kind, value, value_norm) VALUES ('p2', 'oem', 'D1234-X', 'D1234X'), ('p1', 'barcode', '0123456789012', '0123456789012');
             INSERT INTO inventory_items (id, part_id, sku) VALUES ('i1', 'p1', 'ALT-120');
             INSERT INTO vehicles (id, vin, year, make, model) VALUES ('v1', '1HGCM82633A004352', 2003, 'Honda', 'Accord');",
        )
        .unwrap();
        conn
    }

    #[test]
    fn routes_by_what_was_typed() {
        let conn = seeded();
        assert_eq!(
            search(&conn, "1hgcm82633a004352", None, 10).unwrap().route,
            Some(Route::Vin { vin: "1HGCM82633A004352".into(), vehicle_id: Some("v1".into()) })
        );
        assert_eq!(search(&conn, "alt-120", None, 10).unwrap().route, Some(Route::Item { id: "i1".into() }));
        assert_eq!(search(&conn, "d1234x", None, 10).unwrap().route, Some(Route::Part { id: "p2".into() }));
        assert_eq!(
            search(&conn, "0123456789012", None, 10).unwrap().route,
            Some(Route::Item { id: "i1".into() }),
            "barcode scan opens the stocked item"
        );
    }

    #[test]
    fn finds_words_and_suggests_on_typos() {
        let conn = seeded();
        let r = search(&conn, "brake", None, 10).unwrap();
        assert_eq!(r.hits[0].id, "p2");
        let r = search(&conn, "accord", None, 10).unwrap();
        assert!(r.hits.iter().any(|h| h.kind == "vehicle"));
        let r = search(&conn, "alternatr", None, 10).unwrap();
        assert!(r.hits.is_empty());
        assert_eq!(r.suggestion.as_deref(), Some("Alternator 120A"));
        let only_parts = search(&conn, "alternator", Some(&["part".to_string()]), 10).unwrap();
        assert!(only_parts.hits.iter().all(|h| h.kind == "part"));
        assert!(search(&conn, "\" OR *", None, 10).is_ok(), "FTS syntax in input can't break the query");
    }
}
