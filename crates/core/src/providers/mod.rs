//! External data providers. Each one is a small module with a pure parser (tested against
//! recorded responses) and a fetch function that goes through the shared HTTP client and the
//! `api_cache` table, so a lookup that worked once keeps working offline.
//!
//! See docs/PROVIDERS.md for what each provider supplies and its terms.

pub mod bulletins;
pub mod canada;
pub mod epa;
pub mod http;
pub mod oem_docs;
pub mod recalls;
pub mod safety;
pub mod vpic;

use crate::db;
use crate::error::{AppError, AppResult};
use crate::state::Core;
use rusqlite::OptionalExtension;
use serde::Serialize;
use ts_rs::TS;

/// Base URLs. Overridable through the environment so end-to-end tests can point the app at a
/// local fixture server; production builds use the defaults.
#[derive(Clone, Debug)]
pub struct Endpoints {
    pub vpic: String,
    pub nhtsa: String,
    /// Where NHTSA's public documents (bulletin PDFs) are served from.
    pub nhtsa_static: String,
    pub fueleconomy: String,
}

impl Endpoints {
    pub fn from_env() -> Self {
        let get = |k: &str, d: &str| std::env::var(k).ok().filter(|v| !v.is_empty()).unwrap_or_else(|| d.to_string());
        Endpoints {
            vpic: get("AUTO_MANAGER_VPIC_BASE", "https://vpic.nhtsa.dot.gov/api/vehicles"),
            nhtsa: get("AUTO_MANAGER_NHTSA_BASE", "https://api.nhtsa.gov"),
            nhtsa_static: get("AUTO_MANAGER_NHTSA_STATIC_BASE", "https://static.nhtsa.gov"),
            fueleconomy: get("AUTO_MANAGER_FUELECONOMY_BASE", "https://www.fueleconomy.gov/ws/rest"),
        }
    }
}

/// Where a provider answer came from, shown next to the data.
#[derive(Serialize, TS, Clone, Debug, PartialEq)]
#[ts(export)]
pub struct Freshness {
    pub source: String,
    pub fetched_at: String,
    /// True when the network failed and this is an older saved copy.
    pub stale: bool,
    pub from_cache: bool,
}

pub struct Fetched<T> {
    pub value: T,
    pub freshness: Freshness,
}

impl Core {
    /// GET `url` through the cache. A fresh cached copy is returned without a request; on a
    /// network failure an expired copy is returned and marked stale.
    pub async fn cached_text(&self, source: &str, url: &str, who: &str, ttl_hours: i64) -> AppResult<Fetched<String>> {
        self.cached_fetch(source, url, None, who, ttl_hours).await
    }

    /// As `cached_text`, asking for JSON with an Accept header.
    pub async fn cached_json_accept(&self, source: &str, url: &str, who: &str, ttl_hours: i64) -> AppResult<Fetched<String>> {
        self.cached_fetch(source, url, Some("application/json"), who, ttl_hours).await
    }

    async fn cached_fetch(&self, source: &str, url: &str, accept: Option<&str>, who: &str, ttl_hours: i64) -> AppResult<Fetched<String>> {
        let cached: Option<(String, String, String)> = self.read(|c| {
            Ok(c.query_row("SELECT body, fetched_at, expires_at FROM api_cache WHERE key = ?1", [url], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?))
            })
            .optional()?)
        })?;
        let now = db::now_utc();
        if let Some((body, fetched_at, expires_at)) = &cached
            && *expires_at > now
        {
            let freshness = Freshness { source: source.into(), fetched_at: fetched_at.clone(), stale: false, from_cache: true };
            return Ok(Fetched { value: body.clone(), freshness });
        }
        let fetched = match accept {
            Some(a) => self.http.get_accept(url, a, who).await,
            None => self.http.get_text(url, who).await,
        };
        match fetched {
            Ok(body) => {
                let expires = (chrono::Utc::now() + chrono::Duration::hours(ttl_hours)).format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string();
                self.tx(|c| {
                    c.execute(
                        "INSERT INTO api_cache (key, source_id, body, fetched_at, expires_at) VALUES (?1, ?2, ?3, ?4, ?5)
                         ON CONFLICT (key) DO UPDATE SET body = excluded.body, fetched_at = excluded.fetched_at, expires_at = excluded.expires_at",
                        rusqlite::params![url, source, body, now, expires],
                    )?;
                    Ok(())
                })?;
                Ok(Fetched {
                    value: body,
                    freshness: Freshness { source: source.into(), fetched_at: now, stale: false, from_cache: false },
                })
            }
            Err(e @ AppError::Provider { .. }) => match cached {
                Some((body, fetched_at, _)) => {
                    tracing::warn!(source, "provider failed, using saved copy: {e}");
                    Ok(Fetched { value: body, freshness: Freshness { source: source.into(), fetched_at, stale: true, from_cache: true } })
                }
                None => Err(e),
            },
            Err(e) => Err(e),
        }
    }
}

#[derive(Serialize, TS, Clone, Debug)]
#[ts(export)]
pub struct CacheStats {
    pub entries: i64,
    pub bytes: i64,
    pub oldest: Option<String>,
}

pub fn cache_stats(conn: &rusqlite::Connection) -> AppResult<CacheStats> {
    Ok(conn.query_row("SELECT count(*), coalesce(sum(length(body)), 0), min(fetched_at) FROM api_cache", [], |r| {
        Ok(CacheStats { entries: r.get(0)?, bytes: r.get(1)?, oldest: r.get(2)? })
    })?)
}

pub fn set_priority(conn: &rusqlite::Connection, id: &str, priority: i64) -> AppResult<()> {
    db::expect_one(
        conn.execute("UPDATE data_sources SET priority = ?2 WHERE id = ?1 AND id <> 'user'", rusqlite::params![id, priority])?,
        "That source",
    )
}

pub fn clear_cache(conn: &rusqlite::Connection) -> AppResult<usize> {
    Ok(conn.execute("DELETE FROM api_cache", [])?)
}

/// Percent-encode one URL path segment.
pub fn seg(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~' | b'*') {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}
