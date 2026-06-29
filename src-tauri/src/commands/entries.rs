use tauri::State;

use crate::{
    app_state::AppState,
    db::{migrations::now_string, repos::EntriesRepo},
    error::{AppErrorResponse, CommandResult},
    types::entries::{EntryDetail, EntryListFilter, EntryPage, EntryPatch, PageRequest},
};

#[tauri::command]
pub fn entries_create(state: State<'_, AppState>, content: String) -> CommandResult<EntryDetail> {
    let mut conn = state.conn().map_err(AppErrorResponse::from)?;
    let tx = conn
        .transaction()
        .map_err(crate::error::AppError::from)
        .map_err(AppErrorResponse::from)?;
    let entry =
        EntriesRepo::create(&tx, &content, &now_string()).map_err(AppErrorResponse::from)?;
    tx.commit()
        .map_err(crate::error::AppError::from)
        .map_err(AppErrorResponse::from)?;
    log::info!("entry_created id={}", entry.id);
    Ok(entry)
}

#[tauri::command]
pub fn entries_list(
    state: State<'_, AppState>,
    filter: EntryListFilter,
    page: PageRequest,
) -> CommandResult<EntryPage> {
    let conn = state.conn().map_err(AppErrorResponse::from)?;
    EntriesRepo::list(&conn, &filter, &page).map_err(AppErrorResponse::from)
}

#[tauri::command]
pub fn entries_get(state: State<'_, AppState>, id: String) -> CommandResult<EntryDetail> {
    let conn = state.conn().map_err(AppErrorResponse::from)?;
    EntriesRepo::get(&conn, &id).map_err(AppErrorResponse::from)
}

#[tauri::command]
pub fn entries_update(
    state: State<'_, AppState>,
    id: String,
    patch: EntryPatch,
    expected_revision: i64,
) -> CommandResult<EntryDetail> {
    let mut conn = state.conn().map_err(AppErrorResponse::from)?;
    let tx = conn
        .transaction()
        .map_err(crate::error::AppError::from)
        .map_err(AppErrorResponse::from)?;
    let entry = EntriesRepo::update(&tx, &id, patch, expected_revision, &now_string())
        .map_err(AppErrorResponse::from)?;
    tx.commit()
        .map_err(crate::error::AppError::from)
        .map_err(AppErrorResponse::from)?;
    log::info!("entry_updated id={}", entry.id);
    Ok(entry)
}

#[tauri::command]
pub fn entries_move_to_trash(state: State<'_, AppState>, id: String) -> CommandResult<EntryDetail> {
    let mut conn = state.conn().map_err(AppErrorResponse::from)?;
    let tx = conn
        .transaction()
        .map_err(crate::error::AppError::from)
        .map_err(AppErrorResponse::from)?;
    let entry =
        EntriesRepo::move_to_trash(&tx, &id, &now_string()).map_err(AppErrorResponse::from)?;
    tx.commit()
        .map_err(crate::error::AppError::from)
        .map_err(AppErrorResponse::from)?;
    Ok(entry)
}

#[tauri::command]
pub fn entries_restore_from_trash(
    state: State<'_, AppState>,
    id: String,
) -> CommandResult<EntryDetail> {
    let mut conn = state.conn().map_err(AppErrorResponse::from)?;
    let tx = conn
        .transaction()
        .map_err(crate::error::AppError::from)
        .map_err(AppErrorResponse::from)?;
    let entry =
        EntriesRepo::restore_from_trash(&tx, &id, &now_string()).map_err(AppErrorResponse::from)?;
    tx.commit()
        .map_err(crate::error::AppError::from)
        .map_err(AppErrorResponse::from)?;
    Ok(entry)
}

#[tauri::command]
pub fn entries_delete_forever(state: State<'_, AppState>, id: String) -> CommandResult<()> {
    let mut conn = state.conn().map_err(AppErrorResponse::from)?;
    let tx = conn
        .transaction()
        .map_err(crate::error::AppError::from)
        .map_err(AppErrorResponse::from)?;
    EntriesRepo::delete_forever(&tx, &id).map_err(AppErrorResponse::from)?;
    tx.commit()
        .map_err(crate::error::AppError::from)
        .map_err(AppErrorResponse::from)?;
    Ok(())
}
