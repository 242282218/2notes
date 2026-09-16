use rusqlite::{params, Connection, OptionalExtension, Transaction};

use crate::{
    db::migrations::now_string,
    db::repos::EntriesRepo,
    error::{AppError, AppResult},
    types::settings::Draft,
};

pub struct DraftsRepo;

const QUICK_CAPTURE_DRAFT_ID: &str = "quick_capture";
const MAX_QUICK_CAPTURE_BYTES: usize = 2 * 1024 * 1024;

fn validate_content(content: &str) -> AppResult<()> {
    if content.len() > MAX_QUICK_CAPTURE_BYTES {
        return Err(AppError::validation(
            "CONTENT_TOO_LARGE",
            "快速记录内容不能超过 2 MiB",
        ));
    }
    Ok(())
}

impl DraftsRepo {
    pub fn get(conn: &Connection) -> AppResult<Draft> {
        let draft = conn
            .query_row(
                "SELECT content, revision, updated_at FROM drafts WHERE id = ?1",
                params![QUICK_CAPTURE_DRAFT_ID],
                |row| {
                    Ok(Draft {
                        content: row.get(0)?,
                        revision: row.get(1)?,
                        updated_at: row.get(2)?,
                    })
                },
            )
            .optional()?;

        Ok(draft.unwrap_or_else(|| Draft {
            content: String::new(),
            revision: 0,
            updated_at: now_string(),
        }))
    }

    pub fn update(
        tx: &Transaction<'_>,
        content: &str,
        expected_revision: i64,
        now: &str,
    ) -> AppResult<Draft> {
        validate_content(content)?;
        let existing: Option<i64> = tx
            .query_row(
                "SELECT revision FROM drafts WHERE id = ?1",
                params![QUICK_CAPTURE_DRAFT_ID],
                |row| row.get(0),
            )
            .optional()?;

        match existing {
            Some(revision) if revision != expected_revision => Err(AppError::RevisionConflict),
            Some(revision) => {
                let next = revision + 1;
                tx.execute(
                    "UPDATE drafts SET content = ?1, revision = ?2, updated_at = ?3 WHERE id = ?4",
                    params![content, next, now, QUICK_CAPTURE_DRAFT_ID],
                )?;
                Ok(Draft {
                    content: content.to_string(),
                    revision: next,
                    updated_at: now.to_string(),
                })
            }
            None if expected_revision != 0 => Err(AppError::RevisionConflict),
            None => {
                tx.execute(
                    "INSERT INTO drafts(id, content, revision, updated_at) VALUES (?1, ?2, 1, ?3)",
                    params![QUICK_CAPTURE_DRAFT_ID, content, now],
                )?;
                Ok(Draft {
                    content: content.to_string(),
                    revision: 1,
                    updated_at: now.to_string(),
                })
            }
        }
    }

    pub fn clear(tx: &Transaction<'_>, expected_revision: i64, now: &str) -> AppResult<Draft> {
        let revision: Option<i64> = tx
            .query_row(
                "SELECT revision FROM drafts WHERE id = ?1",
                params![QUICK_CAPTURE_DRAFT_ID],
                |row| row.get(0),
            )
            .optional()?;
        if revision.unwrap_or(0) != expected_revision {
            return Err(AppError::RevisionConflict);
        }
        let next = expected_revision + 1;
        tx.execute(
            "
            INSERT INTO drafts(id, content, revision, updated_at)
            VALUES (?1, '', ?2, ?3)
            ON CONFLICT(id) DO UPDATE SET content = '', revision = ?2, updated_at = ?3
            ",
            params![QUICK_CAPTURE_DRAFT_ID, next, now],
        )?;
        Ok(Draft {
            content: String::new(),
            revision: next,
            updated_at: now.to_string(),
        })
    }

    pub fn submit_quick_capture(
        tx: &Transaction<'_>,
        content: &str,
        expected_revision: i64,
        now: &str,
    ) -> AppResult<Draft> {
        validate_content(content)?;
        EntriesRepo::create(tx, content, now)?;
        Self::clear(tx, expected_revision, now)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        db::connection::open_in_memory,
        db::repos::EntriesRepo,
        types::entries::{EntryListFilter, PageRequest},
    };

    #[test]
    fn updates_and_clears_draft() {
        let (mut conn, _) = open_in_memory().unwrap();
        let tx = conn.transaction().unwrap();
        let draft = DraftsRepo::update(&tx, "hello", 0, &now_string()).unwrap();
        let cleared = DraftsRepo::clear(&tx, draft.revision, &now_string()).unwrap();
        tx.commit().unwrap();

        assert_eq!(draft.revision, 1);
        assert_eq!(cleared.content, "");
        assert_eq!(DraftsRepo::get(&conn).unwrap().content, "");
    }

    #[test]
    fn rejects_oversized_quick_capture_content() {
        let (mut conn, _) = open_in_memory().unwrap();
        let tx = conn.transaction().unwrap();
        let err = DraftsRepo::update(
            &tx,
            &"x".repeat(MAX_QUICK_CAPTURE_BYTES + 1),
            0,
            &now_string(),
        )
        .unwrap_err();

        assert!(matches!(
            err,
            AppError::Validation { code, .. } if code == "CONTENT_TOO_LARGE"
        ));
    }

    #[test]
    fn submit_quick_capture_rejects_stale_draft_revision() {
        let (mut conn, _) = open_in_memory().unwrap();
        let now = now_string();
        let tx = conn.transaction().unwrap();
        DraftsRepo::update(&tx, "old", 0, &now).unwrap();
        let updated = DraftsRepo::update(&tx, "new", 1, &now).unwrap();

        let err = DraftsRepo::submit_quick_capture(&tx, "old", 1, &now).unwrap_err();

        assert!(matches!(err, AppError::RevisionConflict));
        assert_eq!(updated.revision, 2);
    }
    #[test]
    fn submit_quick_capture_creates_entry_and_clears_draft_atomically() {
        let (mut conn, _) = open_in_memory().unwrap();
        let now = now_string();
        let tx = conn.transaction().unwrap();
        DraftsRepo::update(&tx, "hello", 0, &now).unwrap();

        let cleared = DraftsRepo::submit_quick_capture(&tx, "hello", 1, &now).unwrap();

        tx.commit().unwrap();
        let page = EntriesRepo::list(
            &conn,
            &EntryListFilter {
                query: None,
                entry_type: None,
                status: None,
                knowledge_state: None,
                tag: None,
                include_deleted: false,
                trash_only: false,
            },
            &PageRequest {
                limit: None,
                offset: None,
            },
        )
        .unwrap();

        assert_eq!(cleared.content, "");
        assert_eq!(DraftsRepo::get(&conn).unwrap().content, "");
        assert_eq!(page.items.len(), 1);
    }
}
