CREATE TABLE schema_migrations (
  version INTEGER PRIMARY KEY,
  checksum TEXT NOT NULL,
  applied_at TEXT NOT NULL
);

CREATE TABLE entries (
  id TEXT PRIMARY KEY,
  title TEXT NULL,
  title_source TEXT NOT NULL CHECK (title_source IN ('auto', 'user')),
  original_content TEXT NOT NULL,
  current_content TEXT NOT NULL,
  type TEXT NOT NULL CHECK (type IN ('unclear', 'idea', 'task', 'material', 'question')),
  status TEXT NOT NULL CHECK (status IN ('pending', 'done', 'archived')),
  revision INTEGER NOT NULL DEFAULT 0,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  deleted_at TEXT NULL
);

CREATE INDEX idx_entries_status_deleted_created
  ON entries(status, deleted_at, created_at DESC);

CREATE INDEX idx_entries_type
  ON entries(type);

CREATE INDEX idx_entries_deleted_at
  ON entries(deleted_at);

CREATE TABLE tags (
  id TEXT PRIMARY KEY,
  name TEXT NOT NULL,
  normalized_name TEXT NOT NULL UNIQUE,
  created_at TEXT NOT NULL
);

CREATE TABLE entry_tags (
  entry_id TEXT NOT NULL REFERENCES entries(id) ON DELETE CASCADE,
  tag_id TEXT NOT NULL REFERENCES tags(id) ON DELETE CASCADE,
  PRIMARY KEY (entry_id, tag_id)
);

CREATE INDEX idx_entry_tags_tag_id
  ON entry_tags(tag_id);

CREATE TABLE settings (
  key TEXT PRIMARY KEY,
  value TEXT NOT NULL,
  updated_at TEXT NOT NULL
);

CREATE TABLE drafts (
  id TEXT PRIMARY KEY,
  content TEXT NOT NULL,
  revision INTEGER NOT NULL DEFAULT 0,
  updated_at TEXT NOT NULL
);
