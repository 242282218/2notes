CREATE TABLE entry_documents (
  entry_id TEXT PRIMARY KEY REFERENCES entries(id) ON DELETE CASCADE,
  schema_version INTEGER NOT NULL CHECK (schema_version = 1),
  entry_revision INTEGER NOT NULL,
  source_content_checksum TEXT NOT NULL,
  document_json TEXT NOT NULL CHECK (json_valid(document_json)),
  markdown_text TEXT NOT NULL,
  plain_text TEXT NOT NULL,
  legacy_content TEXT NULL,
  updated_at TEXT NOT NULL
);

CREATE TABLE blocks (
  id TEXT PRIMARY KEY,
  entry_id TEXT NOT NULL REFERENCES entries(id) ON DELETE CASCADE,
  parent_block_id TEXT NULL REFERENCES blocks(id) ON DELETE CASCADE,
  ordinal INTEGER NOT NULL CHECK (ordinal >= 0),
  depth INTEGER NOT NULL CHECK (depth BETWEEN 0 AND 32),
  kind TEXT NOT NULL,
  text_content TEXT NOT NULL,
  attrs_json TEXT NOT NULL CHECK (json_valid(attrs_json))
);

CREATE INDEX idx_blocks_entry_parent_ordinal
  ON blocks(entry_id, parent_block_id, ordinal);
CREATE INDEX idx_blocks_entry_kind ON blocks(entry_id, kind);
