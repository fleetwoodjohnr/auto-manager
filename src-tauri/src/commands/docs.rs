use super::check_id;
use auto_manager_core::manuals::{self, Annotation, AnnotationInput, Manual, ManualLink, ManualMeta, PageHit, WhereIsIt};
use auto_manager_core::{AppResult, Core, attachments};
use std::path::PathBuf;
use tauri::State;
use tauri::ipc::Response;

#[tauri::command(async)]
pub fn manual_list(core: State<'_, Core>, vehicle_id: Option<String>) -> AppResult<Vec<Manual>> {
    core.read(|c| manuals::list(c, vehicle_id.as_deref()))
}

#[tauri::command(async)]
pub fn manual_get(core: State<'_, Core>, id: String) -> AppResult<Manual> {
    check_id(&id)?;
    core.read(|c| manuals::get(c, &id))
}

#[tauri::command(async)]
pub fn manual_import(core: State<'_, Core>, path: String, meta: ManualMeta, links: Vec<ManualLink>) -> AppResult<Manual> {
    let paths = core.paths.clone();
    core.tx(|c| {
        let m = manuals::import(c, &paths, &PathBuf::from(&path), &meta)?;
        if !links.is_empty() { manuals::set_links(c, &m.id, &links) } else { Ok(m) }
    })
}

#[tauri::command(async)]
pub fn manual_update(core: State<'_, Core>, id: String, meta: ManualMeta) -> AppResult<Manual> {
    check_id(&id)?;
    core.tx(|c| manuals::update(c, &id, &meta))
}

#[tauri::command(async)]
pub fn manual_set_links(core: State<'_, Core>, id: String, links: Vec<ManualLink>) -> AppResult<Manual> {
    check_id(&id)?;
    core.tx(|c| manuals::set_links(c, &id, &links))
}

#[tauri::command(async)]
pub fn manual_delete(core: State<'_, Core>, id: String) -> AppResult<()> {
    check_id(&id)?;
    let paths = core.paths.clone();
    core.tx(|c| manuals::delete(c, &paths, &id))
}

/// The PDF bytes, sent as a raw binary response (no JSON encoding).
#[tauri::command(async)]
pub fn manual_bytes(core: State<'_, Core>, id: String) -> AppResult<Response> {
    check_id(&id)?;
    let paths = core.paths.clone();
    Ok(Response::new(core.read(|c| manuals::read_file(c, &paths, &id))?))
}

#[tauri::command(async)]
pub fn manual_set_pages(core: State<'_, Core>, id: String, page_count: i64, pages: Vec<(i64, String)>, done: bool) -> AppResult<()> {
    check_id(&id)?;
    core.tx(|c| manuals::set_pages_text(c, &id, page_count, &pages, done))
}

#[tauri::command(async)]
pub fn manual_search(core: State<'_, Core>, query: String, manual_ids: Option<Vec<String>>) -> AppResult<Vec<PageHit>> {
    core.read(|c| manuals::search_pages(c, manual_ids.as_deref(), &query, 50))
}

#[tauri::command(async)]
pub fn annotation_list(core: State<'_, Core>, manual_id: Option<String>, kind: Option<String>) -> AppResult<Vec<Annotation>> {
    core.read(|c| manuals::annotations(c, manual_id.as_deref(), kind.as_deref()))
}

#[tauri::command(async)]
pub fn annotation_add(core: State<'_, Core>, manual_id: String, input: AnnotationInput) -> AppResult<Annotation> {
    check_id(&manual_id)?;
    core.tx(|c| manuals::add_annotation(c, &manual_id, &input))
}

#[tauri::command(async)]
pub fn annotation_delete(core: State<'_, Core>, id: String) -> AppResult<()> {
    check_id(&id)?;
    core.tx(|c| manuals::delete_annotation(c, &id))
}

#[tauri::command(async)]
pub fn where_is(core: State<'_, Core>, vehicle_id: String, component: String) -> AppResult<WhereIsIt> {
    check_id(&vehicle_id)?;
    core.read(|c| manuals::where_is(c, &vehicle_id, &component))
}

#[tauri::command(async)]
pub fn attachment_list(core: State<'_, Core>, owner_kind: String, owner_id: String) -> AppResult<Vec<attachments::Attachment>> {
    check_id(&owner_id)?;
    core.read(|c| attachments::list(c, &owner_kind, &owner_id))
}

#[tauri::command(async)]
pub fn attachment_add(
    core: State<'_, Core>,
    owner_kind: String,
    owner_id: String,
    path: String,
    caption: Option<String>,
) -> AppResult<attachments::Attachment> {
    check_id(&owner_id)?;
    let paths = core.paths.clone();
    core.tx(|c| attachments::add(c, &paths, &owner_kind, &owner_id, &PathBuf::from(&path), caption.as_deref().unwrap_or("")))
}

#[tauri::command(async)]
pub fn attachment_bytes(core: State<'_, Core>, id: String) -> AppResult<Response> {
    check_id(&id)?;
    let paths = core.paths.clone();
    Ok(Response::new(core.read(|c| attachments::bytes(c, &paths, &id))?))
}

/// Open a stored file in the system's own viewer (PDF reader, image viewer).
#[tauri::command(async)]
pub fn attachment_open(app: tauri::AppHandle, core: State<'_, Core>, id: String) -> AppResult<()> {
    use tauri_plugin_opener::OpenerExt;
    check_id(&id)?;
    let paths = core.paths.clone();
    let p = core.read(|c| attachments::path_of(c, &paths, &id))?;
    app.opener()
        .open_path(p.to_string_lossy(), None::<&str>)
        .map_err(|e| auto_manager_core::AppError::Other(format!("Couldn't open the file: {e}")))
}

#[tauri::command(async)]
pub fn attachment_delete(core: State<'_, Core>, id: String) -> AppResult<()> {
    check_id(&id)?;
    let paths = core.paths.clone();
    core.tx(|c| attachments::delete(c, &paths, &id))
}

/// Read VINs and barcodes from a picture file the person chose.
#[tauri::command(async)]
pub fn scan_image(path: String) -> AppResult<auto_manager_core::scan::ScanResult> {
    auto_manager_core::scan::scan_file(&PathBuf::from(path))
}

/// Read VINs and barcodes from a camera frame (JPEG/PNG bytes sent as the raw request body).
#[tauri::command(async)]
pub fn scan_frame(request: tauri::ipc::Request<'_>) -> AppResult<auto_manager_core::scan::ScanResult> {
    match request.body() {
        tauri::ipc::InvokeBody::Raw(bytes) => auto_manager_core::scan::scan(bytes),
        _ => Err(auto_manager_core::AppError::msg("Send the picture as raw bytes.")),
    }
}
