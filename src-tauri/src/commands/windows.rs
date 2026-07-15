use tauri::{AppHandle, WebviewWindow};

use crate::{
    commands::require_main_window,
    error::{AppErrorResponse, CommandResult},
    system::windows::{hide_quick_capture_window, show_quick_capture_window},
};

#[tauri::command]
pub async fn window_open_quick_capture(app: AppHandle, window: WebviewWindow) -> CommandResult<()> {
    require_main_window(window.label()).map_err(AppErrorResponse::from)?;
    show_quick_capture_window(&app).map_err(AppErrorResponse::from)
}

#[tauri::command]
pub async fn window_hide_quick_capture(app: AppHandle) -> CommandResult<()> {
    hide_quick_capture_window(&app).map_err(AppErrorResponse::from)
}

#[tauri::command]
pub async fn app_quit_ready(
    app: AppHandle,
    request_id: String,
    window: WebviewWindow,
) -> CommandResult<()> {
    crate::system::windows::mark_app_quit_ready(&app, &request_id, window.label())
        .map_err(AppErrorResponse::from)
}
