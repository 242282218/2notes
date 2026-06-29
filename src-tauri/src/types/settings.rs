use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct AppSettings {
    pub data_dir: String,
    pub log_dir: String,
    pub shortcut: String,
    pub shortcut_registered: bool,
    pub shortcut_error: Option<String>,
    pub autostart_enabled: bool,
    pub backup_error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct SettingsPatch {
    pub autostart_enabled: Option<bool>,
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
