use std::path::Path;

use rusqlite::{Connection, OpenFlags};

use crate::error::AppResult;

/// Number of read-only connections kept open for concurrent readers.
/// WAL mode allows multiple readers to execute in parallel, so a single
/// shared read connection would serialize every read query in the app.
pub const READ_POOL_SIZE: usize = 4;

/// Open the database and return a (write_connection, read_connection) pair.
/// WAL mode enables concurrent reads via the separate read connection.
/// Migrations are applied only on the write connection.
/// The read connection is opened read-only and also sets `PRAGMA query_only = ON`.
pub fn open_database(path: &Path) -> AppResult<(Connection, Connection)> {
    let (write_conn, first_read) = open_database_with_read_pool(path)?;
    let mut read_conns = first_read.into_iter();
    let read_conn = read_conns
        .next()
        .expect("open_database_with_read_pool always returns at least one read connection");
    Ok((write_conn, read_conn))
}

/// Open the database and return the write connection plus a read pool.
/// Shared by startup and the backup-restore path so both keep the same
/// connection topology (one writer + a pool of read-only readers).
pub fn open_database_with_read_pool(path: &Path) -> AppResult<(Connection, Vec<Connection>)> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut write_conn = Connection::open(path)?;
    apply_write_pragmas(&write_conn)?;
    super::migrations::run_migrations(&mut write_conn)?;
    super::repos::EntriesRepo::repair_documents(&mut write_conn).map_err(|err| {
        crate::error::AppError::system(
            "DB_DOCUMENT_REPAIR_FAILED",
            format!("启动时修复文档投影失败: {err}"),
        )
    })?;
    super::repos::KnowledgeRepo::ensure_link_index(&mut write_conn).map_err(|err| {
        crate::error::AppError::system(
            "DB_LINK_INDEX_FAILED",
            format!("启动时重建知识索引失败: {err}"),
        )
    })?;

    let read_conns = open_read_connections(path, READ_POOL_SIZE)?;

    Ok((write_conn, read_conns))
}

/// Open `count` read-only connections to the database, each with read pragmas.
pub fn open_read_connections(path: &Path, count: usize) -> AppResult<Vec<Connection>> {
    let read_flags = OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_URI;
    let mut conns = Vec::with_capacity(count);
    for _ in 0..count {
        let conn = Connection::open_with_flags(path, read_flags)?;
        apply_read_pragmas(&conn)?;
        conns.push(conn);
    }
    Ok(conns)
}

#[cfg(test)]
/// Open two connections to the same in-memory database.
/// Shared-memory URIs cannot use pure `SQLITE_OPEN_READ_ONLY`, so the read
/// connection is protected with `PRAGMA query_only = ON` instead.
pub fn open_in_memory() -> AppResult<(Connection, Connection)> {
    let (write_conn, mut read_conns) = open_in_memory_pool(1)?;
    let read_conn = read_conns
        .pop()
        .expect("open_in_memory_pool returns exactly one read connection");
    Ok((write_conn, read_conn))
}

#[cfg(test)]
/// Open a write connection plus `count` read connections on a fresh
/// shared-cache in-memory database, for read-pool concurrency tests.
pub fn open_in_memory_pool(count: usize) -> AppResult<(Connection, Vec<Connection>)> {
    let uri = format!(
        "file:2notes-test-{}?mode=memory&cache=shared",
        uuid::Uuid::new_v4()
    );
    let flags = OpenFlags::SQLITE_OPEN_URI
        | OpenFlags::SQLITE_OPEN_READ_WRITE
        | OpenFlags::SQLITE_OPEN_CREATE;
    let mut write_conn = Connection::open_with_flags(&uri, flags)?;
    apply_write_pragmas(&write_conn)?;
    super::migrations::run_migrations(&mut write_conn)?;
    super::repos::EntriesRepo::repair_documents(&mut write_conn)?;
    super::repos::KnowledgeRepo::ensure_link_index(&mut write_conn)?;

    let mut read_conns = Vec::with_capacity(count);
    for _ in 0..count {
        let conn = Connection::open_with_flags(&uri, flags)?;
        apply_read_pragmas(&conn)?;
        read_conns.push(conn);
    }

    Ok((write_conn, read_conns))
}

fn apply_write_pragmas(conn: &Connection) -> AppResult<()> {
    conn.execute_batch(
        "
        PRAGMA foreign_keys = ON;
        PRAGMA journal_mode = WAL;
        PRAGMA busy_timeout = 5000;
        PRAGMA synchronous = NORMAL;
        ",
    )?;
    Ok(())
}

fn apply_read_pragmas(conn: &Connection) -> AppResult<()> {
    conn.execute_batch(
        "
        PRAGMA query_only = ON;
        PRAGMA foreign_keys = ON;
        PRAGMA busy_timeout = 5000;
        PRAGMA cache_size = -32768;
        ",
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::{Error, ErrorCode};

    #[test]
    fn read_connection_rejects_writes() {
        let temp = tempfile::tempdir().unwrap();
        let db_path = temp.path().join("readonly-test.db");

        let (write_conn, read_conn) = open_database(&db_path).unwrap();

        write_conn
            .execute(
                "INSERT INTO settings (key, value, updated_at) VALUES (?1, ?2, ?3)",
                ("readonly_probe", "write-ok", "2026-01-01T00:00:00Z"),
            )
            .unwrap();

        let value: String = read_conn
            .query_row(
                "SELECT value FROM settings WHERE key = ?1",
                ["readonly_probe"],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(value, "write-ok");

        let err = read_conn
            .execute(
                "INSERT INTO settings (key, value, updated_at) VALUES (?1, ?2, ?3)",
                ("readonly_should_fail", "nope", "2026-01-01T00:00:00Z"),
            )
            .expect_err("read connection must reject INSERT");

        match err {
            Error::SqliteFailure(e, _) => {
                assert_eq!(
                    e.code,
                    ErrorCode::ReadOnly,
                    "expected SQLITE_READONLY, got {e:?}"
                );
            }
            other => panic!("expected SqliteFailure(ReadOnly), got {other:?}"),
        }
    }

    #[test]
    fn in_memory_read_connection_rejects_writes_via_query_only() {
        let (write_conn, read_conn) = open_in_memory().unwrap();

        write_conn
            .execute(
                "INSERT INTO settings (key, value, updated_at) VALUES (?1, ?2, ?3)",
                ("mem_probe", "write-ok", "2026-01-01T00:00:00Z"),
            )
            .unwrap();

        let value: String = read_conn
            .query_row(
                "SELECT value FROM settings WHERE key = ?1",
                ["mem_probe"],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(value, "write-ok");

        let err = read_conn
            .execute(
                "INSERT INTO settings (key, value, updated_at) VALUES (?1, ?2, ?3)",
                ("mem_should_fail", "nope", "2026-01-01T00:00:00Z"),
            )
            .expect_err("in-memory read connection must reject INSERT via query_only");

        match err {
            Error::SqliteFailure(e, _) => {
                assert_eq!(
                    e.code,
                    ErrorCode::ReadOnly,
                    "expected SQLITE_READONLY, got {e:?}"
                );
            }
            other => panic!("expected SqliteFailure(ReadOnly), got {other:?}"),
        }
    }
}
