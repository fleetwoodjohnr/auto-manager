//! QuickBooks export files: QuickBooks Online "Products and Services", customer and vendor lists
//! (Export to Excel), and QuickBooks Desktop's Item, Customer and Vendor lists (Excel or CSV).
//! The kind is recognized from the column headers. Report-style exports have a few title lines
//! above the headers and a date line at the end; both are passed over.
//!
//! Intuit doesn't publish the exact export headers, so these are matched loosely (case,
//! spacing and punctuation ignored) and checked against the layouts named in the tests.

use super::{QbCustomer, QbData, QbItem, QbRow, QbVendor};
use crate::csvio::{self, Table};
use crate::db::clean;
use crate::error::{AppError, AppResult};
use crate::money;
use crate::qty::Qty;
use serde::Serialize;
use std::path::Path;
use ts_rs::TS;

#[derive(Serialize, TS, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum ExportKind {
    Items,
    Customers,
    Vendors,
}

fn key(h: &str) -> String {
    h.chars().filter(|c| c.is_alphanumeric()).collect::<String>().to_lowercase()
}

struct Cols(Vec<String>);

impl Cols {
    fn find(&self, keys: &[&str]) -> Option<usize> {
        keys.iter().find_map(|k| self.0.iter().position(|h| h == k))
    }
    fn all(&self, prefix: &str) -> Vec<usize> {
        self.0.iter().enumerate().filter(|(_, h)| h.starts_with(prefix)).map(|(i, _)| i).collect()
    }
}

const ITEM_NAME: &[&str] = &["productservicename", "productservice", "item", "itemname", "name"];
const ITEM_HINTS: &[&str] =
    &["salespricerate", "salesprice", "price", "rate", "purchasecost", "cost", "quantityonhand", "qtyonhand", "onhand", "sku", "type"];
const CUSTOMER_NAME: &[&str] = &["customer", "customername", "customerdisplayname", "displayname"];
const VENDOR_NAME: &[&str] = &["vendor", "vendorname", "vendordisplayname", "supplier"];

fn kind_of(cols: &Cols) -> Option<ExportKind> {
    if cols.find(CUSTOMER_NAME).is_some() && cols.find(&["productservicename", "productservice", "item"]).is_none() {
        Some(ExportKind::Customers)
    } else if cols.find(VENDOR_NAME).is_some() && cols.find(&["productservicename", "productservice", "item"]).is_none() {
        Some(ExportKind::Vendors)
    } else if cols.find(ITEM_NAME).is_some() && cols.find(ITEM_HINTS).is_some() {
        Some(ExportKind::Items)
    } else {
        None
    }
}

/// The header row: the first of the opening lines that names a recognized layout.
fn header_row(table: &Table) -> Option<(usize, Cols, ExportKind)> {
    std::iter::once(&table.headers).chain(table.rows.iter()).take(12).enumerate().find_map(|(i, row)| {
        let cols = Cols(row.iter().map(|h| key(h)).collect());
        kind_of(&cols).map(|k| (i, cols, k))
    })
}

/// Report footers and section totals ("TOTAL", "Accrual Basis Monday, November 14, 2026…").
fn report_line(name: &str) -> bool {
    let n = name.trim().to_lowercase();
    n == "total"
        || n.starts_with("total ")
        || n.starts_with("accrual basis")
        || n.starts_with("cash basis")
        || ["monday,", "tuesday,", "wednesday,", "thursday,", "friday,", "saturday,", "sunday,"].iter().any(|d| n.starts_with(d))
}

fn cell(row: &[String], i: Option<usize>) -> String {
    i.and_then(|i| row.get(i)).map(|s| s.trim().to_string()).unwrap_or_default()
}

fn active(row: &[String], i: Option<usize>) -> bool {
    !matches!(key(&cell(row, i)).as_str(), "inactive" | "no" | "false" | "n")
}

fn money_cell(row: &[String], i: Option<usize>, what: &str, problems: &mut Vec<String>) -> Option<String> {
    let s = cell(row, i);
    if s.is_empty() {
        return None;
    }
    // Accounting exports write negatives as (12.50).
    let t = if s.starts_with('(') && s.ends_with(')') { format!("-{}", &s[1..s.len() - 1]) } else { s.clone() };
    match money::parse("x", what, &t) {
        Ok(d) => Some(d.to_string()),
        Err(_) => {
            problems.push(format!("{what} “{s}” isn't a number"));
            None
        }
    }
}

fn qty_cell(row: &[String], i: Option<usize>, what: &str, problems: &mut Vec<String>) -> Option<Qty> {
    let s = cell(row, i).replace(',', "");
    if s.is_empty() {
        return None;
    }
    let t = if s.starts_with('(') && s.ends_with(')') { format!("-{}", &s[1..s.len() - 1]) } else { s.clone() };
    match Qty::parse("qty", &t) {
        Ok(q) => Some(q),
        Err(_) => {
            problems.push(format!("{what} “{s}” isn't a quantity"));
            None
        }
    }
}

/// "Inventory" (Online), "Inventory Part"/"Inventory Assembly" (Desktop), …
fn item_kind(t: &str) -> &'static str {
    match key(t).as_str() {
        "inventory" | "inventorypart" | "inventoryassembly" => "inventory",
        "noninventory" | "noninventorypart" => "non_inventory",
        "service" => "service",
        // A file without a Type column is a plain stock list.
        "" => "inventory",
        _ => "other",
    }
}

fn split_name(full: &str) -> (String, String) {
    match full.rsplit_once(':') {
        Some((cat, name)) if !name.trim().is_empty() => (clean(cat), clean(name)),
        _ => (String::new(), clean(full)),
    }
}

fn address(row: &[String], cols: &Cols) -> String {
    if let Some(i) = cols.find(&["billingaddress", "billaddress", "address", "billto"]) {
        return cell(row, Some(i)).split(['\n', '\r']).map(str::trim).filter(|s| !s.is_empty()).collect::<Vec<_>>().join(", ");
    }
    // Desktop: "Bill to 1" … "Bill to 5"; Online CSV: Billing street, city, state, ZIP, country.
    let mut parts: Vec<String> = cols.all("billto").into_iter().map(|i| cell(row, Some(i))).collect();
    for k in ["billingstreet", "billingaddressline1", "billingcity", "billingstate", "billingzip", "billingpostalcode", "billingcountry"] {
        parts.push(cell(row, cols.find(&[k])));
    }
    parts.into_iter().filter(|s| !s.is_empty()).collect::<Vec<_>>().join(", ")
}

/// Read a QuickBooks export into records.
pub fn read(table: &Table) -> AppResult<(ExportKind, QbData)> {
    let (at, cols, kind) = header_row(table).ok_or_else(|| {
        AppError::msg("This doesn't look like a QuickBooks products and services, customer or vendor list. For other spreadsheets, use Inventory → Import.")
    })?;
    let rows = table.rows.iter().skip(at);
    let mut data = QbData::default();
    let problem = |entity: &str, name: &str, line: usize, why: Vec<String>| QbRow {
        entity: entity.into(),
        name: if name.is_empty() { format!("Line {line}") } else { name.into() },
        status: "skipped".into(),
        notes: why,
    };
    match kind {
        ExportKind::Items => {
            let name_col = cols.find(ITEM_NAME);
            let type_col = cols.find(&["type", "itemtype"]);
            for (n, row) in rows.enumerate() {
                let line = at + n + 2;
                let full = cell(row, name_col);
                if full.is_empty() || report_line(&full) {
                    continue;
                }
                let type_text = cell(row, type_col);
                if type_col.is_some() && type_text.is_empty() {
                    continue; // a heading or subtotal line inside a report
                }
                let mut why = Vec::new();
                let (category_from_name, name) = split_name(&full);
                let category = cell(row, cols.find(&["category"]));
                let item = QbItem {
                    external_id: full.to_lowercase(),
                    name,
                    category: if category.is_empty() { category_from_name } else { category },
                    sku: cell(row, cols.find(&["sku", "itemsku"])),
                    kind: item_kind(&type_text).into(),
                    description: cell(row, cols.find(&["salesdescription", "description", "salesdesc"])),
                    price: money_cell(row, cols.find(&["salespricerate", "salesprice", "price", "rate"]), "Price", &mut why),
                    cost: money_cell(row, cols.find(&["purchasecost", "cost"]), "Cost", &mut why),
                    qty: qty_cell(row, cols.find(&["quantityonhand", "qtyonhand", "onhand"]), "Quantity on hand", &mut why),
                    reorder_point: qty_cell(row, cols.find(&["reorderpoint", "reorderptmin", "reorderpt"]), "Reorder point", &mut why),
                    vendor: cell(row, cols.find(&["preferredvendor", "vendor"])),
                    active: active(row, cols.find(&["activestatus", "status", "active"])),
                };
                if why.is_empty() {
                    data.items.push(item);
                } else {
                    data.problems.push(problem("item", &item.name, line, why));
                }
            }
        }
        ExportKind::Customers => {
            let name_col = cols.find(CUSTOMER_NAME);
            for row in rows {
                let name = cell(row, name_col);
                if name.is_empty() || report_line(&name) {
                    continue;
                }
                data.customers.push(QbCustomer {
                    external_id: name.to_lowercase(),
                    name: clean(&name),
                    phone: cell(row, cols.find(&["phone", "phonenumbers", "mainphone", "phonenumber", "mobile"])),
                    email: cell(row, cols.find(&["email", "mainemail", "emailaddress"])),
                    address: address(row, &cols),
                    notes: cell(row, cols.find(&["notes", "note"])),
                    active: active(row, cols.find(&["activestatus", "status", "active"])),
                });
            }
        }
        ExportKind::Vendors => {
            let name_col = cols.find(VENDOR_NAME);
            for row in rows {
                let name = cell(row, name_col);
                if name.is_empty() || report_line(&name) {
                    continue;
                }
                data.vendors.push(QbVendor {
                    external_id: name.to_lowercase(),
                    name: clean(&name),
                    contact: cell(row, cols.find(&["fullname", "contact", "primarycontact", "contactname"])),
                    phone: cell(row, cols.find(&["phone", "phonenumbers", "mainphone", "phonenumber"])),
                    email: cell(row, cols.find(&["email", "mainemail", "emailaddress"])),
                    website: cell(row, cols.find(&["website", "web", "websiteurl"])),
                    account_no: cell(row, cols.find(&["account", "accountno", "accountnumber", "acctnum"])),
                    active: active(row, cols.find(&["activestatus", "status", "active"])),
                });
            }
        }
    }
    Ok((kind, data))
}

pub fn read_file(path: &Path) -> AppResult<(ExportKind, QbData)> {
    read(&csvio::read_table(path)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn table(lines: &[&[&str]]) -> Table {
        let mut rows: Vec<Vec<String>> = lines.iter().map(|l| l.iter().map(|s| s.to_string()).collect()).collect();
        let headers = rows.remove(0);
        Table { headers, rows }
    }

    #[test]
    fn online_products_and_services() {
        let t = table(&[
            &[
                "Product/Service Name",
                "Sales Description",
                "SKU",
                "Type",
                "Sales Price / Rate",
                "Income Account",
                "Purchase Description",
                "Purchase Cost",
                "Expense Account",
                "Quantity On Hand",
                "Reorder Point",
                "Inventory Asset Account",
                "Quantity as-of Date",
            ],
            &[
                "Brakes:Front pads",
                "Ceramic front brake pads",
                "BP-100",
                "Inventory",
                "89.99",
                "Sales",
                "",
                "41.50",
                "Cost of Goods Sold",
                "12",
                "4",
                "Inventory Asset",
                "11/14/2026",
            ],
            &[
                "Engine oil 5W-30",
                "",
                "",
                "Inventory",
                "8.50",
                "Sales",
                "",
                "4.10",
                "Cost of Goods Sold",
                "37.5",
                "",
                "Inventory Asset",
                "11/14/2026",
            ],
            &["Labor", "Shop labor", "", "Service", "120", "Services", "", "", "", "", "", "", ""],
            &["Shop towels", "", "", "Non-inventory", "", "Sales", "", "1,200.00", "", "", "", "", ""],
            &["Wipers", "", "W-1", "Inventory", "abc", "", "", "", "", "", "", "", ""],
        ]);
        let (kind, d) = read(&t).unwrap();
        assert_eq!(kind, ExportKind::Items);
        assert_eq!(d.items.len(), 4);
        let pads = &d.items[0];
        assert_eq!(
            (pads.name.as_str(), pads.category.as_str(), pads.sku.as_str(), pads.kind.as_str()),
            ("Front pads", "Brakes", "BP-100", "inventory")
        );
        assert_eq!(
            (pads.price.as_deref(), pads.cost.as_deref(), pads.qty, pads.reorder_point),
            (Some("89.99"), Some("41.50"), Some(Qty::units(12)), Some(Qty::units(4)))
        );
        assert_eq!(d.items[1].qty, Some(Qty::from_milli(37_500)));
        assert_eq!(d.items[2].kind, "service");
        assert_eq!((d.items[3].kind.as_str(), d.items[3].cost.as_deref()), ("non_inventory", Some("1200.00")));
        assert_eq!(d.problems.len(), 1, "a price that isn't a number is reported, not guessed");
        assert!(d.problems[0].notes[0].contains("abc"));
    }

    #[test]
    fn desktop_item_list_report_with_title_lines_and_footer() {
        let t = table(&[
            &["Shop Name Inc.", "", "", "", "", "", ""],
            &["Item Listing", "", "", "", "", "", ""],
            &["", "", "", "", "", "", ""],
            &["Item", "Description", "Type", "Price", "Quantity On Hand", "Reorder Pt (Min)", "Preferred Vendor"],
            &["Filters", "", "", "", "", "", ""],
            &["Filters:Oil filter", "Spin-on oil filter", "Inventory Part", "12.95", "(2)", "5", "Parts Plus"],
            &["Shop supplies", "", "Other Charge", "5.00", "", "", ""],
            &["Accrual Basis Monday, November 14, 2026 03:12 PM GMT-05:00", "", "", "", "", "", ""],
        ]);
        let (kind, d) = read(&t).unwrap();
        assert_eq!(kind, ExportKind::Items);
        assert_eq!(d.items.len(), 2, "{:?}", d.items);
        let f = &d.items[0];
        assert_eq!(
            (f.name.as_str(), f.category.as_str(), f.kind.as_str(), f.vendor.as_str()),
            ("Oil filter", "Filters", "inventory", "Parts Plus")
        );
        assert_eq!(f.qty, Some(Qty::units(-2)), "QuickBooks allows negative stock; (2) is −2");
        assert_eq!(d.items[1].kind, "other");
    }

    #[test]
    fn customers_and_vendors() {
        let (kind, d) = read(&table(&[
            &["Customer", "Phone Numbers", "Email", "Full Name", "Billing Address"],
            &["Ada Lovelace", "555-0100", "ada@example.com", "Ada Lovelace", "1 Analytical Way\nLondon"],
            &["TOTAL", "", "", "", ""],
        ]))
        .unwrap();
        assert_eq!(kind, ExportKind::Customers);
        assert_eq!(d.customers.len(), 1);
        assert_eq!(d.customers[0].address, "1 Analytical Way, London");

        let (kind, d) = read(&table(&[
            &["Vendor", "Main Phone", "Main Email", "Full Name", "Account No.", "Bill to 1", "Bill to 2", "Active Status"],
            &["Parts Plus", "555-0199", "orders@partsplus.example", "Sam Diaz", "A-77", "9 Depot Rd", "Springfield", "Active"],
            &["Old Supplier", "", "", "", "", "", "", "Inactive"],
        ]))
        .unwrap();
        assert_eq!(kind, ExportKind::Vendors);
        assert_eq!((d.vendors[0].contact.as_str(), d.vendors[0].account_no.as_str()), ("Sam Diaz", "A-77"));
        assert!(!d.vendors[1].active);
    }

    #[test]
    fn other_spreadsheets_are_refused() {
        let e = read(&table(&[&["Make", "Model", "Year"], &["Ford", "F-150", "2019"]])).unwrap_err();
        assert!(e.to_string().contains("Inventory → Import"));
    }
}
