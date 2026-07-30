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
        .and_then(|conn| SettingsRepo::get_theme_mode(&conn).map_err(AppErrorResponse::from))?;

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
    // Autostart is an OS-level side effect outside SQLite; apply it first so a
    // later DB write failure leaves the OS in the user-requested state. The two
    // settings rows below are written in ONE transaction so either both land
    // or neither does (previously each had its own tx → partial-apply bug).
    if let Some(enabled) = patch.autostart_enabled {
        let result = if enabled {
            app.autolaunch().enable()
        } else {
            app.autolaunch().disable()
        };
        result
            .map_err(|err| AppError::system("AUTOSTART_UPDATE_FAILED", err.to_string()))
            .map_err(AppErrorResponse::from)?;
    }
    if patch.autostart_enabled.is_some() || patch.theme_mode.is_some() {
        let now = now_string();
        state
            .with_write_tx(|tx| {
                if let Some(enabled) = patch.autostart_enabled {
                    SettingsRepo::set_bool(tx, "autostart_enabled", enabled, &now)?;
                }
                if let Some(theme_mode) = patch.theme_mode {
                    SettingsRepo::set_theme_mode(tx, theme_mode, &now)?;
                }
                Ok(())
            })
            .map_err(AppErrorResponse::from)?;
    }
    settings_get(app, window, state)
}
