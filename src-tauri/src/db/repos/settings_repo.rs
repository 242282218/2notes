use rusqlite::{params, Connection, OptionalExtension, Transaction};

use crate::error::{AppError, AppResult};
use crate::types::settings::ThemeMode;

pub const DEFAULT_BACKUP_RETENTION_COUNT: i64 = 10;
pub const MIN_BACKUP_RETENTION_COUNT: i64 = 1;
pub const MAX_BACKUP_RETENTION_COUNT: i64 = 100;
pub const DEFAULT_SHORTCUT: &str = "Ctrl+Alt+Space";

pub struct SettingsRepo;

impl SettingsRepo {
    pub fn get_string(conn: &Connection, key: &str) -> AppResult<Option<String>> {
        let value = conn
            .query_row("SELECT value FROM settings WHERE key = ?1", [key], |row| {
                row.get::<_, String>(0)
            })
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

    pub fn get_backup_retention_count(conn: &Connection) -> AppResult<i64> {
        let value = Self::get_string(conn, "backup_retention_count")?
            .and_then(|value| value.parse::<i64>().ok())
            .filter(|value| {
                (MIN_BACKUP_RETENTION_COUNT..=MAX_BACKUP_RETENTION_COUNT).contains(value)
            });
        Ok(value.unwrap_or(DEFAULT_BACKUP_RETENTION_COUNT))
    }

    pub fn validate_backup_retention_count(value: i64) -> AppResult<()> {
        if !(MIN_BACKUP_RETENTION_COUNT..=MAX_BACKUP_RETENTION_COUNT).contains(&value) {
            return Err(AppError::validation(
                "BACKUP_RETENTION_INVALID",
                format!(
                    "自动备份保留数量必须在 {MIN_BACKUP_RETENTION_COUNT} 到 {MAX_BACKUP_RETENTION_COUNT} 之间"
                ),
            ));
        }
        Ok(())
    }

    pub fn set_backup_retention_count(
        tx: &Transaction<'_>,
        value: i64,
        now: &str,
    ) -> AppResult<()> {
        Self::validate_backup_retention_count(value)?;
        Self::set_string(tx, "backup_retention_count", &value.to_string(), now)
    }

    pub fn get_shortcut(conn: &Connection) -> AppResult<String> {
        Ok(Self::get_string(conn, "shortcut")?.unwrap_or_else(|| DEFAULT_SHORTCUT.to_string()))
    }

    pub fn validate_shortcut(value: &str) -> AppResult<()> {
        let normalized = value.trim();
        if normalized.is_empty() || !normalized.contains('+') || normalized.len() > 64 {
            return Err(AppError::validation(
                "SHORTCUT_INVALID",
                "快捷键必须包含修饰键和按键，例如 Ctrl+Alt+Space",
            ));
        }
        Ok(())
    }

    pub fn set_shortcut(tx: &Transaction<'_>, value: &str, now: &str) -> AppResult<()> {
        Self::validate_shortcut(value)?;
        Self::set_string(tx, "shortcut", value.trim(), now)
    }

    pub fn get_theme_mode(conn: &Connection) -> AppResult<ThemeMode> {
        let value = Self::get_string(conn, "theme_mode")?;
        Ok(value
            .as_deref()
            .and_then(ThemeMode::from_db)
            .unwrap_or(ThemeMode::System))
    }

    pub fn set_theme_mode(tx: &Transaction<'_>, theme_mode: ThemeMode, now: &str) -> AppResult<()> {
        Self::set_string(tx, "theme_mode", theme_mode.as_str(), now)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::connection::open_in_memory;

    #[test]
    fn missing_retention_uses_default() {
        let (conn, _) = open_in_memory().unwrap();
        assert_eq!(
            SettingsRepo::get_backup_retention_count(&conn).unwrap(),
            DEFAULT_BACKUP_RETENTION_COUNT
        );
    }

    #[test]
    fn retention_round_trips_through_settings_table() {
        let (mut conn, _) = open_in_memory().unwrap();
        let tx = conn.transaction().unwrap();
        SettingsRepo::set_backup_retention_count(&tx, 25, "2026-07-31T00:00:00Z").unwrap();
        tx.commit().unwrap();

        assert_eq!(SettingsRepo::get_backup_retention_count(&conn).unwrap(), 25);
    }

    #[test]
    fn retention_validation_rejects_out_of_range_values() {
        assert!(SettingsRepo::validate_backup_retention_count(0).is_err());
        assert!(SettingsRepo::validate_backup_retention_count(101).is_err());
        assert!(SettingsRepo::validate_backup_retention_count(10).is_ok());
    }
}
