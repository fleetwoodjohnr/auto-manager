//! "Parts for this vehicle": every catalog part with fitment records for the vehicle, its fit
//! status, and the stock and price of each inventory item that stocks it.

use crate::error::AppResult;
use crate::fitment::{self, FitResult, FitStatus, Requirement, VehicleFacts};
use crate::parts::{self, Fitment};
use crate::{inventory, pricing, vehicles};
use rusqlite::Connection;
use rust_decimal::Decimal;
use serde::Serialize;
use std::collections::BTreeMap;
use ts_rs::TS;

#[derive(Serialize, TS, Clone, Debug)]
#[ts(export)]
pub struct StockOption {
    pub item_id: String,
    pub sku: String,
    pub condition: String,
    pub on_hand: i64,
    pub reserved: i64,
    pub available: i64,
    /// "ok", "low" or "out"
    pub stock_state: String,
    pub bins: String,
    #[ts(type = "string | null")]
    pub price: Option<Decimal>,
}

#[derive(Serialize, TS, Clone, Debug)]
#[ts(export)]
pub struct PartMatch {
    pub part_id: String,
    pub name: String,
    pub brand: String,
    pub category_id: Option<String>,
    pub category: String,
    pub numbers: String,
    pub fit: FitResult,
    pub items: Vec<StockOption>,
    /// The newest part that replaces this one, if it has been superseded: (part id, name).
    pub superseded_by: Option<(String, String)>,
}

#[derive(Serialize, TS, Clone, Debug)]
#[ts(export)]
pub struct VehicleMatches {
    pub vehicle_id: String,
    pub matches: Vec<PartMatch>,
    /// Details that would resolve conditional matches, with how many matches each affects.
    pub missing: Vec<(Requirement, i64)>,
    pub counts: BTreeMap<String, i64>,
}

pub fn facts(conn: &Connection, vehicle_id: &str) -> AppResult<VehicleFacts> {
    let v = vehicles::get(conn, vehicle_id)?;
    let attrs = vehicles::effective(conn, vehicle_id)?;
    let make = attrs.get("make").cloned().unwrap_or(v.make);
    let model = attrs.get("model").cloned().unwrap_or(v.model);
    Ok(VehicleFacts { year: v.year, make, model, attrs })
}

pub fn fit_for(conn: &Connection, part_id: &str, vehicle_id: &str) -> AppResult<FitResult> {
    let f = facts(conn, vehicle_id)?;
    Ok(fitment::evaluate(&f, &parts::fitments_for_part(conn, part_id)?))
}

fn status_key(s: FitStatus) -> &'static str {
    match s {
        FitStatus::Confirmed => "confirmed",
        FitStatus::Conditional => "conditional",
        FitStatus::Possible => "possible",
        FitStatus::NotCompatible => "not_compatible",
        FitStatus::Unknown => "unknown",
    }
}

pub fn for_vehicle(
    conn: &Connection,
    vehicle_id: &str,
    category_id: Option<&str>,
    in_stock_only: bool,
    valuation: &str,
) -> AppResult<VehicleMatches> {
    let f = facts(conn, vehicle_id)?;
    let mut by_part: BTreeMap<String, Vec<Fitment>> = BTreeMap::new();
    if let Some(year) = f.year {
        for rec in parts::fitments_for_vehicle(conn, year, &f.make, &f.model)? {
            by_part.entry(rec.part_id.clone()).or_default().push(rec);
        }
    }
    let wanted: Option<Vec<String>> = match category_id {
        Some(c) => {
            let mut st = conn.prepare("WITH RECURSIVE cats(id) AS (SELECT ?1 UNION ALL SELECT c.id FROM part_categories c JOIN cats ON c.parent_id = cats.id) SELECT id FROM cats")?;
            Some(st.query_map([c], |r| r.get(0))?.collect::<Result<_, _>>()?)
        }
        None => None,
    };
    let profile = pricing::default_profile(conn)?;
    let mut matches = Vec::new();
    for (part_id, records) in by_part {
        let part = parts::get(conn, &part_id)?;
        if let Some(w) = &wanted
            && !part.category_id.as_ref().is_some_and(|c| w.contains(c))
        {
            continue;
        }
        let fit = fitment::evaluate(&f, &records);
        let mut items = Vec::new();
        for i in inventory::for_part(conn, &part_id)? {
            let price = pricing::price_item(conn, &i.id, 1, &[Some(profile.id.as_str())], valuation)?.price;
            items.push(StockOption {
                stock_state: i.stock_state().into(),
                item_id: i.id,
                sku: i.sku,
                condition: i.condition,
                on_hand: i.on_hand,
                reserved: i.reserved,
                available: i.available,
                bins: i.bins,
                price,
            });
        }
        if in_stock_only && !items.iter().any(|i| i.on_hand > 0) {
            continue;
        }
        let numbers = part.identifiers.iter().filter(|i| i.kind != "barcode").map(|i| i.value.clone()).collect::<Vec<_>>().join(" · ");
        let latest = crate::interchange::latest(conn, &part_id)?;
        let superseded_by = if latest != part_id { Some((latest.clone(), parts::get(conn, &latest)?.name)) } else { None };
        matches.push(PartMatch {
            superseded_by,
            part_id,
            name: part.name,
            brand: part.brand,
            category_id: part.category_id,
            category: part.category_path,
            numbers,
            fit,
            items,
        });
    }
    matches.sort_by(|a, b| a.fit.status.cmp(&b.fit.status).then(a.category.cmp(&b.category)).then(a.name.cmp(&b.name)));
    let mut counts = BTreeMap::new();
    let mut missing: Vec<(Requirement, i64)> = Vec::new();
    for m in &matches {
        *counts.entry(status_key(m.fit.status).to_string()).or_insert(0) += 1;
        for r in &m.fit.requirements {
            match missing.iter_mut().find(|(x, _)| x.key == r.key) {
                Some((_, n)) => *n += 1,
                None => missing.push((Requirement { value: String::new(), ..r.clone() }, 1)),
            }
        }
    }
    Ok(VehicleMatches { vehicle_id: vehicle_id.into(), matches, missing, counts })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{db, inventory as inv, parts::*, providers::vpic};

    #[test]
    fn parts_in_stock_for_a_decoded_vehicle() {
        let conn = db::open_memory().unwrap();
        let body =
            std::fs::read_to_string(format!("{}/tests/fixtures/vpic/decode_3N1AB7AP4KY250873.json", env!("CARGO_MANIFEST_DIR"))).unwrap();
        let v = vehicles::store_decoded(&conn, &vpic::parse_decode(&body).unwrap(), vpic::SOURCE).unwrap();
        let mk = |name: &str, cat: &str| {
            create(&conn, &PartInput { name: name.into(), category_id: Some(cat.into()), ..Default::default() }, "user").unwrap()
        };
        let pads = mk("Front pads", "brakes.pads");
        let filter = mk("Oil filter", "engine.oil_filter");
        let other = mk("Pads for another car", "brakes.pads");
        let fit = |part: &str, verified: bool, qs: Vec<Qualifier>| {
            add_fitment(
                &conn,
                part,
                &FitmentInput {
                    kind: "fits".into(),
                    year_from: 2016,
                    year_to: 2019,
                    make: "Nissan".into(),
                    model: "Sentra".into(),
                    position: String::new(),
                    quantity: None,
                    notes: String::new(),
                    qualifiers: qs,
                    verified,
                    source_ref: None,
                },
                "user",
                false,
            )
            .unwrap()
        };
        fit(&pads.id, true, vec![Qualifier { key: "brake_package".into(), value: "Standard".into() }]);
        fit(&filter.id, true, vec![Qualifier { key: "engine_displacement_l".into(), value: "1.8".into() }]);
        add_fitment(
            &conn,
            &other.id,
            &FitmentInput {
                kind: "fits".into(),
                year_from: 2019,
                year_to: 2019,
                make: "Honda".into(),
                model: "Civic".into(),
                position: String::new(),
                quantity: None,
                notes: String::new(),
                qualifiers: vec![],
                verified: true,
                source_ref: None,
            },
            "user",
            false,
        )
        .unwrap();
        let item = inv::create(
            &conn,
            &filter.id,
            &inv::ItemInput {
                sku: "OF-1".into(),
                condition: "new".into(),
                unit: "each".into(),
                cost: "4".into(),
                price_override: Some("9.99".into()),
                pricing_profile_id: None,
                min_stock: 0,
                reorder_qty: 0,
                supplier_id: None,
                warranty: String::new(),
                notes: String::new(),
            },
        )
        .unwrap();
        inv::receive(
            &conn,
            &inv::Receive {
                item_id: item.id.clone(),
                location_id: "loc-main".into(),
                qty: 8,
                unit_cost: None,
                bin: None,
                reason: String::new(),
                ref_kind: None,
                ref_id: None,
            },
        )
        .unwrap();

        let m = for_vehicle(&conn, &v.id, None, false, "average").unwrap();
        assert_eq!(m.matches.len(), 2, "the Civic part is not listed");
        assert_eq!(m.matches[0].name, "Oil filter");
        assert_eq!(m.matches[0].fit.status, FitStatus::Confirmed);
        assert_eq!(m.matches[0].items[0].available, 8);
        assert_eq!(m.matches[0].items[0].price.map(|p| p.to_string()), Some("9.99".into()));
        assert_eq!(m.matches[1].fit.status, FitStatus::Conditional);
        assert_eq!(m.missing[0].0.key, "brake_package");
        let stocked = for_vehicle(&conn, &v.id, None, true, "average").unwrap();
        assert_eq!(stocked.matches.len(), 1);
        let brakes = for_vehicle(&conn, &v.id, Some("brakes"), false, "average").unwrap();
        assert_eq!(brakes.matches.len(), 1);
        // Supplying the missing detail resolves the conditional match.
        vehicles::set_spec(
            &conn,
            &v.id,
            &vehicles::SpecInput { key: "brake_package".into(), value: "Standard".into(), unit: None, source_ref: None },
        )
        .unwrap();
        assert_eq!(fit_for(&conn, &pads.id, &v.id).unwrap().status, FitStatus::Confirmed);
    }
}
