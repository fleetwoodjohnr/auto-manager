//! Spreadsheet import and export. Imports go column mapping → validated preview → commit, so
//! nothing is written until the person has seen what will happen to every row.

use crate::db::{self, clean};
use crate::error::{AppError, AppResult};
use crate::qty::Qty;
use crate::{inventory, money, parts};
use rusqlite::{Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::Path;
use ts_rs::TS;

pub const MAX_ROWS: usize = 50_000;
const MAX_FILE: u64 = 50 * 1024 * 1024;

#[derive(Serialize, TS, Debug, Clone)]
#[ts(export)]
pub struct Table {
    pub headers: Vec<String>,
    pub rows: Vec<Vec<String>>,
}

fn check_size(path: &Path) -> AppResult<()> {
    if std::fs::metadata(path)?.len() > MAX_FILE {
        return Err(AppError::msg("That file is larger than 50 MB. Split it into smaller files."));
    }
    Ok(())
}

pub fn read_csv(path: &Path) -> AppResult<Table> {
    check_size(path)?;
    let mut rdr = csv::ReaderBuilder::new()
        .flexible(true)
        .trim(csv::Trim::All)
        .from_path(path)
        .map_err(|e| AppError::msg(format!("Couldn't open the CSV file: {e}")))?;
    let headers: Vec<String> = rdr
        .headers()
        .map_err(|e| AppError::msg(format!("Couldn't read the header row: {e}")))?
        .iter()
        .map(|h| h.trim_start_matches('\u{feff}').to_string())
        .collect();
    let mut rows = Vec::new();
    for (i, rec) in rdr.records().enumerate() {
        if i >= MAX_ROWS {
            return Err(AppError::msg(format!("This file has more than {MAX_ROWS} rows. Split it into smaller files.")));
        }
        let rec = rec.map_err(|e| AppError::msg(format!("Row {} can't be read: {e}", i + 2)))?;
        if rec.iter().all(|c| c.is_empty()) {
            continue;
        }
        rows.push(rec.iter().map(String::from).collect());
    }
    Ok(Table { headers, rows })
}

/// The first worksheet of an .xlsx/.xls/.ods file, as text.
pub fn read_sheet(path: &Path) -> AppResult<Table> {
    use calamine::{Data, Reader, open_workbook_auto};
    check_size(path)?;
    let mut wb = open_workbook_auto(path).map_err(|e| AppError::msg(format!("Couldn't open the spreadsheet: {e}")))?;
    let name = wb.sheet_names().first().cloned().ok_or_else(|| AppError::msg("The spreadsheet has no sheets."))?;
    let range = wb.worksheet_range(&name).map_err(|e| AppError::msg(format!("Couldn't read sheet “{name}”: {e}")))?;
    let text = |c: &Data| match c {
        Data::Empty => String::new(),
        Data::Float(f) if f.fract() == 0.0 && f.abs() < 1e15 => format!("{}", *f as i64),
        other => other.to_string().trim().to_string(),
    };
    let mut it = range.rows();
    let headers: Vec<String> = it.next().map(|r| r.iter().map(text).collect()).unwrap_or_default();
    let mut rows = Vec::new();
    for r in it {
        if rows.len() >= MAX_ROWS {
            return Err(AppError::msg(format!("This sheet has more than {MAX_ROWS} rows. Split it into smaller files.")));
        }
        let row: Vec<String> = r.iter().map(text).collect();
        if row.iter().any(|c| !c.is_empty()) {
            rows.push(row);
        }
    }
    Ok(Table { headers, rows })
}

pub fn read_table(path: &Path) -> AppResult<Table> {
    match path.extension().and_then(|e| e.to_str()).map(|e| e.to_lowercase()).as_deref() {
        Some("csv") | Some("txt") => read_csv(path),
        Some("xlsx") | Some("xls") | Some("ods") => read_sheet(path),
        _ => Err(AppError::msg("Choose a .csv or .xlsx file.")),
    }
}

/// Neutralize spreadsheet formula injection in exported cells.
pub fn sanitize(c: &str) -> String {
    if c.starts_with(['=', '+', '@', '\t', '\r']) || (c.starts_with('-') && c.parse::<f64>().is_err()) {
        format!("'{c}")
    } else {
        c.to_string()
    }
}

pub fn write_csv(path: &Path, headers: &[&str], rows: impl IntoIterator<Item = Vec<String>>) -> AppResult<usize> {
    let tmp = path.with_extension("csv.partial");
    let mut n = 0;
    {
        let mut w = csv::Writer::from_path(&tmp).map_err(|e| AppError::msg(format!("Couldn't create the file: {e}")))?;
        w.write_record(headers).map_err(|e| AppError::Other(e.to_string()))?;
        for r in rows {
            w.write_record(r.iter().map(|c| sanitize(c))).map_err(|e| AppError::Other(e.to_string()))?;
            n += 1;
        }
        w.flush()?;
    }
    std::fs::rename(tmp, path)?;
    Ok(n)
}

pub fn write_xlsx(path: &Path, sheet: &str, headers: &[&str], rows: impl IntoIterator<Item = Vec<String>>) -> AppResult<usize> {
    let mut wb = rust_xlsxwriter::Workbook::new();
    let ws = wb.add_worksheet();
    let err = |e: rust_xlsxwriter::XlsxError| AppError::Other(format!("Couldn't write the spreadsheet: {e}"));
    ws.set_name(sheet).map_err(err)?;
    let bold = rust_xlsxwriter::Format::new().set_bold();
    for (c, h) in headers.iter().enumerate() {
        ws.write_string_with_format(0, c as u16, *h, &bold).map_err(err)?;
    }
    let mut n = 0;
    for (r, row) in rows.into_iter().enumerate() {
        for (c, v) in row.iter().enumerate() {
            // Numbers stay numbers so totals work in the spreadsheet; text is written as text,
            // which can't be evaluated as a formula.
            match v.parse::<f64>() {
                Ok(f) if !v.is_empty() && !v.starts_with('0') || v == "0" || v.starts_with("0.") => {
                    ws.write_number(r as u32 + 1, c as u16, f).map_err(err)?
                }
                _ => ws.write_string(r as u32 + 1, c as u16, v).map_err(err)?,
            };
        }
        n += 1;
    }
    ws.autofit();
    let tmp = path.with_extension("xlsx.partial");
    wb.save(&tmp).map_err(err)?;
    std::fs::rename(tmp, path)?;
    Ok(n)
}

fn cell(row: &[String], i: Option<usize>) -> String {
    i.and_then(|i| row.get(i)).cloned().unwrap_or_default().trim().to_string()
}

fn opt_int(s: &str, what: &str) -> Result<Option<i64>, String> {
    let t = s.trim().replace(',', "");
    if t.is_empty() {
        return Ok(None);
    }
    t.parse::<i64>().ok().filter(|n| *n >= 0).map(Some).ok_or_else(|| format!("{what} “{s}” isn't a whole number"))
}

fn opt_qty(s: &str, what: &str) -> Result<Option<Qty>, String> {
    let t = s.trim();
    if t.is_empty() {
        return Ok(None);
    }
    Qty::parse("qty", t).ok().filter(|q| *q >= 0).map(Some).ok_or_else(|| format!("{what} “{s}” isn't a quantity (like 2 or 4.5)"))
}

fn opt_money(s: &str, what: &str) -> Result<Option<String>, String> {
    let t = s.trim();
    if t.is_empty() {
        return Ok(None);
    }
    money::parse("x", what, t).map(|d| Some(d.to_string())).map_err(|_| format!("{what} “{s}” isn't a number"))
}

// ---------------------------------------------------------------- inventory import

#[derive(Deserialize, Serialize, TS, Debug, Clone, Default)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct InventoryMapping {
    pub name: Option<usize>,
    pub sku: Option<usize>,
    pub brand: Option<usize>,
    pub category: Option<usize>,
    pub part_number: Option<usize>,
    pub oem_number: Option<usize>,
    pub barcode: Option<usize>,
    pub description: Option<usize>,
    pub cost: Option<usize>,
    pub price: Option<usize>,
    pub qty: Option<usize>,
    pub location: Option<usize>,
    pub bin: Option<usize>,
    pub min_stock: Option<usize>,
    pub reorder_qty: Option<usize>,
    #[serde(default)]
    pub unit: Option<usize>,
    pub supplier: Option<usize>,
    pub condition: Option<usize>,
}

/// Best-guess column mapping from the header names.
pub fn guess_inventory_mapping(headers: &[String]) -> InventoryMapping {
    let find = |pats: &[&str]| {
        headers.iter().position(|h| {
            let h = h.to_lowercase();
            pats.iter().any(|p| h == *p || h.contains(p))
        })
    };
    InventoryMapping {
        name: find(&["product name", "name", "description"]).filter(|_| true),
        sku: find(&["sku", "stock code", "item code"]),
        brand: find(&["brand", "manufacturer", "make"]),
        category: find(&["category", "type", "group"]),
        part_number: find(&["part number", "part no", "part #", "partnumber", "mpn", "aftermarket"]),
        oem_number: find(&["oem"]),
        barcode: find(&["barcode", "upc", "ean", "gtin"]),
        description: headers
            .iter()
            .position(|h| h.to_lowercase() == "description" || h.to_lowercase() == "details")
            .filter(|i| Some(*i) != find(&["product name", "name"])),
        cost: find(&["cost", "purchase price", "buy price"]),
        price: find(&["sell price", "selling price", "retail", "price"])
            .filter(|i| Some(*i) != find(&["cost", "purchase price", "buy price"])),
        qty: find(&["qty", "quantity", "on hand", "stock"]),
        location: find(&["location", "warehouse"]),
        bin: find(&["bin", "shelf"]),
        min_stock: find(&["min", "reorder point", "minimum"]),
        reorder_qty: find(&["reorder qty", "reorder quantity"]),
        unit: find(&["unit", "uom", "unit of measure"]),
        supplier: find(&["supplier", "vendor"]),
        condition: find(&["condition"]),
    }
}

#[derive(Serialize, TS, Debug, Clone)]
#[ts(export)]
pub struct InventoryRow {
    pub line: usize,
    /// "new" | "duplicate" | "error"
    pub status: String,
    pub message: Option<String>,
    pub name: String,
    pub sku: String,
    pub brand: String,
    pub category_id: Option<String>,
    pub category: String,
    pub part_number: String,
    pub oem_number: String,
    pub barcode: String,
    pub description: String,
    pub cost: Option<String>,
    pub price: Option<String>,
    pub qty: Option<Qty>,
    pub location_id: Option<String>,
    pub location: String,
    pub bin: String,
    pub min_stock: Option<Qty>,
    pub reorder_qty: Option<Qty>,
    /// "each" unless the sheet says otherwise.
    pub unit: String,
    /// The sheet has a quantity in parts of a unit for this item.
    pub fractional: bool,
    pub supplier: String,
    pub condition: String,
}

#[derive(Serialize, TS, Debug, Clone)]
#[ts(export)]
pub struct InventoryPreview {
    pub rows: Vec<InventoryRow>,
    pub new_count: usize,
    pub duplicate_count: usize,
    pub error_count: usize,
}

fn match_category(conn: &Connection, text: &str) -> AppResult<Option<(String, String)>> {
    let t = text.trim();
    if t.is_empty() {
        return Ok(None);
    }
    // "Braking > Brake pads", "Brake pads", or a category id.
    let last = t.rsplit(['>', '›', '/']).next().unwrap_or(t).trim();
    Ok(conn
        .query_row(
            "SELECT id, name FROM part_categories WHERE id = ?1 OR name = ?2 COLLATE NOCASE ORDER BY parent_id IS NULL LIMIT 1",
            [t, last],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?)
}

pub fn preview_inventory(conn: &Connection, table: &Table, m: &InventoryMapping) -> AppResult<InventoryPreview> {
    if m.name.is_none() || m.sku.is_none() {
        return Err(AppError::msg("Map at least the product name and SKU columns."));
    }
    let locations: HashMap<String, String> =
        inventory::locations(conn)?.into_iter().filter(|l| l.kind == "warehouse").map(|l| (l.name.to_lowercase(), l.id)).collect();
    let default_loc = inventory::default_location(conn)?;
    let mut seen: HashSet<String> = HashSet::new();
    let mut rows = Vec::new();
    for (i, r) in table.rows.iter().enumerate() {
        let mut errors: Vec<String> = Vec::new();
        let name = clean(&cell(r, m.name));
        let sku = clean(&cell(r, m.sku));
        if name.is_empty() {
            errors.push("no product name".into());
        }
        if sku.is_empty() {
            errors.push("no SKU".into());
        }
        let category_text = cell(r, m.category);
        let category = match_category(conn, &category_text)?;
        if !category_text.is_empty() && category.is_none() {
            errors.push(format!("category “{category_text}” isn't in the catalog"));
        }
        let mut num = |s: Result<Option<Qty>, String>| {
            s.unwrap_or_else(|e| {
                errors.push(e);
                None
            })
        };
        let qty = num(opt_qty(&cell(r, m.qty), "Quantity"));
        let min_stock = num(opt_qty(&cell(r, m.min_stock), "Minimum stock"));
        let reorder_qty = num(opt_qty(&cell(r, m.reorder_qty), "Reorder quantity"));
        let fractional = [qty, min_stock, reorder_qty].iter().flatten().any(|q| !q.is_whole());
        let unit = Some(clean(&cell(r, m.unit))).filter(|u| !u.is_empty()).unwrap_or_else(|| "each".into());
        let mut cash = |s: Result<Option<String>, String>| {
            s.unwrap_or_else(|e| {
                errors.push(e);
                None
            })
        };
        let cost = cash(opt_money(&cell(r, m.cost), "Cost"));
        let price = cash(opt_money(&cell(r, m.price), "Price"));
        let loc_text = cell(r, m.location);
        let location_id = if loc_text.is_empty() {
            Some(default_loc.clone())
        } else {
            let found = locations.get(&loc_text.to_lowercase()).cloned();
            if found.is_none() {
                errors.push(format!("location “{loc_text}” doesn't exist (add it in Settings first)"));
            }
            found
        };
        let condition = match cell(r, m.condition).to_lowercase().as_str() {
            "" | "new" => "new".to_string(),
            "used" => "used".to_string(),
            "refurbished" | "reman" | "remanufactured" => "refurbished".to_string(),
            other => {
                errors.push(format!("condition “{other}” should be new, used or refurbished"));
                "new".into()
            }
        };
        let dup_file = !sku.is_empty() && !seen.insert(sku.to_lowercase());
        let dup_db = !sku.is_empty() && inventory::by_sku(conn, &sku)?.is_some();
        let (status, message) = if !errors.is_empty() {
            ("error", Some(errors.join("; ")))
        } else if dup_db {
            ("duplicate", Some("SKU already in inventory: skipped".to_string()))
        } else if dup_file {
            ("duplicate", Some("SKU repeated earlier in this file: skipped".to_string()))
        } else {
            ("new", None)
        };
        rows.push(InventoryRow {
            line: i + 2,
            status: status.into(),
            message,
            name,
            sku,
            brand: clean(&cell(r, m.brand)),
            category_id: category.as_ref().map(|c| c.0.clone()),
            category: category.map(|c| c.1).unwrap_or_default(),
            part_number: cell(r, m.part_number),
            oem_number: cell(r, m.oem_number),
            barcode: cell(r, m.barcode),
            description: cell(r, m.description),
            cost,
            price,
            qty,
            location_id,
            location: loc_text,
            bin: cell(r, m.bin),
            min_stock,
            reorder_qty,
            unit,
            fractional,
            supplier: clean(&cell(r, m.supplier)),
            condition,
        });
    }
    let count = |s: &str| rows.iter().filter(|r| r.status == s).count();
    Ok(InventoryPreview { new_count: count("new"), duplicate_count: count("duplicate"), error_count: count("error"), rows })
}

#[derive(Serialize, TS, Debug, Clone)]
#[ts(export)]
pub struct ImportSummary {
    pub created: usize,
    pub skipped: usize,
}

/// Create a part, item and opening stock for every "new" row. Run inside one transaction.
pub fn commit_inventory(conn: &Connection, preview: &InventoryPreview) -> AppResult<ImportSummary> {
    let mut suppliers: HashMap<String, String> = HashMap::new();
    let mut created = 0;
    for r in preview.rows.iter().filter(|r| r.status == "new") {
        let mut ids = Vec::new();
        for (kind, v) in [("aftermarket", &r.part_number), ("oem", &r.oem_number), ("barcode", &r.barcode)] {
            if !parts::norm(v).is_empty() {
                ids.push(parts::Identifier {
                    kind: kind.into(),
                    value: v.clone(),
                    brand: if kind == "aftermarket" { r.brand.clone() } else { String::new() },
                });
            }
        }
        // Reuse a catalog part that already carries this part number.
        let existing = if r.part_number.is_empty() { vec![] } else { parts::by_identifier(conn, &r.part_number)? };
        let part_id = match existing.as_slice() {
            [one] => one.clone(),
            _ => {
                parts::create(
                    conn,
                    &parts::PartInput {
                        name: r.name.clone(),
                        category_id: r.category_id.clone(),
                        brand: r.brand.clone(),
                        description: r.description.clone(),
                        identifiers: ids,
                        ..Default::default()
                    },
                    "user",
                )?
                .id
            }
        };
        let supplier_id = if r.supplier.is_empty() {
            None
        } else if let Some(id) = suppliers.get(&r.supplier.to_lowercase()) {
            Some(id.clone())
        } else {
            let id: Option<String> =
                conn.query_row("SELECT id FROM suppliers WHERE name = ?1 COLLATE NOCASE", [&r.supplier], |x| x.get(0)).optional()?;
            let id = match id {
                Some(id) => id,
                None => {
                    crate::suppliers::create(conn, &crate::suppliers::SupplierInput { name: r.supplier.clone(), ..Default::default() })?.id
                }
            };
            suppliers.insert(r.supplier.to_lowercase(), id.clone());
            Some(id)
        };
        let item = inventory::create(
            conn,
            &part_id,
            &inventory::ItemInput {
                sku: r.sku.clone(),
                condition: r.condition.clone(),
                unit: r.unit.clone(),
                fractional: r.fractional,
                cost: r.cost.clone().unwrap_or_else(|| "0".into()),
                price_override: r.price.clone(),
                pricing_profile_id: None,
                min_stock: r.min_stock.unwrap_or_default(),
                reorder_qty: r.reorder_qty.unwrap_or_default(),
                supplier_id,
                warranty: String::new(),
                notes: String::new(),
            },
        )?;
        if let (Some(q), Some(loc)) = (r.qty.filter(|q| *q > 0), &r.location_id) {
            inventory::receive(
                conn,
                &inventory::Receive {
                    item_id: item.id,
                    location_id: loc.clone(),
                    qty: q,
                    unit_cost: r.cost.clone(),
                    bin: Some(r.bin.clone()),
                    reason: "Opening stock (import)".into(),
                    ref_kind: Some("import".into()),
                    ref_id: None,
                },
            )?;
        }
        created += 1;
    }
    db::audit(conn, "import", None, "inventory", &format!("Imported {created} inventory items"), None)?;
    Ok(ImportSummary { created, skipped: preview.rows.len() - created })
}

// ---------------------------------------------------------------- fitment import

/// Columns of the Auto Manager fitment CSV. Any other column becomes a qualifier named after
/// its header (e.g. a "brake_package" column).
pub const FITMENT_COLUMNS: &[&str] = &["part", "kind", "year_from", "year_to", "make", "model", "position", "quantity", "notes"];

#[derive(Serialize, TS, Debug, Clone)]
#[ts(export)]
pub struct FitmentRow {
    pub line: usize,
    pub status: String,
    pub message: Option<String>,
    pub part_id: Option<String>,
    pub part: String,
    pub kind: String,
    pub year_from: i32,
    pub year_to: i32,
    pub make: String,
    pub model: String,
    pub position: String,
    pub quantity: Option<i64>,
    pub notes: String,
    pub qualifiers: Vec<parts::Qualifier>,
}

#[derive(Serialize, TS, Debug, Clone)]
#[ts(export)]
pub struct FitmentPreview {
    pub rows: Vec<FitmentRow>,
    pub ok_count: usize,
    pub error_count: usize,
    pub qualifier_keys: Vec<String>,
}

fn header_key(h: &str) -> String {
    h.trim().to_lowercase().replace([' ', '-'], "_")
}

pub fn preview_fitment(conn: &Connection, table: &Table) -> AppResult<FitmentPreview> {
    let keys: Vec<String> = table.headers.iter().map(|h| header_key(h)).collect();
    let col = |name: &str| keys.iter().position(|k| k == name);
    for need in ["part", "year_from", "make", "model"] {
        if col(need).is_none() {
            return Err(AppError::msg(format!("The file needs a “{need}” column. Expected columns: {}.", FITMENT_COLUMNS.join(", "))));
        }
    }
    let qualifier_cols: Vec<(usize, String)> = keys
        .iter()
        .enumerate()
        .filter(|(_, k)| !FITMENT_COLUMNS.contains(&k.as_str()) && crate::specs::valid_key(k))
        .map(|(i, k)| (i, k.clone()))
        .collect();
    let mut rows = Vec::new();
    for (i, r) in table.rows.iter().enumerate() {
        let mut errors = Vec::new();
        let part = cell(r, col("part"));
        let part_id = if let Some(item) = inventory::by_sku(conn, &part)? {
            Some(item.part_id)
        } else {
            match parts::by_identifier(conn, &part)?.as_slice() {
                [one] => Some(one.clone()),
                [] => {
                    errors.push(format!("no part or SKU “{part}” in the catalog"));
                    None
                }
                _ => {
                    errors.push(format!("“{part}” matches several parts; use the SKU"));
                    None
                }
            }
        };
        let kind = match cell(r, col("kind")).to_lowercase().as_str() {
            "" | "fits" | "fit" | "yes" => "fits".to_string(),
            "excludes" | "exclude" | "no" | "does not fit" => "excludes".to_string(),
            other => {
                errors.push(format!("kind “{other}” should be fits or excludes"));
                "fits".into()
            }
        };
        let year = |c: &str| cell(r, col(c)).parse::<i32>().ok().filter(|y| (1886..=2100).contains(y));
        let year_from = year("year_from");
        let year_to = if cell(r, col("year_to")).is_empty() { year_from } else { year("year_to") };
        if year_from.is_none() || year_to.is_none() {
            errors.push("years must be four-digit model years".into());
        }
        let (yf, yt) = (year_from.unwrap_or(0), year_to.unwrap_or(0));
        if yf > yt {
            errors.push("year_to is before year_from".into());
        }
        let make = clean(&cell(r, col("make")));
        let model = clean(&cell(r, col("model")));
        if make.is_empty() || model.is_empty() {
            errors.push("make and model are required".into());
        }
        let quantity = opt_int(&cell(r, col("quantity")), "Quantity").unwrap_or_else(|e| {
            errors.push(e);
            None
        });
        let qualifiers = qualifier_cols
            .iter()
            .filter_map(|(c, k)| Some(cell(r, Some(*c))).filter(|v| !v.is_empty()).map(|v| parts::Qualifier { key: k.clone(), value: v }))
            .collect();
        rows.push(FitmentRow {
            line: i + 2,
            status: if errors.is_empty() { "ok".into() } else { "error".into() },
            message: (!errors.is_empty()).then(|| errors.join("; ")),
            part_id,
            part,
            kind,
            year_from: yf,
            year_to: yt,
            make,
            model,
            position: cell(r, col("position")),
            quantity,
            notes: cell(r, col("notes")),
            qualifiers,
        });
    }
    let ok_count = rows.iter().filter(|r| r.status == "ok").count();
    Ok(FitmentPreview {
        error_count: rows.len() - ok_count,
        ok_count,
        rows,
        qualifier_keys: qualifier_cols.into_iter().map(|(_, k)| k).collect(),
    })
}

/// Commit the valid rows under a new import source. `authoritative` marks the records as
/// verified by their source (only for licensed catalogs the person trusts).
pub fn commit_fitment(conn: &Connection, preview: &FitmentPreview, source_name: &str, authoritative: bool) -> AppResult<ImportSummary> {
    let source_id = format!("import-{}", db::new_id());
    conn.execute(
        "INSERT INTO data_sources (id, name, kind, priority) VALUES (?1, ?2, 'import', 50)",
        rusqlite::params![source_id, format!("Import: {}", clean(source_name))],
    )?;
    let mut created = 0;
    for r in preview.rows.iter().filter(|r| r.status == "ok") {
        let Some(part_id) = &r.part_id else { continue };
        parts::add_fitment(
            conn,
            part_id,
            &parts::FitmentInput {
                kind: r.kind.clone(),
                year_from: r.year_from,
                year_to: r.year_to,
                make: r.make.clone(),
                model: r.model.clone(),
                position: r.position.clone(),
                quantity: r.quantity,
                notes: r.notes.clone(),
                qualifiers: r.qualifiers.clone(),
                verified: false,
                source_ref: Some(format!("{source_name} line {}", r.line)),
            },
            &source_id,
            authoritative,
        )?;
        created += 1;
    }
    db::audit(conn, "import", Some(&source_id), "fitment", &format!("Imported {created} fitment records"), None)?;
    Ok(ImportSummary { created, skipped: preview.rows.len() - created })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn table(csv: &str) -> Table {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("t.csv");
        std::fs::write(&p, csv).unwrap();
        read_csv(&p).unwrap()
    }

    #[test]
    fn inventory_import_preview_and_commit() {
        let conn = db::open_memory().unwrap();
        let t = table(
            "\u{feff}Name,SKU,Brand,Category,Part Number,Cost,Price,Qty,Bin,Supplier\nOil filter,OF-1,Acme,Oil filters,AC-1,4.10,9.99,8,A-1,Parts Co\nAir filter,AF-1,Acme,Nope,,3,,2,,\nOil filter dup,of-1,Acme,,,1,,1,,\nBad qty,BQ-1,,,,x,,two,,\n",
        );
        let m = guess_inventory_mapping(&t.headers);
        assert_eq!(m.sku, Some(1));
        assert_eq!(m.cost, Some(5));
        assert_eq!(m.price, Some(6));
        let p = preview_inventory(&conn, &t, &m).unwrap();
        assert_eq!((p.new_count, p.duplicate_count, p.error_count), (1, 1, 2), "{:#?}", p.rows);
        let s = commit_inventory(&conn, &p).unwrap();
        assert_eq!(s.created, 1);
        let item = inventory::by_sku(&conn, "OF-1").unwrap().unwrap();
        assert_eq!((item.on_hand, item.bins.as_str(), item.supplier_name.as_deref()), (Qty::units(8), "Main: A-1", Some("Parts Co")));
        assert_eq!(item.category, "Oil filters");
        let again = preview_inventory(&conn, &t, &m).unwrap();
        assert_eq!(again.rows[0].status, "duplicate", "re-importing doesn't duplicate");
    }

    #[test]
    fn fitment_import_with_qualifiers() {
        let conn = db::open_memory().unwrap();
        let p = parts::create(
            &conn,
            &parts::PartInput {
                name: "Pads".into(),
                identifiers: vec![parts::Identifier { kind: "aftermarket".into(), value: "BP-9".into(), brand: String::new() }],
                ..Default::default()
            },
            "user",
        )
        .unwrap();
        let t = table(
            "part,kind,year_from,year_to,make,model,engine_displacement_l,brake_package\nBP-9,fits,2016,2019,Nissan,Sentra,1.8,Standard\nnope,fits,2016,2019,Nissan,Sentra,,\nBP-9,fits,2019,2016,Nissan,Sentra,,\n",
        );
        let pv = preview_fitment(&conn, &t).unwrap();
        assert_eq!((pv.ok_count, pv.error_count), (1, 2));
        assert_eq!(pv.qualifier_keys, vec!["engine_displacement_l", "brake_package"]);
        commit_fitment(&conn, &pv, "catalog.csv", true).unwrap();
        let f = parts::fitments_for_part(&conn, &p.id).unwrap();
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].verified_by.as_deref(), Some("source"));
        assert_eq!(f[0].qualifiers.len(), 2);
    }

    #[test]
    fn exports_neutralize_formulas() {
        assert_eq!(sanitize("=HYPERLINK(\"x\")"), "'=HYPERLINK(\"x\")");
        assert_eq!(sanitize("-12.5"), "-12.5");
        assert_eq!(sanitize("-cmd"), "'-cmd");
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("o.xlsx");
        write_xlsx(&p, "Items", &["A", "B"], vec![vec!["=1+1".into(), "12.5".into()], vec!["007".into(), "0".into()]]).unwrap();
        let t = read_sheet(&p).unwrap();
        assert_eq!(t.rows[0], vec!["=1+1", "12.5"], "formula text stays text");
        assert_eq!(t.rows[1][0], "007", "leading zeros (part numbers) are kept as text");
    }
}
