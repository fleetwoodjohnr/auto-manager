//! QuickBooks → Auto Manager. Exports are re-read and re-checked on apply, all in one
//! transaction, so what's written matches the data as it is then. QuickBooks Online is read
//! through `quickbooks::online`; its data is fetched for the preview and kept in memory until
//! applied, so the preview and what's applied are the same answer from QuickBooks.

use auto_manager_core::quickbooks::online::{self, Connection, KeySource, Keys, Online, SignIn, Urls};
use auto_manager_core::quickbooks::{self, QbData, QbOptions, QbPreview, QbSummary, files::ExportKind};
use auto_manager_core::secrets::{self, Secrets};
use auto_manager_core::{AppError, AppResult, Core, db};
use serde::Serialize;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, State};

const FILE: &str = "qb-file";

#[derive(Serialize)]
pub struct FilePreview {
    pub kind: ExportKind,
    pub preview: QbPreview,
}

#[tauri::command(async)]
pub fn quickbooks_file_preview(core: State<'_, Core>, path: String, options: QbOptions) -> AppResult<FilePreview> {
    let (kind, data) = quickbooks::files::read_file(&PathBuf::from(path))?;
    let preview = core.read(|c| quickbooks::preview(c, FILE, &data, &options))?;
    Ok(FilePreview { kind, preview })
}

#[tauri::command(async)]
pub fn quickbooks_file_apply(core: State<'_, Core>, path: String, options: QbOptions) -> AppResult<QbSummary> {
    let (_, data) = quickbooks::files::read_file(&PathBuf::from(path))?;
    core.tx(|c| quickbooks::apply(c, FILE, &data, &options))
}

// ---------------------------------------------------------------- QuickBooks Online

/// QuickBooks Online state for this window: the secret store (the app's keys and the
/// sign-in), the client (when there are keys), a sign-in in progress, and the last data read
/// for a preview.
pub struct Qbo {
    secrets: Arc<dyn Secrets>,
    online: Mutex<Option<Arc<Online>>>,
    sign_in: Mutex<Option<Arc<SignIn>>>,
    fetched: Mutex<Option<(String, QbData)>>,
}

impl Default for Qbo {
    fn default() -> Self {
        Qbo { secrets: secrets::store().into(), online: Mutex::default(), sign_in: Mutex::default(), fetched: Mutex::default() }
    }
}

impl Qbo {
    /// The client for the current keys, made when first needed (and again after they change).
    fn online(&self) -> Option<Arc<Online>> {
        let mut slot = self.online.lock().unwrap_or_else(|e| e.into_inner());
        if slot.is_none() {
            *slot = Keys::find(&*self.secrets)
                .map(|k| Arc::new(Online::new(k.clone(), Urls::new(k.production), Box::new(self.secrets.clone()))));
        }
        slot.clone()
    }

    fn client(&self) -> AppResult<Arc<Online>> {
        self.online().ok_or_else(|| AppError::msg("Enter your Intuit app's keys first."))
    }

    /// The keys can't change under a connection: its sign-in belongs to the app that made it.
    fn change_keys(&self, core: &Core, change: impl FnOnce(&dyn Secrets) -> AppResult<()>) -> AppResult<()> {
        if online::load_connection(&core.paths.root).is_some() {
            return Err(AppError::msg("Disconnect from QuickBooks first, then change the keys."));
        }
        change(&*self.secrets)?;
        *self.online.lock().unwrap_or_else(|e| e.into_inner()) = None;
        Ok(())
    }
}

fn system(c: &Connection) -> String {
    format!("qbo:{}", c.realm)
}

fn connection(core: &Core) -> AppResult<Connection> {
    online::load_connection(&core.paths.root).ok_or_else(|| AppError::msg("Connect to QuickBooks Online first."))
}

#[derive(Serialize)]
pub struct QboStatus {
    /// There are Intuit app keys to connect with.
    pub available: bool,
    /// "sandbox" or "production" (the keys' environment)
    pub environment: Option<&'static str>,
    pub source: Option<KeySource>,
    /// Not secret: Intuit shows it in the browser's address. The client secret never leaves.
    pub client_id: Option<String>,
    /// The redirect URI the Intuit app must list.
    pub redirect: String,
    pub connection: Option<Connection>,
    pub signing_in: bool,
}

#[tauri::command(async)]
pub fn quickbooks_status(core: State<'_, Core>, qbo: State<'_, Qbo>) -> AppResult<QboStatus> {
    let client = qbo.online();
    let keys = client.as_ref().map(|c| c.keys());
    Ok(QboStatus {
        available: keys.is_some(),
        environment: keys.map(|k| k.environment()),
        source: keys.map(|k| k.source),
        client_id: keys.map(|k| k.client_id.clone()),
        redirect: Urls::new(false).redirect,
        connection: online::load_connection(&core.paths.root),
        signing_in: qbo.sign_in.lock().unwrap_or_else(|e| e.into_inner()).is_some(),
    })
}

/// Keep the person's own Intuit app keys (in the OS keyring).
#[tauri::command(async)]
pub fn quickbooks_keys_save(
    core: State<'_, Core>,
    qbo: State<'_, Qbo>,
    client_id: String,
    client_secret: String,
    production: bool,
) -> AppResult<()> {
    qbo.change_keys(&core, |s| Keys::save(s, &client_id, &client_secret, production))
}

#[tauri::command(async)]
pub fn quickbooks_keys_forget(core: State<'_, Core>, qbo: State<'_, Qbo>) -> AppResult<()> {
    qbo.change_keys(&core, Keys::forget)
}

/// Open Intuit's sign-in in the browser and wait for its answer (up to 5 minutes).
#[tauri::command]
pub async fn quickbooks_connect(app: AppHandle, core: State<'_, Core>, qbo: State<'_, Qbo>) -> AppResult<Connection> {
    use tauri_plugin_opener::OpenerExt;
    let client = qbo.client()?;
    let keys = client.keys();
    let sign_in = Arc::new(online::begin(keys, &Urls::new(keys.production))?);
    if let Some(old) = qbo.sign_in.lock().unwrap_or_else(|e| e.into_inner()).replace(sign_in.clone()) {
        old.cancel();
    }
    let result = async {
        // Tests answer the sign-in themselves instead of opening a browser.
        if std::env::var_os("AUTO_MANAGER_QBO_NO_BROWSER").is_none() {
            app.opener().open_url(&sign_in.url, None::<&str>).map_err(|e| AppError::Other(format!("Couldn't open the browser: {e}")))?;
        }
        let waiting = sign_in.clone();
        let cb = tauri::async_runtime::spawn_blocking(move || waiting.wait(online::SIGN_IN_TIMEOUT))
            .await
            .map_err(|e| AppError::Other(e.to_string()))??;
        let company = client.connect(&cb, &sign_in.redirect).await?;
        let c = Connection {
            realm: cb.realm,
            company,
            environment: client.environment().into(),
            connected_at: db::now_utc(),
            last_refresh: None,
        };
        online::save_connection(&core.paths.root, &c)?;
        Ok(c)
    }
    .await;
    let mut slot = qbo.sign_in.lock().unwrap_or_else(|e| e.into_inner());
    if slot.as_ref().is_some_and(|s| Arc::ptr_eq(s, &sign_in)) {
        *slot = None;
    }
    result
}

/// The sign-in address, while connecting (for tests, and to show if the browser didn't open).
#[tauri::command]
pub fn quickbooks_sign_in_url(qbo: State<'_, Qbo>) -> Option<String> {
    qbo.sign_in.lock().unwrap_or_else(|e| e.into_inner()).as_ref().map(|s| s.url.clone())
}

/// The address the browser ended on, when it couldn't hand the answer back by itself.
#[tauri::command]
pub fn quickbooks_finish(qbo: State<'_, Qbo>, url: String) -> AppResult<()> {
    let slot = qbo.sign_in.lock().unwrap_or_else(|e| e.into_inner());
    slot.as_ref().ok_or_else(|| AppError::msg("Start connecting first."))?.paste(&url);
    Ok(())
}

#[tauri::command]
pub fn quickbooks_cancel(qbo: State<'_, Qbo>) {
    if let Some(s) = qbo.sign_in.lock().unwrap_or_else(|e| e.into_inner()).take() {
        s.cancel();
    }
}

/// Read everything from QuickBooks Online and show what importing it would do.
#[tauri::command]
pub async fn quickbooks_preview(core: State<'_, Core>, qbo: State<'_, Qbo>, options: QbOptions, reload: bool) -> AppResult<QbPreview> {
    let c = connection(&core)?;
    let cached = qbo.fetched.lock().unwrap_or_else(|e| e.into_inner()).clone().filter(|(realm, _)| !reload && *realm == c.realm);
    let data = match cached {
        Some((_, d)) => d,
        None => {
            let d = qbo.client()?.read_all(&c.realm).await?;
            *qbo.fetched.lock().unwrap_or_else(|e| e.into_inner()) = Some((c.realm.clone(), d.clone()));
            d
        }
    };
    core.read(|conn| quickbooks::preview(conn, &system(&c), &data, &options))
}

/// Apply the data the last preview read.
#[tauri::command]
pub async fn quickbooks_apply(core: State<'_, Core>, qbo: State<'_, Qbo>, options: QbOptions) -> AppResult<QbSummary> {
    let mut c = connection(&core)?;
    let data = qbo.fetched.lock().unwrap_or_else(|e| e.into_inner()).clone().filter(|(realm, _)| *realm == c.realm);
    let (_, data) = data.ok_or_else(|| AppError::msg("Review the QuickBooks data first."))?;
    let summary = core.tx(|conn| quickbooks::apply(conn, &system(&c), &data, &options))?;
    *qbo.fetched.lock().unwrap_or_else(|e| e.into_inner()) = None;
    c.last_refresh = Some(db::now_utc());
    online::save_connection(&core.paths.root, &c)?;
    Ok(summary)
}

/// Sign out at Intuit and forget the sign-in here. Imported records and their links stay, so
/// connecting again updates them.
#[tauri::command]
pub async fn quickbooks_disconnect(core: State<'_, Core>, qbo: State<'_, Qbo>) -> AppResult<Option<String>> {
    let c = connection(&core)?;
    let note = match qbo.online() {
        Some(client) => client.disconnect(&c.realm).await?,
        None => None,
    };
    online::remove_connection(&core.paths.root)?;
    *qbo.fetched.lock().unwrap_or_else(|e| e.into_inner()) = None;
    Ok(note)
}
