//! Phase 3–4 commands: purchasing, reports, labels, saved searches, sync bundles and OBD-II.

use super::check_id;
use auto_manager_core::csvio;
use auto_manager_core::diagnostics::{self, ObdReading, ObdSession};
use auto_manager_core::folder_sync;
use auto_manager_core::labels::{self, Label};
use auto_manager_core::purchasing::{self, CostPoint, PoUpdate, PurchaseOrder, ReceiveLine, Suggestion, SupplierPrice, SupplierPriceInput};
use auto_manager_core::reports::{self, Report, ReportInfo, ReportQuery};
use auto_manager_core::search::{self, SavedSearch};
use auto_manager_core::{AppError, AppResult, Core, settings, sync};
use serde::Serialize;
use std::path::PathBuf;
use tauri::State;

#[tauri::command(async)]
pub fn supplier_prices(core: State<'_, Core>, item_id: Option<String>, supplier_id: Option<String>) -> AppResult<Vec<SupplierPrice>> {
    core.read(|c| match (&item_id, &supplier_id) {
        (Some(i), _) => purchasing::prices_for_item(c, i),
        (None, Some(s)) => purchasing::prices_for_supplier(c, s),
        _ => Err(AppError::msg("Choose an item or a supplier.")),
    })
}

#[tauri::command(async)]
pub fn supplier_price_set(core: State<'_, Core>, input: SupplierPriceInput) -> AppResult<()> {
    core.tx(|c| purchasing::set_price(c, &input))
}

#[tauri::command(async)]
pub fn supplier_price_remove(core: State<'_, Core>, supplier_id: String, item_id: String) -> AppResult<()> {
    core.tx(|c| purchasing::remove_price(c, &supplier_id, &item_id))
}

#[derive(Serialize)]
pub struct PriceListResult {
    pub updated: usize,
    pub not_found: Vec<String>,
}

#[tauri::command(async)]
pub fn supplier_price_list_import(core: State<'_, Core>, supplier_id: String, path: String) -> AppResult<PriceListResult> {
    let t = csvio::read_table(&PathBuf::from(path))?;
    core.tx(|c| purchasing::import_price_list(c, &supplier_id, &t)).map(|(updated, not_found)| PriceListResult { updated, not_found })
}

#[tauri::command(async)]
pub fn reorder_suggestions(core: State<'_, Core>) -> AppResult<Vec<Suggestion>> {
    core.read(purchasing::suggestions)
}

#[derive(Serialize)]
pub struct CreatedOrders {
    pub orders: Vec<PurchaseOrder>,
    pub without_supplier: Vec<String>,
}

#[tauri::command(async)]
pub fn reorder_create(core: State<'_, Core>, picks: Vec<(String, auto_manager_core::Qty)>) -> AppResult<CreatedOrders> {
    core.tx(|c| purchasing::orders_from_suggestions(c, &picks)).map(|(orders, without_supplier)| CreatedOrders { orders, without_supplier })
}

#[tauri::command(async)]
pub fn po_list(core: State<'_, Core>, open_only: bool) -> AppResult<Vec<PurchaseOrder>> {
    core.read(|c| purchasing::list_pos(c, open_only))
}

#[tauri::command(async)]
pub fn po_get(core: State<'_, Core>, id: String) -> AppResult<PurchaseOrder> {
    check_id(&id)?;
    core.read(|c| purchasing::get_po(c, &id))
}

#[tauri::command(async)]
pub fn po_create(core: State<'_, Core>, supplier_id: String) -> AppResult<PurchaseOrder> {
    core.tx(|c| purchasing::create_po(c, &supplier_id))
}

#[tauri::command(async)]
pub fn po_update(core: State<'_, Core>, id: String, input: PoUpdate) -> AppResult<PurchaseOrder> {
    core.tx(|c| purchasing::update_po(c, &id, &input))
}

#[tauri::command(async)]
pub fn po_add_line(
    core: State<'_, Core>,
    id: String,
    item_id: String,
    qty: auto_manager_core::Qty,
    unit_cost: Option<String>,
) -> AppResult<PurchaseOrder> {
    core.tx(|c| purchasing::add_line(c, &id, &item_id, qty, unit_cost.as_deref()))
}

#[tauri::command(async)]
pub fn po_update_line(core: State<'_, Core>, line_id: String, qty: auto_manager_core::Qty, unit_cost: String) -> AppResult<PurchaseOrder> {
    core.tx(|c| purchasing::update_line(c, &line_id, qty, &unit_cost))
}

#[tauri::command(async)]
pub fn po_remove_line(core: State<'_, Core>, line_id: String) -> AppResult<PurchaseOrder> {
    core.tx(|c| purchasing::remove_line(c, &line_id))
}

#[tauri::command(async)]
pub fn po_set_status(core: State<'_, Core>, id: String, status: String) -> AppResult<PurchaseOrder> {
    core.tx(|c| purchasing::set_po_status(c, &id, &status))
}

#[tauri::command(async)]
pub fn po_receive(core: State<'_, Core>, id: String, location_id: String, lines: Vec<ReceiveLine>) -> AppResult<PurchaseOrder> {
    core.tx(|c| purchasing::receive(c, &id, &location_id, &lines))
}

#[tauri::command(async)]
pub fn po_delete(core: State<'_, Core>, id: String) -> AppResult<()> {
    core.tx(|c| purchasing::delete_po(c, &id))
}

#[tauri::command(async)]
pub fn cost_history(core: State<'_, Core>, item_id: String) -> AppResult<Vec<CostPoint>> {
    check_id(&item_id)?;
    core.read(|c| purchasing::cost_history(c, &item_id))
}

#[tauri::command(async)]
pub fn report_list() -> Vec<ReportInfo> {
    reports::list()
}

#[tauri::command(async)]
pub fn report(core: State<'_, Core>, query: ReportQuery) -> AppResult<Report> {
    core.read(|c| reports::run(c, &query))
}

#[tauri::command(async)]
pub fn report_export(core: State<'_, Core>, query: ReportQuery, path: String) -> AppResult<usize> {
    let ext = if path.to_lowercase().ends_with(".xlsx") { "xlsx" } else { "csv" };
    let out = core.output_path(&path, ext)?;
    let r = core.read(|c| reports::run(c, &query))?;
    reports::export(&out, &r)
}

#[tauri::command(async)]
pub fn labels(core: State<'_, Core>, values: Vec<String>) -> AppResult<Vec<Label>> {
    if values.len() > 500 {
        return Err(AppError::msg("Print at most 500 labels at a time."));
    }
    let sym = core.read(|c| Ok(settings::load(c).label_symbology))?;
    values.iter().map(|v| labels::make(v, &sym)).collect()
}

#[tauri::command(async)]
pub fn saved_search_list(core: State<'_, Core>, page: String) -> AppResult<Vec<SavedSearch>> {
    core.read(|c| search::saved_searches(c, &page))
}

#[tauri::command(async)]
pub fn saved_search_add(core: State<'_, Core>, name: String, page: String, filters: serde_json::Value) -> AppResult<SavedSearch> {
    core.tx(|c| search::save_search(c, &name, &page, &filters))
}

#[tauri::command(async)]
pub fn saved_search_delete(core: State<'_, Core>, id: String) -> AppResult<()> {
    core.tx(|c| search::delete_saved_search(c, &id))
}

#[tauri::command(async)]
pub fn sync_export(core: State<'_, Core>, path: String, passphrase: String) -> AppResult<()> {
    let out = core.output_path(&path, "amsync")?;
    let paths = core.paths.clone();
    sync::export(&core, &paths, &out, &passphrase)
}

#[tauri::command(async)]
pub fn sync_import(core: State<'_, Core>, path: String, passphrase: String) -> AppResult<auto_manager_core::backup::Manifest> {
    sync::import(&core, &PathBuf::from(path), &passphrase)
}

#[tauri::command(async)]
pub fn obd_ports() -> Vec<String> {
    auto_manager_obd::ports()
}

/// Connect to an adapter and read codes, freeze frame and live values (read-only). The result
/// is stored as a session for the vehicle.
#[tauri::command]
pub async fn obd_scan(core: State<'_, Core>, port: String, vehicle_id: Option<String>) -> AppResult<ObdSession> {
    if port.trim().is_empty() || port.len() > 200 {
        return Err(AppError::msg("Choose the adapter's port."));
    }
    let scan = tauri::async_runtime::spawn_blocking(move || auto_manager_obd::connect_serial(&port).and_then(|mut e| e.scan()))
        .await
        .map_err(|e| AppError::Other(e.to_string()))?
        .map_err(|e| AppError::Provider { message: e.to_string(), offline: false })?;
    let mut readings: Vec<ObdReading> = Vec::new();
    readings.extend(scan.stored.iter().map(|c| ObdReading {
        kind: "stored_dtc".into(),
        code: c.clone(),
        value: String::new(),
        unit: String::new(),
    }));
    readings.extend(scan.pending.iter().map(|c| ObdReading {
        kind: "pending_dtc".into(),
        code: c.clone(),
        value: String::new(),
        unit: String::new(),
    }));
    if let Some(m) = scan.mil_on {
        readings.push(ObdReading {
            kind: "readiness".into(),
            code: "MIL".into(),
            value: if m { "on".into() } else { "off".into() },
            unit: String::new(),
        });
    }
    readings.extend(scan.live.iter().map(|r| ObdReading {
        kind: "pid".into(),
        code: r.name.clone(),
        value: r.value.clone(),
        unit: r.unit.clone(),
    }));
    if let Some(d) = &scan.freeze_dtc {
        readings.push(ObdReading { kind: "freeze_frame".into(), code: "Triggered by".into(), value: d.clone(), unit: String::new() });
    }
    readings.extend(scan.freeze_frame.iter().map(|r| ObdReading {
        kind: "freeze_frame".into(),
        code: r.name.clone(),
        value: r.value.clone(),
        unit: r.unit.clone(),
    }));
    let id = core.tx(|c| diagnostics::save(c, vehicle_id.as_deref(), &scan.adapter, &scan.protocol, scan.vin.as_deref(), &readings))?;
    core.read(|c| diagnostics::sessions(c, None))?.into_iter().find(|s| s.id == id).ok_or_else(|| AppError::Other("Scan not saved".into()))
}

#[tauri::command(async)]
pub fn obd_sessions(core: State<'_, Core>, vehicle_id: Option<String>) -> AppResult<Vec<ObdSession>> {
    core.read(|c| diagnostics::sessions(c, vehicle_id.as_deref()))
}

// ---------------------------------------------------------------- two-way sync through a folder

#[tauri::command(async)]
pub fn folder_sync_status(core: State<'_, Core>) -> AppResult<folder_sync::SyncStatus> {
    Ok(folder_sync::status(&core.paths))
}

/// A name for this computer in other computers' sync lists.
#[tauri::command(async)]
pub fn folder_sync_default_name() -> String {
    ["COMPUTERNAME", "HOSTNAME"]
        .iter()
        .find_map(|k| std::env::var(k).ok())
        .or_else(|| std::fs::read_to_string("/etc/hostname").ok())
        .map(|n| n.trim().to_string())
        .filter(|n| !n.is_empty())
        .unwrap_or_else(|| "This computer".into())
}

#[tauri::command(async)]
pub fn folder_sync_setup(core: State<'_, Core>, folder: String, passphrase: String, device_name: String) -> AppResult<folder_sync::Report> {
    folder_sync::setup(&core.paths, &PathBuf::from(folder), &passphrase, &device_name)?;
    folder_sync::sync(&core)
}

#[tauri::command(async)]
pub fn folder_sync_now(core: State<'_, Core>) -> AppResult<folder_sync::Report> {
    folder_sync::sync(&core)
}

#[tauri::command(async)]
pub fn folder_sync_set_auto(core: State<'_, Core>, auto: bool) -> AppResult<()> {
    folder_sync::set_auto(&core.paths, auto)
}

#[tauri::command(async)]
pub fn folder_sync_stop(core: State<'_, Core>) -> AppResult<()> {
    folder_sync::stop(&core.paths)
}
