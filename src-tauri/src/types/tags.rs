use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Tag {
    pub id: String,
    pub name: String,
    pub normalized_name: String,
    pub created_at: String,
    #[ts(type = "number")]
    pub entry_count: i64,
}
