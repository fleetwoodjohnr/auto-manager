use auto_manager_core::backup::{self, BackupInfo, Manifest};
use auto_manager_core::dashboard::{self, Dashboard};
use auto_manager_core::providers::{self, CacheStats};
use auto_manager_core::search::{self, SearchResult};
use auto_manager_core::settings::{self, Settings};
use auto_manager_core::{AppError, AppResult, Core, db};
use serde::Serialize;
use std::path::PathBuf;
use tauri::{AppHandle, State};

#[derive(Serialize)]
pub struct AppInfo {
    pub version: String,
    pub schema_version: i64,
    pub data_dir: String,
    pub log_dir: String,
    pub os: &'static str,
    pub arch: &'static str,
    /// How this copy was installed (appimage, deb, rpm, nsis, msi, app), if known.
    pub bundle: Option<String>,
    pub debug: bool,
}

/// The version is the app's (tauri.conf.json), the one the updater compares against.
#[tauri::command(async)]
pub fn app_info(app: AppHandle, core: State<'_, Core>) -> AppResult<AppInfo> {
    Ok(AppInfo {
        version: app.package_info().version.to_string(),
        schema_version: core.read(db::user_version)?,
        data_dir: core.paths.root.to_string_lossy().into(),
        log_dir: core.paths.logs().to_string_lossy().into(),
        os: std::env::consts::OS,
        arch: std::env::consts::ARCH,
        bundle: tauri::utils::platform::bundle_type().map(|b| format!("{b:?}").to_lowercase()),
        debug: cfg!(debug_assertions),
    })
}

#[tauri::command(async)]
pub fn settings_get(core: State<'_, Core>) -> AppResult<Settings> {
    core.read(|c| Ok(settings::load(c)))
}

#[tauri::command(async)]
pub fn settings_save(core: State<'_, Core>, input: Settings) -> AppResult<Settings> {
    core.tx(|c| settings::save(c, &input))
}

#[tauri::command(async)]
pub fn dashboard(core: State<'_, Core>) -> AppResult<Dashboard> {
    core.read(dashboard::summary)
}

#[tauri::command(async)]
pub fn search(core: State<'_, Core>, query: String, kinds: Option<Vec<String>>, limit: Option<i64>) -> AppResult<SearchResult> {
    if query.len() > 200 {
        return Err(AppError::invalid("query", "That search is too long."));
    }
    core.read(|c| search::search(c, &query, kinds.as_deref(), limit.unwrap_or(20).clamp(1, 100)))
}

#[tauri::command(async)]
pub fn remember_search(core: State<'_, Core>, query: String) -> AppResult<()> {
    core.tx(|c| settings::remember_search(c, &query))
}

#[derive(Serialize)]
pub struct Source {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub priority: i64,
    pub url: Option<String>,
    pub license: Option<String>,
    pub enabled: bool,
    pub records: i64,
}

#[tauri::command(async)]
pub fn data_sources(core: State<'_, Core>) -> AppResult<Vec<Source>> {
    core.read(|c| {
        let mut st = c.prepare(
            "SELECT d.id, d.name, d.kind, d.priority, d.url, d.license, d.enabled,
               (SELECT count(*) FROM vehicle_specifications s WHERE s.source_id = d.id) + (SELECT count(*) FROM part_fitments f WHERE f.source_id = d.id)
             FROM data_sources d ORDER BY d.priority, d.name",
        )?;
        let rows = st.query_map([], |r| {
            Ok(Source { id: r.get(0)?, name: r.get(1)?, kind: r.get(2)?, priority: r.get(3)?, url: r.get(4)?, license: r.get(5)?, enabled: r.get::<_, i64>(6)? == 1, records: r.get(7)? })
        })?;
        Ok(rows.collect::<Result<_, _>>()?)
    })
}

/// Change a provider or import's priority (lower wins when sources disagree). The person's
/// own entries always stay first.
#[tauri::command(async)]
pub fn data_source_priority(core: State<'_, Core>, id: String, priority: i64) -> AppResult<()> {
    if id == "user" {
        return Err(AppError::msg("Your own entries always take priority."));
    }
    if !(1..=1000).contains(&priority) {
        return Err(AppError::invalid("priority", "Priority is between 1 and 1000."));
    }
    core.tx(|c| providers::set_priority(c, &id, priority))
}

#[tauri::command(async)]
pub fn cache_stats(core: State<'_, Core>) -> AppResult<CacheStats> {
    core.read(providers::cache_stats)
}

#[tauri::command(async)]
pub fn cache_clear(core: State<'_, Core>) -> AppResult<usize> {
    core.tx(providers::clear_cache)
}

#[tauri::command(async)]
pub fn backup_list(core: State<'_, Core>) -> AppResult<Vec<BackupInfo>> {
    backup::list(&core.paths.backups())
}

/// A backup into the app's backups folder now.
#[tauri::command(async)]
pub fn backup_now(core: State<'_, Core>) -> AppResult<String> {
    let paths = core.paths.clone();
    core.read(|c| backup::create(c, &paths, "manual")).map(|p| p.to_string_lossy().to_string())
}

/// A backup to a file the person chose (e.g. a USB drive).
#[tauri::command(async)]
pub fn backup_to(core: State<'_, Core>, path: String) -> AppResult<Manifest> {
    let out = core.output_path(&path, "zip")?;
    let paths = core.paths.clone();
    core.read(|c| backup::create_at(c, &paths, &out, "manual"))
}

#[tauri::command(async)]
pub fn backup_inspect(path: String) -> AppResult<Manifest> {
    backup::read_manifest(&PathBuf::from(path))
}

#[tauri::command(async)]
pub fn backup_restore(core: State<'_, Core>, path: String) -> AppResult<Manifest> {
    core.restore(&PathBuf::from(path))
}

#[tauri::command(async)]
pub fn integrity_check(core: State<'_, Core>) -> AppResult<String> {
    core.read(|c| {
        let ok: String = c.query_row("PRAGMA integrity_check", [], |r| r.get(0))?;
        let fk: i64 = c.query_row("SELECT count(*) FROM pragma_foreign_key_check", [], |r| r.get(0))?;
        Ok(if ok == "ok" && fk == 0 { "ok".into() } else { format!("{ok}; {fk} broken references") })
    })
}

// ---------------------------------------------------------------- updates

/// Back up the data before the updater installs (the webview checks, downloads and verifies
/// through the updater plugin, as in the other Simple Suite apps).
#[tauri::command(async)]
pub fn update_prepare(core: State<'_, Core>) -> AppResult<()> {
    let paths = core.paths.clone();
    core.read(|c| backup::create(c, &paths, "pre-update").map(|_| ()))
}

#[tauri::command(async)]
pub fn export_json(core: State<'_, Core>, path: String) -> AppResult<()> {
    let out = core.output_path(&path, "json")?;
    core.read(|c| auto_manager_core::export::write_json(c, &out))
}
