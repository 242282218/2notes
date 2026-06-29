use rusqlite::{params, Connection, OptionalExtension, Transaction};

use crate::error::AppResult;

pub struct SettingsRepo;

impl SettingsRepo {
    pub fn get_bool(conn: &Connection, key: &str, default: bool) -> AppResult<bool> {
        let value: Option<String> = conn
            .query_row(
                "SELECT value FROM settings WHERE key = ?1",
                params![key],
                |row| row.get(0),
            )
            .optional()?;
        Ok(value
            .as_deref()
            .map(|value| value == "true")
            .unwrap_or(default))
    }

    pub fn set_bool(tx: &Transaction<'_>, key: &str, value: bool, now: &str) -> AppResult<()> {
        tx.execute(
            "
            INSERT INTO settings(key, value, updated_at)
            VALUES (?1, ?2, ?3)
            ON CONFLICT(key) DO UPDATE SET value = ?2, updated_at = ?3
            ",
            params![key, value.to_string(), now],
        )?;
        Ok(())
    }
}
