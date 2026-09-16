-- Soft-deleted knowledge entries must not reserve their title key: they cannot be
-- edited while in the trash, so an unfiltered unique index would permanently burn
-- the title until a force delete. Rebuild the index to only cover live entries.
DROP INDEX IF EXISTS idx_entries_knowledge_title_key;

CREATE UNIQUE INDEX idx_entries_knowledge_title_key_active
  ON entries(knowledge_title_key)
  WHERE knowledge_state = 'knowledge'
    AND knowledge_title_key IS NOT NULL
    AND deleted_at IS NULL;
