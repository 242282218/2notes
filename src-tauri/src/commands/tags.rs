use tauri::{AppHandle, Manager, WebviewWindow};

use crate::{
    app_state::AppState,
    commands::{require_main_window, run_blocking},
    db::repos::TagsRepo,
    error::{AppErrorResponse, CommandResult},
    types::tags::Tag,
};

#[tauri::command]
pub async fn tags_suggest(
    app: AppHandle,
    window: WebviewWindow,
    query: String,
) -> CommandResult<Vec<Tag>> {
    require_main_window(window.label()).map_err(AppErrorResponse::from)?;
    run_blocking(move || {
        let state = app.state::<AppState>();
        state.with_read_conn(|conn| TagsRepo::suggest(conn, &query))
    })
    .await
    .map_err(AppErrorResponse::from)
}

#[tauri::command]
pub async fn tags_list(app: AppHandle, window: WebviewWindow) -> CommandResult<Vec<Tag>> {
    require_main_window(window.label()).map_err(AppErrorResponse::from)?;
    run_blocking(move || {
        let state = app.state::<AppState>();
        state.with_read_conn(TagsRepo::list)
    })
    .await
    .map_err(AppErrorResponse::from)
}
