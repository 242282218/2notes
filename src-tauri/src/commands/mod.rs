pub mod backups;
pub mod drafts;
pub mod entries;
pub mod export_markdown;
pub mod import_markdown;
pub mod knowledge;
pub mod settings;
pub mod tags;
pub mod windows;

use crate::error::{AppError, AppResult};

pub fn require_main_window(label: &str) -> AppResult<()> {
    if label == "main" {
        return Ok(());
    }
    Err(AppError::validation(
        "COMMAND_FORBIDDEN",
        "当前窗口无权执行此操作",
    ))
}

pub async fn run_blocking<F, T>(task: F) -> AppResult<T>
where
    F: FnOnce() -> AppResult<T> + Send + 'static,
    T: Send + 'static,
{
    tauri::async_runtime::spawn_blocking(task)
        .await
        .map_err(|err| AppError::system("BLOCKING_TASK_FAILED", err.to_string()))?
}

#[cfg(test)]
mod tests {
    use super::require_main_window;
    use crate::error::AppError;

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum CommandClass {
        MainOnly,
        SharedWithQuickCapture,
    }

    fn classify_command(name: &str) -> Option<CommandClass> {
        match name {
            "entries_create"
            | "entries_list"
            | "entries_get"
            | "entries_update"
            | "entries_move_to_trash"
            | "entries_restore_from_trash"
            | "entries_delete_forever"
            | "knowledge_suggest"
            | "knowledge_tree_get"
            | "knowledge_breadcrumbs_get"
            | "knowledge_health_summary_get"
            | "knowledge_health_issues_get"
            | "knowledge_relations_get"
            | "knowledge_rebuild_index"
            | "knowledge_promote"
            | "knowledge_move"
            | "knowledge_demote"
            | "tags_suggest"
            | "tags_list"
            | "settings_get"
            | "settings_update"
            | "export_markdown"
            | "markdown_import_preview"
            | "markdown_import_commit"
            | "backups_create"
            | "backups_list"
            | "backups_restore"
            | "window_open_quick_capture" => Some(CommandClass::MainOnly),
            "draft_get"
            | "draft_update"
            | "quick_capture_submit"
            | "window_hide_quick_capture"
            | "database_restore_ready"
            | "app_quit_ready" => Some(CommandClass::SharedWithQuickCapture),
            _ => None,
        }
    }

    fn side_effect_command_allowed(window_label: &str, command: &str) -> bool {
        match classify_command(command) {
            Some(CommandClass::MainOnly) => require_main_window(window_label).is_ok(),
            Some(CommandClass::SharedWithQuickCapture) => {
                window_label == "main" || window_label == "quick-capture"
            }
            None => false,
        }
    }

    fn assert_forbidden(result: Result<(), AppError>) {
        let err = result.expect_err("expected COMMAND_FORBIDDEN");
        assert!(matches!(err, AppError::Validation { code, .. } if code == "COMMAND_FORBIDDEN"));
    }

    #[test]
    fn blocking_command_returns_the_task_result() {
        let result = tauri::async_runtime::block_on(super::run_blocking(|| Ok(42))).unwrap();

        assert_eq!(result, 42);
    }

    #[test]
    fn main_window_guard_accepts_main_and_rejects_quick_capture() {
        assert!(require_main_window("main").is_ok());
        assert_forbidden(require_main_window("quick-capture"));
    }

    #[test]
    fn main_window_guard_rejects_unknown_windows() {
        assert_forbidden(require_main_window("settings"));
        assert_forbidden(require_main_window(""));
        assert_forbidden(require_main_window("MAIN"));
    }

    #[test]
    fn main_only_commands_reject_quick_capture() {
        let main_only = [
            "entries_create",
            "entries_list",
            "entries_get",
            "entries_update",
            "entries_move_to_trash",
            "entries_restore_from_trash",
            "entries_delete_forever",
            "knowledge_suggest",
            "knowledge_tree_get",
            "knowledge_breadcrumbs_get",
            "knowledge_health_summary_get",
            "knowledge_health_issues_get",
            "knowledge_relations_get",
            "knowledge_rebuild_index",
            "knowledge_promote",
            "knowledge_move",
            "knowledge_demote",
            "tags_suggest",
            "tags_list",
            "settings_get",
            "settings_update",
            "export_markdown",
            "markdown_import_preview",
            "markdown_import_commit",
            "backups_create",
            "backups_list",
            "backups_restore",
            "window_open_quick_capture",
        ];

        for command in main_only {
            assert!(
                side_effect_command_allowed("main", command),
                "{command} should allow main"
            );
            assert!(
                !side_effect_command_allowed("quick-capture", command),
                "{command} should reject quick-capture"
            );
        }
    }

    #[test]
    fn draft_restore_ready_and_hide_allow_quick_capture() {
        let shared = [
            "draft_get",
            "draft_update",
            "quick_capture_submit",
            "window_hide_quick_capture",
            "database_restore_ready",
            "app_quit_ready",
        ];

        for command in shared {
            assert!(
                side_effect_command_allowed("quick-capture", command),
                "{command} should allow quick-capture"
            );
            assert!(
                side_effect_command_allowed("main", command),
                "{command} should still allow main unless a command-specific guard tightens it"
            );
        }
    }

    #[test]
    fn unknown_window_rejects_all_side_effect_commands() {
        let commands = [
            "entries_update",
            "export_markdown",
            "backups_restore",
            "draft_update",
            "quick_capture_submit",
            "window_hide_quick_capture",
            "database_restore_ready",
            "app_quit_ready",
            "not_a_real_command",
        ];

        for command in commands {
            assert!(
                !side_effect_command_allowed("popup", command),
                "{command} must reject unknown windows"
            );
        }
    }

    #[test]
    fn knowledge_commands_reject_quick_capture_window() {
        let err = require_main_window("quick-capture").unwrap_err();
        assert!(matches!(err, AppError::Validation { code, .. } if code == "COMMAND_FORBIDDEN"));
    }

    #[test]
    fn every_known_command_is_classified() {
        let known = [
            "entries_create",
            "entries_list",
            "entries_get",
            "entries_update",
            "entries_move_to_trash",
            "entries_restore_from_trash",
            "entries_delete_forever",
            "knowledge_suggest",
            "knowledge_tree_get",
            "knowledge_breadcrumbs_get",
            "knowledge_health_summary_get",
            "knowledge_health_issues_get",
            "knowledge_relations_get",
            "knowledge_rebuild_index",
            "knowledge_promote",
            "knowledge_move",
            "knowledge_demote",
            "tags_suggest",
            "tags_list",
            "settings_get",
            "settings_update",
            "draft_get",
            "draft_update",
            "quick_capture_submit",
            "export_markdown",
            "markdown_import_preview",
            "markdown_import_commit",
            "backups_create",
            "backups_list",
            "backups_restore",
            "database_restore_ready",
            "window_open_quick_capture",
            "window_hide_quick_capture",
            "app_quit_ready",
        ];

        for command in known {
            assert!(
                classify_command(command).is_some(),
                "missing classification for {command}"
            );
        }
        assert!(classify_command("brand_new_command").is_none());
    }
}
