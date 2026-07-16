use std::{
    ffi::OsString,
    fs,
    path::{Path, PathBuf},
};

use rusqlite::{params, Connection};

use crate::{
    app_state::AppState,
    db::{connection::open_database, repos::KnowledgeRepo},
    error::{AppError, AppResult},
    files::{paths::AppPaths, timestamps::now_string},
    types::{backups::BackupInfo, knowledge::KnowledgeIndexReport},
};

const BACKUP_PREFIX: &str = "2notes";
const BACKUP_EXT: &str = "sqlite";
const ALLOWED_KINDS: &[&str] = &["manual", "daily", "before_restore"];

pub fn create_backup(paths: &AppPaths, conn: &Connection, kind: &str) -> AppResult<BackupInfo> {
    let kind = normalize_kind(kind)?;
    fs::create_dir_all(&paths.backup_dir)?;
    let target = unique_backup_path(&paths.backup_dir, kind);
    let target_raw = target.display().to_string();
    conn.execute("VACUUM main INTO ?1", params![target_raw])?;
    backup_info(&target, kind)
}

pub fn list_backups(paths: &AppPaths) -> AppResult<Vec<BackupInfo>> {
    fs::create_dir_all(&paths.backup_dir)?;
    let mut backups = Vec::new();
    for entry in fs::read_dir(&paths.backup_dir)? {
        let path = entry?.path();
        if path.extension().and_then(|ext| ext.to_str()) != Some(BACKUP_EXT) {
            continue;
        }
        backups.push(backup_info(&path, infer_kind(&path).unwrap_or("manual"))?);
    }
    backups.sort_by(|left, right| right.file_name.cmp(&left.file_name));
    Ok(backups)
}

pub fn restore_backup(state: &AppState, requested_path: &str) -> AppResult<BackupInfo> {
    let paths = state.paths.clone();
    let source = validate_backup_path(&paths, requested_path)?;
    let restore_temp = paths.database_path.with_extension("restore.tmp");
    remove_database_files(&restore_temp)?;

    // Phase 1: All file I/O happens OUTSIDE the lock to avoid holding MutexGuard
    // during disk operations and prevent Mutex poisoning from panics.
    fs::copy(&source, &restore_temp)?;
    validate_restore_candidate(&restore_temp)?;

    let restore_result = {
        let (mut current_write, mut current_read) = state.database_pair()?;
        let before_restore = create_backup(&paths, &current_write, "before_restore")?;
        let snapshot_path: PathBuf = PathBuf::from(&before_restore.path);
        let rollback_path = paths.database_path.with_extension("restore.bak");
        let placeholder_write = Connection::open_in_memory()?;
        let placeholder_read = Connection::open_in_memory()?;
        let (old_write, old_read) = swap_live_connections(
            &mut current_write,
            &mut current_read,
            placeholder_write,
            placeholder_read,
        );
        drop(old_read);
        drop(old_write);

        match replace_database_file(&paths.database_path, &restore_temp) {
            Ok(_) => match reopen_restored_database(&paths.database_path) {
                Ok((write_conn, read_conn, report)) => {
                    log::info!(
                        "backup_restore_indexes_rebuilt indexed_sources={} links={} unresolved={} search={}",
                        report.indexed_sources,
                        report.link_occurrences,
                        report.unresolved_occurrences,
                        report.search_index_available,
                    );
                    *current_write = write_conn;
                    *current_read = read_conn;
                    if let Err(err) = cleanup_rollback(&rollback_path) {
                        log::warn!("backup_restore_cleanup_failed source={err}");
                    }
                    Ok(())
                }
                Err(err) => {
                    log::error!("backup_restore_reopen_failed source={err}");
                    match recover_live_connections(
                        &paths,
                        &rollback_path,
                        &snapshot_path,
                        &mut current_write,
                        &mut current_read,
                    ) {
                        Ok(()) => Err(err),
                        Err(recovery_err) => Err(recovery_err),
                    }
                }
            },
            Err(err) => {
                match recover_live_connections(
                    &paths,
                    &rollback_path,
                    &snapshot_path,
                    &mut current_write,
                    &mut current_read,
                ) {
                    Ok(()) => Err(err),
                    Err(recovery_err) => Err(recovery_err),
                }
            }
        }
    };

    if let Err(err) = remove_if_exists(&restore_temp) {
        log::warn!("backup_restore_temp_cleanup_failed source={err}");
    }
    restore_result?;

    backup_info(&source, infer_kind(&source).unwrap_or("manual"))
}

fn swap_live_connections(
    current_write: &mut Connection,
    current_read: &mut Connection,
    placeholder_write: Connection,
    placeholder_read: Connection,
) -> (Connection, Connection) {
    (
        std::mem::replace(current_write, placeholder_write),
        std::mem::replace(current_read, placeholder_read),
    )
}

fn recover_live_connections(
    paths: &AppPaths,
    rollback_path: &Path,
    snapshot_path: &Path,
    current_write: &mut Connection,
    current_read: &mut Connection,
) -> AppResult<()> {
    let mut recovery_error = None;
    if rollback_path.is_file() {
        match rollback_database_file(&paths.database_path, rollback_path)
            .and_then(|_| open_database(&paths.database_path))
        {
            Ok((write_conn, read_conn)) => {
                *current_write = write_conn;
                *current_read = read_conn;
                return Ok(());
            }
            Err(err) => {
                log::error!("backup_restore_rollback_recovery_failed source={err}");
                recovery_error = Some(err);
            }
        }
    }
    if snapshot_path.is_file() {
        match restore_from_snapshot(&paths.database_path, snapshot_path)
            .and_then(|_| open_database(&paths.database_path))
        {
            Ok((write_conn, read_conn)) => {
                *current_write = write_conn;
                *current_read = read_conn;
                return Ok(());
            }
            Err(err) => {
                log::error!("backup_restore_snapshot_recovery_failed source={err}");
                recovery_error = Some(err);
            }
        }
    }
    if paths.database_path.is_file() {
        match open_database(&paths.database_path) {
            Ok((write_conn, read_conn)) => {
                *current_write = write_conn;
                *current_read = read_conn;
                return Ok(());
            }
            Err(err) => {
                log::error!("backup_restore_current_recovery_failed source={err}");
                recovery_error = Some(err);
            }
        }
    }
    Err(recovery_error
        .unwrap_or_else(|| AppError::system("BACKUP_RECOVERY_FAILED", "恢复数据库连接失败")))
}

fn reopen_restored_database(
    database_path: &Path,
) -> AppResult<(Connection, Connection, KnowledgeIndexReport)> {
    let (mut write_conn, read_conn) = open_database(database_path)?;
    let report = KnowledgeRepo::rebuild_all_indexes(&mut write_conn)?;
    Ok((write_conn, read_conn, report))
}

fn normalize_kind(kind: &str) -> AppResult<&'static str> {
    ALLOWED_KINDS
        .iter()
        .copied()
        .find(|allowed| *allowed == kind)
        .ok_or_else(|| AppError::validation("BACKUP_KIND_INVALID", "备份类型无效"))
}

fn unique_backup_path(backup_dir: &Path, kind: &str) -> PathBuf {
    // Use shared timestamp helper for consistent naming across backups and exports.
    let timestamp = crate::files::timestamps::backup_timestamp(&now_string());
    let base_name = format!("{BACKUP_PREFIX}-{timestamp}-{kind}");
    let mut candidate = backup_dir.join(format!("{base_name}.{BACKUP_EXT}"));
    let mut count = 1;
    while candidate.exists() {
        count += 1;
        candidate = backup_dir.join(format!("{base_name}-{count}.{BACKUP_EXT}"));
    }
    candidate
}

fn validate_backup_path(paths: &AppPaths, requested_path: &str) -> AppResult<PathBuf> {
    let requested = PathBuf::from(requested_path);
    if !requested.exists() {
        return Err(AppError::validation("BACKUP_NOT_FOUND", "备份文件不存在"));
    }
    let backup_dir = fs::canonicalize(&paths.backup_dir)?;
    let resolved = fs::canonicalize(&requested)?;
    if !resolved.starts_with(&backup_dir) {
        return Err(AppError::validation(
            "BACKUP_OUTSIDE_DIR",
            "只能恢复备份目录中的文件",
        ));
    }
    if resolved.extension().and_then(|ext| ext.to_str()) != Some(BACKUP_EXT) {
        return Err(AppError::validation(
            "BACKUP_EXTENSION_INVALID",
            "只能恢复 sqlite 备份文件",
        ));
    }
    Ok(resolved)
}

fn validate_sqlite_database(path: &Path) -> AppResult<()> {
    let conn = Connection::open(path)?;
    let result: String = conn.query_row("PRAGMA integrity_check", [], |row| row.get(0))?;
    if result != "ok" {
        return Err(AppError::validation(
            "BACKUP_INTEGRITY_FAILED",
            "备份文件完整性校验失败",
        ));
    }
    Ok(())
}

fn validate_restore_candidate(path: &Path) -> AppResult<()> {
    validate_sqlite_database(path)?;
    validate_2notes_schema(path)?;
    let (write_conn, read_conn) = open_database(path)?;
    drop(read_conn);
    write_conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")?;
    drop(write_conn);
    remove_if_exists(&sidecar_path(path, "-wal"))?;
    remove_if_exists(&sidecar_path(path, "-shm"))?;
    Ok(())
}

fn validate_2notes_schema(path: &Path) -> AppResult<()> {
    let conn = Connection::open(path)?;
    let required_tables: i64 = conn.query_row(
        "
        SELECT COUNT(*)
        FROM sqlite_master
        WHERE type = 'table'
          AND name IN (
            'schema_migrations', 'entries', 'drafts',
            'settings', 'tags', 'entry_tags'
          )
        ",
        [],
        |row| row.get(0),
    )?;
    let migration_version: Option<i64> = conn
        .query_row("SELECT MAX(version) FROM schema_migrations", [], |row| {
            row.get(0)
        })
        .ok();
    if required_tables != 6 || migration_version.is_none() {
        return Err(AppError::validation(
            "BACKUP_SCHEMA_INVALID",
            "备份文件不是有效的 2notes 数据库",
        ));
    }
    Ok(())
}

fn replace_database_file(database_path: &Path, restore_temp: &Path) -> AppResult<PathBuf> {
    let rollback_path = database_path.with_extension("restore.bak");
    remove_database_files(&rollback_path)?;
    remove_if_exists(&sidecar_path(database_path, "-wal"))?;
    remove_if_exists(&sidecar_path(database_path, "-shm"))?;
    if database_path.exists() {
        fs::rename(database_path, &rollback_path)?;
    }
    if let Err(err) = fs::rename(restore_temp, database_path) {
        if let Err(rollback_err) = rollback_database_file(database_path, &rollback_path) {
            log::error!("backup_restore_rollback_failed source={rollback_err}");
        }
        return Err(err.into());
    }
    Ok(rollback_path)
}

fn rollback_database_file(database_path: &Path, rollback_path: &Path) -> AppResult<()> {
    remove_database_files(database_path)?;
    if rollback_path.exists() {
        fs::rename(rollback_path, database_path)?;
    }
    Ok(())
}

fn restore_from_snapshot(database_path: &Path, snapshot_path: &Path) -> AppResult<()> {
    remove_database_files(database_path)?;
    fs::copy(snapshot_path, database_path)?;
    Ok(())
}

fn cleanup_rollback(rollback_path: &Path) -> AppResult<()> {
    remove_database_files(rollback_path)
}

fn remove_database_files(database_path: &Path) -> AppResult<()> {
    remove_if_exists(database_path)?;
    remove_if_exists(&sidecar_path(database_path, "-wal"))?;
    remove_if_exists(&sidecar_path(database_path, "-shm"))?;
    Ok(())
}

fn remove_if_exists(path: &Path) -> AppResult<()> {
    if path.exists() {
        fs::remove_file(path)?;
    }
    Ok(())
}

fn sidecar_path(path: &Path, suffix: &str) -> PathBuf {
    let mut raw = OsString::from(path.as_os_str());
    raw.push(suffix);
    PathBuf::from(raw)
}

fn backup_info(path: &Path, kind: &str) -> AppResult<BackupInfo> {
    let metadata = fs::metadata(path)?;
    let file_name = path
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_else(|| "backup.sqlite".to_string());
    let created_at = {
        let from_name = created_at_from_name(&file_name);
        if from_name != "1970-01-01T00:00:00Z" {
            from_name
        } else {
            file_modified_to_rfc3339(&metadata).unwrap_or_else(now_string)
        }
    };
    Ok(BackupInfo {
        path: path.display().to_string(),
        file_name: file_name.clone(),
        kind: kind.to_string(),
        created_at,
        size_bytes: metadata.len(),
    })
}

/// Fallback: convert file modification time to RFC3339 string.
fn file_modified_to_rfc3339(metadata: &fs::Metadata) -> Option<String> {
    use time::macros::format_description;

    let duration = metadata.modified().ok()?;
    let since_epoch = duration
        .duration_since(std::time::SystemTime::UNIX_EPOCH)
        .ok()?;
    let dt = time::OffsetDateTime::from_unix_timestamp(since_epoch.as_secs() as i64).ok()?;
    let fmt = format_description!("[year]-[month]-[day]T[hour]:[minute]:[second]Z");
    dt.format(&fmt).ok()
}

fn infer_kind(path: &Path) -> Option<&'static str> {
    let name = path.file_stem()?.to_string_lossy();
    ALLOWED_KINDS.iter().copied().find(|kind| {
        let marker = format!("-{kind}");
        name.ends_with(&marker)
            || name.rsplit_once('-').is_some_and(|(prefix, suffix)| {
                prefix.ends_with(&marker) && suffix.parse::<u32>().is_ok()
            })
    })
}

fn created_at_from_name(file_name: &str) -> String {
    use time::{format_description::well_known::Rfc3339, macros::format_description};

    let Some(value) = file_name
        .strip_prefix(&format!("{BACKUP_PREFIX}-"))
        .and_then(|value| value.strip_suffix(&format!(".{BACKUP_EXT}")))
    else {
        return "1970-01-01T00:00:00Z".to_string();
    };
    let Some(stamp) = value.get(..15) else {
        return "1970-01-01T00:00:00Z".to_string();
    };
    let format = format_description!("[year][month][day]-[hour][minute][second]");
    let Ok(value) = time::PrimitiveDateTime::parse(stamp, &format) else {
        return "1970-01-01T00:00:00Z".to_string();
    };
    value
        .assume_utc()
        .format(&Rfc3339)
        .unwrap_or_else(|_| "1970-01-01T00:00:00Z".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        app_state::AppState,
        db::{
            connection::{open_database, open_in_memory},
            migrations::{checksum, now_string},
            repos::{EntriesRepo, KnowledgeRepo},
        },
        types::{
            entries::{EntryListFilter, EntryPatch, PageRequest},
            knowledge::KnowledgeState,
        },
    };

    #[test]
    fn creates_and_lists_sqlite_backup() {
        let paths = test_paths();
        fs::create_dir_all(&paths.data_dir).unwrap();
        let (conn, _) = open_database(&paths.database_path).unwrap();

        let backup = create_backup(&paths, &conn, "manual").unwrap();
        let backups = list_backups(&paths).unwrap();

        assert!(PathBuf::from(&backup.path).exists());
        assert_eq!(backups.len(), 1);
        assert_eq!(backups[0].kind, "manual");
    }

    #[test]
    fn restore_replaces_current_database() {
        let paths = test_paths();
        fs::create_dir_all(&paths.data_dir).unwrap();
        let (mut write_conn, read_conn) = open_database(&paths.database_path).unwrap();
        let now = now_string();
        let tx = write_conn.transaction().unwrap();
        EntriesRepo::create(&tx, "before backup", &now).unwrap();
        tx.commit().unwrap();
        let backup = create_backup(&paths, &write_conn, "manual").unwrap();
        let tx = write_conn.transaction().unwrap();
        EntriesRepo::create(&tx, "after backup", &now).unwrap();
        tx.commit().unwrap();
        let state = AppState::new(write_conn, read_conn, paths);

        restore_backup(&state, &backup.path).unwrap();

        let conn = state.read_conn().unwrap();
        let page = EntriesRepo::list(&conn, &default_filter(), &default_page()).unwrap();
        assert_eq!(page.items.len(), 1);
        assert_eq!(page.items[0].summary, "before backup");
    }

    #[test]
    fn restores_legacy_v2_backup_and_migrates_knowledge_schema() {
        let paths = test_paths();
        fs::create_dir_all(&paths.data_dir).unwrap();
        fs::create_dir_all(&paths.backup_dir).unwrap();
        let (write_conn, read_conn) = open_database(&paths.database_path).unwrap();
        let legacy = paths.backup_dir.join("2notes-legacy-manual.sqlite");
        create_legacy_v2_database(&legacy);
        let state = AppState::new(write_conn, read_conn, paths);

        restore_backup(&state, &legacy.display().to_string()).unwrap();

        let conn = state.read_conn().unwrap();
        let version: i64 = conn
            .query_row("SELECT MAX(version) FROM schema_migrations", [], |row| {
                row.get(0)
            })
            .unwrap();
        let row: (String, String, i64, String) = conn
            .query_row(
                "SELECT current_content, original_content, revision, knowledge_state
                 FROM entries WHERE id = 'legacy-entry'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .unwrap();
        let knowledge_tables: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master
                 WHERE type = 'table' AND name IN ('entry_aliases', 'entry_links')",
                [],
                |row| row.get(0),
            )
            .unwrap();

        assert_eq!(version, 4);
        assert_eq!(
            row,
            (
                "current legacy".into(),
                "original legacy".into(),
                7,
                "capture".into()
            )
        );
        assert_eq!(knowledge_tables, 2);
    }

    #[test]
    fn restore_rebuilds_knowledge_indexes_from_truth_data() {
        let paths = test_paths();
        fs::create_dir_all(&paths.data_dir).unwrap();
        let (mut write_conn, read_conn) = open_database(&paths.database_path).unwrap();
        let now = now_string();
        let tx = write_conn.transaction().unwrap();
        let target = EntriesRepo::create(&tx, "旧标题", &now).unwrap();
        let target = KnowledgeRepo::promote(&tx, &target.id, target.revision, &now).unwrap();
        let target = EntriesRepo::update(
            &tx,
            &target.id,
            EntryPatch {
                title: Some("新标题".to_string()),
                current_content: None,
                entry_type: None,
                status: None,
                tags: None,
            },
            target.revision,
            &now,
        )
        .unwrap();
        let source = EntriesRepo::create(&tx, "[[旧标题]] 再次 [[旧标题]]", &now).unwrap();
        tx.commit().unwrap();
        let backup = create_backup(&paths, &write_conn, "manual").unwrap();
        let backup_conn = Connection::open(&backup.path).unwrap();
        backup_conn.execute("DELETE FROM entry_links", []).unwrap();
        backup_conn
            .execute(
                "INSERT INTO entry_links(
                   source_entry_id, ordinal, raw_target, normalized_target, target_entry_id
                 ) VALUES (?1, 99, '坏数据', '坏数据', NULL)",
                [&source.id],
            )
            .unwrap();
        backup_conn.execute("DELETE FROM entries_fts", []).unwrap();
        backup_conn
            .execute(
                "INSERT INTO entries_fts(
                   entry_id, title, original_content, current_content, tags_text, aliases_text
                 ) VALUES ('bogus', '坏数据', '坏数据', '坏数据', '', '')",
                [],
            )
            .unwrap();
        drop(backup_conn);
        let state = AppState::new(write_conn, read_conn, paths);

        restore_backup(&state, &backup.path).unwrap();

        let conn = state.read_conn().unwrap();
        let restored = EntriesRepo::get(&conn, &target.id).unwrap();
        let relations = KnowledgeRepo::relations(&conn, &target.id).unwrap();
        let mut filter = default_filter();
        filter.query = Some("旧标题".to_string());
        filter.knowledge_state = Some(KnowledgeState::Knowledge);
        let search = EntriesRepo::list(&conn, &filter, &default_page()).unwrap();
        let bogus_links: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM entry_links WHERE raw_target = '坏数据'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let bogus_search: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM entries_fts WHERE entry_id = 'bogus'",
                [],
                |row| row.get(0),
            )
            .unwrap();

        assert_eq!(restored.knowledge_state, KnowledgeState::Knowledge);
        assert_eq!(restored.knowledge_aliases, vec!["旧标题"]);
        assert_eq!(relations.backlinks.len(), 1);
        assert_eq!(relations.backlinks[0].id, source.id);
        assert_eq!(relations.backlinks[0].occurrence_count, 2);
        assert!(search.items.iter().any(|entry| entry.id == target.id));
        assert_eq!(bogus_links, 0);
        assert_eq!(bogus_search, 0);
    }

    #[test]
    fn restore_rebuild_failure_rolls_back_live_database() {
        let paths = test_paths();
        fs::create_dir_all(&paths.data_dir).unwrap();
        let (mut write_conn, read_conn) = open_database(&paths.database_path).unwrap();
        let tx = write_conn.transaction().unwrap();
        EntriesRepo::create(&tx, "live entry", &now_string()).unwrap();
        tx.commit().unwrap();
        let backup = create_backup(&paths, &write_conn, "manual").unwrap();
        let tx = write_conn.transaction().unwrap();
        EntriesRepo::create(&tx, "live only after backup", &now_string()).unwrap();
        tx.commit().unwrap();
        let backup_conn = Connection::open(&backup.path).unwrap();
        backup_conn.execute("DROP TABLE entry_links", []).unwrap();
        drop(backup_conn);
        let state = AppState::new(write_conn, read_conn, paths);

        let result = restore_backup(&state, &backup.path);

        assert!(result.is_err());
        let conn = state.read_conn().unwrap();
        let page = EntriesRepo::list(&conn, &default_filter(), &default_page()).unwrap();
        assert_eq!(page.items.len(), 2);
        assert!(page
            .items
            .iter()
            .any(|entry| entry.summary == "live only after backup"));
    }

    #[test]
    fn restore_rejects_invalid_backup_without_replacing_current_database() {
        let paths = test_paths();
        fs::create_dir_all(&paths.data_dir).unwrap();
        fs::create_dir_all(&paths.backup_dir).unwrap();
        let (mut write_conn, read_conn) = open_database(&paths.database_path).unwrap();
        let tx = write_conn.transaction().unwrap();
        EntriesRepo::create(&tx, "still here", &now_string()).unwrap();
        tx.commit().unwrap();
        let invalid_backup = paths.backup_dir.join("2notes-invalid-manual.sqlite");
        fs::write(&invalid_backup, "not sqlite").unwrap();
        let state = AppState::new(write_conn, read_conn, paths);

        let result = restore_backup(&state, &invalid_backup.display().to_string());

        assert!(result.is_err());
        let conn = state.read_conn().unwrap();
        let page = EntriesRepo::list(&conn, &default_filter(), &default_page()).unwrap();
        assert_eq!(page.items.len(), 1);
        assert_eq!(page.items[0].summary, "still here");
    }

    #[test]
    fn restore_rejects_empty_sqlite_file() {
        let paths = test_paths();
        fs::create_dir_all(&paths.data_dir).unwrap();
        fs::create_dir_all(&paths.backup_dir).unwrap();
        let (mut write_conn, read_conn) = open_database(&paths.database_path).unwrap();
        let tx = write_conn.transaction().unwrap();
        EntriesRepo::create(&tx, "still here", &now_string()).unwrap();
        tx.commit().unwrap();
        let empty_backup = paths.backup_dir.join("2notes-empty-manual.sqlite");
        fs::write(&empty_backup, []).unwrap();
        let state = AppState::new(write_conn, read_conn, paths);

        let result = restore_backup(&state, &empty_backup.display().to_string());

        assert!(result.is_err());
        let conn = state.read_conn().unwrap();
        let page = EntriesRepo::list(&conn, &default_filter(), &default_page()).unwrap();
        assert_eq!(page.items.len(), 1);
        assert_eq!(page.items[0].summary, "still here");
    }

    #[test]
    fn restore_rejects_sqlite_without_2notes_schema() {
        let paths = test_paths();
        fs::create_dir_all(&paths.data_dir).unwrap();
        fs::create_dir_all(&paths.backup_dir).unwrap();
        let (mut write_conn, read_conn) = open_database(&paths.database_path).unwrap();
        let tx = write_conn.transaction().unwrap();
        EntriesRepo::create(&tx, "still here", &now_string()).unwrap();
        tx.commit().unwrap();
        let foreign_backup = paths.backup_dir.join("2notes-foreign-manual.sqlite");
        let foreign = Connection::open(&foreign_backup).unwrap();
        foreign
            .execute("CREATE TABLE unrelated(value TEXT)", [])
            .unwrap();
        drop(foreign);
        let state = AppState::new(write_conn, read_conn, paths);

        let result = restore_backup(&state, &foreign_backup.display().to_string());

        assert!(result.is_err());
        let conn = state.read_conn().unwrap();
        let page = EntriesRepo::list(&conn, &default_filter(), &default_page()).unwrap();
        assert_eq!(page.items.len(), 1);
        assert_eq!(page.items[0].summary, "still here");
    }

    #[test]
    fn restore_file_swap_failure_keeps_live_connections_usable() {
        let paths = test_paths();
        fs::create_dir_all(&paths.data_dir).unwrap();
        let (mut write_conn, read_conn) = open_database(&paths.database_path).unwrap();
        let tx = write_conn.transaction().unwrap();
        EntriesRepo::create(&tx, "still here", &now_string()).unwrap();
        tx.commit().unwrap();
        let backup = create_backup(&paths, &write_conn, "manual").unwrap();
        let rollback_path = paths.database_path.with_extension("restore.bak");
        fs::create_dir_all(&rollback_path).unwrap();
        let state = AppState::new(write_conn, read_conn, paths);

        let result = restore_backup(&state, &backup.path);

        assert!(result.is_err());
        let conn = state.read_conn().unwrap();
        let page = EntriesRepo::list(&conn, &default_filter(), &default_page()).unwrap();
        assert_eq!(page.items.len(), 1);
        assert_eq!(page.items[0].summary, "still here");
    }

    #[test]
    fn recovery_uses_rollback_when_live_database_is_missing() {
        let paths = test_paths();
        fs::create_dir_all(&paths.data_dir).unwrap();
        let (mut write_conn, read_conn) = open_database(&paths.database_path).unwrap();
        let tx = write_conn.transaction().unwrap();
        EntriesRepo::create(&tx, "rollback entry", &now_string()).unwrap();
        tx.commit().unwrap();
        let snapshot = create_backup(&paths, &write_conn, "before_restore").unwrap();
        drop(read_conn);
        drop(write_conn);
        let rollback_path = paths.database_path.with_extension("restore.bak");
        fs::rename(&paths.database_path, &rollback_path).unwrap();
        let mut current_write = Connection::open_in_memory().unwrap();
        let mut current_read = Connection::open_in_memory().unwrap();

        recover_live_connections(
            &paths,
            &rollback_path,
            Path::new(&snapshot.path),
            &mut current_write,
            &mut current_read,
        )
        .unwrap();

        let page = EntriesRepo::list(&current_read, &default_filter(), &default_page()).unwrap();
        assert_eq!(page.items.len(), 1);
        assert_eq!(page.items[0].summary, "rollback entry");
    }

    #[test]
    fn recovery_prefers_snapshot_over_openable_candidate_when_rollback_is_missing() {
        let paths = test_paths();
        fs::create_dir_all(&paths.data_dir).unwrap();
        let (mut write_conn, read_conn) = open_database(&paths.database_path).unwrap();
        let tx = write_conn.transaction().unwrap();
        EntriesRepo::create(&tx, "snapshot entry", &now_string()).unwrap();
        tx.commit().unwrap();
        let snapshot = create_backup(&paths, &write_conn, "before_restore").unwrap();
        let tx = write_conn.transaction().unwrap();
        EntriesRepo::create(&tx, "candidate only", &now_string()).unwrap();
        tx.commit().unwrap();
        drop(read_conn);
        drop(write_conn);
        let rollback_path = paths.database_path.with_extension("restore.bak");
        assert!(!rollback_path.exists());
        let mut current_write = Connection::open_in_memory().unwrap();
        let mut current_read = Connection::open_in_memory().unwrap();

        recover_live_connections(
            &paths,
            &rollback_path,
            Path::new(&snapshot.path),
            &mut current_write,
            &mut current_read,
        )
        .unwrap();

        let page = EntriesRepo::list(&current_read, &default_filter(), &default_page()).unwrap();
        assert_eq!(page.items.len(), 1);
        assert_eq!(page.items[0].summary, "snapshot entry");
    }

    #[test]
    fn live_connection_swap_installs_both_prebuilt_placeholders() {
        let (mut current_write, mut current_read) = open_in_memory().unwrap();
        let tx = current_write.transaction().unwrap();
        EntriesRepo::create(&tx, "old live entry", &now_string()).unwrap();
        tx.commit().unwrap();
        let placeholder_write = Connection::open_in_memory().unwrap();
        let placeholder_read = Connection::open_in_memory().unwrap();

        let (_old_write, old_read) = swap_live_connections(
            &mut current_write,
            &mut current_read,
            placeholder_write,
            placeholder_read,
        );

        let page = EntriesRepo::list(&old_read, &default_filter(), &default_page()).unwrap();
        assert_eq!(page.items.len(), 1);
        assert!(current_read.prepare("SELECT 1 FROM entries").is_err());
    }

    #[test]
    fn backup_metadata_parses_timestamp_and_collision_kind() {
        let paths = test_paths();
        fs::create_dir_all(&paths.backup_dir).unwrap();
        let path = paths
            .backup_dir
            .join("2notes-20260715-123456-daily-2.sqlite");
        fs::write(&path, "backup").unwrap();

        let info = backup_info(&path, infer_kind(&path).unwrap_or("manual")).unwrap();

        assert_eq!(info.kind, "daily");
        assert!(time::OffsetDateTime::parse(
            &info.created_at,
            &time::format_description::well_known::Rfc3339,
        )
        .is_ok());
    }

    fn create_legacy_v2_database(path: &Path) {
        let conn = Connection::open(path).unwrap();
        let migration_1 = include_str!("../db/schema/001_init.sql");
        let migration_2 = include_str!("../db/schema/002_entries_fts.sql");
        conn.execute_batch(migration_1).unwrap();
        conn.execute_batch(migration_2).unwrap();
        let now = "2026-07-01T00:00:00Z";
        conn.execute(
            "INSERT INTO schema_migrations(version, checksum, applied_at)
             VALUES (1, ?1, ?3), (2, ?2, ?3)",
            params![checksum(migration_1), checksum(migration_2), now],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO entries(
               id, title, title_source, original_content, current_content,
               type, status, revision, created_at, updated_at, deleted_at
             ) VALUES (
               'legacy-entry', 'Legacy', 'user', 'original legacy', 'current legacy',
               'material', 'archived', 7, ?1, ?1, NULL
             )",
            [now],
        )
        .unwrap();
    }

    fn test_paths() -> AppPaths {
        let temp = tempfile::tempdir().unwrap().keep();
        AppPaths {
            data_dir: temp.join("data"),
            log_dir: temp.join("logs"),
            backup_dir: temp.join("backups"),
            database_path: temp.join("data").join("2notes.sqlite"),
            bootstrap_path: temp.join("bootstrap.json"),
        }
    }

    fn default_filter() -> EntryListFilter {
        EntryListFilter {
            query: None,
            entry_type: None,
            status: None,
            knowledge_state: None,
            tag: None,
            include_deleted: false,
            trash_only: false,
        }
    }

    fn default_page() -> PageRequest {
        PageRequest {
            limit: None,
            offset: None,
        }
    }
}
