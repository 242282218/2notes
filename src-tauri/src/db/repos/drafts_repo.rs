use rusqlite::{params, Connection, OptionalExtension, Transaction};

use crate::{
    db::migrations::now_string,
    error::{AppError, AppResult},
    types::settings::Draft,
};

pub struct DraftsRepo;

const QUICK_CAPTURE_DRAFT_ID: &str = "quick_capture";

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

    pub fn clear(tx: &Transaction<'_>, now: &str) -> AppResult<Draft> {
        let revision: Option<i64> = tx
            .query_row(
                "SELECT revision FROM drafts WHERE id = ?1",
                params![QUICK_CAPTURE_DRAFT_ID],
                |row| row.get(0),
            )
            .optional()?;
        let next = revision.unwrap_or(0) + 1;
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::connection::open_in_memory;

    #[test]
    fn updates_and_clears_draft() {
        let mut conn = open_in_memory().unwrap();
        let tx = conn.transaction().unwrap();
        let draft = DraftsRepo::update(&tx, "hello", 0, &now_string()).unwrap();
        let cleared = DraftsRepo::clear(&tx, &now_string()).unwrap();
        tx.commit().unwrap();

        assert_eq!(draft.revision, 1);
        assert_eq!(cleared.content, "");
        assert_eq!(DraftsRepo::get(&conn).unwrap().content, "");
    }
}
