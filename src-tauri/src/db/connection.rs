use std::path::Path;

use rusqlite::Connection;

use crate::error::AppResult;

pub fn open_database(path: &Path) -> AppResult<Connection> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut conn = Connection::open(path)?;
    apply_pragmas(&conn)?;
    super::migrations::run_migrations(&mut conn)?;
    Ok(conn)
}

#[cfg(test)]
pub fn open_in_memory() -> AppResult<Connection> {
    let mut conn = Connection::open_in_memory()?;
    apply_pragmas(&conn)?;
    super::migrations::run_migrations(&mut conn)?;
    Ok(conn)
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
