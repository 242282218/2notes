CREATE TABLE entry_hierarchy (
  entry_id TEXT PRIMARY KEY REFERENCES entries(id) ON DELETE CASCADE,
  parent_entry_id TEXT NULL REFERENCES entries(id) ON DELETE SET NULL,
  sibling_order INTEGER NOT NULL DEFAULT 0 CHECK (sibling_order >= 0),
  updated_at TEXT NOT NULL,
  CHECK (entry_id <> parent_entry_id)
);

CREATE INDEX idx_entry_hierarchy_parent_order
  ON entry_hierarchy(parent_entry_id, sibling_order, entry_id);
