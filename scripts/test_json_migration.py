"""Synthetic-only regression coverage for the offline migration tool."""
import importlib.util
import hashlib
import json
from pathlib import Path
import sqlite3
import tempfile
import unittest
from unittest import mock

HERE = Path(__file__).parent
spec = importlib.util.spec_from_file_location("migration", HERE / "migrate-sqlite-to-json.py")
m = importlib.util.module_from_spec(spec)
spec.loader.exec_module(m)

class MigrationTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.db = sqlite3.connect(self.root / "db.sqlite")
        self.addCleanup(self.db.close)
        self.db.executescript((HERE / "legacy-json-migration-schema.sql").read_text())
        self.db.execute("INSERT INTO skills(id,name,file_path,is_central,scanned_at) VALUES('skill','Synthetic','/fixture/SKILL.md',1,'now')")
        self.db.execute("INSERT INTO skill_installations(skill_id,agent_id,installed_path,link_type,is_managed,created_at) VALUES('skill','agent','/fixture','symlink',1,'first')")
        self.db.execute("INSERT INTO folder_notes VALUES('folder','Note','[\"skill\"]')")
        note = {"id":1,"sourceKey":"github:fixture/repo","repositoryUid":"00000000-0000-4000-8000-000000000001","fullName":"fixture/repo","url":"","description":"","tags":[],"stars":None,"skillIds":["skill"],"notebook":"Text","obsidian":None}
        self.db.execute("INSERT INTO repository_notebooks VALUES(1,?,?,?)", (note["sourceKey"],note["repositoryUid"],json.dumps(note)))
        self.db.execute("INSERT INTO agents(id,display_name,category,global_skills_dir,is_enabled) VALUES('agent','Synthetic','platform','/fixture',1)")
        self.db.execute("INSERT INTO skill_sources(skill_id,source_type,source_repo,updated_at) VALUES('skill','github','fixture/repo','now')")
        self.db.execute("INSERT INTO skill_source_syncs(skill_id,remote_deleted,updated_at) VALUES('skill',1,'now')")
        self.db.execute("INSERT INTO skill_metadata VALUES('skill','note','[\"tag\"]','now')")
        self.db.execute("INSERT INTO collections VALUES('collection','Synthetic',NULL,'now','now')")
        self.db.execute("INSERT INTO collection_skills VALUES('collection','skill','now')")
        self.db.execute("INSERT INTO skill_explanations(skill_id,lang,explanation,created_at,updated_at) VALUES('skill','zh','cached','now','now')")
        self.db.execute("INSERT INTO github_repository_stats VALUES('Fixture/Repo',3,'now')")
        self.db.execute("INSERT INTO repository_mirrors VALUES('fixture/repo',NULL)")
        self.db.execute("INSERT INTO repository_local_directories VALUES('fixture/repo','fixture/repo')")
        self.db.execute("INSERT INTO import_transaction_receipts VALUES('receipt')")
        self.db.execute("INSERT INTO agent_skill_observations(row_id,agent_id,skill_id,name,file_path,dir_path,source_kind,source_root,link_type,is_read_only,scanned_at) VALUES('observation','agent','skill','Synthetic','/fixture/SKILL.md','/fixture','direct','/fixture','native',1,'now')")
        self.db.commit()
        (self.root / "config.json").write_text('{"settings":{}}')
        (self.root / "asset.txt").write_text("synthetic asset")

    def table(self, name):
        manifest = json.loads((self.root / "store/manifest.json").read_bytes())
        path = self.root / "store/objects" / (manifest["tables"][name] + ".json")
        self.assertEqual(hashlib.sha256(path.read_bytes()).hexdigest(), manifest["tables"][name])
        return json.loads(path.read_bytes())

    def test_all_tables_roundtrip_and_source_unchanged(self):
        before = {p.name:p.read_bytes() for p in self.root.iterdir() if p.is_file()}
        counts = m.migrate(self.root, True)
        self.assertEqual(set(counts), m.TABLES | {"store_counters"})
        self.assertIs(self.table("skills")["skill"]["is_central"], True)
        install = self.table("skill_installations")['["skill","agent"]']
        self.assertIs(install["is_managed"], True)
        self.assertEqual(install["created_at"], "first")
        self.assertEqual(self.table("folder_notes")["folder"]["skillIds"], ["skill"])
        self.assertEqual(self.table("repository_notebooks")["1"]["notebook"], "Text")
        for name, data in before.items(): self.assertEqual((self.root/name).read_bytes(), data)
        with self.assertRaises(ValueError): m.migrate(self.root, True)

    def test_deleted_high_id_sequence_preserved(self):
        self.db.execute("UPDATE sqlite_sequence SET seq=42 WHERE name='repository_notebooks'")
        self.db.commit()
        m.migrate(self.root, True)
        self.assertEqual(self.table("store_counters")["repository_notebooks"]["next_id"],43)

    def test_requires_offline_confirmation(self):
        with self.assertRaises(ValueError): m.migrate(self.root)
        self.assertFalse((self.root / "store").exists())

    def test_pending_address_rejected(self):
        self.db.execute("INSERT INTO repository_address_moves VALUES('pending','{}',0)")
        self.db.commit()
        with self.assertRaises(ValueError): m.migrate(self.root, True)
        self.assertFalse((self.root / "store").exists())

    def test_unknown_table_and_column_rejected(self):
        self.db.execute("CREATE TABLE future(id TEXT)")
        self.db.commit()
        with self.assertRaises(ValueError): m.migrate(self.root, True)
        self.db.execute("DROP TABLE future")
        self.db.execute("ALTER TABLE skills ADD COLUMN future TEXT")
        self.db.commit()
        with self.assertRaises(ValueError): m.migrate(self.root, True)

    def test_incomplete_import_rejected(self):
        (self.root / ".skillshub-import-fixture").mkdir()
        with self.assertRaises(ValueError): m.migrate(self.root, True)

    def test_failed_write_does_not_publish(self):
        with mock.patch.object(m, "write", side_effect=OSError("synthetic disk full")):
            with self.assertRaises(OSError): m.migrate(self.root, True)
        self.assertFalse((self.root / "store").exists())
        self.assertFalse(list(self.root.glob(".json-migration-*")))

    def test_committed_wal_included(self):
        self.db.execute("PRAGMA journal_mode=WAL")
        self.db.execute("PRAGMA wal_autocheckpoint=0")
        self.db.execute("INSERT INTO skills(id,name,file_path,scanned_at) VALUES('wal','WAL','/fixture/WAL.md','now')")
        self.db.commit()
        self.assertTrue((self.root / "db.sqlite-wal").exists())
        before = (self.root / "db.sqlite-wal").read_bytes()
        m.migrate(self.root, True)
        self.assertIn("wal", self.table("skills"))
        self.assertEqual(before,(self.root / "db.sqlite-wal").read_bytes())

    def test_failed_snapshot_recovery_rejected(self):
        self.db.execute("INSERT INTO repository_mirror_snapshots VALUES('snap','fixture/repo','{}',?,'failed','now')",(hashlib.sha256(b'{}').hexdigest(),))
        self.db.execute("INSERT INTO repository_mirror_recovery VALUES('snap','[]')")
        self.db.commit()
        with self.assertRaises(ValueError): m.migrate(self.root, True)

if __name__ == "__main__":
    import sys
    import shutil
    if len(sys.argv) == 3 and sys.argv[1] == "--export-fixture":
        case = MigrationTests()
        case.setUp()
        case.db.execute("UPDATE sqlite_sequence SET seq=42 WHERE name='repository_notebooks'")
        case.db.commit()
        case.db.close()
        try:
            destination = Path(sys.argv[2])
            for source in case.root.iterdir(): shutil.copyfile(source, destination / source.name)
            m.migrate(destination, True)
        finally: case.doCleanups()
    elif len(sys.argv) == 3 and sys.argv[1] == "--probe-lock":
        try:
            with m.access_lock(Path(sys.argv[2])): pass
        except OSError: raise SystemExit(73)
    else: unittest.main()
