use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum KnowledgeState {
    Capture,
    Knowledge,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeSuggestion {
    pub id: String,
    pub title: String,
    pub matched_alias: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct RelatedEntry {
    pub id: String,
    pub title: Option<String>,
    pub summary: String,
    pub occurrence_count: u32,
    pub deleted_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct UnresolvedWikiLink {
    pub raw_target: String,
    pub occurrence_count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeRelations {
    pub outgoing: Vec<RelatedEntry>,
    pub backlinks: Vec<RelatedEntry>,
    pub unresolved: Vec<UnresolvedWikiLink>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct SearchSnippetPart {
    pub text: String,
    pub highlighted: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct SearchSnippet {
    pub parts: Vec<SearchSnippetPart>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeIndexReport {
    pub indexed_sources: u32,
    pub link_occurrences: u32,
    pub unresolved_occurrences: u32,
    pub search_index_available: bool,
}
