import importlib.util
import json
import sqlite3
import tempfile
import unittest
from pathlib import Path

SCRIPT = Path(__file__).with_name("installed-nsis-evidence.py")
SPEC = importlib.util.spec_from_file_location("installed_nsis_evidence", SCRIPT)
assert SPEC and SPEC.loader
MODULE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MODULE)


class InstalledNsisEvidenceTest(unittest.TestCase):
    def create_database(self, path):
        connection = sqlite3.connect(path)
        connection.executescript(
            """
            CREATE TABLE entries (
              id TEXT PRIMARY KEY, title TEXT, revision INTEGER, deleted_at TEXT,
              knowledge_state TEXT, current_content TEXT
            );
            CREATE TABLE entry_documents (
              entry_id TEXT PRIMARY KEY, schema_version INTEGER, entry_revision INTEGER,
              document_json TEXT, markdown_text TEXT, plain_text TEXT
            );
            CREATE TABLE blocks (
              id TEXT PRIMARY KEY, entry_id TEXT, parent_block_id TEXT, ordinal INTEGER,
              depth INTEGER, kind TEXT, text_content TEXT, attrs_json TEXT
            );
            CREATE TABLE entry_links (
              source_entry_id TEXT, ordinal INTEGER, raw_target TEXT,
              normalized_target TEXT, target_entry_id TEXT
            );
            CREATE VIRTUAL TABLE entries_fts USING fts5(
              entry_id UNINDEXED, title, original_content, current_content, tags_text, aliases_text
            );
            CREATE TABLE entry_hierarchy (
              entry_id TEXT PRIMARY KEY, parent_entry_id TEXT, sibling_order INTEGER
            );
            CREATE TABLE entry_imports (
              entry_id TEXT PRIMARY KEY, canonical_source_path TEXT, source_hash TEXT,
              source_entry_id TEXT, imported_at TEXT
            );
            """
        )
        connection.executemany(
            "INSERT INTO entries VALUES (?, ?, ?, ?, ?, ?)",
            [
                ("target", "Run Target", 3, None, "knowledge", "Target content"),
                ("source", "Run Source", 5, None, "knowledge", "Run Heading\nRun token [[Run Target]] [[Missing]]"),
                ("imported", "Imported", 1, None, "capture", "Imported content"),
                ("quick-capture", "Quick Capture", 2, None, "capture", "Quick Capture"),
            ],
        )
        connection.executemany(
            "INSERT INTO entry_documents VALUES (?, ?, ?, ?, ?, ?)",
            [
                ("target", 1, 3, '{"blocks": []}', "Target content", "Target content"),
                ("source", 1, 5, '{"blocks": [{"id": "heading"}]}', "# Run Heading\n\nRun token", "Run Heading\nRun token"),
                ("imported", 1, 1, '{"blocks": []}', "Imported content", "Imported content"),
                ("quick-capture", 1, 2, '{"blocks": []}', "Quick Capture", "Quick Capture"),
            ],
        )
        connection.executemany(
            "INSERT INTO blocks VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
            [
                ("target-block", "target", None, 0, 0, "Paragraph", "Target content", "{}"),
                ("heading", "source", None, 0, 0, "Heading", "Run Heading", '{"level": 1}'),
                ("source-block", "source", None, 1, 0, "Paragraph", "Run token [[Run Target]] [[Missing]]", "{}"),
            ],
        )
        connection.executemany(
            "INSERT INTO entry_links VALUES (?, ?, ?, ?, ?)",
            [
                ("source", 0, "Run Target", "run target", "target"),
                ("source", 1, "Missing", "missing", None),
            ],
        )
        connection.executemany(
            "INSERT INTO entries_fts VALUES (?, ?, ?, ?, ?, ?)",
            [
                ("target", "Run Target", "", "Target content", "", ""),
                ("source", "Run Source", "", "Run Heading Run token", "", ""),
                ("imported", "Imported", "", "Imported content", "", ""),
                ("quick-capture", "Quick Capture", "", "Quick Capture", "", ""),
            ],
        )
        connection.executemany(
            "INSERT INTO entry_hierarchy VALUES (?, ?, ?)",
            [
                ("target", None, 0),
                ("source", None, 1),
                ("imported", None, 2),
                ("quick-capture", None, 3),
            ],
        )
        connection.execute(
            "INSERT INTO entry_imports VALUES (?, ?, ?, ?, ?)",
            ("imported", "C:/fixtures/01.md", "hash", None, "2026-07-29T00:00:00Z"),
        )
        connection.commit()
        connection.close()

    def scenario(self):
        return {
            "runId": "run-123",
            "targetId": "target",
            "sourceId": "source",
            "inputs": {
                "targetTitle": "Run Target",
                "sourceTitle": "Run Source",
                "heading": "Run Heading",
                "undo": "Run token",
                "missing": "Missing",
            },
            "quickCapture": {"title": "Quick Capture", "entryId": "quick-capture"},
            "stages": {
                "imports": {"entryIds": ["imported"], "expectedCount": 1},
                "export": {"manifest": "export-manifest.json"},
                "backup": {
                    "baselineTitle": "Run Target",
                    "afterBackupId": "after-backup",
                    "afterBackupTitle": "Absent After Backup",
                },
            },
        }

    def add_after_backup_entry(self, database):
        connection = sqlite3.connect(database)
        try:
            connection.execute(
                "INSERT INTO entries VALUES (?, ?, ?, ?, ?, ?)",
                ("after-backup", "Absent After Backup", 2, None, "capture", "Mutation content"),
            )
            connection.commit()
        finally:
            connection.close()

    def remove_after_backup_entry(self, database):
        connection = sqlite3.connect(database)
        try:
            connection.execute("DELETE FROM entries WHERE id = ?", ("after-backup",))
            connection.commit()
        finally:
            connection.close()

    def test_writes_passing_evidence_for_available_stages(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            database = root / "2notes.sqlite"
            self.create_database(database)
            self.add_after_backup_entry(database)
            scenario_path = root / "scenario.json"
            scenario_path.write_text(json.dumps(self.scenario()), encoding="utf-8")
            pre_restore = MODULE.verify_pre_restore(database, scenario_path, root)
            self.assertTrue(pre_restore["ok"])
            self.remove_after_backup_entry(database)
            (root / "exported.md").write_text("Run Target\nRun Source", encoding="utf-8")
            (root / "export-manifest.json").write_text(
                json.dumps({"files": ["exported.md"]}), encoding="utf-8"
            )

            result = MODULE.verify("run-123", database, scenario_path, root)

            self.assertTrue(result["ok"])
            self.assertTrue((root / "pre-restore-evidence.json").exists())
            self.assertTrue((root / "final-evidence.json").exists())
            phases = {phase["name"]: phase for phase in result["phases"]}
            self.assertTrue(phases["scenario_contract"]["ok"])
            self.assertTrue(phases["quick_capture_restart"]["ok"])
            self.assertTrue(phases["core_entries"]["ok"])
            self.assertTrue(phases["documents_blocks"]["ok"])
            self.assertTrue(phases["links_fts"]["ok"])
            self.assertTrue(phases["imports"]["ok"])
            self.assertTrue(phases["backup_restore"]["ok"])
            self.assertTrue(phases["export_manifest"]["ok"])

    def test_pre_restore_helper_rejects_mutation_that_was_never_persisted(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            database = root / "2notes.sqlite"
            self.create_database(database)
            scenario_path = root / "scenario.json"
            scenario_path.write_text(json.dumps(self.scenario()), encoding="utf-8")

            result = MODULE.verify_pre_restore(database, scenario_path, root)

            self.assertFalse(result["ok"])
            self.assertTrue((root / "pre-restore-evidence.json").exists())

    def test_rejects_missing_pre_restore_evidence(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            database = root / "2notes.sqlite"
            self.create_database(database)
            scenario_path = root / "scenario.json"
            scenario_path.write_text(json.dumps(self.scenario()), encoding="utf-8")

            result = MODULE.verify("run-123", database, scenario_path, root)

            self.assertFalse(result["ok"])
            phases = {phase["name"]: phase for phase in result["phases"]}
            self.assertFalse(phases["backup_restore"]["ok"])

    def test_rejects_missing_or_mismatched_quick_capture(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            database = root / "2notes.sqlite"
            self.create_database(database)
            scenario = self.scenario()
            scenario["quickCapture"]["title"] = "Missing Quick Capture"
            scenario_path = root / "scenario.json"
            scenario_path.write_text(json.dumps(scenario), encoding="utf-8")

            result = MODULE.verify("run-123", database, scenario_path, root)

            self.assertFalse(result["ok"])
            phases = {phase["name"]: phase for phase in result["phases"]}
            self.assertFalse(phases["quick_capture_restart"]["ok"])

    def test_returns_failure_without_writing_to_database(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            database = root / "2notes.sqlite"
            self.create_database(database)
            scenario = self.scenario()
            scenario["inputs"]["heading"] = "Absent heading"
            scenario_path = root / "scenario.json"
            scenario_path.write_text(json.dumps(scenario), encoding="utf-8")

            result = MODULE.verify("run-123", database, scenario_path, root)

            self.assertFalse(result["ok"])
            connection = sqlite3.connect(database)
            try:
                self.assertEqual(connection.execute("PRAGMA query_only").fetchone()[0], 0)
            finally:
                connection.close()
            self.assertIn("Absent heading", json.dumps(result))

    def test_rejects_empty_scenario_instead_of_skipping_required_phases(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            database = root / "2notes.sqlite"
            self.create_database(database)
            scenario_path = root / "scenario.json"
            scenario_path.write_text("{}", encoding="utf-8")

            result = MODULE.verify("run-123", database, scenario_path, root)

            self.assertFalse(result["ok"])
            phases = {phase["name"]: phase for phase in result["phases"]}
            self.assertFalse(phases["scenario_contract"]["ok"])
            self.assertFalse(phases["core_entries"]["skipped"])
            self.assertFalse(phases["export_manifest"]["skipped"])

    def test_rejects_missing_required_backup_stage(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            database = root / "2notes.sqlite"
            self.create_database(database)
            scenario = self.scenario()
            del scenario["stages"]["backup"]
            scenario_path = root / "scenario.json"
            scenario_path.write_text(json.dumps(scenario), encoding="utf-8")

            result = MODULE.verify("run-123", database, scenario_path, root)

            self.assertFalse(result["ok"])
            phases = {phase["name"]: phase for phase in result["phases"]}
            self.assertFalse(phases["scenario_contract"]["ok"])
            self.assertFalse(phases["backup_restore"]["ok"])
            self.assertFalse(phases["backup_restore"]["skipped"])


if __name__ == "__main__":
    unittest.main()
