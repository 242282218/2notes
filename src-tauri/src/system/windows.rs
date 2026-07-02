use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, WebviewUrl, WebviewWindowBuilder};

use crate::{app_state::AppState, error::AppResult};

const APP_QUIT_REQUESTED: &str = "app-quit-requested";

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct QuitRequestPayload {
    request_id: String,
}

pub fn show_main_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

pub fn show_quick_capture_window(app: &AppHandle) -> AppResult<()> {
    let window = if let Some(window) = app.get_webview_window("quick-capture") {
        window
    } else {
        WebviewWindowBuilder::new(
            app,
            "quick-capture",
            WebviewUrl::App("index.html?view=quick-capture".into()),
        )
        .title("快速记录")
        .inner_size(520.0, 280.0)
        .resizable(false)
        .always_on_top(true)
        .visible(false)
        .build()?
    };
    window.show()?;
    window.unminimize()?;
    window.set_focus()?;
    Ok(())
}

pub fn hide_quick_capture_window(app: &AppHandle) -> AppResult<()> {
    if let Some(window) = app.get_webview_window("quick-capture") {
        window.hide()?;
    }
    Ok(())
}

pub fn request_app_quit(app: &AppHandle) -> AppResult<()> {
    let windows = app
        .webview_windows()
        .keys()
        .cloned()
        .collect::<Vec<String>>();
    if windows.is_empty() {
        app.exit(0);
        return Ok(());
    }

    let Some(state) = app.try_state::<AppState>() else {
        app.exit(0);
        return Ok(());
    };
    let request_id = state.start_quit_request(windows.clone())?;
    let payload = QuitRequestPayload { request_id };
    for label in windows {
        app.emit_to(label.as_str(), APP_QUIT_REQUESTED, payload.clone())?;
    }
    Ok(())
}

pub fn mark_app_quit_ready(app: &AppHandle, request_id: &str, window_label: &str) -> AppResult<()> {
    let Some(state) = app.try_state::<AppState>() else {
        return Ok(());
    };
    if state.mark_quit_ready(request_id, window_label)? {
        app.exit(0);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quit_request_payload_uses_frontend_field_names() {
        let value = serde_json::to_value(QuitRequestPayload {
            request_id: "request-1".to_string(),
        })
        .unwrap();

        assert_eq!(value["requestId"], "request-1");
        assert!(value.get("request_id").is_none());
    }
}
