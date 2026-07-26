use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ThemeMode {
    System,
    Light,
    Dark,
}

impl ThemeMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::System => "system",
            Self::Light => "light",
            Self::Dark => "dark",
        }
    }

    pub fn from_db(value: &str) -> Option<Self> {
        match value {
            "system" => Some(Self::System),
            "light" => Some(Self::Light),
            "dark" => Some(Self::Dark),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct AppSettings {
    pub data_dir: String,
    pub log_dir: String,
    pub backup_dir: String,
    pub shortcut: String,
    pub shortcut_registered: bool,
    pub shortcut_error: Option<String>,
    pub autostart_enabled: bool,
    pub theme_mode: ThemeMode,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct SettingsPatch {
    pub autostart_enabled: Option<bool>,
    pub theme_mode: Option<ThemeMode>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct Draft {
    pub content: String,
    #[ts(type = "number")]
    pub revision: i64,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ExportResult {
    pub exported_count: usize,
    pub target_dir: String,
}

#[cfg(test)]
mod tests {
    use super::ThemeMode;

    #[test]
    fn theme_mode_serializes_lowercase() {
        assert_eq!(
            serde_json::to_string(&ThemeMode::System).unwrap(),
            "\"system\""
        );
        assert_eq!(
            serde_json::to_string(&ThemeMode::Light).unwrap(),
            "\"light\""
        );
        assert_eq!(serde_json::to_string(&ThemeMode::Dark).unwrap(), "\"dark\"");
    }

    #[test]
    fn theme_mode_deserializes_lowercase() {
        assert_eq!(
            serde_json::from_str::<ThemeMode>("\"system\"").unwrap(),
            ThemeMode::System
        );
        assert_eq!(
            serde_json::from_str::<ThemeMode>("\"light\"").unwrap(),
            ThemeMode::Light
        );
        assert_eq!(
            serde_json::from_str::<ThemeMode>("\"dark\"").unwrap(),
            ThemeMode::Dark
        );
    }

    #[test]
    fn theme_mode_rejects_invalid_strings() {
        assert!(serde_json::from_str::<ThemeMode>("\"blue\"").is_err());
        assert!(serde_json::from_str::<ThemeMode>("\"System\"").is_err());
        assert!(serde_json::from_str::<ThemeMode>("\"LIGHT\"").is_err());
        assert!(serde_json::from_str::<ThemeMode>("\"\"").is_err());
    }
}
