use time::{Duration, OffsetDateTime};

use rusqlite::{params, Connection};

use crate::{
    error::{AppError, AppResult},
    types::{
        entries::PageRequest,
        health::{HealthIssue, HealthIssueKind, HealthIssuePage, KnowledgeHealthSummary},
    },
};

const MAX_PAGE_SIZE: u32 = 50;
const MAX_PAGE_OFFSET: u32 = 100_000;
const STALE_CAPTURE_AGE: Duration = Duration::days(30);

pub struct HealthRepo;

impl HealthRepo {
    pub fn summary(conn: &Connection, now: OffsetDateTime) -> AppResult<KnowledgeHealthSummary> {
        Ok(KnowledgeHealthSummary {
            unresolved_link: count_unresolved_links(conn)?,
            orphan_knowledge: count_orphan_knowledge(conn)?,
            untagged_knowledge: count_untagged_knowledge(conn)?,
            stale_capture: count_stale_capture(conn, now)?,
        })
    }

    pub fn issues(
        conn: &Connection,
        kind: HealthIssueKind,
        page: &PageRequest,
        now: OffsetDateTime,
    ) -> AppResult<HealthIssuePage> {
        let limit = page.limit.unwrap_or(MAX_PAGE_SIZE).clamp(1, MAX_PAGE_SIZE);
        let offset = page.offset.unwrap_or(0).min(MAX_PAGE_OFFSET);
        let mut items = match kind {
            HealthIssueKind::UnresolvedLink => unresolved_link_issues(conn, limit, offset)?,
            HealthIssueKind::OrphanKnowledge => orphan_knowledge_issues(conn, limit, offset)?,
            HealthIssueKind::UntaggedKnowledge => untagged_knowledge_issues(conn, limit, offset)?,
            HealthIssueKind::StaleCapture => stale_capture_issues(conn, limit, offset, now)?,
        };
        let has_more = items.len() > limit as usize;
        items.truncate(limit as usize);
        Ok(HealthIssuePage {
            items,
            limit,
            offset,
            has_more,
        })
    }
}

fn count_unresolved_links(conn: &Connection) -> AppResult<u32> {
    count(
        conn,
        "SELECT COUNT(*) FROM (
           SELECT 1
           FROM entry_links l
           JOIN entries e ON e.id = l.source_entry_id
           WHERE l.target_entry_id IS NULL AND e.deleted_at IS NULL
           GROUP BY l.source_entry_id, l.normalized_target, l.raw_target
         )",
    )
}

fn count_orphan_knowledge(conn: &Connection) -> AppResult<u32> {
    count(
        conn,
        &format!("SELECT COUNT(*) FROM ({})", orphan_knowledge_sql()),
    )
}

fn count_untagged_knowledge(conn: &Connection) -> AppResult<u32> {
    count(
        conn,
        &format!("SELECT COUNT(*) FROM ({})", untagged_knowledge_sql()),
    )
}

fn count_stale_capture(conn: &Connection, now: OffsetDateTime) -> AppResult<u32> {
    let cutoff = stale_capture_cutoff(now)?;
    Ok(conn.query_row(
        "SELECT COUNT(*) FROM entries
         WHERE knowledge_state = 'capture' AND status = 'pending' AND deleted_at IS NULL
           AND updated_at < ?1",
        [cutoff],
        |row| row.get(0),
    )?)
}

fn stale_capture_cutoff(now: OffsetDateTime) -> AppResult<String> {
    (now - STALE_CAPTURE_AGE)
        .format(&time::format_description::well_known::Rfc3339)
        .map_err(|err| AppError::system("HEALTH_TIME_FORMAT", err.to_string()))
}

fn count(conn: &Connection, sql: &str) -> AppResult<u32> {
    Ok(conn.query_row(sql, [], |row| row.get(0))?)
}

fn unresolved_link_issues(
    conn: &Connection,
    limit: u32,
    offset: u32,
) -> AppResult<Vec<HealthIssue>> {
    collect_issues(
        conn,
        "SELECT e.id, e.title, e.updated_at, l.raw_target, COUNT(*)
         FROM entry_links l
         JOIN entries e ON e.id = l.source_entry_id
         WHERE l.target_entry_id IS NULL AND e.deleted_at IS NULL
         GROUP BY e.id, l.normalized_target, l.raw_target
         ORDER BY e.updated_at DESC, e.id ASC, l.normalized_target, l.raw_target
         LIMIT ?1 OFFSET ?2",
        params![i64::from(limit) + 1, i64::from(offset)],
    )
}

fn orphan_knowledge_issues(
    conn: &Connection,
    limit: u32,
    offset: u32,
) -> AppResult<Vec<HealthIssue>> {
    collect_issues(
        conn,
        &format!(
            "SELECT id, title, updated_at, NULL, NULL FROM ({}) LIMIT ?1 OFFSET ?2",
            orphan_knowledge_sql()
        ),
        params![i64::from(limit) + 1, i64::from(offset)],
    )
}

fn untagged_knowledge_issues(
    conn: &Connection,
    limit: u32,
    offset: u32,
) -> AppResult<Vec<HealthIssue>> {
    collect_issues(
        conn,
        &format!(
            "SELECT id, title, updated_at, NULL, NULL FROM ({}) LIMIT ?1 OFFSET ?2",
            untagged_knowledge_sql()
        ),
        params![i64::from(limit) + 1, i64::from(offset)],
    )
}

fn stale_capture_issues(
    conn: &Connection,
    limit: u32,
    offset: u32,
    now: OffsetDateTime,
) -> AppResult<Vec<HealthIssue>> {
    let cutoff = stale_capture_cutoff(now)?;
    collect_issues(
        conn,
        "SELECT id, title, updated_at, NULL, NULL
         FROM entries
         WHERE knowledge_state = 'capture' AND status = 'pending' AND deleted_at IS NULL
           AND updated_at < ?1
         ORDER BY updated_at ASC, id ASC
         LIMIT ?2 OFFSET ?3",
        params![cutoff, i64::from(limit) + 1, i64::from(offset)],
    )
}

fn collect_issues<P: rusqlite::Params>(
    conn: &Connection,
    sql: &str,
    params: P,
) -> AppResult<Vec<HealthIssue>> {
    let mut stmt = conn.prepare(sql)?;
    let issues = stmt
        .query_map(params, |row| {
            Ok(HealthIssue {
                entry_id: row.get(0)?,
                title: row.get(1)?,
                updated_at: row.get(2)?,
                raw_target: row.get(3)?,
                occurrence_count: row.get(4)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(issues)
}

fn orphan_knowledge_sql() -> &'static str {
    "SELECT e.id, e.title, e.updated_at FROM entries e
     JOIN entry_hierarchy h ON h.entry_id = e.id
     WHERE e.knowledge_state = 'knowledge' AND e.deleted_at IS NULL
       AND h.parent_entry_id IS NULL
       AND NOT EXISTS (
           SELECT 1
           FROM entry_links l
           JOIN entries target ON target.id = l.target_entry_id
           WHERE l.source_entry_id = e.id AND target.deleted_at IS NULL
       )
       AND NOT EXISTS (
           SELECT 1
           FROM entry_links l
           JOIN entries source ON source.id = l.source_entry_id
           WHERE l.target_entry_id = e.id AND source.deleted_at IS NULL
       )
       AND NOT EXISTS (
           SELECT 1
           FROM entry_hierarchy child
           JOIN entries child_entry ON child_entry.id = child.entry_id
           WHERE child.parent_entry_id = e.id AND child_entry.deleted_at IS NULL
       )
     ORDER BY e.updated_at DESC, e.id ASC"
}

fn untagged_knowledge_sql() -> &'static str {
    "SELECT e.id, e.title, e.updated_at FROM entries e
     WHERE e.knowledge_state = 'knowledge' AND e.deleted_at IS NULL
       AND NOT EXISTS (SELECT 1 FROM entry_tags et WHERE et.entry_id = e.id)
     ORDER BY e.updated_at DESC, e.id ASC"
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    use crate::{
        db::{
            connection::{open_database, open_in_memory},
            repos::{EntriesRepo, KnowledgeRepo},
        },
        types::entries::PageRequest,
    };

    const NOW: &str = "2026-07-29T00:00:00Z";

    fn fixed_now() -> OffsetDateTime {
        OffsetDateTime::parse(NOW, &time::format_description::well_known::Rfc3339).unwrap()
    }

    fn create_entry(tx: &rusqlite::Transaction<'_>, content: &str, updated_at: &str) -> String {
        let entry = EntriesRepo::create(tx, content, updated_at).unwrap();
        entry.id
    }

    fn promote_entry(tx: &rusqlite::Transaction<'_>, id: &str, updated_at: &str) {
        let revision: i64 = tx
            .query_row("SELECT revision FROM entries WHERE id = ?1", [id], |row| {
                row.get(0)
            })
            .unwrap();
        KnowledgeRepo::promote(tx, id, revision, updated_at).unwrap();
    }

    fn page(limit: Option<u32>, offset: Option<u32>) -> PageRequest {
        PageRequest { limit, offset }
    }

    #[test]
    fn summary_and_issues_classify_eligible_entries() {
        let (mut write, read) = open_in_memory().unwrap();
        let tx = write.transaction().unwrap();
        let unresolved = create_entry(&tx, "[[Missing]] [[Missing]]", "2026-06-01T00:00:00Z");
        let orphan = create_entry(&tx, "Orphan", "2026-06-02T00:00:00Z");
        promote_entry(&tx, &orphan, "2026-06-02T00:00:00Z");
        let tagged = create_entry(&tx, "Tagged", "2026-06-03T00:00:00Z");
        promote_entry(&tx, &tagged, "2026-06-03T00:00:00Z");
        tx.execute(
            "INSERT INTO tags(id, name, normalized_name, created_at) VALUES ('tag-1', 'Tag', 'tag', ?1)",
            [NOW],
        )
        .unwrap();
        tx.execute(
            "INSERT INTO entry_tags(entry_id, tag_id) VALUES (?1, 'tag-1')",
            [&tagged],
        )
        .unwrap();
        let recent = create_entry(&tx, "Recent", "2026-07-01T00:00:00Z");
        let boundary = create_entry(&tx, "Boundary", "2026-06-29T00:00:00Z");
        let done = create_entry(&tx, "Done", "2026-06-01T00:00:00Z");
        let deleted = create_entry(&tx, "[[Deleted missing]]", "2026-06-01T00:00:00Z");
        tx.execute(
            "UPDATE entries SET deleted_at = ?1 WHERE id = ?2",
            [NOW, deleted.as_str()],
        )
        .unwrap();
        tx.execute("UPDATE entries SET status = 'done' WHERE id = ?1", [&done])
            .unwrap();
        tx.commit().unwrap();

        let summary = HealthRepo::summary(&read, fixed_now()).unwrap();
        assert_eq!(summary.unresolved_link, 1);
        assert_eq!(summary.orphan_knowledge, 2);
        assert_eq!(summary.untagged_knowledge, 1);
        assert_eq!(summary.stale_capture, 1);

        let unresolved_page = HealthRepo::issues(
            &read,
            HealthIssueKind::UnresolvedLink,
            &page(None, None),
            fixed_now(),
        )
        .unwrap();
        assert_eq!(unresolved_page.items.len(), 1);
        assert_eq!(unresolved_page.items[0].entry_id, unresolved);
        assert_eq!(
            unresolved_page.items[0].raw_target.as_deref(),
            Some("Missing")
        );
        assert_eq!(unresolved_page.items[0].occurrence_count, Some(2));

        let stale_page = HealthRepo::issues(
            &read,
            HealthIssueKind::StaleCapture,
            &page(None, None),
            fixed_now(),
        )
        .unwrap();
        assert_eq!(
            stale_page
                .items
                .iter()
                .map(|issue| &issue.entry_id)
                .collect::<Vec<_>>(),
            vec![&unresolved]
        );
        assert_ne!(recent, unresolved);
        assert_ne!(boundary, unresolved);
    }

    #[test]
    fn issues_paginate_and_clamp_limit() {
        let (mut write, read) = open_in_memory().unwrap();
        let tx = write.transaction().unwrap();
        let first = create_entry(&tx, "[[Missing one]]", "2026-06-01T00:00:00Z");
        let second = create_entry(&tx, "[[Missing two]]", "2026-06-02T00:00:00Z");
        let third = create_entry(&tx, "[[Missing three]]", "2026-06-03T00:00:00Z");
        tx.commit().unwrap();

        let first_page = HealthRepo::issues(
            &read,
            HealthIssueKind::UnresolvedLink,
            &page(Some(0), Some(0)),
            fixed_now(),
        )
        .unwrap();
        assert_eq!(first_page.limit, 1);
        assert!(first_page.has_more);
        assert_eq!(first_page.items[0].entry_id, third);

        let second_page = HealthRepo::issues(
            &read,
            HealthIssueKind::UnresolvedLink,
            &page(Some(1), Some(1)),
            fixed_now(),
        )
        .unwrap();
        assert_eq!(second_page.offset, 1);
        assert_eq!(second_page.items[0].entry_id, second);

        let max_page = HealthRepo::issues(
            &read,
            HealthIssueKind::UnresolvedLink,
            &page(Some(99), Some(0)),
            fixed_now(),
        )
        .unwrap();
        assert_eq!(max_page.limit, 50);
        assert!(!max_page.has_more);
        assert_eq!(max_page.items.len(), 3);
        assert_eq!(first, max_page.items[2].entry_id);
    }

    #[test]
    #[ignore = "release-scale benchmark"]
    fn health_issue_page_fifty_rows_has_p95_under_one_hundred_fifty_milliseconds() {
        const ELIGIBLE_ENTRY_COUNT: usize = 10_000;
        const QUERY_COUNT: usize = 100;
        const P95_LIMIT_MILLIS: u128 = 150;

        let temp = tempfile::tempdir().unwrap();
        let db_path = temp.path().join("health-scale.db");
        let (mut write, read) = open_database(&db_path).unwrap();
        let tx = write.transaction().unwrap();
        let mut insert = tx
            .prepare(
                "INSERT INTO entries(
                   id, title, title_source, original_content, current_content, type, status,
                   revision, created_at, updated_at, deleted_at,
                   knowledge_state, knowledge_promoted_at, knowledge_title_key
                 ) VALUES (?1, ?2, 'user', ?3, ?3, 'material', 'pending', 0, ?4, ?4, NULL,
                           'capture', NULL, NULL)",
            )
            .unwrap();
        for index in 0..ELIGIBLE_ENTRY_COUNT {
            let id = format!("scale-entry-{index:05}");
            let title = format!("Scale entry {index}");
            insert
                .execute(params![id, title, "eligible", "2026-06-01T00:00:00Z"])
                .unwrap();
        }
        drop(insert);
        tx.commit().unwrap();

        let mut durations = Vec::with_capacity(QUERY_COUNT);
        for _ in 0..QUERY_COUNT {
            let started = Instant::now();
            let issues = HealthRepo::issues(
                &read,
                HealthIssueKind::StaleCapture,
                &page(Some(50), Some(0)),
                fixed_now(),
            )
            .unwrap();
            durations.push(started.elapsed());
            assert_eq!(issues.items.len(), 50);
        }

        durations.sort_unstable();
        let p95 = durations[(QUERY_COUNT * 95 - 1) / 100];
        let p95_millis = p95.as_secs_f64() * 1_000.0;
        eprintln!("health_issue_page_fifty_rows p95_ms={p95_millis:.2}");
        assert!(
            p95_millis < P95_LIMIT_MILLIS as f64,
            "health issue page p95 was {p95_millis:.2}ms, expected below {P95_LIMIT_MILLIS}ms"
        );
    }
}
