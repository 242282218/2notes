use rusqlite::{params, types::Value, Connection, OptionalExtension, Transaction};
use uuid::Uuid;

use crate::{
    db::repos::tags_repo::{normalize_name, TagsRepo},
    error::{AppError, AppResult},
    types::{
        entries::{
            EntryDetail, EntryListFilter, EntryListItem, EntryPage, EntryPatch, EntryStatus,
            EntryType, PageRequest, TitleSource,
        },
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

        Self::get_with_tx(tx, &id)
    }

    pub fn list(
        conn: &Connection,
        filter: &EntryListFilter,
        page: &PageRequest,
    ) -> AppResult<EntryPage> {
        let limit = page.limit.unwrap_or(50).clamp(1, 200);
        let offset = page.offset.unwrap_or(0);
        let fetch_limit = limit + 1;

        let (where_sql, values) = build_filter(filter);
        let sql = format!(
            "
            SELECT id, title, title_source, original_content, current_content, type, status,
                   revision, created_at, updated_at, deleted_at
            FROM entries e
            {where_sql}
            ORDER BY created_at DESC
            LIMIT ? OFFSET ?
            "
        );
        let mut params = values;
        params.push(Value::Integer(i64::from(fetch_limit)));
        params.push(Value::Integer(i64::from(offset)));

        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt.query_map(rusqlite::params_from_iter(params), map_record)?;
        let mut items = Vec::new();
        for row in rows {
            let record = row?;
            items.push(record_to_list_item(conn, record)?);
        }

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

        let (title, title_source) = match title {
            Some(raw) => {
                let trimmed = raw.trim();
                if trimmed.is_empty() {
                    (auto_title(&new_content), TitleSource::Auto)
                } else {
                    (Some(trimmed.to_string()), TitleSource::User)
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

        if let Some(tags) = tags {
            TagsRepo::replace_entry_tags(tx, id, &tags, now)?;
        }

        Self::get_with_tx(tx, id)
    }

    pub fn move_to_trash(tx: &Transaction<'_>, id: &str, now: &str) -> AppResult<EntryDetail> {
        let current =
            Self::find_with_tx(tx, id)?.ok_or_else(|| AppError::not_found("条目不存在"))?;
        tx.execute(
            "UPDATE entries SET deleted_at = ?1, revision = revision + 1, updated_at = ?1 WHERE id = ?2",
            params![now, id],
        )?;
        log::info!("entry_moved_to_trash id={id}");
        Self::get_with_tx(tx, &current.id)
    }

    pub fn restore_from_trash(tx: &Transaction<'_>, id: &str, now: &str) -> AppResult<EntryDetail> {
        Self::find_with_tx(tx, id)?.ok_or_else(|| AppError::not_found("条目不存在"))?;
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
                   revision, created_at, updated_at, deleted_at
            FROM entries
            ORDER BY created_at DESC
            ",
        )?;
        let rows = stmt.query_map([], map_record)?;
        let mut entries = Vec::new();
        for row in rows {
            entries.push(record_to_detail(conn, row?)?);
        }
        Ok(entries)
    }

    fn get_with_tx(tx: &Transaction<'_>, id: &str) -> AppResult<EntryDetail> {
        let record =
            Self::find_with_tx(tx, id)?.ok_or_else(|| AppError::not_found("条目不存在"))?;
        record_to_detail_tx(tx, record)
    }

    fn find(conn: &Connection, id: &str) -> AppResult<Option<EntryRecord>> {
        conn.query_row(
            "
            SELECT id, title, title_source, original_content, current_content, type, status,
                   revision, created_at, updated_at, deleted_at
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
                   revision, created_at, updated_at, deleted_at
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

pub fn escape_like(input: &str) -> String {
    let mut escaped = String::new();
    for ch in input.chars() {
        match ch {
            '%' | '_' | '\\' => {
                escaped.push('\\');
                escaped.push(ch);
            }
            _ => escaped.push(ch),
        }
    }
    escaped
}

pub fn auto_title(content: &str) -> Option<String> {
    let compact = content.split_whitespace().collect::<Vec<_>>().join(" ");
    if compact.is_empty() {
        return None;
    }
    Some(compact.chars().take(80).collect())
}

fn build_filter(filter: &EntryListFilter) -> (String, Vec<Value>) {
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

    if let Some(query) = filter
        .query
        .as_ref()
        .map(|query| query.trim())
        .filter(|query| !query.is_empty())
    {
        clauses.push(
            "(
              COALESCE(e.title, '') LIKE ? ESCAPE '\\'
              OR e.current_content LIKE ? ESCAPE '\\'
              OR e.original_content LIKE ? ESCAPE '\\'
              OR EXISTS (
                SELECT 1 FROM entry_tags et
                JOIN tags t ON t.id = et.tag_id
                WHERE et.entry_id = e.id AND t.name LIKE ? ESCAPE '\\'
              )
            )"
            .to_string(),
        );
        let pattern = format!("%{}%", escape_like(query));
        values.extend([
            Value::Text(pattern.clone()),
            Value::Text(pattern.clone()),
            Value::Text(pattern.clone()),
            Value::Text(pattern),
        ]);
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
    })
}

fn record_to_list_item(conn: &Connection, record: EntryRecord) -> AppResult<EntryListItem> {
    let tags = TagsRepo::tags_for_entry(conn, &record.id)?;
    Ok(list_item(record, tags))
}

fn record_to_detail(conn: &Connection, record: EntryRecord) -> AppResult<EntryDetail> {
    let tags = TagsRepo::tags_for_entry(conn, &record.id)?;
    Ok(detail(record, tags))
}

fn record_to_detail_tx(tx: &Transaction<'_>, record: EntryRecord) -> AppResult<EntryDetail> {
    let tags = tags_for_entry_tx(tx, &record.id)?;
    Ok(detail(record, tags))
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
        tags,
        revision: record.revision,
        created_at: record.created_at,
        updated_at: record.updated_at,
        deleted_at: record.deleted_at,
    }
}

fn detail(record: EntryRecord, tags: Vec<Tag>) -> EntryDetail {
    EntryDetail {
        id: record.id,
        title: record.title,
        title_source: record.title_source,
        original_content: record.original_content,
        current_content: record.current_content,
        entry_type: record.entry_type,
        status: record.status,
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
    use crate::db::{connection::open_in_memory, migrations::now_string};

    fn default_filter() -> EntryListFilter {
        EntryListFilter {
            query: None,
            entry_type: None,
            status: None,
            tag: None,
            include_deleted: false,
            trash_only: false,
        }
    }

    #[test]
    fn creates_entry_with_defaults() {
        let mut conn = open_in_memory().unwrap();
        let tx = conn.transaction().unwrap();
        let entry = EntriesRepo::create(&tx, "hello world", &now_string()).unwrap();
        tx.commit().unwrap();

        assert_eq!(entry.entry_type, EntryType::Unclear);
        assert_eq!(entry.status, EntryStatus::Pending);
        assert_eq!(entry.original_content, "hello world");
    }

    #[test]
    fn update_does_not_overwrite_original_content() {
        let mut conn = open_in_memory().unwrap();
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
        let mut conn = open_in_memory().unwrap();
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
        let mut conn = open_in_memory().unwrap();
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
    fn revision_conflict_is_reported() {
        let mut conn = open_in_memory().unwrap();
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
    fn delete_forever_requires_trash() {
        let mut conn = open_in_memory().unwrap();
        let tx = conn.transaction().unwrap();
        let entry = EntriesRepo::create(&tx, "hello", &now_string()).unwrap();
        let err = EntriesRepo::delete_forever(&tx, &entry.id).unwrap_err();
        EntriesRepo::move_to_trash(&tx, &entry.id, &now_string()).unwrap();
        EntriesRepo::delete_forever(&tx, &entry.id).unwrap();
        tx.commit().unwrap();

        assert!(matches!(err, AppError::Validation { code, .. } if code == "ENTRY_NOT_IN_TRASH"));
    }
}
