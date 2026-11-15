//! IPC adapters. The webview is untrusted: every argument is validated by the core before use,
//! and errors cross the boundary as `{ kind, message, field }`.
//!
//! Commands run on the async runtime's thread pool (`async` or `#[command(async)]`), never on
//! the UI thread, so a slow query or lookup can't freeze the window.

pub mod business;
pub mod catalog;
pub mod docs;
pub mod intel;
pub mod quickbooks;
pub mod sales;
pub mod stock;
pub mod system;
pub mod vehicles;

use auto_manager_core::{AppError, AppResult, Core, settings};

/// Opaque ids are only used as bound SQL parameters; this bounds their size and alphabet.
pub fn check_id(id: &str) -> AppResult<&str> {
    if id.is_empty() || id.len() > 80 || !id.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.')) {
        return Err(AppError::msg("Invalid identifier."));
    }
    Ok(id)
}

pub fn valuation(core: &Core) -> String {
    core.read(|c| Ok(settings::load(c).valuation_method)).unwrap_or_else(|_| "average".into())
}
