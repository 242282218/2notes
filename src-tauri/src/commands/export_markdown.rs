use std::path::PathBuf;

use tauri::State;

use crate::{
    app_state::AppState,
    db::repos::EntriesRepo,
    error::{AppErrorResponse, CommandResult},
    files::markdown::export_entries,
    types::settings::ExportResult,
};

#[tauri::command]
pub fn export_markdown(
    state: State<'_, AppState>,
    target_dir: String,
) -> CommandResult<ExportResult> {
    let target = PathBuf::from(target_dir);
    let entries = {
        let conn = state.conn().map_err(AppErrorResponse::from)?;
        EntriesRepo::list_exportable(&conn).map_err(AppErrorResponse::from)?
    };
    let paths = export_entries(&entries, &target).map_err(AppErrorResponse::from)?;
    log::info!(
        "markdown_exported count={} target={}",
        paths.len(),
        target.display()
    );
    Ok(ExportResult {
        exported_count: paths.len(),
        target_dir: target.display().to_string(),
    })
}
