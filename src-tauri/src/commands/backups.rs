use std::time::Duration;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State, WebviewWindow};

use crate::{
    app_state::AppState,
    commands::{require_main_window, run_blocking},
    error::{AppError, AppErrorResponse, CommandResult},
    files::backups::{create_backup, list_backups, restore_backup},
    types::backups::BackupInfo,
};

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct DatabaseRestorePreparePayload {
    request_id: String,
}

#[tauri::command]
pub async fn backups_create(
    app: AppHandle,
    window: WebviewWindow,
    kind: String,
) -> CommandResult<BackupInfo> {
    require_main_window(window.label()).map_err(AppErrorResponse::from)?;
    let paths = app.state::<AppState>().paths.clone();
    run_blocking(move || create_backup(&paths, &kind))
        .await
        .map_err(AppErrorResponse::from)
}

#[tauri::command]
pub async fn backups_list(app: AppHandle, window: WebviewWindow) -> CommandResult<Vec<BackupInfo>> {
    require_main_window(window.label()).map_err(AppErrorResponse::from)?;
    let paths = app.state::<AppState>().paths.clone();
    run_blocking(move || list_backups(&paths))
        .await
        .map_err(AppErrorResponse::from)
}

#[tauri::command]
pub async fn backups_restore(
    app: AppHandle,
    window: WebviewWindow,
    path: String,
) -> CommandResult<BackupInfo> {
    require_main_window(window.label()).map_err(AppErrorResponse::from)?;
    prepare_quick_capture_for_restore(&app).await?;
    let worker_app = app.clone();
    let restored = run_blocking(move || {
        let state = worker_app.state::<AppState>();
        restore_backup(&state, &path)
    })
    .await
    .map_err(AppErrorResponse::from)?;
    if let Err(err) = app.emit_to("quick-capture", "database-restored", ()) {
        log::warn!("database_restored_emit_failed source={err}");
    }
    Ok(restored)
}
#[tauri::command]
pub fn database_restore_ready(
    window: WebviewWindow,
    state: State<'_, AppState>,
    request_id: String,
) -> CommandResult<()> {
    if window.label() != "quick-capture" {
        return Err(AppErrorResponse::from(AppError::validation(
            "COMMAND_FORBIDDEN",
            "当前窗口无权确认数据库恢复",
        )));
    }
    if !state
        .mark_restore_ready(&request_id)
        .map_err(AppErrorResponse::from)?
    {
        return Err(AppErrorResponse::from(AppError::validation(
            "RESTORE_REQUEST_STALE",
            "数据库恢复请求已失效",
        )));
    }
    Ok(())
}

async fn prepare_quick_capture_for_restore(app: &AppHandle) -> CommandResult<()> {
    if app.get_webview_window("quick-capture").is_none() {
        return Ok(());
    }
    let state = app.state::<AppState>();
    let request_id = state
        .start_restore_request()
        .map_err(AppErrorResponse::from)?;
    let payload = DatabaseRestorePreparePayload {
        request_id: request_id.clone(),
    };
    if let Err(err) = app.emit_to("quick-capture", "database-restore-prepare", payload) {
        let _ = state.cancel_restore_request(&request_id);
        return Err(AppErrorResponse::from(AppError::from(err)));
    }
    let worker_app = app.clone();
    let wait_request_id = request_id.clone();
    let ready = run_blocking(move || {
        worker_app
            .state::<AppState>()
            .wait_restore_ready(&wait_request_id, Duration::from_secs(5))
    })
    .await
    .map_err(AppErrorResponse::from)?;
    if ready {
        return Ok(());
    }
    Err(AppErrorResponse::from(AppError::validation(
        "QUICK_CAPTURE_NOT_READY",
        "快速记录尚未保存，已取消数据库恢复",
    )))
}
