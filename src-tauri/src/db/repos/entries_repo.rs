use std::collections::HashMap;

use rusqlite::{params, types::Value, Connection, OptionalExtension, Transaction};
use uuid::Uuid;

use crate::{
    db::repos::{
        knowledge_repo::KnowledgeRepo,
        tags_repo::{normalize_name, TagsRepo},
    },
    error::{AppError, AppResult},
    knowledge::wiki_links::canonicalize_knowledge_title,
    types::{
        entries::{
            EntryDetail, EntryListFilter, EntryListItem, EntryPage, EntryPatch, EntryStatus,
            EntryType, PageRequest, TitleSource,
        },
        knowledge::{KnowledgeState, SearchSnippet, SearchSnippetPart},
        tags::Tag,
    },
};

pub struct EntriesRepo;

struct EntryRecord {
    id: String,
    title: Option<String>,
    title_source: TitleSource,
    original_content: String,
    current_content: String,
    entry_type: EntryType,
    status: EntryStatus,
    revision: i64,
    created_at: String,
    updated_at: String,
    deleted_at: Option<String>,
    knowledge_state: KnowledgeState,
    search_snippet: Option<SearchSnippet>,
    knowledge_promoted_at: Option<String>,
    knowledge_title_key: Option<String>,
}

impl EntriesRepo {
    pub fn create(tx: &Transaction<'_>, content: &str, now: &str) -> AppResult<EntryDetail> {
        let content = content.trim();
        if content.is_empty() {
            return Err(AppError::validation(
                "VALIDATION_EMPTY_CONTENT",
                "内容不能为空",
            ));
        }

        let id = Uuid::new_v4().to_string();
        let title = auto_title(content);
        tx.execute(
            "
            INSERT INTO entries(
              id, title, title_source, original_content, current_content, type, status,
              revision, created_at, updated_at, deleted_at
            )
            VALUES (?1, ?2, 'auto', ?3, ?4, 'unclear', 'pending', 0, ?5, ?5, NULL)
            ",
            params![id, title, content, content, now],
        )?;

        KnowledgeRepo::refresh_source_links(tx, &id, content)?;

        Self::get_with_tx(tx, &id)
    }

    pub fn list(
        conn: &Connection,
        filter: &EntryListFilter,
        page: &PageRequest,
    ) -> AppResult<EntryPage> {
        let terms = search_terms(filter);
        if terms.is_empty() {
            return list_with_search(conn, filter, page, SearchMode::Like);
        }
        if terms
            .iter()
            .any(|term| term.chars().count() < 3 || !term.chars().all(char::is_alphanumeric))
            || !entries_fts_is_trigram(conn)?
        {
            return list_with_search(conn, filter, page, SearchMode::Like);
        }

        match list_with_search(conn, filter, page, SearchMode::Fts) {
            Ok(page) => Ok(page),
            Err(err) if can_fallback_from_fts(&err) => {
                list_with_search(conn, filter, page, SearchMode::Like)
            }
            Err(err) => Err(err),
        }
    }

    pub fn get(conn: &Connection, id: &str) -> AppResult<EntryDetail> {
        let record = Self::find(conn, id)?.ok_or_else(|| AppError::not_found("条目不存在"))?;
        record_to_detail(conn, record)
    }

    pub fn update(
        tx: &Transaction<'_>,
        id: &str,
        patch: EntryPatch,
        expected_revision: i64,
        now: &str,
    ) -> AppResult<EntryDetail> {
        let current =
            Self::find_with_tx(tx, id)?.ok_or_else(|| AppError::not_found("条目不存在"))?;
        if current.revision != expected_revision {
            return Err(AppError::RevisionConflict);
        }
        if current.deleted_at.is_some() {
            return Err(AppError::validation(
                "ENTRY_IN_TRASH",
                "回收站中的条目不能编辑",
            ));
        }

        let EntryPatch {
            title,
            current_content,
            entry_type,
            status,
            tags,
        } = patch;
        let content_changed = current_content.is_some();
        let new_content = current_content.unwrap_or_else(|| current.current_content.clone());
        if new_content.trim().is_empty() {
            return Err(AppError::validation(
                "VALIDATION_EMPTY_CONTENT",
                "内容不能为空",
            ));
        }

        let old_title = current.title.clone().unwrap_or_default();
        let is_knowledge = current.knowledge_state == KnowledgeState::Knowledge;
        let (title, title_source) = match title {
            Some(raw) => {
                if is_knowledge {
                    (Some(validated_knowledge_title(&raw)?), TitleSource::User)
                } else if raw.trim().is_empty() {
                    (auto_title(&new_content), TitleSource::Auto)
                } else {
                    (Some(raw.trim().to_string()), TitleSource::User)
                }
            }
            None if current.title_source == TitleSource::Auto && content_changed => {
                (auto_title(&new_content), TitleSource::Auto)
            }
            None => (current.title.clone(), current.title_source.clone()),
        };
        let entry_type = entry_type.unwrap_or(current.entry_type);
        let status = status.unwrap_or(current.status);
        let next_revision = current.revision + 1;

        tx.execute(
            "
            UPDATE entries
            SET title = ?1,
                title_source = ?2,
                current_content = ?3,
                type = ?4,
                status = ?5,
                revision = ?6,
                updated_at = ?7
            WHERE id = ?8
            ",
            params![
                title,
                title_source.as_str(),
                new_content,
                entry_type.as_str(),
                status.as_str(),
                next_revision,
                now,
                id
            ],
        )?;

        if is_knowledge {
            KnowledgeRepo::sync_title_metadata(
                tx,
                id,
                &old_title,
                title.as_deref().unwrap_or_default(),
                now,
            )?;
        }

        if let Some(tags) = tags {
            TagsRepo::replace_entry_tags(tx, id, &tags, now)?;
        }

        if new_content != current.current_content {
            KnowledgeRepo::refresh_source_links(tx, id, &new_content)?;
        }

        Self::get_with_tx(tx, id)
    }

    pub fn move_to_trash(
        tx: &Transaction<'_>,
        id: &str,
        expected_revision: i64,
        now: &str,
    ) -> AppResult<EntryDetail> {
        let current =
            Self::find_with_tx(tx, id)?.ok_or_else(|| AppError::not_found("条目不存在"))?;
        if current.revision != expected_revision || current.deleted_at.is_some() {
            return Err(AppError::RevisionConflict);
        }
        tx.execute(
            "UPDATE entries SET deleted_at = ?1, revision = revision + 1, updated_at = ?1 WHERE id = ?2",
            params![now, id],
        )?;
        log::info!("entry_moved_to_trash id={id}");
        Self::get_with_tx(tx, &current.id)
    }

    pub fn restore_from_trash(
        tx: &Transaction<'_>,
        id: &str,
        expected_revision: i64,
        now: &str,
    ) -> AppResult<EntryDetail> {
        let current =
            Self::find_with_tx(tx, id)?.ok_or_else(|| AppError::not_found("条目不存在"))?;
        if current.revision != expected_revision || current.deleted_at.is_none() {
            return Err(AppError::RevisionConflict);
        }
        tx.execute(
            "UPDATE entries SET deleted_at = NULL, revision = revision + 1, updated_at = ?1 WHERE id = ?2",
            params![now, id],
        )?;
        log::info!("entry_restored id={id}");
        Self::get_with_tx(tx, id)
    }

    pub fn delete_forever(tx: &Transaction<'_>, id: &str) -> AppResult<()> {
        let deleted_at: Option<String> = tx
            .query_row(
                "SELECT deleted_at FROM entries WHERE id = ?1",
                params![id],
                |row| row.get(0),
            )
            .optional()?
            .ok_or_else(|| AppError::not_found("条目不存在"))?;

        if deleted_at.is_none() {
            return Err(AppError::validation(
                "ENTRY_NOT_IN_TRASH",
                "只能永久删除回收站中的条目",
            ));
        }

        tx.execute("DELETE FROM entries WHERE id = ?1", params![id])?;
        log::info!("entry_deleted_forever id={id}");
        Ok(())
    }

    pub fn list_exportable(conn: &Connection) -> AppResult<Vec<EntryDetail>> {
        let mut stmt = conn.prepare(
            "
            SELECT id, title, title_source, original_content, current_content, type, status,
                   revision, created_at, updated_at, deleted_at,
                   knowledge_state, knowledge_promoted_at, knowledge_title_key
            FROM entries
            WHERE deleted_at IS NULL
            ORDER BY created_at DESC
            ",
        )?;
        let rows = stmt.query_map([], map_record)?;
        let mut records = Vec::new();
        for row in rows {
            records.push(row?);
        }
        let entries = records_to_details(conn, records)?;
        Ok(entries)
    }

    pub(crate) fn get_with_tx(tx: &Transaction<'_>, id: &str) -> AppResult<EntryDetail> {
        let record =
            Self::find_with_tx(tx, id)?.ok_or_else(|| AppError::not_found("条目不存在"))?;
        record_to_detail_tx(tx, record)
    }

    fn find(conn: &Connection, id: &str) -> AppResult<Option<EntryRecord>> {
        conn.query_row(
            "
            SELECT id, title, title_source, original_content, current_content, type, status,
                   revision, created_at, updated_at, deleted_at,
                   knowledge_state, knowledge_promoted_at, knowledge_title_key
            FROM entries
            WHERE id = ?1
            ",
            params![id],
            map_record,
        )
        .optional()
        .map_err(Into::into)
    }

    fn find_with_tx(tx: &Transaction<'_>, id: &str) -> AppResult<Option<EntryRecord>> {
        tx.query_row(
            "
            SELECT id, title, title_source, original_content, current_content, type, status,
                   revision, created_at, updated_at, deleted_at,
                   knowledge_state, knowledge_promoted_at, knowledge_title_key
            FROM entries
            WHERE id = ?1
            ",
            params![id],
            map_record,
        )
        .optional()
        .map_err(Into::into)
    }
}

fn validated_knowledge_title(input: &str) -> AppResult<String> {
    let title = canonicalize_knowledge_title(input);
    if title.is_empty() {
        return Err(AppError::validation(
            "KNOWLEDGE_TITLE_REQUIRED",
            "知识标题不能为空",
        ));
    }
    if title.chars().count() > 200 {
        return Err(AppError::validation(
            "KNOWLEDGE_TITLE_TOO_LONG",
            "知识标题不能超过 200 个字符",
        ));
    }
    if title.contains("[[") || title.contains("]]") {
        return Err(AppError::validation(
            "KNOWLEDGE_TITLE_INVALID",
            "知识标题不能包含 WikiLink 定界符",
        ));
    }
    Ok(title)
}

#[derive(Clone, Copy)]
enum SearchMode {
    Fts,
    Like,
}

fn list_with_search(
    conn: &Connection,
    filter: &EntryListFilter,
    page: &PageRequest,
    search_mode: SearchMode,
) -> AppResult<EntryPage> {
    let limit = page.limit.unwrap_or(50).clamp(1, 200);
    let offset = page.offset.unwrap_or(0).min(100_000);
    let fetch_limit = limit + 1;

    let (where_sql, values) = build_filter(filter, search_mode);
    let (sql, map_row): (
        String,
        fn(&rusqlite::Row<'_>) -> rusqlite::Result<EntryRecord>,
    ) = match search_mode {
        SearchMode::Like => (
            format!(
                "
                SELECT id, title, title_source, original_content, current_content, type, status,
                       revision, created_at, updated_at, deleted_at,
                       knowledge_state, knowledge_promoted_at, knowledge_title_key
                FROM entries e
                {where_sql}
                ORDER BY created_at DESC
                LIMIT ? OFFSET ?
                "
            ),
            map_record,
        ),
        SearchMode::Fts => (
            format!(
                "
                WITH search_plan AS (
                  SELECT e.id, e.title, e.title_source, e.original_content, e.current_content,
                         e.type, e.status, e.revision, e.created_at, e.updated_at, e.deleted_at,
                         e.knowledge_state, e.knowledge_promoted_at, e.knowledge_title_key,
                         bm25(entries_fts, 0.0, 10.0, 1.0, 4.0, 3.0, 6.0) AS rank,
                         snippet(entries_fts, -1, char(31), char(30), '…', 32) AS search_snippet
                  FROM entries_fts
                  JOIN entries e ON e.id = entries_fts.entry_id
                  {where_sql}
                  ORDER BY rank ASC, e.updated_at DESC, e.id ASC
                  LIMIT ? OFFSET ?
                )
                SELECT id, title, title_source, original_content, current_content, type, status,
                       revision, created_at, updated_at, deleted_at,
                       knowledge_state, knowledge_promoted_at, knowledge_title_key,
                       search_snippet
                FROM search_plan
                ORDER BY rank ASC, updated_at DESC, id ASC
                "
            ),
            map_record_with_search_snippet,
        ),
    };
    let mut params = values;
    params.push(Value::Integer(i64::from(fetch_limit)));
    params.push(Value::Integer(i64::from(offset)));

    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(rusqlite::params_from_iter(params), map_row)?;
    let mut records = Vec::new();
    for row in rows {
        records.push(row?);
    }

    let mut items = records_to_list_items(conn, records)?;

    let has_more = items.len() > limit as usize;
    if has_more {
        items.pop();
    }

    Ok(EntryPage {
        items,
        limit,
        offset,
        has_more,
    })
}

pub fn escape_like(input: &str) -> std::borrow::Cow<'_, str> {
    let needs_escape = input.chars().any(|ch| ch == '%' || ch == '_' || ch == '\\');
    if !needs_escape {
        return std::borrow::Cow::Borrowed(input);
    }
    let mut escaped = String::with_capacity(input.len() * 2);
    for ch in input.chars() {
        match ch {
            '%' | '_' | '\\' => {
                escaped.push('\\');
                escaped.push(ch);
            }
            _ => escaped.push(ch),
        }
    }
    std::borrow::Cow::Owned(escaped)
}

pub fn auto_title(content: &str) -> Option<String> {
    let compact = content.split_whitespace().collect::<Vec<_>>().join(" ");
    if compact.is_empty() {
        return None;
    }
    Some(compact.chars().take(80).collect())
}

fn normalized_query(filter: &EntryListFilter) -> Option<&str> {
    filter
        .query
        .as_ref()
        .map(|query| query.trim())
        .filter(|query| !query.is_empty())
}

fn search_terms(filter: &EntryListFilter) -> Vec<&str> {
    normalized_query(filter)
        .map(|query| query.split_whitespace().collect())
        .unwrap_or_default()
}

fn fts_phrase(query: &str) -> String {
    query
        .split_whitespace()
        .map(|term| format!("\"{}\"", term.replace('"', "\"\"")))
        .collect::<Vec<_>>()
        .join(" ")
}

pub(crate) fn entries_fts_is_trigram(conn: &Connection) -> AppResult<bool> {
    let sql = conn
        .query_row(
            "SELECT sql FROM sqlite_master WHERE type = 'table' AND name = 'entries_fts'",
            [],
            |row| row.get::<_, String>(0),
        )
        .optional()?;
    let Some(sql) = sql else {
        return Ok(false);
    };
    let normalized = sql
        .chars()
        .filter(|ch| !ch.is_whitespace())
        .flat_map(char::to_lowercase)
        .collect::<String>();
    Ok(normalized.contains("usingfts5(")
        && (normalized.contains("tokenize='trigram'")
            || normalized.contains("tokenize=\"trigram\"")))
}

fn can_fallback_from_fts(err: &AppError) -> bool {
    let AppError::Db(db_error) = err else {
        return false;
    };
    let message = db_error.to_string();
    message.contains("entries_fts")
        || message.contains("fts5")
        || message.contains("MATCH")
        || message.contains("no such module")
}

fn like_search_clause() -> String {
    r"(
      COALESCE(e.title, '') LIKE ? ESCAPE '\'
      OR e.current_content LIKE ? ESCAPE '\'
      OR e.original_content LIKE ? ESCAPE '\'
      OR EXISTS (
        SELECT 1 FROM entry_tags et
        JOIN tags t ON t.id = et.tag_id
        WHERE et.entry_id = e.id AND t.name LIKE ? ESCAPE '\'
      )
      OR EXISTS (
        SELECT 1 FROM entry_aliases ea
        WHERE ea.entry_id = e.id AND ea.alias LIKE ? ESCAPE '\'
      )
    )"
    .to_string()
}

fn build_filter(filter: &EntryListFilter, search_mode: SearchMode) -> (String, Vec<Value>) {
    let mut clauses = Vec::new();
    let mut values = Vec::new();

    if filter.trash_only {
        clauses.push("e.deleted_at IS NOT NULL".to_string());
    } else if !filter.include_deleted {
        clauses.push("e.deleted_at IS NULL".to_string());
    }

    if let Some(status) = &filter.status {
        clauses.push("e.status = ?".to_string());
        values.push(Value::Text(status.as_str().to_string()));
    }

    if let Some(knowledge_state) = &filter.knowledge_state {
        clauses.push("e.knowledge_state = ?".to_string());
        values.push(Value::Text(knowledge_state.as_str().to_string()));
    }

    if let Some(entry_type) = &filter.entry_type {
        clauses.push("e.type = ?".to_string());
        values.push(Value::Text(entry_type.as_str().to_string()));
    }

    if let Some(tag) = filter
        .tag
        .as_ref()
        .map(|tag| tag.trim())
        .filter(|tag| !tag.is_empty())
    {
        clauses.push(
            "EXISTS (
              SELECT 1 FROM entry_tags et
              JOIN tags t ON t.id = et.tag_id
              WHERE et.entry_id = e.id AND t.normalized_name = ?
            )"
            .to_string(),
        );
        values.push(Value::Text(normalize_name(tag)));
    }

    if let Some(query) = normalized_query(filter) {
        match search_mode {
            SearchMode::Fts => {
                clauses.push("entries_fts MATCH ?".to_string());
                values.push(Value::Text(fts_phrase(query)));
            }
            SearchMode::Like => {
                for term in query.split_whitespace() {
                    let pattern = format!("%{}%", escape_like(term));
                    clauses.push(like_search_clause());
                    values.extend([
                        Value::Text(pattern.clone()),
                        Value::Text(pattern.clone()),
                        Value::Text(pattern.clone()),
                        Value::Text(pattern.clone()),
                        Value::Text(pattern),
                    ]);
                }
            }
        }
    }

    if clauses.is_empty() {
        (String::new(), values)
    } else {
        (format!("WHERE {}", clauses.join(" AND ")), values)
    }
}

fn map_record(row: &rusqlite::Row<'_>) -> rusqlite::Result<EntryRecord> {
    let title_source_raw: String = row.get(2)?;
    let type_raw: String = row.get(5)?;
    let status_raw: String = row.get(6)?;
    Ok(EntryRecord {
        id: row.get(0)?,
        title: row.get(1)?,
        title_source: TitleSource::from_db(&title_source_raw).unwrap_or(TitleSource::Auto),
        original_content: row.get(3)?,
        current_content: row.get(4)?,
        entry_type: EntryType::from_db(&type_raw).unwrap_or(EntryType::Unclear),
        status: EntryStatus::from_db(&status_raw).unwrap_or(EntryStatus::Pending),
        revision: row.get(7)?,
        created_at: row.get(8)?,
        updated_at: row.get(9)?,
        deleted_at: row.get(10)?,
        knowledge_state: KnowledgeState::from_db(&row.get::<_, String>(11)?)
            .unwrap_or(KnowledgeState::Capture),
        search_snippet: None,
        knowledge_promoted_at: row.get(12)?,
        knowledge_title_key: row.get(13)?,
    })
}

fn map_record_with_search_snippet(row: &rusqlite::Row<'_>) -> rusqlite::Result<EntryRecord> {
    let mut record = map_record(row)?;
    let snippet = row.get::<_, String>(14)?;
    record.search_snippet = Some(parse_search_snippet(&snippet));
    Ok(record)
}

fn parse_search_snippet(snippet: &str) -> SearchSnippet {
    let mut parts = Vec::new();
    let mut text = String::new();
    let mut highlighted = false;

    for ch in snippet.chars() {
        if ch != '\u{1f}' && ch != '\u{1e}' {
            text.push(ch);
            continue;
        }
        if !text.is_empty() {
            parts.push(SearchSnippetPart {
                text: std::mem::take(&mut text),
                highlighted,
            });
        }
        highlighted = ch == '\u{1f}';
    }
    if !text.is_empty() {
        parts.push(SearchSnippetPart { text, highlighted });
    }

    SearchSnippet { parts }
}

fn record_to_detail(conn: &Connection, record: EntryRecord) -> AppResult<EntryDetail> {
    let tags = TagsRepo::tags_for_entry(conn, &record.id)?;
    let aliases = KnowledgeRepo::aliases_for_entry(conn, &record.id)?;
    Ok(detail(record, tags, aliases))
}

fn records_to_list_items(
    conn: &Connection,
    records: Vec<EntryRecord>,
) -> AppResult<Vec<EntryListItem>> {
    let tags_by_entry = tags_for_entries(conn, &records)?;
    Ok(records
        .into_iter()
        .map(|record| {
            let tags = tags_by_entry.get(&record.id).cloned().unwrap_or_default();
            list_item(record, tags)
        })
        .collect())
}

fn records_to_details(conn: &Connection, records: Vec<EntryRecord>) -> AppResult<Vec<EntryDetail>> {
    let tags_by_entry = tags_for_entries(conn, &records)?;
    let ids = records
        .iter()
        .map(|record| record.id.clone())
        .collect::<Vec<_>>();
    let aliases_by_entry = KnowledgeRepo::aliases_for_entries(conn, &ids)?;
    Ok(records
        .into_iter()
        .map(|record| {
            let tags = tags_by_entry.get(&record.id).cloned().unwrap_or_default();
            let aliases = aliases_by_entry
                .get(&record.id)
                .cloned()
                .unwrap_or_default();
            detail(record, tags, aliases)
        })
        .collect())
}

fn tags_for_entries(
    conn: &Connection,
    records: &[EntryRecord],
) -> AppResult<HashMap<String, Vec<Tag>>> {
    if records.is_empty() {
        return Ok(HashMap::new());
    }

    let placeholders = std::iter::repeat_n("?", records.len())
        .collect::<Vec<_>>()
        .join(", ");
    let sql = format!(
        "
        SELECT et.entry_id, t.id, t.name, t.normalized_name, t.created_at, COUNT(e2.id) AS entry_count
        FROM entry_tags et
        JOIN tags t ON t.id = et.tag_id
        LEFT JOIN entry_tags et2 ON et2.tag_id = t.id
        LEFT JOIN entries e2 ON e2.id = et2.entry_id AND e2.deleted_at IS NULL
        WHERE et.entry_id IN ({placeholders})
        GROUP BY et.entry_id, t.id
        ORDER BY et.entry_id, t.name COLLATE NOCASE
        "
    );
    let values = records
        .iter()
        .map(|record| Value::Text(record.id.clone()))
        .collect::<Vec<_>>();
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(rusqlite::params_from_iter(values), |row| {
        Ok((
            row.get::<_, String>(0)?,
            Tag {
                id: row.get(1)?,
                name: row.get(2)?,
                normalized_name: row.get(3)?,
                created_at: row.get(4)?,
                entry_count: row.get(5)?,
            },
        ))
    })?;

    let mut tags_by_entry: HashMap<String, Vec<Tag>> = HashMap::new();
    for row in rows {
        let (entry_id, tag) = row?;
        tags_by_entry.entry(entry_id).or_default().push(tag);
    }
    Ok(tags_by_entry)
}

fn record_to_detail_tx(tx: &Transaction<'_>, record: EntryRecord) -> AppResult<EntryDetail> {
    let tags = tags_for_entry_tx(tx, &record.id)?;
    let aliases = KnowledgeRepo::aliases_for_entry(tx, &record.id)?;
    Ok(detail(record, tags, aliases))
}

fn tags_for_entry_tx(tx: &Transaction<'_>, entry_id: &str) -> AppResult<Vec<Tag>> {
    let mut stmt = tx.prepare(
        "
        SELECT t.id, t.name, t.normalized_name, t.created_at, COUNT(e2.id) AS entry_count
        FROM tags t
        JOIN entry_tags et ON et.tag_id = t.id
        LEFT JOIN entry_tags et2 ON et2.tag_id = t.id
        LEFT JOIN entries e2 ON e2.id = et2.entry_id AND e2.deleted_at IS NULL
        WHERE et.entry_id = ?1
        GROUP BY t.id
        ORDER BY t.name COLLATE NOCASE
        ",
    )?;
    let rows = stmt.query_map(params![entry_id], |row| {
        Ok(Tag {
            id: row.get(0)?,
            name: row.get(1)?,
            normalized_name: row.get(2)?,
            created_at: row.get(3)?,
            entry_count: row.get(4)?,
        })
    })?;
    let mut tags = Vec::new();
    for row in rows {
        tags.push(row?);
    }
    Ok(tags)
}

fn list_item(record: EntryRecord, tags: Vec<Tag>) -> EntryListItem {
    let summary = record
        .current_content
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(120)
        .collect();
    EntryListItem {
        id: record.id,
        title: record.title,
        summary,
        entry_type: record.entry_type,
        status: record.status,
        knowledge_state: record.knowledge_state,
        search_snippet: record.search_snippet,
        tags,
        revision: record.revision,
        created_at: record.created_at,
        updated_at: record.updated_at,
        deleted_at: record.deleted_at,
    }
}

fn detail(record: EntryRecord, tags: Vec<Tag>, knowledge_aliases: Vec<String>) -> EntryDetail {
    EntryDetail {
        id: record.id,
        title: record.title,
        title_source: record.title_source,
        original_content: record.original_content,
        current_content: record.current_content,
        entry_type: record.entry_type,
        status: record.status,
        knowledge_state: record.knowledge_state,
        knowledge_promoted_at: record.knowledge_promoted_at,
        knowledge_aliases,
        tags,
        revision: record.revision,
        created_at: record.created_at,
        updated_at: record.updated_at,
        deleted_at: record.deleted_at,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{connection::open_in_memory, migrations::now_string, repos::KnowledgeRepo};

    fn default_filter() -> EntryListFilter {
        EntryListFilter {
            query: None,
            entry_type: None,
            status: None,
            knowledge_state: None,
            tag: None,
            include_deleted: false,
            trash_only: false,
        }
    }

    #[test]
    fn creates_entry_with_defaults() {
        let (mut conn, _) = open_in_memory().unwrap();
        let tx = conn.transaction().unwrap();
        let entry = EntriesRepo::create(&tx, "hello world", &now_string()).unwrap();
        tx.commit().unwrap();

        assert_eq!(entry.entry_type, EntryType::Unclear);
        assert_eq!(entry.status, EntryStatus::Pending);
        assert_eq!(entry.original_content, "hello world");
    }

    #[test]
    fn knowledge_filter_returns_only_knowledge_entries() {
        let (mut conn, _) = open_in_memory().unwrap();
        let now = now_string();
        let tx = conn.transaction().unwrap();
        EntriesRepo::create(&tx, "capture", &now).unwrap();
        let knowledge = EntriesRepo::create(&tx, "knowledge", &now).unwrap();
        KnowledgeRepo::promote(&tx, &knowledge.id, knowledge.revision, &now).unwrap();
        tx.commit().unwrap();
        let mut filter = default_filter();
        filter.knowledge_state = Some(KnowledgeState::Knowledge);

        let page = EntriesRepo::list(
            &conn,
            &filter,
            &PageRequest {
                limit: None,
                offset: None,
            },
        )
        .unwrap();
        assert_eq!(page.items.len(), 1);
        assert_eq!(page.items[0].id, knowledge.id);
    }

    #[test]
    fn create_indexes_wiki_links_from_quick_capture_path() {
        let (mut conn, _) = open_in_memory().unwrap();
        let now = now_string();
        let tx = conn.transaction().unwrap();
        let target = EntriesRepo::create(&tx, "目标", &now).unwrap();
        let target = KnowledgeRepo::promote(&tx, &target.id, target.revision, &now).unwrap();
        let source = EntriesRepo::create(&tx, "引用 [[目标]]", &now).unwrap();

        let resolved: String = tx
            .query_row(
                "SELECT target_entry_id FROM entry_links WHERE source_entry_id = ?1",
                [&source.id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(resolved, target.id);
    }

    #[test]
    fn updating_knowledge_title_adds_alias_in_same_transaction() {
        let (mut conn, _) = open_in_memory().unwrap();
        let now = now_string();
        let tx = conn.transaction().unwrap();
        let entry = EntriesRepo::create(&tx, "旧名", &now).unwrap();
        let entry = KnowledgeRepo::promote(&tx, &entry.id, entry.revision, &now).unwrap();

        let updated = EntriesRepo::update(
            &tx,
            &entry.id,
            EntryPatch {
                title: Some("新名".into()),
                current_content: None,
                entry_type: None,
                status: None,
                tags: None,
            },
            entry.revision,
            &now,
        )
        .unwrap();

        assert_eq!(updated.title.as_deref(), Some("新名"));
        assert_eq!(updated.knowledge_aliases, vec!["旧名"]);
    }

    #[test]
    fn clearing_a_knowledge_title_is_rejected() {
        let (mut conn, _) = open_in_memory().unwrap();
        let now = now_string();
        let tx = conn.transaction().unwrap();
        let entry = EntriesRepo::create(&tx, "保留标题", &now).unwrap();
        let entry = KnowledgeRepo::promote(&tx, &entry.id, entry.revision, &now).unwrap();

        let err = EntriesRepo::update(
            &tx,
            &entry.id,
            EntryPatch {
                title: Some("  ".into()),
                current_content: Some("不应保存".into()),
                entry_type: None,
                status: None,
                tags: None,
            },
            entry.revision,
            &now,
        )
        .unwrap_err();
        assert!(
            matches!(err, AppError::Validation { code, .. } if code == "KNOWLEDGE_TITLE_REQUIRED")
        );
        let unchanged = EntriesRepo::get_with_tx(&tx, &entry.id).unwrap();
        assert_eq!(unchanged.title, entry.title);
        assert_eq!(unchanged.current_content, entry.current_content);
        assert_eq!(unchanged.revision, entry.revision);
    }

    #[test]
    fn case_only_knowledge_rename_does_not_create_alias() {
        let (mut conn, _) = open_in_memory().unwrap();
        let now = now_string();
        let tx = conn.transaction().unwrap();
        let entry = EntriesRepo::create(&tx, "Phase Two", &now).unwrap();
        let entry = KnowledgeRepo::promote(&tx, &entry.id, entry.revision, &now).unwrap();
        let updated = EntriesRepo::update(
            &tx,
            &entry.id,
            EntryPatch {
                title: Some("PHASE TWO".into()),
                current_content: None,
                entry_type: None,
                status: None,
                tags: None,
            },
            entry.revision,
            &now,
        )
        .unwrap();
        assert_eq!(updated.title.as_deref(), Some("PHASE TWO"));
        assert!(updated.knowledge_aliases.is_empty());
    }

    #[test]
    fn deleting_target_forever_keeps_unresolved_source_text() {
        let (mut conn, _) = open_in_memory().unwrap();
        let now = now_string();
        let tx = conn.transaction().unwrap();
        let target = EntriesRepo::create(&tx, "将删除", &now).unwrap();
        let target = KnowledgeRepo::promote(&tx, &target.id, target.revision, &now).unwrap();
        let source = EntriesRepo::create(&tx, "保留 [[将删除]]", &now).unwrap();
        let target = EntriesRepo::move_to_trash(&tx, &target.id, target.revision, &now).unwrap();
        EntriesRepo::delete_forever(&tx, &target.id).unwrap();

        let (raw, resolved): (String, Option<String>) = tx
            .query_row(
                "SELECT raw_target, target_entry_id FROM entry_links WHERE source_entry_id = ?1",
                [&source.id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(raw, "将删除");
        assert!(resolved.is_none());
        assert_eq!(
            EntriesRepo::get_with_tx(&tx, &source.id)
                .unwrap()
                .current_content,
            "保留 [[将删除]]"
        );
    }

    #[test]
    fn update_does_not_overwrite_original_content() {
        let (mut conn, _) = open_in_memory().unwrap();
        let now = now_string();
        let tx = conn.transaction().unwrap();
        let entry = EntriesRepo::create(&tx, "first", &now).unwrap();
        let updated = EntriesRepo::update(
            &tx,
            &entry.id,
            EntryPatch {
                title: None,
                current_content: Some("second".to_string()),
                entry_type: None,
                status: None,
                tags: None,
            },
            entry.revision,
            &now,
        )
        .unwrap();
        tx.commit().unwrap();

        assert_eq!(updated.original_content, "first");
        assert_eq!(updated.current_content, "second");
    }

    #[test]
    fn search_hits_content_original_and_tags_with_chinese() {
        let (mut conn, _) = open_in_memory().unwrap();
        let now = now_string();
        let tx = conn.transaction().unwrap();
        let entry = EntriesRepo::create(&tx, "原始 中文 taggable", &now).unwrap();
        EntriesRepo::update(
            &tx,
            &entry.id,
            EntryPatch {
                title: Some("标题".to_string()),
                current_content: Some("当前内容".to_string()),
                entry_type: None,
                status: None,
                tags: Some(vec!["项目A".to_string()]),
            },
            entry.revision,
            &now,
        )
        .unwrap();
        tx.commit().unwrap();

        for query in ["标题", "当前", "原始", "项目A"] {
            let mut filter = default_filter();
            filter.query = Some(query.to_string());
            assert_eq!(
                EntriesRepo::list(
                    &conn,
                    &filter,
                    &PageRequest {
                        limit: None,
                        offset: None
                    }
                )
                .unwrap()
                .items
                .len(),
                1
            );
        }
    }

    #[test]
    fn like_wildcards_are_escaped() {
        let (mut conn, _) = open_in_memory().unwrap();
        let tx = conn.transaction().unwrap();
        EntriesRepo::create(&tx, "100% literal", &now_string()).unwrap();
        EntriesRepo::create(&tx, "1000 literal", &now_string()).unwrap();
        tx.commit().unwrap();

        let mut filter = default_filter();
        filter.query = Some("100%".to_string());
        let page = EntriesRepo::list(
            &conn,
            &filter,
            &PageRequest {
                limit: None,
                offset: None,
            },
        )
        .unwrap();

        assert_eq!(page.items.len(), 1);
        assert_eq!(escape_like("%_\\"), "\\%\\_\\\\");
    }

    #[test]
    fn fts_index_matches_full_terms() {
        let (mut conn, _) = open_in_memory().unwrap();
        let tx = conn.transaction().unwrap();
        let entry = EntriesRepo::create(&tx, "alpha searchable", &now_string()).unwrap();
        tx.commit().unwrap();

        let mut filter = default_filter();
        filter.query = Some("searchable".to_string());
        let page = EntriesRepo::list(
            &conn,
            &filter,
            &PageRequest {
                limit: None,
                offset: None,
            },
        )
        .unwrap();

        assert_eq!(page.items.len(), 1);
        assert_eq!(page.items[0].id, entry.id);
    }

    #[test]
    fn legacy_unicode61_index_forces_like_instead_of_false_empty_results() {
        let (mut conn, _) = open_in_memory().unwrap();
        let tx = conn.transaction().unwrap();
        let entry = EntriesRepo::create(&tx, "legacy searchable value", &now_string()).unwrap();
        tx.commit().unwrap();
        conn.execute("DROP TABLE entries_fts", []).unwrap();
        conn.execute_batch(
            "CREATE VIRTUAL TABLE entries_fts USING fts5(
               entry_id UNINDEXED,
               title,
               original_content,
               current_content,
               tags_text,
               aliases_text,
               tokenize='unicode61'
             );",
        )
        .unwrap();

        let mut filter = default_filter();
        filter.query = Some("searchable".to_string());
        let page = EntriesRepo::list(
            &conn,
            &filter,
            &PageRequest {
                limit: None,
                offset: None,
            },
        )
        .unwrap();

        assert_eq!(page.items.len(), 1);
        assert_eq!(page.items[0].id, entry.id);
    }

    #[test]
    fn mixed_short_and_long_terms_match_independently_in_like_fallback() {
        let (mut conn, _) = open_in_memory().unwrap();
        let now = now_string();
        let tx = conn.transaction().unwrap();
        let target = EntriesRepo::create(&tx, "知识", &now).unwrap();
        let target = KnowledgeRepo::promote(&tx, &target.id, target.revision, &now).unwrap();
        let target = EntriesRepo::update(
            &tx,
            &target.id,
            EntryPatch {
                title: Some("searchable target".to_string()),
                current_content: None,
                entry_type: None,
                status: None,
                tags: None,
            },
            target.revision,
            &now,
        )
        .unwrap();
        EntriesRepo::create(&tx, "知识 only", &now).unwrap();
        EntriesRepo::create(&tx, "searchable only", &now).unwrap();
        tx.commit().unwrap();

        let mut filter = default_filter();
        filter.query = Some("知识 searchable".to_string());
        let page = EntriesRepo::list(
            &conn,
            &filter,
            &PageRequest {
                limit: None,
                offset: None,
            },
        )
        .unwrap();

        assert_eq!(page.items.len(), 1);
        assert_eq!(page.items[0].id, target.id);
    }

    #[test]
    fn three_character_chinese_query_uses_ranked_fts_snippet() {
        let (mut conn, _) = open_in_memory().unwrap();
        let now = now_string();
        let tx = conn.transaction().unwrap();
        let entry = EntriesRepo::create(&tx, "知识库检索", &now).unwrap();
        let entry = KnowledgeRepo::promote(&tx, &entry.id, entry.revision, &now).unwrap();
        tx.commit().unwrap();

        let mut filter = default_filter();
        filter.query = Some("知识库".to_string());
        filter.knowledge_state = Some(KnowledgeState::Knowledge);
        let page = EntriesRepo::list(
            &conn,
            &filter,
            &PageRequest {
                limit: None,
                offset: None,
            },
        )
        .unwrap();

        assert_eq!(page.items.len(), 1);
        assert_eq!(page.items[0].id, entry.id);
        assert!(page.items[0]
            .search_snippet
            .as_ref()
            .is_some_and(|snippet| snippet.parts.iter().any(|part| part.highlighted)));
    }

    #[test]
    fn fts_search_orders_equal_ranks_by_updated_at_then_id() {
        let (mut conn, _) = open_in_memory().unwrap();
        let created_at = "2026-07-16T00:00:00Z";
        let tx = conn.transaction().unwrap();
        let first = EntriesRepo::create(&tx, "stableterm", created_at).unwrap();
        let second = EntriesRepo::create(&tx, "stableterm", created_at).unwrap();
        let newest = EntriesRepo::create(&tx, "stableterm", created_at).unwrap();
        tx.execute(
            "UPDATE entries SET updated_at = '2026-07-16T00:00:01Z' WHERE id = ?1",
            [&newest.id],
        )
        .unwrap();
        tx.commit().unwrap();

        let mut tied_ids = vec![first.id, second.id];
        tied_ids.sort();
        let mut expected_ids = vec![newest.id];
        expected_ids.extend(tied_ids);

        let mut filter = default_filter();
        filter.query = Some("stableterm".to_string());
        let page = EntriesRepo::list(
            &conn,
            &filter,
            &PageRequest {
                limit: None,
                offset: None,
            },
        )
        .unwrap();

        assert_eq!(
            page.items
                .into_iter()
                .map(|item| item.id)
                .collect::<Vec<_>>(),
            expected_ids
        );
    }

    #[test]
    fn parse_search_snippet_keeps_xss_payload_as_text() {
        let snippet = parse_search_snippet("\u{1f}<img onerror=alert(1)>\u{1e} safe");

        assert_eq!(snippet.parts.len(), 2);
        assert_eq!(snippet.parts[0].text, "<img onerror=alert(1)>");
        assert!(snippet.parts[0].highlighted);
        assert_eq!(snippet.parts[1].text, " safe");
        assert!(!snippet.parts[1].highlighted);
    }

    #[test]
    fn search_falls_back_to_like_when_fts_table_is_missing() {
        let (mut conn, _) = open_in_memory().unwrap();
        let tx = conn.transaction().unwrap();
        EntriesRepo::create(&tx, "substring-search-value", &now_string()).unwrap();
        tx.commit().unwrap();
        conn.execute("DROP TABLE entries_fts", []).unwrap();

        let mut filter = default_filter();
        filter.query = Some("search-value".to_string());
        let page = EntriesRepo::list(
            &conn,
            &filter,
            &PageRequest {
                limit: None,
                offset: None,
            },
        )
        .unwrap();

        assert_eq!(page.items.len(), 1);
        assert_eq!(page.items[0].summary, "substring-search-value");
    }

    #[test]
    fn fts_tag_update_rebuilds_each_entry_tags_independently() {
        let (mut conn, _) = open_in_memory().unwrap();
        let now = now_string();
        let tx = conn.transaction().unwrap();
        let first = EntriesRepo::create(&tx, "first", &now).unwrap();
        let second = EntriesRepo::create(&tx, "second", &now).unwrap();
        EntriesRepo::update(
            &tx,
            &first.id,
            EntryPatch {
                title: None,
                current_content: None,
                entry_type: None,
                status: None,
                tags: Some(vec!["shared".to_string(), "only-a".to_string()]),
            },
            first.revision,
            &now,
        )
        .unwrap();
        EntriesRepo::update(
            &tx,
            &second.id,
            EntryPatch {
                title: None,
                current_content: None,
                entry_type: None,
                status: None,
                tags: Some(vec!["shared".to_string(), "only-b".to_string()]),
            },
            second.revision,
            &now,
        )
        .unwrap();
        tx.execute(
            "UPDATE tags SET name = 'renamed' WHERE normalized_name = 'shared'",
            [],
        )
        .unwrap();
        tx.commit().unwrap();

        let first_tags: String = conn
            .query_row(
                "SELECT tags_text FROM entries_fts WHERE entry_id = ?1",
                params![first.id],
                |row| row.get(0),
            )
            .unwrap();
        let second_tags: String = conn
            .query_row(
                "SELECT tags_text FROM entries_fts WHERE entry_id = ?1",
                params![second.id],
                |row| row.get(0),
            )
            .unwrap();

        assert!(first_tags.contains("only-a"));
        assert!(!first_tags.contains("only-b"));
        assert!(second_tags.contains("only-b"));
        assert!(!second_tags.contains("only-a"));
    }
    #[test]
    fn fts_search_paginates_beyond_one_thousand_matches() {
        let (mut conn, _) = open_in_memory().unwrap();
        let tx = conn.transaction().unwrap();
        for index in 0..1005 {
            let timestamp = format!(
                "2026-01-01T{:02}:{:02}:{:02}Z",
                index / 3600,
                (index / 60) % 60,
                index % 60
            );
            EntriesRepo::create(&tx, &format!("bulkterm note {index}"), &timestamp).unwrap();
        }
        tx.commit().unwrap();

        let mut filter = default_filter();
        filter.query = Some("bulkterm".to_string());
        let page = EntriesRepo::list(
            &conn,
            &filter,
            &PageRequest {
                limit: Some(10),
                offset: Some(1000),
            },
        )
        .unwrap();

        assert_eq!(page.items.len(), 5);
        assert!(!page.has_more);
    }

    #[test]
    fn revision_conflict_is_reported() {
        let (mut conn, _) = open_in_memory().unwrap();
        let tx = conn.transaction().unwrap();
        let entry = EntriesRepo::create(&tx, "hello", &now_string()).unwrap();
        let err = EntriesRepo::update(
            &tx,
            &entry.id,
            EntryPatch {
                title: None,
                current_content: Some("changed".to_string()),
                entry_type: None,
                status: None,
                tags: None,
            },
            99,
            &now_string(),
        )
        .unwrap_err();

        assert!(matches!(err, AppError::RevisionConflict));
    }

    #[test]
    fn update_rejects_trashed_entry() {
        let (mut conn, _) = open_in_memory().unwrap();
        let tx = conn.transaction().unwrap();
        let entry = EntriesRepo::create(&tx, "hello", &now_string()).unwrap();
        let trashed =
            EntriesRepo::move_to_trash(&tx, &entry.id, entry.revision, &now_string()).unwrap();
        let err = EntriesRepo::update(
            &tx,
            &entry.id,
            EntryPatch {
                title: None,
                current_content: Some("changed".to_string()),
                entry_type: None,
                status: None,
                tags: None,
            },
            trashed.revision,
            &now_string(),
        )
        .unwrap_err();

        assert!(matches!(err, AppError::Validation { code, .. } if code == "ENTRY_IN_TRASH"));
    }

    #[test]
    fn delete_forever_requires_trash() {
        let (mut conn, _) = open_in_memory().unwrap();
        let tx = conn.transaction().unwrap();
        let entry = EntriesRepo::create(&tx, "hello", &now_string()).unwrap();
        let err = EntriesRepo::delete_forever(&tx, &entry.id).unwrap_err();
        EntriesRepo::move_to_trash(&tx, &entry.id, entry.revision, &now_string()).unwrap();
        EntriesRepo::delete_forever(&tx, &entry.id).unwrap();
        tx.commit().unwrap();

        assert!(matches!(err, AppError::Validation { code, .. } if code == "ENTRY_NOT_IN_TRASH"));
    }

    #[test]
    fn exportable_entries_exclude_trash() {
        let (mut conn, _) = open_in_memory().unwrap();
        let tx = conn.transaction().unwrap();
        let kept = EntriesRepo::create(&tx, "keep", &now_string()).unwrap();
        let trashed = EntriesRepo::create(&tx, "trash", &now_string()).unwrap();
        EntriesRepo::move_to_trash(&tx, &trashed.id, trashed.revision, &now_string()).unwrap();
        tx.commit().unwrap();

        let entries = EntriesRepo::list_exportable(&conn).unwrap();

        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].id, kept.id);
    }

    #[test]
    fn trash_and_restore_require_current_revision_and_state() {
        let (mut conn, _) = open_in_memory().unwrap();
        let tx = conn.transaction().unwrap();
        let entry = EntriesRepo::create(&tx, "hello", &now_string()).unwrap();
        let stale_err =
            EntriesRepo::move_to_trash(&tx, &entry.id, entry.revision + 1, &now_string())
                .unwrap_err();
        let trashed =
            EntriesRepo::move_to_trash(&tx, &entry.id, entry.revision, &now_string()).unwrap();
        let repeat_trash_err =
            EntriesRepo::move_to_trash(&tx, &entry.id, trashed.revision, &now_string())
                .unwrap_err();
        let restore_stale_err =
            EntriesRepo::restore_from_trash(&tx, &entry.id, entry.revision, &now_string())
                .unwrap_err();
        let restored =
            EntriesRepo::restore_from_trash(&tx, &entry.id, trashed.revision, &now_string())
                .unwrap();
        let repeat_restore_err =
            EntriesRepo::restore_from_trash(&tx, &entry.id, restored.revision, &now_string())
                .unwrap_err();

        assert!(matches!(stale_err, AppError::RevisionConflict));
        assert!(matches!(repeat_trash_err, AppError::RevisionConflict));
        assert!(matches!(restore_stale_err, AppError::RevisionConflict));
        assert!(matches!(repeat_restore_err, AppError::RevisionConflict));
    }

    #[test]
    fn fts_filter_does_not_mix_in_like_scan() {
        let mut filter = default_filter();
        filter.query = Some("search term".to_string());

        let (where_sql, _) = build_filter(&filter, SearchMode::Fts);

        assert!(where_sql.contains("entries_fts MATCH"));
        assert!(!where_sql.contains(" LIKE "));
    }
}
