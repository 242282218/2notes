use std::path::Path;

use rusqlite::Connection;
#[cfg(test)]
use rusqlite::OpenFlags;

use crate::error::AppResult;

/// Open the database and return a (write_connection, read_connection) pair.
/// WAL mode enables concurrent reads via the separate read connection.
/// Migrations are applied only on the write connection.
pub fn open_database(path: &Path) -> AppResult<(Connection, Connection)> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut write_conn = Connection::open(path)?;
    apply_pragmas(&write_conn)?;
    super::migrations::run_migrations(&mut write_conn)?;
    if let Err(err) = super::repos::KnowledgeRepo::ensure_link_index(&mut write_conn) {
        log::error!("knowledge_link_index_rebuild_failed source={err}");
    }

    let read_conn = Connection::open(path)?;
    apply_pragmas(&read_conn)?;

    Ok((write_conn, read_conn))
}

#[cfg(test)]
/// Open two connections to the same in-memory database.
pub fn open_in_memory() -> AppResult<(Connection, Connection)> {
    let uri = format!(
        "file:2notes-test-{}?mode=memory&cache=shared",
        uuid::Uuid::new_v4()
    );
    let flags = OpenFlags::SQLITE_OPEN_URI
        | OpenFlags::SQLITE_OPEN_READ_WRITE
        | OpenFlags::SQLITE_OPEN_CREATE;
    let mut write_conn = Connection::open_with_flags(&uri, flags)?;
    apply_pragmas(&write_conn)?;
    super::migrations::run_migrations(&mut write_conn)?;
    super::repos::KnowledgeRepo::ensure_link_index(&mut write_conn)?;

    let read_conn = Connection::open_with_flags(&uri, flags)?;
    apply_pragmas(&read_conn)?;

    Ok((write_conn, read_conn))
}

fn apply_pragmas(conn: &Connection) -> AppResult<()> {
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
