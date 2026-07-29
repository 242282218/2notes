use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct EntryTreeNode {
    pub id: String,
    pub title: String,
    pub children: Vec<EntryTreeNode>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct EntryBreadcrumb {
    pub id: String,
    pub title: String,
}
