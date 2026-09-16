use tauri::{AppHandle, Manager};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};

use crate::{
    app_state::AppState, db::repos::settings_repo::DEFAULT_SHORTCUT, db::repos::SettingsRepo,
    error::AppErrorResponse, system::windows::show_quick_capture_window,
};

pub fn register_default_shortcut(app: &AppHandle) {
    let shortcut = app
        .try_state::<AppState>()
        .and_then(|state| state.with_read_conn(SettingsRepo::get_shortcut).ok())
        .unwrap_or_else(|| DEFAULT_SHORTCUT.to_string());
    let result = register_shortcut(app, &shortcut);
    if let Some(state) = app.try_state::<AppState>() {
        let error = result.as_ref().err().cloned();
        let _ = state.set_shortcut(shortcut, result.is_ok(), error.clone());
        if let Some(message) = error {
            log::warn!("global_shortcut_failed source={message}");
        } else {
            log::info!("global_shortcut_registered");
        }
    }
}

pub fn register_shortcut(app: &AppHandle, shortcut: &str) -> Result<(), String> {
    SettingsRepo::validate_shortcut(shortcut).map_err(|err| err.to_string())?;
    app.global_shortcut()
        .on_shortcut(shortcut, |app, _shortcut, event| {
            if event.state == ShortcutState::Pressed {
                if let Err(err) = show_quick_capture_window(app) {
                    let response = AppErrorResponse::from(err);
                    log::error!("quick_capture_open_failed code={}", response.code);
                }
            }
        })
        .map_err(|err| err.to_string())
}

pub fn unregister_shortcut(app: &AppHandle, shortcut: &str) -> Result<(), String> {
    app.global_shortcut()
        .unregister(shortcut)
        .map_err(|err| err.to_string())
}

pub fn update_shortcut(app: &AppHandle, state: &AppState, next: &str) -> Result<(), String> {
    SettingsRepo::validate_shortcut(next).map_err(|err| err.to_string())?;
    let previous = state.shortcut_status().map_err(|err| err.to_string())?;
    if previous.registered && previous.shortcut == next {
        return Ok(());
    }
    if previous.registered {
        unregister_shortcut(app, &previous.shortcut)?;
    }
    if let Err(error) = register_shortcut(app, next) {
        if previous.registered {
            let _ = register_shortcut(app, &previous.shortcut);
        }
        return Err(error);
    }
    state
        .set_shortcut(next.to_string(), true, None)
        .map_err(|err| err.to_string())?;
    Ok(())
}
