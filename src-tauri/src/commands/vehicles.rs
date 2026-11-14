use super::{check_id, valuation};
use auto_manager_core::fitment::FitResult;
use auto_manager_core::matches::{self, VehicleMatches};
use auto_manager_core::providers::{Freshness, vpic};
use auto_manager_core::vehicles::{self, ManualVehicle, SpecInput, Vehicle, VehicleDetail, VehicleUpdate};
use auto_manager_core::vin::{self, VinCheck};
use auto_manager_core::{AppError, AppResult, Core, specs};
use serde::Serialize;
use tauri::State;

#[tauri::command]
pub fn vin_check(vin: String) -> VinCheck {
    vin::check(&vin)
}

#[derive(Serialize)]
pub struct DecodeOutcome {
    pub decoded: vpic::Decoded,
    pub freshness: Freshness,
    /// Set when the decode identified a vehicle and it was stored.
    pub vehicle: Option<Vehicle>,
}

/// Decode a full or partial VIN with NHTSA vPIC. A full VIN that identifies a vehicle is
/// stored (as a recent lookup until the person saves it).
#[tauri::command]
pub async fn decode_vin(core: State<'_, Core>, vin: String, model_year: Option<i32>, refresh: Option<bool>) -> AppResult<DecodeOutcome> {
    let v = match vin::check(&vin) {
        VinCheck::Invalid { reason } => return Err(AppError::invalid("vin", reason)),
        VinCheck::Valid { vin } | VinCheck::CheckDigitMismatch { vin, .. } | VinCheck::Partial { vin } => vin,
    };
    if model_year.is_some_and(|y| !(1981..=2100).contains(&y)) {
        return Err(AppError::invalid("model_year", "VIN decoding covers model years from 1981."));
    }
    let f = vpic::decode_with(&core, &v, model_year, refresh.unwrap_or(false)).await?;
    let full = v.len() == 17 && !v.contains('*');
    let vehicle = if full && f.value.identified { Some(core.tx(|c| vehicles::store_decoded(c, &f.value, vpic::SOURCE))?) } else { None };
    Ok(DecodeOutcome { decoded: f.value, freshness: f.freshness, vehicle })
}

#[tauri::command]
pub async fn vpic_makes(core: State<'_, Core>) -> AppResult<vpic::NameList> {
    vpic::makes(&core).await
}

#[tauri::command]
pub async fn vpic_models(core: State<'_, Core>, make: String, year: i32) -> AppResult<vpic::NameList> {
    if make.trim().is_empty() || make.len() > 80 {
        return Err(AppError::invalid("make", "Choose a make."));
    }
    vpic::models(&core, make.trim(), year).await
}

#[tauri::command(async)]
pub fn spec_fields() -> Vec<specs::SpecField> {
    specs::FIELDS.to_vec()
}

#[tauri::command(async)]
pub fn vehicle_create_manual(core: State<'_, Core>, input: ManualVehicle) -> AppResult<Vehicle> {
    core.tx(|c| vehicles::create_manual(c, &input))
}

#[tauri::command(async)]
pub fn vehicle_detail(core: State<'_, Core>, id: String) -> AppResult<VehicleDetail> {
    check_id(&id)?;
    core.tx(|c| {
        vehicles::touch(c, &id)?;
        vehicles::detail(c, &id)
    })
}

#[tauri::command(async)]
pub fn vehicle_list(core: State<'_, Core>, saved: bool) -> AppResult<Vec<Vehicle>> {
    core.read(|c| vehicles::list(c, saved, 1000))
}

#[tauri::command(async)]
pub fn vehicle_recent(core: State<'_, Core>) -> AppResult<Vec<Vehicle>> {
    core.read(|c| vehicles::recent(c, 20))
}

#[tauri::command(async)]
pub fn vehicle_update(core: State<'_, Core>, id: String, input: VehicleUpdate) -> AppResult<Vehicle> {
    check_id(&id)?;
    core.tx(|c| vehicles::update(c, &id, &input))
}

#[tauri::command(async)]
pub fn vehicle_set_saved(core: State<'_, Core>, id: String, saved: bool) -> AppResult<Vehicle> {
    check_id(&id)?;
    core.tx(|c| vehicles::set_saved(c, &id, saved))
}

#[tauri::command(async)]
pub fn vehicle_delete(core: State<'_, Core>, id: String) -> AppResult<()> {
    check_id(&id)?;
    core.tx(|c| vehicles::delete(c, &id))
}

#[tauri::command(async)]
pub fn vehicle_set_spec(core: State<'_, Core>, id: String, spec: SpecInput) -> AppResult<VehicleDetail> {
    check_id(&id)?;
    core.tx(|c| vehicles::set_spec(c, &id, &spec))
}

#[tauri::command(async)]
pub fn vehicle_clear_spec(core: State<'_, Core>, id: String, key: String) -> AppResult<VehicleDetail> {
    check_id(&id)?;
    core.tx(|c| vehicles::clear_user_spec(c, &id, &key))
}

#[tauri::command(async)]
pub fn vehicle_matches(core: State<'_, Core>, id: String, category_id: Option<String>, in_stock_only: bool) -> AppResult<VehicleMatches> {
    check_id(&id)?;
    let val = valuation(&core);
    core.read(|c| matches::for_vehicle(c, &id, category_id.as_deref(), in_stock_only, &val))
}

#[tauri::command(async)]
pub fn part_fit(core: State<'_, Core>, part_id: String, vehicle_id: String) -> AppResult<FitResult> {
    check_id(&part_id)?;
    check_id(&vehicle_id)?;
    core.read(|c| matches::fit_for(c, &part_id, &vehicle_id))
}

#[derive(Serialize)]
pub struct BulkRow {
    pub vin: String,
    /// "decoded", "not_identified" or "invalid"
    pub status: &'static str,
    pub title: String,
    pub vehicle_id: Option<String>,
    pub message: String,
}

/// Decode up to 50 VINs in one NHTSA batch request (the UI sends a large file in chunks, one
/// at a time, so the provider's rate limits are respected).
#[tauri::command]
pub async fn bulk_decode(core: State<'_, Core>, vins: Vec<String>, save: bool) -> AppResult<Vec<BulkRow>> {
    if vins.len() > 50 {
        return Err(AppError::msg("Send at most 50 VINs at a time."));
    }
    let mut rows: Vec<BulkRow> = Vec::new();
    let mut to_decode = Vec::new();
    for raw in vins {
        match vin::check(&raw) {
            VinCheck::Valid { vin } | VinCheck::CheckDigitMismatch { vin, .. } => to_decode.push(vin),
            VinCheck::Partial { vin } => rows.push(BulkRow {
                vin,
                status: "invalid",
                title: String::new(),
                vehicle_id: None,
                message: "Not a full 17-character VIN.".into(),
            }),
            VinCheck::Invalid { reason } => {
                rows.push(BulkRow { vin: raw, status: "invalid", title: String::new(), vehicle_id: None, message: reason })
            }
        }
    }
    if !to_decode.is_empty() {
        for d in vpic::decode_batch(&core, &to_decode).await? {
            if d.identified {
                let v = core.tx(|c| {
                    let v = vehicles::store_decoded(c, &d, vpic::SOURCE)?;
                    if save { vehicles::set_saved(c, &v.id, true) } else { Ok(v) }
                })?;
                rows.push(BulkRow {
                    vin: d.vin.clone(),
                    status: "decoded",
                    title: v.title(),
                    vehicle_id: Some(v.id),
                    message: if d.error_codes.iter().all(|c| c == "0") { String::new() } else { d.message },
                });
            } else {
                rows.push(BulkRow {
                    vin: d.vin.clone(),
                    status: "not_identified",
                    title: String::new(),
                    vehicle_id: None,
                    message: d.message,
                });
            }
        }
    }
    Ok(rows)
}
