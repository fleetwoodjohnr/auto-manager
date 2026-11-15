//! QuickBooks → Auto Manager. Exports are re-read and re-checked on apply, all in one
//! transaction, so what's written matches the data as it is then.

use auto_manager_core::quickbooks::{self, QbOptions, QbPreview, QbSummary, files::ExportKind};
use auto_manager_core::{AppResult, Core};
use serde::Serialize;
use std::path::PathBuf;
use tauri::State;

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
