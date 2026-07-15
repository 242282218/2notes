use std::collections::HashMap;

use rusqlite::{params, Connection, OptionalExtension, Transaction};

use crate::{
    db::{migrations::now_string, repos::EntriesRepo},
    error::{AppError, AppResult},
    knowledge::wiki_links::{
        canonicalize_knowledge_title, normalize_knowledge_title, parse_wiki_links,
    },
    types::{
        entries::EntryDetail,
        knowledge::{KnowledgeIndexReport, KnowledgeSuggestion},
    },
};

const LINK_INDEX_VERSION_KEY: &str = "knowledge_link_index_version";

pub struct KnowledgeRepo;

impl KnowledgeRepo {
    pub fn suggest(
        conn: &Connection,
        query: &str,
        limit: u32,
    ) -> AppResult<Vec<KnowledgeSuggestion>> {
        let normalized = normalize_knowledge_title(query);
        let limit = i64::from(limit.clamp(1, 20));
        if normalized.is_empty() {
            let mut stmt = conn.prepare(
                "SELECT id, title, NULL
                 FROM entries
                 WHERE knowledge_state = 'knowledge' AND deleted_at IS NULL
                 ORDER BY updated_at DESC, id ASC
                 LIMIT ?1",
            )?;
            return collect_suggestions(stmt.query_map([limit], map_suggestion)?);
        }

        let pattern = format!("%{}%", super::entries_repo::escape_like(&normalized));
        let mut stmt = conn.prepare(
            "SELECT e.id, e.title,
                    CASE
                        WHEN e.knowledge_title_key LIKE ?2 ESCAPE '\\' THEN NULL
                        ELSE (
                            SELECT ea.alias
                            FROM entry_aliases ea
                            WHERE ea.entry_id = e.id
                              AND ea.normalized_alias LIKE ?2 ESCAPE '\\'
                            ORDER BY ea.normalized_alias
                            LIMIT 1
                        )
                    END AS matched_alias
             FROM entries e
             WHERE e.knowledge_state = 'knowledge'
               AND e.deleted_at IS NULL
               AND (
                   e.knowledge_title_key LIKE ?2 ESCAPE '\\'
                   OR EXISTS (
                       SELECT 1
                       FROM entry_aliases ea
                       WHERE ea.entry_id = e.id
                         AND ea.normalized_alias LIKE ?2 ESCAPE '\\'
                   )
               )
             ORDER BY CASE
                          WHEN e.knowledge_title_key = ?1 THEN 0
                          WHEN e.knowledge_title_key LIKE ?2 ESCAPE '\\' THEN 1
                          ELSE 2
                      END,
                      e.title COLLATE NOCASE,
                      e.id
             LIMIT ?3",
        )?;
        let suggestions = collect_suggestions(
            stmt.query_map(params![normalized, pattern, limit], map_suggestion)?,
        );
        suggestions
    }

    pub fn promote(
        tx: &Transaction<'_>,
        id: &str,
        expected_revision: i64,
        now: &str,
    ) -> AppResult<EntryDetail> {
        let (title, content, revision, deleted_at, state): (
            Option<String>,
            String,
            i64,
            Option<String>,
            String,
        ) = tx
            .query_row(
                "SELECT title, current_content, revision, deleted_at, knowledge_state
                 FROM entries WHERE id = ?1",
                [id],
                |row| {
                    Ok((
                        row.get(0)?,
                        row.get(1)?,
                        row.get(2)?,
                        row.get(3)?,
                        row.get(4)?,
                    ))
                },
            )
            .optional()?
            .ok_or_else(|| AppError::not_found("条目不存在"))?;
        if revision != expected_revision {
            return Err(AppError::RevisionConflict);
        }
        if deleted_at.is_some() {
            return Err(AppError::validation(
                "ENTRY_IN_TRASH",
                "回收站中的条目不能沉淀",
            ));
        }
        if state == "knowledge" {
            return Err(AppError::validation(
                "ENTRY_ALREADY_KNOWLEDGE",
                "条目已在知识库中",
            ));
        }
        let canonical = validate_title(title.as_deref().unwrap_or_default())?;
        let key = normalize_knowledge_title(&canonical);
        ensure_title_available(tx, &key, id)?;

        tx.execute(
            "UPDATE entries SET title = ?1, title_source = 'user', knowledge_state = 'knowledge',
             knowledge_promoted_at = ?2, knowledge_title_key = ?3,
             revision = revision + 1, updated_at = ?2 WHERE id = ?4",
            params![canonical, now, key, id],
        )?;
        Self::refresh_source_links(tx, id, &content)?;
        tx.execute(
            "UPDATE entry_links SET target_entry_id = ?1
             WHERE target_entry_id IS NULL AND normalized_target = ?2",
            params![id, key],
        )?;
        EntriesRepo::get_with_tx(tx, id)
    }

    pub fn demote(
        tx: &Transaction<'_>,
        id: &str,
        expected_revision: i64,
        now: &str,
    ) -> AppResult<EntryDetail> {
        let (revision, state): (i64, String) = tx
            .query_row(
                "SELECT revision, knowledge_state FROM entries WHERE id = ?1",
                [id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?
            .ok_or_else(|| AppError::not_found("条目不存在"))?;
        if revision != expected_revision {
            return Err(AppError::RevisionConflict);
        }
        if state != "knowledge" {
            return Err(AppError::validation(
                "ENTRY_NOT_KNOWLEDGE",
                "条目不在知识库中",
            ));
        }
        tx.execute(
            "UPDATE entries SET knowledge_state = 'capture', knowledge_promoted_at = NULL,
             knowledge_title_key = NULL, revision = revision + 1, updated_at = ?1 WHERE id = ?2",
            params![now, id],
        )?;
        tx.execute("DELETE FROM entry_aliases WHERE entry_id = ?1", [id])?;
        tx.execute(
            "UPDATE entry_links SET target_entry_id = NULL WHERE target_entry_id = ?1",
            [id],
        )?;
        EntriesRepo::get_with_tx(tx, id)
    }

    pub(crate) fn sync_title_metadata(
        tx: &Transaction<'_>,
        entry_id: &str,
        old_title: &str,
        new_title: &str,
        now: &str,
    ) -> AppResult<()> {
        let canonical = validate_title(new_title)?;
        let old_key = normalize_knowledge_title(old_title);
        let new_key = normalize_knowledge_title(&canonical);
        ensure_title_available(tx, &new_key, entry_id)?;
        tx.execute(
            "DELETE FROM entry_aliases WHERE normalized_alias = ?1 AND entry_id = ?2",
            params![new_key, entry_id],
        )?;
        if old_key != new_key {
            tx.execute(
                "INSERT OR IGNORE INTO entry_aliases(normalized_alias, entry_id, alias, created_at)
                 VALUES (?1, ?2, ?3, ?4)",
                params![
                    old_key,
                    entry_id,
                    canonicalize_knowledge_title(old_title),
                    now
                ],
            )?;
        }
        tx.execute(
            "UPDATE entries SET title = ?1, knowledge_title_key = ?2 WHERE id = ?3",
            params![canonical, new_key, entry_id],
        )?;
        tx.execute(
            "UPDATE entry_links SET target_entry_id = ?1
             WHERE target_entry_id IS NULL AND normalized_target = ?2",
            params![entry_id, new_key],
        )?;
        Ok(())
    }

    pub fn refresh_source_links(
        tx: &Transaction<'_>,
        source_id: &str,
        current_content: &str,
    ) -> AppResult<()> {
        tx.execute(
            "DELETE FROM entry_links WHERE source_entry_id = ?1",
            [source_id],
        )?;
        for (ordinal, link) in parse_wiki_links(current_content).into_iter().enumerate() {
            let target_id = resolve_target_id(tx, &link.normalized_target)?;
            tx.execute(
                "INSERT INTO entry_links(source_entry_id, ordinal, raw_target, normalized_target, target_entry_id)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![source_id, ordinal as i64, link.raw_target, link.normalized_target, target_id],
            )?;
        }
        Ok(())
    }

    pub(crate) fn aliases_for_entry(conn: &Connection, entry_id: &str) -> AppResult<Vec<String>> {
        Ok(Self::aliases_for_entries(conn, &[entry_id.to_string()])?
            .remove(entry_id)
            .unwrap_or_default())
    }

    pub(crate) fn aliases_for_entries(
        conn: &Connection,
        entry_ids: &[String],
    ) -> AppResult<HashMap<String, Vec<String>>> {
        let mut aliases = HashMap::new();
        let mut stmt = conn.prepare(
            "SELECT entry_id, alias FROM entry_aliases ORDER BY created_at, normalized_alias",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?;
        for row in rows {
            let (entry_id, alias) = row?;
            if entry_ids.contains(&entry_id) {
                aliases.entry(entry_id).or_insert_with(Vec::new).push(alias);
            }
        }
        Ok(aliases)
    }

    pub fn rebuild_links(tx: &Transaction<'_>) -> AppResult<KnowledgeIndexReport> {
        tx.execute("DELETE FROM entry_links", [])?;
        let mut stmt = tx.prepare("SELECT id, current_content FROM entries")?;
        let rows = stmt.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?;
        let mut sources = Vec::new();
        for row in rows {
            sources.push(row?);
        }
        drop(stmt);
        for (id, content) in &sources {
            Self::refresh_source_links(tx, id, content)?;
        }
        let (links, unresolved): (i64, i64) = tx.query_row(
            "SELECT COUNT(*), COALESCE(SUM(target_entry_id IS NULL), 0) FROM entry_links",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        let search_index_available = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'entries_fts')",
            [],
            |row| row.get::<_, bool>(0),
        )?;
        Ok(KnowledgeIndexReport {
            indexed_sources: sources.len() as u32,
            link_occurrences: links as u32,
            unresolved_occurrences: unresolved as u32,
            search_index_available,
        })
    }

    pub fn ensure_link_index(conn: &mut Connection) -> AppResult<()> {
        let current = conn
            .query_row(
                "SELECT value FROM settings WHERE key = ?1",
                [LINK_INDEX_VERSION_KEY],
                |row| row.get::<_, String>(0),
            )
            .optional()?;
        if current.as_deref() == Some("1") {
            return Ok(());
        }
        let tx = conn.transaction()?;
        Self::rebuild_links(&tx)?;
        tx.execute(
            "INSERT INTO settings(key, value, updated_at) VALUES (?1, '1', ?2)
             ON CONFLICT(key) DO UPDATE SET value = '1', updated_at = ?2",
            params![LINK_INDEX_VERSION_KEY, now_string()],
        )?;
        tx.commit()?;
        Ok(())
    }
}

fn collect_suggestions(
    rows: impl Iterator<Item = rusqlite::Result<KnowledgeSuggestion>>,
) -> AppResult<Vec<KnowledgeSuggestion>> {
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(Into::into)
}

fn map_suggestion(row: &rusqlite::Row<'_>) -> rusqlite::Result<KnowledgeSuggestion> {
    Ok(KnowledgeSuggestion {
        id: row.get(0)?,
        title: row.get(1)?,
        matched_alias: row.get(2)?,
    })
}

fn validate_title(input: &str) -> AppResult<String> {
    let canonical = canonicalize_knowledge_title(input);
    if canonical.is_empty() {
        return Err(AppError::validation(
            "KNOWLEDGE_TITLE_REQUIRED",
            "知识标题不能为空",
        ));
    }
    if canonical.chars().count() > 200 {
        return Err(AppError::validation(
            "KNOWLEDGE_TITLE_TOO_LONG",
            "知识标题不能超过 200 个字符",
        ));
    }
    if canonical.contains("[[") || canonical.contains("]]") {
        return Err(AppError::validation(
            "KNOWLEDGE_TITLE_INVALID",
            "知识标题不能包含 WikiLink 定界符",
        ));
    }
    Ok(canonical)
}

fn ensure_title_available(tx: &Transaction<'_>, key: &str, entry_id: &str) -> AppResult<()> {
    let current: Option<String> = tx
        .query_row(
            "SELECT id FROM entries WHERE knowledge_state = 'knowledge'
             AND knowledge_title_key = ?1 AND id <> ?2 LIMIT 1",
            params![key, entry_id],
            |row| row.get(0),
        )
        .optional()?;
    let alias: Option<String> = tx
        .query_row(
            "SELECT entry_id FROM entry_aliases WHERE normalized_alias = ?1
             AND entry_id <> ?2 LIMIT 1",
            params![key, entry_id],
            |row| row.get(0),
        )
        .optional()?;
    if current.is_some() || alias.is_some() {
        return Err(AppError::validation(
            "KNOWLEDGE_TITLE_CONFLICT",
            "知识标题已存在",
        ));
    }
    Ok(())
}

fn resolve_target_id(tx: &Transaction<'_>, key: &str) -> AppResult<Option<String>> {
    let current = tx
        .query_row(
            "SELECT id FROM entries WHERE knowledge_state = 'knowledge' AND knowledge_title_key = ?1 LIMIT 1",
            [key],
            |row| row.get(0),
        )
        .optional()?;
    if current.is_some() {
        return Ok(current);
    }
    tx.query_row(
        "SELECT entry_id FROM entry_aliases WHERE normalized_alias = ?1 LIMIT 1",
        [key],
        |row| row.get(0),
    )
    .optional()
    .map_err(Into::into)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        db::{connection::open_in_memory, migrations::now_string, repos::EntriesRepo},
        error::AppError,
        types::entries::EntryPatch,
    };

    #[test]
    fn suggest_matches_current_title_and_alias_but_excludes_trash() {
        let (mut conn, _) = open_in_memory().unwrap();
        let now = now_string();
        let tx = conn.transaction().unwrap();

        let exact = EntriesRepo::create(&tx, "Atlas", &now).unwrap();
        let exact = KnowledgeRepo::promote(&tx, &exact.id, exact.revision, &now).unwrap();

        let current = EntriesRepo::create(&tx, "Current Atlas", &now).unwrap();
        let current = KnowledgeRepo::promote(&tx, &current.id, current.revision, &now).unwrap();

        let alias = EntriesRepo::create(&tx, "Atlas First", &now).unwrap();
        let alias = KnowledgeRepo::promote(&tx, &alias.id, alias.revision, &now).unwrap();
        let alias = EntriesRepo::update(
            &tx,
            &alias.id,
            EntryPatch {
                title: Some("Atlas Second".to_string()),
                current_content: None,
                entry_type: None,
                status: None,
                tags: None,
            },
            alias.revision,
            &now,
        )
        .unwrap();
        let alias = EntriesRepo::update(
            &tx,
            &alias.id,
            EntryPatch {
                title: Some("Canonical Page".to_string()),
                current_content: None,
                entry_type: None,
                status: None,
                tags: None,
            },
            alias.revision,
            &now,
        )
        .unwrap();

        let trashed = EntriesRepo::create(&tx, "Atlas Trash", &now).unwrap();
        let trashed = KnowledgeRepo::promote(&tx, &trashed.id, trashed.revision, &now).unwrap();
        EntriesRepo::move_to_trash(&tx, &trashed.id, trashed.revision, &now).unwrap();
        tx.commit().unwrap();

        let suggestions = KnowledgeRepo::suggest(&conn, " atlas ", 20).unwrap();

        assert_eq!(suggestions.len(), 3);
        assert_eq!(suggestions[0].id, exact.id);
        assert_eq!(suggestions[0].title, "Atlas");
        assert_eq!(suggestions[0].matched_alias, None);
        assert_eq!(suggestions[1].id, current.id);
        assert_eq!(suggestions[1].title, "Current Atlas");
        assert_eq!(suggestions[1].matched_alias, None);
        assert_eq!(suggestions[2].id, alias.id);
        assert_eq!(suggestions[2].title, "Canonical Page");
        assert_eq!(suggestions[2].matched_alias.as_deref(), Some("Atlas First"));
    }

    #[test]
    fn suggest_empty_query_orders_recent_and_clamps_limit() {
        let (mut conn, _) = open_in_memory().unwrap();
        let tx = conn.transaction().unwrap();
        let older = EntriesRepo::create(&tx, "Older", "2026-07-16T00:00:00Z").unwrap();
        KnowledgeRepo::promote(&tx, &older.id, older.revision, "2026-07-16T00:00:00Z").unwrap();
        for index in 0..19 {
            let entry =
                EntriesRepo::create(&tx, &format!("Middle {index}"), "2026-07-16T01:00:00Z")
                    .unwrap();
            KnowledgeRepo::promote(&tx, &entry.id, entry.revision, "2026-07-16T01:00:00Z").unwrap();
        }
        let newest = EntriesRepo::create(&tx, "Newest", "2026-07-16T02:00:00Z").unwrap();
        let newest =
            KnowledgeRepo::promote(&tx, &newest.id, newest.revision, "2026-07-16T02:00:00Z")
                .unwrap();
        tx.commit().unwrap();

        let recent = KnowledgeRepo::suggest(&conn, "", 0).unwrap();
        let limited = KnowledgeRepo::suggest(&conn, "", 100).unwrap();

        assert_eq!(recent.len(), 1);
        assert_eq!(recent[0].id, newest.id);
        assert_eq!(limited.len(), 20);
    }

    #[test]
    fn promote_uses_persisted_title_and_rejects_normalized_conflicts() {
        let (mut conn, _) = open_in_memory().unwrap();
        let now = now_string();
        let tx = conn.transaction().unwrap();
        let first = EntriesRepo::create(&tx, "Same Title", &now).unwrap();
        let second = EntriesRepo::create(&tx, "same   title", &now).unwrap();

        let promoted = KnowledgeRepo::promote(&tx, &first.id, first.revision, &now).unwrap();
        let err = KnowledgeRepo::promote(&tx, &second.id, second.revision, &now).unwrap_err();

        assert_eq!(promoted.title.as_deref(), Some("Same Title"));
        assert!(
            matches!(err, AppError::Validation { code, .. } if code == "KNOWLEDGE_TITLE_CONFLICT")
        );
    }

    #[test]
    fn promote_rejects_titles_that_break_wiki_link_syntax() {
        let (mut conn, _) = open_in_memory().unwrap();
        let now = now_string();
        let tx = conn.transaction().unwrap();
        let entry = EntriesRepo::create(&tx, "Bad ]] Title", &now).unwrap();

        let err = KnowledgeRepo::promote(&tx, &entry.id, entry.revision, &now).unwrap_err();

        assert!(
            matches!(err, AppError::Validation { code, .. } if code == "KNOWLEDGE_TITLE_INVALID")
        );
    }

    #[test]
    fn rebuild_links_is_atomic_and_reports_unresolved_occurrences() {
        let (mut conn, _) = open_in_memory().unwrap();
        let now = now_string();
        let tx = conn.transaction().unwrap();
        let _ = EntriesRepo::create(&tx, "[[不存在]] [[不存在]]", &now).unwrap();
        let report = KnowledgeRepo::rebuild_links(&tx).unwrap();
        assert_eq!(report.indexed_sources, 1);
        assert_eq!(report.link_occurrences, 2);
        assert_eq!(report.unresolved_occurrences, 2);
    }
}
