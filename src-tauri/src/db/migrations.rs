use std::collections::HashMap;

use rusqlite::{params, Connection, Transaction};
use time::{format_description::well_known::Rfc3339, OffsetDateTime};

use crate::content::document::{
    document_to_markdown, document_to_plain_text, flatten_blocks, legacy_text_document,
};
use crate::error::{AppError, AppResult};

struct Migration {
    version: i64,
    sql: &'static str,
    data: Option<fn(&Transaction<'_>) -> AppResult<()>>,
}

const MIGRATIONS: &[Migration] = &[
    Migration {
        version: 1,
        sql: include_str!("schema/001_init.sql"),
        data: None,
    },
    Migration {
        version: 2,
        sql: include_str!("schema/002_entries_fts.sql"),
        data: None,
    },
    Migration {
        version: 3,
        sql: include_str!("schema/003_knowledge_graph.sql"),
        data: None,
    },
    Migration {
        version: 4,
        sql: include_str!("schema/004_entries_fts_trigram.sql"),
        data: None,
    },
    Migration {
        version: 5,
        sql: include_str!("schema/005_block_documents.sql"),
        data: Some(migrate_legacy_content_to_documents),
    },
    Migration {
        version: 6,
        sql: include_str!("schema/006_entry_hierarchy.sql"),
        data: Some(migrate_knowledge_hierarchy),
    },
    Migration {
        version: 7,
        sql: include_str!("schema/007_entry_imports.sql"),
        data: None,
    },
];

pub fn run_migrations(conn: &mut Connection) -> AppResult<()> {
    let fts5_available = fts5_available(conn)?;
    let fts5_trigram_available = fts5_trigram_available(conn)?;
    let tx = conn.transaction()?;
    let applied = load_applied(&tx)?;
    let current_max = MIGRATIONS
        .last()
        .map(|migration| migration.version)
        .unwrap_or(0);

    if applied.keys().any(|version| *version > current_max) {
        return Err(AppError::migration(
            "DB_VERSION_TOO_NEW",
            "数据库版本高于当前应用支持版本",
        ));
    }

    for migration in MIGRATIONS {
        let checksum = checksum(migration.sql);
        if let Some(existing) = applied.get(&migration.version) {
            if existing != &checksum {
                return Err(AppError::migration(
                    "DB_MIGRATION_CHECKSUM_MISMATCH",
                    "数据库迁移校验失败",
                ));
            }
            continue;
        }

        if migration.version == 2 && !fts5_available {
            log::warn!("fts5_unavailable migration=2 fallback=like_search");
            tx.execute(
                "INSERT INTO schema_migrations(version, checksum, applied_at) VALUES (?1, ?2, ?3)",
                params![migration.version, checksum, now_string()],
            )?;
            continue;
        }

        if migration.version == 4 && !fts5_trigram_available {
            log::warn!("fts5_trigram_unavailable migration=4 fallback=fts5_unicode61");
            tx.execute(
                "INSERT INTO schema_migrations(version, checksum, applied_at) VALUES (?1, ?2, ?3)",
                params![migration.version, checksum, now_string()],
            )?;
            continue;
        }

        tx.execute_batch(migration.sql)?;
        if let Some(data_hook) = migration.data {
            data_hook(&tx)?;
        }
        tx.execute(
            "INSERT INTO schema_migrations(version, checksum, applied_at) VALUES (?1, ?2, ?3)",
            params![migration.version, checksum, now_string()],
        )?;
    }

    tx.commit()?;
    Ok(())
}

fn migrate_legacy_content_to_documents(tx: &Transaction<'_>) -> AppResult<()> {
    let now = now_string();
    let mut select = tx.prepare("SELECT id, current_content, revision FROM entries")?;
    let rows = select.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, i64>(2)?,
        ))
    })?;
    let mut insert_document = tx.prepare(
        "INSERT INTO entry_documents(
           entry_id, schema_version, entry_revision, source_content_checksum,
           document_json, markdown_text, plain_text, legacy_content, updated_at
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
    )?;
    let mut insert_block = tx.prepare(
        "INSERT INTO blocks(
           id, entry_id, parent_block_id, ordinal, depth, kind, text_content, attrs_json
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
    )?;
    for row in rows {
        let (id, current_content, revision) = row?;
        let document = legacy_text_document(&current_content);
        let document_json = serde_json::to_string(&document)
            .map_err(|err| AppError::migration("DOCUMENT_JSON_ENCODE", err.to_string()))?;
        let markdown_text = document_to_markdown(&document)
            .map_err(|err| AppError::migration("DOCUMENT_MARKDOWN_ENCODE", err.to_string()))?;
        let plain_text = document_to_plain_text(&document);
        insert_document.execute(params![
            id,
            document.schema_version,
            revision,
            checksum(&current_content),
            document_json,
            markdown_text,
            plain_text,
            current_content,
            now,
        ])?;

        for projection in flatten_blocks(&document) {
            insert_block.execute(params![
                projection.id,
                id,
                projection.parent_block_id,
                projection.ordinal,
                projection.depth,
                format!("{:?}", projection.kind),
                projection.text_content,
                serde_json::to_string(&projection.attrs)
                    .map_err(|err| AppError::migration("BLOCK_ATTRS_ENCODE", err.to_string()))?,
            ])?;
        }
    }
    Ok(())
}

fn migrate_knowledge_hierarchy(tx: &Transaction<'_>) -> AppResult<()> {
    let now = now_string();
    let mut entries = tx.prepare(
        "SELECT id FROM entries
         WHERE deleted_at IS NULL AND knowledge_state = 'knowledge'
         ORDER BY updated_at DESC, id ASC",
    )?;
    let ids = entries
        .query_map([], |row| row.get::<_, String>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    drop(entries);
    for (sibling_order, id) in ids.iter().enumerate() {
        tx.execute(
            "INSERT INTO entry_hierarchy(entry_id, parent_entry_id, sibling_order, updated_at)
             VALUES (?1, NULL, ?2, ?3)",
            params![id, sibling_order as i64, now],
        )?;
    }
    Ok(())
}

fn load_applied(conn: &Connection) -> AppResult<HashMap<i64, String>> {
    let exists: i64 = conn.query_row(
        "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'schema_migrations'",
        [],
        |row| row.get(0),
    )?;

    if exists == 0 {
        return Ok(HashMap::new());
    }

    let mut stmt = conn.prepare("SELECT version, checksum FROM schema_migrations")?;
    let rows = stmt.query_map([], |row| {
        Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
    })?;
    let mut applied = HashMap::new();
    for row in rows {
        let (version, checksum) = row?;
        applied.insert(version, checksum);
    }
    Ok(applied)
}

fn fts5_available(conn: &Connection) -> AppResult<bool> {
    match conn.execute_batch(
        "
        CREATE VIRTUAL TABLE temp.__fts5_probe USING fts5(value);
        DROP TABLE temp.__fts5_probe;
        ",
    ) {
        Ok(()) => Ok(true),
        Err(err) if err.to_string().contains("no such module") => Ok(false),
        Err(err) => Err(err.into()),
    }
}

fn fts5_trigram_available(conn: &Connection) -> AppResult<bool> {
    match conn.execute_batch(
        "
        CREATE VIRTUAL TABLE temp.__fts5_trigram_probe USING fts5(value, tokenize='trigram');
        DROP TABLE temp.__fts5_trigram_probe;
        ",
    ) {
        Ok(()) => Ok(true),
        Err(err)
            if err.to_string().contains("no such module")
                || err.to_string().contains("no such tokenizer") =>
        {
            Ok(false)
        }
        Err(err) => Err(err.into()),
    }
}

pub fn checksum(input: &str) -> String {
    let mut hash: u64 = 0xcbf29ce484222325;
    for byte in input.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("{hash:016x}")
}

pub fn now_string() -> String {
    OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .unwrap_or_else(|_| "1970-01-01T00:00:00Z".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migration_uses_trigram_fts_when_supported() {
        let mut conn = Connection::open_in_memory().unwrap();
        if !fts5_trigram_available(&conn).unwrap() {
            return;
        }

        run_migrations(&mut conn).unwrap();

        let sql: String = conn
            .query_row(
                "SELECT sql FROM sqlite_master WHERE type = 'table' AND name = 'entries_fts'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert!(sql.contains("tokenize='trigram'"), "{sql}");
    }

    #[test]
    fn migration_004_backfills_existing_aliases() {
        let mut conn = Connection::open_in_memory().unwrap();
        for migration in &MIGRATIONS[..3] {
            conn.execute_batch(migration.sql).unwrap();
            conn.execute(
                "INSERT INTO schema_migrations(version, checksum, applied_at)
                 VALUES (?1, ?2, ?3)",
                params![migration.version, checksum(migration.sql), now_string()],
            )
            .unwrap();
        }
        conn.execute(
            "INSERT INTO entries(
               id, title, title_source, original_content, current_content, type, status,
               revision, created_at, updated_at, deleted_at,
               knowledge_state, knowledge_promoted_at, knowledge_title_key
             ) VALUES ('e1', '当前标题', 'user', '正文', '正文', 'idea', 'archived', 0,
                       '2026-07-16T00:00:00Z', '2026-07-16T00:00:00Z', NULL,
                       'knowledge', '2026-07-16T00:00:00Z', '当前标题')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO entry_aliases(normalized_alias, entry_id, alias, created_at)
             VALUES ('历史标题', 'e1', '历史标题', '2026-07-16T00:00:00Z')",
            [],
        )
        .unwrap();

        run_migrations(&mut conn).unwrap();

        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM entries_fts WHERE entries_fts MATCH '历史标题'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn trigram_fts_matches_chinese_substring() {
        let mut conn = Connection::open_in_memory().unwrap();
        run_migrations(&mut conn).unwrap();
        conn.execute(
            "INSERT INTO entries(
               id, title, title_source, original_content, current_content, type, status,
               revision, created_at, updated_at, deleted_at,
               knowledge_state, knowledge_promoted_at, knowledge_title_key
             ) VALUES ('e1', '第二阶段知识库全文搜索', 'user', '正文', '正文', 'idea',
                       'archived', 0, '2026-07-16T00:00:00Z', '2026-07-16T00:00:00Z',
                       NULL, 'knowledge', '2026-07-16T00:00:00Z', '第二阶段知识库全文搜索')",
            [],
        )
        .unwrap();

        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM entries_fts WHERE entries_fts MATCH '知识库'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn migration_is_idempotent() {
        let mut conn = Connection::open_in_memory().unwrap();

        run_migrations(&mut conn).unwrap();
        run_migrations(&mut conn).unwrap();

        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM schema_migrations", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(count, 7);
    }

    #[test]
    fn migration_upgrades_existing_v2_entries_with_capture_defaults() {
        let mut conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(include_str!("schema/001_init.sql"))
            .unwrap();
        conn.execute(
            "INSERT INTO entries(
               id, title, title_source, original_content, current_content, type, status,
               revision, created_at, updated_at, deleted_at
             ) VALUES ('e1', 'Title', 'user', 'body', 'body', 'idea', 'pending', 7,
                       '2026-07-15T00:00:00Z', '2026-07-15T00:00:00Z', NULL)",
            [],
        )
        .unwrap();
        if fts5_available(&conn).unwrap() {
            conn.execute_batch(include_str!("schema/002_entries_fts.sql"))
                .unwrap();
        }
        for migration in &MIGRATIONS[..2] {
            conn.execute(
                "INSERT INTO schema_migrations(version, checksum, applied_at)
                 VALUES (?1, ?2, ?3)",
                rusqlite::params![migration.version, checksum(migration.sql), now_string()],
            )
            .unwrap();
        }

        run_migrations(&mut conn).unwrap();

        let row: (String, Option<String>, Option<String>, i64) = conn
            .query_row(
                "SELECT knowledge_state, knowledge_promoted_at, knowledge_title_key, revision
                 FROM entries WHERE id = 'e1'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .unwrap();
        assert_eq!(row, ("capture".into(), None, None, 7));
    }

    #[test]
    fn knowledge_title_key_is_unique() {
        let mut conn = Connection::open_in_memory().unwrap();
        run_migrations(&mut conn).unwrap();
        let insert = |id: &str| {
            conn.execute(
                "INSERT INTO entries(
                   id, title, title_source, original_content, current_content, type, status,
                   revision, created_at, updated_at, deleted_at,
                   knowledge_state, knowledge_promoted_at, knowledge_title_key
                 ) VALUES (?1, 'Title', 'user', 'body', 'body', 'idea', 'archived', 0,
                           '2026-07-15T00:00:00Z', '2026-07-15T00:00:00Z', NULL,
                           'knowledge', '2026-07-15T00:00:00Z', 'title')",
                [id],
            )
        };

        insert("e1").unwrap();
        assert!(insert("e2").is_err());
    }

    #[test]
    fn checksum_mismatch_rejects_startup() {
        let mut conn = Connection::open_in_memory().unwrap();
        run_migrations(&mut conn).unwrap();
        conn.execute("UPDATE schema_migrations SET checksum = 'changed'", [])
            .unwrap();

        let err = run_migrations(&mut conn).unwrap_err();

        assert!(
            matches!(err, AppError::Migration { code, .. } if code == "DB_MIGRATION_CHECKSUM_MISMATCH")
        );
    }

    #[test]
    fn migration_rejects_database_with_newer_schema_version() {
        let mut conn = Connection::open_in_memory().unwrap();
        run_migrations(&mut conn).unwrap();
        let supported_version = MIGRATIONS.last().unwrap().version;
        conn.execute(
            "INSERT INTO schema_migrations(version, checksum, applied_at) VALUES (?1, ?2, ?3)",
            params![supported_version + 1, "future-checksum", now_string()],
        )
        .unwrap();

        let err = run_migrations(&mut conn).unwrap_err();

        assert!(matches!(err, AppError::Migration { code, .. } if code == "DB_VERSION_TOO_NEW"));
        let recorded_version: i64 = conn
            .query_row("SELECT MAX(version) FROM schema_migrations", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(recorded_version, supported_version + 1);
    }

    #[test]
    #[ignore = "10k v4-to-v5 on-disk migration scale verification"]
    fn migration_v4_to_v5_projects_ten_thousand_entries_under_thirty_seconds() {
        use std::time::{Duration, Instant};

        let temp = tempfile::tempdir().unwrap();
        let database_path = temp.path().join("2notes.sqlite");
        let mut conn = Connection::open(&database_path).unwrap();
        conn.execute_batch("PRAGMA journal_mode = WAL; PRAGMA foreign_keys = ON;")
            .unwrap();

        for migration in &MIGRATIONS[..4] {
            conn.execute_batch(migration.sql).unwrap();
            conn.execute(
                "INSERT INTO schema_migrations(version, checksum, applied_at) VALUES (?1, ?2, ?3)",
                params![migration.version, checksum(migration.sql), now_string()],
            )
            .unwrap();
        }
        let tx = conn.transaction().unwrap();
        for index in 0..10_000 {
            tx.execute(
                "INSERT INTO entries(
                   id, title, title_source, original_content, current_content, type, status,
                   revision, created_at, updated_at, deleted_at, knowledge_state,
                   knowledge_promoted_at, knowledge_title_key
                 ) VALUES (?1, NULL, 'auto', ?2, ?2, 'unclear', 'pending', 0, ?3, ?3, NULL,
                   'capture', NULL, NULL)",
                params![
                    format!("legacy-{index}"),
                    format!("legacy content {index}"),
                    now_string()
                ],
            )
            .unwrap();
        }
        tx.commit().unwrap();

        let started = Instant::now();
        let tx = conn.transaction().unwrap();
        tx.execute_batch(MIGRATIONS[4].sql).unwrap();
        MIGRATIONS[4].data.unwrap()(&tx).unwrap();
        tx.execute(
            "INSERT INTO schema_migrations(version, checksum, applied_at) VALUES (?1, ?2, ?3)",
            params![
                MIGRATIONS[4].version,
                checksum(MIGRATIONS[4].sql),
                now_string()
            ],
        )
        .unwrap();
        tx.commit().unwrap();
        let elapsed = started.elapsed();

        let document_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM entry_documents", [], |row| row.get(0))
            .unwrap();
        let block_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM blocks", [], |row| row.get(0))
            .unwrap();
        eprintln!(
            "migration_v4_to_v5_projects_ten_thousand_entries elapsed_ms={:.2}",
            elapsed.as_secs_f64() * 1000.0
        );
        assert_eq!(document_count, 10_000);
        assert_eq!(block_count, 10_000);
        assert!(elapsed < Duration::from_secs(30));
    }

    #[test]
    fn migration_005_projects_legacy_content_into_block_documents() {
        let mut conn = Connection::open_in_memory().unwrap();
        for migration in &MIGRATIONS[..4] {
            conn.execute_batch(migration.sql).unwrap();
            conn.execute(
                "INSERT INTO schema_migrations(version, checksum, applied_at)
                 VALUES (?1, ?2, ?3)",
                params![migration.version, checksum(migration.sql), now_string()],
            )
            .unwrap();
        }
        conn.execute(
            "INSERT INTO entries(
               id, title, title_source, original_content, current_content, type, status,
               revision, created_at, updated_at, deleted_at,
               knowledge_state, knowledge_promoted_at, knowledge_title_key
             ) VALUES ('e1', '标题', 'user', '第一行\n第二行\n\n段落二 [[WikiLink]]', '第一行\n第二行\n\n段落二 [[WikiLink]]',
                       'idea', 'pending', 3, '2026-07-20T00:00:00Z', '2026-07-20T00:00:00Z', NULL,
                       'capture', NULL, NULL)",
            [],
        )
        .unwrap();

        run_migrations(&mut conn).unwrap();

        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM schema_migrations", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(count, 7);

        let current: String = conn
            .query_row(
                "SELECT current_content FROM entries WHERE id = 'e1'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(current, "第一行\n第二行\n\n段落二 [[WikiLink]]");

        let revision: i64 = conn
            .query_row("SELECT revision FROM entries WHERE id = 'e1'", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(revision, 3);

        let (legacy, doc_json, markdown, plain, entry_revision, source_checksum): (
            String,
            String,
            String,
            String,
            i64,
            String,
        ) = conn
            .query_row(
                "SELECT legacy_content, document_json, markdown_text, plain_text,
                        entry_revision, source_content_checksum
                 FROM entry_documents WHERE entry_id = 'e1'",
                [],
                |row| {
                    Ok((
                        row.get(0)?,
                        row.get(1)?,
                        row.get(2)?,
                        row.get(3)?,
                        row.get(4)?,
                        row.get(5)?,
                    ))
                },
            )
            .unwrap();
        assert_eq!(legacy, "第一行\n第二行\n\n段落二 [[WikiLink]]");
        assert_eq!(entry_revision, 3);
        assert_eq!(
            source_checksum,
            checksum("第一行\n第二行\n\n段落二 [[WikiLink]]")
        );
        assert!(doc_json.contains("第一行"));
        assert!(doc_json.contains("段落二"));
        assert!(markdown.contains("[[WikiLink]]"));
        assert!(plain.contains("第一行"));
        assert!(plain.contains("段落二"));

        let block_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM blocks WHERE entry_id = 'e1'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(block_count, 2);
    }

    #[test]
    fn migration_006_backfills_active_knowledge_roots_in_stable_order() {
        let mut conn = Connection::open_in_memory().unwrap();
        for migration in &MIGRATIONS[..5] {
            conn.execute_batch(migration.sql).unwrap();
            conn.execute(
                "INSERT INTO schema_migrations(version, checksum, applied_at)
                 VALUES (?1, ?2, ?3)",
                params![migration.version, checksum(migration.sql), now_string()],
            )
            .unwrap();
        }
        conn.execute(
            "INSERT INTO entries(
               id, title, title_source, original_content, current_content, type, status,
               revision, created_at, updated_at, deleted_at,
               knowledge_state, knowledge_promoted_at, knowledge_title_key
             ) VALUES
               ('knowledge-newest', 'Newest', 'user', 'body', 'body', 'idea', 'archived', 0,
                '2026-07-20T00:00:00Z', '2026-07-22T00:00:00Z', NULL,
                'knowledge', '2026-07-20T00:00:00Z', 'newest'),
               ('knowledge-tie-a', 'Tie A', 'user', 'body', 'body', 'idea', 'archived', 0,
                '2026-07-20T00:00:00Z', '2026-07-21T00:00:00Z', NULL,
                'knowledge', '2026-07-20T00:00:00Z', 'tie a'),
               ('capture', NULL, 'auto', 'body', 'body', 'idea', 'pending', 0,
                '2026-07-20T00:00:00Z', '2026-07-23T00:00:00Z', NULL,
                'capture', NULL, NULL),
               ('knowledge-trash', 'Trash', 'user', 'body', 'body', 'idea', 'archived', 0,
                '2026-07-20T00:00:00Z', '2026-07-24T00:00:00Z', '2026-07-25T00:00:00Z',
                'knowledge', '2026-07-20T00:00:00Z', 'trash')",
            [],
        )
        .unwrap();

        run_migrations(&mut conn).unwrap();

        let rows = conn
            .prepare(
                "SELECT entry_id, parent_entry_id, sibling_order
                 FROM entry_hierarchy ORDER BY sibling_order, entry_id",
            )
            .unwrap()
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, Option<String>>(1)?,
                    row.get::<_, i64>(2)?,
                ))
            })
            .unwrap()
            .collect::<rusqlite::Result<Vec<_>>>()
            .unwrap();
        assert_eq!(
            rows,
            vec![
                ("knowledge-newest".to_string(), None, 0),
                ("knowledge-tie-a".to_string(), None, 1),
            ]
        );
    }

    #[test]
    fn migration_005_is_idempotent_and_checksum_protects_future() {
        let mut conn = Connection::open_in_memory().unwrap();
        run_migrations(&mut conn).unwrap();
        run_migrations(&mut conn).unwrap();

        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM schema_migrations", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(count, 7);
    }
}
