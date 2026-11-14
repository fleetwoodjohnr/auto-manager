//! Phase 2 commands: recalls, safety ratings, extra spec sources, maintenance, trouble codes,
//! interchange, catalog imports and data quality.

use super::check_id;
use auto_manager_core::aces_pies::{self, AcesSummary, PiesSummary};
use auto_manager_core::csvio::{self, FitmentPreview, ImportSummary};
use auto_manager_core::dtc::{self, Dtc};
use auto_manager_core::interchange::{self, Interchange};
use auto_manager_core::maintenance::{self, DueItem, Interval, IntervalInput, Record, RecordInput};
use auto_manager_core::manuals::{self, Manual, ManualLink, ManualMeta};
use auto_manager_core::providers::bulletins;
use auto_manager_core::providers::oem_docs::{self, OemLink};
use auto_manager_core::providers::recalls::{self, RecallNotice, VehicleRecalls};
use auto_manager_core::providers::safety::{self, Ratings};
use auto_manager_core::providers::{canada, epa};
use auto_manager_core::quality::{self, IssueGroup};
use auto_manager_core::vehicles::{self, VehicleDetail};
use auto_manager_core::{AppError, AppResult, Core};
use std::path::PathBuf;
use tauri::State;

fn today() -> chrono::NaiveDate {
    chrono::Local::now().date_naive()
}

/// Ask NHTSA (or use the saved copy when offline).
#[tauri::command]
pub async fn recalls_fetch(core: State<'_, Core>, vehicle_id: String) -> AppResult<VehicleRecalls> {
    check_id(&vehicle_id)?;
    recalls::for_vehicle(&core, &vehicle_id).await
}

/// Recalls saved earlier, without going online.
#[tauri::command(async)]
pub fn recalls_saved(core: State<'_, Core>, vehicle_id: String) -> AppResult<VehicleRecalls> {
    check_id(&vehicle_id)?;
    core.read(|c| recalls::saved_for_vehicle(c, &vehicle_id))
}

#[tauri::command(async)]
pub fn recall_set_status(
    core: State<'_, Core>,
    vehicle_id: String,
    campaign: String,
    status: Option<String>,
    note: Option<String>,
) -> AppResult<()> {
    check_id(&vehicle_id)?;
    core.tx(|c| recalls::set_status(c, &vehicle_id, &campaign, status.as_deref(), note.as_deref().unwrap_or("")))
}

#[tauri::command(async)]
pub fn recall_notices(core: State<'_, Core>) -> AppResult<Vec<RecallNotice>> {
    core.read(recalls::notices)
}

#[tauri::command]
pub async fn safety_ratings(core: State<'_, Core>, vehicle_id: String) -> AppResult<Ratings> {
    check_id(&vehicle_id)?;
    let Some((y, make, model)) = core.read(|c| recalls::vehicle_key(c, &vehicle_id))? else {
        return Err(AppError::msg("Ratings are listed by model year, make and model; add those to this vehicle first."));
    };
    safety::for_model(&core, y, &make, &model).await
}

#[tauri::command]
pub async fn canada_options(core: State<'_, Core>, vehicle_id: String) -> AppResult<canada::CanadaOptions> {
    check_id(&vehicle_id)?;
    let Some((y, make, model)) = core.read(|c| recalls::vehicle_key(c, &vehicle_id))? else {
        return Err(AppError::msg("Add the model year, make and model first."));
    };
    canada::options(&core, y, &make, &model).await
}

#[tauri::command]
pub async fn canada_apply(core: State<'_, Core>, vehicle_id: String, index: i64) -> AppResult<VehicleDetail> {
    let opts = canada_options(core.clone(), vehicle_id.clone()).await?;
    let row = opts.rows.into_iter().find(|r| r.index == index).ok_or_else(|| AppError::msg("Choose one of the listed body styles."))?;
    core.tx(|c| {
        vehicles::store_values(c, &vehicle_id, canada::SOURCE, "model_level", &row.values)?;
        vehicles::detail(c, &vehicle_id)
    })
}

#[tauri::command]
pub async fn epa_options(core: State<'_, Core>, vehicle_id: String) -> AppResult<epa::EpaOptions> {
    check_id(&vehicle_id)?;
    let Some((y, make, model)) = core.read(|c| recalls::vehicle_key(c, &vehicle_id))? else {
        return Err(AppError::msg("Add the model year, make and model first."));
    };
    // fueleconomy.gov uses its own capitalisation of makes ("Nissan", not "NISSAN").
    epa::options(&core, y, &vehicles::display_make(&make), &model).await
}

#[tauri::command]
pub async fn epa_apply(core: State<'_, Core>, vehicle_id: String, option_id: String) -> AppResult<VehicleDetail> {
    check_id(&vehicle_id)?;
    let (values, _) = epa::vehicle(&core, &option_id).await?;
    core.tx(|c| {
        vehicles::store_values(c, &vehicle_id, epa::SOURCE, "model_level", &values)?;
        vehicles::detail(c, &vehicle_id)
    })
}

#[tauri::command(async)]
pub fn oem_link(make: String) -> Option<OemLink> {
    oem_docs::for_make(&make)
}

#[tauri::command(async)]
pub fn maintenance_intervals(core: State<'_, Core>, vehicle_id: String) -> AppResult<Vec<Interval>> {
    check_id(&vehicle_id)?;
    core.read(|c| maintenance::intervals(c, &vehicle_id, today()))
}

#[tauri::command(async)]
pub fn maintenance_interval_save(core: State<'_, Core>, vehicle_id: String, id: Option<String>, input: IntervalInput) -> AppResult<()> {
    check_id(&vehicle_id)?;
    core.tx(|c| match &id {
        Some(id) => maintenance::update_interval(c, id, &input),
        None => maintenance::add_interval(c, &vehicle_id, &input).map(|_| ()),
    })
}

#[tauri::command(async)]
pub fn maintenance_interval_delete(core: State<'_, Core>, id: String) -> AppResult<()> {
    core.tx(|c| maintenance::delete_interval(c, &id))
}

#[tauri::command(async)]
pub fn maintenance_copy(core: State<'_, Core>, from_vehicle: String, to_vehicle: String) -> AppResult<usize> {
    core.tx(|c| maintenance::copy_intervals(c, &from_vehicle, &to_vehicle))
}

#[tauri::command(async)]
pub fn maintenance_records(core: State<'_, Core>, vehicle_id: String) -> AppResult<Vec<Record>> {
    check_id(&vehicle_id)?;
    core.read(|c| maintenance::records(c, &vehicle_id))
}

#[tauri::command(async)]
pub fn maintenance_record_save(core: State<'_, Core>, vehicle_id: String, id: Option<String>, input: RecordInput) -> AppResult<()> {
    check_id(&vehicle_id)?;
    core.tx(|c| match &id {
        Some(id) => maintenance::update_record(c, id, &input),
        None => maintenance::add_record(c, &vehicle_id, &input).map(|_| ()),
    })
}

#[tauri::command(async)]
pub fn maintenance_record_delete(core: State<'_, Core>, id: String) -> AppResult<()> {
    core.tx(|c| maintenance::delete_record(c, &id))
}

#[tauri::command(async)]
pub fn maintenance_due(core: State<'_, Core>) -> AppResult<Vec<DueItem>> {
    core.read(|c| maintenance::due(c, today()))
}

#[tauri::command(async)]
pub fn dtc_search(core: State<'_, Core>, query: String) -> AppResult<Vec<Dtc>> {
    if query.len() > 100 {
        return Err(AppError::msg("That search is too long."));
    }
    // The first search loads the bundled codes into the database.
    core.tx(dtc::ensure_loaded)?;
    core.read(|c| dtc::search(c, &query, 60))
}

#[tauri::command(async)]
pub fn interchange_get(core: State<'_, Core>, part_id: String) -> AppResult<Interchange> {
    check_id(&part_id)?;
    core.read(|c| interchange::get(c, &part_id))
}

#[tauri::command(async)]
pub fn supersession_add(core: State<'_, Core>, old_part_id: String, new_part_id: String, note: Option<String>) -> AppResult<()> {
    core.tx(|c| interchange::add_supersession(c, &old_part_id, &new_part_id, note.as_deref().unwrap_or(""), "user"))
}

#[tauri::command(async)]
pub fn supersession_remove(core: State<'_, Core>, old_part_id: String, new_part_id: String) -> AppResult<()> {
    core.tx(|c| interchange::remove_supersession(c, &old_part_id, &new_part_id))
}

#[tauri::command(async)]
pub fn equivalent_add(core: State<'_, Core>, part_id: String, other_id: String, note: Option<String>) -> AppResult<()> {
    core.tx(|c| interchange::add_equivalent(c, &part_id, &other_id, note.as_deref().unwrap_or(""), "user"))
}

#[tauri::command(async)]
pub fn equivalent_remove(core: State<'_, Core>, part_id: String, other_id: String) -> AppResult<()> {
    core.tx(|c| interchange::remove_equivalent(c, &part_id, &other_id))
}

#[tauri::command(async)]
pub fn import_fitment_preview(core: State<'_, Core>, path: String) -> AppResult<FitmentPreview> {
    let t = csvio::read_table(&PathBuf::from(path))?;
    core.read(|c| csvio::preview_fitment(c, &t))
}

#[tauri::command(async)]
pub fn import_fitment_commit(core: State<'_, Core>, path: String, authoritative: bool) -> AppResult<ImportSummary> {
    let p = PathBuf::from(&path);
    let t = csvio::read_table(&p)?;
    let name = p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
    core.tx(|c| {
        let pv = csvio::preview_fitment(c, &t)?;
        csvio::commit_fitment(c, &pv, &name, authoritative)
    })
}

#[tauri::command(async)]
pub fn import_aces(
    core: State<'_, Core>,
    path: String,
    vehicle_map: String,
    attribute_map: Option<String>,
    authoritative: bool,
) -> AppResult<AcesSummary> {
    core.tx(|c| {
        aces_pies::import_aces_files(
            c,
            &PathBuf::from(&path),
            &PathBuf::from(&vehicle_map),
            attribute_map.as_deref().map(std::path::Path::new),
            authoritative,
        )
    })
}

#[tauri::command(async)]
pub fn import_pies(core: State<'_, Core>, path: String, category_id: Option<String>) -> AppResult<PiesSummary> {
    core.tx(|c| aces_pies::import_pies_file(c, &PathBuf::from(&path), category_id.as_deref()))
}

#[tauri::command(async)]
pub fn data_quality(core: State<'_, Core>) -> AppResult<Vec<IssueGroup>> {
    core.read(quality::report)
}

/// Manufacturer communications (service bulletins) NHTSA lists for the vehicle's model.
#[tauri::command]
pub async fn bulletins_fetch(core: State<'_, Core>, vehicle_id: String) -> AppResult<bulletins::Bulletins> {
    check_id(&vehicle_id)?;
    let Some((y, make, model)) = core.read(|c| recalls::vehicle_key(c, &vehicle_id))? else {
        return Err(AppError::msg("Bulletins are listed by model year, make and model; add those to this vehicle first."));
    };
    bulletins::for_model(&core, y, &make, &model).await
}

#[tauri::command]
pub async fn bulletin_documents(core: State<'_, Core>, nhtsa_id: i64) -> AppResult<Vec<bulletins::BulletinDocument>> {
    bulletins::documents(&core, nhtsa_id).await
}

/// Download a bulletin PDF into the document library, linked to the vehicle's year, make and model.
#[tauri::command]
pub async fn bulletin_import(core: State<'_, Core>, vehicle_id: String, url: String, title: String) -> AppResult<Manual> {
    check_id(&vehicle_id)?;
    let Some((y, make, model)) = core.read(|c| recalls::vehicle_key(c, &vehicle_id))? else {
        return Err(AppError::msg("Add the model year, make and model first."));
    };
    let bytes = bulletins::download(&core, &url).await?;
    // Keep the bulletin's own file name (it's what the library shows); only safe characters.
    let name: String =
        url.rsplit('/').next().unwrap_or("").chars().filter(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.')).collect();
    let name = if name.ends_with(".pdf") && !name.starts_with('.') { name } else { "bulletin.pdf".into() };
    let dir = core.paths.documents().join(auto_manager_core::db::new_id());
    std::fs::create_dir_all(&dir)?;
    let tmp = dir.join(name);
    std::fs::write(&tmp, &bytes)?;
    let paths = core.paths.clone();
    let meta = ManualMeta {
        title,
        kind: "tsb".into(),
        publisher: make.clone(),
        source_url: url.clone(),
        license_note: "Public record published by NHTSA".into(),
    };
    let result = core.tx(|c| {
        let m = manuals::import(c, &paths, &tmp, &meta)?;
        let mut links = m.links.clone();
        let wanted = ManualLink {
            vehicle_id: None,
            vehicle_title: None,
            make: Some(make.clone()),
            model: Some(model.clone()),
            year_from: Some(y),
            year_to: Some(y),
        };
        if !links.iter().any(|l| l.make == wanted.make && l.model == wanted.model && l.year_from == wanted.year_from) {
            links.push(wanted);
        }
        manuals::set_links(c, &m.id, &links)
    });
    let _ = std::fs::remove_dir_all(&dir);
    result
}

/// Specification values found in the vehicle's own documents, each with its page (nothing is
/// saved until the person picks one).
#[tauri::command(async)]
pub fn manual_spec_suggestions(core: State<'_, Core>, vehicle_id: String) -> AppResult<Vec<auto_manager_core::manual_facts::SpecFound>> {
    check_id(&vehicle_id)?;
    core.read(|c| auto_manager_core::manual_facts::find_specs(c, &vehicle_id))
}

/// Service intervals found in the vehicle's own documents.
#[tauri::command(async)]
pub fn manual_interval_suggestions(
    core: State<'_, Core>,
    vehicle_id: String,
) -> AppResult<Vec<auto_manager_core::manual_facts::IntervalFound>> {
    check_id(&vehicle_id)?;
    core.read(|c| auto_manager_core::manual_facts::find_intervals(c, &vehicle_id))
}
