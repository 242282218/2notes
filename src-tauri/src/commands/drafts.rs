use tauri::State;

use crate::{
    app_state::AppState,
    db::{migrations::now_string, repos::DraftsRepo},
    error::{AppErrorResponse, CommandResult},
    types::settings::Draft,
};

#[tauri::command]
pub fn draft_get(state: State<'_, AppState>) -> CommandResult<Draft> {
    let conn = state.conn().map_err(AppErrorResponse::from)?;
    DraftsRepo::get(&conn).map_err(AppErrorResponse::from)
}

#[tauri::command]
pub fn draft_update(
    state: State<'_, AppState>,
    content: String,
    expected_revision: i64,
) -> CommandResult<Draft> {
    let mut conn = state.conn().map_err(AppErrorResponse::from)?;
    let tx = conn
        .transaction()
        .map_err(crate::error::AppError::from)
        .map_err(AppErrorResponse::from)?;
    let draft = DraftsRepo::update(&tx, &content, expected_revision, &now_string())
        .map_err(AppErrorResponse::from)?;
    tx.commit()
        .map_err(crate::error::AppError::from)
        .map_err(AppErrorResponse::from)?;
    Ok(draft)
}

#[tauri::command]
pub fn draft_clear(state: State<'_, AppState>) -> CommandResult<Draft> {
    let mut conn = state.conn().map_err(AppErrorResponse::from)?;
    let tx = conn
        .transaction()
        .map_err(crate::error::AppError::from)
        .map_err(AppErrorResponse::from)?;
    let draft = DraftsRepo::clear(&tx, &now_string()).map_err(AppErrorResponse::from)?;
    tx.commit()
        .map_err(crate::error::AppError::from)
        .map_err(AppErrorResponse::from)?;
    Ok(draft)
}
