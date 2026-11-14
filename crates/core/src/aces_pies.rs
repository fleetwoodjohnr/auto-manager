//! ACES (fitment) and PIES (product information) XML import, the Auto Care Association
//! formats most aftermarket catalogs are delivered in.
//!
//! ACES applications identify vehicles by VCdb ids (BaseVehicle, EngineBase, SubModel, …),
//! which are licensed reference data. Auto Manager doesn't ship VCdb. To import ACES the person
//! supplies a vehicle map exported from their licensed VCdb (BaseVehicleID → year, make,
//! model) and, optionally, an attribute map (e.g. EngineBase 1234 → engine_displacement_l 2.0).
//! Applications that can't be resolved are reported and skipped, never guessed.
//!
//! The XML reader doesn't process DTDs or external entities.

use crate::csvio;
use crate::db;
use crate::error::{AppError, AppResult};
use crate::parts::{self, FitmentInput, Identifier, PartInput, Qualifier};
use quick_xml::events::Event;
use quick_xml::{Reader, XmlVersion};
use rusqlite::{Connection, OptionalExtension, params};
use serde::Serialize;
use std::collections::HashMap;
use std::path::Path;
use ts_rs::TS;

const MAX_XML: u64 = 200 * 1024 * 1024;

fn read_xml(path: &Path) -> AppResult<String> {
    if std::fs::metadata(path)?.len() > MAX_XML {
        return Err(AppError::msg("That file is larger than 200 MB; split it first."));
    }
    let s = std::fs::read_to_string(path).map_err(|_| AppError::msg("That file isn't readable text XML."))?;
    if s.contains("<!ENTITY") {
        return Err(AppError::msg("This XML declares entities, which Auto Manager doesn't accept for safety."));
    }
    Ok(s)
}

fn attr(e: &quick_xml::events::BytesStart, name: &str) -> Option<String> {
    e.attributes()
        .flatten()
        .find(|a| a.key.as_ref() == name)
        .and_then(|a| a.normalized_value(XmlVersion::Implicit1_0).ok())
        .map(|v| v.to_string())
}

/// The five predefined entities and numeric character references; anything else is dropped.
fn entity(name: &str) -> String {
    match name {
        "amp" => "&".into(),
        "lt" => "<".into(),
        "gt" => ">".into(),
        "quot" => "\"".into(),
        "apos" => "'".into(),
        n if n.starts_with("#x") => u32::from_str_radix(&n[2..], 16).ok().and_then(char::from_u32).map(String::from).unwrap_or_default(),
        n if n.starts_with('#') => n[1..].parse().ok().and_then(char::from_u32).map(String::from).unwrap_or_default(),
        _ => String::new(),
    }
}

fn xml_err(e: quick_xml::Error) -> AppError {
    AppError::msg(format!("The XML file is malformed: {e}"))
}

// ---------------------------------------------------------------- ACES

#[derive(Serialize, TS, Clone, Debug, Default)]
#[ts(export)]
pub struct AcesApp {
    pub id: String,
    pub base_vehicle: Option<String>,
    pub years: Option<(i32, i32)>,
    pub make_id: Option<String>,
    pub model_id: Option<String>,
    /// Other VCdb attributes, e.g. ("EngineBase", "1234").
    pub attributes: Vec<(String, String)>,
    pub part: String,
    pub qty: Option<i64>,
    pub position_id: Option<String>,
    pub notes: Vec<String>,
    pub delete: bool,
}

/// Parse the <App> elements of an ACES file.
pub fn parse_aces(xml: &str) -> AppResult<Vec<AcesApp>> {
    let mut r = Reader::from_str(xml);
    r.config_mut().trim_text(true);
    let mut apps = Vec::new();
    let mut cur: Option<AcesApp> = None;
    let mut buf = String::new();
    loop {
        match r.read_event().map_err(xml_err)? {
            Event::Start(e) | Event::Empty(e) => {
                buf.clear();
                let name = e.name().as_ref().to_string();
                if name == "App" {
                    cur = Some(AcesApp {
                        id: attr(&e, "id").unwrap_or_default(),
                        delete: attr(&e, "action").as_deref() == Some("D"),
                        ..Default::default()
                    });
                    continue;
                }
                let Some(app) = cur.as_mut() else { continue };
                match name.as_str() {
                    "BaseVehicle" => app.base_vehicle = attr(&e, "id"),
                    "Years" => {
                        let f = attr(&e, "from").and_then(|v| v.parse().ok());
                        let t = attr(&e, "to").and_then(|v| v.parse().ok());
                        app.years = f.zip(t);
                    }
                    "Make" => app.make_id = attr(&e, "id"),
                    "Model" => app.model_id = attr(&e, "id"),
                    "Position" => app.position_id = attr(&e, "id"),
                    "Part" | "Qty" | "Note" | "PartType" | "MfrLabel" | "Mfr" => {}
                    other => {
                        if let Some(id) = attr(&e, "id") {
                            app.attributes.push((other.to_string(), id));
                        }
                    }
                }
            }
            Event::Text(t) => buf.push_str(&t.xml_content(XmlVersion::Implicit1_0)),
            Event::GeneralRef(g) => buf.push_str(&entity(&g)),
            Event::End(e) => {
                let name = e.name().as_ref().to_string();
                let v = buf.trim().to_string();
                buf.clear();
                match (name.as_str(), cur.as_mut()) {
                    ("App", _) => {
                        if let Some(a) = cur.take() {
                            apps.push(a);
                        }
                    }
                    ("Part", Some(app)) => app.part = v,
                    ("Qty", Some(app)) => app.qty = v.parse().ok(),
                    ("Note", Some(app)) => app.notes.push(v),
                    _ => {}
                }
            }
            Event::Eof => break,
            _ => {}
        }
    }
    Ok(apps)
}

/// A user-supplied VCdb export: BaseVehicleID → (year, make, model).
pub type VehicleMap = HashMap<String, (i32, String, String)>;
/// ("EngineBase", "1234") → (spec key, value)
pub type AttributeMap = HashMap<(String, String), (String, String)>;

/// CSV with columns base_vehicle_id, year, make, model (header names flexible).
pub fn load_vehicle_map(path: &Path) -> AppResult<VehicleMap> {
    let t = csvio::read_table(path)?;
    let col = |names: &[&str]| t.headers.iter().position(|h| names.iter().any(|n| h.to_lowercase().replace([' ', '_'], "") == *n));
    let (Some(id), Some(y), Some(mk), Some(md)) =
        (col(&["basevehicleid", "basevehicle", "id"]), col(&["year", "yearid"]), col(&["make", "makename"]), col(&["model", "modelname"]))
    else {
        return Err(AppError::msg("The vehicle map needs columns: BaseVehicleID, Year, Make, Model."));
    };
    let mut m = HashMap::new();
    for r in &t.rows {
        if let (Some(i), Some(year)) = (r.get(id), r.get(y).and_then(|v| v.trim().parse().ok())) {
            m.insert(i.trim().to_string(), (year, r.get(mk).cloned().unwrap_or_default(), r.get(md).cloned().unwrap_or_default()));
        }
    }
    Ok(m)
}

/// CSV with columns attribute, id, key, value: e.g. EngineBase,1234,engine_displacement_l,2.0.
pub fn load_attribute_map(path: &Path) -> AppResult<AttributeMap> {
    let t = csvio::read_table(path)?;
    if t.headers.len() < 4 {
        return Err(AppError::msg("The attribute map needs columns: attribute, id, key, value."));
    }
    Ok(t.rows
        .iter()
        .filter(|r| r.len() >= 4)
        .map(|r| ((r[0].trim().to_string(), r[1].trim().to_string()), (r[2].trim().to_string(), r[3].trim().to_string())))
        .collect())
}

#[derive(Serialize, TS, Clone, Debug, Default)]
#[ts(export)]
pub struct AcesSummary {
    pub apps: i64,
    pub imported: i64,
    pub deletes_skipped: i64,
    /// App ids whose BaseVehicle isn't in the vehicle map.
    pub unresolved_vehicles: Vec<String>,
    /// Part numbers not found in the catalog (import PIES or the parts first).
    pub unknown_parts: Vec<String>,
    /// Attributes kept as unresolved qualifiers (vehicles can't match them until mapped).
    pub unmapped_attributes: Vec<String>,
}

/// Import ACES applications as fitment records under a new import source. `authoritative`
/// marks the records verified by their source.
pub fn import_aces(
    conn: &Connection,
    apps: &[AcesApp],
    vehicles: &VehicleMap,
    attrs: &AttributeMap,
    source_name: &str,
    authoritative: bool,
) -> AppResult<AcesSummary> {
    let mut s = AcesSummary { apps: apps.len() as i64, ..Default::default() };
    let source_id = format!("import-{}", db::new_id());
    conn.execute(
        "INSERT INTO data_sources (id, name, kind, priority) VALUES (?1, ?2, 'import', 50)",
        params![source_id, format!("ACES: {}", db::clean(source_name))],
    )?;
    let mut unmapped = std::collections::BTreeSet::new();
    for a in apps {
        if a.delete {
            s.deletes_skipped += 1;
            continue;
        }
        let Some((year, make, model)) = a.base_vehicle.as_ref().and_then(|b| vehicles.get(b)).cloned() else {
            s.unresolved_vehicles.push(a.id.clone());
            continue;
        };
        let found = parts::by_identifier(conn, &a.part)?;
        let Some(part_id) = found.first() else {
            if !s.unknown_parts.contains(&a.part) {
                s.unknown_parts.push(a.part.clone());
            }
            continue;
        };
        let mut qualifiers = Vec::new();
        for (name, id) in &a.attributes {
            match attrs.get(&(name.clone(), id.clone())) {
                Some((k, v)) => qualifiers.push(Qualifier { key: k.clone(), value: v.clone() }),
                None => {
                    unmapped.insert(format!("{name} {id}"));
                    qualifiers.push(Qualifier { key: format!("aces.{}", name.to_lowercase()), value: id.clone() });
                }
            }
        }
        let (yf, yt) = a.years.unwrap_or((year, year));
        parts::add_fitment(
            conn,
            part_id,
            &FitmentInput {
                kind: "fits".into(),
                year_from: yf,
                year_to: yt,
                make,
                model,
                position: a.position_id.as_ref().map(|p| format!("ACES position {p}")).unwrap_or_default(),
                quantity: a.qty,
                notes: a.notes.join("; "),
                qualifiers,
                verified: false,
                source_ref: Some(format!("{source_name} App {}", a.id)),
            },
            &source_id,
            authoritative,
        )?;
        s.imported += 1;
    }
    s.unmapped_attributes = unmapped.into_iter().collect();
    db::audit(conn, "import", Some(&source_id), "aces", &format!("Imported {} ACES applications", s.imported), None)?;
    Ok(s)
}

pub fn import_aces_files(
    conn: &Connection,
    aces: &Path,
    vehicle_map: &Path,
    attribute_map: Option<&Path>,
    authoritative: bool,
) -> AppResult<AcesSummary> {
    let apps = parse_aces(&read_xml(aces)?)?;
    let vm = load_vehicle_map(vehicle_map)?;
    if vm.is_empty() {
        return Err(AppError::msg("The vehicle map has no rows. ACES needs a VCdb export to know which vehicles each application covers."));
    }
    let am = match attribute_map {
        Some(p) => load_attribute_map(p)?,
        None => HashMap::new(),
    };
    let name = aces.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
    import_aces(conn, &apps, &vm, &am, &name, authoritative)
}

// ---------------------------------------------------------------- PIES

#[derive(Serialize, TS, Clone, Debug, Default)]
#[ts(export)]
pub struct PiesItem {
    pub part_number: String,
    pub brand: String,
    pub name: String,
    pub description: String,
    pub gtin: Option<String>,
    pub attributes: Vec<(String, String)>,
    pub prices: Vec<(String, String)>,
    pub interchange: Vec<String>,
}

pub fn parse_pies(xml: &str) -> AppResult<Vec<PiesItem>> {
    let mut r = Reader::from_str(xml);
    r.config_mut().trim_text(true);
    let mut items = Vec::new();
    let mut cur: Option<PiesItem> = None;
    let mut stack: Vec<String> = Vec::new();
    let mut desc_code = String::new();
    let mut attr_id = String::new();
    let mut price_type = String::new();
    let mut buf = String::new();
    loop {
        match r.read_event().map_err(xml_err)? {
            Event::Start(e) => {
                buf.clear();
                let name = e.name().as_ref().to_string();
                match name.as_str() {
                    "Item" => cur = Some(PiesItem::default()),
                    "Description" => desc_code = attr(&e, "DescriptionCode").unwrap_or_default(),
                    "ProductAttribute" => attr_id = attr(&e, "AttributeID").unwrap_or_default(),
                    "Pricing" => price_type = attr(&e, "PriceType").unwrap_or_default(),
                    _ => {}
                }
                stack.push(name);
            }
            Event::Text(t) => buf.push_str(&t.xml_content(XmlVersion::Implicit1_0)),
            Event::GeneralRef(g) => buf.push_str(&entity(&g)),
            Event::End(e) => {
                let v = buf.trim().to_string();
                buf.clear();
                if let Some(item) = cur.as_mut() {
                    let inside = |n: &str| stack.iter().any(|s| s == n);
                    match stack.last().map(String::as_str) {
                        Some("PartNumber") if inside("PartInterchange") => item.interchange.push(v),
                        Some("PartNumber") => item.part_number = v,
                        Some("BrandLabel") => item.brand = v,
                        Some("ItemLevelGTIN") => item.gtin = Some(v),
                        Some("Description") => match desc_code.as_str() {
                            "DES" | "SHO" if item.name.is_empty() => item.name = v,
                            "EXT" | "MKT" | "FAB" => {
                                if !item.description.is_empty() {
                                    item.description.push('\n');
                                }
                                item.description.push_str(&v);
                            }
                            _ => {}
                        },
                        Some("ProductAttribute") => item.attributes.push((attr_id.clone(), v)),
                        Some("Price") => item.prices.push((price_type.clone(), v)),
                        _ => {}
                    }
                }
                stack.pop();
                if e.name().as_ref() == "Item"
                    && let Some(i) = cur.take()
                    && !i.part_number.is_empty()
                {
                    items.push(i);
                }
            }
            Event::Eof => break,
            _ => {}
        }
    }
    Ok(items)
}

#[derive(Serialize, TS, Clone, Debug, Default)]
#[ts(export)]
pub struct PiesSummary {
    pub created: i64,
    pub updated: i64,
}

/// Create or update catalog parts from PIES items (matched by part number and brand).
pub fn import_pies(conn: &Connection, items: &[PiesItem], category_id: Option<&str>) -> AppResult<PiesSummary> {
    let mut s = PiesSummary::default();
    for it in items {
        let mut ids = vec![Identifier { kind: "aftermarket".into(), value: it.part_number.clone(), brand: it.brand.clone() }];
        if let Some(g) = &it.gtin {
            ids.push(Identifier { kind: "barcode".into(), value: g.clone(), brand: String::new() });
        }
        for x in &it.interchange {
            ids.push(Identifier { kind: "interchange".into(), value: x.clone(), brand: String::new() });
        }
        let mut specs = it.attributes.clone();
        specs.extend(it.prices.iter().map(|(t, p)| (format!("Price ({t})"), p.clone())));
        let existing: Option<String> = conn
            .query_row(
                "SELECT p.id FROM parts p JOIN part_identifiers i ON i.part_id = p.id WHERE i.value_norm = ?1 AND i.kind = 'aftermarket' AND p.brand = ?2 COLLATE NOCASE LIMIT 1",
                params![parts::norm(&it.part_number), it.brand],
                |r| r.get(0),
            )
            .optional()?;
        let input = PartInput {
            name: if it.name.is_empty() { format!("Part {}", it.part_number) } else { it.name.clone() },
            category_id: category_id.map(Into::into),
            brand: it.brand.clone(),
            manufacturer: String::new(),
            description: it.description.clone(),
            specs,
            install_notes: String::new(),
            warranty: String::new(),
            identifiers: ids,
        };
        match existing {
            Some(id) => {
                let old = parts::get(conn, &id)?;
                parts::update(conn, &id, &PartInput { category_id: old.category_id.or(input.category_id.clone()), ..input })?;
                s.updated += 1;
            }
            None => {
                parts::create(conn, &input, "user")?;
                s.created += 1;
            }
        }
    }
    db::audit(conn, "import", None, "pies", &format!("PIES: {} created, {} updated", s.created, s.updated), None)?;
    Ok(s)
}

pub fn import_pies_file(conn: &Connection, path: &Path, category_id: Option<&str>) -> AppResult<PiesSummary> {
    let items = parse_pies(&read_xml(path)?)?;
    if items.is_empty() {
        return Err(AppError::msg("No PIES <Item> elements with a part number were found in that file."));
    }
    import_pies(conn, &items, category_id)
}

#[cfg(test)]
mod tests {
    use super::*;

    // Hand-written samples following the ACES 4.2 / PIES 7.2 element structure (ids are made up).
    const ACES: &str = r#"<?xml version="1.0"?>
<ACES version="4.2"><Header><Company>Example</Company></Header>
<App action="A" id="1"><BaseVehicle id="100"/><EngineBase id="555"/><Note>w/o Sport pkg</Note><Qty>1</Qty><PartType id="1684"/><Position id="22"/><Part>BP-1</Part></App>
<App action="A" id="2"><BaseVehicle id="999"/><Qty>1</Qty><PartType id="1684"/><Part>BP-1</Part></App>
<App action="A" id="3"><BaseVehicle id="100"/><Qty>1</Qty><PartType id="1684"/><Part>NOPE</Part></App>
<App action="D" id="4"><BaseVehicle id="100"/><Part>BP-1</Part></App>
</ACES>"#;

    const PIES: &str = r#"<PIES><Items>
<Item MaintenanceType="A"><PartNumber>BP-1</PartNumber><BrandLabel>Acme</BrandLabel><ItemLevelGTIN GTINQualifier="UP">012345678905</ItemLevelGTIN>
<Descriptions><Description DescriptionCode="DES" LanguageCode="EN">Brake Pad Set</Description><Description DescriptionCode="EXT">Ceramic, low dust</Description></Descriptions>
<ProductAttributes><ProductAttribute AttributeID="Thickness">17 mm</ProductAttribute></ProductAttributes>
<Prices><Pricing PriceType="LST"><Price UOM="PE">49.99</Price></Pricing></Prices>
<PartInterchangeInfo><PartInterchange><PartNumber>OEM-777</PartNumber></PartInterchange></PartInterchangeInfo>
</Item></Items></PIES>"#;

    #[test]
    fn pies_then_aces() {
        let conn = db::open_memory().unwrap();
        let items = parse_pies(PIES).unwrap();
        assert_eq!(items[0].name, "Brake Pad Set");
        assert_eq!(items[0].interchange, vec!["OEM-777"]);
        let s = import_pies(&conn, &items, Some("brakes.pads")).unwrap();
        assert_eq!(s.created, 1);
        assert_eq!(import_pies(&conn, &items, None).unwrap().updated, 1, "re-import updates, doesn't duplicate");
        let apps = parse_aces(ACES).unwrap();
        assert_eq!(apps.len(), 4);
        assert_eq!(apps[0].attributes, vec![("EngineBase".to_string(), "555".to_string())]);
        let vm: VehicleMap = [("100".to_string(), (2019, "Nissan".to_string(), "Sentra".to_string()))].into_iter().collect();
        let am: AttributeMap = [(("EngineBase".to_string(), "555".to_string()), ("engine_displacement_l".to_string(), "1.8".to_string()))]
            .into_iter()
            .collect();
        let sum = import_aces(&conn, &apps, &vm, &am, "test.xml", true).unwrap();
        assert_eq!(sum.imported, 1);
        assert_eq!(sum.unresolved_vehicles, vec!["2"]);
        assert_eq!(sum.unknown_parts, vec!["NOPE"]);
        assert_eq!(sum.deletes_skipped, 1);
        let pid = parts::by_identifier(&conn, "BP-1").unwrap()[0].clone();
        let f = parts::fitments_for_part(&conn, &pid).unwrap();
        assert_eq!(f[0].qualifiers[0].key, "engine_displacement_l");
        assert_eq!(f[0].verified_by.as_deref(), Some("source"));
    }

    #[test]
    fn refuses_entities() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("x.xml");
        std::fs::write(&p, r#"<!DOCTYPE x [<!ENTITY e SYSTEM "file:///etc/passwd">]><ACES>&e;</ACES>"#).unwrap();
        assert!(read_xml(&p).is_err());
    }
}
