use super::{check_id, valuation};
use auto_manager_core::customers::{self, Customer, CustomerInput};
use auto_manager_core::jobs::{self, Job, JobDetail, JobInput, JobQuery, LineUpdate, OtherLineInput, PartLineInput};
use auto_manager_core::pricing::{self, PriceInput, PriceResult, Profile, ProfileInput, RuleInput};
use auto_manager_core::suppliers::{self, Supplier, SupplierInput};
use auto_manager_core::{AppResult, Core, money, parts};
use tauri::State;

#[tauri::command(async)]
pub fn profile_list(core: State<'_, Core>) -> AppResult<Vec<Profile>> {
    core.read(pricing::profiles)
}

#[tauri::command(async)]
pub fn profile_save(core: State<'_, Core>, id: Option<String>, input: ProfileInput) -> AppResult<Profile> {
    core.tx(|c| pricing::save_profile(c, id.as_deref(), &input))
}

#[tauri::command(async)]
pub fn profile_set_default(core: State<'_, Core>, id: String) -> AppResult<()> {
    core.tx(|c| pricing::set_default_profile(c, &id))
}

#[tauri::command(async)]
pub fn profile_delete(core: State<'_, Core>, id: String) -> AppResult<()> {
    core.tx(|c| pricing::delete_profile(c, &id))
}

#[tauri::command(async)]
pub fn rule_save(core: State<'_, Core>, profile_id: String, id: Option<String>, input: RuleInput) -> AppResult<Profile> {
    core.tx(|c| pricing::save_rule(c, &profile_id, id.as_deref(), &input))
}

#[tauri::command(async)]
pub fn rule_delete(core: State<'_, Core>, id: String) -> AppResult<()> {
    core.tx(|c| pricing::delete_rule(c, &id))
}

/// Try a profile on a hypothetical product ("what would a $50 brake pad sell for?").
#[tauri::command(async)]
pub fn price_preview(
    core: State<'_, Core>,
    profile_id: String,
    cost: String,
    category_id: Option<String>,
    brand: Option<String>,
    qty: Option<i64>,
) -> AppResult<PriceResult> {
    let cost = money::non_negative("cost", "Cost", money::parse("cost", "Cost", &cost)?)?;
    core.read(|c| {
        let p = pricing::profile(c, &profile_id)?;
        let categories = match &category_id {
            Some(id) => parts::category_chain(c, id)?,
            None => vec![],
        };
        Ok(pricing::compute(
            &p,
            &PriceInput { cost, categories, brand: brand.unwrap_or_default(), item_id: None, qty: qty.unwrap_or(1), price_override: None },
        ))
    })
}

#[tauri::command(async)]
pub fn price_item(core: State<'_, Core>, item_id: String, qty: i64, profile_id: Option<String>) -> AppResult<PriceResult> {
    check_id(&item_id)?;
    let val = valuation(&core);
    core.read(|c| pricing::price_item(c, &item_id, qty, &[profile_id.as_deref()], &val))
}

#[tauri::command(async)]
pub fn job_list(core: State<'_, Core>, query: JobQuery) -> AppResult<Vec<Job>> {
    core.read(|c| jobs::list(c, &query))
}

#[tauri::command(async)]
pub fn job_get(core: State<'_, Core>, id: String) -> AppResult<JobDetail> {
    check_id(&id)?;
    core.read(|c| jobs::detail(c, &id))
}

#[tauri::command(async)]
pub fn job_create(core: State<'_, Core>, input: JobInput) -> AppResult<JobDetail> {
    core.tx(|c| jobs::create(c, &input))
}

#[tauri::command(async)]
pub fn job_update(core: State<'_, Core>, id: String, input: JobInput) -> AppResult<JobDetail> {
    check_id(&id)?;
    core.tx(|c| jobs::update(c, &id, &input))
}

#[tauri::command(async)]
pub fn job_add_part(core: State<'_, Core>, id: String, input: PartLineInput) -> AppResult<JobDetail> {
    check_id(&id)?;
    let val = valuation(&core);
    core.tx(|c| jobs::add_part_line(c, &id, &input, &val))
}

#[tauri::command(async)]
pub fn job_add_other(core: State<'_, Core>, id: String, input: OtherLineInput) -> AppResult<JobDetail> {
    check_id(&id)?;
    core.tx(|c| jobs::add_other_line(c, &id, &input))
}

#[tauri::command(async)]
pub fn job_update_line(core: State<'_, Core>, line_id: String, input: LineUpdate) -> AppResult<JobDetail> {
    check_id(&line_id)?;
    core.tx(|c| jobs::update_line(c, &line_id, &input))
}

#[tauri::command(async)]
pub fn job_delete_line(core: State<'_, Core>, line_id: String) -> AppResult<JobDetail> {
    check_id(&line_id)?;
    core.tx(|c| jobs::delete_line(c, &line_id))
}

#[tauri::command(async)]
pub fn job_reserve_line(core: State<'_, Core>, line_id: String) -> AppResult<JobDetail> {
    check_id(&line_id)?;
    core.tx(|c| jobs::reserve_line(c, &line_id))
}

#[tauri::command(async)]
pub fn job_release_line(core: State<'_, Core>, line_id: String) -> AppResult<JobDetail> {
    check_id(&line_id)?;
    core.tx(|c| jobs::release_line(c, &line_id))
}

#[tauri::command(async)]
pub fn job_set_status(core: State<'_, Core>, id: String, status: String) -> AppResult<JobDetail> {
    check_id(&id)?;
    core.tx(|c| {
        let d = jobs::set_status(c, &id, &status)?;
        // Completed work goes into the vehicle's maintenance history.
        if status == "completed" {
            auto_manager_core::maintenance::record_from_job(c, &id)?;
        }
        Ok(d)
    })
}

#[tauri::command(async)]
pub fn job_delete(core: State<'_, Core>, id: String) -> AppResult<()> {
    check_id(&id)?;
    core.tx(|c| jobs::delete(c, &id))
}

#[tauri::command(async)]
pub fn customer_list(core: State<'_, Core>, q: Option<String>) -> AppResult<Vec<Customer>> {
    core.read(|c| customers::list(c, q.as_deref()))
}

#[tauri::command(async)]
pub fn customer_get(core: State<'_, Core>, id: String) -> AppResult<Customer> {
    check_id(&id)?;
    core.read(|c| customers::get(c, &id))
}

#[tauri::command(async)]
pub fn customer_save(core: State<'_, Core>, id: Option<String>, input: CustomerInput) -> AppResult<Customer> {
    core.tx(|c| match &id {
        Some(id) => customers::update(c, id, &input),
        None => customers::create(c, &input),
    })
}

#[tauri::command(async)]
pub fn customer_delete(core: State<'_, Core>, id: String) -> AppResult<()> {
    check_id(&id)?;
    core.tx(|c| customers::delete(c, &id))
}

#[tauri::command(async)]
pub fn supplier_list(core: State<'_, Core>) -> AppResult<Vec<Supplier>> {
    core.read(suppliers::list)
}

#[tauri::command(async)]
pub fn supplier_save(core: State<'_, Core>, id: Option<String>, input: SupplierInput) -> AppResult<Supplier> {
    core.tx(|c| match &id {
        Some(id) => suppliers::update(c, id, &input),
        None => suppliers::create(c, &input),
    })
}

#[tauri::command(async)]
pub fn supplier_delete(core: State<'_, Core>, id: String) -> AppResult<()> {
    check_id(&id)?;
    core.tx(|c| suppliers::delete(c, &id))
}
