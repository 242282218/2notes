// Command/repo wiring lands in M1 task 4; allow dead code until then.
#![allow(dead_code)]

use rusqlite::{params, Connection, Transaction};

use crate::{
    content::document::{
        document_to_markdown, document_to_plain_text, flatten_blocks, validate_document,
    },
    error::{AppError, AppResult},
    types::documents::{BlockDocument, DOCUMENT_SCHEMA_VERSION},
};

pub struct DocumentsRepo;

#[derive(Debug)]
pub struct DocumentRecord {
    pub entry_id: String,
    pub entry_revision: i64,
    pub source_content_checksum: String,
    pub document_json: String,
    pub markdown_text: String,
    pub plain_text: String,
    pub legacy_content: Option<String>,
    pub updated_at: String,
}

impl DocumentsRepo {
    pub fn create(
        tx: &Transaction<'_>,
        entry_id: &str,
        document: &BlockDocument,
        legacy_content: Option<&str>,
        now: &str,
    ) -> AppResult<DocumentRecord> {
        validate_document(document)?;
        let record = DocumentRecord::new(entry_id, 0, document, legacy_content, now);
        Self::insert(tx, &record, document)?;
        Ok(record)
    }

    pub fn get(conn: &Connection, entry_id: &str) -> AppResult<BlockDocument> {
        let json: String = conn
            .query_row(
                "SELECT document_json FROM entry_documents WHERE entry_id = ?1",
                [entry_id],
                |row| row.get(0),
            )
            .map_err(|_| AppError::not_found("文档不存在"))?;
        serde_json::from_str::<BlockDocument>(&json)
            .map_err(|err| AppError::validation("DOCUMENT_JSON_INVALID", err.to_string()))
    }

    pub fn get_with_tx(tx: &Transaction<'_>, entry_id: &str) -> AppResult<BlockDocument> {
        let json: String = tx
            .query_row(
                "SELECT document_json FROM entry_documents WHERE entry_id = ?1",
                [entry_id],
                |row| row.get(0),
            )
            .map_err(|_| AppError::not_found("文档不存在"))?;
        serde_json::from_str::<BlockDocument>(&json)
            .map_err(|err| AppError::validation("DOCUMENT_JSON_INVALID", err.to_string()))
    }

    pub fn replace(
        tx: &Transaction<'_>,
        entry_id: &str,
        entry_revision: i64,
        document: &BlockDocument,
        now: &str,
    ) -> AppResult<DocumentRecord> {
        validate_document(document)?;
        if !Self::exists(tx, entry_id)? {
            return Err(AppError::not_found("文档不存在"));
        }
        let record = DocumentRecord::new(entry_id, entry_revision, document, None, now);
        tx.execute("DELETE FROM blocks WHERE entry_id = ?1", [entry_id])?;
        Self::insert(tx, &record, document)?;
        Ok(record)
    }

    pub fn rebuild_projection(tx: &Transaction<'_>, entry_id: &str) -> AppResult<u32> {
        let document = Self::get_with_tx(tx, entry_id)?;
        tx.execute("DELETE FROM blocks WHERE entry_id = ?1", [entry_id])?;
        let projections = flatten_blocks(&document);
        let mut inserted = 0u32;
        for projection in &projections {
            tx.execute(
                "INSERT INTO blocks(
                   id, entry_id, parent_block_id, ordinal, depth, kind, text_content, attrs_json
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![
                    projection.id,
                    entry_id,
                    projection.parent_block_id,
                    projection.ordinal,
                    projection.depth,
                    format!("{:?}", projection.kind),
                    projection.text_content,
                    serde_json::to_string(&projection.attrs).map_err(|err| AppError::migration(
                        "BLOCK_ATTRS_ENCODE",
                        err.to_string()
                    ))?,
                ],
            )?;
            inserted += 1;
        }
        Ok(inserted)
    }

    fn exists(tx: &Transaction<'_>, entry_id: &str) -> AppResult<bool> {
        let count: i64 = tx.query_row(
            "SELECT COUNT(*) FROM entry_documents WHERE entry_id = ?1",
            [entry_id],
            |row| row.get(0),
        )?;
        Ok(count > 0)
    }

    fn insert(
        tx: &Transaction<'_>,
        record: &DocumentRecord,
        document: &BlockDocument,
    ) -> AppResult<()> {
        tx.execute(
            "INSERT INTO entry_documents(
               entry_id, schema_version, entry_revision, source_content_checksum,
               document_json, markdown_text, plain_text, legacy_content, updated_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
             ON CONFLICT(entry_id) DO UPDATE SET
               schema_version = excluded.schema_version,
               entry_revision = excluded.entry_revision,
               source_content_checksum = excluded.source_content_checksum,
               document_json = excluded.document_json,
               markdown_text = excluded.markdown_text,
               plain_text = excluded.plain_text,
               updated_at = excluded.updated_at
             -- legacy_content is preserved from the original row.",
            params![
                record.entry_id,
                DOCUMENT_SCHEMA_VERSION,
                record.entry_revision,
                record.source_content_checksum,
                record.document_json,
                record.markdown_text,
                record.plain_text,
                record.legacy_content,
                record.updated_at,
            ],
        )?;

        for projection in flatten_blocks(document) {
            tx.execute(
                "INSERT INTO blocks(
                   id, entry_id, parent_block_id, ordinal, depth, kind, text_content, attrs_json
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![
                    projection.id,
                    record.entry_id,
                    projection.parent_block_id,
                    projection.ordinal,
                    projection.depth,
                    format!("{:?}", projection.kind),
                    projection.text_content,
                    serde_json::to_string(&projection.attrs).map_err(|err| AppError::migration(
                        "BLOCK_ATTRS_ENCODE",
                        err.to_string()
                    ))?,
                ],
            )?;
        }
        Ok(())
    }
}

impl DocumentRecord {
    fn new(
        entry_id: &str,
        entry_revision: i64,
        document: &BlockDocument,
        legacy_content: Option<&str>,
        now: &str,
    ) -> Self {
        let document_json = serde_json::to_string(document).unwrap_or_default();
        let markdown_text = document_to_markdown(document).unwrap_or_default();
        let plain_text = document_to_plain_text(document);
        Self {
            entry_id: entry_id.to_string(),
            entry_revision,
            source_content_checksum: crate::db::migrations::checksum(&document_json),
            document_json,
            markdown_text,
            plain_text,
            legacy_content: legacy_content.map(str::to_string),
            updated_at: now.to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::connection::open_in_memory;
    use crate::db::migrations::now_string;
    use crate::types::documents::{BlockDocument, BlockNode};
    use rusqlite::params;

    fn uuid(label: &str) -> String {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        let mut hasher = DefaultHasher::new();
        label.hash(&mut hasher);
        let n = hasher.finish();
        let a = (n >> 32) as u32;
        let b = ((n >> 16) & 0xffff) as u16;
        let c = 0x4000u16 | (((n >> 4) as u16) & 0x0fff);
        let d = 0x8000u16 | ((n as u16) & 0x3fff);
        let e = n & 0x0000_ffff_ffff_ffff;
        format!("{a:08x}-{b:04x}-{c:04x}-{d:04x}-{e:012x}")
    }

    fn paragraph_block(label: &str, text: &str) -> BlockNode {
        BlockNode::paragraph(uuid(label), text)
    }

    fn paragraph_document(label: &str, text: &str) -> BlockDocument {
        BlockDocument::from_blocks(vec![paragraph_block(label, text)])
    }

    fn seed_entry(conn: &mut rusqlite::Connection, id: &str) {
        conn.execute(
            "INSERT INTO entries(
               id, title, title_source, original_content, current_content, type, status,
               revision, created_at, updated_at, deleted_at,
               knowledge_state, knowledge_promoted_at, knowledge_title_key
             ) VALUES (?1, 't', 'user', 'c', 'c', 'unclear', 'pending', 0,
                       '2026-07-01T00:00:00Z', '2026-07-01T00:00:00Z', NULL,
                       'capture', NULL, NULL)",
            params![id],
        )
        .unwrap();
    }

    #[test]
    fn create_inserts_document_and_projection() {
        let (mut conn, _) = open_in_memory().unwrap();
        seed_entry(&mut conn, "e1");
        let now = now_string();
        let document = paragraph_document("p1", "hello world");
        let tx = conn.transaction().unwrap();
        let record =
            DocumentsRepo::create(&tx, "e1", &document, Some("legacy bytes"), &now).unwrap();
        tx.commit().unwrap();

        assert_eq!(record.entry_id, "e1");
        assert_eq!(record.legacy_content.as_deref(), Some("legacy bytes"));

        let loaded = DocumentsRepo::get(&conn, "e1").unwrap();
        assert_eq!(loaded.blocks.len(), 1);
        assert_eq!(loaded.blocks[0].content, document.blocks[0].content);

        let block_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM blocks WHERE entry_id = 'e1'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(block_count, 1);
    }

    #[test]
    fn get_with_tx_round_trips_document() {
        let (mut conn, _) = open_in_memory().unwrap();
        seed_entry(&mut conn, "e1");
        let now = now_string();
        let document = paragraph_document("p1", "round trip");
        let tx = conn.transaction().unwrap();
        DocumentsRepo::create(&tx, "e1", &document, None, &now).unwrap();
        let loaded = DocumentsRepo::get_with_tx(&tx, "e1").unwrap();
        assert_eq!(loaded, document);
    }

    #[test]
    fn replace_updates_projection_and_keeps_legacy_snapshot() {
        let (mut conn, _) = open_in_memory().unwrap();
        seed_entry(&mut conn, "e1");
        let now = now_string();
        let v1 = paragraph_document("p1", "version one");
        let tx = conn.transaction().unwrap();
        DocumentsRepo::create(&tx, "e1", &v1, Some("legacy bytes"), &now).unwrap();
        tx.commit().unwrap();

        let v2 = BlockDocument::from_blocks(vec![
            paragraph_block("p1", "version two"),
            paragraph_block("p2", "second block"),
        ]);
        let tx = conn.transaction().unwrap();
        DocumentsRepo::replace(&tx, "e1", 1, &v2, &now).unwrap();
        tx.commit().unwrap();

        let loaded = DocumentsRepo::get(&conn, "e1").unwrap();
        assert_eq!(loaded.blocks.len(), 2);

        let legacy: String = conn
            .query_row(
                "SELECT legacy_content FROM entry_documents WHERE entry_id = 'e1'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(legacy, "legacy bytes");

        let block_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM blocks WHERE entry_id = 'e1'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(block_count, 2);
    }

    #[test]
    fn replace_rejects_cross_document_duplicate_block_id() {
        let (mut conn, _) = open_in_memory().unwrap();
        seed_entry(&mut conn, "a");
        seed_entry(&mut conn, "b");
        let now = now_string();
        let shared_id = uuid("shared");
        let doc_a = BlockDocument::from_blocks(vec![BlockNode::paragraph(shared_id.as_str(), "a")]);
        let tx = conn.transaction().unwrap();
        DocumentsRepo::create(&tx, "a", &doc_a, None, &now).unwrap();
        tx.commit().unwrap();

        let doc_b = BlockDocument::from_blocks(vec![BlockNode::paragraph(shared_id.as_str(), "b")]);
        let tx = conn.transaction().unwrap();
        let err = DocumentsRepo::create(&tx, "b", &doc_b, None, &now).unwrap_err();
        assert!(matches!(err, AppError::Db(_)));
    }

    #[test]
    fn rebuild_projection_keeps_block_ids_stable() {
        let (mut conn, _) = open_in_memory().unwrap();
        seed_entry(&mut conn, "e1");
        let now = now_string();
        let document = BlockDocument::from_blocks(vec![
            paragraph_block("p1", "first"),
            paragraph_block("p2", "second"),
        ]);
        let tx = conn.transaction().unwrap();
        DocumentsRepo::create(&tx, "e1", &document, None, &now).unwrap();
        tx.commit().unwrap();

        let before: Vec<String> = conn
            .prepare("SELECT id FROM blocks WHERE entry_id = 'e1' ORDER BY ordinal")
            .unwrap()
            .query_map([], |row| row.get(0))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();

        let tx = conn.transaction().unwrap();
        let inserted = DocumentsRepo::rebuild_projection(&tx, "e1").unwrap();
        tx.commit().unwrap();
        assert_eq!(inserted, 2);

        let after: Vec<String> = conn
            .prepare("SELECT id FROM blocks WHERE entry_id = 'e1' ORDER BY ordinal")
            .unwrap()
            .query_map([], |row| row.get(0))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        assert_eq!(before, after);
    }

    #[test]
    fn markdown_and_plain_text_are_derived_consistently() {
        let (mut conn, _) = open_in_memory().unwrap();
        seed_entry(&mut conn, "e1");
        let now = now_string();
        let document = paragraph_document("p1", "hello **not bold** [[Link]]");
        let tx = conn.transaction().unwrap();
        let record = DocumentsRepo::create(&tx, "e1", &document, None, &now).unwrap();
        assert!(record.markdown_text.contains("[[Link]]"));
        assert!(record.plain_text.contains("hello"));
    }

    #[test]
    fn empty_paragraph_document_is_valid() {
        let (mut conn, _) = open_in_memory().unwrap();
        seed_entry(&mut conn, "e1");
        let now = now_string();
        let document =
            BlockDocument::from_blocks(vec![BlockNode::empty_paragraph(uuid("empty").as_str())]);
        let tx = conn.transaction().unwrap();
        DocumentsRepo::create(&tx, "e1", &document, None, &now).unwrap();
        tx.commit().unwrap();
        let loaded = DocumentsRepo::get(&conn, "e1").unwrap();
        assert_eq!(loaded.blocks.len(), 1);
        assert!(loaded.blocks[0].content.is_empty());
    }
}
