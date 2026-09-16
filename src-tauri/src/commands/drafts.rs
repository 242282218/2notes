use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, WebviewWindow};

use crate::{
    app_state::AppState,
    commands::{require_known_window, run_blocking},
    db::{migrations::now_string, repos::DraftsRepo},
    error::{AppErrorResponse, CommandResult},
    types::settings::Draft,
};

const ENTRIES_CHANGED: &str = "entries-changed";

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct EntriesChangedPayload {
    source: &'static str,
}

#[tauri::command]
pub async fn draft_get(app: AppHandle, window: WebviewWindow) -> CommandResult<Draft> {
    require_known_window(window.label()).map_err(AppErrorResponse::from)?;
    run_blocking(move || {
        let state = app.state::<AppState>();
        state.with_read_conn(DraftsRepo::get)
    })
    .await
    .map_err(AppErrorResponse::from)
}

#[tauri::command]
pub async fn draft_update(
    app: AppHandle,
    window: WebviewWindow,
    content: String,
    expected_revision: i64,
) -> CommandResult<Draft> {
    require_known_window(window.label()).map_err(AppErrorResponse::from)?;
    let now = now_string();
    run_blocking(move || {
        let state = app.state::<AppState>();
        state.with_write_tx(|tx| DraftsRepo::update(tx, &content, expected_revision, &now))
    })
    .await
    .map_err(AppErrorResponse::from)
}

#[tauri::command]
pub async fn quick_capture_submit(
    app: AppHandle,
    window: WebviewWindow,
    content: String,
    expected_revision: i64,
) -> CommandResult<Draft> {
    require_known_window(window.label()).map_err(AppErrorResponse::from)?;
    let now = now_string();
    let worker_app = app.clone();
    let draft = run_blocking(move || {
        let state = worker_app.state::<AppState>();
        state.with_write_tx(|tx| {
            DraftsRepo::submit_quick_capture(tx, &content, expected_revision, &now)
        })
    })
    .await
    .map_err(AppErrorResponse::from)?;
    if let Err(err) = app.emit_to(
        "main",
        ENTRIES_CHANGED,
        EntriesChangedPayload {
            source: "quick_capture",
        },
    ) {
        log::warn!("entries_changed_emit_failed source={err}");
    }
    Ok(draft)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entries_changed_payload_uses_frontend_field_names() {
        let value = serde_json::to_value(EntriesChangedPayload {
            source: "quick_capture",
        })
        .unwrap();

        assert_eq!(value["source"], "quick_capture");
    }
}
