// Domain contract for M1+; consumers land in later tasks.
#![allow(dead_code)]

use serde::{Deserialize, Serialize};
use ts_rs::TS;

pub const DOCUMENT_SCHEMA_VERSION: u32 = 1;
pub const MAX_BLOCKS_PER_DOCUMENT: usize = 10_000;
pub const MAX_BLOCK_DEPTH: u32 = 32;
pub const MAX_DOCUMENT_JSON_BYTES: usize = 10 * 1024 * 1024;
pub const MAX_TEXT_NODE_BYTES: usize = 1024 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct BlockDocument {
    pub schema_version: u32,
    pub blocks: Vec<BlockNode>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct BlockNode {
    pub id: String,
    pub kind: BlockKind,
    pub attrs: BlockAttrs,
    pub content: Vec<InlineNode>,
    pub children: Vec<BlockNode>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum BlockKind {
    Paragraph,
    Heading,
    BulletList,
    OrderedList,
    ListItem,
    Blockquote,
    CodeBlock,
    HorizontalRule,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct BlockAttrs {
    pub level: Option<u8>,
    pub language: Option<String>,
    pub start: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum InlineNode {
    Text {
        text: String,
        marks: Vec<InlineMark>,
    },
    HardBreak,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum InlineMark {
    Bold,
    Italic,
    Strike,
    Code,
    Link { href: String },
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct OutlineItem {
    pub id: String,
    pub level: u8,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct BlockProjection {
    pub id: String,
    pub parent_block_id: Option<String>,
    pub ordinal: u32,
    pub depth: u32,
    pub kind: BlockKind,
    pub text_content: String,
    pub attrs: BlockAttrs,
}

impl BlockNode {
    pub fn empty_paragraph(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            kind: BlockKind::Paragraph,
            attrs: BlockAttrs::default(),
            content: Vec::new(),
            children: Vec::new(),
        }
    }

    pub fn paragraph(id: impl Into<String>, text: impl Into<String>) -> Self {
        let text = text.into();
        let content = if text.is_empty() {
            Vec::new()
        } else {
            vec![InlineNode::Text {
                text,
                marks: Vec::new(),
            }]
        };
        Self {
            id: id.into(),
            kind: BlockKind::Paragraph,
            attrs: BlockAttrs::default(),
            content,
            children: Vec::new(),
        }
    }
}

impl BlockDocument {
    pub fn from_blocks(blocks: Vec<BlockNode>) -> Self {
        Self {
            schema_version: DOCUMENT_SCHEMA_VERSION,
            blocks,
        }
    }
}
