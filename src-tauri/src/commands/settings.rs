use tauri::{AppHandle, State, WebviewWindow};
use tauri_plugin_autostart::ManagerExt;

use crate::{
    app_state::AppState,
    commands::require_main_window,
    db::{migrations::now_string, repos::SettingsRepo},
    error::{AppError, AppErrorResponse, CommandResult},
    types::settings::{AppSettings, SettingsPatch},
};

#[tauri::command]
pub fn settings_get(
    app: AppHandle,
    window: WebviewWindow,
    state: State<'_, AppState>,
) -> CommandResult<AppSettings> {
    require_main_window(window.label()).map_err(AppErrorResponse::from)?;
    let shortcut = state.shortcut_status().map_err(AppErrorResponse::from)?;
    let autostart_enabled = app.autolaunch().is_enabled().unwrap_or_else(|err| {
        log::warn!("autostart_status_failed source={err}");
        false
    });
    let theme_mode = state
        .read_conn()
        .map_err(AppErrorResponse::from)
        .and_then(|conn| SettingsRepo::get_string(&conn, "theme_mode").map_err(AppErrorResponse::from))?
        .unwrap_or_else(|| "system".to_string());

    Ok(AppSettings {
        data_dir: state.data_dir().display().to_string(),
        log_dir: state.log_dir().display().to_string(),
        backup_dir: state.backup_dir().display().to_string(),
        shortcut: shortcut.shortcut,
        shortcut_registered: shortcut.registered,
        shortcut_error: shortcut.error,
        autostart_enabled,
        theme_mode,
    })
}

#[tauri::command]
pub fn settings_update(
    app: AppHandle,
    window: WebviewWindow,
    state: State<'_, AppState>,
    patch: SettingsPatch,
) -> CommandResult<AppSettings> {
    require_main_window(window.label()).map_err(AppErrorResponse::from)?;
    if let Some(enabled) = patch.autostart_enabled {
        let result = if enabled {
            app.autolaunch().enable()
        } else {
            app.autolaunch().disable()
        };
        result
            .map_err(|err| AppError::system("AUTOSTART_UPDATE_FAILED", err.to_string()))
            .map_err(AppErrorResponse::from)?;

        let now = now_string();
        state
            .with_write_tx(|tx| SettingsRepo::set_bool(tx, "autostart_enabled", enabled, &now))
            .map_err(AppErrorResponse::from)?;
    }
    if let Some(theme_mode) = patch.theme_mode {
        if !matches!(theme_mode.as_str(), "system" | "light" | "dark") {
            return Err(AppError::validation(
                "INVALID_THEME_MODE",
                "主题必须是 system、light 或 dark",
            )
            .into());
        }
        let now = now_string();
        state
            .with_write_tx(|tx| SettingsRepo::set_string(tx, "theme_mode", &theme_mode, &now))
            .map_err(AppErrorResponse::from)?;
    }
    settings_get(app, window, state)
}
