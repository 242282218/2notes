use std::{
    collections::HashSet,
    fs,
    io::Write,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

#[cfg(test)]
use crate::db::repos::EntriesRepo;
use crate::{
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
                if name.ends_with(".md") {
                    used.insert(name.to_string());
                }
            }
        }
    }

    let mut paths = Vec::new();

    for entry in entries {
        let file_name = unique_file_name(entry, &mut used, target_dir);
        let path = target_dir.join(file_name);
        write_entry_file(&path, &render_entry(entry)?)?;
        paths.push(path);
    }

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
    };
    let yaml = serde_json::to_string_pretty(&frontmatter)
        .map_err(|err| AppError::Yaml(err.to_string()))?;
    let title = entry
        .title
        .clone()
        .or_else(|| super::super::db::repos::entries_repo::auto_title(&entry.current_content))
        .unwrap_or_else(|| "untitled".to_string());

    Ok(format!(
        "---\n{}\n---\n\n# {}\n\n{}\n\n---\n\n## 原始内容\n\n{}\n",
        yaml, title, entry.current_content, entry.original_content
    ))
}

fn unique_file_name(entry: &EntryDetail, used: &mut HashSet<String>, _target_dir: &Path) -> String {
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
        }
    }
    used.insert(candidate.clone());
    candidate
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
        types::entries::{EntryPatch, EntryStatus},
    };

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
                document: None,
                title: Some("hello <>:\"/\\|?* world".to_string()),
                current_content: Some("changed".to_string()),
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
        assert!(content.contains("## 原始内容"));
        assert!(!paths[0]
            .file_name()
            .unwrap()
            .to_string_lossy()
            .contains('<'));
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
                current_content: None,
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
                current_content: None,
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
        let parsed: Frontmatter =
            serde_json::from_str(content.split("---").nth(1).unwrap()).unwrap();

        assert_eq!(parsed.knowledge_state, "knowledge");
        assert_eq!(parsed.knowledge_promoted_at.as_deref(), Some(now.as_str()));
        assert_eq!(parsed.aliases, vec!["旧标题"]);
        assert!(content.contains("\"aliases\""));
        assert!(!content.contains("\"knowledge_aliases\""));
        assert!(content.contains("\n\n[[关联标题]] 正文\n\n---"));
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
