//! QuickBooks Online, read only.
//!
//! Sign-in is OAuth 2.0 in the person's browser. Intuit only redirects live companies to an
//! HTTPS address, so it sends the browser to a static page (site/quickbooks/index.html on GitHub
//! Pages) that forwards the answer to a one-time listener on 127.0.0.1. The port travels in the
//! `state` value, with 48 random hex characters that must come back unchanged. If the browser
//! can't reach the listener, the person can paste the address it ended on.
//!
//! The refresh token is kept in the OS keyring and replaced after every refresh (Intuit rotates
//! it); the access token only lives in memory. Only GET requests reach the QuickBooks API.

use super::{QbCustomer, QbData, QbItem, QbVendor};
use crate::error::{AppError, AppResult};
use crate::providers::http::{Auth, Body, Http};
use crate::qty::Qty;
use crate::secrets::Secrets;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};
use ts_rs::TS;

const WHO: &str = "QuickBooks";
pub const SCOPE: &str = "com.intuit.quickbooks.accounting";
const PAGE: usize = 1000;
const MINOR_VERSION: &str = "75";
pub const SIGN_IN_TIMEOUT: Duration = Duration::from_secs(5 * 60);

/// The Intuit app's keys: from the environment when set (development), otherwise built into
/// this copy of the app by the release workflow. None when this build has no keys.
#[derive(Clone)]
pub struct Keys {
    pub client_id: String,
    pub client_secret: String,
    pub production: bool,
}

impl Keys {
    pub fn get() -> Option<Keys> {
        let pick = |run: &str, built: Option<&'static str>| {
            std::env::var(run).ok().filter(|v| !v.trim().is_empty()).or(built.map(str::to_string)).filter(|v| !v.trim().is_empty())
        };
        Some(Keys {
            client_id: pick("AUTO_MANAGER_QBO_CLIENT_ID", option_env!("AUTO_MANAGER_QBO_CLIENT_ID"))?,
            client_secret: pick("AUTO_MANAGER_QBO_CLIENT_SECRET", option_env!("AUTO_MANAGER_QBO_CLIENT_SECRET"))?,
            production: pick("AUTO_MANAGER_QBO_ENV", option_env!("AUTO_MANAGER_QBO_ENV")).as_deref() == Some("production"),
        })
    }

    pub fn environment(&self) -> &'static str {
        if self.production { "production" } else { "sandbox" }
    }
}

/// Intuit's addresses. Each can be overridden for tests (`AUTO_MANAGER_QBO_*`).
pub struct Urls {
    pub auth: String,
    pub token: String,
    pub revoke: String,
    pub api: String,
    /// Where Intuit sends the browser; `{port}` is replaced by the listener's port.
    pub redirect: String,
}

impl Urls {
    pub fn new(production: bool) -> Urls {
        let get = |k: &str, d: &str| std::env::var(k).ok().filter(|v| !v.is_empty()).unwrap_or_else(|| d.to_string());
        Urls {
            auth: get("AUTO_MANAGER_QBO_AUTH_URL", "https://appcenter.intuit.com/connect/oauth2"),
            token: get("AUTO_MANAGER_QBO_TOKEN_URL", "https://oauth.platform.intuit.com/oauth2/v1/tokens/bearer"),
            revoke: get("AUTO_MANAGER_QBO_REVOKE_URL", "https://developer.api.intuit.com/v2/oauth2/tokens/revoke"),
            api: get(
                "AUTO_MANAGER_QBO_API_BASE",
                if production { "https://quickbooks.api.intuit.com" } else { "https://sandbox-quickbooks.api.intuit.com" },
            ),
            redirect: get("AUTO_MANAGER_QBO_REDIRECT", "https://fleetwoodjohnr.github.io/auto-manager/quickbooks/"),
        }
    }
}

// ---------------------------------------------------------------- sign-in

/// What Intuit sends back: a one-time code and the company's id.
#[derive(Debug, Clone, PartialEq)]
pub struct Callback {
    pub code: String,
    pub realm: String,
}

pub struct SignIn {
    listener: TcpListener,
    state: String,
    /// The browser address to open.
    pub url: String,
    /// The redirect address this sign-in used (the token request must repeat it).
    pub redirect: String,
    pasted: Mutex<Option<String>>,
    cancelled: AtomicBool,
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn msg(s: &str) -> AppError {
    AppError::msg(s)
}

pub fn begin(keys: &Keys, urls: &Urls) -> AppResult<SignIn> {
    let listener = TcpListener::bind("127.0.0.1:0")?;
    listener.set_nonblocking(true)?;
    let port = listener.local_addr()?.port();
    let mut random = [0u8; 24];
    getrandom::fill(&mut random).map_err(|e| AppError::Other(format!("No secure random numbers available: {e}")))?;
    let state = format!("{port}.{}", hex(&random));
    let redirect = urls.redirect.replace("{port}", &port.to_string());
    let url = reqwest::Url::parse_with_params(
        &urls.auth,
        &[
            ("client_id", keys.client_id.as_str()),
            ("response_type", "code"),
            ("scope", SCOPE),
            ("redirect_uri", &redirect),
            ("state", &state),
        ],
    )
    .map_err(|e| AppError::Other(format!("Bad QuickBooks sign-in address: {e}")))?
    .to_string();
    Ok(SignIn { listener, state, url, redirect, pasted: Mutex::new(None), cancelled: AtomicBool::new(false) })
}

fn valid_realm(realm: &str) -> bool {
    !realm.is_empty() && realm.len() <= 32 && realm.bytes().all(|b| b.is_ascii_digit())
}

impl SignIn {
    /// The address the browser ended on, for when it couldn't reach this computer.
    pub fn paste(&self, url: &str) {
        *self.pasted.lock().unwrap_or_else(|e| e.into_inner()) = Some(url.to_string());
    }

    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::SeqCst);
    }

    fn parse(&self, url: &str) -> AppResult<Callback> {
        let u = reqwest::Url::parse(url.trim()).map_err(|_| msg("That isn't the address QuickBooks sent your browser to."))?;
        let q: HashMap<String, String> = u.query_pairs().into_owned().collect();
        if q.get("state") != Some(&self.state) {
            return Err(msg("That answer isn't from this sign-in. Start connecting again."));
        }
        if let Some(e) = q.get("error") {
            return Err(if e == "access_denied" {
                msg("QuickBooks access wasn't allowed.")
            } else {
                AppError::msg(format!("QuickBooks didn't connect ({e})."))
            });
        }
        match (q.get("code"), q.get("realmId")) {
            (Some(code), Some(realm)) if !code.is_empty() && valid_realm(realm) => {
                Ok(Callback { code: code.clone(), realm: realm.clone() })
            }
            _ => Err(msg("QuickBooks' answer is missing the company. Start connecting again.")),
        }
    }

    /// Read one request from the browser. `None` for anything that isn't the callback.
    fn answer(&self, mut stream: TcpStream) -> AppResult<Option<AppResult<Callback>>> {
        stream.set_nonblocking(false)?;
        stream.set_read_timeout(Some(Duration::from_secs(5)))?;
        let mut buf = Vec::new();
        let mut chunk = [0u8; 2048];
        while !buf.windows(4).any(|w| w == b"\r\n\r\n") && buf.len() < 16 * 1024 {
            match stream.read(&mut chunk) {
                Ok(0) | Err(_) => break,
                Ok(n) => buf.extend_from_slice(&chunk[..n]),
            }
        }
        let head = String::from_utf8_lossy(&buf);
        let mut words = head.lines().next().unwrap_or("").split(' ');
        let (method, target) = (words.next().unwrap_or(""), words.next().unwrap_or(""));
        if method != "GET" || !target.starts_with("/quickbooks/callback") {
            let _ = stream.write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
            return Ok(None);
        }
        let result = self.parse(&format!("http://127.0.0.1{target}"));
        let (title, text) = match &result {
            Ok(_) => ("Connected".to_string(), "Auto Manager is connected to QuickBooks. You can close this tab.".to_string()),
            Err(e) => ("Not connected".to_string(), e.to_string()),
        };
        let esc = |s: &str| s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;");
        let page = format!(
            "<!doctype html><meta charset=utf-8><title>{t}</title><body style=\"font:16px system-ui;margin:3em\"><h1>{t}</h1><p>{x}</p>",
            t = esc(&title),
            x = esc(&text)
        );
        let _ = stream.write_all(
            format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Security-Policy: default-src 'none'; style-src 'unsafe-inline'\r\nCache-Control: no-store\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{page}",
                page.len()
            )
            .as_bytes(),
        );
        Ok(Some(result))
    }

    /// Wait for Intuit's answer, from the browser or pasted. Blocks: run it off the async runtime.
    pub fn wait(&self, timeout: Duration) -> AppResult<Callback> {
        let deadline = Instant::now() + timeout;
        loop {
            if self.cancelled.load(Ordering::SeqCst) {
                return Err(msg("Connecting to QuickBooks was cancelled."));
            }
            if let Some(u) = self.pasted.lock().unwrap_or_else(|e| e.into_inner()).take() {
                return self.parse(&u);
            }
            match self.listener.accept() {
                Ok((stream, _)) => {
                    if let Some(result) = self.answer(stream)? {
                        return result;
                    }
                }
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {}
                Err(e) => return Err(e.into()),
            }
            if Instant::now() > deadline {
                return Err(msg("QuickBooks didn't answer within 5 minutes. Start connecting again."));
            }
            std::thread::sleep(Duration::from_millis(100));
        }
    }
}

// ---------------------------------------------------------------- the device-local connection

#[derive(Serialize, Deserialize, TS, Clone, Debug, PartialEq)]
#[ts(export)]
pub struct Connection {
    pub realm: String,
    pub company: String,
    /// "sandbox" or "production"
    pub environment: String,
    pub connected_at: String,
    pub last_refresh: Option<String>,
}

fn connection_file(root: &Path) -> PathBuf {
    root.join("quickbooks.json")
}

pub fn load_connection(root: &Path) -> Option<Connection> {
    std::fs::read(connection_file(root)).ok().and_then(|b| serde_json::from_slice(&b).ok())
}

pub fn save_connection(root: &Path, c: &Connection) -> AppResult<()> {
    let tmp = connection_file(root).with_extension("json.partial");
    std::fs::write(&tmp, serde_json::to_vec_pretty(c)?)?;
    std::fs::rename(tmp, connection_file(root))?;
    Ok(())
}

pub fn remove_connection(root: &Path) -> AppResult<()> {
    match std::fs::remove_file(connection_file(root)) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e.into()),
        _ => Ok(()),
    }
}

// ---------------------------------------------------------------- the API

struct Session {
    realm: String,
    access: String,
    until: Instant,
}

#[derive(Deserialize)]
struct TokenReply {
    access_token: String,
    refresh_token: String,
    expires_in: u64,
}

pub struct Online {
    http: Http,
    keys: Keys,
    urls: Urls,
    secrets: Box<dyn Secrets>,
    session: tokio::sync::Mutex<Option<Session>>,
}

fn bad_answer() -> AppError {
    AppError::Provider { message: "QuickBooks sent an answer Auto Manager couldn't read.".into(), offline: false }
}

fn reconnect() -> AppError {
    AppError::Provider {
        message: "QuickBooks needs you to connect again (the sign-in expired or was disconnected in QuickBooks).".into(),
        offline: false,
    }
}

/// Intuit's error text ({"Fault":{"Error":[{"Message","Detail"}]}}), or the status.
fn fault(status: u16, body: &[u8]) -> AppError {
    let v: Value = serde_json::from_slice(body).unwrap_or(Value::Null);
    let e = &v["Fault"]["Error"][0];
    let text = e["Detail"].as_str().or(e["Message"].as_str()).map(str::to_string).unwrap_or_else(|| format!("error {status}"));
    AppError::Provider { message: format!("QuickBooks: {text}"), offline: false }
}

impl Online {
    pub fn new(keys: Keys, urls: Urls, secrets: Box<dyn Secrets>) -> Online {
        Online { http: Http::new(), keys, urls, secrets, session: tokio::sync::Mutex::new(None) }
    }

    pub fn environment(&self) -> &'static str {
        self.keys.environment()
    }

    fn secret_id(&self, realm: &str) -> String {
        format!("quickbooks:{}:{realm}", self.keys.environment())
    }

    async fn token(&self, form: &[(&str, &str)]) -> AppResult<TokenReply> {
        let auth = Auth::Basic(&self.keys.client_id, &self.keys.client_secret);
        let r = self.http.send(&self.urls.token, Body::Form(form), auth, Some("application/json"), WHO).await?;
        if r.status == 200 {
            return serde_json::from_slice(&r.body).map_err(|_| bad_answer());
        }
        let v: Value = serde_json::from_slice(&r.body).unwrap_or(Value::Null);
        match v["error"].as_str() {
            Some("invalid_grant") => Err(reconnect()),
            Some("invalid_client") => {
                Err(AppError::Provider { message: "QuickBooks didn't accept this app's keys.".into(), offline: false })
            }
            _ => Err(AppError::Provider { message: format!("QuickBooks sign-in failed (error {}).", r.status), offline: false }),
        }
    }

    async fn keep(&self, realm: &str, t: TokenReply) -> AppResult<()> {
        // Intuit may rotate the refresh token on any refresh: keep the newest before using it.
        self.secrets.set(&self.secret_id(realm), &t.refresh_token)?;
        *self.session.lock().await = Some(Session {
            realm: realm.into(),
            access: t.access_token,
            until: Instant::now() + Duration::from_secs(t.expires_in.saturating_sub(60)),
        });
        Ok(())
    }

    /// Finish signing in with Intuit's answer. Returns the company's name.
    pub async fn connect(&self, cb: &Callback, redirect: &str) -> AppResult<String> {
        let t = self.token(&[("grant_type", "authorization_code"), ("code", &cb.code), ("redirect_uri", redirect)]).await?;
        self.keep(&cb.realm, t).await?;
        self.company_name(&cb.realm).await
    }

    async fn refresh(&self, realm: &str) -> AppResult<()> {
        let stored = self.secrets.get(&self.secret_id(realm))?.ok_or_else(reconnect)?;
        let t = self.token(&[("grant_type", "refresh_token"), ("refresh_token", &stored)]).await?;
        self.keep(realm, t).await
    }

    async fn access(&self, realm: &str) -> AppResult<String> {
        {
            let s = self.session.lock().await;
            if let Some(s) = s.as_ref().filter(|s| s.realm == realm && Instant::now() < s.until) {
                return Ok(s.access.clone());
            }
        }
        self.refresh(realm).await?;
        Ok(self.session.lock().await.as_ref().map(|s| s.access.clone()).unwrap_or_default())
    }

    /// GET a path under the company. An expired access token is refreshed once.
    async fn get(&self, realm: &str, path: &str, params: &[(&str, &str)]) -> AppResult<Value> {
        if !valid_realm(realm) {
            return Err(msg("That isn't a QuickBooks company id."));
        }
        let mut all: Vec<(&str, &str)> = params.to_vec();
        all.push(("minorversion", MINOR_VERSION));
        let url = reqwest::Url::parse_with_params(&format!("{}/v3/company/{realm}/{path}", self.urls.api), &all)
            .map_err(|e| AppError::Other(format!("Bad QuickBooks address: {e}")))?
            .to_string();
        for attempt in 0..2 {
            let token = self.access(realm).await?;
            let r = self.http.send(&url, Body::None, Auth::Bearer(&token), Some("application/json"), WHO).await?;
            match r.status {
                200 => return serde_json::from_slice(&r.body).map_err(|_| bad_answer()),
                401 if attempt == 0 => {
                    *self.session.lock().await = None;
                }
                401 => return Err(reconnect()),
                429 => {
                    return Err(AppError::Provider {
                        message: "QuickBooks is limiting requests right now. Try again in a minute.".into(),
                        offline: false,
                    });
                }
                s => return Err(fault(s, &r.body)),
            }
        }
        Err(reconnect())
    }

    pub async fn company_name(&self, realm: &str) -> AppResult<String> {
        let v = self.get(realm, &format!("companyinfo/{realm}"), &[]).await?;
        Ok(v["CompanyInfo"]["CompanyName"].as_str().unwrap_or("QuickBooks company").to_string())
    }

    /// Every record of one entity, inactive ones included, 1,000 at a time.
    async fn query(&self, realm: &str, entity: &str) -> AppResult<Vec<Value>> {
        let mut out = Vec::new();
        loop {
            let q = format!("select * from {entity} where Active in (true, false) startposition {} maxresults {PAGE}", out.len() + 1);
            let v = self.get(realm, "query", &[("query", &q)]).await?;
            let page = v["QueryResponse"][entity].as_array().cloned().unwrap_or_default();
            let n = page.len();
            out.extend(page);
            if n < PAGE {
                return Ok(out);
            }
        }
    }

    // ponytail: reads everything each time; use Intuit's change data capture if catalogs get large.
    pub async fn read_all(&self, realm: &str) -> AppResult<QbData> {
        Ok(QbData {
            vendors: self.query(realm, "Vendor").await?.iter().map(vendor).collect(),
            customers: self.query(realm, "Customer").await?.iter().map(customer).collect(),
            items: self.query(realm, "Item").await?.iter().filter_map(item).collect(),
            problems: vec![],
        })
    }

    /// Revoke the sign-in at Intuit (best effort) and forget it here.
    pub async fn disconnect(&self, realm: &str) -> AppResult<Option<String>> {
        let id = self.secret_id(realm);
        let note = match self.secrets.get(&id)? {
            Some(token) => {
                let body = serde_json::json!({ "token": token });
                let auth = Auth::Basic(&self.keys.client_id, &self.keys.client_secret);
                match self.http.send(&self.urls.revoke, Body::Json(&body), auth, Some("application/json"), WHO).await {
                    Ok(r) if r.status == 200 => None,
                    _ => Some("QuickBooks couldn't be told; remove Auto Manager under Apps in QuickBooks to be sure.".to_string()),
                }
            }
            None => None,
        };
        self.secrets.delete(&id)?;
        *self.session.lock().await = None;
        Ok(note)
    }
}

// ---------------------------------------------------------------- mapping

fn text(v: &Value) -> String {
    v.as_str().map(|s| s.trim().to_string()).unwrap_or_default()
}

fn number(v: &Value) -> Option<Decimal> {
    match v {
        Value::Number(n) => Decimal::from_str(&n.to_string()).ok().or_else(|| n.as_f64().and_then(|f| Decimal::try_from(f).ok())),
        Value::String(s) => Decimal::from_str(s.trim()).ok(),
        _ => None,
    }
    .map(|d| d.normalize())
}

fn qty(v: &Value) -> Option<Qty> {
    number(v).and_then(|d| Qty::from_decimal(d.round_dp(3)))
}

fn active(v: &Value) -> bool {
    v["Active"].as_bool().unwrap_or(true)
}

/// None for QuickBooks categories, which are folders rather than products.
fn item(v: &Value) -> Option<QbItem> {
    let kind = match v["Type"].as_str().unwrap_or("") {
        "Category" => return None,
        "Inventory" => "inventory",
        "NonInventory" => "non_inventory",
        "Service" => "service",
        _ => "other",
    };
    let name = text(&v["Name"]);
    let full = text(&v["FullyQualifiedName"]);
    let category = full.strip_suffix(&name).map(|p| p.trim_end_matches(':').to_string()).unwrap_or_default();
    Some(QbItem {
        external_id: text(&v["Id"]),
        name,
        category,
        sku: text(&v["Sku"]),
        kind: kind.into(),
        description: text(&v["Description"]),
        price: number(&v["UnitPrice"]).map(|d| d.to_string()),
        cost: number(&v["PurchaseCost"]).map(|d| d.to_string()),
        qty: qty(&v["QtyOnHand"]),
        reorder_point: qty(&v["ReorderPoint"]),
        vendor: text(&v["PrefVendorRef"]["name"]),
        active: active(v),
    })
}

fn address(a: &Value) -> String {
    let mut parts: Vec<String> = ["Line1", "Line2", "Line3", "Line4", "Line5", "City", "CountrySubDivisionCode", "PostalCode", "Country"]
        .iter()
        .map(|k| text(&a[*k]))
        .filter(|s| !s.is_empty())
        .collect();
    parts.dedup();
    parts.join(", ")
}

fn customer(v: &Value) -> QbCustomer {
    QbCustomer {
        external_id: text(&v["Id"]),
        name: text(&v["DisplayName"]),
        phone: text(&v["PrimaryPhone"]["FreeFormNumber"]),
        email: text(&v["PrimaryEmailAddr"]["Address"]),
        address: address(&v["BillAddr"]),
        notes: text(&v["Notes"]),
        active: active(v),
    }
}

fn vendor(v: &Value) -> QbVendor {
    let contact = [text(&v["GivenName"]), text(&v["FamilyName"])].into_iter().filter(|s| !s.is_empty()).collect::<Vec<_>>().join(" ");
    QbVendor {
        external_id: text(&v["Id"]),
        name: text(&v["DisplayName"]),
        contact,
        phone: text(&v["PrimaryPhone"]["FreeFormNumber"]),
        email: text(&v["PrimaryEmailAddr"]["Address"]),
        website: text(&v["WebAddr"]["URI"]),
        account_no: text(&v["AcctNum"]),
        active: active(v),
    }
}

#[cfg(test)]
#[path = "online_tests.rs"]
mod tests;
