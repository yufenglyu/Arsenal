-- Frozen final SQLite schema, solely for the offline migration tool.
CREATE TABLE IF NOT EXISTS repository_mirror_recovery (snapshot_id TEXT PRIMARY KEY, payload TEXT NOT NULL);
CREATE TABLE IF NOT EXISTS import_transaction_receipts (id TEXT PRIMARY KEY);
CREATE TABLE IF NOT EXISTS repository_mirror_snapshots (
        id TEXT PRIMARY KEY, repository TEXT NOT NULL, payload TEXT NOT NULL,
        digest TEXT NOT NULL, state TEXT NOT NULL DEFAULT 'checked', created_at TEXT NOT NULL
    );
CREATE TABLE IF NOT EXISTS repository_mirrors (
        repository TEXT PRIMARY KEY, last_snapshot_id TEXT
    );
CREATE TABLE IF NOT EXISTS repository_local_directories (repository TEXT PRIMARY KEY COLLATE NOCASE, directory TEXT NOT NULL UNIQUE);
CREATE TABLE IF NOT EXISTS repository_address_moves (id TEXT PRIMARY KEY, payload TEXT NOT NULL, committed INTEGER NOT NULL DEFAULT 0);
CREATE TABLE IF NOT EXISTS skills (
            id             TEXT PRIMARY KEY,
            name           TEXT NOT NULL,
            description    TEXT,
            file_path      TEXT NOT NULL,
            canonical_path TEXT,
            is_central     BOOLEAN NOT NULL DEFAULT 0,
            source         TEXT,
            content        TEXT,
            scanned_at     TEXT NOT NULL
        );
CREATE TABLE IF NOT EXISTS skill_installations (
            skill_id       TEXT NOT NULL,
            agent_id       TEXT NOT NULL,
            installed_path TEXT NOT NULL,
            link_type      TEXT NOT NULL,
            symlink_target TEXT,
            is_managed     BOOLEAN NOT NULL DEFAULT 0,
            created_at     TEXT NOT NULL DEFAULT (datetime('now')),
            PRIMARY KEY (skill_id, agent_id)
        );
CREATE TABLE IF NOT EXISTS skill_sources (
            skill_id      TEXT PRIMARY KEY,
            source_type   TEXT NOT NULL,
            source_url    TEXT,
            source_author TEXT,
            source_repo   TEXT,
            source_path   TEXT,
            updated_at    TEXT NOT NULL
        );
CREATE TABLE IF NOT EXISTS skill_source_syncs (
            skill_id          TEXT PRIMARY KEY,
            sync_scope        TEXT NOT NULL DEFAULT 'skill',
            remote_ref        TEXT,
            skill_fingerprint TEXT,
            last_checked_at   TEXT,
            last_sync_at      TEXT,
            sync_status       TEXT NOT NULL DEFAULT 'never',
            sync_error        TEXT,
            remote_deleted    BOOLEAN NOT NULL DEFAULT 0,
            updated_at        TEXT NOT NULL,
            FOREIGN KEY (skill_id) REFERENCES skill_sources(skill_id)
        );
CREATE TABLE IF NOT EXISTS skill_metadata (
            skill_id   TEXT PRIMARY KEY,
            notes      TEXT,
            tags       TEXT NOT NULL DEFAULT '[]',
            updated_at TEXT NOT NULL
        );
CREATE TABLE IF NOT EXISTS agent_skill_observations (
            row_id         TEXT PRIMARY KEY,
            agent_id       TEXT NOT NULL,
            skill_id       TEXT NOT NULL,
            name           TEXT NOT NULL,
            description    TEXT,
            file_path      TEXT NOT NULL,
            dir_path       TEXT NOT NULL,
            source_kind    TEXT NOT NULL,
            source_root    TEXT NOT NULL,
            link_type      TEXT NOT NULL,
            symlink_target TEXT,
            is_read_only   BOOLEAN NOT NULL DEFAULT 0,
            scanned_at     TEXT NOT NULL
        );
CREATE TABLE IF NOT EXISTS agents (
            id                 TEXT PRIMARY KEY,
            display_name       TEXT NOT NULL,
            category           TEXT NOT NULL,
            global_skills_dir  TEXT NOT NULL,
            project_skills_dir TEXT,
            is_detected        BOOLEAN NOT NULL DEFAULT 0,
            is_builtin         BOOLEAN NOT NULL DEFAULT 1,
            is_enabled         BOOLEAN NOT NULL DEFAULT 1
        );
CREATE TABLE IF NOT EXISTS collections (
            id          TEXT PRIMARY KEY,
            name        TEXT NOT NULL,
            description TEXT,
            created_at  TEXT NOT NULL,
            updated_at  TEXT NOT NULL
        );
CREATE TABLE IF NOT EXISTS collection_skills (
            collection_id TEXT NOT NULL,
            skill_id      TEXT NOT NULL,
            added_at      TEXT NOT NULL,
            PRIMARY KEY (collection_id, skill_id)
        );
CREATE TABLE IF NOT EXISTS github_repository_stats (repository TEXT PRIMARY KEY COLLATE NOCASE, stars INTEGER NOT NULL CHECK(stars >= 0), checked_at TEXT NOT NULL);
CREATE TABLE IF NOT EXISTS skill_explanations (
            skill_id    TEXT NOT NULL,
            explanation TEXT NOT NULL,
            lang        TEXT NOT NULL DEFAULT 'zh',
            model       TEXT,
            created_at  TEXT NOT NULL DEFAULT (datetime('now')),
            updated_at  TEXT NOT NULL DEFAULT (datetime('now')),
            PRIMARY KEY (skill_id, lang)
        );
CREATE TABLE IF NOT EXISTS repository_notebooks (id INTEGER PRIMARY KEY AUTOINCREMENT, source_key TEXT NOT NULL UNIQUE, repository_uid TEXT NOT NULL UNIQUE, data TEXT NOT NULL);
CREATE TABLE IF NOT EXISTS folder_notes (id TEXT PRIMARY KEY, notes TEXT NOT NULL, skill_ids TEXT NOT NULL);
