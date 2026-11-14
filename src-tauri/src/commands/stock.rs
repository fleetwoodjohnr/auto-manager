use super::{check_id, valuation};
use auto_manager_core::csvio::{self, ImportSummary, InventoryMapping, InventoryPreview, Table};
use auto_manager_core::inventory::{
    self as inv, Adjust, Change, CountLine, CountResult, Item, ItemInput, ItemQuery, Location, Movement, Receive, Reservation, Reserve,
    StockLevel, Transfer,
};
use auto_manager_core::parts::{self, PartInput};
use auto_manager_core::pricing::{self, PriceResult};
use auto_manager_core::{AppError, AppResult, Core};
use serde::Serialize;
use std::path::PathBuf;
use tauri::State;

#[tauri::command(async)]
pub fn item_list(core: State<'_, Core>, query: ItemQuery) -> AppResult<Vec<Item>> {
    core.read(|c| inv::list(c, &query))
}

#[derive(Serialize)]
pub struct ItemDetail {
    pub item: Item,
    pub levels: Vec<StockLevel>,
    pub reservations: Vec<Reservation>,
    pub movements: Vec<Movement>,
    pub price: PriceResult,
}

#[tauri::command(async)]
pub fn item_get(core: State<'_, Core>, id: String) -> AppResult<ItemDetail> {
    check_id(&id)?;
    let val = valuation(&core);
    core.read(|c| {
        Ok(ItemDetail {
            item: inv::get(c, &id)?,
            levels: inv::levels(c, &id)?,
            reservations: inv::reservations(c, Some(&id), true)?,
            movements: inv::movements(c, Some(&id), 200)?,
            price: pricing::price_item(c, &id, 1, &[], &val)?,
        })
    })
}

/// Create an item for an existing catalog part, or for a new part described in `new_part`.
#[tauri::command(async)]
pub fn item_create(core: State<'_, Core>, part_id: Option<String>, new_part: Option<PartInput>, input: ItemInput) -> AppResult<Item> {
    core.tx(|c| {
        let pid = match (part_id, new_part) {
            (Some(p), _) => p,
            (None, Some(np)) => parts::create(c, &np, "user")?.id,
            (None, None) => return Err(AppError::msg("Choose a catalog part or describe a new one.")),
        };
        inv::create(c, &pid, &input)
    })
}

#[tauri::command(async)]
pub fn item_update(core: State<'_, Core>, id: String, version: i64, input: ItemInput) -> AppResult<Item> {
    check_id(&id)?;
    core.tx(|c| inv::update(c, &id, version, &input))
}

#[tauri::command(async)]
pub fn item_archive(core: State<'_, Core>, id: String, archived: bool) -> AppResult<Item> {
    check_id(&id)?;
    core.tx(|c| inv::set_archived(c, &id, archived))
}

#[tauri::command(async)]
pub fn item_delete(core: State<'_, Core>, id: String) -> AppResult<()> {
    check_id(&id)?;
    core.tx(|c| inv::delete(c, &id))
}

#[tauri::command(async)]
pub fn stock_receive(core: State<'_, Core>, input: Receive) -> AppResult<String> {
    core.tx(|c| inv::receive(c, &input))
}

#[tauri::command(async)]
pub fn stock_issue(core: State<'_, Core>, input: Change) -> AppResult<String> {
    core.tx(|c| inv::issue(c, &input))
}

#[tauri::command(async)]
pub fn stock_return_in(core: State<'_, Core>, input: Change) -> AppResult<String> {
    core.tx(|c| inv::return_in(c, &input))
}

#[tauri::command(async)]
pub fn stock_return_out(core: State<'_, Core>, input: Change) -> AppResult<String> {
    core.tx(|c| inv::return_out(c, &input))
}

#[tauri::command(async)]
pub fn stock_damage(core: State<'_, Core>, input: Change) -> AppResult<String> {
    core.tx(|c| inv::mark_damaged(c, &input))
}

#[tauri::command(async)]
pub fn stock_transfer(core: State<'_, Core>, input: Transfer) -> AppResult<String> {
    core.tx(|c| inv::transfer(c, &input))
}

#[tauri::command(async)]
pub fn stock_adjust(core: State<'_, Core>, input: Adjust) -> AppResult<Option<String>> {
    core.tx(|c| inv::adjust(c, &input))
}

#[tauri::command(async)]
pub fn stock_count(core: State<'_, Core>, location_id: String, lines: Vec<CountLine>) -> AppResult<CountResult> {
    core.tx(|c| inv::post_count(c, &location_id, &lines))
}

#[tauri::command(async)]
pub fn stock_reserve(core: State<'_, Core>, input: Reserve) -> AppResult<Reservation> {
    core.tx(|c| inv::reserve(c, &input))
}

#[tauri::command(async)]
pub fn stock_release(core: State<'_, Core>, id: String) -> AppResult<Reservation> {
    check_id(&id)?;
    core.tx(|c| inv::release(c, &id))
}

#[tauri::command(async)]
pub fn reservation_list(core: State<'_, Core>) -> AppResult<Vec<Reservation>> {
    core.read(|c| inv::reservations(c, None, true))
}

#[tauri::command(async)]
pub fn movement_list(core: State<'_, Core>, item_id: Option<String>, limit: Option<i64>) -> AppResult<Vec<Movement>> {
    core.read(|c| inv::movements(c, item_id.as_deref(), limit.unwrap_or(500)))
}

#[tauri::command(async)]
pub fn movement_reverse(core: State<'_, Core>, id: String) -> AppResult<Vec<String>> {
    check_id(&id)?;
    core.tx(|c| inv::reverse(c, &id))
}

#[tauri::command(async)]
pub fn location_list(core: State<'_, Core>) -> AppResult<Vec<Location>> {
    core.read(inv::locations)
}

#[tauri::command(async)]
pub fn location_add(core: State<'_, Core>, name: String) -> AppResult<Location> {
    core.tx(|c| inv::add_location(c, &name))
}

#[tauri::command(async)]
pub fn location_rename(core: State<'_, Core>, id: String, name: String) -> AppResult<()> {
    core.tx(|c| inv::rename_location(c, &id, &name))
}

#[tauri::command(async)]
pub fn location_set_default(core: State<'_, Core>, id: String) -> AppResult<()> {
    core.tx(|c| inv::set_default_location(c, &id))
}

#[tauri::command(async)]
pub fn table_read(path: String) -> AppResult<Table> {
    csvio::read_table(&PathBuf::from(path))
}

#[tauri::command(async)]
pub fn table_guess_inventory(headers: Vec<String>) -> InventoryMapping {
    csvio::guess_inventory_mapping(&headers)
}

#[tauri::command(async)]
pub fn import_inventory_preview(core: State<'_, Core>, path: String, mapping: InventoryMapping) -> AppResult<InventoryPreview> {
    let table = csvio::read_table(&PathBuf::from(path))?;
    core.read(|c| csvio::preview_inventory(c, &table, &mapping))
}

/// Re-validates against the current data before writing, all rows in one transaction.
#[tauri::command(async)]
pub fn import_inventory_commit(core: State<'_, Core>, path: String, mapping: InventoryMapping) -> AppResult<ImportSummary> {
    let table = csvio::read_table(&PathBuf::from(path))?;
    core.tx(|c| {
        let preview = csvio::preview_inventory(c, &table, &mapping)?;
        csvio::commit_inventory(c, &preview)
    })
}

#[tauri::command(async)]
pub fn export_inventory(core: State<'_, Core>, path: String) -> AppResult<usize> {
    let ext = if path.to_lowercase().ends_with(".xlsx") { "xlsx" } else { "csv" };
    let out = core.output_path(&path, ext)?;
    let val = valuation(&core);
    let rows = core.read(|c| {
        inv::list(c, &ItemQuery { include_archived: true, ..Default::default() })?
            .into_iter()
            .map(|i| {
                let price = pricing::price_item(c, &i.id, 1, &[], &val)?.price.map(|p| p.to_string()).unwrap_or_default();
                Ok(vec![
                    i.sku,
                    i.name,
                    i.brand,
                    i.category,
                    i.numbers,
                    i.barcode.unwrap_or_default(),
                    i.condition,
                    i.on_hand.to_string(),
                    i.reserved.to_string(),
                    i.available.to_string(),
                    i.damaged.to_string(),
                    i.min_stock.to_string(),
                    i.reorder_qty.to_string(),
                    i.cost.to_string(),
                    i.avg_cost.to_string(),
                    price,
                    i.supplier_name.unwrap_or_default(),
                    i.bins,
                    if i.archived { "yes".into() } else { String::new() },
                ])
            })
            .collect::<AppResult<Vec<_>>>()
    })?;
    let headers = [
        "SKU",
        "Name",
        "Brand",
        "Category",
        "Part numbers",
        "Barcode",
        "Condition",
        "On hand",
        "Reserved",
        "Available",
        "Damaged",
        "Min stock",
        "Reorder qty",
        "Last cost",
        "Average cost",
        "Selling price",
        "Supplier",
        "Locations",
        "Archived",
    ];
    if ext == "xlsx" { csvio::write_xlsx(&out, "Inventory", &headers, rows) } else { csvio::write_csv(&out, &headers, rows) }
}

#[tauri::command(async)]
pub fn stock_on_hand_at(core: State<'_, Core>, location_id: String) -> AppResult<Vec<(String, i64)>> {
    core.read(|c| inv::on_hand_at(c, &location_id))
}
