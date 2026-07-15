use tauri::{State, WebviewWindow};

use crate::{
    app_state::AppState,
    commands::require_main_window,
    db::migrations::now_string,
    db::repos::EntriesRepo,
    error::{AppErrorResponse, CommandResult},
    types::entries::{EntryDetail, EntryListFilter, EntryPage, EntryPatch, PageRequest},
};

#[tauri::command]
pub fn entries_list(
    window: WebviewWindow,
    state: State<'_, AppState>,
    filter: EntryListFilter,
    page: PageRequest,
) -> CommandResult<EntryPage> {
    require_main_window(window.label()).map_err(AppErrorResponse::from)?;
    let conn = state.read_conn().map_err(AppErrorResponse::from)?;
    EntriesRepo::list(&conn, &filter, &page).map_err(AppErrorResponse::from)
}

#[tauri::command]
pub fn entries_get(
    window: WebviewWindow,
    state: State<'_, AppState>,
    id: String,
) -> CommandResult<EntryDetail> {
    require_main_window(window.label()).map_err(AppErrorResponse::from)?;
    let conn = state.read_conn().map_err(AppErrorResponse::from)?;
    EntriesRepo::get(&conn, &id).map_err(AppErrorResponse::from)
}

#[tauri::command]
pub fn entries_update(
    window: WebviewWindow,
    state: State<'_, AppState>,
    id: String,
    patch: EntryPatch,
    expected_revision: i64,
) -> CommandResult<EntryDetail> {
    require_main_window(window.label()).map_err(AppErrorResponse::from)?;
    let now = now_string();
    let entry = state
        .with_write_tx(|tx| EntriesRepo::update(tx, &id, patch, expected_revision, &now))
        .map_err(AppErrorResponse::from)?;
    log::info!("entry_updated id={}", entry.id);
    Ok(entry)
}

#[tauri::command]
pub fn entries_move_to_trash(
    window: WebviewWindow,
    state: State<'_, AppState>,
    id: String,
    expected_revision: i64,
) -> CommandResult<EntryDetail> {
    require_main_window(window.label()).map_err(AppErrorResponse::from)?;
    let now = now_string();
    state
        .with_write_tx(|tx| EntriesRepo::move_to_trash(tx, &id, expected_revision, &now))
        .map_err(AppErrorResponse::from)
}

#[tauri::command]
pub fn entries_restore_from_trash(
    window: WebviewWindow,
    state: State<'_, AppState>,
    id: String,
    expected_revision: i64,
) -> CommandResult<EntryDetail> {
    require_main_window(window.label()).map_err(AppErrorResponse::from)?;
    let now = now_string();
    state
        .with_write_tx(|tx| EntriesRepo::restore_from_trash(tx, &id, expected_revision, &now))
        .map_err(AppErrorResponse::from)
}

#[tauri::command]
pub fn entries_delete_forever(
    window: WebviewWindow,
    state: State<'_, AppState>,
    id: String,
) -> CommandResult<()> {
    require_main_window(window.label()).map_err(AppErrorResponse::from)?;
    state
        .with_write_tx(|tx| EntriesRepo::delete_forever(tx, &id))
        .map_err(AppErrorResponse::from)
}
