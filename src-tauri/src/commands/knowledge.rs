use tauri::{AppHandle, Manager, WebviewWindow};

use crate::{
    app_state::AppState,
    commands::{require_main_window, run_blocking},
    db::{
        migrations::now_string,
        repos::{HealthRepo, HierarchyRepo, KnowledgeRepo},
    },
    error::{AppErrorResponse, CommandResult},
    types::{
        entries::EntryDetail,
        entries::PageRequest,
        health::{HealthIssueKind, HealthIssuePage, KnowledgeHealthSummary},
        hierarchy::{EntryBreadcrumb, EntryTreeNode},
        knowledge::{KnowledgeIndexReport, KnowledgeRelations, KnowledgeSuggestion},
    },
};

#[tauri::command]
pub async fn knowledge_suggest(
    app: AppHandle,
    window: WebviewWindow,
    query: String,
    limit: Option<u32>,
) -> CommandResult<Vec<KnowledgeSuggestion>> {
    require_main_window(window.label()).map_err(AppErrorResponse::from)?;
    run_blocking(move || {
        let state = app.state::<AppState>();
        state.with_read_conn(|conn| {
            KnowledgeRepo::suggest(conn, &query, limit.unwrap_or(10).clamp(1, 20))
        })
    })
    .await
    .map_err(AppErrorResponse::from)
}

#[tauri::command]
pub async fn knowledge_tree_get(
    app: AppHandle,
    window: WebviewWindow,
) -> CommandResult<Vec<EntryTreeNode>> {
    require_main_window(window.label()).map_err(AppErrorResponse::from)?;
    run_blocking(move || {
        let state = app.state::<AppState>();
        state.with_read_conn(HierarchyRepo::tree)
    })
    .await
    .map_err(AppErrorResponse::from)
}

#[tauri::command]
pub async fn knowledge_breadcrumbs_get(
    app: AppHandle,
    window: WebviewWindow,
    id: String,
) -> CommandResult<Vec<EntryBreadcrumb>> {
    require_main_window(window.label()).map_err(AppErrorResponse::from)?;
    run_blocking(move || {
        let state = app.state::<AppState>();
        state.with_read_conn(|conn| HierarchyRepo::breadcrumbs(conn, &id))
    })
    .await
    .map_err(AppErrorResponse::from)
}

#[tauri::command]
pub async fn knowledge_health_summary_get(
    app: AppHandle,
    window: WebviewWindow,
) -> CommandResult<KnowledgeHealthSummary> {
    require_main_window(window.label()).map_err(AppErrorResponse::from)?;
    run_blocking(move || {
        let state = app.state::<AppState>();
        state.with_read_conn(|conn| HealthRepo::summary(conn, time::OffsetDateTime::now_utc()))
    })
    .await
    .map_err(AppErrorResponse::from)
}

#[tauri::command]
pub async fn knowledge_health_issues_get(
    app: AppHandle,
    window: WebviewWindow,
    kind: HealthIssueKind,
    page: PageRequest,
) -> CommandResult<HealthIssuePage> {
    require_main_window(window.label()).map_err(AppErrorResponse::from)?;
    run_blocking(move || {
        let state = app.state::<AppState>();
        state.with_read_conn(|conn| {
            HealthRepo::issues(conn, kind, &page, time::OffsetDateTime::now_utc())
        })
    })
    .await
    .map_err(AppErrorResponse::from)
}

#[tauri::command]
pub async fn knowledge_relations_get(
    app: AppHandle,
    window: WebviewWindow,
    id: String,
) -> CommandResult<KnowledgeRelations> {
    require_main_window(window.label()).map_err(AppErrorResponse::from)?;
    run_blocking(move || {
        let state = app.state::<AppState>();
        state.with_read_conn(|conn| KnowledgeRepo::relations(conn, &id))
    })
    .await
    .map_err(AppErrorResponse::from)
}

#[tauri::command]
pub async fn knowledge_rebuild_index(
    app: AppHandle,
    window: WebviewWindow,
) -> CommandResult<KnowledgeIndexReport> {
    require_main_window(window.label()).map_err(AppErrorResponse::from)?;
    run_blocking(move || {
        let state = app.state::<AppState>();
        state.with_write_conn(KnowledgeRepo::rebuild_all_indexes)
    })
    .await
    .map_err(AppErrorResponse::from)
}

#[tauri::command]
pub async fn knowledge_promote(
    app: AppHandle,
    window: WebviewWindow,
    id: String,
    expected_revision: i64,
) -> CommandResult<EntryDetail> {
    require_main_window(window.label()).map_err(AppErrorResponse::from)?;
    let now = now_string();
    run_blocking(move || {
        let state = app.state::<AppState>();
        state.with_write_tx(|tx| KnowledgeRepo::promote(tx, &id, expected_revision, &now))
    })
    .await
    .map_err(AppErrorResponse::from)
}

#[tauri::command]
pub async fn knowledge_move(
    app: AppHandle,
    window: WebviewWindow,
    id: String,
    parent_entry_id: Option<String>,
    sibling_order: u32,
    expected_revision: i64,
) -> CommandResult<EntryDetail> {
    require_main_window(window.label()).map_err(AppErrorResponse::from)?;
    let now = now_string();
    run_blocking(move || {
        let state = app.state::<AppState>();
        state.with_write_tx(|tx| {
            HierarchyRepo::move_entry(
                tx,
                &id,
                parent_entry_id.as_deref(),
                sibling_order,
                expected_revision,
                &now,
            )
        })
    })
    .await
    .map_err(AppErrorResponse::from)
}

#[tauri::command]
pub async fn knowledge_demote(
    app: AppHandle,
    window: WebviewWindow,
    id: String,
    expected_revision: i64,
) -> CommandResult<EntryDetail> {
    require_main_window(window.label()).map_err(AppErrorResponse::from)?;
    let now = now_string();
    run_blocking(move || {
        let state = app.state::<AppState>();
        state.with_write_tx(|tx| KnowledgeRepo::demote(tx, &id, expected_revision, &now))
    })
    .await
    .map_err(AppErrorResponse::from)
}
