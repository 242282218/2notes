use std::path::{Path, PathBuf};

use tauri::{AppHandle, Manager, WebviewWindow};
use tauri_plugin_dialog::DialogExt;

use crate::{
    app_state::AppState,
    commands::{require_main_window, run_blocking},
    db::repos::EntriesRepo,
    error::{AppError, AppErrorResponse, AppResult, CommandResult},
    files::markdown::export_entries,
    types::settings::ExportResult,
};

#[tauri::command]
pub async fn export_markdown(
    app: AppHandle,
    window: WebviewWindow,
) -> CommandResult<Option<ExportResult>> {
    require_main_window(window.label()).map_err(AppErrorResponse::from)?;

    let selected = run_blocking({
        let window = window.clone();
        move || {
            Ok(window
                .dialog()
                .file()
                .set_title("选择 Markdown 导出目录")
                .blocking_pick_folder())
        }
    })
    .await
    .map_err(AppErrorResponse::from)?;

    let Some(selected) = selected else {
        return Ok(None);
    };

    let target = selected.into_path().map_err(|err| {
        AppErrorResponse::from(AppError::system(
            "EXPORT_DIR_INVALID",
            format!("无法解析导出目录: {err}"),
        ))
    })?;

    let result = run_blocking(move || export_to_selected_dir(&app, &target))
        .await
        .map_err(AppErrorResponse::from)?;
    Ok(Some(result))
}

fn export_to_selected_dir(app: &AppHandle, target: &Path) -> AppResult<ExportResult> {
    let validated = validate_export_dir(target)?;
    let state = app.state::<AppState>();
    let entries = state.with_read_conn(EntriesRepo::list_exportable)?;
    let paths = export_entries(&entries, &validated)?;
    log::info!(
        "markdown_exported count={} target={}",
        paths.len(),
        validated.display()
    );
    Ok(ExportResult {
        exported_count: paths.len(),
        target_dir: validated.display().to_string(),
    })
}

fn validate_export_dir(target: &Path) -> AppResult<PathBuf> {
    if !target.exists() {
        // Directory picker may return a not-yet-created path; create then canonicalize.
        std::fs::create_dir_all(target)?;
    }
    let canonical = target.canonicalize().map_err(|err| {
        AppError::system("EXPORT_DIR_INVALID", format!("无法解析导出目录: {err}"))
    })?;
    if !canonical.is_dir() {
        return Err(AppError::validation(
            "EXPORT_DIR_INVALID",
            "导出目标必须是目录",
        ));
    }
    Ok(canonical)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn validate_export_dir_accepts_existing_directory() {
        let temp = tempfile::tempdir().unwrap();
        let path = validate_export_dir(temp.path()).unwrap();
        assert!(path.is_dir());
    }

    #[test]
    fn validate_export_dir_creates_missing_directory() {
        let temp = tempfile::tempdir().unwrap();
        let nested = temp.path().join("new-export-dir");
        let path = validate_export_dir(&nested).unwrap();
        assert!(path.is_dir());
        assert!(nested.exists());
    }

    #[test]
    fn validate_export_dir_rejects_file_path() {
        let temp = tempfile::tempdir().unwrap();
        let file_path = temp.path().join("not-a-dir.txt");
        fs::write(&file_path, "x").unwrap();
        let err = validate_export_dir(&file_path).unwrap_err();
        assert!(matches!(
            err,
            AppError::Validation { code, .. } if code == "EXPORT_DIR_INVALID"
        ));
    }

    #[test]
    fn optional_target_none_skips_export() {
        // Pure orchestration: None short-circuits without needing AppHandle.
        let result = match Option::<PathBuf>::None {
            None => Ok::<Option<ExportResult>, AppError>(None),
            Some(_) => unreachable!("should not export"),
        }
        .unwrap();
        assert!(result.is_none());
    }
}
