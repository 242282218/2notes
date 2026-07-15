DROP TRIGGER IF EXISTS entries_fts_entries_insert;
DROP TRIGGER IF EXISTS entries_fts_entries_update;
DROP TRIGGER IF EXISTS entries_fts_entries_delete;
DROP TRIGGER IF EXISTS entries_fts_entry_tags_insert;
DROP TRIGGER IF EXISTS entries_fts_entry_tags_delete;
DROP TRIGGER IF EXISTS entries_fts_tags_update;
DROP TABLE IF EXISTS entries_fts;

CREATE VIRTUAL TABLE entries_fts USING fts5(
  entry_id UNINDEXED,
  title,
  original_content,
  current_content,
  tags_text,
  aliases_text,
  tokenize='trigram'
);

INSERT INTO entries_fts(
  entry_id,
  title,
  original_content,
  current_content,
  tags_text,
  aliases_text
)
SELECT
  e.id,
  COALESCE(e.title, ''),
  e.original_content,
  e.current_content,
  (
    SELECT COALESCE(GROUP_CONCAT(t.name, ' '), '')
    FROM entry_tags et
    JOIN tags t ON t.id = et.tag_id
    WHERE et.entry_id = e.id
  ),
  (
    SELECT COALESCE(GROUP_CONCAT(ea.alias, ' '), '')
    FROM entry_aliases ea
    WHERE ea.entry_id = e.id
  )
FROM entries e;

CREATE TRIGGER entries_fts_entries_insert
AFTER INSERT ON entries
BEGIN
  INSERT INTO entries_fts(
    entry_id,
    title,
    original_content,
    current_content,
    tags_text,
    aliases_text
  )
  VALUES (
    new.id,
    COALESCE(new.title, ''),
    new.original_content,
    new.current_content,
    '',
    ''
  );
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

CREATE TRIGGER entries_fts_entry_aliases_insert
AFTER INSERT ON entry_aliases
BEGIN
  UPDATE entries_fts
  SET aliases_text = (
    SELECT COALESCE(GROUP_CONCAT(ea.alias, ' '), '')
    FROM entry_aliases ea
    WHERE ea.entry_id = new.entry_id
  )
  WHERE entry_id = new.entry_id;
END;

CREATE TRIGGER entries_fts_entry_aliases_delete
AFTER DELETE ON entry_aliases
BEGIN
  UPDATE entries_fts
  SET aliases_text = (
    SELECT COALESCE(GROUP_CONCAT(ea.alias, ' '), '')
    FROM entry_aliases ea
    WHERE ea.entry_id = old.entry_id
  )
  WHERE entry_id = old.entry_id;
END;
