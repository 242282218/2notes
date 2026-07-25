use rusqlite::{params, Connection, OptionalExtension, Transaction};

use crate::error::AppResult;

pub struct SettingsRepo;

impl SettingsRepo {
    pub fn get_string(conn: &Connection, key: &str) -> AppResult<Option<String>> {
        let value = conn
            .query_row(
                "SELECT value FROM settings WHERE key = ?1",
                [key],
                |row| row.get::<_, String>(0),
            )
            .optional()?;
        Ok(value)
    }

    pub fn set_bool(tx: &Transaction<'_>, key: &str, value: bool, now: &str) -> AppResult<()> {
        Self::set_string(tx, key, &value.to_string(), now)
    }

    pub fn set_string(tx: &Transaction<'_>, key: &str, value: &str, now: &str) -> AppResult<()> {
        tx.execute(
            "
            INSERT INTO settings(key, value, updated_at)
            VALUES (?1, ?2, ?3)
            ON CONFLICT(key) DO UPDATE SET value = ?2, updated_at = ?3
            ",
            params![key, value, now],
        )?;
        Ok(())
    }
}
