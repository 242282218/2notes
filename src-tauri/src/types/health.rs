use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum HealthIssueKind {
    UnresolvedLink,
    OrphanKnowledge,
    UntaggedKnowledge,
    StaleCapture,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeHealthSummary {
    pub unresolved_link: u32,
    pub orphan_knowledge: u32,
    pub untagged_knowledge: u32,
    pub stale_capture: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct HealthIssue {
    pub entry_id: String,
    pub title: Option<String>,
    pub updated_at: String,
    pub raw_target: Option<String>,
    pub occurrence_count: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct HealthIssuePage {
    pub items: Vec<HealthIssue>,
    pub limit: u32,
    pub offset: u32,
    pub has_more: bool,
}
