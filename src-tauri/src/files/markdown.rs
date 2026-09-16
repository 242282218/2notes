use std::{
    collections::HashSet,
    fs,
    io::Write,
    path::{Path, PathBuf},
};

use blake3::Hasher;
use serde::{Deserialize, Serialize};

#[cfg(test)]
use crate::db::repos::EntriesRepo;
use crate::{
    content::document::document_to_markdown,
    error::{AppError, AppResult},
    types::entries::EntryDetail,
};

#[derive(Debug, Serialize, Deserialize)]
struct Frontmatter {
    id: String,
    #[serde(rename = "type")]
    entry_type: String,
    status: String,
    knowledge_state: String,
    knowledge_promoted_at: Option<String>,
    aliases: Vec<String>,
    tags: Vec<String>,
    created_at: String,
    updated_at: String,
    deleted_at: Option<String>,
    original_content: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ExportManifest {
    format: &'static str,
    app_version: &'static str,
    schema_version: i64,
    entry_count: usize,
    files: Vec<ExportManifestFile>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ExportManifestFile {
    path: String,
    bytes: u64,
    blake3: String,
}

#[cfg(test)]
pub fn export_all(conn: &rusqlite::Connection, target_dir: &Path) -> AppResult<Vec<PathBuf>> {
    let entries = EntriesRepo::list_exportable(conn)?;
    export_entries(&entries, target_dir)
}

pub fn export_entries(entries: &[EntryDetail], target_dir: &Path) -> AppResult<Vec<PathBuf>> {
    fs::create_dir_all(target_dir)?;
    let mut used = HashSet::new();

    // Pre-scan target directory to populate `used` once, avoiding O(n²) disk hits.
    if let Ok(dir_entries) = fs::read_dir(target_dir) {
        for entry in dir_entries.flatten() {
            if let Some(name) = entry.file_name().to_str() {
                used.insert(name.to_string());
            }
        }
    }

    let mut paths = Vec::new();

    for entry in entries {
        let file_name = unique_file_name(entry, &mut used, target_dir)?;
        let path = target_dir.join(file_name);
        write_entry_file(&path, &render_entry(entry)?)?;
        paths.push(path);
    }

    write_full_json_export(entries, target_dir, &mut used)?;
    write_export_manifest(entries.len(), &paths, target_dir, &mut used)?;
    Ok(paths)
}

fn write_entry_file(path: &Path, content: &str) -> AppResult<()> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let mut file = tempfile::NamedTempFile::new_in(parent)?;
    file.write_all(content.as_bytes())?;
    file.flush()?;
    file.persist_noclobber(path).map_err(std::io::Error::from)?;
    Ok(())
}

fn write_full_json_export(
    entries: &[EntryDetail],
    target_dir: &Path,
    used: &mut HashSet<String>,
) -> AppResult<()> {
    let file_name = unique_metadata_name("2notes-entries", used);
    let path = target_dir.join(file_name);
    let content = serde_json::to_string_pretty(entries)
        .map_err(|err| AppError::validation("EXPORT_JSON_ENCODE", err.to_string()))?;
    write_entry_file(&path, &content)
}

fn write_export_manifest(
    entry_count: usize,
    markdown_paths: &[PathBuf],
    target_dir: &Path,
    used: &mut HashSet<String>,
) -> AppResult<()> {
    let files = markdown_paths
        .iter()
        .map(|path| -> AppResult<ExportManifestFile> {
            let content = fs::read(path)?;
            let mut hasher = Hasher::new();
            hasher.update(&content);
            let bytes = (content.len() as u64, hasher.finalize().to_hex().to_string());
            Ok(ExportManifestFile {
                path: path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or_default()
                    .to_string(),
                bytes: bytes.0,
                blake3: bytes.1,
            })
        })
        .collect::<AppResult<Vec<_>>>()?;
    let manifest = ExportManifest {
        format: "2notes-markdown-export-v1",
        app_version: env!("CARGO_PKG_VERSION"),
        schema_version: crate::db::migrations::current_schema_version(),
        entry_count,
        files,
    };
    let content = serde_json::to_string_pretty(&manifest)
        .map_err(|err| AppError::validation("EXPORT_MANIFEST_ENCODE", err.to_string()))?;
    let path = target_dir.join(unique_metadata_name("2notes-manifest", used));
    write_entry_file(&path, &content)
}

fn unique_metadata_name(prefix: &str, used: &mut HashSet<String>) -> String {
    let mut index = 0;
    loop {
        let suffix = if index == 0 {
            String::new()
        } else {
            format!("-{index}")
        };
        let candidate = format!("{prefix}{suffix}.json");
        if used.insert(candidate.clone()) {
            return candidate;
        }
        index += 1;
    }
}

fn render_entry(entry: &EntryDetail) -> AppResult<String> {
    let frontmatter = Frontmatter {
        id: entry.id.clone(),
        entry_type: entry.entry_type.as_str().to_string(),
        status: entry.status.as_str().to_string(),
        knowledge_state: entry.knowledge_state.as_str().to_string(),
        knowledge_promoted_at: entry.knowledge_promoted_at.clone(),
        aliases: entry.knowledge_aliases.clone(),
        tags: entry.tags.iter().map(|tag| tag.name.clone()).collect(),
        created_at: entry.created_at.clone(),
        updated_at: entry.updated_at.clone(),
        deleted_at: entry.deleted_at.clone(),
        original_content: entry.original_content.clone(),
    };
    let yaml = serde_json::to_string_pretty(&frontmatter)
        .map_err(|err| AppError::Yaml(err.to_string()))?;
    let title = entry
        .title
        .clone()
        .or_else(|| super::super::db::repos::entries_repo::auto_title(&entry.current_content))
        .unwrap_or_else(|| "untitled".to_string());

    // Body is the document's canonical Markdown, which preserves `[[wiki]]` as plain text
    // while escaping syntax characters; this keeps exports round-trippable for Task 12.
    let body_markdown =
        document_to_markdown(&entry.document).unwrap_or_else(|_| entry.current_content.clone());

    Ok(format!(
        "---\n{}\n---\n\n# {}\n\n{}\n",
        yaml, title, body_markdown
    ))
}

fn unique_file_name(
    entry: &EntryDetail,
    used: &mut HashSet<String>,
    _target_dir: &Path,
) -> AppResult<String> {
    // Use shared timestamp helper for consistent naming across backups and markdown exports.
    let timestamp = super::timestamps::markdown_timestamp(&entry.created_at);
    let summary = entry
        .title
        .clone()
        .or_else(|| super::super::db::repos::entries_repo::auto_title(&entry.current_content))
        .unwrap_or_else(|| "untitled".to_string())
        .chars()
        .take(40)
        .collect::<String>();
    let sanitized = sanitize_filename::sanitize(summary);
    let slug = if sanitized.trim().is_empty() {
        "untitled".to_string()
    } else {
        sanitized
    };
    let suffix = entry.id.chars().take(8).collect::<String>();
    let base_name = format!("{timestamp}-{slug}");
    let mut candidate = format!("{base_name}.md");
    let mut collision_count = 0;
    // Upper bound prevents a pathological `used` set from looping forever;
    // 9999 collisions is far beyond any legitimate export volume and keeps the
    // operation bounded even when the directory already holds many siblings.
    const MAX_COLLISIONS: u32 = 9999;

    if used.contains(&candidate) {
        loop {
            collision_count += 1;
            candidate = if collision_count == 1 {
                format!("{base_name}-{suffix}.md")
            } else {
                format!("{base_name}-{suffix}-{collision_count}.md")
            };
            if !used.contains(&candidate) {
                break;
            }
            if collision_count >= MAX_COLLISIONS {
                return Err(AppError::validation(
                    "EXPORT_FILE_NAME_EXHAUSTED",
                    format!(
                        "could not allocate unique markdown file name for entry {entry_id}",
                        entry_id = entry.id
                    ),
                ));
            }
        }
    }
    used.insert(candidate.clone());
    Ok(candidate)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        db::{
            connection::open_in_memory,
            migrations::now_string,
            repos::{EntriesRepo, KnowledgeRepo},
        },
        types::{
            documents::{BlockDocument, BlockNode},
            entries::{EntryPatch, EntryStatus},
        },
    };

    fn paragraph_document(content: impl Into<String>) -> BlockDocument {
        BlockDocument::from_blocks(vec![BlockNode::paragraph(
            uuid::Uuid::new_v4().to_string(),
            content,
        )])
    }

    #[test]
    fn exports_parseable_frontmatter_and_sanitized_names() {
        let (mut conn, _) = open_in_memory().unwrap();
        let now = now_string();
        let tx = conn.transaction().unwrap();
        let entry = EntriesRepo::create(&tx, "hello <>:\"/\\|?* world", &now).unwrap();
        EntriesRepo::update(
            &tx,
            &entry.id,
            EntryPatch {
                document: Some(paragraph_document("changed")),
                title: Some("hello <>:\"/\\|?* world".to_string()),
                entry_type: None,
                status: Some(EntryStatus::Done),
                tags: Some(vec!["a:b".to_string(), "中文".to_string()]),
            },
            entry.revision,
            &now,
        )
        .unwrap();
        tx.commit().unwrap();

        let dir = tempfile::tempdir().unwrap();
        let paths = export_all(&conn, dir.path()).unwrap();
        let content = fs::read_to_string(&paths[0]).unwrap();
        assert!(content.starts_with("---\n{\n"));
        assert!(content.contains("\n}\n---\n\n# "));
        let yaml = content.split("---").nth(1).unwrap();
        let parsed: Frontmatter = serde_json::from_str(yaml).unwrap();

        assert_eq!(paths.len(), 1);
        assert_eq!(parsed.tags.len(), 2);
        assert_eq!(parsed.original_content, "hello <>:\"/\\|?* world");
        assert!(!paths[0]
            .file_name()
            .unwrap()
            .to_string_lossy()
            .contains('<'));
    }

    #[test]
    fn export_writes_full_json_and_manifest_with_hashes() {
        let (mut conn, _) = open_in_memory().unwrap();
        let tx = conn.transaction().unwrap();
        EntriesRepo::create(&tx, "manifest entry", &now_string()).unwrap();
        tx.commit().unwrap();

        let dir = tempfile::tempdir().unwrap();
        let paths = export_all(&conn, dir.path()).unwrap();
        assert_eq!(paths.len(), 1);
        assert!(dir.path().join("2notes-entries.json").is_file());
        let manifest = fs::read_to_string(dir.path().join("2notes-manifest.json")).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&manifest).unwrap();
        assert_eq!(parsed["format"], "2notes-markdown-export-v1");
        assert_eq!(parsed["entryCount"], 1);
        assert_eq!(
            parsed["schemaVersion"],
            crate::db::migrations::current_schema_version()
        );
        assert_eq!(
            parsed["files"][0]["path"],
            paths[0].file_name().unwrap().to_string_lossy().as_ref()
        );
        assert_eq!(parsed["files"][0]["blake3"].as_str().unwrap().len(), 64);
    }

    #[test]
    fn exports_original_content_as_structured_frontmatter() {
        let (mut conn, _) = open_in_memory().unwrap();
        let now = now_string();
        let raw = "raw capture\n\n---\n\n## 原始内容\n\nmarker\n";
        let tx = conn.transaction().unwrap();
        EntriesRepo::create(&tx, raw, &now).unwrap();
        tx.commit().unwrap();

        let dir = tempfile::tempdir().unwrap();
        let paths = export_all(&conn, dir.path()).unwrap();
        let content = fs::read_to_string(&paths[0]).unwrap();
        let frontmatter = content
            .strip_prefix("---\n")
            .and_then(|rest| rest.split_once("\n---\n\n"))
            .map(|(frontmatter, _)| frontmatter)
            .unwrap();
        let metadata: serde_json::Value = serde_json::from_str(frontmatter).unwrap();

        assert_eq!(metadata["original_content"], raw.trim_end_matches('\n'));
        assert!(!content.contains("\n\n---\n\n## 原始内容\n"));
    }

    #[test]
    fn exports_knowledge_metadata() {
        let (mut conn, _) = open_in_memory().unwrap();
        let now = now_string();
        let tx = conn.transaction().unwrap();
        let entry = EntriesRepo::create(&tx, "[[关联标题]] 正文", &now).unwrap();
        let entry = EntriesRepo::update(
            &tx,
            &entry.id,
            EntryPatch {
                document: None,
                title: Some("旧标题".to_string()),
                entry_type: None,
                status: None,
                tags: None,
            },
            entry.revision,
            &now,
        )
        .unwrap();
        let entry = KnowledgeRepo::promote(&tx, &entry.id, entry.revision, &now).unwrap();
        EntriesRepo::update(
            &tx,
            &entry.id,
            EntryPatch {
                document: None,
                title: Some("新标题".to_string()),
                entry_type: None,
                status: None,
                tags: None,
            },
            entry.revision,
            &now,
        )
        .unwrap();
        tx.commit().unwrap();

        let dir = tempfile::tempdir().unwrap();
        let paths = export_all(&conn, dir.path()).unwrap();
        let content = fs::read_to_string(&paths[0]).unwrap();
        let frontmatter = content
            .strip_prefix("---\n")
            .and_then(|rest| rest.split_once("\n---\n\n"))
            .map(|(frontmatter, _)| frontmatter)
            .unwrap();
        let parsed: Frontmatter = serde_json::from_str(frontmatter).unwrap();

        assert_eq!(parsed.knowledge_state, "knowledge");
        assert_eq!(parsed.knowledge_promoted_at.as_deref(), Some(now.as_str()));
        assert_eq!(parsed.aliases, vec!["旧标题"]);
        assert!(content.contains("\"aliases\""));
        assert!(!content.contains("\"knowledge_aliases\""));
        assert!(content.contains("\n\n[[关联标题]] 正文\n"));
        assert_eq!(parsed.original_content, "[[关联标题]] 正文");
    }

    #[test]
    fn export_does_not_overwrite_existing_suffix_collision() {
        let (mut conn, _) = open_in_memory().unwrap();
        let now = now_string();
        let tx = conn.transaction().unwrap();
        let _entry = EntriesRepo::create(&tx, "duplicate", &now).unwrap();
        tx.commit().unwrap();

        let dir = tempfile::tempdir().unwrap();

        // Pre-create two files on disk that simulate prior exports of the same entry.
        // export_entries will discover them via the pre-scan and avoid collisions.
        let first_name = "2026-07-06-114548-duplicate.md";
        let second_name = "2026-07-06-114548-duplicate-abc12345.md";
        fs::write(dir.path().join(first_name), "first").unwrap();
        fs::write(dir.path().join(second_name), "second").unwrap();

        let paths = export_all(&conn, dir.path()).unwrap();

        // The export should pick a name that does not collide with either
        // pre-existing file. Both "first" and "second" files must remain untouched.
        assert_eq!(paths.len(), 1, "paths={paths:?}");
        assert_ne!(paths[0].file_name().unwrap().to_string_lossy(), second_name);
        assert_ne!(paths[0].file_name().unwrap().to_string_lossy(), first_name);
        assert_eq!(
            fs::read_to_string(dir.path().join(first_name)).unwrap(),
            "first"
        );
        assert_eq!(
            fs::read_to_string(dir.path().join(second_name)).unwrap(),
            "second"
        );
    }
}
