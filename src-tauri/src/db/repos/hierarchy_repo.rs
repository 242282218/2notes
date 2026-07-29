use std::collections::{HashMap, HashSet};

use rusqlite::{params, Connection, OptionalExtension, Transaction};

use crate::{
    db::repos::EntriesRepo,
    error::{AppError, AppResult},
    types::{
        entries::EntryDetail,
        hierarchy::{EntryBreadcrumb, EntryTreeNode},
    },
};

pub struct HierarchyRepo;

impl HierarchyRepo {
    pub fn tree(conn: &Connection) -> AppResult<Vec<EntryTreeNode>> {
        let mut stmt = conn.prepare(
            "SELECT h.entry_id, e.title, h.parent_entry_id
             FROM entry_hierarchy h
             JOIN entries e ON e.id = h.entry_id
             WHERE e.knowledge_state = 'knowledge' AND e.deleted_at IS NULL
             ORDER BY h.parent_entry_id, h.sibling_order, h.entry_id",
        )?;
        let rows = stmt
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, Option<String>>(2)?,
                ))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        let mut nodes = HashMap::new();
        let mut children = HashMap::<Option<String>, Vec<String>>::new();
        for (id, title, parent_id) in rows {
            nodes.insert(id.clone(), title);
            children.entry(parent_id).or_default().push(id);
        }
        Ok(build_children(None, &nodes, &children, &mut HashSet::new()))
    }

    pub fn breadcrumbs(conn: &Connection, entry_id: &str) -> AppResult<Vec<EntryBreadcrumb>> {
        let mut breadcrumbs = Vec::new();
        let mut current = Some(entry_id.to_string());
        let mut seen = HashSet::new();
        while let Some(id) = current {
            if !seen.insert(id.clone()) {
                return Err(AppError::validation("HIERARCHY_CYCLE", "知识层级存在循环"));
            }
            let row = conn
                .query_row(
                    "SELECT e.title, h.parent_entry_id
                     FROM entry_hierarchy h
                     JOIN entries e ON e.id = h.entry_id
                     WHERE h.entry_id = ?1
                       AND e.knowledge_state = 'knowledge'
                       AND e.deleted_at IS NULL",
                    [&id],
                    |row| Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?)),
                )
                .optional()?;
            let Some((title, parent_id)) = row else {
                if breadcrumbs.is_empty() {
                    return Err(AppError::not_found("知识条目不存在"));
                }
                return Err(AppError::validation("HIERARCHY_INVALID", "知识层级无效"));
            };
            breadcrumbs.push(EntryBreadcrumb { id, title });
            current = parent_id;
        }
        breadcrumbs.reverse();
        Ok(breadcrumbs)
    }

    pub fn move_entry(
        tx: &Transaction<'_>,
        entry_id: &str,
        parent_entry_id: Option<&str>,
        sibling_order: u32,
        expected_revision: i64,
        now: &str,
    ) -> AppResult<EntryDetail> {
        let (revision, state, deleted_at): (i64, String, Option<String>) = tx
            .query_row(
                "SELECT revision, knowledge_state, deleted_at FROM entries WHERE id = ?1",
                [entry_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .optional()?
            .ok_or_else(|| AppError::not_found("条目不存在"))?;
        if revision != expected_revision {
            return Err(AppError::RevisionConflict);
        }
        if state != "knowledge" || deleted_at.is_some() {
            return Err(AppError::validation(
                "ENTRY_NOT_ACTIVE_KNOWLEDGE",
                "只能移动未删除的知识条目",
            ));
        }
        if parent_entry_id == Some(entry_id) {
            return Err(AppError::validation(
                "HIERARCHY_SELF_PARENT",
                "条目不能作为自己的父级",
            ));
        }
        if let Some(parent_id) = parent_entry_id {
            Self::validate_parent(tx, parent_id)?;
            Self::ensure_not_descendant(tx, entry_id, parent_id)?;
        }

        let old_parent = tx
            .query_row(
                "SELECT parent_entry_id FROM entry_hierarchy WHERE entry_id = ?1",
                [entry_id],
                |row| row.get::<_, Option<String>>(0),
            )
            .optional()?
            .ok_or_else(|| AppError::validation("HIERARCHY_MISSING", "知识条目缺少层级记录"))?;
        let new_parent = parent_entry_id.map(str::to_string);
        tx.execute(
            "UPDATE entry_hierarchy SET parent_entry_id = ?1, updated_at = ?2 WHERE entry_id = ?3",
            params![new_parent, now, entry_id],
        )?;

        if old_parent != new_parent {
            Self::resequence_siblings(tx, old_parent.as_deref(), now)?;
        }
        Self::insert_at_sibling_order(tx, entry_id, new_parent.as_deref(), sibling_order, now)?;
        tx.execute(
            "UPDATE entries SET revision = revision + 1, updated_at = ?1 WHERE id = ?2",
            params![now, entry_id],
        )?;
        EntriesRepo::get_with_tx(tx, entry_id)
    }

    pub(crate) fn insert_root(tx: &Transaction<'_>, entry_id: &str, now: &str) -> AppResult<()> {
        let next_order: i64 = tx.query_row(
            "SELECT COALESCE(MAX(sibling_order) + 1, 0)
             FROM entry_hierarchy WHERE parent_entry_id IS NULL",
            [],
            |row| row.get(0),
        )?;
        tx.execute(
            "INSERT INTO entry_hierarchy(entry_id, parent_entry_id, sibling_order, updated_at)
             VALUES (?1, NULL, ?2, ?3)",
            params![entry_id, next_order, now],
        )?;
        Ok(())
    }

    pub(crate) fn ensure_root(tx: &Transaction<'_>, entry_id: &str, now: &str) -> AppResult<()> {
        let exists = tx
            .query_row(
                "SELECT 1 FROM entry_hierarchy WHERE entry_id = ?1",
                [entry_id],
                |_| Ok(()),
            )
            .optional()?
            .is_some();
        if !exists {
            Self::insert_root(tx, entry_id, now)?;
        }
        Ok(())
    }

    pub(crate) fn remove_entry_promote_children(
        tx: &Transaction<'_>,
        entry_id: &str,
        now: &str,
    ) -> AppResult<()> {
        let parent_id = tx
            .query_row(
                "SELECT parent_entry_id FROM entry_hierarchy WHERE entry_id = ?1",
                [entry_id],
                |row| row.get::<_, Option<String>>(0),
            )
            .optional()?;
        let Some(parent_id) = parent_id else {
            return Ok(());
        };
        tx.execute(
            "UPDATE entry_hierarchy SET parent_entry_id = NULL, updated_at = ?1
             WHERE parent_entry_id = ?2",
            params![now, entry_id],
        )?;
        tx.execute(
            "DELETE FROM entry_hierarchy WHERE entry_id = ?1",
            [entry_id],
        )?;
        Self::resequence_siblings(tx, parent_id.as_deref(), now)?;
        Self::resequence_siblings(tx, None, now)
    }

    pub(crate) fn assert_demotable(tx: &Transaction<'_>, entry_id: &str) -> AppResult<()> {
        let parent_id = tx
            .query_row(
                "SELECT parent_entry_id FROM entry_hierarchy WHERE entry_id = ?1",
                [entry_id],
                |row| row.get::<_, Option<String>>(0),
            )
            .optional()?
            .ok_or_else(|| AppError::validation("HIERARCHY_MISSING", "知识条目缺少层级记录"))?;
        if parent_id.is_some() {
            return Err(AppError::validation(
                "HIERARCHY_NOT_ROOT",
                "仅根知识条目可以取消沉淀",
            ));
        }
        let has_children = tx
            .query_row(
                "SELECT 1 FROM entry_hierarchy WHERE parent_entry_id = ?1 LIMIT 1",
                [entry_id],
                |_| Ok(()),
            )
            .optional()?
            .is_some();
        if has_children {
            return Err(AppError::validation(
                "HIERARCHY_HAS_CHILDREN",
                "包含子条目的知识条目不能取消沉淀",
            ));
        }
        Ok(())
    }

    pub(crate) fn remove_entry(tx: &Transaction<'_>, entry_id: &str) -> AppResult<()> {
        tx.execute(
            "DELETE FROM entry_hierarchy WHERE entry_id = ?1",
            [entry_id],
        )?;
        Ok(())
    }

    pub(crate) fn repair(tx: &Transaction<'_>, now: &str) -> AppResult<()> {
        tx.execute(
            "DELETE FROM entry_hierarchy
             WHERE entry_id IN (
               SELECT id FROM entries
               WHERE deleted_at IS NOT NULL OR knowledge_state <> 'knowledge'
             )",
            [],
        )?;
        tx.execute(
            "UPDATE entry_hierarchy
             SET parent_entry_id = NULL, updated_at = ?1
             WHERE parent_entry_id IS NOT NULL
               AND parent_entry_id NOT IN (
                 SELECT id FROM entries
                 WHERE deleted_at IS NULL AND knowledge_state = 'knowledge'
               )",
            [now],
        )?;
        let mut missing = tx.prepare(
            "SELECT e.id
             FROM entries e
             LEFT JOIN entry_hierarchy h ON h.entry_id = e.id
             WHERE e.deleted_at IS NULL AND e.knowledge_state = 'knowledge'
               AND h.entry_id IS NULL
             ORDER BY e.updated_at DESC, e.id ASC",
        )?;
        let ids = missing
            .query_map([], |row| row.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        drop(missing);
        for id in ids {
            Self::insert_root(tx, &id, now)?;
        }
        Self::break_cycles(tx, now)?;
        let parents = tx
            .prepare("SELECT DISTINCT parent_entry_id FROM entry_hierarchy")?
            .query_map([], |row| row.get::<_, Option<String>>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        for parent_id in parents {
            Self::resequence_siblings(tx, parent_id.as_deref(), now)?;
        }
        Ok(())
    }

    fn break_cycles(tx: &Transaction<'_>, now: &str) -> AppResult<()> {
        let rows = tx
            .prepare("SELECT entry_id, parent_entry_id FROM entry_hierarchy")?
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        let parents = rows.into_iter().collect::<HashMap<_, _>>();
        let mut roots = HashSet::<String>::new();
        for entry_id in parents.keys() {
            let mut path = Vec::<String>::new();
            let mut positions = HashMap::<String, usize>::new();
            let mut current = Some(entry_id.clone());
            while let Some(id) = current {
                if let Some(&cycle_start) = positions.get(&id) {
                    let root = path[cycle_start..]
                        .iter()
                        .min()
                        .expect("cycle contains at least one entry");
                    roots.insert(root.clone());
                    break;
                }
                positions.insert(id.clone(), path.len());
                path.push(id.clone());
                current = parents.get(&id).cloned().flatten();
            }
        }
        for entry_id in roots {
            tx.execute(
                "UPDATE entry_hierarchy SET parent_entry_id = NULL, updated_at = ?1 WHERE entry_id = ?2",
                params![now, entry_id],
            )?;
        }
        Ok(())
    }

    fn validate_parent(tx: &Transaction<'_>, parent_id: &str) -> AppResult<()> {
        let valid = tx
            .query_row(
                "SELECT 1 FROM entries
                 WHERE id = ?1 AND knowledge_state = 'knowledge' AND deleted_at IS NULL",
                [parent_id],
                |_| Ok(()),
            )
            .optional()?
            .is_some();
        if !valid {
            return Err(AppError::validation(
                "HIERARCHY_INVALID_PARENT",
                "父级必须是未删除的知识条目",
            ));
        }
        Ok(())
    }

    fn ensure_not_descendant(
        tx: &Transaction<'_>,
        entry_id: &str,
        parent_id: &str,
    ) -> AppResult<()> {
        let mut current = Some(parent_id.to_string());
        let mut seen = HashSet::new();
        while let Some(id) = current {
            if !seen.insert(id.clone()) || id == entry_id {
                return Err(AppError::validation(
                    "HIERARCHY_CYCLE",
                    "知识层级不能形成循环",
                ));
            }
            current = tx
                .query_row(
                    "SELECT parent_entry_id FROM entry_hierarchy WHERE entry_id = ?1",
                    [&id],
                    |row| row.get::<_, Option<String>>(0),
                )
                .optional()?
                .flatten();
        }
        Ok(())
    }

    fn insert_at_sibling_order(
        tx: &Transaction<'_>,
        entry_id: &str,
        parent_id: Option<&str>,
        sibling_order: u32,
        now: &str,
    ) -> AppResult<()> {
        let mut siblings = Self::sibling_ids(tx, parent_id)?;
        siblings.retain(|id| id != entry_id);
        let index = usize::min(sibling_order as usize, siblings.len());
        siblings.insert(index, entry_id.to_string());
        for (order, id) in siblings.iter().enumerate() {
            tx.execute(
                "UPDATE entry_hierarchy SET sibling_order = ?1, updated_at = ?2 WHERE entry_id = ?3",
                params![order as i64, now, id],
            )?;
        }
        Ok(())
    }

    fn resequence_siblings(
        tx: &Transaction<'_>,
        parent_id: Option<&str>,
        now: &str,
    ) -> AppResult<()> {
        for (order, id) in Self::sibling_ids(tx, parent_id)?.iter().enumerate() {
            tx.execute(
                "UPDATE entry_hierarchy SET sibling_order = ?1, updated_at = ?2 WHERE entry_id = ?3",
                params![order as i64, now, id],
            )?;
        }
        Ok(())
    }

    fn sibling_ids(tx: &Transaction<'_>, parent_id: Option<&str>) -> AppResult<Vec<String>> {
        let mut stmt = tx.prepare(
            "SELECT entry_id FROM entry_hierarchy
             WHERE parent_entry_id IS ?1
             ORDER BY sibling_order, entry_id",
        )?;
        let ids = stmt
            .query_map([parent_id], |row| row.get(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(ids)
    }
}

fn build_children(
    parent_id: Option<String>,
    titles: &HashMap<String, String>,
    children: &HashMap<Option<String>, Vec<String>>,
    path: &mut HashSet<String>,
) -> Vec<EntryTreeNode> {
    children
        .get(&parent_id)
        .into_iter()
        .flatten()
        .filter_map(|id| {
            if !path.insert(id.clone()) {
                return None;
            }
            let node = EntryTreeNode {
                id: id.clone(),
                title: titles.get(id)?.clone(),
                children: build_children(Some(id.clone()), titles, children, path),
            };
            path.remove(id);
            Some(node)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        db::{connection::open_in_memory, migrations::now_string, repos::KnowledgeRepo},
        types::knowledge::KnowledgeState,
    };

    fn create_knowledge(
        tx: &Transaction<'_>,
        title: &str,
        now: &str,
    ) -> crate::error::AppResult<EntryDetail> {
        let entry = EntriesRepo::create(tx, title, now)?;
        KnowledgeRepo::promote(tx, &entry.id, entry.revision, now)
    }

    fn sibling_rows(tx: &Transaction<'_>, parent_id: Option<&str>) -> Vec<(String, i64)> {
        tx.prepare(
            "SELECT entry_id, sibling_order FROM entry_hierarchy
             WHERE parent_entry_id IS ?1 ORDER BY sibling_order, entry_id",
        )
        .unwrap()
        .query_map([parent_id], |row| Ok((row.get(0)?, row.get(1)?)))
        .unwrap()
        .collect::<rusqlite::Result<Vec<_>>>()
        .unwrap()
    }

    #[test]
    fn move_builds_tree_and_breadcrumbs() {
        let (mut write, read) = open_in_memory().unwrap();
        let now = now_string();
        let tx = write.transaction().unwrap();
        let root = create_knowledge(&tx, "Root", &now).unwrap();
        let child = create_knowledge(&tx, "Child", &now).unwrap();
        let child =
            HierarchyRepo::move_entry(&tx, &child.id, Some(&root.id), 0, child.revision, &now)
                .unwrap();
        tx.commit().unwrap();

        let tree = HierarchyRepo::tree(&read).unwrap();
        assert_eq!(tree.len(), 1);
        assert_eq!(tree[0].id, root.id);
        assert_eq!(tree[0].children[0].id, child.id);
        let breadcrumbs = HierarchyRepo::breadcrumbs(&read, &child.id).unwrap();
        assert_eq!(
            breadcrumbs.iter().map(|item| &item.id).collect::<Vec<_>>(),
            vec![&root.id, &child.id]
        );
    }

    #[test]
    fn move_rejects_cycle_and_stale_revision() {
        let (mut write, _) = open_in_memory().unwrap();
        let now = now_string();
        let tx = write.transaction().unwrap();
        let root = create_knowledge(&tx, "Root", &now).unwrap();
        let child = create_knowledge(&tx, "Child", &now).unwrap();
        let child =
            HierarchyRepo::move_entry(&tx, &child.id, Some(&root.id), 0, child.revision, &now)
                .unwrap();

        let cycle =
            HierarchyRepo::move_entry(&tx, &root.id, Some(&child.id), 0, root.revision, &now)
                .unwrap_err();
        assert!(matches!(
            cycle,
            AppError::Validation {
                code: "HIERARCHY_CYCLE",
                ..
            }
        ));
        let stale = HierarchyRepo::move_entry(&tx, &child.id, None, 0, child.revision - 1, &now)
            .unwrap_err();
        assert!(matches!(stale, AppError::RevisionConflict));
    }

    #[test]
    fn move_reorders_siblings_and_rejects_invalid_parents() {
        let (mut write, _) = open_in_memory().unwrap();
        let now = now_string();
        let tx = write.transaction().unwrap();
        let first_parent = create_knowledge(&tx, "First parent", &now).unwrap();
        let second_parent = create_knowledge(&tx, "Second parent", &now).unwrap();
        let first = create_knowledge(&tx, "First", &now).unwrap();
        let second = create_knowledge(&tx, "Second", &now).unwrap();
        let third = create_knowledge(&tx, "Third", &now).unwrap();
        let first = HierarchyRepo::move_entry(
            &tx,
            &first.id,
            Some(&first_parent.id),
            0,
            first.revision,
            &now,
        )
        .unwrap();
        let second = HierarchyRepo::move_entry(
            &tx,
            &second.id,
            Some(&first_parent.id),
            1,
            second.revision,
            &now,
        )
        .unwrap();
        let third = HierarchyRepo::move_entry(
            &tx,
            &third.id,
            Some(&first_parent.id),
            2,
            third.revision,
            &now,
        )
        .unwrap();
        let third = HierarchyRepo::move_entry(
            &tx,
            &third.id,
            Some(&first_parent.id),
            0,
            third.revision,
            &now,
        )
        .unwrap();
        let second = HierarchyRepo::move_entry(
            &tx,
            &second.id,
            Some(&second_parent.id),
            0,
            second.revision,
            &now,
        )
        .unwrap();
        let capture = EntriesRepo::create(&tx, "Capture", &now).unwrap();
        let trashed_parent = create_knowledge(&tx, "Trashed parent", &now).unwrap();
        let trashed_parent =
            EntriesRepo::move_to_trash(&tx, &trashed_parent.id, trashed_parent.revision, &now)
                .unwrap();

        let invalid_parent =
            HierarchyRepo::move_entry(&tx, &first.id, Some(&capture.id), 0, first.revision, &now)
                .unwrap_err();
        assert!(matches!(
            invalid_parent,
            AppError::Validation {
                code: "HIERARCHY_INVALID_PARENT",
                ..
            }
        ));
        let trashed_parent_error = HierarchyRepo::move_entry(
            &tx,
            &first.id,
            Some(&trashed_parent.id),
            0,
            first.revision,
            &now,
        )
        .unwrap_err();
        assert!(matches!(
            trashed_parent_error,
            AppError::Validation {
                code: "HIERARCHY_INVALID_PARENT",
                ..
            }
        ));
        assert_eq!(
            EntriesRepo::get_with_tx(&tx, &first.id).unwrap().revision,
            first.revision
        );
        let self_parent =
            HierarchyRepo::move_entry(&tx, &first.id, Some(&first.id), 0, first.revision, &now)
                .unwrap_err();
        assert!(matches!(
            self_parent,
            AppError::Validation {
                code: "HIERARCHY_SELF_PARENT",
                ..
            }
        ));

        let first_parent_children = sibling_rows(&tx, Some(&first_parent.id));
        assert_eq!(
            first_parent_children,
            vec![(third.id.clone(), 0), (first.id.clone(), 1)]
        );
        assert_eq!(
            sibling_rows(&tx, Some(&second_parent.id)),
            vec![(second.id.clone(), 0)]
        );
    }

    #[test]
    fn repair_removes_invalid_rows_and_restores_missing_knowledge_roots() {
        let (mut write, _) = open_in_memory().unwrap();
        let now = now_string();
        let tx = write.transaction().unwrap();
        let valid = create_knowledge(&tx, "Valid", &now).unwrap();
        let missing = create_knowledge(&tx, "Missing", &now).unwrap();
        let capture = EntriesRepo::create(&tx, "Capture", &now).unwrap();
        let trashed = create_knowledge(&tx, "Trashed", &now).unwrap();
        let trashed = EntriesRepo::move_to_trash(&tx, &trashed.id, trashed.revision, &now).unwrap();

        tx.execute(
            "INSERT INTO entry_hierarchy(entry_id, parent_entry_id, sibling_order, updated_at)
             VALUES (?1, NULL, 99, ?2)",
            params![capture.id, now],
        )
        .unwrap();
        tx.execute(
            "INSERT INTO entry_hierarchy(entry_id, parent_entry_id, sibling_order, updated_at)
             VALUES (?1, NULL, 99, ?2)",
            params![trashed.id, now],
        )
        .unwrap();
        tx.execute(
            "UPDATE entry_hierarchy SET parent_entry_id = ?1 WHERE entry_id = ?2",
            params![capture.id, valid.id],
        )
        .unwrap();
        tx.execute(
            "DELETE FROM entry_hierarchy WHERE entry_id = ?1",
            [&missing.id],
        )
        .unwrap();

        HierarchyRepo::repair(&tx, &now).unwrap();

        assert_eq!(
            sibling_rows(&tx, None),
            vec![(valid.id, 0), (missing.id, 1)]
        );
        let invalid_rows: i64 = tx
            .query_row(
                "SELECT COUNT(*) FROM entry_hierarchy
                 WHERE entry_id IN (?1, ?2)",
                params![capture.id, trashed.id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(invalid_rows, 0);
    }

    #[test]
    fn repair_breaks_existing_cycles_into_traversable_trees() {
        let (mut write, read) = open_in_memory().unwrap();
        let now = now_string();
        let tx = write.transaction().unwrap();
        let alpha = create_knowledge(&tx, "Alpha", &now).unwrap();
        let beta = create_knowledge(&tx, "Beta", &now).unwrap();
        let gamma = create_knowledge(&tx, "Gamma", &now).unwrap();
        tx.execute(
            "UPDATE entry_hierarchy SET parent_entry_id = ?1 WHERE entry_id = ?2",
            params![beta.id, alpha.id],
        )
        .unwrap();
        tx.execute(
            "UPDATE entry_hierarchy SET parent_entry_id = ?1 WHERE entry_id = ?2",
            params![gamma.id, beta.id],
        )
        .unwrap();
        tx.execute(
            "UPDATE entry_hierarchy SET parent_entry_id = ?1 WHERE entry_id = ?2",
            params![alpha.id, gamma.id],
        )
        .unwrap();

        HierarchyRepo::repair(&tx, &now).unwrap();
        tx.commit().unwrap();

        let tree = HierarchyRepo::tree(&read).unwrap();
        let root_id = [alpha.id.clone(), beta.id.clone(), gamma.id.clone()]
            .into_iter()
            .min()
            .unwrap();
        assert_eq!(tree.len(), 1);
        assert_eq!(tree[0].id, root_id);
        assert_eq!(
            HierarchyRepo::breadcrumbs(&read, &gamma.id)
                .unwrap()
                .last()
                .map(|item| item.id.as_str()),
            Some(gamma.id.as_str())
        );
    }

    #[test]
    fn restore_returns_knowledge_to_root_and_delete_promotes_remaining_children() {
        let (mut write, read) = open_in_memory().unwrap();
        let now = now_string();
        let tx = write.transaction().unwrap();
        let parent = create_knowledge(&tx, "Parent", &now).unwrap();
        let child = create_knowledge(&tx, "Child", &now).unwrap();
        let child =
            HierarchyRepo::move_entry(&tx, &child.id, Some(&parent.id), 0, child.revision, &now)
                .unwrap();
        let trashed = EntriesRepo::move_to_trash(&tx, &parent.id, parent.revision, &now).unwrap();
        let restored =
            EntriesRepo::restore_from_trash(&tx, &parent.id, trashed.revision, &now).unwrap();
        assert_eq!(sibling_rows(&tx, None)[0].0, child.id);
        assert!(sibling_rows(&tx, None)
            .iter()
            .any(|(id, _)| id == &restored.id));

        EntriesRepo::move_to_trash(&tx, &restored.id, restored.revision, &now).unwrap();
        tx.execute(
            "INSERT INTO entry_hierarchy(entry_id, parent_entry_id, sibling_order, updated_at)
             VALUES (?1, ?2, 0, ?3)",
            params![restored.id, child.id, now],
        )
        .unwrap();
        tx.execute(
            "UPDATE entry_hierarchy SET parent_entry_id = ?1 WHERE entry_id = ?2",
            params![restored.id, child.id],
        )
        .unwrap();

        EntriesRepo::delete_forever(&tx, &restored.id).unwrap();
        tx.commit().unwrap();

        let tree = HierarchyRepo::tree(&read).unwrap();
        assert_eq!(tree.len(), 1);
        assert_eq!(tree[0].id, child.id);
        assert!(tree[0].children.is_empty());
    }

    #[test]
    fn demote_rejects_non_empty_tree_and_trash_promotes_children() {
        let (mut write, read) = open_in_memory().unwrap();
        let now = now_string();
        let tx = write.transaction().unwrap();
        let root = create_knowledge(&tx, "Root", &now).unwrap();
        let child = create_knowledge(&tx, "Child", &now).unwrap();
        let child =
            HierarchyRepo::move_entry(&tx, &child.id, Some(&root.id), 0, child.revision, &now)
                .unwrap();

        let demote = KnowledgeRepo::demote(&tx, &root.id, root.revision, &now).unwrap_err();
        assert!(matches!(
            demote,
            AppError::Validation {
                code: "HIERARCHY_HAS_CHILDREN",
                ..
            }
        ));
        EntriesRepo::move_to_trash(&tx, &root.id, root.revision, &now).unwrap();
        tx.commit().unwrap();

        let tree = HierarchyRepo::tree(&read).unwrap();
        assert_eq!(tree.len(), 1);
        assert_eq!(tree[0].id, child.id);
        assert!(tree[0].children.is_empty());
        let state: KnowledgeState = read
            .query_row(
                "SELECT knowledge_state FROM entries WHERE id = ?1",
                [&child.id],
                |row| {
                    let value: String = row.get(0)?;
                    Ok(KnowledgeState::from_db(&value).unwrap())
                },
            )
            .unwrap();
        assert_eq!(state, KnowledgeState::Knowledge);
    }
}
