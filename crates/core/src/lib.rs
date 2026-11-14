//! Auto Manager core: everything except the window. No Tauri dependency, so it is tested
//! directly with `cargo test`.

pub mod aces_pies;
pub mod backup;
pub mod csvio;
pub mod customers;
pub mod dashboard;
pub mod db;
pub mod diagnostics;
pub mod dtc;
pub mod error;
pub mod export;
pub mod fitment;
pub mod interchange;
pub mod inventory;
pub mod jobs;
pub mod labels;
pub mod lists;
pub mod logging;
pub mod maintenance;
pub mod manuals;
pub mod matches;
pub mod money;
pub mod parts;
pub mod pricing;
pub mod providers;
pub mod purchasing;
pub mod quality;
pub mod reports;
pub mod search;
pub mod settings;
pub mod specs;
pub mod state;
pub mod suppliers;
pub mod sync;
pub mod vehicles;
pub mod vin;

pub use error::{AppError, AppResult};
pub use state::{Core, Paths};
