#!/usr/bin/env python3
"""Verify installed NSIS smoke persistence without mutating its SQLite database."""

import argparse
import json
import sqlite3
import sys
from pathlib import Path


def scalar(connection, sql, parameters=()):
    return connection.execute(sql, parameters).fetchone()[0]


def phase(name, checks, skipped=False):
    failures = [message for ok, message, _ in checks if not ok]
    return {
        "name": name,
        "ok": not failures,
        "skipped": skipped,
        "checks": [
            {"ok": ok, "message": message, "result": result}
            for ok, message, result in checks
        ],
        "failures": failures,
    }


def optional(mapping, *keys):
    value = mapping
    for key in keys:
        if not isinstance(value, dict) or key not in value:
            return None
        value = value[key]
    return value


def scenario_contract_checks(scenario, evidence_dir=None):
    inputs = scenario.get("inputs")
    stages = scenario.get("stages")
    checks = [
        (isinstance(scenario.get("sourceId"), str) and bool(scenario["sourceId"]), "scenario has sourceId", scenario.get("sourceId")),
        (isinstance(scenario.get("targetId"), str) and bool(scenario["targetId"]), "scenario has targetId", scenario.get("targetId")),
        (isinstance(inputs, dict), "scenario has inputs object", inputs),
        (isinstance(stages, dict), "scenario has stages object", stages),
    ]
    if isinstance(inputs, dict):
        for key in ("sourceTitle", "targetTitle", "heading", "undo", "missing"):
            checks.append((isinstance(inputs.get(key), str) and bool(inputs[key]), f"inputs has {key}", inputs.get(key)))
    if isinstance(stages, dict):
        for key in ("imports", "export", "backup"):
            checks.append((isinstance(stages.get(key), dict), f"stages has {key}", stages.get(key)))
        backup = stages.get("backup")
        if isinstance(backup, dict):
            for key in ("afterBackupId", "afterBackupTitle"):
                checks.append(
                    (
                        isinstance(backup.get(key), str) and bool(backup[key]),
                        f"backup has {key}",
                        backup.get(key),
                    )
                )
    if evidence_dir is not None:
        pre_restore, pre_restore_checks_result = load_pre_restore_evidence(evidence_dir)
        checks.extend(pre_restore_checks_result)
        backup = optional(scenario, "stages", "backup")
        mutation = pre_restore.get("mutation") if isinstance(pre_restore, dict) and isinstance(pre_restore.get("mutation"), dict) else {}
        if isinstance(backup, dict):
            checks.extend(
                [
                    (mutation.get("id") == backup.get("afterBackupId"), "pre-restore id matches scenario contract", {"scenario": backup.get("afterBackupId"), "evidence": mutation.get("id")}),
                    (mutation.get("title") == backup.get("afterBackupTitle"), "pre-restore title matches scenario contract", {"scenario": backup.get("afterBackupTitle"), "evidence": mutation.get("title")}),
                    (isinstance(mutation.get("revision"), int) and mutation["revision"] > 0, "pre-restore revision matches scenario contract", mutation.get("revision")),
                ]
            )
    quick_capture = scenario.get("quickCapture")
    checks.append((isinstance(quick_capture, dict), "scenario has quickCapture object", quick_capture))
    if isinstance(quick_capture, dict):
        for key in ("title", "entryId"):
            checks.append(
                (
                    isinstance(quick_capture.get(key), str) and bool(quick_capture[key]),
                    f"quickCapture has {key}",
                    quick_capture.get(key),
                )
            )
    return checks, False


def quick_capture_checks(connection, scenario):
    quick_capture = scenario.get("quickCapture") if isinstance(scenario.get("quickCapture"), dict) else {}
    title = quick_capture.get("title")
    entry_id = quick_capture.get("entryId")
    if not title or not entry_id:
        return [(False, "quick-capture title and UI entry id are required", quick_capture)], False
    rows = connection.execute(
        """
        SELECT id, title, current_content, deleted_at
        FROM entries
        WHERE title = ?
        """,
        (title,),
    ).fetchall()
    checks = [(len(rows) == 1, "quick-capture entry exists exactly once", [dict(row) for row in rows])]
    if len(rows) == 1:
        row = rows[0]
        checks.extend(
            [
                (row["deleted_at"] is None, "quick-capture entry is active after restart", row["deleted_at"]),
                (title in row["current_content"], "quick-capture content matches emitted title", row["current_content"]),
            ]
        )
        checks.append((row["id"] == entry_id, "quick-capture UI selection matches SQLite entry", {"ui": entry_id, "sqlite": row["id"]}))
    return checks, False


def entry_checks(connection, scenario):
    inputs = scenario.get("inputs") if isinstance(scenario.get("inputs"), dict) else {}
    roles = (
        ("target", scenario.get("targetId"), inputs.get("targetTitle")),
        ("source", scenario.get("sourceId"), inputs.get("sourceTitle")),
    )
    checks = []
    for role, entry_id, title in roles:
        if not entry_id:
            checks.append((False, f"{role} entry id is required", entry_id))
            continue
        row = connection.execute(
            """
            SELECT id, title, revision, deleted_at, knowledge_state
            FROM entries WHERE id = ?
            """,
            (entry_id,),
        ).fetchone()
        checks.append((row is not None, f"{role} entry exists", dict(row) if row else None))
        if row:
            if title:
                checks.append((row["title"] == title, f"{role} title matches", row["title"]))
            checks.append((row["revision"] > 0, f"{role} revision is persisted", row["revision"]))
            checks.append((row["knowledge_state"] == "knowledge", f"{role} is knowledge", row["knowledge_state"]))
            checks.append((row["deleted_at"] is None, f"{role} is active after restore", row["deleted_at"]))
    return checks, False


def document_block_checks(connection, scenario):
    source_id = scenario.get("sourceId")
    if not source_id:
        return [(False, "source id is required for document checks", source_id)], False
    inputs = scenario.get("inputs") if isinstance(scenario.get("inputs"), dict) else {}
    row = connection.execute(
        """
        SELECT schema_version, entry_revision, document_json, markdown_text, plain_text
        FROM entry_documents WHERE entry_id = ?
        """,
        (source_id,),
    ).fetchone()
    checks = [(row is not None, "source document exists", dict(row) if row else None)]
    if not row:
        return checks, False
    checks.extend(
        [
            (row["schema_version"] == 1, "document schema version is 1", row["schema_version"]),
            (row["entry_revision"] > 0, "document revision is persisted", row["entry_revision"]),
        ]
    )
    for field, label in (("heading", "heading"), ("undo", "undo token")):
        expected = inputs.get(field)
        if expected:
            found = expected in row["markdown_text"] or expected in row["plain_text"]
            checks.append((found, f"{label} appears in document projection", expected))
    heading = inputs.get("heading")
    block_rows = connection.execute(
        "SELECT id, kind, text_content FROM blocks WHERE entry_id = ? ORDER BY ordinal",
        (source_id,),
    ).fetchall()
    checks.append((bool(block_rows), "source blocks exist", [dict(item) for item in block_rows]))
    if heading:
        checks.append(
            (
                any(item["text_content"] == heading and "heading" in item["kind"].lower() for item in block_rows),
                "heading block is projected",
                heading,
            )
        )
    return checks, False


def link_fts_checks(connection, scenario):
    source_id = scenario.get("sourceId")
    target_id = scenario.get("targetId")
    if not source_id or not target_id:
        return [(False, "source and target ids are required for link checks", {"sourceId": source_id, "targetId": target_id})], False
    inputs = scenario.get("inputs") if isinstance(scenario.get("inputs"), dict) else {}
    links = connection.execute(
        """
        SELECT raw_target, target_entry_id FROM entry_links
        WHERE source_entry_id = ? ORDER BY ordinal
        """,
        (source_id,),
    ).fetchall()
    checks = []
    target_title = inputs.get("targetTitle")
    missing = inputs.get("missing")
    if target_title and target_id:
        checks.append(
            (
                any(row["raw_target"] == target_title and row["target_entry_id"] == target_id for row in links),
                "resolved WikiLink points to target",
                [dict(item) for item in links],
            )
        )
    if missing:
        checks.append(
            (
                any(row["raw_target"] == missing and row["target_entry_id"] is None for row in links),
                "unresolved WikiLink remains unresolved",
                [dict(item) for item in links],
            )
        )
    token = inputs.get("undo")
    if token:
        fts_count = scalar(
            connection,
            "SELECT count(*) FROM entries_fts WHERE entry_id = ? AND current_content LIKE ?",
            (source_id, f"%{token}%"),
        )
        checks.append((fts_count == 1, "FTS projection contains undo token", fts_count))
    return checks, False


def hierarchy_checks(connection, scenario):
    source_id = scenario.get("sourceId")
    target_id = scenario.get("targetId")
    ids = [entry_id for entry_id in (source_id, target_id) if entry_id]
    if len(ids) != 2:
        return [(False, "source and target ids are required for hierarchy checks", ids)], False
    placeholders = ", ".join("?" for _ in ids)
    rows = connection.execute(
        f"SELECT entry_id, parent_entry_id FROM entry_hierarchy WHERE entry_id IN ({placeholders})",
        ids,
    ).fetchall()
    roots = {row["entry_id"]: row["parent_entry_id"] for row in rows}
    return [
        (len(rows) == len(ids), "hierarchy records exist", [dict(row) for row in rows]),
        (all(roots.get(entry_id) is None for entry_id in ids), "restored entries are roots", roots),
    ], False


def import_checks(connection, scenario):
    stage = optional(scenario, "stages", "imports")
    if not isinstance(stage, dict):
        return [(False, "imports stage is required", stage)], False
    entry_ids = stage.get("entryIds", [])
    expected_count = stage.get("expectedCount", len(entry_ids))
    if entry_ids:
        placeholders = ", ".join("?" for _ in entry_ids)
        count = scalar(connection, f"SELECT count(*) FROM entry_imports WHERE entry_id IN ({placeholders})", entry_ids)
    else:
        count = scalar(connection, "SELECT count(*) FROM entry_imports")
    return [
        (count == expected_count, "imports retain one record per imported entry", count),
    ], False


def export_checks(scenario, evidence_dir):
    stage = optional(scenario, "stages", "export")
    if not isinstance(stage, dict):
        return [(False, "export stage is required", stage)], False
    manifest_name = stage.get("manifest", "export-manifest.json")
    manifest_path = evidence_dir / manifest_name
    checks = [(manifest_path.is_file(), "export manifest exists", str(manifest_path))]
    if not manifest_path.is_file():
        return checks, False
    try:
        manifest = json.loads(manifest_path.read_text(encoding="utf-8-sig"))
        files = manifest.get("files", manifest if isinstance(manifest, list) else [])
        listed_files = [item if isinstance(item, str) else item.get("path") for item in files]
        paths = [Path(item) if Path(item).is_absolute() else evidence_dir / item for item in listed_files if item]
        missing = [str(path) for path in paths if not path.is_file()]
        checks.append((bool(listed_files), "export manifest lists files", listed_files))
        checks.append((not missing, "listed export files exist", missing))
        expected_titles = [stage.get("sourceTitle"), stage.get("targetTitle")]
        exported_text = "\n".join(path.read_text(encoding="utf-8") for path in paths if path.is_file())
        for title in expected_titles:
            if title:
                checks.append((title in exported_text, f"export contains {title}", title))
    except (OSError, json.JSONDecodeError) as error:
        checks.append((False, "export manifest is valid JSON", str(error)))
    return checks, False


def load_pre_restore_evidence(evidence_dir):
    path = evidence_dir / "pre-restore-evidence.json"
    if not path.is_file():
        return None, [(False, "pre-restore evidence exists", str(path))]
    try:
        evidence = json.loads(path.read_text(encoding="utf-8-sig"))
    except (OSError, json.JSONDecodeError) as error:
        return None, [(False, "pre-restore evidence is valid JSON", str(error))]
    return evidence, [(evidence.get("ok") is True, "pre-restore evidence is ok", evidence.get("ok"))]


def pre_restore_checks(connection, scenario):
    stage = optional(scenario, "stages", "backup")
    if not isinstance(stage, dict):
        return [(False, "backup stage is required", stage)]
    entry_id = stage.get("afterBackupId")
    title = stage.get("afterBackupTitle")
    if not entry_id or not title:
        return [(False, "backup mutation id and title are required", {"id": entry_id, "title": title})]
    rows = connection.execute(
        "SELECT id, title, revision, deleted_at FROM entries WHERE id = ?",
        (entry_id,),
    ).fetchall()
    checks = [(len(rows) == 1, "post-backup entry id exists exactly once", [dict(row) for row in rows])]
    if len(rows) == 1:
        row = rows[0]
        checks.extend(
            [
                (row["title"] == title, "post-backup entry title matches", row["title"]),
                (row["revision"] > 0, "post-backup entry revision is persisted", row["revision"]),
                (row["deleted_at"] is None, "post-backup entry is active", row["deleted_at"]),
            ]
        )
    title_rows = connection.execute("SELECT id FROM entries WHERE title = ?", (title,)).fetchall()
    checks.append((len(title_rows) == 1 and title_rows[0]["id"] == entry_id, "post-backup title identifies the mutation id", [dict(row) for row in title_rows]))
    return checks


def backup_restore_checks(connection, scenario, evidence_dir):
    stage = optional(scenario, "stages", "backup")
    if not isinstance(stage, dict):
        return [(False, "backup stage is required", stage)], False
    baseline_title = stage.get("baselineTitle")
    after_id = stage.get("afterBackupId")
    after_title = stage.get("afterBackupTitle")
    backup_dir_value = stage.get("backupDir")
    backup_dir = Path(backup_dir_value) if backup_dir_value else None
    checks = []
    if baseline_title:
        baseline_count = scalar(connection, "SELECT count(*) FROM entries WHERE title = ?", (baseline_title,))
        checks.append((baseline_count == 1, "backup baseline entry remains after restore", baseline_count))
    pre_restore, pre_restore_checks_result = load_pre_restore_evidence(evidence_dir)
    checks.extend(pre_restore_checks_result)
    if pre_restore is not None:
        mutation = pre_restore.get("mutation") if isinstance(pre_restore.get("mutation"), dict) else {}
        checks.extend(
            [
                (mutation.get("id") == after_id, "pre-restore id matches scenario", {"scenario": after_id, "evidence": mutation.get("id")}),
                (mutation.get("title") == after_title, "pre-restore title matches scenario", {"scenario": after_title, "evidence": mutation.get("title")}),
                (isinstance(mutation.get("revision"), int) and mutation["revision"] > 0, "pre-restore revision is persisted", mutation.get("revision")),
            ]
        )
    if after_id:
        after_rows = connection.execute("SELECT id, title FROM entries WHERE id = ?", (after_id,)).fetchall()
        checks.append((not after_rows, "post-backup entry id is absent after restore", [dict(row) for row in after_rows]))
    if backup_dir is not None:
        snapshots = sorted(str(path) for path in backup_dir.glob("*.sqlite*"))
        checks.append((bool(snapshots), "backup directory contains SQLite snapshots", snapshots))
        checks.append((any("before_restore" in Path(path).name for path in snapshots), "before_restore snapshot exists", snapshots))
    return checks, False


def verify_pre_restore(database_path, scenario_path, evidence_dir):
    database_path = Path(database_path).resolve()
    scenario_path = Path(scenario_path).resolve()
    evidence_dir = Path(evidence_dir).resolve()
    evidence_dir.mkdir(parents=True, exist_ok=True)
    output = {"database": str(database_path), "scenario": str(scenario_path), "phases": []}
    connection = None
    try:
        scenario = json.loads(scenario_path.read_text(encoding="utf-8-sig"))
        connection = sqlite3.connect(f"file:{database_path.as_posix()}?mode=ro", uri=True)
        connection.row_factory = sqlite3.Row
        connection.execute("PRAGMA query_only = ON")
        checks = pre_restore_checks(connection, scenario)
        output["phases"].append(phase("pre_restore_mutation", checks))
        rows = connection.execute(
            "SELECT id, title, revision FROM entries WHERE id = ?",
            (optional(scenario, "stages", "backup", "afterBackupId"),),
        ).fetchall()
        if len(rows) == 1:
            output["mutation"] = dict(rows[0])
    except (OSError, sqlite3.Error, json.JSONDecodeError) as error:
        output["phases"].append(phase("setup", [(False, "pre-restore evidence setup failed", str(error))]))
    finally:
        if connection is not None:
            connection.close()
    output["ok"] = len(output["phases"]) == 1 and output["phases"][0]["ok"]
    (evidence_dir / "pre-restore-evidence.json").write_text(
        json.dumps(output, ensure_ascii=False, indent=2) + "\n", encoding="utf-8"
    )
    return output


def verify(run_id, database_path, scenario_path, evidence_dir):
    database_path = Path(database_path).resolve()
    scenario_path = Path(scenario_path).resolve()
    evidence_dir = Path(evidence_dir).resolve()
    evidence_dir.mkdir(parents=True, exist_ok=True)
    scenario = json.loads(scenario_path.read_text(encoding="utf-8"))
    output = {
        "runId": run_id,
        "database": str(database_path),
        "scenario": str(scenario_path),
        "phases": [],
    }
    connection = None
    try:
        connection = sqlite3.connect(f"file:{database_path.as_posix()}?mode=ro", uri=True)
        connection.row_factory = sqlite3.Row
        connection.execute("PRAGMA query_only = ON")
        contract_checks, contract_skipped = scenario_contract_checks(scenario, evidence_dir)
        output["phases"].append(phase("scenario_contract", contract_checks, contract_skipped))
        for name, checker in (
            ("quick_capture_restart", quick_capture_checks),
            ("core_entries", entry_checks),
            ("documents_blocks", document_block_checks),
            ("links_fts", link_fts_checks),
            ("hierarchy_trash", hierarchy_checks),
            ("imports", import_checks),
            ("backup_restore", backup_restore_checks),
        ):
            if name == "backup_restore":
                checks, skipped = checker(connection, scenario, evidence_dir)
            else:
                checks, skipped = checker(connection, scenario)
            output["phases"].append(phase(name, checks, skipped))
        checks, skipped = export_checks(scenario, evidence_dir)
        output["phases"].append(phase("export_manifest", checks, skipped))
    except (OSError, sqlite3.Error, json.JSONDecodeError) as error:
        output["phases"].append(phase("setup", [(False, "evidence setup failed", str(error))]))
    finally:
        if connection is not None:
            connection.close()
    required_phases = {
        "scenario_contract",
        "quick_capture_restart",
        "core_entries",
        "documents_blocks",
        "links_fts",
        "hierarchy_trash",
        "imports",
        "backup_restore",
        "export_manifest",
    }
    phase_results = {item["name"]: item for item in output["phases"]}
    output["ok"] = all(
        name in phase_results
        and not phase_results[name]["skipped"]
        and phase_results[name]["ok"]
        for name in required_phases
    )
    (evidence_dir / "final-evidence.json").write_text(
        json.dumps(output, ensure_ascii=False, indent=2) + "\n", encoding="utf-8"
    )
    return output


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--run-id", required=True)
    parser.add_argument("--db", required=True, type=Path)
    parser.add_argument("--scenario-ids", required=True, type=Path)
    parser.add_argument("--evidence-dir", required=True, type=Path)
    parser.add_argument("--mode", choices=("final", "pre-restore"), default="final")
    arguments = parser.parse_args(argv)
    if arguments.mode == "pre-restore":
        result = verify_pre_restore(arguments.db, arguments.scenario_ids, arguments.evidence_dir)
    else:
        result = verify(arguments.run_id, arguments.db, arguments.scenario_ids, arguments.evidence_dir)
    print(json.dumps(result, ensure_ascii=False, indent=2))
    return 0 if result["ok"] else 1


if __name__ == "__main__":
    sys.exit(main())
