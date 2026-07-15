use std::collections::HashMap;

use rusqlite::{params, Connection};
use time::{format_description::well_known::Rfc3339, OffsetDateTime};

use crate::error::{AppError, AppResult};

const MIGRATIONS: &[(i64, &str)] = &[
    (1, include_str!("schema/001_init.sql")),
    (2, include_str!("schema/002_entries_fts.sql")),
    (3, include_str!("schema/003_knowledge_graph.sql")),
];

pub fn run_migrations(conn: &mut Connection) -> AppResult<()> {
    let fts5_available = fts5_available(conn)?;
    let tx = conn.transaction()?;
    let applied = load_applied(&tx)?;
    let current_max = MIGRATIONS.last().map(|(version, _)| *version).unwrap_or(0);

    if applied.keys().any(|version| *version > current_max) {
        return Err(AppError::migration(
            "DB_VERSION_TOO_NEW",
            "数据库版本高于当前应用支持版本",
        ));
    }

    for (version, sql) in MIGRATIONS {
        let checksum = checksum(sql);
        if let Some(existing) = applied.get(version) {
            if existing != &checksum {
                return Err(AppError::migration(
                    "DB_MIGRATION_CHECKSUM_MISMATCH",
                    "数据库迁移校验失败",
                ));
            }
            continue;
        }

        if *version == 2 && !fts5_available {
            log::warn!("fts5_unavailable migration=2 fallback=like_search");
            tx.execute(
                "INSERT INTO schema_migrations(version, checksum, applied_at) VALUES (?1, ?2, ?3)",
                params![version, checksum, now_string()],
            )?;
            continue;
        }

        tx.execute_batch(sql)?;
        tx.execute(
            "INSERT INTO schema_migrations(version, checksum, applied_at) VALUES (?1, ?2, ?3)",
            params![version, checksum, now_string()],
        )?;
    }

    tx.commit()?;
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
    fn migration_is_idempotent() {
        let mut conn = Connection::open_in_memory().unwrap();

        run_migrations(&mut conn).unwrap();
        run_migrations(&mut conn).unwrap();

        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM schema_migrations", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(count, 3);
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
        for (version, sql) in &MIGRATIONS[..2] {
            conn.execute(
                "INSERT INTO schema_migrations(version, checksum, applied_at)
                 VALUES (?1, ?2, ?3)",
                rusqlite::params![version, checksum(sql), now_string()],
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
}
