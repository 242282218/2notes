use std::collections::HashSet;

use rusqlite::{params, Connection, Transaction};
use uuid::Uuid;

use crate::{error::AppResult, types::tags::Tag};

pub struct TagsRepo;

impl TagsRepo {
    pub fn list(conn: &Connection) -> AppResult<Vec<Tag>> {
        let mut stmt = conn.prepare(
            "
            SELECT t.id, t.name, t.normalized_name, t.created_at, COUNT(e.id) AS entry_count
            FROM tags t
            LEFT JOIN entry_tags et ON et.tag_id = t.id
            LEFT JOIN entries e ON e.id = et.entry_id AND e.deleted_at IS NULL
            GROUP BY t.id
            ORDER BY t.name COLLATE NOCASE
            ",
        )?;
        let rows = stmt.query_map([], map_tag)?;
        collect_tags(rows)
    }

    pub fn suggest(conn: &Connection, query: &str) -> AppResult<Vec<Tag>> {
        let pattern = format!("%{}%", super::entries_repo::escape_like(query.trim()));
        let mut stmt = conn.prepare(
            "
            SELECT t.id, t.name, t.normalized_name, t.created_at, COUNT(e.id) AS entry_count
            FROM tags t
            LEFT JOIN entry_tags et ON et.tag_id = t.id
            LEFT JOIN entries e ON e.id = et.entry_id AND e.deleted_at IS NULL
            WHERE t.name LIKE ?1 ESCAPE '\\'
            GROUP BY t.id
            ORDER BY t.name COLLATE NOCASE
            LIMIT 20
            ",
        )?;
        let rows = stmt.query_map(params![pattern], map_tag)?;
        collect_tags(rows)
    }

    pub fn resolve_many(tx: &Transaction<'_>, names: &[String], now: &str) -> AppResult<Vec<Tag>> {
        let mut seen = HashSet::new();
        let mut tags = Vec::new();

        for raw in names {
            let name = raw.trim();
            if name.is_empty() {
                continue;
            }
            let normalized = normalize_name(name);
            if !seen.insert(normalized.clone()) {
                continue;
            }

            tx.execute(
                "
                INSERT INTO tags(id, name, normalized_name, created_at)
                VALUES (?1, ?2, ?3, ?4)
                ON CONFLICT(normalized_name) DO NOTHING
                ",
                params![Uuid::new_v4().to_string(), name, normalized, now],
            )?;

            tags.push(Self::get_by_normalized(tx, &normalize_name(name))?);
        }

        Ok(tags)
    }

    pub fn replace_entry_tags(
        tx: &Transaction<'_>,
        entry_id: &str,
        tag_names: &[String],
        now: &str,
    ) -> AppResult<Vec<Tag>> {
        let tags = Self::resolve_many(tx, tag_names, now)?;
        tx.execute(
            "DELETE FROM entry_tags WHERE entry_id = ?1",
            params![entry_id],
        )?;
        for tag in &tags {
            tx.execute(
                "INSERT INTO entry_tags(entry_id, tag_id) VALUES (?1, ?2)",
                params![entry_id, tag.id],
            )?;
        }
        Ok(tags)
    }

    pub fn tags_for_entry(conn: &Connection, entry_id: &str) -> AppResult<Vec<Tag>> {
        let mut stmt = conn.prepare(
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
        let rows = stmt.query_map(params![entry_id], map_tag)?;
        collect_tags(rows)
    }

    fn get_by_normalized(tx: &Transaction<'_>, normalized: &str) -> AppResult<Tag> {
        Ok(tx.query_row(
            "
            SELECT t.id, t.name, t.normalized_name, t.created_at, COUNT(e.id) AS entry_count
            FROM tags t
            LEFT JOIN entry_tags et ON et.tag_id = t.id
            LEFT JOIN entries e ON e.id = et.entry_id AND e.deleted_at IS NULL
            WHERE t.normalized_name = ?1
            GROUP BY t.id
            ",
            params![normalized],
            map_tag,
        )?)
    }
}

pub fn normalize_name(name: &str) -> String {
    name.trim().to_lowercase()
}

fn collect_tags(rows: impl Iterator<Item = rusqlite::Result<Tag>>) -> AppResult<Vec<Tag>> {
    let mut tags = Vec::new();
    for row in rows {
        tags.push(row?);
    }
    Ok(tags)
}

fn map_tag(row: &rusqlite::Row<'_>) -> rusqlite::Result<Tag> {
    Ok(Tag {
        id: row.get(0)?,
        name: row.get(1)?,
        normalized_name: row.get(2)?,
        created_at: row.get(3)?,
        entry_count: row.get(4)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        db::{connection::open_in_memory, migrations::now_string, repos::EntriesRepo},
        types::entries::EntryPatch,
    };

    #[test]
    fn normalized_names_are_reused() {
        let (mut conn, _) = open_in_memory().unwrap();
        let tx = conn.transaction().unwrap();
        let tags = TagsRepo::resolve_many(
            &tx,
            &[" Work ".to_string(), "work".to_string(), "中文".to_string()],
            &now_string(),
        )
        .unwrap();
        tx.commit().unwrap();

        assert_eq!(tags.len(), 2);
        assert_eq!(TagsRepo::list(&conn).unwrap().len(), 2);
    }

    #[test]
    fn tag_counts_ignore_trashed_entries() {
        let (mut conn, _) = open_in_memory().unwrap();
        let now = now_string();
        let tx = conn.transaction().unwrap();
        let entry = EntriesRepo::create(&tx, "hello", &now).unwrap();
        let updated = EntriesRepo::update(
            &tx,
            &entry.id,
            EntryPatch {
                title: None,
                current_content: None,
                entry_type: None,
                status: None,
                tags: Some(vec!["work".to_string()]),
            },
            entry.revision,
            &now,
        )
        .unwrap();
        EntriesRepo::move_to_trash(&tx, &entry.id, updated.revision, &now).unwrap();
        tx.commit().unwrap();

        let tags = TagsRepo::list(&conn).unwrap();

        assert_eq!(tags[0].entry_count, 0);
    }
}
