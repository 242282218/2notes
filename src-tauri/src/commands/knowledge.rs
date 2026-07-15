use tauri::{State, WebviewWindow};

use crate::{
    app_state::AppState,
    commands::require_main_window,
    db::{migrations::now_string, repos::KnowledgeRepo},
    error::{AppErrorResponse, CommandResult},
    types::entries::EntryDetail,
};

#[tauri::command]
pub fn knowledge_promote(
    window: WebviewWindow,
    state: State<'_, AppState>,
    id: String,
    expected_revision: i64,
) -> CommandResult<EntryDetail> {
    require_main_window(window.label()).map_err(AppErrorResponse::from)?;
    let now = now_string();
    state
        .with_write_tx(|tx| KnowledgeRepo::promote(tx, &id, expected_revision, &now))
        .map_err(AppErrorResponse::from)
}

#[tauri::command]
pub fn knowledge_demote(
    window: WebviewWindow,
    state: State<'_, AppState>,
    id: String,
    expected_revision: i64,
) -> CommandResult<EntryDetail> {
    require_main_window(window.label()).map_err(AppErrorResponse::from)?;
    let now = now_string();
    state
        .with_write_tx(|tx| KnowledgeRepo::demote(tx, &id, expected_revision, &now))
        .map_err(AppErrorResponse::from)
}
