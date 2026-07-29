use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct MarkdownImportPreview {
    pub session_id: String,
    pub file_count: u32,
    #[ts(type = "number")]
    pub total_bytes: u64,
    pub paths: Vec<String>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct MarkdownImportReport {
    pub imported_count: u32,
    pub skipped_count: u32,
    pub failed_count: u32,
    pub failures: Vec<String>,
}
