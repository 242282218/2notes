#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WikiLink {
    pub raw_target: String,
    pub normalized_target: String,
}

pub fn canonicalize_knowledge_title(input: &str) -> String {
    input.split_whitespace().collect::<Vec<_>>().join(" ")
}

pub fn normalize_knowledge_title(input: &str) -> String {
    canonicalize_knowledge_title(input).to_lowercase()
}

pub fn parse_wiki_links(content: &str) -> Vec<WikiLink> {
    let bytes = content.as_bytes();
    let mut links = Vec::new();
    let mut cursor = 0;

    while cursor + 1 < bytes.len() {
        let Some(relative_open) = content[cursor..].find("[[") else {
            break;
        };
        let open = cursor + relative_open;
        let prefix = open.checked_sub(1).map(|index| bytes[index]);
        if prefix == Some(b'\\') || prefix == Some(b'!') {
            cursor = open + 2;
            continue;
        }

        let target_start = open + 2;
        let Some(relative_close) = content[target_start..].find("]]") else {
            break;
        };
        let close = target_start + relative_close;
        if let Some(nested) = content[target_start..close].find("[[") {
            cursor = target_start + nested;
            continue;
        }

        let raw_target = content[target_start..close].trim();
        let invalid = raw_target.is_empty()
            || raw_target.chars().count() > 200
            || raw_target.contains('|')
            || raw_target.chars().any(|ch| ch == '\r' || ch == '\n');
        if !invalid {
            links.push(WikiLink {
                raw_target: raw_target.to_string(),
                normalized_target: normalize_knowledge_title(raw_target),
            });
        }
        cursor = close + 2;
    }

    links
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_valid_links_and_preserves_occurrences() {
        let links = parse_wiki_links("前文 [[ 知识库 ]] 与 [[知识库]]");
        assert_eq!(links.len(), 2);
        assert!(links.iter().all(|link| link.raw_target == "知识库"));
        assert!(links.iter().all(|link| link.normalized_target == "知识库"));
    }

    #[test]
    fn ignores_unsupported_or_invalid_forms() {
        let oversized = "长".repeat(201);
        let input = format!(r"\[[跳过]] ![[嵌入]] [[目标|显示]] [[]] [[{oversized}]]");
        assert!(parse_wiki_links(&input).is_empty());
    }

    #[test]
    fn restarts_from_nested_opening_brackets() {
        let links = parse_wiki_links("[[未闭合 [[保留]]");
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].raw_target, "保留");
    }

    #[test]
    fn canonicalizes_display_title_and_normalizes_key() {
        assert_eq!(
            canonicalize_knowledge_title("  Phase\n  TWO  "),
            "Phase TWO"
        );
        assert_eq!(normalize_knowledge_title("  Phase\n  TWO  "), "phase two");
    }
}
