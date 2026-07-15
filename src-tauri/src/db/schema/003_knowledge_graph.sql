ALTER TABLE entries ADD COLUMN knowledge_state TEXT NOT NULL DEFAULT 'capture'
  CHECK (knowledge_state IN ('capture', 'knowledge'));
ALTER TABLE entries ADD COLUMN knowledge_promoted_at TEXT NULL;
ALTER TABLE entries ADD COLUMN knowledge_title_key TEXT NULL;

CREATE UNIQUE INDEX idx_entries_knowledge_title_key
  ON entries(knowledge_title_key)
  WHERE knowledge_state = 'knowledge' AND knowledge_title_key IS NOT NULL;

CREATE INDEX idx_entries_knowledge_state_deleted_updated
  ON entries(knowledge_state, deleted_at, updated_at DESC);

CREATE TABLE entry_aliases (
  normalized_alias TEXT PRIMARY KEY,
  entry_id TEXT NOT NULL REFERENCES entries(id) ON DELETE CASCADE,
  alias TEXT NOT NULL,
  created_at TEXT NOT NULL
);

CREATE INDEX idx_entry_aliases_entry_id
  ON entry_aliases(entry_id);

CREATE TABLE entry_links (
  source_entry_id TEXT NOT NULL REFERENCES entries(id) ON DELETE CASCADE,
  ordinal INTEGER NOT NULL,
  raw_target TEXT NOT NULL,
  normalized_target TEXT NOT NULL,
  target_entry_id TEXT NULL REFERENCES entries(id) ON DELETE SET NULL,
  PRIMARY KEY (source_entry_id, ordinal)
);

CREATE INDEX idx_entry_links_target_entry_id
  ON entry_links(target_entry_id);

CREATE INDEX idx_entry_links_unresolved
  ON entry_links(normalized_target)
  WHERE target_entry_id IS NULL;
