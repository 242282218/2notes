use tauri::AppHandle;

use crate::{
    error::{AppErrorResponse, CommandResult},
    system::windows::{hide_quick_capture_window, show_quick_capture_window},
};

#[tauri::command]
pub async fn window_open_quick_capture(app: AppHandle) -> CommandResult<()> {
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
    window_label: String,
) -> CommandResult<()> {
    crate::system::windows::mark_app_quit_ready(&app, &request_id, &window_label)
        .map_err(AppErrorResponse::from)
}
