use tauri::State;

use crate::{
    app_state::AppState,
    db::repos::TagsRepo,
    error::{AppErrorResponse, CommandResult},
    types::tags::Tag,
};

#[tauri::command]
pub fn tags_suggest(state: State<'_, AppState>, query: String) -> CommandResult<Vec<Tag>> {
    let conn = state.conn().map_err(AppErrorResponse::from)?;
    TagsRepo::suggest(&conn, &query).map_err(AppErrorResponse::from)
}

#[tauri::command]
pub fn tags_list(state: State<'_, AppState>) -> CommandResult<Vec<Tag>> {
    let conn = state.conn().map_err(AppErrorResponse::from)?;
    TagsRepo::list(&conn).map_err(AppErrorResponse::from)
}
