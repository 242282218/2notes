use std::collections::HashMap;

use rusqlite::{params, Connection, OptionalExtension, Transaction};

use crate::{
    db::{
        migrations::now_string,
        repos::{
            documents_repo::DocumentsRepo,
            entries_repo::{can_fallback_from_fts, entries_fts_is_trigram, entry_summary},
            EntriesRepo, HierarchyRepo,
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

        // The trigram FTS index covers title/aliases and scales far better than a
        // leading-wildcard LIKE scan once the knowledge base grows. Short or
        // non-alphanumeric queries (common for 1-2 char CJK prefixes) keep the
        // LIKE path, mirroring the main search's rule.
        let use_fts = normalized.chars().count() >= 3
            && normalized.chars().all(char::is_alphanumeric)
            && entries_fts_is_trigram(conn)?;
        if use_fts {
            match suggest_fts(conn, &normalized, limit) {
                Ok(suggestions) => return Ok(suggestions),
                Err(err) if can_fallback_from_fts(&err) => {}
                Err(err) => return Err(err),
            }
        }
        suggest_like(conn, &normalized, limit)
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
        // Also verify that `entry_documents` rows have a consistent `blocks` projection and
        // repair any drift in the same transaction. FTS above was already regenerated from
        // `entries.current_content`, so a rebuild cannot desync the search index.
        let (_documents, projected_blocks, repaired_documents) =
            DocumentsRepo::verify_and_repair_projections(&tx)?;
        report.projected_blocks = projected_blocks;
        report.repaired_documents = repaired_documents;
        HierarchyRepo::repair(&tx, &now_string())?;
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
        HierarchyRepo::insert_root(tx, id, now)?;
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
        HierarchyRepo::assert_demotable(tx, id)?;
        HierarchyRepo::remove_entry(tx, id)?;
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
        Self::apply_title_change(tx, entry_id, old_title, new_title, now, true)
    }

    /// Restoring a knowledge entry can collide with a live entry that claimed its
    /// title while it sat in the trash, because trashed entries deliberately do not
    /// reserve their key. Dropping the key would leave `knowledge_state = 'knowledge'`
    /// with a NULL key, which the rest of the code treats as impossible, and which
    /// makes the entry uneditable because every later rename re-checks availability.
    /// Rename to the first free "<title> (n)" instead.
    ///
    /// Returns whether a rename happened.
    pub(crate) fn resolve_restore_title_conflict(
        tx: &Transaction<'_>,
        entry_id: &str,
        title: &str,
        key: &str,
        now: &str,
    ) -> AppResult<bool> {
        if title_is_available(tx, key, entry_id)? {
            return Ok(false);
        }
        let renamed = disambiguated_title(tx, title, entry_id)?;
        // The previous title now belongs to the live entry that claimed it, so it must
        // not be recorded as an alias of this one.
        Self::apply_title_change(tx, entry_id, title, &renamed, now, false)?;
        log::info!("knowledge_title_renamed_on_restore id={entry_id} key={key} title={renamed}");
        Ok(true)
    }

    fn apply_title_change(
        tx: &Transaction<'_>,
        entry_id: &str,
        old_title: &str,
        new_title: &str,
        now: &str,
        record_alias: bool,
    ) -> AppResult<()> {
        let canonical = validate_title(new_title)?;
        let old_key = normalize_knowledge_title(old_title);
        let new_key = normalize_knowledge_title(&canonical);
        ensure_title_available(tx, &new_key, entry_id)?;
        tx.execute(
            "DELETE FROM entry_aliases WHERE normalized_alias = ?1 AND entry_id = ?2",
            params![new_key, entry_id],
        )?;
        if record_alias && old_key != new_key {
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

    /// Rebuild outgoing links for `source_id`. The recall source is the stored
    /// `DocumentRecord.markdown_text`, which preserves `[[wiki]]` as plain text. A missing
    /// document row falls back to `entries.current_content` so capture entries written before
    /// schema 005 do not silently lose their links.
    pub fn refresh_source_links(
        tx: &Transaction<'_>,
        source_id: &str,
        current_content: &str,
    ) -> AppResult<()> {
        let source_text = match DocumentsRepo::markdown_text_with_tx(tx, source_id)? {
            Some(markdown) => markdown,
            None => current_content.to_string(),
        };
        tx.execute(
            "DELETE FROM entry_links WHERE source_entry_id = ?1",
            [source_id],
        )?;
        for (ordinal, link) in parse_wiki_links(&source_text).into_iter().enumerate() {
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
        if entry_ids.is_empty() {
            return Ok(HashMap::new());
        }

        let mut aliases = HashMap::new();
        let sql = aliases_for_entries_sql(entry_ids.len());
        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt.query_map(rusqlite::params_from_iter(entry_ids.iter()), |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?;
        for row in rows {
            let (entry_id, alias) = row?;
            aliases.entry(entry_id).or_insert_with(Vec::new).push(alias);
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
            // Populated by `rebuild_all_indexes_inner` after projection verification; zero
            // when this builder is called directly outside a full rebuild.
            projected_blocks: 0,
            repaired_documents: 0,
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

fn aliases_for_entries_sql(id_count: usize) -> String {
    let placeholders = (1..=id_count)
        .map(|index| format!("?{index}"))
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        "SELECT entry_id, alias FROM entry_aliases WHERE entry_id IN ({placeholders}) ORDER BY created_at, normalized_alias"
    )
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

/// Suggest via the trigram FTS index restricted to title/aliases_text columns.
/// Exact-key matches are pinned first, then bm25 ranking breaks ties.
fn suggest_fts(
    conn: &Connection,
    normalized: &str,
    limit: i64,
) -> AppResult<Vec<KnowledgeSuggestion>> {
    let match_query = format!(
        "{{title aliases_text}} : {}",
        super::entries_repo::fts_phrase(normalized)
    );
    let pattern = format!("%{}%", super::entries_repo::escape_like(normalized));
    let mut stmt = conn.prepare(
        "
        WITH ranked AS MATERIALIZED (
          SELECT entries_fts.rowid AS search_rowid, e.id,
                 entries_fts.rank AS score
          FROM entries_fts
          JOIN entries e ON e.id = entries_fts.entry_id
          WHERE e.knowledge_state = 'knowledge'
            AND e.deleted_at IS NULL
            AND entries_fts MATCH ?1
            AND entries_fts.rank MATCH 'bm25(0.0, 10.0, 1.0, 4.0, 3.0, 6.0)'
          ORDER BY CASE
                       WHEN e.knowledge_title_key = ?2 THEN 0
                       ELSE 1
                   END,
                   score ASC,
                   e.updated_at DESC,
                   e.id ASC
          LIMIT ?3
        )
        SELECT e.id, e.title,
               CASE
                   WHEN e.knowledge_title_key LIKE ?4 ESCAPE '\\' THEN NULL
                   ELSE (
                       SELECT ea.alias
                       FROM entry_aliases ea
                       WHERE ea.entry_id = e.id
                         AND ea.normalized_alias LIKE ?4 ESCAPE '\\'
                       ORDER BY ea.normalized_alias
                       LIMIT 1
                   )
               END AS matched_alias
        FROM ranked
        JOIN entries e ON e.id = ranked.id
        ORDER BY CASE
                     WHEN e.knowledge_title_key = ?2 THEN 0
                     ELSE 1
                 END,
                 ranked.score ASC,
                 e.title COLLATE NOCASE,
                 e.id ASC
        ",
    )?;
    let rows = stmt.query_map(
        params![match_query, normalized, limit, pattern],
        map_suggestion,
    )?;
    collect_suggestions(rows)
}

/// Leading-wildcard LIKE fallback used for short/non-alphanumeric queries or
/// when the FTS index is unavailable.
fn suggest_like(
    conn: &Connection,
    normalized: &str,
    limit: i64,
) -> AppResult<Vec<KnowledgeSuggestion>> {
    let pattern = format!("%{}%", super::entries_repo::escape_like(normalized));
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
    let rows = stmt.query_map(params![normalized, pattern, limit], map_suggestion)?;
    collect_suggestions(rows)
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
    if title_is_available(tx, key, entry_id)? {
        Ok(())
    } else {
        Err(AppError::validation(
            "KNOWLEDGE_TITLE_CONFLICT",
            "知识标题已存在",
        ))
    }
}

fn title_is_available(tx: &Transaction<'_>, key: &str, entry_id: &str) -> AppResult<bool> {
    // Trashed entries must not reserve their title key: soft-deleted knowledge entries
    // cannot be edited, so the key would be permanently burned until a force delete.
    let current: Option<String> = tx
        .query_row(
            "SELECT id FROM entries WHERE knowledge_state = 'knowledge'
             AND knowledge_title_key = ?1 AND id <> ?2 AND deleted_at IS NULL LIMIT 1",
            params![key, entry_id],
            |row| row.get(0),
        )
        .optional()?;
    if current.is_some() {
        return Ok(false);
    }
    let alias: Option<String> = tx
        .query_row(
            "SELECT ea.entry_id FROM entry_aliases ea
             JOIN entries e ON e.id = ea.entry_id AND e.deleted_at IS NULL
             WHERE ea.normalized_alias = ?1 AND ea.entry_id <> ?2 LIMIT 1",
            params![key, entry_id],
            |row| row.get(0),
        )
        .optional()?;
    Ok(alias.is_none())
}

/// `validate_title` rejects titles longer than this; disambiguation must stay under it.
const MAX_KNOWLEDGE_TITLE_CHARS: usize = 200;

/// First free "<title> (n)" for a title that a live entry already holds.
fn disambiguated_title(tx: &Transaction<'_>, title: &str, entry_id: &str) -> AppResult<String> {
    for suffix in 2..=99u32 {
        let tail = format!(" ({suffix})");
        let budget = MAX_KNOWLEDGE_TITLE_CHARS.saturating_sub(tail.chars().count());
        let base: String = title.chars().take(budget).collect();
        let candidate = format!("{}{}", base.trim_end(), tail);
        // Truncation can expose a delimiter that the original title never had, so the
        // candidate has to pass the same validation as any other title.
        let Ok(canonical) = validate_title(&candidate) else {
            continue;
        };
        if title_is_available(tx, &normalize_knowledge_title(&canonical), entry_id)? {
            return Ok(canonical);
        }
    }
    Err(AppError::validation(
        "KNOWLEDGE_TITLE_CONFLICT",
        "恢复时无法为该条目分配可用的知识标题",
    ))
}

fn resolve_target_id(tx: &Transaction<'_>, key: &str) -> AppResult<Option<String>> {
    let current = tx
        .query_row(
            "SELECT id FROM entries WHERE knowledge_state = 'knowledge'
             AND knowledge_title_key = ?1 AND deleted_at IS NULL LIMIT 1",
            [key],
            |row| row.get(0),
        )
        .optional()?;
    if current.is_some() {
        return Ok(current);
    }
    tx.query_row(
        "SELECT ea.entry_id FROM entry_aliases ea
         JOIN entries e ON e.id = ea.entry_id AND e.deleted_at IS NULL
         WHERE ea.normalized_alias = ?1 LIMIT 1",
        [key],
        |row| row.get(0),
    )
    .optional()
    .map_err(Into::into)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    use crate::{
        db::{
            connection::{open_database, open_in_memory},
            migrations::now_string,
            repos::{documents_repo::DocumentsRepo, EntriesRepo},
        },
        error::AppError,
        types::{
            documents::{BlockAttrs, BlockDocument, BlockKind, BlockNode, InlineMark, InlineNode},
            entries::{
                CreateEntrySpec, EntryListFilter, EntryPage, EntryPatch, EntryStatus, EntryType,
                PageRequest, TitleSource,
            },
            knowledge::KnowledgeState,
        },
    };

    #[test]
    fn aliases_for_entries_sql_scopes_to_requested_ids() {
        assert_eq!(
            aliases_for_entries_sql(0),
            "SELECT entry_id, alias FROM entry_aliases WHERE entry_id IN () ORDER BY created_at, normalized_alias"
        );
        assert_eq!(
            aliases_for_entries_sql(1),
            "SELECT entry_id, alias FROM entry_aliases WHERE entry_id IN (?1) ORDER BY created_at, normalized_alias"
        );
        assert_eq!(
            aliases_for_entries_sql(3),
            "SELECT entry_id, alias FROM entry_aliases WHERE entry_id IN (?1, ?2, ?3) ORDER BY created_at, normalized_alias"
        );

        let sql = aliases_for_entries_sql(2);
        assert!(sql.contains("WHERE entry_id IN (?1, ?2)"));
        assert_eq!(sql.matches('?').count(), 2);
    }

    #[test]
    fn aliases_for_entries_returns_only_requested_entry_aliases() {
        let (mut conn, _) = open_in_memory().unwrap();
        let now = now_string();
        let tx = conn.transaction().unwrap();
        let target = EntriesRepo::create(&tx, "Target Entry", &now).unwrap();
        let other = EntriesRepo::create(&tx, "Other Entry", &now).unwrap();
        tx.commit().unwrap();

        for index in 0..50 {
            conn.execute(
                "INSERT INTO entry_aliases(normalized_alias, entry_id, alias, created_at)
                 VALUES (?1, ?2, ?3, ?4)",
                params![
                    format!("target-alias-{index}"),
                    target.id,
                    format!("Target Alias {index}"),
                    now
                ],
            )
            .unwrap();
            conn.execute(
                "INSERT INTO entry_aliases(normalized_alias, entry_id, alias, created_at)
                 VALUES (?1, ?2, ?3, ?4)",
                params![
                    format!("other-alias-{index}"),
                    other.id,
                    format!("Other Alias {index}"),
                    now
                ],
            )
            .unwrap();
        }

        let empty = KnowledgeRepo::aliases_for_entries(&conn, &[]).unwrap();
        assert!(empty.is_empty());

        let aliases =
            KnowledgeRepo::aliases_for_entries(&conn, std::slice::from_ref(&target.id)).unwrap();
        assert_eq!(aliases.len(), 1);
        let target_aliases = aliases.get(&target.id).expect("target aliases present");
        assert_eq!(target_aliases.len(), 50);
        assert!(target_aliases
            .iter()
            .all(|alias| alias.starts_with("Target Alias ")));
        assert!(!aliases.contains_key(&other.id));

        let both =
            KnowledgeRepo::aliases_for_entries(&conn, &[target.id.clone(), other.id.clone()])
                .unwrap();
        assert_eq!(both.len(), 2);
        assert_eq!(both.get(&target.id).map(Vec::len), Some(50));
        assert_eq!(both.get(&other.id).map(Vec::len), Some(50));
    }

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
                document: None,
                title: Some("Atlas Second".to_string()),
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
                document: None,
                title: Some("Canonical Page".to_string()),
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
    fn suggest_matches_cjk_substrings_via_fts_and_short_queries_via_like() {
        let (mut conn, _) = open_in_memory().unwrap();
        let now = now_string();
        let tx = conn.transaction().unwrap();
        let entry = EntriesRepo::create(&tx, "知识库管理实践", &now).unwrap();
        KnowledgeRepo::promote(&tx, &entry.id, entry.revision, &now).unwrap();
        tx.commit().unwrap();

        // Long CJK substring takes the trigram FTS path and still matches.
        let fts = KnowledgeRepo::suggest(&conn, "知识库管", 20).unwrap();
        assert_eq!(fts.len(), 1);
        assert_eq!(fts[0].id, entry.id);

        // 2-char CJK prefix falls back to LIKE and still matches.
        let like = KnowledgeRepo::suggest(&conn, "知识", 20).unwrap();
        assert_eq!(like.len(), 1);
        assert_eq!(like[0].id, entry.id);
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
                document: None,
                title: Some("来源".to_string()),
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
    fn structured_document_drives_wikilink_search_export_and_projection() {
        use crate::{content::document::document_to_markdown, files::markdown::export_entries};
        use tempfile::tempdir;

        let (mut conn, _) = open_in_memory().unwrap();
        let now = now_string();

        // Build a structured document with heading, bullet list, marks, a hard break, and a
        // [[目标]] WikiLink embedded as plain text in a paragraph.
        let target = {
            let tx = conn.transaction().unwrap();
            let created = EntriesRepo::create(&tx, "目标", &now).unwrap();
            let promoted =
                KnowledgeRepo::promote(&tx, &created.id, created.revision, &now).unwrap();
            tx.commit().unwrap();
            promoted
        };

        // Manually craft the source document so we exercise prose beyond a single paragraph.
        let block_uuid = |seed: usize| format!("00000000-0000-4000-8000-{seed:012x}");
        let heading = BlockNode {
            id: block_uuid(1),
            kind: BlockKind::Heading,
            attrs: BlockAttrs {
                level: Some(2),
                language: None,
                start: None,
            },
            content: vec![InlineNode::Text {
                text: "笔记标题".to_string(),
                marks: Vec::new(),
            }],
            children: Vec::new(),
        };
        let intro = BlockNode {
            id: block_uuid(2),
            kind: BlockKind::Paragraph,
            attrs: BlockAttrs::default(),
            content: vec![
                InlineNode::Text {
                    text: "粗体".to_string(),
                    marks: vec![InlineMark::Bold],
                },
                InlineNode::Text {
                    text: "与普通文字和".to_string(),
                    marks: Vec::new(),
                },
                InlineNode::HardBreak,
                InlineNode::Text {
                    text: "随后硬换行并提及 [[目标]] 收尾。".to_string(),
                    marks: Vec::new(),
                },
            ],
            children: Vec::new(),
        };
        let list_item_a = BlockNode {
            id: block_uuid(3),
            kind: BlockKind::ListItem,
            attrs: BlockAttrs::default(),
            content: vec![InlineNode::Text {
                text: "列表项带 关键字 alpha".to_string(),
                marks: Vec::new(),
            }],
            children: Vec::new(),
        };
        let list_item_b = BlockNode {
            id: block_uuid(4),
            kind: BlockKind::ListItem,
            attrs: BlockAttrs::default(),
            content: vec![InlineNode::Text {
                text: "列表项带 其它术语".to_string(),
                marks: Vec::new(),
            }],
            children: Vec::new(),
        };
        let bullet = BlockNode {
            id: block_uuid(5),
            kind: BlockKind::BulletList,
            attrs: BlockAttrs::default(),
            content: Vec::new(),
            children: vec![list_item_a, list_item_b],
        };
        let document = BlockDocument::from_blocks(vec![heading, intro, bullet]);

        // Persist the source entry whose document carries WikiLink + structured content.
        let original_content =
            "笔记标题\n粗体与普通文字和\n随后硬换行并提及 [[目标]] 收尾。\n- 列表项带 关键字 alpha\n- 列表项带 其它术语"
                .to_string();
        let source = {
            let spec = CreateEntrySpec {
                title: Some("来源笔记".to_string()),
                title_source: TitleSource::User,
                original_content: original_content.clone(),
                document,
                entry_type: EntryType::Idea,
                status: EntryStatus::Pending,
                tags: Vec::new(),
            };
            let tx = conn.transaction().unwrap();
            let entry = EntriesRepo::create_with_document(&tx, spec, &now).unwrap();
            tx.commit().unwrap();
            entry
        };

        // (a) Resolved outgoing link: refresh_source_links should resolve [[目标]] to target.
        let relations = KnowledgeRepo::relations(&conn, &source.id).unwrap();
        assert_eq!(relations.outgoing.len(), 1);
        assert_eq!(relations.outgoing[0].id, target.id);
        assert_eq!(relations.outgoing[0].occurrence_count, 1);
        assert!(relations.unresolved.is_empty());

        // Reach for the stored `markdown_text` to confirm WikiLink survives serialization.
        let markdown = DocumentsRepo::markdown_text(&conn, &source.id)
            .unwrap()
            .expect("expected stored markdown_text for source entry");
        assert!(markdown.contains("[[目标]]"));
        assert!(markdown.starts_with("## 笔记标题"));

        // (b) Keyword search via FTS indexes the derived plain text including the heading
        // and list item content.
        let page = EntriesRepo::list(
            &conn,
            &EntryListFilter {
                query: Some("关键字".to_string()),
                entry_type: None,
                status: None,
                knowledge_state: None,
                tag: None,
                include_deleted: false,
                trash_only: false,
            },
            &PageRequest {
                limit: Some(10),
                offset: Some(0),
            },
        )
        .unwrap();
        assert!(
            matches!(page, EntryPage { items, .. } if items.iter().any(|item| item.id == source.id))
        );

        // (c) Markdown export renders heading, list, marks, and keeps the WikiLink cleanup.
        let detail = EntriesRepo::get(&conn, &source.id).unwrap();
        let export_body = document_to_markdown(&detail.document).unwrap();
        assert!(export_body.contains("## 笔记标题"));
        assert!(export_body.contains("- 列表项带 关键字 alpha"));
        assert!(export_body.contains("[[目标]]"));
        assert!(!export_body.contains(r"\[[目标]]"));
        let dir = tempdir().unwrap();
        let paths = export_entries(std::slice::from_ref(&detail), dir.path()).unwrap();
        let exported = std::fs::read_to_string(&paths[0]).unwrap();
        assert!(exported.contains("## 笔记标题"));
        assert!(exported.contains("- 列表项带 关键字 alpha"));
        assert!(exported.contains("[[目标]]"));
        assert!(!exported.contains(r"\[[目标]]"));

        // (d) Simulate projection drift: nuke blocks rows, then rebuild must repair them.
        conn.execute("DELETE FROM blocks WHERE entry_id = ?1", [&source.id])
            .unwrap();
        let blocks_before: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM blocks WHERE entry_id = ?1",
                [&source.id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(blocks_before, 0);

        let report = KnowledgeRepo::rebuild_all_indexes(&mut conn).unwrap();
        assert_eq!(report.repaired_documents, 1);
        // Source contributes 5 blocks (heading + intro + bullet + 2 list items); target
        // contributes 1 (a single legacy paragraph), so the verified total is 6.
        assert_eq!(report.projected_blocks, 6);
        let blocks_after: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM blocks WHERE entry_id = ?1",
                [&source.id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(blocks_after, 5);

        // Link integrity preserved across projection repair.
        let relations_after = KnowledgeRepo::relations(&conn, &source.id).unwrap();
        assert_eq!(relations_after.outgoing.len(), 1);
        assert_eq!(relations_after.outgoing[0].id, target.id);
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

    #[test]
    #[ignore = "10k on-disk scale verification"]
    fn knowledge_scale_10000_entries() {
        let temp = tempfile::tempdir().unwrap();
        let database_path = temp.path().join("2notes.sqlite");
        let (mut write_conn, read_conn) = open_database(&database_path).unwrap();
        let journal_mode: String = write_conn
            .query_row("PRAGMA journal_mode", [], |row| row.get(0))
            .unwrap();
        assert_eq!(journal_mode, "wal");

        let now = now_string();
        let body = "知识库性能验证正文".repeat(55);
        let tx = write_conn.transaction().unwrap();
        for index in 0..10_000 {
            let content =
                format!("{body} 条目{index} [[知识标题0]] [[知识标题1]] [[缺失目标{index}]]");
            let entry = EntriesRepo::create(&tx, &content, &now).unwrap();
            if index < 100 {
                let titled = EntriesRepo::update(
                    &tx,
                    &entry.id,
                    EntryPatch {
                        document: None,
                        title: Some(format!("知识标题{index}")),
                        entry_type: None,
                        status: None,
                        tags: None,
                    },
                    entry.revision,
                    &now,
                )
                .unwrap();
                KnowledgeRepo::promote(&tx, &titled.id, titled.revision, &now).unwrap();
            }
        }
        tx.commit().unwrap();

        let rebuild_started = Instant::now();
        let report = KnowledgeRepo::rebuild_all_indexes(&mut write_conn).unwrap();
        let rebuild_elapsed = rebuild_started.elapsed();
        assert_eq!(report.indexed_sources, 10_000);
        assert_eq!(report.link_occurrences, 30_000);

        let page = PageRequest {
            limit: Some(50),
            offset: Some(0),
        };
        let search_filter = scale_filter(Some("知识库"), None);
        let knowledge_filter = scale_filter(None, Some(KnowledgeState::Knowledge));
        assert!(!EntriesRepo::list(&read_conn, &search_filter, &page)
            .unwrap()
            .items
            .is_empty());
        assert_eq!(
            EntriesRepo::list(&read_conn, &knowledge_filter, &page)
                .unwrap()
                .items
                .len(),
            50
        );

        let mut search_samples = Vec::with_capacity(20);
        let mut knowledge_list_samples = Vec::with_capacity(20);
        for _ in 0..20 {
            let started = Instant::now();
            let result = EntriesRepo::list(&read_conn, &search_filter, &page).unwrap();
            search_samples.push(started.elapsed());
            assert!(!result.items.is_empty());

            let started = Instant::now();
            let result = EntriesRepo::list(&read_conn, &knowledge_filter, &page).unwrap();
            knowledge_list_samples.push(started.elapsed());
            assert_eq!(result.items.len(), 50);
        }
        let search_p95 = p95(&mut search_samples);
        let knowledge_list_p95 = p95(&mut knowledge_list_samples);
        eprintln!(
            "knowledge_scale_10000_entries rebuild_ms={:.2} search_p95_ms={:.2} knowledge_list_p95_ms={:.2}",
            rebuild_elapsed.as_secs_f64() * 1000.0,
            search_p95.as_secs_f64() * 1000.0,
            knowledge_list_p95.as_secs_f64() * 1000.0,
        );

        if cfg!(not(debug_assertions)) {
            assert!(search_p95 < Duration::from_millis(300));
            assert!(knowledge_list_p95 < Duration::from_millis(300));
        }
    }

    fn scale_filter(
        query: Option<&str>,
        knowledge_state: Option<KnowledgeState>,
    ) -> EntryListFilter {
        EntryListFilter {
            query: query.map(str::to_string),
            entry_type: None,
            status: None,
            knowledge_state,
            tag: None,
            include_deleted: false,
            trash_only: false,
        }
    }

    fn p95(samples: &mut [Duration]) -> Duration {
        assert!(!samples.is_empty());
        samples.sort_unstable();
        samples[(samples.len() * 95 - 1) / 100]
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
