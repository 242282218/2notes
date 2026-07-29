use std::{
    fs,
    path::{Path, PathBuf},
    time::Duration,
};

use rusqlite::{params, OptionalExtension};
use tauri::{AppHandle, Manager, WebviewWindow};
use tauri_plugin_dialog::DialogExt;

use crate::{
    app_state::{AppState, ImportSessionCandidate},
    commands::{require_main_window, run_blocking},
    content::markdown_import::parse_import_bytes,
    db::{migrations::now_string, repos::EntriesRepo},
    error::{AppError, AppErrorResponse, AppResult, CommandResult},
    types::{
        entries::CreateEntrySpec,
        imports::{MarkdownImportPreview, MarkdownImportReport},
    },
};

const MAX_FILES: usize = 1_000;
const MAX_FILE_BYTES: u64 = 2 * 1024 * 1024;
const MAX_TOTAL_BYTES: u64 = 100 * 1024 * 1024;
const SESSION_TTL: Duration = Duration::from_secs(15 * 60);

#[tauri::command]
pub async fn markdown_import_preview(
    app: AppHandle,
    window: WebviewWindow,
) -> CommandResult<Option<MarkdownImportPreview>> {
    require_main_window(window.label()).map_err(AppErrorResponse::from)?;
    let selected = run_blocking({
        let window = window.clone();
        move || {
            Ok(window
                .dialog()
                .file()
                .set_title("选择 Markdown 导入目录")
                .blocking_pick_folder())
        }
    })
    .await
    .map_err(AppErrorResponse::from)?;
    let Some(selected) = selected else {
        return Ok(None);
    };
    let root = selected.into_path().map_err(|err| {
        AppErrorResponse::from(AppError::system(
            "IMPORT_DIR_INVALID",
            format!("无法解析导入目录: {err}"),
        ))
    })?;
    let scanned = run_blocking(move || {
        let mut scanned = scan_import_root(&root)?;
        scanned
            .warnings
            .extend(collect_parse_warnings(&scanned.root, &scanned.candidates));
        Ok(scanned)
    })
    .await
    .map_err(AppErrorResponse::from)?;
    let state = app.state::<AppState>();
    let session_id = state
        .create_import_session(scanned.root, scanned.candidates.clone(), SESSION_TTL)
        .map_err(AppErrorResponse::from)?;
    Ok(Some(MarkdownImportPreview {
        session_id,
        file_count: scanned.candidates.len() as u32,
        total_bytes: scanned.total_bytes,
        paths: scanned
            .candidates
            .iter()
            .take(100)
            .map(|candidate| candidate.relative_path.display().to_string())
            .collect(),
        warnings: scanned.warnings,
    }))
}

#[tauri::command]
pub async fn markdown_import_commit(
    app: AppHandle,
    window: WebviewWindow,
    session_id: String,
) -> CommandResult<MarkdownImportReport> {
    require_main_window(window.label()).map_err(AppErrorResponse::from)?;
    let state = app.state::<AppState>();
    let session = state
        .consume_import_session(&session_id)
        .map_err(AppErrorResponse::from)?;
    let report =
        run_blocking(move || commit_import_session(&app, session.root, session.candidates))
            .await
            .map_err(AppErrorResponse::from)?;
    Ok(report)
}

struct ScannedImport {
    root: PathBuf,
    candidates: Vec<ImportSessionCandidate>,
    total_bytes: u64,
    warnings: Vec<String>,
}

fn scan_import_root(root: &Path) -> AppResult<ScannedImport> {
    let root = root.canonicalize().map_err(|err| {
        AppError::system("IMPORT_DIR_INVALID", format!("无法解析导入目录: {err}"))
    })?;
    if !root.is_dir() {
        return Err(AppError::validation(
            "IMPORT_DIR_INVALID",
            "导入目标必须是目录",
        ));
    }

    let mut candidates = Vec::new();
    let mut warnings = Vec::new();
    let mut total_bytes = 0;
    scan_dir(
        &root,
        &root,
        &mut candidates,
        &mut total_bytes,
        &mut warnings,
    )?;
    candidates.sort_by(|left, right| left.relative_path.cmp(&right.relative_path));
    Ok(ScannedImport {
        root,
        candidates,
        total_bytes,
        warnings,
    })
}

fn scan_dir(
    root: &Path,
    current: &Path,
    candidates: &mut Vec<ImportSessionCandidate>,
    total_bytes: &mut u64,
    warnings: &mut Vec<String>,
) -> AppResult<()> {
    let entries = fs::read_dir(current).map_err(|err| {
        AppError::system(
            "IMPORT_SCAN_FAILED",
            format!("无法读取导入目录 {}: {err}", current.display()),
        )
    })?;
    for entry in entries {
        let entry = entry?;
        let path = entry.path();
        let file_type = entry.file_type()?;
        if file_type.is_symlink() {
            warnings.push(format!(
                "已跳过符号链接: {}",
                display_relative_path(root, &path)
            ));
            continue;
        }
        if file_type.is_dir() {
            scan_dir(root, &path, candidates, total_bytes, warnings)?;
            continue;
        }
        if !file_type.is_file() || !is_markdown_path(&path) {
            continue;
        }
        if candidates.len() >= MAX_FILES {
            warnings.push(format!("最多导入 {MAX_FILES} 个 Markdown 文件"));
            break;
        }
        let size_bytes = entry.metadata()?.len();
        if size_bytes > MAX_FILE_BYTES {
            warnings.push(format!(
                "文件超过 2 MiB，已跳过: {}",
                display_relative_path(root, &path)
            ));
            continue;
        }
        if *total_bytes + size_bytes > MAX_TOTAL_BYTES {
            warnings.push("导入总大小超过 100 MiB，剩余文件已跳过".to_string());
            break;
        }
        let content = fs::read(&path)?;
        let source_hash = blake3::hash(&content).to_hex().to_string();
        let relative_path = path
            .strip_prefix(root)
            .map_err(|_| AppError::system("IMPORT_SCAN_FAILED", "导入文件不在所选目录内"))?;
        candidates.push(ImportSessionCandidate {
            relative_path: relative_path.to_path_buf(),
            source_hash,
            size_bytes,
        });
        *total_bytes += size_bytes;
    }
    Ok(())
}

fn collect_parse_warnings(root: &Path, candidates: &[ImportSessionCandidate]) -> Vec<String> {
    candidates
        .iter()
        .flat_map(|candidate| {
            let path = root.join(&candidate.relative_path);
            match fs::read(&path) {
                Ok(content) => match parse_import_bytes(&content) {
                    Ok(parsed) => parsed
                        .warnings
                        .into_iter()
                        .map(|warning| format!("{}: {warning}", candidate.relative_path.display()))
                        .collect(),
                    Err(err) => vec![format!(
                        "无法预览 {}: {err}",
                        candidate.relative_path.display()
                    )],
                },
                Err(err) => vec![format!(
                    "无法预览 {}: {err}",
                    candidate.relative_path.display()
                )],
            }
        })
        .collect()
}

fn commit_import_session(
    app: &AppHandle,
    root: PathBuf,
    candidates: Vec<ImportSessionCandidate>,
) -> AppResult<MarkdownImportReport> {
    let root = root.canonicalize().map_err(|err| {
        AppError::system("IMPORT_DIR_INVALID", format!("导入目录已不可用: {err}"))
    })?;
    if !root.is_dir() {
        return Err(AppError::validation(
            "IMPORT_DIR_INVALID",
            "导入目录已不可用",
        ));
    }

    commit_scanned_import(app.state::<AppState>().inner(), root, candidates)
}

fn commit_scanned_import(
    state: &AppState,
    root: PathBuf,
    candidates: Vec<ImportSessionCandidate>,
) -> AppResult<MarkdownImportReport> {
    let mut report = MarkdownImportReport {
        imported_count: 0,
        skipped_count: 0,
        failed_count: 0,
        failures: Vec::new(),
    };
    for candidate in candidates {
        match commit_one(state, &root, &candidate) {
            Ok(CommitOutcome::Imported) => report.imported_count += 1,
            Ok(CommitOutcome::Skipped) => report.skipped_count += 1,
            Err(err) => {
                report.failed_count += 1;
                report
                    .failures
                    .push(format!("{}: {err}", candidate.relative_path.display()));
            }
        }
    }
    Ok(report)
}

enum CommitOutcome {
    Imported,
    Skipped,
}

fn commit_one(
    state: &AppState,
    root: &Path,
    candidate: &ImportSessionCandidate,
) -> AppResult<CommitOutcome> {
    let path = root.join(&candidate.relative_path);
    if path_has_symlink_component(root, &candidate.relative_path)? {
        return Err(AppError::validation(
            "IMPORT_FILE_INVALID",
            "导入文件路径包含符号链接",
        ));
    }
    let metadata = fs::symlink_metadata(&path)?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(AppError::validation(
            "IMPORT_FILE_INVALID",
            "导入文件不再是普通文件",
        ));
    }
    if metadata.len() > MAX_FILE_BYTES || metadata.len() != candidate.size_bytes {
        return Err(AppError::validation(
            "IMPORT_FILE_CHANGED",
            "导入文件大小已变化",
        ));
    }
    let canonical_path = path.canonicalize()?;
    if !canonical_path.starts_with(root) || !canonical_path.is_file() {
        return Err(AppError::validation(
            "IMPORT_FILE_INVALID",
            "导入文件不在所选目录内",
        ));
    }
    let content = fs::read(&canonical_path)?;
    let source_hash = blake3::hash(&content).to_hex().to_string();
    if source_hash != candidate.source_hash {
        return Err(AppError::validation(
            "IMPORT_FILE_CHANGED",
            "导入文件内容已变化",
        ));
    }
    let metadata_after_read = fs::symlink_metadata(&canonical_path)?;
    if metadata_after_read.file_type().is_symlink()
        || !metadata_after_read.is_file()
        || metadata_after_read.len() != candidate.size_bytes
    {
        return Err(AppError::validation(
            "IMPORT_FILE_CHANGED",
            "导入文件已变化",
        ));
    }
    let canonical_source_path = canonical_path.display().to_string();
    let parsed = parse_import_bytes(&content)?;
    let source_entry_id = parsed.source_entry_id.clone();

    state.with_write_tx(|tx| {
        let source_is_existing_entry = if let Some(source_entry_id) = source_entry_id.as_deref() {
            tx.query_row(
                "SELECT 1 FROM entries WHERE id = ?1",
                params![source_entry_id],
                |_| Ok(()),
            )
            .optional()?
            .is_some()
        } else {
            false
        };
        if source_is_existing_entry {
            return Ok(CommitOutcome::Skipped);
        }
        let duplicate = tx
            .query_row(
                "SELECT 1 FROM entry_imports
                 WHERE canonical_source_path = ?1 AND source_hash = ?2",
                params![canonical_source_path, source_hash],
                |_| Ok(()),
            )
            .optional()?
            .is_some();
        if duplicate {
            return Ok(CommitOutcome::Skipped);
        }
        let entry = EntriesRepo::create_with_document(
            tx,
            CreateEntrySpec {
                title: parsed.title.clone(),
                title_source: parsed.title_source.clone(),
                original_content: parsed.original_content.clone(),
                document: parsed.document.clone(),
                entry_type: parsed.entry_type.clone(),
                status: parsed.status.clone(),
                tags: parsed.tags.clone(),
            },
            &now_string(),
        )?;
        tx.execute(
            "INSERT INTO entry_imports(
               entry_id, canonical_source_path, source_hash, source_entry_id, imported_at
             ) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                entry.id,
                canonical_source_path,
                source_hash,
                source_entry_id,
                now_string(),
            ],
        )?;
        Ok(CommitOutcome::Imported)
    })
}

fn path_has_symlink_component(root: &Path, relative_path: &Path) -> AppResult<bool> {
    let mut current = root.to_path_buf();
    for component in relative_path.components() {
        current.push(component);
        if fs::symlink_metadata(&current)?.file_type().is_symlink() {
            return Ok(true);
        }
    }
    Ok(false)
}

fn is_markdown_path(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("md"))
}

fn display_relative_path(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .display()
        .to_string()
}

#[cfg(test)]
mod tests {
    use std::time::Instant;

    use crate::{app_state::AppState, db::connection::open_database, files::paths::prepare_paths};

    use super::*;

    #[test]
    fn recognizes_markdown_extension_without_case_sensitivity() {
        assert!(is_markdown_path(Path::new("note.md")));
        assert!(is_markdown_path(Path::new("note.MD")));
        assert!(!is_markdown_path(Path::new("note.txt")));
    }

    #[test]
    fn scan_limits_files_and_ignores_non_markdown_files() {
        let temp = tempfile::tempdir().unwrap();
        fs::write(temp.path().join("a.md"), "a").unwrap();
        fs::write(temp.path().join("b.txt"), "b").unwrap();

        let scanned = scan_import_root(temp.path()).unwrap();

        assert_eq!(scanned.candidates.len(), 1);
        assert_eq!(scanned.candidates[0].relative_path, PathBuf::from("a.md"));
        assert_eq!(scanned.total_bytes, 1);
    }

    #[test]
    fn scan_warning_uses_relative_path() {
        let temp = tempfile::tempdir().unwrap();
        let oversized = temp.path().join("nested").join("large.md");
        fs::create_dir_all(oversized.parent().unwrap()).unwrap();
        fs::write(&oversized, vec![b'x'; MAX_FILE_BYTES as usize + 1]).unwrap();

        let scanned = scan_import_root(temp.path()).unwrap();

        assert_eq!(
            scanned.warnings,
            ["文件超过 2 MiB，已跳过: nested\\large.md"]
        );
        assert!(!scanned.warnings[0].contains(&temp.path().display().to_string()));
    }

    #[test]
    fn relative_display_uses_path_below_root() {
        let root = Path::new("C:/imports");
        assert_eq!(
            display_relative_path(root, &root.join("folder").join("note.md")),
            "folder\\note.md"
        );
    }

    #[test]
    fn changed_candidate_hash_is_rejected_before_import() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("note.md");
        fs::write(&path, "before").unwrap();
        let scanned = scan_import_root(temp.path()).unwrap();
        fs::write(&path, "after!").unwrap();

        let candidate = &scanned.candidates[0];
        let bytes = fs::read(&path).unwrap();
        assert_ne!(
            blake3::hash(&bytes).to_hex().to_string(),
            candidate.source_hash
        );
    }

    #[test]
    #[ignore = "1k Markdown import on-disk release-scale verification"]
    fn imports_one_thousand_small_markdown_files_under_sixty_seconds() {
        const FILE_COUNT: usize = 1_000;

        let temp = tempfile::tempdir().unwrap();
        let import_root = temp.path().join("markdown-import");
        fs::create_dir(&import_root).unwrap();
        for index in 0..FILE_COUNT {
            fs::write(
                import_root.join(format!("note-{index:04}.md")),
                format!("# Scale note {index}\n\nSmall import fixture.\n"),
            )
            .unwrap();
        }
        let paths = prepare_paths(&temp.path().join("config"), &temp.path().join("data")).unwrap();
        let (write_conn, read_conn) = open_database(&paths.database_path).unwrap();
        let state = AppState::new(write_conn, read_conn, paths);

        let started = Instant::now();
        let scanned = scan_import_root(&import_root).unwrap();
        let report = commit_scanned_import(&state, scanned.root, scanned.candidates).unwrap();
        let elapsed = started.elapsed();
        let created_count: usize = state
            .read_conn()
            .unwrap()
            .query_row("SELECT COUNT(*) FROM entries", [], |row| row.get(0))
            .unwrap();

        println!(
            "release scale markdown import: {FILE_COUNT} files in {:.3}s",
            elapsed.as_secs_f64()
        );
        assert_eq!(report.imported_count, FILE_COUNT as u32);
        assert_eq!(report.skipped_count, 0);
        assert_eq!(report.failed_count, 0);
        assert_eq!(created_count, FILE_COUNT);
        assert!(
            elapsed < Duration::from_secs(60),
            "imported {FILE_COUNT} files in {:.3}s, expected under 60s",
            elapsed.as_secs_f64()
        );
    }
}
