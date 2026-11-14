use auto_manager_core::backup::{self, BackupInfo, Manifest};
use auto_manager_core::dashboard::{self, Dashboard};
use auto_manager_core::providers::{self, CacheStats};
use auto_manager_core::search::{self, SearchResult};
use auto_manager_core::settings::{self, Settings};
use auto_manager_core::{AppError, AppResult, Core, db};
use serde::Serialize;
use std::path::PathBuf;
use std::sync::Mutex;
use tauri::ipc::Channel;
use tauri::{AppHandle, State};
use tauri_plugin_updater::UpdaterExt;

pub const REPO: &str = "fleetwoodjohnr/auto-manager";

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

#[tauri::command(async)]
pub fn app_info(core: State<'_, Core>) -> AppResult<AppInfo> {
    Ok(AppInfo {
        version: env!("CARGO_PKG_VERSION").into(),
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

#[derive(Default)]
pub struct Updates {
    found: Mutex<Option<tauri_plugin_updater::Update>>,
    /// A verified download waiting for the person to choose "Restart and install".
    bytes: Mutex<Option<Vec<u8>>>,
}

#[derive(Serialize)]
pub struct UpdateInfo {
    pub current: String,
    pub version: String,
    pub notes: Option<String>,
    pub date: Option<String>,
}

#[derive(Serialize, Clone)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum UpdateProgress {
    Started { total: Option<u64> },
    Progress { downloaded: u64, total: Option<u64> },
    Finished,
}

fn stable_endpoint() -> String {
    format!("https://github.com/{REPO}/releases/latest/download/latest.json")
}

/// The newest release of any kind (stable or pre-release) that publishes a latest.json.
async fn beta_endpoint() -> AppResult<String> {
    #[derive(serde::Deserialize)]
    struct Asset {
        name: String,
    }
    #[derive(serde::Deserialize)]
    struct Release {
        tag_name: String,
        draft: bool,
        assets: Vec<Asset>,
    }
    let client = reqwest::Client::builder()
        .user_agent(concat!("AutoManager/", env!("CARGO_PKG_VERSION")))
        .timeout(std::time::Duration::from_secs(20))
        .build()
        .map_err(|e| AppError::Other(e.to_string()))?;
    let releases: Vec<Release> = client
        .get(format!("https://api.github.com/repos/{REPO}/releases?per_page=30"))
        .header("Accept", "application/vnd.github+json")
        .send()
        .await
        .and_then(|r| r.error_for_status())
        .map_err(|e| AppError::Provider {
            message: format!("Couldn't reach GitHub to check for beta releases ({e})."),
            offline: e.is_connect() || e.is_timeout(),
        })?
        .json()
        .await
        .map_err(|_| AppError::Provider { message: "GitHub sent a release list Auto Manager couldn't read.".into(), offline: false })?;
    let best = releases
        .into_iter()
        .filter(|r| !r.draft && r.assets.iter().any(|a| a.name == "latest.json"))
        .filter_map(|r| semver::Version::parse(r.tag_name.trim_start_matches('v')).ok().map(|v| (v, r.tag_name)))
        .max_by(|a, b| a.0.cmp(&b.0));
    Ok(match best {
        Some((_, tag)) => format!("https://github.com/{REPO}/releases/download/{tag}/latest.json"),
        None => stable_endpoint(),
    })
}

fn update_err(what: &str) -> impl Fn(tauri_plugin_updater::Error) -> AppError + '_ {
    move |e| AppError::Provider { message: format!("{what}: {e}"), offline: false }
}

/// Check the chosen channel. The updater only offers versions newer than this one (no
/// downgrades) and verifies every package against the public key built into the app.
#[tauri::command]
pub async fn update_check(
    app: AppHandle,
    core: State<'_, Core>,
    updates: State<'_, Updates>,
    channel: String,
) -> AppResult<Option<UpdateInfo>> {
    let endpoint = match std::env::var("AUTO_MANAGER_UPDATE_ENDPOINT").ok().filter(|e| !e.is_empty()) {
        Some(e) => e,
        None if channel == "beta" => beta_endpoint().await?,
        None => stable_endpoint(),
    };
    let url = url::Url::parse(&endpoint).map_err(|e| AppError::Other(format!("Bad update address: {e}")))?;
    let err = update_err("Couldn't check for updates");
    let found = app.updater_builder().endpoints(vec![url]).map_err(&err)?.build().map_err(&err)?.check().await.map_err(&err)?;
    core.tx(|c| {
        let mut s = settings::load(c);
        s.last_update_check = Some(db::now_utc());
        settings::save(c, &s).map(|_| ())
    })?;
    let info = found.as_ref().map(|u| UpdateInfo {
        current: u.current_version.clone(),
        version: u.version.clone(),
        notes: u.body.clone(),
        date: u.date.map(|d| d.to_string()),
    });
    *updates.found.lock().unwrap_or_else(|e| e.into_inner()) = found;
    *updates.bytes.lock().unwrap_or_else(|e| e.into_inner()) = None;
    Ok(info)
}

/// Download the update found by `update_check` and verify its signature. Nothing is installed.
#[tauri::command]
pub async fn update_download(updates: State<'_, Updates>, on_progress: Channel<UpdateProgress>) -> AppResult<()> {
    let update =
        updates.found.lock().unwrap_or_else(|e| e.into_inner()).clone().ok_or_else(|| AppError::msg("Check for updates first."))?;
    let mut downloaded: u64 = 0;
    let mut started = false;
    let bytes = update
        .download(
            |chunk, total| {
                if !started {
                    started = true;
                    let _ = on_progress.send(UpdateProgress::Started { total });
                }
                downloaded += chunk as u64;
                let _ = on_progress.send(UpdateProgress::Progress { downloaded, total });
            },
            || {
                let _ = on_progress.send(UpdateProgress::Finished);
            },
        )
        .await
        .map_err(update_err("The update couldn't be downloaded or failed verification. Nothing was changed"))?;
    *updates.bytes.lock().unwrap_or_else(|e| e.into_inner()) = Some(bytes);
    Ok(())
}

/// Back up the data, then install the downloaded update. The UI relaunches afterwards (on
/// Windows the installer closes the app itself).
#[tauri::command]
pub async fn update_install(core: State<'_, Core>, updates: State<'_, Updates>) -> AppResult<()> {
    let update =
        updates.found.lock().unwrap_or_else(|e| e.into_inner()).clone().ok_or_else(|| AppError::msg("Check for updates first."))?;
    let bytes =
        updates.bytes.lock().unwrap_or_else(|e| e.into_inner()).take().ok_or_else(|| AppError::msg("Download the update first."))?;
    let paths = core.paths.clone();
    core.read(|c| backup::create(c, &paths, "pre-update").map(|_| ()))?;
    update.install(bytes).map_err(update_err("The update couldn't be installed. Nothing was changed"))
}

#[tauri::command(async)]
pub fn export_json(core: State<'_, Core>, path: String) -> AppResult<()> {
    let out = core.output_path(&path, "json")?;
    core.read(|c| auto_manager_core::export::write_json(c, &out))
}
