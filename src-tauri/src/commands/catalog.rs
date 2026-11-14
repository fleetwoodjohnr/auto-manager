use super::check_id;
use auto_manager_core::parts::{self, Category, Fitment, FitmentInput, Part, PartInput, PartQuery, PartRow};
use auto_manager_core::{AppResult, Core, inventory, lists};
use serde::Serialize;
use tauri::State;

#[tauri::command(async)]
pub fn categories(core: State<'_, Core>) -> AppResult<Vec<Category>> {
    core.read(parts::categories)
}

#[tauri::command(async)]
pub fn category_add(core: State<'_, Core>, parent_id: Option<String>, name: String) -> AppResult<Category> {
    core.tx(|c| parts::add_category(c, parent_id.as_deref(), &name))
}

#[tauri::command(async)]
pub fn part_list(core: State<'_, Core>, query: PartQuery) -> AppResult<Vec<PartRow>> {
    core.read(|c| parts::list(c, &query))
}

#[tauri::command(async)]
pub fn part_brands(core: State<'_, Core>) -> AppResult<Vec<String>> {
    core.read(parts::brands)
}

#[derive(Serialize)]
pub struct PartDetail {
    pub part: Part,
    pub fitments: Vec<Fitment>,
    pub items: Vec<inventory::Item>,
}

#[tauri::command(async)]
pub fn part_get(core: State<'_, Core>, id: String) -> AppResult<PartDetail> {
    check_id(&id)?;
    core.read(|c| {
        Ok(PartDetail { part: parts::get(c, &id)?, fitments: parts::fitments_for_part(c, &id)?, items: inventory::for_part(c, &id)? })
    })
}

#[tauri::command(async)]
pub fn part_create(core: State<'_, Core>, input: PartInput) -> AppResult<Part> {
    core.tx(|c| parts::create(c, &input, "user"))
}

#[tauri::command(async)]
pub fn part_update(core: State<'_, Core>, id: String, input: PartInput) -> AppResult<Part> {
    check_id(&id)?;
    core.tx(|c| parts::update(c, &id, &input))
}

#[tauri::command(async)]
pub fn part_delete(core: State<'_, Core>, id: String) -> AppResult<()> {
    check_id(&id)?;
    core.tx(|c| parts::delete(c, &id))
}

#[tauri::command(async)]
pub fn fitment_add(core: State<'_, Core>, part_id: String, input: FitmentInput) -> AppResult<Fitment> {
    check_id(&part_id)?;
    core.tx(|c| parts::add_fitment(c, &part_id, &input, "user", false))
}

#[tauri::command(async)]
pub fn fitment_update(core: State<'_, Core>, id: String, input: FitmentInput) -> AppResult<Fitment> {
    check_id(&id)?;
    core.tx(|c| parts::update_fitment(c, &id, &input))
}

#[tauri::command(async)]
pub fn fitment_verify(core: State<'_, Core>, id: String, verified: bool) -> AppResult<Fitment> {
    check_id(&id)?;
    core.tx(|c| parts::verify_fitment(c, &id, verified))
}

#[tauri::command(async)]
pub fn fitment_delete(core: State<'_, Core>, id: String) -> AppResult<()> {
    check_id(&id)?;
    core.tx(|c| parts::delete_fitment(c, &id))
}

#[tauri::command(async)]
pub fn plist_list(core: State<'_, Core>, vehicle_id: Option<String>) -> AppResult<Vec<lists::PartList>> {
    core.read(|c| lists::list(c, vehicle_id.as_deref()))
}

#[tauri::command(async)]
pub fn plist_create(core: State<'_, Core>, name: String, vehicle_id: Option<String>) -> AppResult<lists::PartList> {
    core.tx(|c| lists::create(c, &name, vehicle_id.as_deref()))
}

#[tauri::command(async)]
pub fn plist_add(core: State<'_, Core>, list_id: String, part_id: String, qty: i64, note: Option<String>) -> AppResult<lists::PartList> {
    check_id(&list_id)?;
    check_id(&part_id)?;
    core.tx(|c| lists::add(c, &list_id, &part_id, qty, note.as_deref().unwrap_or("")))
}

#[tauri::command(async)]
pub fn plist_remove(core: State<'_, Core>, list_id: String, part_id: String) -> AppResult<lists::PartList> {
    core.tx(|c| lists::remove(c, &list_id, &part_id))
}

#[tauri::command(async)]
pub fn plist_delete(core: State<'_, Core>, id: String) -> AppResult<()> {
    core.tx(|c| lists::delete(c, &id))
}

#[tauri::command(async)]
pub fn restock_request(core: State<'_, Core>, part_id: String) -> AppResult<lists::PartList> {
    check_id(&part_id)?;
    core.tx(|c| lists::restock_request(c, &part_id))
}
