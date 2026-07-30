use serde::Serialize;
use tauri::{AppHandle, Emitter, State};

use crate::{
    app_state::AppState,
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
pub fn draft_get(state: State<'_, AppState>) -> CommandResult<Draft> {
    let conn = state.read_conn().map_err(AppErrorResponse::from)?;
    DraftsRepo::get(&conn).map_err(AppErrorResponse::from)
}

#[tauri::command]
pub fn draft_update(
    state: State<'_, AppState>,
    content: String,
    expected_revision: i64,
) -> CommandResult<Draft> {
    let now = now_string();
    state
        .with_write_tx(|tx| DraftsRepo::update(tx, &content, expected_revision, &now))
        .map_err(AppErrorResponse::from)
}

#[tauri::command]
pub fn quick_capture_submit(
    app: AppHandle,
    state: State<'_, AppState>,
    content: String,
    expected_revision: i64,
) -> CommandResult<Draft> {
    let now = now_string();
    let draft = state
        .with_write_tx(|tx| DraftsRepo::submit_quick_capture(tx, &content, expected_revision, &now))
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
