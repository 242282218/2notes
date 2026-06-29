use tauri::{AppHandle, Manager};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};

use crate::{
    app_state::AppState, error::AppErrorResponse, system::windows::show_quick_capture_window,
};

pub fn register_default_shortcut(app: &AppHandle) {
    let shortcut = "Ctrl+Alt+Space";
    let result = app
        .global_shortcut()
        .on_shortcut(shortcut, |app, _shortcut, event| {
            if event.state == ShortcutState::Pressed {
                if let Err(err) = show_quick_capture_window(app) {
                    let response = AppErrorResponse::from(err);
                    log::error!("quick_capture_open_failed code={}", response.code);
                }
            }
        });

    if let Some(state) = app.try_state::<AppState>() {
        match result {
            Ok(()) => {
                let _ = state.set_shortcut_status(true, None);
                log::info!("global_shortcut_registered shortcut={shortcut}");
            }
            Err(err) => {
                let message = err.to_string();
                let _ = state.set_shortcut_status(false, Some(message.clone()));
                log::warn!("global_shortcut_failed shortcut={shortcut} source={message}");
            }
        }
    }
}
