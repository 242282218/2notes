use tauri::{AppHandle, Manager, State, WebviewWindow};

use crate::{
    app_state::AppState,
    commands::{require_main_window, run_blocking},
    db::{
        migrations::now_string,
        repos::{HierarchyRepo, KnowledgeRepo},
    },
    error::{AppErrorResponse, CommandResult},
    types::{
        entries::EntryDetail,
        hierarchy::{EntryBreadcrumb, EntryTreeNode},
        knowledge::{KnowledgeIndexReport, KnowledgeRelations, KnowledgeSuggestion},
    },
};

#[tauri::command]
pub fn knowledge_suggest(
    window: WebviewWindow,
    state: State<'_, AppState>,
    query: String,
    limit: Option<u32>,
) -> CommandResult<Vec<KnowledgeSuggestion>> {
    require_main_window(window.label()).map_err(AppErrorResponse::from)?;
    let conn = state.read_conn().map_err(AppErrorResponse::from)?;
    KnowledgeRepo::suggest(&conn, &query, limit.unwrap_or(10).clamp(1, 20))
        .map_err(AppErrorResponse::from)
}

#[tauri::command]
pub fn knowledge_tree_get(
    window: WebviewWindow,
    state: State<'_, AppState>,
) -> CommandResult<Vec<EntryTreeNode>> {
    require_main_window(window.label()).map_err(AppErrorResponse::from)?;
    let conn = state.read_conn().map_err(AppErrorResponse::from)?;
    HierarchyRepo::tree(&conn).map_err(AppErrorResponse::from)
}

#[tauri::command]
pub fn knowledge_breadcrumbs_get(
    window: WebviewWindow,
    state: State<'_, AppState>,
    id: String,
) -> CommandResult<Vec<EntryBreadcrumb>> {
    require_main_window(window.label()).map_err(AppErrorResponse::from)?;
    let conn = state.read_conn().map_err(AppErrorResponse::from)?;
    HierarchyRepo::breadcrumbs(&conn, &id).map_err(AppErrorResponse::from)
}

#[tauri::command]
pub fn knowledge_relations_get(
    window: WebviewWindow,
    state: State<'_, AppState>,
    id: String,
) -> CommandResult<KnowledgeRelations> {
    require_main_window(window.label()).map_err(AppErrorResponse::from)?;
    let conn = state.read_conn().map_err(AppErrorResponse::from)?;
    KnowledgeRepo::relations(&conn, &id).map_err(AppErrorResponse::from)
}

#[tauri::command]
pub async fn knowledge_rebuild_index(
    app: AppHandle,
    window: WebviewWindow,
) -> CommandResult<KnowledgeIndexReport> {
    require_main_window(window.label()).map_err(AppErrorResponse::from)?;
    run_blocking(move || {
        let state = app.state::<AppState>();
        let mut conn = state.write_conn()?;
        KnowledgeRepo::rebuild_all_indexes(&mut conn)
    })
    .await
    .map_err(AppErrorResponse::from)
}

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
pub fn knowledge_move(
    window: WebviewWindow,
    state: State<'_, AppState>,
    id: String,
    parent_entry_id: Option<String>,
    sibling_order: u32,
    expected_revision: i64,
) -> CommandResult<EntryDetail> {
    require_main_window(window.label()).map_err(AppErrorResponse::from)?;
    let now = now_string();
    state
        .with_write_tx(|tx| {
            HierarchyRepo::move_entry(
                tx,
                &id,
                parent_entry_id.as_deref(),
                sibling_order,
                expected_revision,
                &now,
            )
        })
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
