//! The one HTTP client every provider uses: HTTPS only, bounded timeouts, a minimum interval
//! between requests to the same host, and retries with backoff on 429/5xx (honouring
//! Retry-After). Nothing here logs request bodies or credentials.

use crate::error::{AppError, AppResult};
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

const ATTEMPTS: u32 = 3;
const MIN_INTERVAL: Duration = Duration::from_millis(250);
const MAX_RETRY_WAIT: Duration = Duration::from_secs(10);
const MAX_BODY: usize = 20 * 1024 * 1024;

pub struct Http {
    client: reqwest::Client,
    last: Mutex<HashMap<String, Instant>>,
    allow_http: bool,
}

impl Default for Http {
    fn default() -> Self {
        Self::new()
    }
}

impl Http {
    pub fn new() -> Self {
        let client = reqwest::Client::builder()
            .user_agent(concat!("AutoManager/", env!("CARGO_PKG_VERSION"), " (+https://github.com/fleetwoodjohnr/auto-manager)"))
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(30))
            .redirect(reqwest::redirect::Policy::limited(3))
            .build()
            .unwrap_or_default();
        // Plain http is only for local test servers (E2E fixtures), never for real providers.
        let allow_http = std::env::var_os("AUTO_MANAGER_ALLOW_HTTP_LOCALHOST").is_some() || cfg!(test);
        Http { client, last: Mutex::new(HashMap::new()), allow_http }
    }

    fn check_url(&self, url: &reqwest::Url) -> AppResult<()> {
        let local = matches!(url.host_str(), Some("127.0.0.1") | Some("localhost"));
        if url.scheme() == "https" || (self.allow_http && local && url.scheme() == "http") {
            Ok(())
        } else {
            Err(AppError::Other(format!("Refusing a non-HTTPS request to {}.", url.host_str().unwrap_or("?"))))
        }
    }

    async fn throttle(&self, host: &str) {
        let wait = {
            let mut last = self.last.lock().unwrap_or_else(|e| e.into_inner());
            let now = Instant::now();
            let next = last.get(host).map(|t| *t + MIN_INTERVAL).unwrap_or(now);
            let start = next.max(now);
            last.insert(host.to_string(), start);
            start - now
        };
        if !wait.is_zero() {
            tokio::time::sleep(wait).await;
        }
    }

    /// GET a URL and return its body text. `who` names the provider in error messages.
    pub async fn get_text(&self, url: &str, who: &str) -> AppResult<String> {
        text(self.request(url, None, None, who).await?, who)
    }

    /// GET a URL and return its body bytes (documents such as PDFs).
    pub async fn get_bytes(&self, url: &str, who: &str) -> AppResult<Vec<u8>> {
        self.request(url, None, None, who).await
    }

    /// GET with an Accept header (some services pick JSON or XML from it).
    pub async fn get_accept(&self, url: &str, accept: &str, who: &str) -> AppResult<String> {
        text(self.request(url, None, Some(accept), who).await?, who)
    }

    /// POST a form (application/x-www-form-urlencoded) and return the body text.
    pub async fn post_form(&self, url: &str, form: &[(&str, &str)], who: &str) -> AppResult<String> {
        text(self.request(url, Some(form), None, who).await?, who)
    }

    async fn request(&self, url: &str, form: Option<&[(&str, &str)]>, accept: Option<&str>, who: &str) -> AppResult<Vec<u8>> {
        let parsed = reqwest::Url::parse(url).map_err(|e| AppError::Other(format!("Bad provider URL: {e}")))?;
        self.check_url(&parsed)?;
        let host = parsed.host_str().unwrap_or_default().to_string();
        let mut attempt = 0;
        loop {
            attempt += 1;
            self.throttle(&host).await;
            let mut req = match form {
                Some(f) => self.client.post(parsed.clone()).form(f),
                None => self.client.get(parsed.clone()),
            };
            if let Some(a) = accept {
                req = req.header(reqwest::header::ACCEPT, a);
            }
            let result = req.send().await;
            let retry_after = match result {
                Ok(resp) => {
                    let status = resp.status();
                    if status.is_success() {
                        let bytes = resp.bytes().await.map_err(|e| net_error(who, &e))?;
                        if bytes.len() > MAX_BODY {
                            return Err(AppError::Provider {
                                message: format!("{who} sent an unexpectedly large response."),
                                offline: false,
                            });
                        }
                        return Ok(bytes.to_vec());
                    }
                    let retryable = status.as_u16() == 429 || status.is_server_error();
                    if !retryable || attempt >= ATTEMPTS {
                        let message = if status.as_u16() == 429 {
                            format!("{who} is limiting requests right now. Try again in a few minutes.")
                        } else {
                            format!("{who} answered with an error ({status}).")
                        };
                        return Err(AppError::Provider { message, offline: false });
                    }
                    resp.headers()
                        .get(reqwest::header::RETRY_AFTER)
                        .and_then(|v| v.to_str().ok())
                        .and_then(|v| v.trim().parse::<u64>().ok())
                        .map(Duration::from_secs)
                }
                Err(e) => {
                    let retryable = e.is_timeout() || e.is_connect() || e.is_request();
                    if !retryable || attempt >= ATTEMPTS {
                        return Err(net_error(who, &e));
                    }
                    None
                }
            };
            let backoff = Duration::from_millis(500 * 2u64.pow(attempt - 1));
            tokio::time::sleep(retry_after.unwrap_or(backoff).min(MAX_RETRY_WAIT)).await;
        }
    }

    pub async fn get_json(&self, url: &str, who: &str) -> AppResult<serde_json::Value> {
        let text = self.get_text(url, who).await?;
        serde_json::from_str(&text)
            .map_err(|_| AppError::Provider { message: format!("{who} sent a response Auto Manager couldn't read."), offline: false })
    }
}

fn text(bytes: Vec<u8>, who: &str) -> AppResult<String> {
    String::from_utf8(bytes).map_err(|_| AppError::Provider { message: format!("{who} sent a response that isn't text."), offline: false })
}

fn net_error(who: &str, e: &reqwest::Error) -> AppError {
    if e.is_connect() || e.is_timeout() {
        AppError::Provider {
            message: format!("Couldn't reach {who}. Check your internet connection; saved data still works offline."),
            offline: true,
        }
    } else {
        AppError::Provider { message: format!("{who} couldn't be reached ({e})."), offline: false }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::path;
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[tokio::test]
    async fn retries_429_then_succeeds() {
        let server = MockServer::start().await;
        Mock::given(path("/x"))
            .respond_with(ResponseTemplate::new(429).insert_header("Retry-After", "0"))
            .up_to_n_times(1)
            .mount(&server)
            .await;
        Mock::given(path("/x")).respond_with(ResponseTemplate::new(200).set_body_string("{\"ok\":true}")).mount(&server).await;
        let v = Http::new().get_json(&format!("{}/x", server.uri()), "Test").await.unwrap();
        assert_eq!(v["ok"], true);
    }

    #[tokio::test]
    async fn gives_up_on_persistent_rate_limit() {
        let server = MockServer::start().await;
        Mock::given(path("/x")).respond_with(ResponseTemplate::new(429).insert_header("Retry-After", "0")).mount(&server).await;
        let e = Http::new().get_json(&format!("{}/x", server.uri()), "Test").await.unwrap_err();
        assert!(e.to_string().contains("limiting"), "{e}");
        assert_eq!(server.received_requests().await.unwrap().len(), ATTEMPTS as usize);
    }

    #[tokio::test]
    async fn does_not_retry_client_errors() {
        let server = MockServer::start().await;
        Mock::given(path("/x")).respond_with(ResponseTemplate::new(404)).mount(&server).await;
        assert!(Http::new().get_text(&format!("{}/x", server.uri()), "Test").await.is_err());
        assert_eq!(server.received_requests().await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn offline_is_reported_as_offline() {
        // Nothing listens on port 9 of localhost.
        let e = Http::new().get_text("http://127.0.0.1:9/x", "Test").await.unwrap_err();
        assert_eq!(e.kind(), "offline", "{e}");
    }

    #[tokio::test]
    async fn refuses_plain_http_to_remote_hosts() {
        assert!(Http::new().get_text("http://example.com/x", "Test").await.is_err());
    }
}
