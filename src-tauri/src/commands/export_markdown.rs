use std::path::PathBuf;

use tauri::{AppHandle, Manager, WebviewWindow};

use crate::{
    app_state::AppState,
    commands::{require_main_window, run_blocking},
    db::repos::EntriesRepo,
    error::{AppErrorResponse, CommandResult},
    files::markdown::export_entries,
    types::settings::ExportResult,
};

#[tauri::command]
pub async fn export_markdown(
    app: AppHandle,
    window: WebviewWindow,
    target_dir: String,
) -> CommandResult<ExportResult> {
    require_main_window(window.label()).map_err(AppErrorResponse::from)?;
    run_blocking(move || {
        let state = app.state::<AppState>();
        let target = PathBuf::from(target_dir);
        let entries = {
            let conn = state.read_conn()?;
            EntriesRepo::list_exportable(&conn)?
        };
        let paths = export_entries(&entries, &target)?;
        log::info!(
            "markdown_exported count={} target={}",
            paths.len(),
            target.display()
        );
        Ok(ExportResult {
            exported_count: paths.len(),
            target_dir: target.display().to_string(),
        })
    })
    .await
    .map_err(AppErrorResponse::from)
}
