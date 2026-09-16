CREATE TABLE entry_tombstones (
  entry_id TEXT PRIMARY KEY,
  deleted_at TEXT NOT NULL,
  recorded_at TEXT NOT NULL
);

CREATE INDEX idx_entry_tombstones_recorded_at
  ON entry_tombstones(recorded_at);
