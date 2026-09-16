use tauri::{AppHandle, Manager, WebviewWindow};
use tauri_plugin_autostart::ManagerExt;

use crate::{
    app_state::AppState,
    commands::{require_main_window, run_blocking},
    db::{migrations::now_string, repos::SettingsRepo},
    error::{AppError, AppErrorResponse, CommandResult},
    types::settings::{AppSettings, SettingsPatch},
};

#[tauri::command]
pub async fn settings_get(app: AppHandle, window: WebviewWindow) -> CommandResult<AppSettings> {
    require_main_window(window.label()).map_err(AppErrorResponse::from)?;
    let state = app.state::<AppState>();
    let shortcut = state.shortcut_status().map_err(AppErrorResponse::from)?;
    let autostart_enabled = app.autolaunch().is_enabled().unwrap_or_else(|err| {
        log::warn!("autostart_status_failed source={err}");
        false
    });
    let worker_app = app.clone();
    let (theme_mode, backup_retention_count) = run_blocking(move || {
        let state = worker_app.state::<AppState>();
        state.with_read_conn(|conn| {
            let theme_mode = SettingsRepo::get_theme_mode(conn)?;
            let backup_retention_count = SettingsRepo::get_backup_retention_count(conn)?;
            Ok((theme_mode, backup_retention_count))
        })
    })
    .await
    .map_err(AppErrorResponse::from)?;

    Ok(AppSettings {
        data_dir: state.data_dir().display().to_string(),
        log_dir: state.log_dir().display().to_string(),
        backup_dir: state.backup_dir().display().to_string(),
        shortcut: shortcut.shortcut,
        shortcut_registered: shortcut.registered,
        shortcut_error: shortcut.error,
        autostart_enabled,
        theme_mode,
        backup_retention_count,
    })
}

#[tauri::command]
pub async fn settings_update(
    app: AppHandle,
    window: WebviewWindow,
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
    if let Some(shortcut) = patch.shortcut.as_deref() {
        let state = app.state::<AppState>();
        let previous_shortcut = state
            .shortcut_status()
            .map_err(AppErrorResponse::from)?
            .shortcut;
        crate::system::shortcuts::update_shortcut(&app, &state, shortcut).map_err(|message| {
            AppErrorResponse::from(AppError::validation("SHORTCUT_UPDATE_FAILED", message))
        })?;
        let now = now_string();
        let shortcut_write = shortcut.to_string();
        let worker_app = app.clone();
        let write_result = run_blocking(move || {
            let state = worker_app.state::<AppState>();
            state.with_write_tx(|tx| SettingsRepo::set_shortcut(tx, &shortcut_write, &now))
        })
        .await
        .map_err(AppErrorResponse::from);
        if let Err(error) = write_result {
            let _ = crate::system::shortcuts::update_shortcut(&app, &state, &previous_shortcut);
            return Err(error);
        }
    }
    if patch.autostart_enabled.is_some()
        || patch.theme_mode.is_some()
        || patch.backup_retention_count.is_some()
    {
        let now = now_string();
        let autostart_enabled = patch.autostart_enabled;
        let theme_mode = patch.theme_mode;
        let backup_retention_count = patch.backup_retention_count;
        let worker_app = app.clone();
        run_blocking(move || {
            let state = worker_app.state::<AppState>();
            state.with_write_tx(|tx| {
                if let Some(enabled) = autostart_enabled {
                    SettingsRepo::set_bool(tx, "autostart_enabled", enabled, &now)?;
                }
                if let Some(theme_mode) = theme_mode {
                    SettingsRepo::set_theme_mode(tx, theme_mode, &now)?;
                }
                if let Some(retention_count) = backup_retention_count {
                    SettingsRepo::set_backup_retention_count(tx, retention_count, &now)?;
                }
                Ok(())
            })
        })
        .await
        .map_err(AppErrorResponse::from)?;
    }
    settings_get(app, window).await
}
