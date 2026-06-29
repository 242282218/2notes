use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::tags::Tag;

#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EntryType {
    Unclear,
    Idea,
    Task,
    Material,
    Question,
}

impl EntryType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Unclear => "unclear",
            Self::Idea => "idea",
            Self::Task => "task",
            Self::Material => "material",
            Self::Question => "question",
        }
    }

    pub fn from_db(value: &str) -> Option<Self> {
        match value {
            "unclear" => Some(Self::Unclear),
            "idea" => Some(Self::Idea),
            "task" => Some(Self::Task),
            "material" => Some(Self::Material),
            "question" => Some(Self::Question),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EntryStatus {
    Pending,
    Done,
    Archived,
}

impl EntryStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Done => "done",
            Self::Archived => "archived",
        }
    }

    pub fn from_db(value: &str) -> Option<Self> {
        match value {
            "pending" => Some(Self::Pending),
            "done" => Some(Self::Done),
            "archived" => Some(Self::Archived),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TitleSource {
    Auto,
    User,
}

impl TitleSource {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::User => "user",
        }
    }

    pub fn from_db(value: &str) -> Option<Self> {
        match value {
            "auto" => Some(Self::Auto),
            "user" => Some(Self::User),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct EntryListFilter {
    pub query: Option<String>,
    pub entry_type: Option<EntryType>,
    pub status: Option<EntryStatus>,
    pub tag: Option<String>,
    pub include_deleted: bool,
    pub trash_only: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct PageRequest {
    pub limit: Option<u32>,
    pub offset: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct EntryPage {
    pub items: Vec<EntryListItem>,
    pub limit: u32,
    pub offset: u32,
    pub has_more: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct EntryListItem {
    pub id: String,
    pub title: Option<String>,
    pub summary: String,
    pub entry_type: EntryType,
    pub status: EntryStatus,
    pub tags: Vec<Tag>,
    #[ts(type = "number")]
    pub revision: i64,
    pub created_at: String,
    pub updated_at: String,
    pub deleted_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct EntryDetail {
    pub id: String,
    pub title: Option<String>,
    pub title_source: TitleSource,
    pub original_content: String,
    pub current_content: String,
    pub entry_type: EntryType,
    pub status: EntryStatus,
    pub tags: Vec<Tag>,
    #[ts(type = "number")]
    pub revision: i64,
    pub created_at: String,
    pub updated_at: String,
    pub deleted_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct EntryPatch {
    pub title: Option<String>,
    pub current_content: Option<String>,
    pub entry_type: Option<EntryType>,
    pub status: Option<EntryStatus>,
    pub tags: Option<Vec<String>>,
}
