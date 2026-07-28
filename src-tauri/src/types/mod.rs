pub mod backups;
pub mod documents;
pub mod entries;
pub mod knowledge;
pub mod settings;
pub mod tags;

#[cfg(test)]
mod tests {
    use ts_rs::{Config, TS};

    use super::{
        backups::BackupInfo,
        documents::{
            BlockAttrs, BlockDocument, BlockKind, BlockNode, BlockProjection, InlineMark,
            InlineNode, OutlineItem,
        },
        entries::{
            EntryDetail, EntryListFilter, EntryListItem, EntryPage, EntryPatch, EntryStatus,
            EntryType, PageRequest, TitleSource,
        },
        knowledge::{
            KnowledgeIndexReport, KnowledgeRelations, KnowledgeState, KnowledgeSuggestion,
            RelatedEntry, SearchSnippet, SearchSnippetPart, UnresolvedWikiLink,
        },
        settings::{AppSettings, Draft, ExportResult, SettingsPatch, ThemeMode},
        tags::Tag,
    };
    use crate::error::AppErrorResponse;

    fn generated_typescript() -> String {
        let config = Config::default();
        let declarations = [
            KnowledgeState::decl(&config),
            EntryType::decl(&config),
            EntryStatus::decl(&config),
            TitleSource::decl(&config),
            KnowledgeSuggestion::decl(&config),
            RelatedEntry::decl(&config),
            UnresolvedWikiLink::decl(&config),
            KnowledgeRelations::decl(&config),
            SearchSnippetPart::decl(&config),
            SearchSnippet::decl(&config),
            KnowledgeIndexReport::decl(&config),
            Tag::decl(&config),
            EntryListFilter::decl(&config),
            PageRequest::decl(&config),
            EntryListItem::decl(&config),
            EntryPage::decl(&config),
            EntryDetail::decl(&config),
            EntryPatch::decl(&config),
            Draft::decl(&config),
            ThemeMode::decl(&config),
            AppSettings::decl(&config),
            SettingsPatch::decl(&config),
            ExportResult::decl(&config),
            BackupInfo::decl(&config),
            AppErrorResponse::decl(&config),
            BlockKind::decl(&config),
            BlockAttrs::decl(&config),
            InlineMark::decl(&config),
            InlineNode::decl(&config),
            BlockNode::decl(&config),
            BlockDocument::decl(&config),
            OutlineItem::decl(&config),
            BlockProjection::decl(&config),
        ]
        .map(|declaration| format!("export {declaration}"));
        format!("{}\n", declarations.join("\n\n"))
    }

    #[test]
    fn generated_typescript_is_current() {
        let actual = normalize_typescript(include_str!("../../../src/types/generated.ts"));
        let expected = normalize_typescript(&generated_typescript());
        assert_eq!(actual, expected);
    }

    fn normalize_typescript(input: &str) -> String {
        let mut normalized = input
            .chars()
            .filter(|ch| !ch.is_whitespace())
            .map(|ch| if ch == ',' { ';' } else { ch })
            .collect::<String>();
        normalized = normalized.replace("=|", "=").replace("\"type\":", "type:");
        while normalized.contains(";;") || normalized.contains(";}") {
            normalized = normalized.replace(";;", ";").replace(";}", "}");
        }
        normalized
    }
}
