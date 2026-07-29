use comrak::{
    nodes::{AstNode, ListType, NodeValue},
    parse_document, Arena, Options,
};
use serde::Deserialize;
use uuid::Uuid;

use crate::{
    content::document::document_to_plain_text,
    db::repos::entries_repo::auto_title,
    error::{AppError, AppResult},
    types::{
        documents::{BlockAttrs, BlockDocument, BlockKind, BlockNode, InlineMark, InlineNode},
        entries::{EntryStatus, EntryType, TitleSource},
    },
};

pub const MAX_IMPORT_BYTES: usize = 2 * 1024 * 1024;

#[derive(Debug, Clone)]
pub struct MarkdownImport {
    pub document: BlockDocument,
    pub title: Option<String>,
    pub title_source: TitleSource,
    pub original_content: String,
    pub entry_type: EntryType,
    pub status: EntryStatus,
    pub tags: Vec<String>,
    pub source_entry_id: Option<String>,
    pub warnings: Vec<String>,
}

pub fn parse_import_bytes(bytes: &[u8]) -> AppResult<MarkdownImport> {
    if bytes.len() > MAX_IMPORT_BYTES {
        return Err(AppError::validation(
            "IMPORT_FILE_TOO_LARGE",
            format!("import exceeds {MAX_IMPORT_BYTES} bytes"),
        ));
    }
    let original_content = std::str::from_utf8(bytes)
        .map_err(|_| AppError::validation("IMPORT_INVALID_UTF8", "import must be valid UTF-8"))?;
    let normalized = original_content
        .strip_prefix('\u{feff}')
        .unwrap_or(original_content)
        .replace("\r\n", "\n")
        .replace('\r', "\n");
    let (frontmatter, body) = extract_frontmatter(&normalized);
    let (body, provenance) = split_provenance(body);
    let metadata = frontmatter.map(parse_frontmatter).unwrap_or_default();
    let arena = Arena::new();
    let mut options = Options::default();
    options.extension.strikethrough = true;
    let root = parse_document(&arena, body, &options);
    let mut warnings = Vec::new();
    let mut blocks = root
        .children()
        .filter_map(|node| block_from_ast(node, &mut warnings))
        .collect::<Vec<_>>();

    let title = blocks
        .first()
        .filter(|block| block.kind == BlockKind::Heading && block.attrs.level == Some(1))
        .map(|block| inline_text(&block.content));
    if title.is_some() {
        blocks.remove(0);
    }
    if blocks.is_empty() {
        blocks.push(BlockNode::empty_paragraph(new_id()));
    }

    let document = BlockDocument::from_blocks(blocks);
    let title_source = if title.is_some() {
        TitleSource::User
    } else {
        TitleSource::Auto
    };
    let title = title.or_else(|| auto_title(&document_to_plain_text(&document)));
    Ok(MarkdownImport {
        document,
        title,
        title_source,
        original_content: provenance.unwrap_or_else(|| original_content.to_string()),
        entry_type: metadata.entry_type.unwrap_or(EntryType::Unclear),
        status: metadata.status.unwrap_or(EntryStatus::Pending),
        tags: metadata.tags,
        source_entry_id: metadata.id,
        warnings,
    })
}

#[derive(Debug, Default, Deserialize)]
struct ImportFrontmatter {
    id: Option<String>,
    #[serde(rename = "type")]
    entry_type: Option<String>,
    status: Option<String>,
    #[serde(default)]
    tags: Vec<String>,
}

#[derive(Default)]
struct TrustedMetadata {
    id: Option<String>,
    entry_type: Option<EntryType>,
    status: Option<EntryStatus>,
    tags: Vec<String>,
}

fn extract_frontmatter(input: &str) -> (Option<&str>, &str) {
    let Some(rest) = input.strip_prefix("---\n") else {
        return (None, input);
    };
    let Some(end) = rest.find("\n---\n") else {
        return (None, input);
    };
    (Some(&rest[..end]), &rest[end + "\n---\n".len()..])
}

fn split_provenance(body: &str) -> (&str, Option<String>) {
    const DIVIDER: &str = "---\n\n## 原始内容\n";
    let Some(index) = body.rfind(DIVIDER) else {
        return (body, None);
    };
    let provenance = body[index + DIVIDER.len()..]
        .strip_prefix('\n')
        .unwrap_or(&body[index + DIVIDER.len()..])
        .to_string();
    (&body[..index], Some(provenance))
}

fn parse_frontmatter(input: &str) -> TrustedMetadata {
    let parsed = serde_json::from_str::<ImportFrontmatter>(input)
        .or_else(|_| serde_yaml_ng::from_str::<ImportFrontmatter>(input));
    let Ok(parsed) = parsed else {
        return TrustedMetadata::default();
    };
    TrustedMetadata {
        id: parsed.id.filter(|id| Uuid::parse_str(id).is_ok()),
        entry_type: parsed.entry_type.as_deref().and_then(EntryType::from_db),
        status: parsed.status.as_deref().and_then(EntryStatus::from_db),
        tags: parsed
            .tags
            .into_iter()
            .filter(|tag| is_trusted_tag(tag))
            .collect(),
    }
}

fn is_trusted_tag(tag: &str) -> bool {
    !tag.trim().is_empty() && tag.len() <= 100 && !tag.chars().any(char::is_control)
}

fn block_from_ast<'a>(node: &'a AstNode<'a>, warnings: &mut Vec<String>) -> Option<BlockNode> {
    let data = node.data.borrow();
    let (kind, attrs, children) = match &data.value {
        NodeValue::Paragraph => (BlockKind::Paragraph, BlockAttrs::default(), Vec::new()),
        NodeValue::Heading(heading) => (
            BlockKind::Heading,
            BlockAttrs {
                level: Some(heading.level),
                ..BlockAttrs::default()
            },
            Vec::new(),
        ),
        NodeValue::List(list) => (
            match list.list_type {
                ListType::Bullet => BlockKind::BulletList,
                ListType::Ordered => BlockKind::OrderedList,
            },
            BlockAttrs {
                start: (list.list_type == ListType::Ordered).then_some(list.start as u32),
                ..BlockAttrs::default()
            },
            child_blocks(node, warnings),
        ),
        NodeValue::Item(_) => (
            BlockKind::ListItem,
            BlockAttrs::default(),
            child_blocks(node, warnings),
        ),
        NodeValue::BlockQuote => (
            BlockKind::Blockquote,
            BlockAttrs::default(),
            child_blocks(node, warnings),
        ),
        NodeValue::CodeBlock(code) => (
            BlockKind::CodeBlock,
            BlockAttrs {
                language: (!code.info.is_empty()).then(|| code.info.to_string()),
                ..BlockAttrs::default()
            },
            Vec::new(),
        ),
        NodeValue::ThematicBreak => (BlockKind::HorizontalRule, BlockAttrs::default(), Vec::new()),
        NodeValue::HtmlBlock(_) => {
            warnings.push("Unsupported HTML block was imported as plain text".to_string());
            (BlockKind::Paragraph, BlockAttrs::default(), Vec::new())
        }
        _ => {
            warnings.push("Unsupported Markdown block was skipped".to_string());
            return None;
        }
    };
    let content = match &data.value {
        NodeValue::CodeBlock(code) => text_node(&code.literal, Vec::new()),
        NodeValue::HtmlBlock(html) => text_node(&html.literal, Vec::new()),
        NodeValue::ThematicBreak | NodeValue::List(_) | NodeValue::BlockQuote => Vec::new(),
        _ => inline_nodes(node, warnings),
    };
    drop(data);
    Some(BlockNode {
        id: new_id(),
        kind,
        attrs,
        content,
        children,
    })
}

fn child_blocks<'a>(node: &'a AstNode<'a>, warnings: &mut Vec<String>) -> Vec<BlockNode> {
    node.children()
        .filter_map(|child| block_from_ast(child, warnings))
        .collect()
}

fn inline_nodes<'a>(node: &'a AstNode<'a>, warnings: &mut Vec<String>) -> Vec<InlineNode> {
    let mut result = Vec::new();
    collect_inlines(node, &mut Vec::new(), &mut result, warnings);
    result
}

fn collect_inlines<'a>(
    node: &'a AstNode<'a>,
    marks: &mut Vec<InlineMark>,
    output: &mut Vec<InlineNode>,
    warnings: &mut Vec<String>,
) {
    for child in node.children() {
        let data = child.data.borrow();
        match &data.value {
            NodeValue::Text(text) => output.extend(text_node(text, marks.clone())),
            NodeValue::SoftBreak | NodeValue::LineBreak => output.push(InlineNode::HardBreak),
            NodeValue::Code(code) => {
                output.extend(text_node(&code.literal, vec![InlineMark::Code]))
            }
            NodeValue::Strong => with_mark(child, marks, InlineMark::Bold, output, warnings),
            NodeValue::Emph => with_mark(child, marks, InlineMark::Italic, output, warnings),
            NodeValue::Strikethrough => {
                with_mark(child, marks, InlineMark::Strike, output, warnings)
            }
            NodeValue::Link(link) if is_safe_href(&link.url) => with_mark(
                child,
                marks,
                InlineMark::Link {
                    href: link.url.to_string(),
                },
                output,
                warnings,
            ),
            NodeValue::Link(link) => {
                warnings.push(format!(
                    "Unsafe link scheme was imported as plain text: {}",
                    link.url
                ));
                collect_inlines(child, marks, output, warnings);
            }
            NodeValue::HtmlInline(html) => {
                warnings
                    .push("Unsupported HTML inline content was imported as plain text".to_string());
                output.extend(text_node(html, marks.clone()));
            }
            _ => collect_inlines(child, marks, output, warnings),
        }
        drop(data);
    }
}

fn with_mark<'a>(
    node: &'a AstNode<'a>,
    marks: &mut Vec<InlineMark>,
    mark: InlineMark,
    output: &mut Vec<InlineNode>,
    warnings: &mut Vec<String>,
) {
    marks.push(mark);
    collect_inlines(node, marks, output, warnings);
    marks.pop();
}

fn text_node(text: &str, marks: Vec<InlineMark>) -> Vec<InlineNode> {
    (!text.is_empty())
        .then(|| InlineNode::Text {
            text: text.to_string(),
            marks,
        })
        .into_iter()
        .collect()
}

fn is_safe_href(href: &str) -> bool {
    let href = href.to_ascii_lowercase();
    href.starts_with("http://") || href.starts_with("https://") || href.starts_with("mailto:")
}

fn inline_text(nodes: &[InlineNode]) -> String {
    nodes
        .iter()
        .map(|node| match node {
            InlineNode::Text { text, .. } => text.as_str(),
            InlineNode::HardBreak => "\n",
        })
        .collect()
}

fn new_id() -> String {
    let mut bytes = *blake3::hash(Uuid::new_v4().as_bytes()).as_bytes();
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    Uuid::from_bytes(bytes[..16].try_into().expect("digest prefix has 16 bytes")).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        content::document::validate_document,
        error::AppError,
        types::{
            documents::{BlockKind, InlineMark, InlineNode},
            entries::{EntryStatus, EntryType, TitleSource},
        },
    };

    fn text(nodes: &[InlineNode]) -> String {
        nodes
            .iter()
            .map(|node| match node {
                InlineNode::Text { text, .. } => text.as_str(),
                InlineNode::HardBreak => "\n",
            })
            .collect()
    }

    #[test]
    fn markdown_import_parses_legacy_json_export_user_title_and_provenance() {
        let markdown = "---\n{\n  \"id\": \"f67fe992-4e45-4c50-95c1-c7a6f264101e\",\n  \"type\": \"task\",\n  \"status\": \"done\",\n  \"tags\": [\"work\", \"rust\"]\n}\n---\n\n# User title\n\nbody\n\n---\n\n## 原始内容\n\nraw capture\n";
        let imported = parse_import_bytes(markdown.as_bytes()).unwrap();
        assert_eq!(imported.title.as_deref(), Some("User title"));
        assert_eq!(imported.title_source, TitleSource::User);
        assert_eq!(imported.original_content, "raw capture\n");
        assert_eq!(imported.entry_type, EntryType::Task);
        assert_eq!(imported.status, EntryStatus::Done);
        assert_eq!(imported.tags, ["work", "rust"]);
        assert_eq!(
            imported.source_entry_id.as_deref(),
            Some("f67fe992-4e45-4c50-95c1-c7a6f264101e")
        );
        assert_eq!(imported.document.blocks.len(), 1);
        assert_eq!(imported.document.blocks[0].kind, BlockKind::Paragraph);
        assert_eq!(text(&imported.document.blocks[0].content), "body");
        validate_document(&imported.document).unwrap();
    }

    #[test]
    fn markdown_import_parses_yaml_frontmatter() {
        let markdown = "---\nid: 550e8400-e29b-41d4-a716-446655440000\ntype: material\nstatus: archived\ntags:\n  - docs\n---\n\n# Heading\n\ntext";
        let imported = parse_import_bytes(markdown.as_bytes()).unwrap();
        assert_eq!(imported.entry_type, EntryType::Material);
        assert_eq!(imported.status, EntryStatus::Archived);
        assert_eq!(imported.tags, ["docs"]);
        assert_eq!(imported.title.as_deref(), Some("Heading"));
    }

    #[test]
    fn markdown_import_normalizes_bom_and_crlf() {
        let imported =
            parse_import_bytes(b"\xef\xbb\xbf# Title\r\n\r\nline one  \r\nline two\r\n").unwrap();
        assert_eq!(imported.title.as_deref(), Some("Title"));
        assert_eq!(
            imported.original_content,
            "\u{feff}# Title\r\n\r\nline one  \r\nline two\r\n"
        );
        assert!(matches!(
            imported.document.blocks[0].content[1],
            InlineNode::HardBreak
        ));
    }

    #[test]
    fn markdown_import_maps_commonmark_shapes_and_safe_marks() {
        let markdown = "## H2\n\n**bold** *italic* ~~strike~~ `code` [web](https://example.com) [mail](mailto:a@example.com)\x20\x20\nnext\n\n- one\n- two\n\n3. three\n\n> quoted\n\n```rs\nlet x = 1;\n```\n\n---";
        let imported = parse_import_bytes(markdown.as_bytes()).unwrap();
        let blocks = &imported.document.blocks;
        assert!(blocks.iter().any(|block| block.kind == BlockKind::Heading));
        assert!(blocks
            .iter()
            .any(|block| block.kind == BlockKind::BulletList));
        assert!(blocks
            .iter()
            .any(|block| block.kind == BlockKind::OrderedList));
        assert!(blocks
            .iter()
            .any(|block| block.kind == BlockKind::Blockquote));
        assert!(blocks
            .iter()
            .any(|block| block.kind == BlockKind::CodeBlock));
        assert!(blocks
            .iter()
            .any(|block| block.kind == BlockKind::HorizontalRule));
        let paragraph = blocks
            .iter()
            .find(|block| block.kind == BlockKind::Paragraph)
            .unwrap();
        assert!(paragraph.content.iter().any(|node| matches!(node, InlineNode::Text { marks, .. } if marks.contains(&InlineMark::Bold))));
        assert!(paragraph.content.iter().any(|node| matches!(node, InlineNode::Text { marks, .. } if marks.iter().any(|mark| matches!(mark, InlineMark::Link { href } if href.starts_with("https://") || href.starts_with("mailto:"))))));
        assert!(paragraph
            .content
            .iter()
            .any(|node| matches!(node, InlineNode::HardBreak)));
        validate_document(&imported.document).unwrap();
    }

    #[test]
    fn markdown_import_uses_empty_paragraph_for_empty_input() {
        let imported = parse_import_bytes(b"").unwrap();
        assert_eq!(imported.document.blocks.len(), 1);
        assert_eq!(imported.document.blocks[0].kind, BlockKind::Paragraph);
        assert!(imported.document.blocks[0].content.is_empty());
        validate_document(&imported.document).unwrap();
    }

    #[test]
    fn markdown_import_rejects_invalid_utf8() {
        let err = parse_import_bytes(&[0xff]).unwrap_err();
        assert!(matches!(err, AppError::Validation { code, .. } if code == "IMPORT_INVALID_UTF8"));
    }

    #[test]
    fn markdown_import_rejects_files_over_two_mebibytes() {
        let err = parse_import_bytes(&vec![b'x'; MAX_IMPORT_BYTES + 1]).unwrap_err();
        assert!(
            matches!(err, AppError::Validation { code, .. } if code == "IMPORT_FILE_TOO_LARGE")
        );
    }
}
