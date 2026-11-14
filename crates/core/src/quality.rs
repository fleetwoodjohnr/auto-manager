//! Data Quality Center: places where the data is incomplete, contradictory or unverified, so
//! they can be fixed instead of silently trusted.

use crate::error::AppResult;
use rusqlite::Connection;
use serde::Serialize;
use ts_rs::TS;

#[derive(Serialize, TS, Clone, Debug)]
#[ts(export)]
pub struct Issue {
    /// What links to: "vehicle", "part", "item"
    pub kind: String,
    pub id: String,
    pub title: String,
    pub detail: String,
}

#[derive(Serialize, TS, Clone, Debug)]
#[ts(export)]
pub struct IssueGroup {
    pub id: String,
    pub title: String,
    pub help: String,
    pub issues: Vec<Issue>,
}

fn group(conn: &Connection, id: &str, title: &str, help: &str, kind: &str, sql: &str) -> AppResult<IssueGroup> {
    let mut st = conn.prepare(sql)?;
    let issues = st
        .query_map([], |r| Ok(Issue { kind: kind.into(), id: r.get(0)?, title: r.get(1)?, detail: r.get(2)? }))?
        .collect::<Result<_, _>>()?;
    Ok(IssueGroup { id: id.into(), title: title.into(), help: help.into(), issues })
}

pub fn report(conn: &Connection) -> AppResult<Vec<IssueGroup>> {
    let vt = "trim(coalesce(v.year, '') || ' ' || v.make || ' ' || v.model || ' ' || v.trim)";
    let mut out = vec![
        group(
            conn,
            "spec_conflicts",
            "Conflicting specifications",
            "Two sources give different values. Check the vehicle or its documents and record the right value.",
            "vehicle",
            &format!(
                "SELECT v.id, {vt}, group_concat(DISTINCT s.key) FROM vehicles v JOIN vehicle_specifications s ON s.vehicle_id = v.id
                 WHERE s.source_id <> 'user' AND EXISTS (SELECT 1 FROM vehicle_specifications s2 WHERE s2.vehicle_id = s.vehicle_id AND s2.key = s.key
                   AND s2.source_id <> s.source_id AND s2.source_id <> 'user' AND lower(s2.value) <> lower(s.value)
                   AND NOT (lower(s2.value) LIKE '%' || lower(s.value) || '%' OR lower(s.value) LIKE '%' || lower(s2.value) || '%')
                   AND NOT (s.key = 'drive_type'))
                 AND NOT EXISTS (SELECT 1 FROM vehicle_specifications u WHERE u.vehicle_id = s.vehicle_id AND u.key = s.key AND u.source_id = 'user')
                 GROUP BY v.id"
            ),
        )?,
        group(
            conn,
            "missing_fitment_specs",
            "Saved vehicles missing matching details",
            "Without engine size, drivetrain or trim, parts can only be offered as conditional fits.",
            "vehicle",
            &format!(
                "SELECT v.id, {vt}, 'Missing: ' || trim(
                   CASE WHEN c.engine_displacement_l IS NULL THEN 'engine size, ' ELSE '' END ||
                   CASE WHEN c.drive_type IS NULL THEN 'drivetrain, ' ELSE '' END ||
                   CASE WHEN c.trim IS NULL THEN 'trim, ' ELSE '' END, ', ')
                 FROM vehicles v JOIN vehicle_configuration c ON c.vehicle_id = v.id
                 WHERE v.saved = 1 AND (c.engine_displacement_l IS NULL OR c.drive_type IS NULL OR c.trim IS NULL)"
            ),
        )?,
        group(
            conn,
            "unverified_fitment",
            "Parts with only unverified fitment",
            "These can show as “possible fit” at best until someone checks a record against an authoritative source.",
            "part",
            "SELECT p.id, p.name, count(f.id) || ' unverified record(s)' FROM parts p JOIN part_fitments f ON f.part_id = p.id
             GROUP BY p.id HAVING sum(f.verified_by IS NOT NULL) = 0",
        )?,
        group(
            conn,
            "unresolved_qualifiers",
            "Fitment conditions no vehicle can meet",
            "Imported ACES attributes that weren't mapped to a vehicle detail. Import an attribute map or edit the records.",
            "part",
            "SELECT DISTINCT p.id, p.name, group_concat(DISTINCT q.key) FROM parts p JOIN part_fitments f ON f.part_id = p.id JOIN fitment_qualifiers q ON q.fitment_id = f.id
             WHERE q.key LIKE 'aces.%' GROUP BY p.id",
        )?,
        group(
            conn,
            "stocked_without_fitment",
            "Stocked parts without fitment records",
            "Never suggested for a vehicle until fitment is added.",
            "part",
            "SELECT p.id, p.name, i.sku FROM parts p JOIN inventory_items i ON i.part_id = p.id
             WHERE i.archived = 0 AND NOT EXISTS (SELECT 1 FROM part_fitments f WHERE f.part_id = p.id) GROUP BY p.id",
        )?,
        group(
            conn,
            "no_category",
            "Parts without a category",
            "Categories drive browsing, category pricing rules and reports.",
            "part",
            "SELECT id, name, brand FROM parts WHERE category_id IS NULL",
        )?,
        group(
            conn,
            "no_cost",
            "Items with no cost",
            "Inventory value, margins and markup prices are wrong while the cost is zero.",
            "item",
            "SELECT i.id, p.name, i.sku FROM inventory_items i JOIN parts p ON p.id = i.part_id WHERE i.archived = 0 AND CAST(i.cost AS REAL) = 0 AND CAST(i.avg_cost AS REAL) = 0",
        )?,
        group(
            conn,
            "no_location_bin",
            "Stock without a shelf or bin",
            "Recording where things are speeds up picking.",
            "item",
            "SELECT DISTINCT i.id, p.name, i.sku FROM inventory_items i JOIN parts p ON p.id = i.part_id JOIN stock_levels s ON s.item_id = i.id
             WHERE i.archived = 0 AND s.on_hand > 0 AND s.bin = ''",
        )?,
    ];
    // Items with no price under the default profile need the pricing engine, not SQL.
    let default = crate::pricing::default_profile(conn)?;
    let mut no_price = Vec::new();
    for i in crate::inventory::list(conn, &crate::inventory::ItemQuery::default())? {
        if crate::pricing::price_item(conn, &i.id, 1, &[Some(default.id.as_str())], "average")?.price.is_none() {
            no_price.push(Issue { kind: "item".into(), id: i.id, title: i.name, detail: i.sku });
        }
    }
    out.push(IssueGroup {
        id: "no_price".into(),
        title: "Items without a selling price".into(),
        help: format!("No rule in “{}” applies and no fixed price is set. Add a pricing rule or a price.", default.name),
        issues: no_price,
    });
    Ok(out)
}

#[cfg(test)]
mod tests {
    #[test]
    fn finds_gaps() {
        let conn = crate::db::open_memory().unwrap();
        conn.execute_batch(
            "INSERT INTO parts (id, name) VALUES ('p1', 'Pads');
             INSERT INTO inventory_items (id, part_id, sku) VALUES ('i1', 'p1', 'BP');
             INSERT INTO vehicles (id, year, make, model, saved) VALUES ('v1', 2019, 'Nissan', 'Sentra', 1);
             INSERT INTO vehicle_specifications (vehicle_id, key, value, source_id, confidence) VALUES ('v1', 'fuel_type', 'Diesel', 'nhtsa_vpic', 'vin_decoded'), ('v1', 'fuel_type', 'Gasoline', 'epa_fueleconomy', 'model_level');",
        )
        .unwrap();
        let r = super::report(&conn).unwrap();
        let n = |id: &str| r.iter().find(|g| g.id == id).unwrap().issues.len();
        assert_eq!(n("spec_conflicts"), 1);
        assert_eq!(n("missing_fitment_specs"), 1);
        assert_eq!(n("stocked_without_fitment"), 1);
        assert_eq!(n("no_category"), 1);
        assert_eq!(n("no_cost"), 1);
        assert_eq!(n("no_price"), 1);
    }
}
