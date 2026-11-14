use super::check_id;
use auto_manager_core::manuals::{self, Annotation, AnnotationInput, Manual, ManualLink, ManualMeta, PageHit, WhereIsIt};
use auto_manager_core::{AppResult, Core};
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
