CREATE VIRTUAL TABLE entries_fts USING fts5(
  entry_id UNINDEXED,
  title,
  original_content,
  current_content,
  tags_text
);

INSERT INTO entries_fts(entry_id, title, original_content, current_content, tags_text)
SELECT
  e.id,
  COALESCE(e.title, ''),
  e.original_content,
  e.current_content,
  COALESCE(GROUP_CONCAT(t.name, ' '), '')
FROM entries e
LEFT JOIN entry_tags et ON et.entry_id = e.id
LEFT JOIN tags t ON t.id = et.tag_id
GROUP BY e.id;

CREATE TRIGGER entries_fts_entries_insert
AFTER INSERT ON entries
BEGIN
  INSERT INTO entries_fts(entry_id, title, original_content, current_content, tags_text)
  VALUES (new.id, COALESCE(new.title, ''), new.original_content, new.current_content, '');
END;

CREATE TRIGGER entries_fts_entries_update
AFTER UPDATE OF title, original_content, current_content ON entries
BEGIN
  UPDATE entries_fts
  SET title = COALESCE(new.title, ''),
      original_content = new.original_content,
      current_content = new.current_content
  WHERE entry_id = new.id;
END;

CREATE TRIGGER entries_fts_entries_delete
AFTER DELETE ON entries
BEGIN
  DELETE FROM entries_fts WHERE entry_id = old.id;
END;

CREATE TRIGGER entries_fts_entry_tags_insert
AFTER INSERT ON entry_tags
BEGIN
  UPDATE entries_fts
  SET tags_text = (
    SELECT COALESCE(GROUP_CONCAT(t.name, ' '), '')
    FROM entry_tags et
    JOIN tags t ON t.id = et.tag_id
    WHERE et.entry_id = new.entry_id
  )
  WHERE entry_id = new.entry_id;
END;

CREATE TRIGGER entries_fts_entry_tags_delete
AFTER DELETE ON entry_tags
BEGIN
  UPDATE entries_fts
  SET tags_text = (
    SELECT COALESCE(GROUP_CONCAT(t.name, ' '), '')
    FROM entry_tags et
    JOIN tags t ON t.id = et.tag_id
    WHERE et.entry_id = old.entry_id
  )
  WHERE entry_id = old.entry_id;
END;

-- Sync FTS tags_text when a tag's name changes.
CREATE TRIGGER entries_fts_tags_update
AFTER UPDATE OF name ON tags
BEGIN
  UPDATE entries_fts
  SET tags_text = (
    SELECT COALESCE(GROUP_CONCAT(t.name, ' '), '')
    FROM entry_tags et
    JOIN tags t ON t.id = et.tag_id
    WHERE et.entry_id = entries_fts.entry_id
  )
  WHERE entry_id IN (
    SELECT entry_id FROM entry_tags WHERE tag_id = new.id
  );
END;
