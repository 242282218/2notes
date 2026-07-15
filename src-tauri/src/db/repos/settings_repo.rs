use rusqlite::{params, Transaction};

use crate::error::AppResult;

pub struct SettingsRepo;

impl SettingsRepo {
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
