use super::*;
use crate::db;
use rust_decimal_macros::dec;

const SYS: &str = "qb-file";

fn pads() -> QbItem {
    QbItem {
        external_id: "brakes:front pads".into(),
        name: "Front pads".into(),
        category: "Brakes".into(),
        sku: "BP-100".into(),
        kind: "inventory".into(),
        description: "Ceramic front brake pads".into(),
        price: Some("89.99".into()),
        cost: Some("41.50".into()),
        qty: Some(Qty::units(12)),
        reorder_point: Some(Qty::units(4)),
        vendor: "Parts Plus".into(),
        active: true,
    }
}

fn data() -> QbData {
    QbData {
        items: vec![
            pads(),
            QbItem {
                external_id: "engine oil 5w-30".into(),
                name: "Engine oil 5W-30".into(),
                kind: "inventory".into(),
                price: Some("8.50".into()),
                cost: Some("4.10".into()),
                qty: Some(Qty::from_milli(37_500)),
                active: true,
                ..Default::default()
            },
            QbItem { external_id: "labor".into(), name: "Labor".into(), kind: "service".into(), active: true, ..Default::default() },
            QbItem {
                external_id: "towels".into(),
                name: "Shop towels".into(),
                kind: "non_inventory".into(),
                active: true,
                ..Default::default()
            },
            QbItem {
                external_id: "old".into(),
                name: "Old part".into(),
                kind: "inventory".into(),
                sku: "OLD".into(),
                active: false,
                ..Default::default()
            },
        ],
        customers: vec![QbCustomer {
            external_id: "ada".into(),
            name: "Ada Lovelace".into(),
            phone: "555-0100".into(),
            active: true,
            ..Default::default()
        }],
        vendors: vec![QbVendor {
            external_id: "parts plus".into(),
            name: "Parts Plus".into(),
            account_no: "A-77".into(),
            active: true,
            ..Default::default()
        }],
        problems: vec![],
    }
}

fn status<'a>(p: &'a QbPreview, name: &str) -> &'a QbRow {
    p.rows.iter().find(|r| r.name == name).unwrap_or_else(|| panic!("no row {name}: {:?}", p.rows))
}

#[test]
fn first_import_adds_stock_items_with_quickbooks_prices() {
    let conn = db::open_memory().unwrap();
    let o = QbOptions::default();
    let p = preview(&conn, SYS, &data(), &o).unwrap();
    assert_eq!((p.new_count, p.update_count, p.skipped_count), (4, 0, 3), "{:?}", p.rows);
    assert!(status(&p, "Labor").notes[0].contains("Service"));
    assert!(status(&p, "Shop towels").notes[0].contains("Non-inventory"));
    assert!(status(&p, "Old part").notes[0].contains("Inactive"));
    assert!(status(&p, "Engine oil 5W-30").notes.iter().any(|n| n.contains("No SKU")), "the name stands in for a missing SKU");
    assert_eq!(conn.query_row("SELECT count(*) FROM inventory_items", [], |r| r.get::<_, i64>(0)).unwrap(), 0, "a preview writes nothing");

    let s = apply(&conn, SYS, &data(), &o).unwrap();
    assert_eq!((s.added, s.updated, s.skipped), (4, 0, 3));
    let item = inventory::by_sku(&conn, "BP-100").unwrap().unwrap();
    assert_eq!(item.name, "Front pads");
    assert_eq!(item.price_override, Some(dec!(89.99)), "QuickBooks' sales price becomes the item's price");
    assert_eq!(item.cost, dec!(41.50));
    assert_eq!(item.on_hand, Qty::units(12));
    assert_eq!(item.min_stock, Qty::units(4));
    assert_eq!(item.supplier_name.as_deref(), Some("Parts Plus"));
    let oil = inventory::by_sku(&conn, "Engine oil 5W-30").unwrap().unwrap();
    assert!(oil.fractional, "37.5 on hand makes it an item sold in parts of a unit");
    assert_eq!(oil.on_hand, Qty::from_milli(37_500));
    let sup = suppliers::get(&conn, item.supplier_id.as_deref().unwrap()).unwrap();
    assert_eq!(sup.account_no, "A-77", "the vendor record was used, not a bare name");
    assert_eq!(conn.query_row("SELECT count(*) FROM suppliers", [], |r| r.get::<_, i64>(0)).unwrap(), 1);
    assert_eq!(conn.query_row("SELECT count(*) FROM external_links WHERE system = ?1", [SYS], |r| r.get::<_, i64>(0)).unwrap(), 4);
}

#[test]
fn importing_again_updates_without_copies_and_leaves_stock_alone() {
    let conn = db::open_memory().unwrap();
    let o = QbOptions::default();
    apply(&conn, SYS, &data(), &o).unwrap();
    // Auto Manager used two pads since; QuickBooks still says 12 and raised the price.
    let id = inventory::by_sku(&conn, "BP-100").unwrap().unwrap().id;
    inventory::adjust_to(&conn, &id, &inventory::default_location(&conn).unwrap(), Qty::units(10), "adjust", "used", None).unwrap();
    let mut d = data();
    d.items[0].price = Some("94.99".into());
    d.items[0].cost = Some("43.00".into());
    d.items[0].name = "Front brake pads".into();

    let p = preview(&conn, SYS, &d, &o).unwrap();
    let row = status(&p, "Front brake pads");
    assert_eq!(row.status, "update");
    assert!(row.notes.contains(&"Price: 89.99 → 94.99".to_string()), "{:?}", row.notes);
    assert!(row.notes.iter().any(|n| n == "QuickBooks has 12 on hand, here 10 (not changed)"), "{:?}", row.notes);
    assert!(row.notes.iter().any(|n| n.contains("cost 43.00") && n.ends_with("(not changed)")), "{:?}", row.notes);
    assert_eq!(status(&p, "Engine oil 5W-30").status, "unchanged");
    assert_eq!(status(&p, "Ada Lovelace").status, "unchanged");

    let s = apply(&conn, SYS, &d, &o).unwrap();
    assert_eq!((s.added, s.updated), (0, 1));
    let item = inventory::get(&conn, &id).unwrap();
    assert_eq!((item.name.as_str(), item.price_override), ("Front brake pads", Some(dec!(94.99))));
    assert_eq!(item.cost, dec!(41.50), "cost only changes when asked");
    assert_eq!(item.on_hand, Qty::units(10), "stock Auto Manager used isn't overwritten");
    assert_eq!(conn.query_row("SELECT count(*) FROM inventory_items", [], |r| r.get::<_, i64>(0)).unwrap(), 2);
    assert_eq!(conn.query_row("SELECT count(*) FROM customers", [], |r| r.get::<_, i64>(0)).unwrap(), 1);
}

#[test]
fn stock_and_cost_follow_quickbooks_when_asked_through_the_ledger() {
    let conn = db::open_memory().unwrap();
    apply(&conn, SYS, &data(), &QbOptions::default()).unwrap();
    let id = inventory::by_sku(&conn, "BP-100").unwrap().unwrap().id;
    let mut d = data();
    d.items[0].qty = Some(Qty::units(7));
    d.items[0].cost = Some("40.00".into());
    let o = QbOptions { apply_stock: true, apply_cost: true, ..Default::default() };
    let row = status(&preview(&conn, SYS, &d, &o).unwrap(), "Front pads").clone();
    assert!(row.notes.contains(&"Stock: 12 → 7".to_string()) && row.notes.contains(&"Cost: 41.50 → 40.00".to_string()), "{:?}", row.notes);
    apply(&conn, SYS, &d, &o).unwrap();
    let item = inventory::get(&conn, &id).unwrap();
    assert_eq!((item.on_hand, item.cost), (Qty::units(7), dec!(40.00)));
    let last = &inventory::movements(&conn, Some(&id), 1).unwrap()[0];
    assert_eq!((last.kind.as_str(), last.qty, last.reason.as_str()), ("count", Qty::units(-5), "Set to the QuickBooks quantity"));
}

#[test]
fn matches_existing_records_by_sku_and_name_instead_of_duplicating() {
    let conn = db::open_memory().unwrap();
    let part = parts::create(&conn, &parts::PartInput { name: "Pads".into(), ..Default::default() }, "user").unwrap();
    inventory::create(
        &conn,
        &part.id,
        &inventory::ItemInput {
            sku: "bp-100".into(),
            condition: "new".into(),
            unit: "each".into(),
            cost: "40".into(),
            price_override: None,
            pricing_profile_id: None,
            fractional: false,
            min_stock: Qty::ZERO,
            reorder_qty: Qty::ZERO,
            supplier_id: None,
            warranty: String::new(),
            notes: String::new(),
        },
    )
    .unwrap();
    customers::create(
        &conn,
        &customers::CustomerInput { name: "ada lovelace".into(), email: "ada@old.example".into(), ..Default::default() },
    )
    .unwrap();
    let p = preview(&conn, SYS, &data(), &QbOptions::default()).unwrap();
    assert_eq!(status(&p, "Front pads").status, "update");
    assert_eq!(status(&p, "Ada Lovelace").status, "update");
    apply(&conn, SYS, &data(), &QbOptions::default()).unwrap();
    assert_eq!(conn.query_row("SELECT count(*) FROM inventory_items", [], |r| r.get::<_, i64>(0)).unwrap(), 2);
    let ada = &customers::list(&conn, None).unwrap()[0];
    assert_eq!(
        (ada.name.as_str(), ada.phone.as_str(), ada.email.as_str()),
        ("Ada Lovelace", "555-0100", "ada@old.example"),
        "empty QuickBooks fields don't erase yours"
    );
}

#[test]
fn non_inventory_products_on_request_and_prices_can_stay_with_the_rules() {
    let conn = db::open_memory().unwrap();
    let o = QbOptions { non_inventory: true, use_prices: false, ..Default::default() };
    apply(&conn, SYS, &data(), &o).unwrap();
    let towels = inventory::by_sku(&conn, "Shop towels").unwrap().unwrap();
    assert_eq!(towels.on_hand, Qty::ZERO);
    assert_eq!(inventory::by_sku(&conn, "BP-100").unwrap().unwrap().price_override, None, "pricing rules decide");
}

#[test]
fn skus_that_cant_be_used_are_skipped_in_the_preview() {
    let conn = db::open_memory().unwrap();
    let mut d = data();
    d.items[1].sku = "BP-100".into();
    d.items.push(QbItem {
        external_id: "long".into(),
        name: "Long".into(),
        sku: "x".repeat(65),
        kind: "inventory".into(),
        active: true,
        ..Default::default()
    });
    let p = preview(&conn, SYS, &d, &QbOptions::default()).unwrap();
    assert!(status(&p, "Engine oil 5W-30").notes[0].contains("used by another QuickBooks item"), "{:?}", p.rows);
    assert!(status(&p, "Long").notes[0].contains("64"));
    let s = apply(&conn, SYS, &d, &QbOptions::default()).unwrap();
    assert_eq!(s.added, 3, "the rest still imports");
}
