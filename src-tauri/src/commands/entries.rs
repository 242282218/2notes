use tauri::{AppHandle, Manager, WebviewWindow};

use crate::{
    app_state::AppState,
    commands::{require_main_window, run_blocking},
    db::migrations::now_string,
    db::repos::EntriesRepo,
    error::{AppErrorResponse, CommandResult},
    types::{
        documents::{BlockDocument, BlockNode},
        entries::{
            CreateEntrySpec, EntryDetail, EntryListFilter, EntryPage, EntryPatch, EntryStatus,
            EntryType, PageRequest, TitleSource,
        },
    },
};

fn new_entry_spec() -> CreateEntrySpec {
    CreateEntrySpec {
        title: None,
        title_source: TitleSource::Auto,
        original_content: String::new(),
        document: BlockDocument::from_blocks(vec![BlockNode::empty_paragraph(
            uuid::Uuid::new_v4().to_string(),
        )]),
        entry_type: EntryType::Unclear,
        status: EntryStatus::Pending,
        tags: Vec::new(),
    }
}

#[tauri::command]
pub async fn entries_create(app: AppHandle, window: WebviewWindow) -> CommandResult<EntryDetail> {
    require_main_window(window.label()).map_err(AppErrorResponse::from)?;
    let now = now_string();
    let entry = run_blocking(move || {
        let state = app.state::<AppState>();
        state.with_write_tx(|tx| EntriesRepo::create_with_document(tx, new_entry_spec(), &now))
    })
    .await
    .map_err(AppErrorResponse::from)?;
    log::info!("entry_created id={}", entry.id);
    Ok(entry)
}

#[tauri::command]
pub async fn entries_list(
    app: AppHandle,
    window: WebviewWindow,
    filter: EntryListFilter,
    page: PageRequest,
) -> CommandResult<EntryPage> {
    require_main_window(window.label()).map_err(AppErrorResponse::from)?;
    run_blocking(move || {
        let state = app.state::<AppState>();
        state.with_read_conn(|conn| EntriesRepo::list(conn, &filter, &page))
    })
    .await
    .map_err(AppErrorResponse::from)
}

#[tauri::command]
pub async fn entries_get(
    app: AppHandle,
    window: WebviewWindow,
    id: String,
) -> CommandResult<EntryDetail> {
    require_main_window(window.label()).map_err(AppErrorResponse::from)?;
    run_blocking(move || {
        let state = app.state::<AppState>();
        state.with_read_conn(|conn| EntriesRepo::get(conn, &id))
    })
    .await
    .map_err(AppErrorResponse::from)
}

#[tauri::command]
pub async fn entries_update(
    app: AppHandle,
    window: WebviewWindow,
    id: String,
    patch: EntryPatch,
    expected_revision: i64,
) -> CommandResult<EntryDetail> {
    require_main_window(window.label()).map_err(AppErrorResponse::from)?;
    let now = now_string();
    let entry = run_blocking(move || {
        let state = app.state::<AppState>();
        state.with_write_tx(|tx| EntriesRepo::update(tx, &id, patch, expected_revision, &now))
    })
    .await
    .map_err(AppErrorResponse::from)?;
    log::info!("entry_updated id={}", entry.id);
    Ok(entry)
}

#[tauri::command]
pub async fn entries_move_to_trash(
    app: AppHandle,
    window: WebviewWindow,
    id: String,
    expected_revision: i64,
) -> CommandResult<EntryDetail> {
    require_main_window(window.label()).map_err(AppErrorResponse::from)?;
    let now = now_string();
    run_blocking(move || {
        let state = app.state::<AppState>();
        state.with_write_tx(|tx| EntriesRepo::move_to_trash(tx, &id, expected_revision, &now))
    })
    .await
    .map_err(AppErrorResponse::from)
}

#[tauri::command]
pub async fn entries_restore_from_trash(
    app: AppHandle,
    window: WebviewWindow,
    id: String,
    expected_revision: i64,
) -> CommandResult<EntryDetail> {
    require_main_window(window.label()).map_err(AppErrorResponse::from)?;
    let now = now_string();
    run_blocking(move || {
        let state = app.state::<AppState>();
        state.with_write_tx(|tx| EntriesRepo::restore_from_trash(tx, &id, expected_revision, &now))
    })
    .await
    .map_err(AppErrorResponse::from)
}

#[tauri::command]
pub async fn entries_delete_forever(
    app: AppHandle,
    window: WebviewWindow,
    id: String,
) -> CommandResult<()> {
    require_main_window(window.label()).map_err(AppErrorResponse::from)?;
    run_blocking(move || {
        let state = app.state::<AppState>();
        state.with_write_tx(|tx| EntriesRepo::delete_forever(tx, &id))
    })
    .await
    .map_err(AppErrorResponse::from)
}

#[cfg(test)]
mod tests {
    use super::new_entry_spec;
    use crate::types::{documents::BlockKind, entries::TitleSource};

    #[test]
    fn new_entry_spec_creates_one_empty_paragraph() {
        let spec = new_entry_spec();

        assert_eq!(spec.title_source, TitleSource::Auto);
        assert!(spec.title.is_none());
        assert!(spec.original_content.is_empty());
        assert_eq!(spec.document.blocks.len(), 1);
        assert_eq!(spec.document.blocks[0].kind, BlockKind::Paragraph);
        assert!(spec.document.blocks[0].content.is_empty());
    }
}
