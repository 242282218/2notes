// Pure document helpers; command/repo wiring lands in later M1 tasks.
#![allow(dead_code)]

use std::collections::HashSet;

use uuid::Uuid;

use crate::error::{AppError, AppResult};
use crate::types::documents::{
    BlockAttrs, BlockDocument, BlockKind, BlockNode, BlockProjection, InlineMark, InlineNode,
    OutlineItem, DOCUMENT_SCHEMA_VERSION, MAX_BLOCKS_PER_DOCUMENT, MAX_BLOCK_DEPTH,
    MAX_DOCUMENT_JSON_BYTES, MAX_TEXT_NODE_BYTES,
};

pub fn validate_document(document: &BlockDocument) -> AppResult<()> {
    if document.schema_version != DOCUMENT_SCHEMA_VERSION {
        return Err(AppError::validation(
            "DOCUMENT_SCHEMA_INVALID",
            format!(
                "unsupported document schema version {}",
                document.schema_version
            ),
        ));
    }

    if document.blocks.is_empty() {
        return Err(AppError::validation(
            "DOCUMENT_EMPTY",
            "document must contain at least one block",
        ));
    }

    let mut seen_ids = HashSet::new();
    let mut block_count = 0usize;
    walk_validate(&document.blocks, None, 0, &mut seen_ids, &mut block_count)?;

    if block_count > MAX_BLOCKS_PER_DOCUMENT {
        return Err(AppError::validation(
            "BLOCK_COUNT_EXCEEDED",
            format!("document exceeds {MAX_BLOCKS_PER_DOCUMENT} blocks"),
        ));
    }

    let json_len = serde_json::to_vec(document)
        .map_err(|err| AppError::validation("DOCUMENT_JSON_INVALID", err.to_string()))?
        .len();
    if json_len > MAX_DOCUMENT_JSON_BYTES {
        return Err(AppError::validation(
            "DOCUMENT_JSON_TOO_LARGE",
            format!("document JSON exceeds {MAX_DOCUMENT_JSON_BYTES} bytes"),
        ));
    }

    Ok(())
}

fn walk_validate(
    blocks: &[BlockNode],
    parent_kind: Option<BlockKind>,
    depth: u32,
    seen_ids: &mut HashSet<String>,
    block_count: &mut usize,
) -> AppResult<()> {
    if depth > MAX_BLOCK_DEPTH {
        return Err(AppError::validation(
            "BLOCK_DEPTH_EXCEEDED",
            format!("block depth exceeds {MAX_BLOCK_DEPTH}"),
        ));
    }

    for block in blocks {
        *block_count += 1;
        if *block_count > MAX_BLOCKS_PER_DOCUMENT {
            return Err(AppError::validation(
                "BLOCK_COUNT_EXCEEDED",
                format!("document exceeds {MAX_BLOCKS_PER_DOCUMENT} blocks"),
            ));
        }

        if !is_uuid_v4(&block.id) {
            return Err(AppError::validation(
                "BLOCK_ID_INVALID",
                format!("block id must be a UUID v4: {}", block.id),
            ));
        }
        if !seen_ids.insert(block.id.clone()) {
            return Err(AppError::validation(
                "BLOCK_ID_DUPLICATE",
                format!("duplicate block id: {}", block.id),
            ));
        }

        if let Some(parent) = parent_kind {
            if matches!(parent, BlockKind::BulletList | BlockKind::OrderedList)
                && block.kind != BlockKind::ListItem
            {
                return Err(AppError::validation(
                    "LIST_CHILD_INVALID",
                    "list blocks may only contain list items",
                ));
            }
        }

        validate_block_shape(block)?;
        validate_inlines(&block.content)?;
        walk_validate(
            &block.children,
            Some(block.kind),
            depth + 1,
            seen_ids,
            block_count,
        )?;
    }

    Ok(())
}

fn is_uuid_v4(id: &str) -> bool {
    // Only accept canonical hyphenated lowercase form so stable IDs compare as strings.
    match Uuid::parse_str(id) {
        Ok(value) if value.get_version() == Some(uuid::Version::Random) => {
            id == value.hyphenated().to_string()
        }
        _ => false,
    }
}

fn validate_block_shape(block: &BlockNode) -> AppResult<()> {
    match block.kind {
        BlockKind::Heading => {
            let level = block.attrs.level.ok_or_else(|| {
                AppError::validation("HEADING_LEVEL_INVALID", "heading requires level 1-6")
            })?;
            if !(1..=6).contains(&level) {
                return Err(AppError::validation(
                    "HEADING_LEVEL_INVALID",
                    format!("heading level {level} is outside 1-6"),
                ));
            }
            if !block.children.is_empty() {
                return Err(AppError::validation(
                    "HEADING_CHILDREN_INVALID",
                    "heading must not contain child blocks",
                ));
            }
        }
        BlockKind::Paragraph | BlockKind::CodeBlock | BlockKind::HorizontalRule => {
            if !block.children.is_empty() {
                return Err(AppError::validation(
                    "BLOCK_CHILDREN_INVALID",
                    format!("{:?} must not contain child blocks", block.kind),
                ));
            }
            if block.kind == BlockKind::HorizontalRule && !block.content.is_empty() {
                return Err(AppError::validation(
                    "HORIZONTAL_RULE_CONTENT_INVALID",
                    "horizontal rule must not contain inline content",
                ));
            }
        }
        BlockKind::BulletList | BlockKind::OrderedList => {
            if !block.content.is_empty() {
                return Err(AppError::validation(
                    "LIST_CONTENT_INVALID",
                    "list blocks must not contain inline content",
                ));
            }
        }
        BlockKind::Blockquote => {
            // Tiptap-style tree: blockquote owns child blocks only.
            if !block.content.is_empty() {
                return Err(AppError::validation(
                    "BLOCKQUOTE_CONTENT_INVALID",
                    "blockquote must not contain inline content; use child paragraphs",
                ));
            }
        }
        BlockKind::ListItem => {}
    }
    Ok(())
}

fn validate_inlines(content: &[InlineNode]) -> AppResult<()> {
    for node in content {
        match node {
            InlineNode::HardBreak => {}
            InlineNode::Text { text, marks } => {
                if text.len() > MAX_TEXT_NODE_BYTES {
                    return Err(AppError::validation(
                        "TEXT_NODE_TOO_LARGE",
                        format!("text node exceeds {MAX_TEXT_NODE_BYTES} bytes"),
                    ));
                }
                for mark in marks {
                    if let InlineMark::Link { href } = mark {
                        if !is_safe_link_href(href) {
                            return Err(AppError::validation(
                                "LINK_SCHEME_INVALID",
                                format!("unsupported link scheme: {href}"),
                            ));
                        }
                    }
                }
            }
        }
    }
    Ok(())
}

fn is_safe_link_href(href: &str) -> bool {
    let trimmed = href.trim();
    if trimmed.is_empty() {
        return false;
    }
    let lower = trimmed.to_ascii_lowercase();
    lower.starts_with("http://") || lower.starts_with("https://") || lower.starts_with("mailto:")
}

pub fn document_to_markdown(document: &BlockDocument) -> AppResult<String> {
    validate_document(document)?;
    let mut out = String::new();
    render_blocks_markdown(&document.blocks, 0, &mut out);
    if out.ends_with('\n') {
        out.pop();
        if out.ends_with('\r') {
            out.pop();
        }
    }
    Ok(out)
}

fn render_blocks_markdown(blocks: &[BlockNode], list_depth: usize, out: &mut String) {
    for (index, block) in blocks.iter().enumerate() {
        if index > 0 && list_depth == 0 {
            ensure_blank_line(out);
        }
        render_block_markdown(block, list_depth, out);
    }
}

fn render_block_markdown(block: &BlockNode, list_depth: usize, out: &mut String) {
    match block.kind {
        BlockKind::Paragraph => {
            render_inlines_markdown(&block.content, out, false);
            out.push('\n');
        }
        BlockKind::Heading => {
            let level = block.attrs.level.unwrap_or(1).clamp(1, 6);
            for _ in 0..level {
                out.push('#');
            }
            out.push(' ');
            render_inlines_markdown(&block.content, out, false);
            out.push('\n');
        }
        BlockKind::BulletList => {
            render_list_markdown(block, list_depth, false, out);
        }
        BlockKind::OrderedList => {
            render_list_markdown(block, list_depth, true, out);
        }
        BlockKind::ListItem => {
            // List items are rendered by their parent list.
            render_blocks_markdown(&block.children, list_depth, out);
        }
        BlockKind::Blockquote => {
            let mut inner = String::new();
            render_blocks_markdown(&block.children, 0, &mut inner);
            if inner.is_empty() {
                out.push_str(">\n");
            } else {
                for line in inner.lines() {
                    out.push('>');
                    if !line.is_empty() {
                        out.push(' ');
                        out.push_str(line);
                    }
                    out.push('\n');
                }
            }
        }
        BlockKind::CodeBlock => {
            let code = inline_plain_text(&block.content);
            let fence = code_fence_marker(&code);
            out.push_str(&fence);
            if let Some(language) = block.attrs.language.as_deref() {
                out.push_str(language);
            }
            out.push('\n');
            out.push_str(&code);
            if !code.ends_with('\n') {
                out.push('\n');
            }
            out.push_str(&fence);
            out.push('\n');
        }
        BlockKind::HorizontalRule => {
            out.push_str("---\n");
        }
    }
}

fn render_list_markdown(list: &BlockNode, list_depth: usize, ordered: bool, out: &mut String) {
    let start = list.attrs.start.unwrap_or(1);
    for (index, item) in list.children.iter().enumerate() {
        let indent = "  ".repeat(list_depth);
        out.push_str(&indent);
        if ordered {
            out.push_str(&(start + index as u32).to_string());
            out.push_str(". ");
        } else {
            out.push_str("- ");
        }

        let mut item_body = String::new();
        if !item.content.is_empty() {
            render_inlines_markdown(&item.content, &mut item_body, false);
        }
        if !item.children.is_empty() {
            if !item_body.is_empty() {
                item_body.push('\n');
            }
            render_blocks_markdown(&item.children, list_depth + 1, &mut item_body);
        }
        if item_body.is_empty() {
            out.push('\n');
            continue;
        }

        let mut lines = item_body.lines();
        if let Some(first) = lines.next() {
            out.push_str(first);
            out.push('\n');
        }
        for line in lines {
            if line.is_empty() {
                out.push('\n');
            } else {
                out.push_str(&indent);
                out.push_str("  ");
                out.push_str(line);
                out.push('\n');
            }
        }
    }
}

fn render_inlines_markdown(content: &[InlineNode], out: &mut String, in_code: bool) {
    for node in content {
        match node {
            // CommonMark hard break: backslash + newline.
            InlineNode::HardBreak => out.push_str("\\\n"),
            InlineNode::Text { text, marks } => {
                if in_code {
                    out.push_str(text);
                    continue;
                }
                if marks.iter().any(|m| matches!(m, InlineMark::Code)) {
                    out.push_str(&wrap_inline_code(text));
                    continue;
                }

                let mut rendered = escape_markdown_text(text);
                for mark in marks {
                    match mark {
                        InlineMark::Bold => rendered = format!("**{rendered}**"),
                        InlineMark::Italic => rendered = format!("*{rendered}*"),
                        InlineMark::Strike => rendered = format!("~~{rendered}~~"),
                        InlineMark::Code => rendered = wrap_inline_code(text),
                        InlineMark::Link { href } => {
                            let safe_href = href.replace('(', "%28").replace(')', "%29");
                            rendered = format!("[{rendered}]({safe_href})");
                        }
                    }
                }
                out.push_str(&rendered);
            }
        }
    }
}

fn longest_backtick_run(input: &str) -> usize {
    let mut longest = 0usize;
    let mut current = 0usize;
    for ch in input.chars() {
        if ch == '`' {
            current += 1;
            longest = longest.max(current);
        } else {
            current = 0;
        }
    }
    longest
}

fn code_fence_marker(code: &str) -> String {
    let ticks = longest_backtick_run(code).max(2) + 1;
    "`".repeat(ticks)
}

fn wrap_inline_code(text: &str) -> String {
    let ticks = longest_backtick_run(text) + 1;
    let marker = "`".repeat(ticks.max(1));
    // CommonMark: pad with spaces when content starts/ends with a backtick.
    if text.starts_with('`') || text.ends_with('`') {
        format!("{marker} {text} {marker}")
    } else {
        format!("{marker}{text}{marker}")
    }
}

fn escape_markdown_text(input: &str) -> String {
    // Keep WikiLinks intact as plain [[title]] while escaping other MD markers.
    let mut out = String::with_capacity(input.len());
    let chars: Vec<char> = input.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '[' && i + 1 < chars.len() && chars[i + 1] == '[' {
            if let Some(close) = find_wikilink_close(&chars, i + 2) {
                for ch in &chars[i..=close] {
                    out.push(*ch);
                }
                i = close + 1;
                continue;
            }
        }

        match chars[i] {
            '\\' | '`' | '*' | '_' | '{' | '}' | '[' | ']' | '(' | ')' | '#' | '+' | '-' | '.'
            | '!' | '|' | '<' | '>' => {
                out.push('\\');
                out.push(chars[i]);
            }
            other => out.push(other),
        }
        i += 1;
    }
    out
}

fn find_wikilink_close(chars: &[char], start: usize) -> Option<usize> {
    let mut i = start;
    while i + 1 < chars.len() {
        if chars[i] == ']' && chars[i + 1] == ']' {
            return Some(i + 1);
        }
        if chars[i] == '[' && chars[i + 1] == '[' {
            return None;
        }
        i += 1;
    }
    None
}

fn ensure_blank_line(out: &mut String) {
    if out.is_empty() {
        return;
    }
    if !out.ends_with('\n') {
        out.push('\n');
    }
    if !out.ends_with("\n\n") {
        out.push('\n');
    }
}

pub fn document_to_plain_text(document: &BlockDocument) -> String {
    let mut parts = Vec::new();
    collect_plain_blocks(&document.blocks, &mut parts);
    parts.join("\n\n")
}

fn collect_plain_blocks(blocks: &[BlockNode], parts: &mut Vec<String>) {
    for block in blocks {
        match block.kind {
            BlockKind::Paragraph | BlockKind::Heading | BlockKind::CodeBlock => {
                let text = inline_plain_text(&block.content);
                if !text.is_empty()
                    || matches!(block.kind, BlockKind::Paragraph | BlockKind::Heading)
                {
                    parts.push(text);
                }
            }
            BlockKind::HorizontalRule => {}
            BlockKind::BulletList | BlockKind::OrderedList | BlockKind::Blockquote => {
                collect_plain_blocks(&block.children, parts);
            }
            BlockKind::ListItem => {
                let mut item_parts = Vec::new();
                let text = inline_plain_text(&block.content);
                if !text.is_empty() {
                    item_parts.push(text);
                }
                collect_plain_blocks(&block.children, &mut item_parts);
                if !item_parts.is_empty() {
                    parts.push(item_parts.join("\n"));
                }
            }
        }
    }
}

fn inline_plain_text(content: &[InlineNode]) -> String {
    let mut out = String::new();
    for node in content {
        match node {
            InlineNode::HardBreak => out.push('\n'),
            InlineNode::Text { text, .. } => out.push_str(text),
        }
    }
    out
}

pub fn document_outline(document: &BlockDocument) -> Vec<OutlineItem> {
    let mut items = Vec::new();
    collect_outline(&document.blocks, &mut items);
    items
}

fn collect_outline(blocks: &[BlockNode], items: &mut Vec<OutlineItem>) {
    for block in blocks {
        if block.kind == BlockKind::Heading {
            let level = block.attrs.level.unwrap_or(1).clamp(1, 6);
            items.push(OutlineItem {
                id: block.id.clone(),
                level,
                text: inline_plain_text(&block.content),
            });
        }
        collect_outline(&block.children, items);
    }
}

pub fn legacy_text_document(content: &str) -> BlockDocument {
    let normalized = content.replace("\r\n", "\n").replace('\r', "\n");
    let paragraphs = split_legacy_paragraphs(&normalized);
    let blocks = if paragraphs.is_empty() {
        vec![BlockNode::empty_paragraph(Uuid::new_v4().to_string())]
    } else {
        paragraphs
            .into_iter()
            .map(|paragraph| {
                let content = legacy_paragraph_inlines(paragraph);
                BlockNode {
                    id: Uuid::new_v4().to_string(),
                    kind: BlockKind::Paragraph,
                    attrs: BlockAttrs::default(),
                    content,
                    children: Vec::new(),
                }
            })
            .collect()
    };

    BlockDocument {
        schema_version: DOCUMENT_SCHEMA_VERSION,
        blocks,
    }
}

fn split_legacy_paragraphs(content: &str) -> Vec<&str> {
    if content.is_empty() {
        return Vec::new();
    }

    let mut paragraphs = Vec::new();
    let mut start = 0usize;
    let bytes = content.as_bytes();
    let mut i = 0usize;
    while i < bytes.len() {
        if bytes[i] == b'\n' {
            let mut j = i;
            let mut newline_count = 0usize;
            while j < bytes.len() && bytes[j] == b'\n' {
                newline_count += 1;
                j += 1;
            }
            if newline_count >= 2 {
                paragraphs.push(&content[start..i]);
                start = j;
                i = j;
                continue;
            }
        }
        i += 1;
    }
    paragraphs.push(&content[start..]);
    paragraphs
}

fn legacy_paragraph_inlines(paragraph: &str) -> Vec<InlineNode> {
    if paragraph.is_empty() {
        return Vec::new();
    }
    let mut content = Vec::new();
    let lines: Vec<&str> = paragraph.split('\n').collect();
    for (index, line) in lines.iter().enumerate() {
        if index > 0 {
            content.push(InlineNode::HardBreak);
        }
        if !line.is_empty() {
            content.push(InlineNode::Text {
                text: (*line).to_string(),
                marks: Vec::new(),
            });
        }
    }
    content
}

pub fn flatten_blocks(document: &BlockDocument) -> Vec<BlockProjection> {
    let mut out = Vec::new();
    flatten_walk(&document.blocks, None, 0, &mut out);
    out
}

fn flatten_walk(
    blocks: &[BlockNode],
    parent_block_id: Option<&str>,
    depth: u32,
    out: &mut Vec<BlockProjection>,
) {
    for (ordinal, block) in blocks.iter().enumerate() {
        out.push(BlockProjection {
            id: block.id.clone(),
            parent_block_id: parent_block_id.map(str::to_string),
            ordinal: ordinal as u32,
            depth,
            kind: block.kind,
            text_content: inline_plain_text(&block.content),
            attrs: block.attrs.clone(),
        });
        flatten_walk(&block.children, Some(block.id.as_str()), depth + 1, out);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::AppError;

    fn id(label: &str) -> String {
        // Stable UUID v4 IDs for tests: version nibble fixed to 4, RFC variant fixed to 8.
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        let mut hasher = DefaultHasher::new();
        label.hash(&mut hasher);
        let n = hasher.finish();
        let a = (n >> 32) as u32;
        let b = ((n >> 16) & 0xffff) as u16;
        let c = (0x4000u16) | (((n >> 4) as u16) & 0x0fff);
        let d = (0x8000u16) | ((n as u16) & 0x3fff);
        let e = n & 0x0000_ffff_ffff_ffff;
        format!("{a:08x}-{b:04x}-{c:04x}-{d:04x}-{e:012x}")
    }

    fn paragraph_block(label: &str, text: &str) -> BlockNode {
        BlockNode::paragraph(id(label), text)
    }

    fn paragraph_document(label: &str, text: &str) -> BlockDocument {
        BlockDocument::from_blocks(vec![paragraph_block(label, text)])
    }

    fn text_inline(text: &str) -> InlineNode {
        InlineNode::Text {
            text: text.to_string(),
            marks: Vec::new(),
        }
    }

    fn heading_block(label: &str, level: u8, text: &str) -> BlockNode {
        BlockNode {
            id: id(label),
            kind: BlockKind::Heading,
            attrs: BlockAttrs {
                level: Some(level),
                language: None,
                start: None,
            },
            content: vec![text_inline(text)],
            children: Vec::new(),
        }
    }

    fn nested_depth(depth: u32) -> BlockNode {
        let mut node = paragraph_block(&format!("d{depth}"), "leaf");
        let mut current_depth = depth;
        while current_depth > 0 {
            current_depth -= 1;
            node = BlockNode {
                id: id(&format!("d{current_depth}")),
                kind: BlockKind::Blockquote,
                attrs: BlockAttrs::default(),
                content: Vec::new(),
                children: vec![node],
            };
        }
        node
    }

    #[test]
    fn accepts_legal_paragraph_document() {
        let document = paragraph_document("p1", "hello");
        assert!(validate_document(&document).is_ok());
    }

    #[test]
    fn accepts_single_empty_paragraph() {
        let document = BlockDocument::from_blocks(vec![BlockNode::empty_paragraph(id("empty"))]);
        assert!(validate_document(&document).is_ok());
    }

    #[test]
    fn rejects_duplicate_block_ids() {
        let mut document = paragraph_document("same", "first");
        document.blocks.push(paragraph_block("same", "second"));
        let err = validate_document(&document).unwrap_err();
        assert!(matches!(err, AppError::Validation { code, .. } if code == "BLOCK_ID_DUPLICATE"));
    }

    #[test]
    fn rejects_non_uuid_v4_block_ids() {
        let document = BlockDocument::from_blocks(vec![paragraph_block_raw("p1", "hello")]);
        let err = validate_document(&document).unwrap_err();
        assert!(matches!(err, AppError::Validation { code, .. } if code == "BLOCK_ID_INVALID"));

        let nil = BlockDocument::from_blocks(vec![BlockNode::paragraph(
            "00000000-0000-0000-0000-000000000000",
            "x",
        )]);
        let err = validate_document(&nil).unwrap_err();
        assert!(matches!(err, AppError::Validation { code, .. } if code == "BLOCK_ID_INVALID"));

        let v1 = BlockDocument::from_blocks(vec![BlockNode::paragraph(
            "6ba7b810-9dad-11d1-80b4-00c04fd430c8",
            "x",
        )]);
        let err = validate_document(&v1).unwrap_err();
        assert!(matches!(err, AppError::Validation { code, .. } if code == "BLOCK_ID_INVALID"));

        // Same UUID v4, non-canonical spellings must not pass string-stable ID checks.
        for raw in [
            "550E8400-E29B-41D4-A716-446655440000",
            "550e8400e29b41d4a716446655440000",
            "urn:uuid:550e8400-e29b-41d4-a716-446655440000",
        ] {
            let document = BlockDocument::from_blocks(vec![BlockNode::paragraph(raw, "x")]);
            let err = validate_document(&document).unwrap_err();
            assert!(
                matches!(err, AppError::Validation { code, .. } if code == "BLOCK_ID_INVALID"),
                "raw={raw}"
            );
        }

        let canonical = BlockDocument::from_blocks(vec![BlockNode::paragraph(
            "550e8400-e29b-41d4-a716-446655440000",
            "ok",
        )]);
        assert!(validate_document(&canonical).is_ok());
    }

    fn paragraph_block_raw(id: &str, text: &str) -> BlockNode {
        BlockNode::paragraph(id, text)
    }

    #[test]
    fn rejects_non_list_item_inside_list() {
        let document = BlockDocument::from_blocks(vec![BlockNode {
            id: id("list"),
            kind: BlockKind::BulletList,
            attrs: BlockAttrs::default(),
            content: Vec::new(),
            children: vec![paragraph_block("not-item", "oops")],
        }]);
        let err = validate_document(&document).unwrap_err();
        assert!(matches!(err, AppError::Validation { code, .. } if code == "LIST_CHILD_INVALID"));
    }

    #[test]
    fn rejects_heading_level_seven() {
        let document = BlockDocument::from_blocks(vec![heading_block("h", 7, "too deep")]);
        let err = validate_document(&document).unwrap_err();
        assert!(
            matches!(err, AppError::Validation { code, .. } if code == "HEADING_LEVEL_INVALID")
        );
    }

    #[test]
    fn rejects_depth_thirty_three() {
        // root depth 0 + 33 nested children => deepest depth is 33
        let document = BlockDocument::from_blocks(vec![nested_depth(33)]);
        let err = validate_document(&document).unwrap_err();
        assert!(matches!(err, AppError::Validation { code, .. } if code == "BLOCK_DEPTH_EXCEEDED"));
    }

    #[test]
    fn rejects_more_than_max_blocks() {
        let blocks = (0..=MAX_BLOCKS_PER_DOCUMENT)
            .map(|index| paragraph_block(&format!("b{index}"), "x"))
            .collect::<Vec<_>>();
        let document = BlockDocument::from_blocks(blocks);
        let err = validate_document(&document).unwrap_err();
        assert!(matches!(err, AppError::Validation { code, .. } if code == "BLOCK_COUNT_EXCEEDED"));
    }

    #[test]
    fn rejects_oversized_text_node() {
        let huge = "a".repeat(MAX_TEXT_NODE_BYTES + 1);
        let document = paragraph_document("big", &huge);
        let err = validate_document(&document).unwrap_err();
        assert!(matches!(err, AppError::Validation { code, .. } if code == "TEXT_NODE_TOO_LARGE"));
    }

    #[test]
    fn rejects_oversized_document_json() {
        // Many medium text nodes push serialized JSON over the 10 MiB cap.
        let chunk = "字".repeat(8_000);
        let mut blocks = Vec::new();
        for index in 0..900 {
            blocks.push(paragraph_block(&format!("j{index}"), &chunk));
        }
        let document = BlockDocument::from_blocks(blocks);
        assert!(document.blocks.len() <= MAX_BLOCKS_PER_DOCUMENT);
        let err = validate_document(&document).unwrap_err();
        assert!(
            matches!(err, AppError::Validation { code, .. } if code == "DOCUMENT_JSON_TOO_LARGE")
        );
    }

    #[test]
    fn rejects_dangerous_link_scheme() {
        let document = BlockDocument::from_blocks(vec![BlockNode {
            id: id("p"),
            kind: BlockKind::Paragraph,
            attrs: BlockAttrs::default(),
            content: vec![InlineNode::Text {
                text: "click".to_string(),
                marks: vec![InlineMark::Link {
                    href: "javascript:alert(1)".to_string(),
                }],
            }],
            children: Vec::new(),
        }]);
        let err = validate_document(&document).unwrap_err();
        assert!(matches!(err, AppError::Validation { code, .. } if code == "LINK_SCHEME_INVALID"));
    }

    #[test]
    fn accepts_safe_link_schemes() {
        for href in [
            "http://example.com",
            "https://example.com/a",
            "mailto:a@b.c",
        ] {
            let document = BlockDocument::from_blocks(vec![BlockNode {
                id: id("p"),
                kind: BlockKind::Paragraph,
                attrs: BlockAttrs::default(),
                content: vec![InlineNode::Text {
                    text: "link".to_string(),
                    marks: vec![InlineMark::Link {
                        href: href.to_string(),
                    }],
                }],
                children: Vec::new(),
            }]);
            assert!(validate_document(&document).is_ok(), "href={href}");
        }
    }

    #[test]
    fn markdown_escapes_special_characters_and_keeps_wikilinks() {
        let document = BlockDocument::from_blocks(vec![BlockNode {
            id: id("p"),
            kind: BlockKind::Paragraph,
            attrs: BlockAttrs::default(),
            content: vec![
                text_inline("a*b_c"),
                InlineNode::Text {
                    text: "[[知识库]]".to_string(),
                    marks: Vec::new(),
                },
            ],
            children: Vec::new(),
        }]);
        let markdown = document_to_markdown(&document).expect("markdown");
        assert!(markdown.contains(r"a\*b\_c") || markdown.contains("a\\*b\\_c"));
        assert!(markdown.contains("[[知识库]]"));
        assert!(!markdown.contains(r"\[[知识库]]"));
    }

    #[test]
    fn markdown_renders_hard_break_as_backslash_newline() {
        let document = BlockDocument::from_blocks(vec![BlockNode {
            id: id("p"),
            kind: BlockKind::Paragraph,
            attrs: BlockAttrs::default(),
            content: vec![
                text_inline("hello"),
                InlineNode::HardBreak,
                text_inline("world"),
            ],
            children: Vec::new(),
        }]);
        let markdown = document_to_markdown(&document).expect("markdown");
        assert_eq!(markdown, "hello\\\nworld");
    }

    #[test]
    fn markdown_code_uses_longer_fences_for_backticks() {
        let document = BlockDocument::from_blocks(vec![
            BlockNode {
                id: id("code"),
                kind: BlockKind::CodeBlock,
                attrs: BlockAttrs {
                    level: None,
                    language: Some("rs".to_string()),
                    start: None,
                },
                content: vec![text_inline("let s = \"```\";\n")],
                children: Vec::new(),
            },
            BlockNode {
                id: id("p"),
                kind: BlockKind::Paragraph,
                attrs: BlockAttrs::default(),
                content: vec![InlineNode::Text {
                    text: "a`b".to_string(),
                    marks: vec![InlineMark::Code],
                }],
                children: Vec::new(),
            },
        ]);
        let markdown = document_to_markdown(&document).expect("markdown");
        assert!(markdown.contains("````rs\nlet s = \"```\";\n````"));
        assert!(markdown.contains("``a`b``") || markdown.contains("` a`b `"));
    }

    #[test]
    fn rejects_blockquote_with_inline_content() {
        let document = BlockDocument::from_blocks(vec![BlockNode {
            id: id("bq"),
            kind: BlockKind::Blockquote,
            attrs: BlockAttrs::default(),
            content: vec![text_inline("lost if only children render")],
            children: Vec::new(),
        }]);
        let err = validate_document(&document).unwrap_err();
        assert!(
            matches!(err, AppError::Validation { code, .. } if code == "BLOCKQUOTE_CONTENT_INVALID")
        );
    }

    #[test]
    fn plain_text_joins_blocks_and_hard_breaks() {
        let document = BlockDocument::from_blocks(vec![
            BlockNode {
                id: id("p1"),
                kind: BlockKind::Paragraph,
                attrs: BlockAttrs::default(),
                content: vec![
                    text_inline("hello"),
                    InlineNode::HardBreak,
                    text_inline("world"),
                ],
                children: Vec::new(),
            },
            paragraph_block("p2", "next"),
        ]);
        let plain = document_to_plain_text(&document);
        assert_eq!(plain, "hello\nworld\n\nnext");
    }

    #[test]
    fn outline_collects_headings_in_order() {
        let h1 = id("h1");
        let h2 = id("h2");
        let document = BlockDocument::from_blocks(vec![
            heading_block("h1", 1, "One"),
            paragraph_block("p", "body"),
            heading_block("h2", 2, "Two"),
        ]);
        let outline = document_outline(&document);
        assert_eq!(outline.len(), 2);
        assert_eq!(outline[0].id, h1);
        assert_eq!(outline[0].level, 1);
        assert_eq!(outline[0].text, "One");
        assert_eq!(outline[1].id, h2);
        assert_eq!(outline[1].level, 2);
        assert_eq!(outline[1].text, "Two");
    }

    #[test]
    fn legacy_text_splits_paragraphs_and_hard_breaks_without_markdown() {
        let document = legacy_text_document("line1\nline2\n\n**not bold**\n\n[[Wiki]]");
        assert_eq!(document.schema_version, DOCUMENT_SCHEMA_VERSION);
        assert_eq!(document.blocks.len(), 3);
        assert_eq!(document.blocks[0].kind, BlockKind::Paragraph);
        assert!(matches!(
            &document.blocks[0].content[..],
            [
                InlineNode::Text { text: t1, .. },
                InlineNode::HardBreak,
                InlineNode::Text { text: t2, .. },
            ] if t1 == "line1" && t2 == "line2"
        ));
        assert!(matches!(
            &document.blocks[1].content[..],
            [InlineNode::Text { text, .. }] if text == "**not bold**"
        ));
        assert!(matches!(
            &document.blocks[2].content[..],
            [InlineNode::Text { text, .. }] if text == "[[Wiki]]"
        ));
        assert!(validate_document(&document).is_ok());
        // Hydrate-only: derived markdown is available but must not be required for validity.
        let _ = document_to_markdown(&document).expect("derived markdown is optional writeback");
    }

    #[test]
    fn legacy_hydrate_only_does_not_side_effect_or_require_writeback() {
        // Pure helpers are side-effect free: hydrate constructs a valid document without
        // forcing callers to serialize. Writeback remains an explicit later step.
        let original = "keep me\nas-is\n\nsecond";
        let document = legacy_text_document(original);
        assert!(validate_document(&document).is_ok());
        let projection = flatten_blocks(&document);
        let outline = document_outline(&document);
        let plain = document_to_plain_text(&document);
        assert!(!projection.is_empty());
        assert!(outline.is_empty());
        assert!(plain.contains("keep me"));
        // Callers that only hydrate must not treat markdown derivation as mandatory writeback.
        // document_to_markdown is pure and opt-in; not invoking it is the hydrate-only contract.
    }

    #[test]
    fn flatten_blocks_assigns_parent_ordinal_and_depth() {
        let bq = id("bq");
        let c0 = id("c0");
        let c1 = id("c1");
        let document = BlockDocument::from_blocks(vec![BlockNode {
            id: bq.clone(),
            kind: BlockKind::Blockquote,
            attrs: BlockAttrs::default(),
            content: Vec::new(),
            children: vec![paragraph_block("c0", "a"), paragraph_block("c1", "b")],
        }]);
        let flat = flatten_blocks(&document);
        assert_eq!(flat.len(), 3);
        assert_eq!(flat[0].id, bq);
        assert_eq!(flat[0].parent_block_id, None);
        assert_eq!(flat[0].ordinal, 0);
        assert_eq!(flat[0].depth, 0);
        assert_eq!(flat[1].id, c0);
        assert_eq!(flat[1].parent_block_id.as_deref(), Some(bq.as_str()));
        assert_eq!(flat[1].ordinal, 0);
        assert_eq!(flat[1].depth, 1);
        assert_eq!(flat[2].id, c1);
        assert_eq!(flat[2].ordinal, 1);
        assert_eq!(flat[2].depth, 1);
        assert_eq!(flat[1].text_content, "a");
        assert_eq!(flat[2].text_content, "b");
    }
}
