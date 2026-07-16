use std::collections::HashMap;

use rusqlite::{params, Connection, OptionalExtension, Transaction};

use crate::{
    db::{
        migrations::now_string,
        repos::{
            entries_repo::{entries_fts_is_trigram, entry_summary},
            EntriesRepo,
        },
    },
    error::{AppError, AppResult},
    knowledge::wiki_links::{
        canonicalize_knowledge_title, normalize_knowledge_title, parse_wiki_links,
    },
    types::{
        entries::EntryDetail,
        knowledge::{
            KnowledgeIndexReport, KnowledgeRelations, KnowledgeSuggestion, RelatedEntry,
            UnresolvedWikiLink,
        },
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

    pub fn relations(conn: &Connection, entry_id: &str) -> AppResult<KnowledgeRelations> {
        ensure_link_index_ready(conn)?;
        let exists = conn
            .query_row(
                "SELECT 1 FROM entries WHERE id = ?1",
                [entry_id],
                |_| Ok(()),
            )
            .optional()?
            .is_some();
        if !exists {
            return Err(AppError::not_found("条目不存在"));
        }

        let mut stmt = conn.prepare(
            "WITH grouped AS (
                 SELECT target_entry_id, COUNT(*) AS occurrence_count,
                        MIN(ordinal) AS first_ordinal
                 FROM entry_links
                 WHERE source_entry_id = ?1 AND target_entry_id IS NOT NULL
                 GROUP BY target_entry_id
             )
             SELECT e.id, e.title, e.current_content, grouped.occurrence_count, e.deleted_at
             FROM grouped
             JOIN entries e ON e.id = grouped.target_entry_id
             ORDER BY grouped.first_ordinal, e.id",
        )?;
        let outgoing = stmt
            .query_map([entry_id], |row| {
                let content: String = row.get(2)?;
                Ok(RelatedEntry {
                    id: row.get(0)?,
                    title: row.get(1)?,
                    summary: entry_summary(&content),
                    occurrence_count: row.get(3)?,
                    deleted_at: row.get(4)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;

        let mut stmt = conn.prepare(
            "WITH grouped AS (
                 SELECT source_entry_id, COUNT(*) AS occurrence_count
                 FROM entry_links
                 WHERE target_entry_id = ?1
                 GROUP BY source_entry_id
             )
             SELECT e.id, e.title, e.current_content, grouped.occurrence_count, e.deleted_at
             FROM grouped
             JOIN entries e ON e.id = grouped.source_entry_id
             ORDER BY e.updated_at DESC, e.id",
        )?;
        let backlinks = stmt
            .query_map([entry_id], |row| {
                let content: String = row.get(2)?;
                Ok(RelatedEntry {
                    id: row.get(0)?,
                    title: row.get(1)?,
                    summary: entry_summary(&content),
                    occurrence_count: row.get(3)?,
                    deleted_at: row.get(4)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;

        let mut stmt = conn.prepare(
            "SELECT raw_target, COUNT(*) AS occurrence_count, MIN(ordinal) AS first_ordinal
             FROM entry_links
             WHERE source_entry_id = ?1 AND target_entry_id IS NULL
             GROUP BY normalized_target, raw_target
             ORDER BY first_ordinal, normalized_target, raw_target",
        )?;
        let unresolved = stmt
            .query_map([entry_id], |row| {
                Ok(UnresolvedWikiLink {
                    raw_target: row.get(0)?,
                    occurrence_count: row.get(1)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;

        Ok(KnowledgeRelations {
            outgoing,
            backlinks,
            unresolved,
        })
    }

    pub fn rebuild_all_indexes(conn: &mut Connection) -> AppResult<KnowledgeIndexReport> {
        match Self::rebuild_all_indexes_inner(conn) {
            Ok(report) => Ok(report),
            Err(err) => {
                log::error!("knowledge_index_rebuild_failed source={err}");
                Err(AppError::system(
                    "KNOWLEDGE_INDEX_REBUILD_FAILED",
                    "知识索引重建失败",
                ))
            }
        }
    }

    fn rebuild_all_indexes_inner(conn: &mut Connection) -> AppResult<KnowledgeIndexReport> {
        let search_index_available = entries_fts_is_trigram(conn)?;
        let tx = conn.transaction()?;
        let mut report = Self::rebuild_links(&tx)?;
        if search_index_available {
            rebuild_fts(&tx)?;
        }
        report.search_index_available = search_index_available;
        tx.execute(
            "INSERT INTO settings(key, value, updated_at) VALUES (?1, '1', ?2)
             ON CONFLICT(key) DO UPDATE SET value = '1', updated_at = ?2",
            params![LINK_INDEX_VERSION_KEY, now_string()],
        )?;
        tx.commit()?;
        Ok(report)
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
        let search_index_available = entries_fts_is_trigram(tx)?;
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
        Self::rebuild_all_indexes(conn).map(|_| ())
    }
}

fn ensure_link_index_ready(conn: &Connection) -> AppResult<()> {
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
    Err(AppError::system(
        "KNOWLEDGE_INDEX_REBUILD_FAILED",
        "知识索引未就绪，请重建索引",
    ))
}

fn rebuild_fts(tx: &Transaction<'_>) -> AppResult<()> {
    tx.execute("DELETE FROM entries_fts", [])?;
    tx.execute(
        "INSERT INTO entries_fts(
             entry_id, title, original_content, current_content, tags_text, aliases_text
         )
         SELECT
             e.id,
             COALESCE(e.title, ''),
             e.original_content,
             e.current_content,
             (
                 SELECT COALESCE(GROUP_CONCAT(t.name, ' '), '')
                 FROM entry_tags et
                 JOIN tags t ON t.id = et.tag_id
                 WHERE et.entry_id = e.id
             ),
             (
                 SELECT COALESCE(GROUP_CONCAT(ea.alias, ' '), '')
                 FROM entry_aliases ea
                 WHERE ea.entry_id = e.id
             )
         FROM entries e",
        [],
    )?;
    Ok(())
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
        types::{
            entries::{EntryPatch, EntryStatus},
            knowledge::KnowledgeState,
        },
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
    fn relations_group_occurrences_and_keep_unresolved_links() {
        let (mut conn, _) = open_in_memory().unwrap();
        let now = now_string();
        let tx = conn.transaction().unwrap();
        let target = EntriesRepo::create(&tx, "目标", &now).unwrap();
        let target = KnowledgeRepo::promote(&tx, &target.id, target.revision, &now).unwrap();
        let source = EntriesRepo::create(
            &tx,
            "[[目标]] 再次 [[目标]] 以及 [[缺失]] [[缺失]] [[MISSING]] [[missing]]",
            &now,
        )
        .unwrap();
        let target = EntriesRepo::move_to_trash(&tx, &target.id, target.revision, &now).unwrap();
        tx.commit().unwrap();

        let target_relations = KnowledgeRepo::relations(&conn, &target.id).unwrap();
        assert_eq!(target_relations.backlinks.len(), 1);
        assert_eq!(target_relations.backlinks[0].id, source.id);
        assert_eq!(target_relations.backlinks[0].occurrence_count, 2);

        let source_relations = KnowledgeRepo::relations(&conn, &source.id).unwrap();
        assert_eq!(source_relations.outgoing.len(), 1);
        assert_eq!(source_relations.outgoing[0].id, target.id);
        assert_eq!(source_relations.outgoing[0].occurrence_count, 2);
        assert_eq!(source_relations.outgoing[0].deleted_at, target.deleted_at);
        assert_eq!(source_relations.unresolved.len(), 3);
        assert_eq!(source_relations.unresolved[0].raw_target, "缺失");
        assert_eq!(source_relations.unresolved[0].occurrence_count, 2);
        assert_eq!(source_relations.unresolved[1].raw_target, "MISSING");
        assert_eq!(source_relations.unresolved[2].raw_target, "missing");
    }

    #[test]
    fn relations_return_not_found_for_missing_entry() {
        let (conn, _) = open_in_memory().unwrap();

        let err = KnowledgeRepo::relations(&conn, "missing").unwrap_err();

        assert!(matches!(err, AppError::NotFound { .. }));
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

    #[test]
    fn rebuild_all_indexes_does_not_change_entry_rows() {
        let (mut conn, _) = open_in_memory().unwrap();
        let now = now_string();
        let tx = conn.transaction().unwrap();
        let target = EntriesRepo::create(&tx, "目标", &now).unwrap();
        let target = KnowledgeRepo::promote(&tx, &target.id, target.revision, &now).unwrap();
        let source = EntriesRepo::create(&tx, "[[目标]] 正文", &now).unwrap();
        EntriesRepo::update(
            &tx,
            &source.id,
            EntryPatch {
                title: Some("来源".to_string()),
                current_content: None,
                entry_type: None,
                status: Some(EntryStatus::Done),
                tags: None,
            },
            source.revision,
            &now,
        )
        .unwrap();
        tx.commit().unwrap();

        let before = entry_rows(&conn);
        let report = KnowledgeRepo::rebuild_all_indexes(&mut conn).unwrap();
        let after = entry_rows(&conn);

        assert_eq!(before, after);
        assert_eq!(report.indexed_sources, 2);
        assert_eq!(report.link_occurrences, 1);
        assert_eq!(target.knowledge_state, KnowledgeState::Knowledge);
    }

    #[test]
    fn relations_report_index_not_ready_instead_of_silent_empty() {
        let (mut conn, _) = open_in_memory().unwrap();
        let tx = conn.transaction().unwrap();
        let entry = EntriesRepo::create(&tx, "正文", &now_string()).unwrap();
        tx.commit().unwrap();
        conn.execute(
            "DELETE FROM settings WHERE key = ?1",
            [LINK_INDEX_VERSION_KEY],
        )
        .unwrap();

        let err = KnowledgeRepo::relations(&conn, &entry.id).unwrap_err();

        assert!(matches!(
            err,
            AppError::System { code, .. } if code == "KNOWLEDGE_INDEX_REBUILD_FAILED"
        ));
    }

    #[test]
    fn rebuild_all_indexes_failure_does_not_write_version_key() {
        let (mut conn, _) = open_in_memory().unwrap();
        conn.execute(
            "DELETE FROM settings WHERE key = ?1",
            [LINK_INDEX_VERSION_KEY],
        )
        .unwrap();
        conn.execute("DROP TABLE entry_links", []).unwrap();

        let err = KnowledgeRepo::rebuild_all_indexes(&mut conn).unwrap_err();

        assert!(matches!(
            err,
            AppError::System { code, .. } if code == "KNOWLEDGE_INDEX_REBUILD_FAILED"
        ));
        let key: Option<String> = conn
            .query_row(
                "SELECT value FROM settings WHERE key = ?1",
                [LINK_INDEX_VERSION_KEY],
                |row| row.get(0),
            )
            .optional()
            .unwrap();
        assert_eq!(key, None);
    }

    #[test]
    fn rebuild_all_indexes_does_not_write_legacy_fts() {
        let (mut conn, _) = open_in_memory().unwrap();
        let tx = conn.transaction().unwrap();
        EntriesRepo::create(&tx, "legacy search", &now_string()).unwrap();
        tx.commit().unwrap();
        conn.execute_batch(
            "DROP TRIGGER IF EXISTS entries_fts_entries_insert;
             DROP TRIGGER IF EXISTS entries_fts_entries_update;
             DROP TRIGGER IF EXISTS entries_fts_entries_delete;
             DROP TRIGGER IF EXISTS entries_fts_entry_tags_insert;
             DROP TRIGGER IF EXISTS entries_fts_entry_tags_delete;
             DROP TRIGGER IF EXISTS entries_fts_tags_update;
             DROP TRIGGER IF EXISTS entries_fts_entry_aliases_insert;
             DROP TRIGGER IF EXISTS entries_fts_entry_aliases_delete;
             DROP TABLE entries_fts;
             CREATE VIRTUAL TABLE entries_fts USING fts5(
               entry_id UNINDEXED, title, original_content, current_content, tags_text
             );
             INSERT INTO entries_fts(entry_id, title, original_content, current_content, tags_text)
             VALUES ('sentinel', 'keep', 'keep', 'keep', 'keep');",
        )
        .unwrap();

        let report = KnowledgeRepo::rebuild_all_indexes(&mut conn).unwrap();

        assert!(!report.search_index_available);
        let sentinel_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM entries_fts WHERE entry_id = 'sentinel'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(sentinel_count, 1);
    }

    fn entry_rows(conn: &Connection) -> Vec<Vec<rusqlite::types::Value>> {
        let mut stmt = conn
            .prepare(
                "SELECT id, title, title_source, original_content, current_content,
                        type, status, revision, created_at, updated_at, deleted_at,
                        knowledge_state, knowledge_promoted_at, knowledge_title_key
                 FROM entries ORDER BY id",
            )
            .unwrap();
        stmt.query_map([], |row| (0..14).map(|index| row.get(index)).collect())
            .unwrap()
            .collect::<rusqlite::Result<Vec<_>>>()
            .unwrap()
    }
}
