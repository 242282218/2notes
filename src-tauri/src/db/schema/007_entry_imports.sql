CREATE TABLE entry_imports (
  entry_id TEXT PRIMARY KEY REFERENCES entries(id) ON DELETE CASCADE,
  canonical_source_path TEXT NOT NULL,
  source_hash TEXT NOT NULL,
  source_entry_id TEXT NULL,
  imported_at TEXT NOT NULL
);

CREATE UNIQUE INDEX idx_entry_imports_source_path_hash
  ON entry_imports(canonical_source_path, source_hash);
