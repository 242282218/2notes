use tauri::{State, WebviewWindow};

use crate::{
    app_state::AppState,
    commands::require_main_window,
    db::repos::TagsRepo,
    error::{AppErrorResponse, CommandResult},
    types::tags::Tag,
};

#[tauri::command]
pub fn tags_suggest(
    window: WebviewWindow,
    state: State<'_, AppState>,
    query: String,
) -> CommandResult<Vec<Tag>> {
    require_main_window(window.label()).map_err(AppErrorResponse::from)?;
    let conn = state.read_conn().map_err(AppErrorResponse::from)?;
    TagsRepo::suggest(&conn, &query).map_err(AppErrorResponse::from)
}

#[tauri::command]
pub fn tags_list(window: WebviewWindow, state: State<'_, AppState>) -> CommandResult<Vec<Tag>> {
    require_main_window(window.label()).map_err(AppErrorResponse::from)?;
    let conn = state.read_conn().map_err(AppErrorResponse::from)?;
    TagsRepo::list(&conn).map_err(AppErrorResponse::from)
}
