#!/usr/bin/env python3
"""Offline, non-destructive SkillsHub SQLite to JSON migration (Python stdlib only)."""
import argparse
from contextlib import contextmanager
import hashlib
import json
import os
from pathlib import Path
import shutil
import sqlite3
import tempfile

TABLES = set("skills skill_installations skill_sources skill_source_syncs skill_metadata agent_skill_observations agents collections collection_skills github_repository_stats skill_explanations repository_mirror_recovery import_transaction_receipts repository_mirror_snapshots repository_mirrors repository_local_directories repository_address_moves repository_notebooks folder_notes".split())
REQUIRED = TABLES - {"repository_notebooks", "folder_notes"}
BOOLS = set("is_central is_managed remote_deleted is_read_only is_detected is_builtin is_enabled committed".split())
PAIRS = {"skill_installations": ("skill_id", "agent_id"), "collection_skills": ("collection_id", "skill_id"), "skill_explanations": ("skill_id", "lang")}

def encoded(value):
    return json.dumps(value, ensure_ascii=False, separators=(",", ":"), sort_keys=True, allow_nan=False).encode("utf-8")

def checksum(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()

def source_hashes(directory):
    return {name: checksum(directory / name) for name in ("db.sqlite", "db.sqlite-wal", "db.sqlite-shm") if (directory / name).exists()}

def ascii_lower(value):
    return value.translate(str.maketrans("ABCDEFGHIJKLMNOPQRSTUVWXYZ", "abcdefghijklmnopqrstuvwxyz"))

def row_key(table, row):
    if table in PAIRS:
        return encoded([row[f] for f in PAIRS[table]]).decode("utf-8")
    field = "id"
    if table in {"skill_sources", "skill_source_syncs", "skill_metadata"}: field = "skill_id"
    elif table == "agent_skill_observations": field = "row_id"
    elif table == "repository_mirror_recovery": field = "snapshot_id"
    elif table in {"repository_mirrors", "repository_local_directories", "github_repository_stats"}: field = "repository"
    value = row[field]
    if value is None: raise ValueError("Null identity")
    return ascii_lower(value) if field == "repository" else str(value)

def convert(table, row):
    for field in BOOLS & row.keys():
        if row[field] not in (0, 1): raise ValueError("Invalid boolean")
        row[field] = bool(row[field])
    if table == "folder_notes":
        row["skillIds"] = json.loads(row.pop("skill_ids"))
        if not isinstance(row["skillIds"], list): raise ValueError("Invalid folder note")
    elif table == "repository_notebooks":
        note = json.loads(row["data"])
        if (note.get("id"), note.get("sourceKey"), note.get("repositoryUid")) != (row["id"], row["source_key"], row["repository_uid"]):
            raise ValueError("Notebook identity mismatch")
        row = note
    return row

def write(path, value):
    with path.open("xb") as stream:
        stream.write(value)
        stream.flush()
        os.fsync(stream.fileno())

def _migrate(directory, offline_confirmed=False):
    if not offline_confirmed: raise ValueError("Stop all SkillsHub versions and confirm offline mode")
    directory = Path(directory).resolve(strict=True)
    target = directory / "store"
    if target.exists() or target.is_symlink(): raise ValueError("Destination store already exists")
    source = directory / "db.sqlite"
    if not source.is_file() or source.is_symlink(): raise ValueError("Missing regular source database")
    inputs = [directory / name for name in ("db.sqlite", "db.sqlite-wal", "db.sqlite-shm") if (directory / name).exists()]
    before = source_hashes(directory)
    with tempfile.TemporaryDirectory(prefix=".json-migration-", dir=directory) as work:
        work = Path(work)
        for path in inputs: shutil.copyfile(path, work / path.name)
        if before != source_hashes(directory): raise ValueError("Source changed during copy")
        connection = sqlite3.connect(work / "db.sqlite")
        connection.row_factory = sqlite3.Row
        try:
            if connection.execute("PRAGMA integrity_check").fetchone()[0] != "ok": raise ValueError("Source integrity check failed")
            if connection.execute("PRAGMA foreign_key_check").fetchone(): raise ValueError("Source has broken references")
            names = {r[0] for r in connection.execute("SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%'")}
            if names - TABLES or REQUIRED - names: raise ValueError("Unsupported database schema; upgrade and recover using the old application first")
            catalog = sqlite3.connect(":memory:")
            try:
                catalog.executescript(Path(__file__).with_name("legacy-json-migration-schema.sql").read_text(encoding="utf-8"))
                for table in names:
                    expected = {r[1] for r in catalog.execute('PRAGMA table_info("' + table + '")')}
                    actual = {r[1] for r in connection.execute('PRAGMA table_info("' + table + '")')}
                    if expected != actual: raise ValueError("Unsupported column schema")
            finally: catalog.close()
            tables = {}
            for table in sorted(names):
                rows = {}
                for result in connection.execute('SELECT * FROM "' + table + '"'):
                    row = convert(table, dict(result))
                    key = row_key(table, row)
                    if key in rows: raise ValueError("Duplicate normalized identity")
                    rows[key] = row
                tables[table] = rows
            sequence = 0
            if "repository_notebooks" in names:
                result = connection.execute("SELECT seq FROM sqlite_sequence WHERE name='repository_notebooks'").fetchone()
                if result: sequence = result[0]
                sequence = max(sequence, max((r["id"] for r in tables["repository_notebooks"].values()), default=0))
            if sequence >= 9223372036854775807: raise ValueError("Notebook ID counter exhausted")
            tables["store_counters"] = {"repository_notebooks": {"next_id": sequence + 1}}
        finally:
            connection.close()
        if tables["repository_address_moves"]: raise ValueError("Pending address recovery; recover with old application")
        recoveries = tables["repository_mirror_recovery"]
        for key, row in tables["repository_mirror_snapshots"].items():
            if row["state"] not in {"checked", "applied", "invalidated", "failed"} or (row["state"] == "failed" and key in recoveries):
                raise ValueError("Pending or unknown mirror recovery")
            if hashlib.sha256(row["payload"].encode()).hexdigest() != row["digest"]: raise ValueError("Snapshot digest mismatch")
        # Import file journals can be outside the config directory; require old app recovery first.
        config = directory / "config.json"
        if config.exists():
            cfg = json.loads(config.read_text(encoding="utf-8"))
            settings = cfg.get("settings", {})
            library = settings.get("skill_resource_library_dir") if isinstance(settings, dict) else None
            roots = {directory}
            if library: roots.add(Path(library).expanduser().resolve().parent)
        else: roots = {directory}
        for root in roots:
            for journal in root.glob(".skillshub-import-*"):
                if journal.is_dir() and not (journal / "committed").exists() and not (journal / "recovered").exists():
                    raise ValueError("Pending import recovery; recover with old application")
        staging = work / "store"
        (staging / "objects").mkdir(parents=True)
        (staging / "manifests").mkdir()
        manifest = {"version": 1, "revision": 1, "tables": {}}
        for table, rows in tables.items():
            payload = encoded(rows)
            digest = hashlib.sha256(payload).hexdigest()
            obj = staging / "objects" / (digest + ".json")
            if not obj.exists(): write(obj, payload)
            if checksum(obj) != digest or json.loads(obj.read_bytes()) != rows: raise ValueError("Migration verification failed")
            manifest["tables"][table] = digest
        write(staging / "manifests" / "1.json", encoded(manifest))
        write(staging / "manifest.json", encoded(manifest))
        write(staging / "migration-report.json", encoded({"version":1, "counts":{t:len(r) for t,r in tables.items()}, "sourceHashes":before}))
        if before != source_hashes(directory): raise ValueError("Source changed before publication")
        if target.exists(): raise ValueError("Destination appeared during migration")
        staging.rename(target)
        if os.name != "nt":
            fd = os.open(directory, os.O_RDONLY)
            try: os.fsync(fd)
            finally: os.close(fd)
        return {table: len(rows) for table, rows in tables.items()}

def migrate_locked(directory, offline_confirmed=False):
    if not offline_confirmed: raise ValueError("Stop all SkillsHub versions and confirm offline mode")
    directory = Path(directory).resolve(strict=True)
    marker = directory / ".json-migration.lock"
    fd = os.open(marker, os.O_CREAT | os.O_EXCL | os.O_WRONLY, 0o600)
    try:
        os.write(fd, b"Offline migration in progress")
        os.fsync(fd)
        os.close(fd)
        fd = None
        return _migrate(directory, True)
    finally:
        if fd is not None: os.close(fd)
        marker.unlink()

@contextmanager
def access_lock(directory):
    directory = Path(directory)
    # Rust File::try_lock uses flock on Unix and LockFileEx on Windows.
    # msvcrt byte-range locks conflict with LockFileEx on the same first byte.
    with (directory / ".store-access.lock").open("a+b") as lock:
        lock.seek(0)
        if os.name == "nt":
            import msvcrt
            msvcrt.locking(lock.fileno(), msvcrt.LK_NBLCK, 1)
        else:
            import fcntl
            fcntl.flock(lock.fileno(), fcntl.LOCK_EX | fcntl.LOCK_NB)
        yield

def migrate(directory, offline_confirmed=False):
    if not offline_confirmed: raise ValueError("Stop all SkillsHub versions and confirm offline mode")
    directory = Path(directory).resolve(strict=True)
    with access_lock(directory):
        return migrate_locked(directory, True)

if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("directory", type=Path, help="Old SkillsHub configuration directory")
    parser.add_argument("--offline-confirmed", action="store_true", help="Confirm all application instances are stopped")
    args = parser.parse_args()
    try:
        counts = migrate(args.directory, args.offline_confirmed)
        print("Migration verified:", sum(counts.values()), "records; original files retained")
    except (ValueError, OSError, sqlite3.Error, KeyError, TypeError) as error:
        # No row contents, credentials, or user paths are printed.
        print("Migration refused or failed:", type(error).__name__)
        raise SystemExit(1)
