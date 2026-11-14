//! The parts catalog: parts, their identifiers (OEM, aftermarket, barcode, interchange) and
//! fitment records.

use crate::db::{self, clean, expect_one};
use crate::error::{AppError, AppResult};
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

pub const ID_KINDS: &[&str] = &["oem", "aftermarket", "barcode", "interchange", "supplier"];

/// Uppercase letters and digits only, so "12345-ABC" and "12345abc" match.
pub fn norm(value: &str) -> String {
    value.chars().filter(|c| c.is_ascii_alphanumeric()).flat_map(char::to_uppercase).collect()
}

#[derive(Serialize, TS, Clone, Debug)]
#[ts(export)]
pub struct Category {
    pub id: String,
    pub parent_id: Option<String>,
    pub name: String,
    pub sort: i64,
    pub part_count: i64,
}

pub fn categories(conn: &Connection) -> AppResult<Vec<Category>> {
    let mut st = conn.prepare(
        "SELECT c.id, c.parent_id, c.name, c.sort, (SELECT count(*) FROM parts p WHERE p.category_id = c.id)
         FROM part_categories c ORDER BY c.parent_id IS NOT NULL, c.sort, c.name",
    )?;
    let rows = st
        .query_map([], |r| Ok(Category { id: r.get(0)?, parent_id: r.get(1)?, name: r.get(2)?, sort: r.get(3)?, part_count: r.get(4)? }))?;
    Ok(rows.collect::<Result<_, _>>()?)
}

/// The category and all its ancestors, nearest first.
pub fn category_chain(conn: &Connection, id: &str) -> AppResult<Vec<String>> {
    let mut st = conn.prepare(
        "WITH RECURSIVE up(id, parent_id, depth) AS (
           SELECT id, parent_id, 0 FROM part_categories WHERE id = ?1
           UNION ALL SELECT c.id, c.parent_id, up.depth + 1 FROM part_categories c JOIN up ON c.id = up.parent_id)
         SELECT id FROM up ORDER BY depth",
    )?;
    let rows = st.query_map([id], |r| r.get(0))?;
    Ok(rows.collect::<Result<_, _>>()?)
}

pub fn category_path(conn: &Connection, id: &str) -> AppResult<String> {
    let chain = category_chain(conn, id)?;
    let mut names = Vec::new();
    for c in chain.iter().rev() {
        names.push(conn.query_row("SELECT name FROM part_categories WHERE id = ?1", [c], |r| r.get::<_, String>(0))?);
    }
    Ok(names.join(" › "))
}

pub fn add_category(conn: &Connection, parent_id: Option<&str>, name: &str) -> AppResult<Category> {
    let name = clean(name);
    if name.is_empty() {
        return Err(AppError::invalid("name", "Enter a category name."));
    }
    let id = format!("c-{}", db::new_id());
    conn.execute(
        "INSERT INTO part_categories (id, parent_id, name, sort) VALUES (?1, ?2, ?3, (SELECT coalesce(max(sort), 0) + 1 FROM part_categories WHERE parent_id IS ?2))",
        params![id, parent_id, name],
    )
    .map_err(|e| if e.to_string().contains("UNIQUE") { AppError::Conflict("That category already exists here.".into()) } else { e.into() })?;
    Ok(Category { id, parent_id: parent_id.map(Into::into), name, sort: 0, part_count: 0 })
}

#[derive(Serialize, Deserialize, TS, Clone, Debug, PartialEq)]
#[ts(export)]
pub struct Identifier {
    pub kind: String,
    pub value: String,
    #[serde(default)]
    pub brand: String,
}

#[derive(Serialize, TS, Clone, Debug)]
#[ts(export)]
pub struct Part {
    pub id: String,
    pub name: String,
    pub category_id: Option<String>,
    pub category_path: String,
    pub brand: String,
    pub manufacturer: String,
    pub description: String,
    /// Technical attributes as name/value pairs.
    pub specs: Vec<(String, String)>,
    pub install_notes: String,
    pub warranty: String,
    pub source_id: String,
    pub source_name: String,
    pub identifiers: Vec<Identifier>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Deserialize, TS, Clone, Debug, Default)]
#[ts(export)]
pub struct PartInput {
    pub name: String,
    pub category_id: Option<String>,
    #[serde(default)]
    pub brand: String,
    #[serde(default)]
    pub manufacturer: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub specs: Vec<(String, String)>,
    #[serde(default)]
    pub install_notes: String,
    #[serde(default)]
    pub warranty: String,
    #[serde(default)]
    pub identifiers: Vec<Identifier>,
}

fn validate(conn: &Connection, p: &PartInput) -> AppResult<()> {
    if clean(&p.name).is_empty() {
        return Err(AppError::invalid("name", "Enter the part name."));
    }
    if let Some(c) = &p.category_id
        && !db::exists(conn, "SELECT 1 FROM part_categories WHERE id = ?1", [c])?
    {
        return Err(AppError::invalid("category_id", "Choose a category from the list."));
    }
    for i in &p.identifiers {
        if !ID_KINDS.contains(&i.kind.as_str()) {
            return Err(AppError::invalid("identifiers", format!("Unknown number type “{}”.", i.kind)));
        }
        if norm(&i.value).is_empty() {
            return Err(AppError::invalid("identifiers", "A part number needs at least one letter or digit."));
        }
    }
    Ok(())
}

fn write_identifiers(conn: &Connection, part_id: &str, ids: &[Identifier]) -> AppResult<()> {
    conn.execute("DELETE FROM part_identifiers WHERE part_id = ?1", [part_id])?;
    let mut st =
        conn.prepare("INSERT OR IGNORE INTO part_identifiers (part_id, kind, value, value_norm, brand) VALUES (?1, ?2, ?3, ?4, ?5)")?;
    for i in ids {
        st.execute(params![part_id, i.kind, i.value.trim(), norm(&i.value), clean(&i.brand)])?;
    }
    Ok(())
}

fn specs_json(specs: &[(String, String)]) -> String {
    let map: serde_json::Map<String, serde_json::Value> = specs
        .iter()
        .filter(|(k, v)| !k.trim().is_empty() && !v.trim().is_empty())
        .map(|(k, v)| (clean(k), serde_json::Value::String(v.trim().into())))
        .collect();
    serde_json::Value::Object(map).to_string()
}

pub fn create(conn: &Connection, p: &PartInput, source_id: &str) -> AppResult<Part> {
    validate(conn, p)?;
    let id = db::new_id();
    conn.execute(
        "INSERT INTO parts (id, name, category_id, brand, manufacturer, description, specs, install_notes, warranty, source_id)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
        params![
            id,
            clean(&p.name),
            p.category_id,
            clean(&p.brand),
            clean(&p.manufacturer),
            p.description.trim(),
            specs_json(&p.specs),
            p.install_notes.trim(),
            p.warranty.trim(),
            source_id
        ],
    )?;
    write_identifiers(conn, &id, &p.identifiers)?;
    db::audit(conn, "part", Some(&id), "create", &format!("Added part {}", clean(&p.name)), None)?;
    get(conn, &id)
}

pub fn update(conn: &Connection, id: &str, p: &PartInput) -> AppResult<Part> {
    validate(conn, p)?;
    let n = conn.execute(
        "UPDATE parts SET name = ?2, category_id = ?3, brand = ?4, manufacturer = ?5, description = ?6, specs = ?7, install_notes = ?8,
           warranty = ?9, updated_at = ?10 WHERE id = ?1",
        params![
            id,
            clean(&p.name),
            p.category_id,
            clean(&p.brand),
            clean(&p.manufacturer),
            p.description.trim(),
            specs_json(&p.specs),
            p.install_notes.trim(),
            p.warranty.trim(),
            db::now_utc()
        ],
    )?;
    expect_one(n, "That part")?;
    write_identifiers(conn, id, &p.identifiers)?;
    db::audit(conn, "part", Some(id), "update", &format!("Edited part {}", clean(&p.name)), None)?;
    get(conn, id)
}

pub fn delete(conn: &Connection, id: &str) -> AppResult<()> {
    if db::exists(conn, "SELECT 1 FROM inventory_items WHERE part_id = ?1", [id])? {
        return Err(AppError::Conflict("This part is stocked in inventory. Archive or delete the inventory item first.".into()));
    }
    let n = conn.execute("DELETE FROM parts WHERE id = ?1", [id])?;
    expect_one(n, "That part")?;
    db::audit(conn, "part", Some(id), "delete", "Deleted part", None)?;
    Ok(())
}

pub fn identifiers(conn: &Connection, part_id: &str) -> AppResult<Vec<Identifier>> {
    let mut st = conn.prepare("SELECT kind, value, brand FROM part_identifiers WHERE part_id = ?1 ORDER BY kind, value")?;
    let rows = st.query_map([part_id], |r| Ok(Identifier { kind: r.get(0)?, value: r.get(1)?, brand: r.get(2)? }))?;
    Ok(rows.collect::<Result<_, _>>()?)
}

pub fn get(conn: &Connection, id: &str) -> AppResult<Part> {
    let p = conn
        .query_row(
            "SELECT p.id, p.name, p.category_id, p.brand, p.manufacturer, p.description, p.specs, p.install_notes, p.warranty, p.source_id, d.name,
                    p.created_at, p.updated_at
             FROM parts p JOIN data_sources d ON d.id = p.source_id WHERE p.id = ?1",
            [id],
            |r| {
                let specs: String = r.get(6)?;
                let specs: Vec<(String, String)> = serde_json::from_str::<serde_json::Map<String, serde_json::Value>>(&specs)
                    .unwrap_or_default()
                    .into_iter()
                    .map(|(k, v)| (k, v.as_str().map(str::to_string).unwrap_or_else(|| v.to_string())))
                    .collect();
                Ok(Part {
                    id: r.get(0)?,
                    name: r.get(1)?,
                    category_id: r.get(2)?,
                    category_path: String::new(),
                    brand: r.get(3)?,
                    manufacturer: r.get(4)?,
                    description: r.get(5)?,
                    specs,
                    install_notes: r.get(7)?,
                    warranty: r.get(8)?,
                    source_id: r.get(9)?,
                    source_name: r.get(10)?,
                    identifiers: vec![],
                    created_at: r.get(11)?,
                    updated_at: r.get(12)?,
                })
            },
        )
        .optional()?
        .ok_or_else(|| AppError::not_found("That part"))?;
    let category_path = match &p.category_id {
        Some(c) => category_path(conn, c)?,
        None => String::new(),
    };
    Ok(Part { category_path, identifiers: identifiers(conn, id)?, ..p })
}

#[derive(Serialize, TS, Clone, Debug)]
#[ts(export)]
pub struct PartRow {
    pub id: String,
    pub name: String,
    pub brand: String,
    pub category_id: Option<String>,
    pub category: String,
    /// "OEM 12345 · Barcode 0123..." for the list.
    pub numbers: String,
    pub fitment_count: i64,
    pub on_hand: i64,
    pub available: i64,
}

#[derive(Deserialize, TS, Clone, Debug, Default)]
#[ts(export)]
pub struct PartQuery {
    pub q: Option<String>,
    /// Includes all subcategories.
    pub category_id: Option<String>,
    pub brand: Option<String>,
    pub limit: Option<i64>,
}

pub fn list(conn: &Connection, q: &PartQuery) -> AppResult<Vec<PartRow>> {
    let text = q.q.as_deref().map(str::trim).filter(|t| !t.is_empty());
    let like = text.map(|t| format!("%{}%", t.replace('%', "\\%").replace('_', "\\_")));
    let num = text.map(norm).filter(|n| !n.is_empty());
    let mut st = conn.prepare(
        "WITH RECURSIVE cats(id) AS (SELECT ?1 UNION ALL SELECT c.id FROM part_categories c JOIN cats ON c.parent_id = cats.id)
         SELECT p.id, p.name, p.brand, p.category_id, coalesce(c.name, ''),
           coalesce((SELECT group_concat(upper(substr(kind, 1, 1)) || substr(kind, 2) || ' ' || value, ' · ') FROM part_identifiers WHERE part_id = p.id), ''),
           (SELECT count(*) FROM part_fitments f WHERE f.part_id = p.id),
           coalesce((SELECT sum(s.on_hand) FROM inventory_items i JOIN stock_levels s ON s.item_id = i.id JOIN inventory_locations l ON l.id = s.location_id
                     WHERE i.part_id = p.id AND l.kind = 'warehouse' AND i.archived = 0), 0),
           coalesce((SELECT sum(s.on_hand - s.reserved) FROM inventory_items i JOIN stock_levels s ON s.item_id = i.id JOIN inventory_locations l ON l.id = s.location_id
                     WHERE i.part_id = p.id AND l.kind = 'warehouse' AND i.archived = 0), 0)
         FROM parts p LEFT JOIN part_categories c ON c.id = p.category_id
         WHERE (?1 IS NULL OR p.category_id IN (SELECT id FROM cats))
           AND (?2 IS NULL OR p.brand = ?2 COLLATE NOCASE)
           AND (?3 IS NULL OR p.name LIKE ?3 ESCAPE '\\' OR p.brand LIKE ?3 ESCAPE '\\' OR p.description LIKE ?3 ESCAPE '\\'
                OR EXISTS (SELECT 1 FROM part_identifiers i WHERE i.part_id = p.id AND i.value_norm LIKE '%' || ?4 || '%'))
         ORDER BY p.name COLLATE NOCASE LIMIT ?5",
    )?;
    let rows = st.query_map(
        params![q.category_id, q.brand, like, num.unwrap_or_else(|| "\u{1}".into()), q.limit.unwrap_or(500).clamp(1, 5000)],
        |r| {
            Ok(PartRow {
                id: r.get(0)?,
                name: r.get(1)?,
                brand: r.get(2)?,
                category_id: r.get(3)?,
                category: r.get(4)?,
                numbers: r.get(5)?,
                fitment_count: r.get(6)?,
                on_hand: r.get(7)?,
                available: r.get(8)?,
            })
        },
    )?;
    Ok(rows.collect::<Result<_, _>>()?)
}

/// Parts carrying exactly this number (any identifier kind).
pub fn by_identifier(conn: &Connection, value: &str) -> AppResult<Vec<String>> {
    let n = norm(value);
    if n.is_empty() {
        return Ok(vec![]);
    }
    let mut st = conn.prepare("SELECT DISTINCT part_id FROM part_identifiers WHERE value_norm = ?1")?;
    let rows = st.query_map([n], |r| r.get(0))?;
    Ok(rows.collect::<Result<_, _>>()?)
}

pub fn brands(conn: &Connection) -> AppResult<Vec<String>> {
    let mut st = conn.prepare("SELECT DISTINCT brand FROM parts WHERE brand <> '' ORDER BY brand COLLATE NOCASE")?;
    let rows = st.query_map([], |r| r.get(0))?;
    Ok(rows.collect::<Result<_, _>>()?)
}

// ---------------------------------------------------------------- fitment records

#[derive(Serialize, Deserialize, TS, Clone, Debug, PartialEq)]
#[ts(export)]
pub struct Qualifier {
    pub key: String,
    pub value: String,
}

#[derive(Serialize, TS, Clone, Debug)]
#[ts(export)]
pub struct Fitment {
    pub id: String,
    pub part_id: String,
    pub kind: String,
    pub year_from: i32,
    pub year_to: i32,
    pub make: String,
    pub model: String,
    pub position: String,
    pub quantity: Option<i64>,
    pub notes: String,
    pub source_id: String,
    pub source_name: String,
    pub source_ref: Option<String>,
    pub verified_by: Option<String>,
    pub verified_at: Option<String>,
    pub qualifiers: Vec<Qualifier>,
}

#[derive(Deserialize, TS, Clone, Debug)]
#[ts(export)]
pub struct FitmentInput {
    /// "fits" or "excludes"
    pub kind: String,
    pub year_from: i32,
    pub year_to: i32,
    pub make: String,
    pub model: String,
    #[serde(default)]
    pub position: String,
    pub quantity: Option<i64>,
    #[serde(default)]
    pub notes: String,
    #[serde(default)]
    pub qualifiers: Vec<Qualifier>,
    /// The person has checked this record against an authoritative source.
    #[serde(default)]
    pub verified: bool,
    pub source_ref: Option<String>,
}

fn validate_fitment(f: &FitmentInput) -> AppResult<()> {
    if !matches!(f.kind.as_str(), "fits" | "excludes") {
        return Err(AppError::invalid("kind", "Choose “fits” or “does not fit”."));
    }
    if !(1886..=2100).contains(&f.year_from) || !(1886..=2100).contains(&f.year_to) {
        return Err(AppError::invalid("year_from", "Enter model years between 1886 and 2100."));
    }
    if f.year_from > f.year_to {
        return Err(AppError::invalid("year_to", "The last year must not be before the first."));
    }
    if clean(&f.make).is_empty() {
        return Err(AppError::invalid("make", "Enter the make."));
    }
    if clean(&f.model).is_empty() {
        return Err(AppError::invalid("model", "Enter the model."));
    }
    for q in &f.qualifiers {
        if !crate::specs::valid_key(&q.key) || q.value.trim().is_empty() {
            return Err(AppError::invalid("qualifiers", "Each condition needs a name and a value."));
        }
    }
    Ok(())
}

fn write_qualifiers(conn: &Connection, fitment_id: &str, qs: &[Qualifier]) -> AppResult<()> {
    conn.execute("DELETE FROM fitment_qualifiers WHERE fitment_id = ?1", [fitment_id])?;
    let mut st = conn.prepare("INSERT OR REPLACE INTO fitment_qualifiers (fitment_id, key, value) VALUES (?1, ?2, ?3)")?;
    for q in qs {
        st.execute(params![fitment_id, q.key, q.value.trim()])?;
    }
    Ok(())
}

pub fn add_fitment(conn: &Connection, part_id: &str, f: &FitmentInput, source_id: &str, source_verified: bool) -> AppResult<Fitment> {
    validate_fitment(f)?;
    if !db::exists(conn, "SELECT 1 FROM parts WHERE id = ?1", [part_id])? {
        return Err(AppError::not_found("That part"));
    }
    let id = db::new_id();
    let verified_by = if source_verified {
        Some("source")
    } else if f.verified {
        Some("user")
    } else {
        None
    };
    conn.execute(
        "INSERT INTO part_fitments (id, part_id, kind, year_from, year_to, make, model, position, quantity, notes, source_id, source_ref, verified_by, verified_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, CASE WHEN ?13 IS NULL THEN NULL ELSE ?14 END)",
        params![
            id,
            part_id,
            f.kind,
            f.year_from,
            f.year_to,
            clean(&f.make),
            clean(&f.model),
            clean(&f.position),
            f.quantity,
            f.notes.trim(),
            source_id,
            f.source_ref,
            verified_by,
            db::now_utc()
        ],
    )?;
    write_qualifiers(conn, &id, &f.qualifiers)?;
    get_fitment(conn, &id)
}

pub fn update_fitment(conn: &Connection, id: &str, f: &FitmentInput) -> AppResult<Fitment> {
    validate_fitment(f)?;
    let old = get_fitment(conn, id)?;
    // Editing a record that came verified from a source makes it the user's own claim.
    let verified_by = if f.verified { Some("user") } else { None };
    let n = conn.execute(
        "UPDATE part_fitments SET kind = ?2, year_from = ?3, year_to = ?4, make = ?5, model = ?6, position = ?7, quantity = ?8, notes = ?9,
           source_ref = ?10, verified_by = ?11, verified_at = CASE WHEN ?11 IS NULL THEN NULL ELSE ?12 END,
           source_id = 'user' WHERE id = ?1",
        params![
            id,
            f.kind,
            f.year_from,
            f.year_to,
            clean(&f.make),
            clean(&f.model),
            clean(&f.position),
            f.quantity,
            f.notes.trim(),
            f.source_ref,
            verified_by,
            db::now_utc()
        ],
    )?;
    expect_one(n, "That fitment record")?;
    write_qualifiers(conn, id, &f.qualifiers)?;
    db::audit(conn, "fitment", Some(id), "update", "Edited fitment record", Some(serde_json::json!({"previous_source": old.source_id})))?;
    get_fitment(conn, id)
}

/// Mark a record as checked (or unchecked) by a person.
pub fn verify_fitment(conn: &Connection, id: &str, verified: bool) -> AppResult<Fitment> {
    let n = conn.execute(
        "UPDATE part_fitments SET verified_by = CASE WHEN ?2 THEN 'user' ELSE NULL END, verified_at = CASE WHEN ?2 THEN ?3 ELSE NULL END WHERE id = ?1",
        params![id, verified, db::now_utc()],
    )?;
    expect_one(n, "That fitment record")?;
    db::audit(
        conn,
        "fitment",
        Some(id),
        if verified { "verify" } else { "unverify" },
        if verified { "Fitment verified" } else { "Fitment verification removed" },
        None,
    )?;
    get_fitment(conn, id)
}

pub fn delete_fitment(conn: &Connection, id: &str) -> AppResult<()> {
    let n = conn.execute("DELETE FROM part_fitments WHERE id = ?1", [id])?;
    expect_one(n, "That fitment record")
}

const FITMENT_SELECT: &str =
    "SELECT f.id, f.part_id, f.kind, f.year_from, f.year_to, f.make, f.model, f.position, f.quantity, f.notes, f.source_id, d.name,
    f.source_ref, f.verified_by, f.verified_at FROM part_fitments f JOIN data_sources d ON d.id = f.source_id";

fn fitment_row(r: &rusqlite::Row) -> rusqlite::Result<Fitment> {
    Ok(Fitment {
        id: r.get(0)?,
        part_id: r.get(1)?,
        kind: r.get(2)?,
        year_from: r.get(3)?,
        year_to: r.get(4)?,
        make: r.get(5)?,
        model: r.get(6)?,
        position: r.get(7)?,
        quantity: r.get(8)?,
        notes: r.get(9)?,
        source_id: r.get(10)?,
        source_name: r.get(11)?,
        source_ref: r.get(12)?,
        verified_by: r.get(13)?,
        verified_at: r.get(14)?,
        qualifiers: vec![],
    })
}

fn with_qualifiers(conn: &Connection, mut list: Vec<Fitment>) -> AppResult<Vec<Fitment>> {
    let mut st = conn.prepare("SELECT key, value FROM fitment_qualifiers WHERE fitment_id = ?1 ORDER BY key")?;
    for f in &mut list {
        f.qualifiers = st.query_map([&f.id], |r| Ok(Qualifier { key: r.get(0)?, value: r.get(1)? }))?.collect::<Result<_, _>>()?;
    }
    Ok(list)
}

pub fn get_fitment(conn: &Connection, id: &str) -> AppResult<Fitment> {
    let f = conn
        .query_row(&format!("{FITMENT_SELECT} WHERE f.id = ?1"), [id], fitment_row)
        .optional()?
        .ok_or_else(|| AppError::not_found("That fitment record"))?;
    Ok(with_qualifiers(conn, vec![f])?.remove(0))
}

pub fn fitments_for_part(conn: &Connection, part_id: &str) -> AppResult<Vec<Fitment>> {
    let mut st = conn.prepare(&format!("{FITMENT_SELECT} WHERE f.part_id = ?1 ORDER BY f.make, f.model, f.year_from"))?;
    let list = st.query_map([part_id], fitment_row)?.collect::<Result<Vec<_>, _>>()?;
    with_qualifiers(conn, list)
}

/// Records whose make/model match and whose years include `year`.
pub fn fitments_for_vehicle(conn: &Connection, year: i32, make: &str, model: &str) -> AppResult<Vec<Fitment>> {
    let mut st = conn.prepare(&format!(
        "{FITMENT_SELECT} WHERE f.make = ?1 COLLATE NOCASE AND f.model = ?2 COLLATE NOCASE AND ?3 BETWEEN f.year_from AND f.year_to ORDER BY f.part_id"
    ))?;
    let list = st.query_map(params![make.trim(), model.trim(), year], fitment_row)?.collect::<Result<Vec<_>, _>>()?;
    with_qualifiers(conn, list)
}

#[cfg(test)]
mod tests {
    use super::*;

    pub fn brake_pad(conn: &Connection) -> Part {
        create(
            conn,
            &PartInput {
                name: "Front brake pad set".into(),
                category_id: Some("brakes.pads".into()),
                brand: "Acme".into(),
                identifiers: vec![
                    Identifier { kind: "aftermarket".into(), value: "ACM-1234".into(), brand: "Acme".into() },
                    Identifier { kind: "barcode".into(), value: "0123456789012".into(), brand: String::new() },
                ],
                ..Default::default()
            },
            "user",
        )
        .unwrap()
    }

    #[test]
    fn create_find_and_filter() {
        let conn = db::open_memory().unwrap();
        let p = brake_pad(&conn);
        assert_eq!(p.category_path, "Braking › Brake pads");
        assert_eq!(by_identifier(&conn, "acm 1234").unwrap(), vec![p.id.clone()]);
        let by_cat = list(&conn, &PartQuery { category_id: Some("brakes".into()), ..Default::default() }).unwrap();
        assert_eq!(by_cat.len(), 1, "parent category includes subcategories");
        let by_num = list(&conn, &PartQuery { q: Some("1234".into()), ..Default::default() }).unwrap();
        assert_eq!(by_num.len(), 1);
        assert!(list(&conn, &PartQuery { category_id: Some("engine".into()), ..Default::default() }).unwrap().is_empty());
        assert!(create(&conn, &PartInput { name: " ".into(), ..Default::default() }, "user").is_err());
    }

    #[test]
    fn fitment_records_and_verification() {
        let conn = db::open_memory().unwrap();
        let p = brake_pad(&conn);
        let f = add_fitment(
            &conn,
            &p.id,
            &FitmentInput {
                kind: "fits".into(),
                year_from: 2017,
                year_to: 2019,
                make: "Nissan".into(),
                model: "Sentra".into(),
                position: "Front".into(),
                quantity: Some(1),
                notes: String::new(),
                qualifiers: vec![Qualifier { key: "engine_displacement_l".into(), value: "1.8".into() }],
                verified: false,
                source_ref: None,
            },
            "user",
            false,
        )
        .unwrap();
        assert_eq!(f.verified_by, None);
        assert_eq!(fitments_for_vehicle(&conn, 2019, "NISSAN", "sentra").unwrap().len(), 1);
        assert!(fitments_for_vehicle(&conn, 2020, "Nissan", "Sentra").unwrap().is_empty());
        let v = verify_fitment(&conn, &f.id, true).unwrap();
        assert_eq!(v.verified_by.as_deref(), Some("user"));
        assert_eq!(v.qualifiers.len(), 1);
    }
}
