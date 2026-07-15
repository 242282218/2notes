pub mod backups;
pub mod entries;
pub mod knowledge;
pub mod settings;
pub mod tags;

#[cfg(test)]
mod tests {
    use ts_rs::{Config, TS};

    use super::{
        backups::BackupInfo,
        entries::{
            EntryDetail, EntryListFilter, EntryListItem, EntryPage, EntryPatch, EntryStatus,
            EntryType, PageRequest, TitleSource,
        },
        settings::{AppSettings, Draft, ExportResult, SettingsPatch},
        tags::Tag,
    };
    use crate::error::AppErrorResponse;

    fn generated_typescript() -> String {
        let config = Config::default();
        let declarations = [
            EntryType::decl(&config),
            EntryStatus::decl(&config),
            TitleSource::decl(&config),
            Tag::decl(&config),
            EntryListFilter::decl(&config),
            PageRequest::decl(&config),
            EntryListItem::decl(&config),
            EntryPage::decl(&config),
            EntryDetail::decl(&config),
            EntryPatch::decl(&config),
            Draft::decl(&config),
            AppSettings::decl(&config),
            SettingsPatch::decl(&config),
            ExportResult::decl(&config),
            BackupInfo::decl(&config),
            AppErrorResponse::decl(&config),
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
        while normalized.contains(";;") || normalized.contains(";}") {
            normalized = normalized.replace(";;", ";").replace(";}", "}");
        }
        normalized
    }
}
