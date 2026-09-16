use std::{
    ffi::OsString,
    fs,
    path::{Path, PathBuf},
};

use rusqlite::{params, Connection};
use time::{format_description::well_known::Rfc3339, OffsetDateTime};
use uuid::Uuid;

use crate::{
    app_state::AppState,
    db::{
        connection::{open_database, open_database_with_read_pool},
        repos::KnowledgeRepo,
    },
    error::{AppError, AppResult},
    files::{paths::AppPaths, timestamps::now_string},
    types::{backups::BackupInfo, knowledge::KnowledgeIndexReport},
};

const BACKUP_PREFIX: &str = "2notes";
const BACKUP_EXT: &str = "sqlite";
const ALLOWED_KINDS: &[&str] = &["manual", "daily", "before_restore"];
pub const DEFAULT_DAILY_RETENTION: usize = 10;
pub const MIN_DAILY_RETENTION: usize = 1;
pub const MAX_DAILY_RETENTION: usize = 100;
const DAILY_BACKUP_INTERVAL_SECONDS: i64 = 24 * 60 * 60;

/// Build a VACUUM'd snapshot of the live database into `backup_dir`.
/// Uses a short-lived standalone connection so the app's shared write/read
/// connections (and their Mutex guards) are NOT held while VACUUM runs.
/// VACUUM INTO reads a consistent snapshot and never mutates the source DB,
/// so it does not need to share a connection with ongoing writes.
pub fn create_backup(paths: &AppPaths, kind: &str) -> AppResult<BackupInfo> {
    let kind = normalize_kind(kind)?;
    fs::create_dir_all(&paths.backup_dir)?;
    let target = unique_backup_path(&paths.backup_dir, kind);
    let target_raw = target.display().to_string();
    let snapshot_conn = Connection::open(&paths.database_path)?;
    // VACUUM INTO takes a shared read snapshot; wait out any in-flight writer
    // instead of failing immediately with SQLITE_BUSY.
    snapshot_conn.busy_timeout(std::time::Duration::from_millis(5000))?;
    if let Err(err) = snapshot_conn.execute("VACUUM main INTO ?1", params![target_raw]) {
        let _ = remove_if_exists(&target);
        return Err(err.into());
    }
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
    let _operation = state.database_operation()?;
    let paths = state.paths.clone();
    let source = validate_backup_path(&paths, requested_path)?;
    let restore_temp = unique_restore_temp_path(&paths);

    let restore_result = (|| {
        fs::copy(&source, &restore_temp)?;
        validate_restore_candidate(&restore_temp)?;
        let before_restore = create_backup(&paths, "before_restore")?;
        let snapshot_path = PathBuf::from(&before_restore.path);

        // Take the write connection, checkpoint, and swap in an in-memory
        // placeholder so no live handle keeps the old file open while it is
        // renamed away (Windows cannot rename a file with open handles). The
        // operation gate already excludes new readers and writers; the read
        // pool is drained next so no straggler can touch the old file either.
        {
            let mut current_write = state.write_conn()?;
            current_write.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")?;
            let placeholder_write = Connection::open_in_memory()?;
            let old_write = std::mem::replace(&mut *current_write, placeholder_write);
            drop(old_write);
        }
        state.replace_read_pool(Vec::new())?;
        let rollback_path = paths.database_path.with_extension("restore.bak");

        match replace_database_file(&paths.database_path, &restore_temp) {
            Ok(_) => match reopen_restored_database(&paths.database_path) {
                Ok((write_conn, read_conns, report)) => {
                    log::info!(
                        "backup_restore_indexes_rebuilt indexed_sources={} links={} unresolved={} search={}",
                        report.indexed_sources,
                        report.link_occurrences,
                        report.unresolved_occurrences,
                        report.search_index_available,
                    );
                    install_live_connections(state, write_conn, read_conns)?;
                    if let Err(err) = cleanup_rollback(&rollback_path) {
                        log::warn!("backup_restore_cleanup_failed source={err}");
                    }
                    Ok(())
                }
                Err(err) => recover_after_restore_failure(
                    state,
                    &paths,
                    &rollback_path,
                    &snapshot_path,
                    err,
                ),
            },
            Err(err) => {
                recover_after_restore_failure(state, &paths, &rollback_path, &snapshot_path, err)
            }
        }
    })();

    if let Err(err) = remove_if_exists(&restore_temp) {
        log::warn!("backup_restore_temp_cleanup_failed source={err}");
    }
    restore_result?;

    backup_info(&source, infer_kind(&source).unwrap_or("manual"))
}

pub fn ensure_daily_backup(
    paths: &AppPaths,
    retention_count: usize,
    now: &str,
) -> AppResult<Option<BackupInfo>> {
    let retention_count = retention_count.clamp(MIN_DAILY_RETENTION, MAX_DAILY_RETENTION);
    let daily = list_daily_backups(paths)?;
    let due = daily
        .first()
        .and_then(|backup| parse_timestamp(&backup.created_at))
        .map(|created| {
            parse_timestamp(now)
                .map(|current| {
                    current - created > time::Duration::seconds(DAILY_BACKUP_INTERVAL_SECONDS)
                })
                .unwrap_or(true)
        })
        .unwrap_or(true);

    let created = if due {
        Some(create_backup(paths, "daily")?)
    } else {
        None
    };
    prune_daily_backups(paths, retention_count)?;
    Ok(created)
}

pub fn list_daily_backups(paths: &AppPaths) -> AppResult<Vec<BackupInfo>> {
    let mut backups = list_backups(paths)?
        .into_iter()
        .filter(|backup| backup.kind == "daily")
        .collect::<Vec<_>>();
    backups.sort_by(|left, right| {
        parse_timestamp(&right.created_at)
            .cmp(&parse_timestamp(&left.created_at))
            .then_with(|| right.file_name.cmp(&left.file_name))
    });
    Ok(backups)
}

pub fn prune_daily_backups(paths: &AppPaths, retention_count: usize) -> AppResult<usize> {
    let retention_count = retention_count.clamp(MIN_DAILY_RETENTION, MAX_DAILY_RETENTION);
    let daily = list_daily_backups(paths)?;
    let mut removed = 0;
    for backup in daily.into_iter().skip(retention_count) {
        match fs::remove_file(&backup.path) {
            Ok(()) => removed += 1,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
            Err(err) => {
                return Err(AppError::system(
                    "BACKUP_RETENTION_FAILED",
                    format!("无法清理旧自动备份 {}: {err}", backup.file_name),
                ));
            }
        }
    }
    Ok(removed)
}

fn parse_timestamp(value: &str) -> Option<OffsetDateTime> {
    OffsetDateTime::parse(value, &Rfc3339).ok()
}

fn unique_restore_temp_path(paths: &AppPaths) -> PathBuf {
    paths
        .data_dir
        .join(format!(".2notes-restore-{}.tmp", Uuid::new_v4()))
}

fn recover_after_restore_failure(
    state: &AppState,
    paths: &AppPaths,
    rollback_path: &Path,
    snapshot_path: &Path,
    restore_error: AppError,
) -> AppResult<()> {
    match recover_live_connections(state, paths, rollback_path, snapshot_path) {
        Ok(()) => Err(restore_error),
        Err(recovery_error) => Err(recovery_error),
    }
}

/// Install the write connection and a fresh read pool back into the state.
/// The caller must hold `database_operation()` so no reader/writer races the swap.
fn install_live_connections(
    state: &AppState,
    write_conn: Connection,
    read_conns: Vec<Connection>,
) -> AppResult<()> {
    let mut current_write = state.write_conn()?;
    *current_write = write_conn;
    drop(current_write);
    state.replace_read_pool(read_conns)
}

fn recover_live_connections(
    state: &AppState,
    paths: &AppPaths,
    rollback_path: &Path,
    snapshot_path: &Path,
) -> AppResult<()> {
    let mut recovery_error = None;
    if rollback_path.is_file() {
        match rollback_database_file(&paths.database_path, rollback_path)
            .and_then(|_| open_database_with_read_pool(&paths.database_path))
        {
            Ok((write_conn, read_conns)) => {
                return install_live_connections(state, write_conn, read_conns);
            }
            Err(err) => {
                log::error!("backup_restore_rollback_recovery_failed source={err}");
                recovery_error = Some(err);
            }
        }
    }
    if snapshot_path.is_file() {
        match restore_from_snapshot(&paths.database_path, snapshot_path)
            .and_then(|_| open_database_with_read_pool(&paths.database_path))
        {
            Ok((write_conn, read_conns)) => {
                return install_live_connections(state, write_conn, read_conns);
            }
            Err(err) => {
                log::error!("backup_restore_snapshot_recovery_failed source={err}");
                recovery_error = Some(err);
            }
        }
    }
    if paths.database_path.is_file() {
        match open_database_with_read_pool(&paths.database_path) {
            Ok((write_conn, read_conns)) => {
                return install_live_connections(state, write_conn, read_conns);
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
) -> AppResult<(Connection, Vec<Connection>, KnowledgeIndexReport)> {
    let (mut write_conn, read_conns) = open_database_with_read_pool(database_path)?;
    let report = KnowledgeRepo::rebuild_all_indexes(&mut write_conn)?;
    Ok((write_conn, read_conns, report))
}

fn normalize_kind(kind: &str) -> AppResult<&'static str> {
    ALLOWED_KINDS
        .iter()
        .copied()
        .find(|allowed| *allowed == kind)
        .ok_or_else(|| AppError::validation("BACKUP_KIND_INVALID", "备份类型无效"))
}

fn unique_backup_path(backup_dir: &Path, kind: &str) -> PathBuf {
    let timestamp = crate::files::timestamps::backup_timestamp(&now_string());
    let suffix = Uuid::new_v4().simple().to_string();
    backup_dir.join(format!(
        "{BACKUP_PREFIX}-{timestamp}-{kind}-{suffix}.{BACKUP_EXT}"
    ))
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
    let foreign_key_violations: i64 =
        write_conn.query_row("SELECT COUNT(*) FROM pragma_foreign_key_check", [], |row| {
            row.get(0)
        })?;
    if foreign_key_violations > 0 {
        return Err(AppError::validation(
            "BACKUP_FOREIGN_KEY_FAILED",
            "备份文件外键一致性校验失败",
        ));
    }
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
            || name.contains(&format!("{marker}-"))
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
            connection::open_database,
            migrations::{checksum, now_string},
            repos::{EntriesRepo, KnowledgeRepo},
        },
        types::{
            documents::{BlockDocument, BlockNode},
            entries::{EntryListFilter, EntryPatch, PageRequest},
            knowledge::KnowledgeState,
        },
    };

    fn paragraph_document(content: impl Into<String>) -> BlockDocument {
        BlockDocument::from_blocks(vec![BlockNode::paragraph(
            uuid::Uuid::new_v4().to_string(),
            content,
        )])
    }

    #[test]
    fn creates_and_lists_sqlite_backup() {
        let paths = test_paths();
        fs::create_dir_all(&paths.data_dir).unwrap();
        let (_, _) = open_database(&paths.database_path).unwrap();

        let backup = create_backup(&paths, "manual").unwrap();
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
        let backup = create_backup(&paths, "manual").unwrap();
        let tx = write_conn.transaction().unwrap();
        EntriesRepo::create(&tx, "after backup", &now).unwrap();
        tx.commit().unwrap();
        let state = AppState::new(write_conn, read_conn, paths);

        restore_backup(&state, &backup.path).unwrap();

        let page = state
            .with_read_conn(|conn| EntriesRepo::list(conn, &default_filter(), &default_page()))
            .unwrap();
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

        let (version, row, knowledge_tables) = state
            .with_read_conn(|conn| {
                let version: i64 =
                    conn.query_row("SELECT MAX(version) FROM schema_migrations", [], |row| {
                        row.get(0)
                    })?;
                let row: (String, String, i64, String) = conn.query_row(
                    "SELECT current_content, original_content, revision, knowledge_state
                         FROM entries WHERE id = 'legacy-entry'",
                    [],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
                )?;
                let knowledge_tables: i64 = conn.query_row(
                    "SELECT COUNT(*) FROM sqlite_master
                         WHERE type = 'table' AND name IN ('entry_aliases', 'entry_links')",
                    [],
                    |row| row.get(0),
                )?;
                Ok((version, row, knowledge_tables))
            })
            .unwrap();

        assert_eq!(version, 9);
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
                document: None,
                title: Some("新标题".to_string()),
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
        let backup = create_backup(&paths, "manual").unwrap();
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

        let restored = state
            .with_read_conn(|conn| EntriesRepo::get(conn, &target.id))
            .unwrap();
        let relations = state
            .with_read_conn(|conn| KnowledgeRepo::relations(conn, &target.id))
            .unwrap();
        let mut filter = default_filter();
        filter.query = Some("旧标题".to_string());
        filter.knowledge_state = Some(KnowledgeState::Knowledge);
        let search = state
            .with_read_conn(|conn| EntriesRepo::list(conn, &filter, &default_page()))
            .unwrap();
        let (bogus_links, bogus_search): (i64, i64) = state
            .with_read_conn(|conn| {
                let links = conn.query_row(
                    "SELECT COUNT(*) FROM entry_links WHERE raw_target = '坏数据'",
                    [],
                    |row| row.get(0),
                )?;
                let search = conn.query_row(
                    "SELECT COUNT(*) FROM entries_fts WHERE entry_id = 'bogus'",
                    [],
                    |row| row.get(0),
                )?;
                Ok((links, search))
            })
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
        let backup = create_backup(&paths, "manual").unwrap();
        let tx = write_conn.transaction().unwrap();
        EntriesRepo::create(&tx, "live only after backup", &now_string()).unwrap();
        tx.commit().unwrap();
        let backup_conn = Connection::open(&backup.path).unwrap();
        backup_conn.execute("DROP TABLE entry_links", []).unwrap();
        drop(backup_conn);
        let state = AppState::new(write_conn, read_conn, paths);

        let result = restore_backup(&state, &backup.path);

        assert!(result.is_err());
        let page = state
            .with_read_conn(|conn| EntriesRepo::list(conn, &default_filter(), &default_page()))
            .unwrap();
        assert_eq!(page.items.len(), 2);
        assert!(page
            .items
            .iter()
            .any(|entry| entry.summary == "live only after backup"));
    }

    #[test]
    fn restore_failure_preserves_committed_data_left_in_wal() {
        let paths = test_paths();
        fs::create_dir_all(&paths.data_dir).unwrap();
        let (mut write_conn, read_conn) = open_database(&paths.database_path).unwrap();
        write_conn
            .execute_batch("PRAGMA wal_autocheckpoint = 0;")
            .unwrap();
        let now = now_string();
        let tx = write_conn.transaction().unwrap();
        EntriesRepo::create(&tx, "before backup", &now).unwrap();
        tx.commit().unwrap();
        let backup = create_backup(&paths, "manual").unwrap();
        let tx = write_conn.transaction().unwrap();
        EntriesRepo::create(&tx, "committed only in wal", &now).unwrap();
        tx.commit().unwrap();
        assert!(sidecar_path(&paths.database_path, "-wal").is_file());

        let backup_conn = Connection::open(&backup.path).unwrap();
        backup_conn.execute("DROP TABLE entry_links", []).unwrap();
        drop(backup_conn);
        let state = AppState::new(write_conn, read_conn, paths);

        assert!(restore_backup(&state, &backup.path).is_err());

        let page = state
            .with_read_conn(|conn| EntriesRepo::list(conn, &default_filter(), &default_page()))
            .unwrap();
        assert_eq!(page.items.len(), 2);
        assert!(page
            .items
            .iter()
            .any(|entry| entry.summary == "committed only in wal"));
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
        let page = state
            .with_read_conn(|conn| EntriesRepo::list(conn, &default_filter(), &default_page()))
            .unwrap();
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
        let page = state
            .with_read_conn(|conn| EntriesRepo::list(conn, &default_filter(), &default_page()))
            .unwrap();
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
        let page = state
            .with_read_conn(|conn| EntriesRepo::list(conn, &default_filter(), &default_page()))
            .unwrap();
        assert_eq!(page.items.len(), 1);
        assert_eq!(page.items[0].summary, "still here");
    }

    #[test]
    fn restore_rejects_foreign_key_violations() {
        let paths = test_paths();
        fs::create_dir_all(&paths.backup_dir).unwrap();
        let backup = paths.backup_dir.join("2notes-orphan-manual.sqlite");
        let (conn, read_conn) = open_database(&backup).unwrap();
        drop(read_conn);
        conn.execute_batch("PRAGMA foreign_keys = OFF;").unwrap();
        conn.execute(
            "INSERT INTO entry_tags(entry_id, tag_id) VALUES ('missing-entry', 'missing-tag')",
            [],
        )
        .unwrap();
        drop(conn);

        let result = validate_restore_candidate(&backup);

        assert!(result.is_err());
    }

    #[test]
    fn restore_file_swap_failure_keeps_live_connections_usable() {
        let paths = test_paths();
        fs::create_dir_all(&paths.data_dir).unwrap();
        let (mut write_conn, read_conn) = open_database(&paths.database_path).unwrap();
        let tx = write_conn.transaction().unwrap();
        EntriesRepo::create(&tx, "still here", &now_string()).unwrap();
        tx.commit().unwrap();
        let backup = create_backup(&paths, "manual").unwrap();
        let rollback_path = paths.database_path.with_extension("restore.bak");
        fs::create_dir_all(&rollback_path).unwrap();
        let state = AppState::new(write_conn, read_conn, paths);

        let result = restore_backup(&state, &backup.path);

        assert!(result.is_err());
        let page = state
            .with_read_conn(|conn| EntriesRepo::list(conn, &default_filter(), &default_page()))
            .unwrap();
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
        let snapshot = create_backup(&paths, "before_restore").unwrap();
        drop(read_conn);
        drop(write_conn);
        let rollback_path = paths.database_path.with_extension("restore.bak");
        fs::rename(&paths.database_path, &rollback_path).unwrap();
        let state = AppState::new(
            Connection::open_in_memory().unwrap(),
            Connection::open_in_memory().unwrap(),
            paths.clone(),
        );

        recover_live_connections(&state, &paths, &rollback_path, Path::new(&snapshot.path))
            .unwrap();

        let page = state
            .with_read_conn(|conn| EntriesRepo::list(conn, &default_filter(), &default_page()))
            .unwrap();
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
        let snapshot = create_backup(&paths, "before_restore").unwrap();
        let tx = write_conn.transaction().unwrap();
        EntriesRepo::create(&tx, "candidate only", &now_string()).unwrap();
        tx.commit().unwrap();
        drop(read_conn);
        drop(write_conn);
        let rollback_path = paths.database_path.with_extension("restore.bak");
        assert!(!rollback_path.exists());
        let state = AppState::new(
            Connection::open_in_memory().unwrap(),
            Connection::open_in_memory().unwrap(),
            paths.clone(),
        );

        recover_live_connections(&state, &paths, &rollback_path, Path::new(&snapshot.path))
            .unwrap();

        let page = state
            .with_read_conn(|conn| EntriesRepo::list(conn, &default_filter(), &default_page()))
            .unwrap();
        assert_eq!(page.items.len(), 1);
        assert_eq!(page.items[0].summary, "snapshot entry");
    }

    #[test]
    fn daily_backup_is_due_when_missing_or_older_than_a_day() {
        let paths = test_paths();
        fs::create_dir_all(&paths.backup_dir).unwrap();
        let _ = open_database(&paths.database_path).unwrap();

        assert!(ensure_daily_backup(&paths, 10, "2026-07-31T00:00:00Z")
            .unwrap()
            .is_some());

        let recent = list_daily_backups(&paths).unwrap();
        assert_eq!(recent.len(), 1);
        assert!(ensure_daily_backup(&paths, 10, &now_string())
            .unwrap()
            .is_none());
    }

    #[test]
    fn retention_removes_old_daily_only() {
        let paths = test_paths();
        fs::create_dir_all(&paths.backup_dir).unwrap();
        for (stamp, kind) in [
            ("20260701-000000", "daily"),
            ("20260702-000000", "daily"),
            ("20260703-000000", "daily"),
            ("20260704-000000", "daily"),
            ("20260705-000000", "manual"),
            ("20260706-000000", "before_restore"),
        ] {
            fs::write(
                paths
                    .backup_dir
                    .join(format!("2notes-{stamp}-{kind}-fixture.sqlite")),
                b"fixture",
            )
            .unwrap();
        }

        assert_eq!(prune_daily_backups(&paths, 2).unwrap(), 2);
        let remaining = list_backups(&paths).unwrap();
        assert_eq!(
            remaining
                .iter()
                .filter(|backup| backup.kind == "daily")
                .count(),
            2
        );
        assert!(remaining.iter().any(|backup| backup.kind == "manual"));
        assert!(remaining
            .iter()
            .any(|backup| backup.kind == "before_restore"));
    }

    #[test]
    fn retention_count_is_clamped_to_safe_bounds() {
        assert_eq!(DEFAULT_DAILY_RETENTION, 10);
        assert_eq!(MIN_DAILY_RETENTION, 1);
        assert_eq!(MAX_DAILY_RETENTION, 100);
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

    /// Builds an older 2notes schema at the requested top version (3 or 4) so the restore
    /// path can be exercised against pre-005 backups. The body and current content contain
    /// a WikiLink that the rebuilt index should re-resolve after migration.
    fn create_legacy_pre005_database(path: &Path, top_version: i64) {
        assert!(matches!(top_version, 3 | 4));
        let conn = Connection::open(path).unwrap();
        conn.execute_batch(include_str!("../db/schema/001_init.sql"))
            .unwrap();
        conn.execute_batch(include_str!("../db/schema/002_entries_fts.sql"))
            .unwrap();
        conn.execute_batch(include_str!("../db/schema/003_knowledge_graph.sql"))
            .unwrap();
        let now = "2026-07-01T00:00:00Z";
        conn.execute(
            "INSERT INTO schema_migrations(version, checksum, applied_at) VALUES
               (1, ?1, ?4), (2, ?2, ?4), (3, ?3, ?4)",
            params![
                checksum(include_str!("../db/schema/001_init.sql")),
                checksum(include_str!("../db/schema/002_entries_fts.sql")),
                checksum(include_str!("../db/schema/003_knowledge_graph.sql")),
                now,
            ],
        )
        .unwrap();
        if top_version >= 4 {
            conn.execute_batch(include_str!("../db/schema/004_entries_fts_trigram.sql"))
                .unwrap();
            conn.execute(
                "INSERT INTO schema_migrations(version, checksum, applied_at) VALUES (?1, ?2, ?3)",
                params![
                    4,
                    checksum(include_str!("../db/schema/004_entries_fts_trigram.sql")),
                    now,
                ],
            )
            .unwrap();
        }

        // Seed a knowledge entry and a source linking to it; migration to 005 must still
        // preserve the current_content that drives WikiLink resolution pre-document.
        conn.execute(
            "INSERT INTO entries(
               id, title, title_source, original_content, current_content,
               type, status, revision, created_at, updated_at, deleted_at,
               knowledge_state, knowledge_title_key
             ) VALUES
               ('legacy-target', '目标', 'user', 'target body', 'target body',
                'material', 'archived', 4, ?1, ?1, NULL, 'knowledge', '目标'),
               ('legacy-source', NULL, 'auto', 'source body', '正文 [[目标]] 收尾',
                'unclear', 'pending', 3, ?1, ?1, NULL, 'capture', NULL)",
            [now],
        )
        .unwrap();
        // Resolve the outgoing link by hand so the rebuild can verify the resolved edge.
        conn.execute(
            "INSERT INTO entry_links(source_entry_id, ordinal, raw_target, normalized_target, target_entry_id)
             VALUES ('legacy-source', 0, '目标', '目标', 'legacy-target')",
            [],
        )
        .unwrap();
    }

    #[test]
    fn restores_legacy_v3_backup_and_migrates_to_block_documents_schema() {
        restores_legacy_pre005_backup_and_migrates_to_block_documents_schema(3);
    }

    #[test]
    fn restores_legacy_v4_backup_and_migrates_to_block_documents_schema() {
        restores_legacy_pre005_backup_and_migrates_to_block_documents_schema(4);
    }

    fn restores_legacy_pre005_backup_and_migrates_to_block_documents_schema(top_version: i64) {
        let paths = test_paths();
        fs::create_dir_all(&paths.data_dir).unwrap();
        fs::create_dir_all(&paths.backup_dir).unwrap();
        let (write_conn, read_conn) = open_database(&paths.database_path).unwrap();
        let legacy = paths
            .backup_dir
            .join(format!("2notes-legacy-v{top_version}.sqlite"));
        create_legacy_pre005_database(&legacy, top_version);
        let state = AppState::new(write_conn, read_conn, paths);

        restore_backup(&state, &legacy.display().to_string()).unwrap();

        let (version, documents, blocks, hierarchy, current_content) = state
            .with_read_conn(|conn| {
                let version: i64 =
                    conn.query_row("SELECT MAX(version) FROM schema_migrations", [], |row| {
                        row.get(0)
                    })?;
                // schema 005 introduces entry_documents and blocks and they must be
                // populated for the source entry, with its current_content preserved
                // verbatim as a legacy snapshot.
                let (documents, blocks): (i64, i64) = conn.query_row(
                    "SELECT (SELECT COUNT(*) FROM entry_documents WHERE entry_id = 'legacy-source'),
                            (SELECT COUNT(*) FROM blocks WHERE entry_id = 'legacy-source')",
                    [],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )?;
                let hierarchy: (Option<String>, i64) = conn.query_row(
                    "SELECT parent_entry_id, sibling_order
                     FROM entry_hierarchy WHERE entry_id = 'legacy-target'",
                    [],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )?;
                let current_content: String = conn.query_row(
                    "SELECT current_content FROM entries WHERE id = 'legacy-source'",
                    [],
                    |row| row.get(0),
                )?;
                Ok((version, documents, blocks, hierarchy, current_content))
            })
            .unwrap();
        assert_eq!(version, 9);
        assert_eq!(documents, 1);
        assert!(blocks > 0);
        assert_eq!(hierarchy, (None, 0));
        assert_eq!(current_content, "正文 [[目标]] 收尾");
    }

    /// Schema-equivalent 0.3.0 baseline content (migration v4), not installer-captured.
    /// Seeds capture / knowledge / alias / resolved+unresolved WikiLinks / trash / draft.
    fn seed_v030_upgrade_baseline(conn: &mut Connection, now: &str) -> V030FixtureMetrics {
        use crate::db::repos::DraftsRepo;
        use crate::knowledge::wiki_links::normalize_knowledge_title;

        let tx = conn.transaction().unwrap();
        // Keep create content short so auto-title is exactly the promote-time alias source.
        let knowledge = EntriesRepo::create(&tx, "Gate0 Knowledge Anchor", now).unwrap();
        let knowledge =
            KnowledgeRepo::promote(&tx, &knowledge.id, knowledge.revision, now).unwrap();
        let knowledge = EntriesRepo::update(
            &tx,
            &knowledge.id,
            EntryPatch {
                document: Some(paragraph_document(
                    "Gate0 Knowledge body.\n\nRenamed after promote so alias is retained.",
                )),
                title: Some("Gate0 Knowledge".to_string()),
                entry_type: None,
                status: None,
                tags: Some(vec!["gate0".into(), "baseline".into()]),
            },
            knowledge.revision,
            now,
        )
        .unwrap();

        let capture = EntriesRepo::create(
            &tx,
            "Plain capture note about baseline freeze without wiki links.",
            now,
        )
        .unwrap();

        let linker = EntriesRepo::create(
            &tx,
            "Resolved [[Gate0 Knowledge]] and unresolved [[Missing Topic]] for upgrade checks.",
            now,
        )
        .unwrap();

        let trashed =
            EntriesRepo::create(&tx, "Trashed capture kept for restore metrics.", now).unwrap();
        let trashed = EntriesRepo::move_to_trash(&tx, &trashed.id, trashed.revision, now).unwrap();

        DraftsRepo::update(
            &tx,
            "Unsubmitted quick capture draft for Gate 0 baseline.",
            0,
            now,
        )
        .unwrap();
        tx.commit().unwrap();

        // Title rename during knowledge update should keep promote-time title as alias.
        let aliases = KnowledgeRepo::aliases_for_entry(conn, &knowledge.id).unwrap();
        assert!(
            aliases
                .iter()
                .any(|alias| alias == "Gate0 Knowledge Anchor"),
            "expected promote-time alias, got {aliases:?}"
        );

        let relations = KnowledgeRepo::relations(conn, &linker.id).unwrap();
        assert_eq!(relations.outgoing.len(), 1);
        assert_eq!(relations.outgoing[0].id, knowledge.id);
        assert_eq!(relations.unresolved.len(), 1);
        assert_eq!(relations.unresolved[0].raw_target, "Missing Topic");

        collect_v030_fixture_metrics(
            conn,
            V030FixtureSeedIds {
                capture_id: capture.id,
                knowledge_id: knowledge.id,
                linker_id: linker.id,
                trash_id: trashed.id,
                knowledge_title_key: normalize_knowledge_title("Gate0 Knowledge"),
            },
        )
    }

    struct V030FixtureSeedIds {
        capture_id: String,
        knowledge_id: String,
        linker_id: String,
        trash_id: String,
        knowledge_title_key: String,
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    struct V030FixtureMetrics {
        schema_version: i64,
        entries_total: i64,
        capture_active: i64,
        knowledge_active: i64,
        trash_only: i64,
        draft_revision: i64,
        draft_nonempty: bool,
        aliases_total: i64,
        link_occurrences: i64,
        resolved_links: i64,
        unresolved_links: i64,
        search_hits_for_gate0: i64,
        capture_id: String,
        knowledge_id: String,
        linker_id: String,
        trash_id: String,
        knowledge_title_key: String,
    }

    fn collect_v030_fixture_metrics(
        conn: &Connection,
        ids: V030FixtureSeedIds,
    ) -> V030FixtureMetrics {
        use crate::db::repos::DraftsRepo;

        let schema_version: i64 = conn
            .query_row("SELECT MAX(version) FROM schema_migrations", [], |row| {
                row.get(0)
            })
            .unwrap();
        let entries_total: i64 = conn
            .query_row("SELECT COUNT(*) FROM entries", [], |row| row.get(0))
            .unwrap();
        let capture_active: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM entries
                 WHERE deleted_at IS NULL AND knowledge_state = 'capture'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let knowledge_active: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM entries
                 WHERE deleted_at IS NULL AND knowledge_state = 'knowledge'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let trash_only: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM entries WHERE deleted_at IS NOT NULL",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let aliases_total: i64 = conn
            .query_row("SELECT COUNT(*) FROM entry_aliases", [], |row| row.get(0))
            .unwrap();
        let link_occurrences: i64 = conn
            .query_row("SELECT COUNT(*) FROM entry_links", [], |row| row.get(0))
            .unwrap();
        let resolved_links: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM entry_links WHERE target_entry_id IS NOT NULL",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let unresolved_links: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM entry_links WHERE target_entry_id IS NULL",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let draft = DraftsRepo::get(conn).unwrap();
        let mut filter = default_filter();
        filter.query = Some("Gate0".to_string());
        let search_hits_for_gate0 = EntriesRepo::list(conn, &filter, &default_page())
            .unwrap()
            .items
            .len() as i64;

        V030FixtureMetrics {
            schema_version,
            entries_total,
            capture_active,
            knowledge_active,
            trash_only,
            draft_revision: draft.revision,
            draft_nonempty: !draft.content.trim().is_empty(),
            aliases_total,
            link_occurrences,
            resolved_links,
            unresolved_links,
            search_hits_for_gate0,
            capture_id: ids.capture_id,
            knowledge_id: ids.knowledge_id,
            linker_id: ids.linker_id,
            trash_id: ids.trash_id,
            knowledge_title_key: ids.knowledge_title_key,
        }
    }

    fn expected_v030_fixture_metrics(ids: &V030FixtureSeedIds) -> V030FixtureMetrics {
        V030FixtureMetrics {
            schema_version: 9,
            entries_total: 4,
            capture_active: 2,
            knowledge_active: 1,
            trash_only: 1,
            draft_revision: 1,
            draft_nonempty: true,
            aliases_total: 1,
            link_occurrences: 2,
            resolved_links: 1,
            unresolved_links: 1,
            search_hits_for_gate0: 2,
            capture_id: ids.capture_id.clone(),
            knowledge_id: ids.knowledge_id.clone(),
            linker_id: ids.linker_id.clone(),
            trash_id: ids.trash_id.clone(),
            knowledge_title_key: ids.knowledge_title_key.clone(),
        }
    }

    #[test]
    fn v030_upgrade_baseline_fixture_seeds_and_restores() {
        let paths = test_paths();
        fs::create_dir_all(&paths.data_dir).unwrap();
        fs::create_dir_all(&paths.backup_dir).unwrap();
        let (mut write_conn, read_conn) = open_database(&paths.database_path).unwrap();
        let now = now_string();
        let metrics = seed_v030_upgrade_baseline(&mut write_conn, &now);
        let expected = expected_v030_fixture_metrics(&V030FixtureSeedIds {
            capture_id: metrics.capture_id.clone(),
            knowledge_id: metrics.knowledge_id.clone(),
            linker_id: metrics.linker_id.clone(),
            trash_id: metrics.trash_id.clone(),
            knowledge_title_key: metrics.knowledge_title_key.clone(),
        });
        assert_eq!(metrics, expected);

        let backup = create_backup(&paths, "manual").unwrap();
        // Mutate live DB so restore has something to replace.
        let tx = write_conn.transaction().unwrap();
        EntriesRepo::create(&tx, "post-backup noise that restore must drop", &now).unwrap();
        tx.commit().unwrap();

        let state = AppState::new(write_conn, read_conn, paths.clone());
        restore_backup(&state, &backup.path).unwrap();

        let restored = state
            .with_read_conn(|conn| {
                Ok(collect_v030_fixture_metrics(
                    conn,
                    V030FixtureSeedIds {
                        capture_id: metrics.capture_id.clone(),
                        knowledge_id: metrics.knowledge_id.clone(),
                        linker_id: metrics.linker_id.clone(),
                        trash_id: metrics.trash_id.clone(),
                        knowledge_title_key: metrics.knowledge_title_key.clone(),
                    },
                ))
            })
            .unwrap();
        assert_eq!(restored, expected);

        if std::env::var("GENERATE_V030_FIXTURE").ok().as_deref() == Some("1") {
            let repo_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .parent()
                .expect("src-tauri parent")
                .to_path_buf();
            let fixture_dir = repo_root.join("scripts/test/fixtures");
            fs::create_dir_all(&fixture_dir).unwrap();
            let fixture_db = fixture_dir.join("v0.3.0-upgrade-baseline.sqlite");
            if fixture_db.exists() {
                fs::remove_file(&fixture_db).unwrap();
            }
            fs::copy(&backup.path, &fixture_db).unwrap();

            let metrics_json = serde_json::json!({
                "name": "v0.3.0-upgrade-baseline",
                "source": "schema-equivalent-0.3.0-content-baseline",
                "not_installer_captured": true,
                "migration_version": restored.schema_version,
                "entries": {
                    "total": restored.entries_total,
                    "capture_active": restored.capture_active,
                    "knowledge_active": restored.knowledge_active,
                    "trash_only": restored.trash_only
                },
                "draft": {
                    "revision": restored.draft_revision,
                    "nonempty": restored.draft_nonempty
                },
                "aliases_total": restored.aliases_total,
                "links": {
                    "occurrences": restored.link_occurrences,
                    "resolved": restored.resolved_links,
                    "unresolved": restored.unresolved_links
                },
                "search_hits_for_query_Gate0": restored.search_hits_for_gate0,
                "seed_ids": {
                    "capture_id": restored.capture_id,
                    "knowledge_id": restored.knowledge_id,
                    "linker_id": restored.linker_id,
                    "trash_id": restored.trash_id,
                    "knowledge_title_key": restored.knowledge_title_key
                },
                "fixture_db": "scripts/test/fixtures/v0.3.0-upgrade-baseline.sqlite"
            });
            let metrics_path = fixture_dir.join("v0.3.0-upgrade-baseline.metrics.json");
            fs::write(
                &metrics_path,
                serde_json::to_string_pretty(&metrics_json).unwrap() + "\n",
            )
            .unwrap();
            eprintln!(
                "wrote fixture db={} metrics={}",
                fixture_db.display(),
                metrics_path.display()
            );
        }
    }

    /// Opens the committed Gate 0 sqlite fixture (not a freshly seeded temp DB)
    /// and asserts live metrics match scripts/test/fixtures/*.metrics.json.
    #[test]
    fn v030_upgrade_baseline_committed_fixture_matches_metrics_json() {
        let repo_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("src-tauri parent")
            .to_path_buf();
        let fixture_db = repo_root.join("scripts/test/fixtures/v0.3.0-upgrade-baseline.sqlite");
        let metrics_path =
            repo_root.join("scripts/test/fixtures/v0.3.0-upgrade-baseline.metrics.json");
        assert!(
            fixture_db.is_file(),
            "committed fixture missing: {}",
            fixture_db.display()
        );
        assert!(
            metrics_path.is_file(),
            "committed metrics missing: {}",
            metrics_path.display()
        );

        // Copy out of the repo so open_database WAL sidecars never touch git files.
        let temp = tempfile::tempdir().unwrap();
        let live_db = temp.path().join("v0.3.0-upgrade-baseline.sqlite");
        fs::copy(&fixture_db, &live_db).unwrap();

        let (write_conn, _read_conn) = open_database(&live_db).unwrap();
        let expected_json: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(&metrics_path).unwrap()).unwrap();
        let seed_ids = &expected_json["seed_ids"];
        let ids = V030FixtureSeedIds {
            capture_id: seed_ids["capture_id"].as_str().unwrap().to_string(),
            knowledge_id: seed_ids["knowledge_id"].as_str().unwrap().to_string(),
            linker_id: seed_ids["linker_id"].as_str().unwrap().to_string(),
            trash_id: seed_ids["trash_id"].as_str().unwrap().to_string(),
            knowledge_title_key: seed_ids["knowledge_title_key"]
                .as_str()
                .unwrap()
                .to_string(),
        };
        let live = collect_v030_fixture_metrics(&write_conn, ids);
        let expected = V030FixtureMetrics {
            schema_version: expected_json["migration_version"].as_i64().unwrap(),
            entries_total: expected_json["entries"]["total"].as_i64().unwrap(),
            capture_active: expected_json["entries"]["capture_active"].as_i64().unwrap(),
            knowledge_active: expected_json["entries"]["knowledge_active"]
                .as_i64()
                .unwrap(),
            trash_only: expected_json["entries"]["trash_only"].as_i64().unwrap(),
            draft_revision: expected_json["draft"]["revision"].as_i64().unwrap(),
            draft_nonempty: expected_json["draft"]["nonempty"].as_bool().unwrap(),
            aliases_total: expected_json["aliases_total"].as_i64().unwrap(),
            link_occurrences: expected_json["links"]["occurrences"].as_i64().unwrap(),
            resolved_links: expected_json["links"]["resolved"].as_i64().unwrap(),
            unresolved_links: expected_json["links"]["unresolved"].as_i64().unwrap(),
            search_hits_for_gate0: expected_json["search_hits_for_query_Gate0"]
                .as_i64()
                .unwrap(),
            capture_id: seed_ids["capture_id"].as_str().unwrap().to_string(),
            knowledge_id: seed_ids["knowledge_id"].as_str().unwrap().to_string(),
            linker_id: seed_ids["linker_id"].as_str().unwrap().to_string(),
            trash_id: seed_ids["trash_id"].as_str().unwrap().to_string(),
            knowledge_title_key: seed_ids["knowledge_title_key"]
                .as_str()
                .unwrap()
                .to_string(),
        };
        assert_eq!(live, expected);
        assert_eq!(
            expected_json["name"].as_str().unwrap(),
            "v0.3.0-upgrade-baseline"
        );
        assert_eq!(
            expected_json["fixture_db"].as_str().unwrap(),
            "scripts/test/fixtures/v0.3.0-upgrade-baseline.sqlite"
        );
        assert!(expected_json["not_installer_captured"].as_bool().unwrap());
    }

    #[test]
    fn v4_pre_block_document_upgrade_preserves_legacy_content_and_metadata() {
        let repo_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("src-tauri parent")
            .to_path_buf();
        let fixture_db = repo_root.join("scripts/test/fixtures/v0.3.0-upgrade-baseline.sqlite");
        let temp = tempfile::tempdir().unwrap();
        let legacy_db = temp.path().join("v0.3.0-upgrade-baseline.sqlite");
        fs::copy(&fixture_db, &legacy_db).unwrap();

        let before = Connection::open(&legacy_db).unwrap();
        let entries_before = before
            .prepare(
                "SELECT id, current_content, revision, updated_at, deleted_at FROM entries ORDER BY id",
            )
            .unwrap()
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, Option<String>>(4)?,
                ))
            })
            .unwrap()
            .collect::<rusqlite::Result<Vec<_>>>()
            .unwrap();
        let fts_before = before
            .prepare("SELECT entry_id, current_content FROM entries_fts ORDER BY entry_id")
            .unwrap()
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })
            .unwrap()
            .collect::<rusqlite::Result<Vec<_>>>()
            .unwrap();
        let tags_before = before
            .prepare(
                "SELECT et.entry_id, t.name
                 FROM entry_tags et JOIN tags t ON t.id = et.tag_id
                 ORDER BY et.entry_id, t.name",
            )
            .unwrap()
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })
            .unwrap()
            .collect::<rusqlite::Result<Vec<_>>>()
            .unwrap();
        let links_before = before
            .prepare(
                "SELECT source_entry_id, ordinal, raw_target, normalized_target, target_entry_id
                 FROM entry_links ORDER BY source_entry_id, ordinal",
            )
            .unwrap()
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, Option<String>>(4)?,
                ))
            })
            .unwrap()
            .collect::<rusqlite::Result<Vec<_>>>()
            .unwrap();
        let draft_before: (String, i64, String) = before
            .query_row(
                "SELECT content, revision, updated_at FROM drafts WHERE id = 'quick_capture'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();
        let applied_versions = before
            .prepare("SELECT version FROM schema_migrations ORDER BY version")
            .unwrap()
            .query_map([], |row| row.get::<_, i64>(0))
            .unwrap()
            .collect::<rusqlite::Result<Vec<_>>>()
            .unwrap();
        assert_eq!(applied_versions, vec![1, 2, 3, 4]);
        drop(before);

        let (write_conn, _read_conn) = open_database(&legacy_db).unwrap();

        let entries_after = write_conn
            .prepare(
                "SELECT id, current_content, revision, updated_at, deleted_at FROM entries ORDER BY id",
            )
            .unwrap()
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, Option<String>>(4)?,
                ))
            })
            .unwrap()
            .collect::<rusqlite::Result<Vec<_>>>()
            .unwrap();
        assert_eq!(entries_after, entries_before);
        let fts_after = write_conn
            .prepare("SELECT entry_id, current_content FROM entries_fts ORDER BY entry_id")
            .unwrap()
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })
            .unwrap()
            .collect::<rusqlite::Result<Vec<_>>>()
            .unwrap();
        assert_eq!(fts_after, fts_before);

        let legacy_documents = write_conn
            .prepare(
                "SELECT entry_id, legacy_content, entry_revision
                 FROM entry_documents ORDER BY entry_id",
            )
            .unwrap()
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, i64>(2)?,
                ))
            })
            .unwrap()
            .collect::<rusqlite::Result<Vec<_>>>()
            .unwrap();
        let expected_legacy_documents = entries_before
            .iter()
            .map(|(id, current_content, revision, _, _)| {
                (id.clone(), current_content.clone(), *revision)
            })
            .collect::<Vec<_>>();
        assert_eq!(legacy_documents, expected_legacy_documents);
        let projected_entry_count: i64 = write_conn
            .query_row("SELECT COUNT(DISTINCT entry_id) FROM blocks", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(projected_entry_count, entries_before.len() as i64);

        let tags_after = write_conn
            .prepare(
                "SELECT et.entry_id, t.name
                 FROM entry_tags et JOIN tags t ON t.id = et.tag_id
                 ORDER BY et.entry_id, t.name",
            )
            .unwrap()
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })
            .unwrap()
            .collect::<rusqlite::Result<Vec<_>>>()
            .unwrap();
        assert_eq!(tags_after, tags_before);

        let links_after = write_conn
            .prepare(
                "SELECT source_entry_id, ordinal, raw_target, normalized_target, target_entry_id
                 FROM entry_links ORDER BY source_entry_id, ordinal",
            )
            .unwrap()
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, Option<String>>(4)?,
                ))
            })
            .unwrap()
            .collect::<rusqlite::Result<Vec<_>>>()
            .unwrap();
        assert_eq!(links_after, links_before);

        let draft_after: (String, i64, String) = write_conn
            .query_row(
                "SELECT content, revision, updated_at FROM drafts WHERE id = 'quick_capture'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();
        assert_eq!(draft_after, draft_before);
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
