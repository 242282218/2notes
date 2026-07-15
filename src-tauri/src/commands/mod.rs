pub mod backups;
pub mod drafts;
pub mod entries;
pub mod export_markdown;
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
    #[test]
    fn blocking_command_returns_the_task_result() {
        let result = tauri::async_runtime::block_on(super::run_blocking(|| Ok(42))).unwrap();

        assert_eq!(result, 42);
    }

    #[test]
    fn main_window_guard_rejects_quick_capture() {
        assert!(super::require_main_window("main").is_ok());
        assert!(super::require_main_window("quick-capture").is_err());
    }

    #[test]
    fn knowledge_commands_reject_quick_capture_window() {
        let err = super::require_main_window("quick-capture").unwrap_err();
        assert!(
            matches!(err, crate::error::AppError::Validation { code, .. } if code == "COMMAND_FORBIDDEN")
        );
    }
}
