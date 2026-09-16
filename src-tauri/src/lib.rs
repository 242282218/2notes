mod app_state;
mod commands;
pub mod content;
mod db;
mod error;
mod files;
pub mod knowledge;
mod system;
mod types;

use app_state::{AppState, BackupOperationGuard};
use db::connection::open_database_with_read_pool;
use files::{backups::ensure_daily_backup, paths::prepare_app_paths};
use tauri::Manager;
use tauri_plugin_autostart::MacosLauncher;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            system::windows::show_main_window(app);
        }))
        .plugin(tauri_plugin_log::Builder::new().build())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_window_state::Builder::default().build())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_autostart::init(
            MacosLauncher::LaunchAgent,
            Some(vec!["--from-autostart"]),
        ))
        .setup(|app| {
            let paths = prepare_app_paths(app.handle())?;
            let (write_conn, read_conns) = open_database_with_read_pool(&paths.database_path)?;
            app.manage(AppState::with_read_pool(write_conn, read_conns, paths));
            let backup_paths = app.state::<AppState>().paths.clone();
            let retention_count = app
                .state::<AppState>()
                .with_read_conn(|conn| {
                    crate::db::repos::SettingsRepo::get_backup_retention_count(conn)
                })
                .ok()
                .unwrap_or(crate::files::backups::DEFAULT_DAILY_RETENTION as i64)
                as usize;
            let backup_app = app.handle().clone();
            tauri::async_runtime::spawn_blocking(move || {
                let state = backup_app.state::<AppState>();
                let Some(_guard) = BackupOperationGuard::begin(&state).unwrap_or(None) else {
                    log::info!("daily_backup_skipped reason=backup_or_restore_in_progress");
                    return;
                };
                let result = ensure_daily_backup(
                    &backup_paths,
                    retention_count,
                    &crate::files::timestamps::now_string(),
                );
                if let Err(err) = result {
                    log::warn!("daily_backup_check_failed source={err}");
                }
            });
            system::tray::create_tray(app.handle())?;
            system::shortcuts::register_default_shortcut(app.handle());
            Ok(())
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                if window.label() == "main" || window.label() == "quick-capture" {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::entries::entries_create,
            commands::entries::entries_list,
            commands::entries::entries_get,
            commands::entries::entries_update,
            commands::entries::entries_move_to_trash,
            commands::entries::entries_restore_from_trash,
            commands::entries::entries_delete_forever,
            commands::knowledge::knowledge_suggest,
            commands::knowledge::knowledge_tree_get,
            commands::knowledge::knowledge_breadcrumbs_get,
            commands::knowledge::knowledge_health_summary_get,
            commands::knowledge::knowledge_health_issues_get,
            commands::knowledge::knowledge_relations_get,
            commands::knowledge::knowledge_rebuild_index,
            commands::knowledge::knowledge_promote,
            commands::knowledge::knowledge_move,
            commands::knowledge::knowledge_demote,
            commands::tags::tags_suggest,
            commands::tags::tags_list,
            commands::settings::settings_get,
            commands::settings::settings_update,
            commands::drafts::draft_get,
            commands::drafts::draft_update,
            commands::drafts::quick_capture_submit,
            commands::export_markdown::export_markdown,
            commands::import_markdown::markdown_import_preview,
            commands::import_markdown::markdown_import_commit,
            commands::backups::backups_create,
            commands::backups::backups_list,
            commands::backups::backups_restore,
            commands::backups::database_restore_ready,
            commands::windows::window_open_quick_capture,
            commands::windows::window_hide_quick_capture,
            commands::windows::app_quit_ready,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
