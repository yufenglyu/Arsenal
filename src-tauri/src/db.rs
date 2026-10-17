pub mod notebooks;
pub mod explanations;
pub mod metadata;
use chrono::Utc;
use sqlx::{
    sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions},
    Row, SqlitePool,
};
use std::{collections::HashSet, path::Path, str::FromStr};
use uuid::Uuid;

use crate::path_utils::{app_data_dir, expand_home_path, path_to_string};

pub type DbPool = SqlitePool;

pub use crate::storage::models::*;
pub use crate::db_config::{config_path, initialize as initialize_config, load as load_config, update as update_config};

// SQL decoding belongs to this transitional adapter, not the domain models.
macro_rules! sqlite_row {
    ($model:ident { $($field:ident),* }) => {
        impl<'r> sqlx::FromRow<'r, sqlx::sqlite::SqliteRow> for $model {
            fn from_row(row: &'r sqlx::sqlite::SqliteRow) -> Result<Self, sqlx::Error> {
                Ok(Self { $($field: row.try_get(stringify!($field))?),* })
            }
        }
    };
}
sqlite_row!(Skill { id, name, description, file_path, canonical_path, is_central, source, content, scanned_at });
sqlite_row!(SkillSource { skill_id, source_type, source_url, source_author, source_repo, source_path, updated_at });
sqlite_row!(SkillSourceSync { skill_id, sync_scope, remote_ref, skill_fingerprint, last_checked_at, last_sync_at, sync_status, sync_error, remote_deleted, updated_at });
sqlite_row!(SkillMetadata { skill_id, notes, tags, updated_at });
sqlite_row!(SkillInstallation { skill_id, agent_id, installed_path, link_type, symlink_target, created_at });
sqlite_row!(AgentSkillObservation { row_id, agent_id, skill_id, name, description, file_path, dir_path, source_kind, source_root, link_type, symlink_target, is_read_only, scanned_at });
sqlite_row!(Agent { id, display_name, category, global_skills_dir, project_skills_dir, is_detected, is_builtin, is_enabled });
sqlite_row!(Collection { id, name, description, created_at, updated_at });
sqlite_row!(ScanDirectory { id, path, label, is_active, is_builtin, added_at });
sqlite_row!(SkillForAgent { installation_source, id, row_id, name, description, file_path, dir_path, link_type, symlink_target, is_central, source, source_kind, source_root, is_read_only, conflict_group, conflict_count, source_url, source_author, source_repo, source_path, created_at });

pub const SKILL_RESOURCE_LIBRARY_DIR_SETTING_KEY: &str = "skill_resource_library_dir";

// ─── Data Types ──────────────────────────────────────────────────────────────

pub use crate::platforms::{agent_supports_universal_agents_skills, builtin_agents};

// ─── Pool Creation ────────────────────────────────────────────────────────────

/// Create a production SQLite pool for the given file path with WAL mode enabled.
pub async fn create_pool(db_path: &str) -> Result<DbPool, String> {
    let opts = SqliteConnectOptions::from_str(&format!("sqlite://{}", db_path))
        .map_err(|e| e.to_string())?
        .create_if_missing(true)
        .journal_mode(SqliteJournalMode::Wal);

    SqlitePoolOptions::new()
        .max_connections(5)
        .connect_with(opts)
        .await
        .map_err(|e| e.to_string())
}

// ─── Init ─────────────────────────────────────────────────────────────────────

/// Initialize all database tables (idempotent) without seeding built-in agents.
pub async fn init_schema(pool: &DbPool) -> Result<(), String> {
    sqlx::query("CREATE TABLE IF NOT EXISTS repository_mirror_recovery (snapshot_id TEXT PRIMARY KEY, payload TEXT NOT NULL)").execute(pool).await.map_err(|e|e.to_string())?;
    sqlx::query("CREATE TABLE IF NOT EXISTS import_transaction_receipts (id TEXT PRIMARY KEY)").execute(pool).await.map_err(|e| e.to_string())?;
    sqlx::query("CREATE TABLE IF NOT EXISTS repository_mirror_snapshots (
        id TEXT PRIMARY KEY, repository TEXT NOT NULL, payload TEXT NOT NULL,
        digest TEXT NOT NULL, state TEXT NOT NULL DEFAULT 'checked', created_at TEXT NOT NULL
    )").execute(pool).await.map_err(|e| e.to_string())?;
    sqlx::query("CREATE TABLE IF NOT EXISTS repository_mirrors (
        repository TEXT PRIMARY KEY, last_snapshot_id TEXT
    )").execute(pool).await.map_err(|e| e.to_string())?;

    sqlx::query("CREATE TABLE IF NOT EXISTS repository_local_directories (repository TEXT PRIMARY KEY COLLATE NOCASE, directory TEXT NOT NULL UNIQUE)").execute(pool).await.map_err(|e| e.to_string())?;

    sqlx::query("CREATE TABLE IF NOT EXISTS repository_address_moves (id TEXT PRIMARY KEY, payload TEXT NOT NULL, committed INTEGER NOT NULL DEFAULT 0)").execute(pool).await.map_err(|e| e.to_string())?;

    // Enable WAL mode (no-op for in-memory databases)
    sqlx::query("PRAGMA journal_mode=WAL")
        .execute(pool)
        .await
        .map_err(|e| e.to_string())?;

    // skills table
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS skills (
            id             TEXT PRIMARY KEY,
            name           TEXT NOT NULL,
            description    TEXT,
            file_path      TEXT NOT NULL,
            canonical_path TEXT,
            is_central     BOOLEAN NOT NULL DEFAULT 0,
            source         TEXT,
            content        TEXT,
            scanned_at     TEXT NOT NULL
        )",
    )
    .execute(pool)
    .await
    .map_err(|e| e.to_string())?;

    // skill_installations table
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS skill_installations (
            skill_id       TEXT NOT NULL,
            agent_id       TEXT NOT NULL,
            installed_path TEXT NOT NULL,
            link_type      TEXT NOT NULL,
            symlink_target TEXT,
            is_managed     BOOLEAN NOT NULL DEFAULT 0,
            created_at     TEXT NOT NULL DEFAULT (datetime('now')),
            PRIMARY KEY (skill_id, agent_id)
        )",
    )
    .execute(pool)
    .await
    .map_err(|e| e.to_string())?;

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS skill_sources (
            skill_id      TEXT PRIMARY KEY,
            source_type   TEXT NOT NULL,
            source_url    TEXT,
            source_author TEXT,
            source_repo   TEXT,
            source_path   TEXT,
            updated_at    TEXT NOT NULL
        )",
    )
    .execute(pool)
    .await
    .map_err(|e| e.to_string())?;

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS skill_source_syncs (
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
        )",
    )
    .execute(pool)
    .await
    .map_err(|e| e.to_string())?;

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS skill_metadata (
            skill_id   TEXT PRIMARY KEY,
            notes      TEXT,
            tags       TEXT NOT NULL DEFAULT '[]',
            updated_at TEXT NOT NULL
        )",
    )
    .execute(pool)
    .await
    .map_err(|e| e.to_string())?;

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS agent_skill_observations (
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
        )",
    )
    .execute(pool)
    .await
    .map_err(|e| e.to_string())?;

    // Migration: add created_at column to skill_installations for existing databases
    // that were created before this column was introduced. We check via PRAGMA table_info
    // and run a two-step migration only when the column is absent:
    //   1. ALTER TABLE ADD COLUMN created_at TEXT  (nullable, no default expression –
    //      SQLite's ALTER TABLE does not support non-constant default expressions on
    //      some builds, e.g. the Apple-modified SQLite on macOS).
    //   2. UPDATE … SET created_at = datetime('now') WHERE created_at IS NULL  –
    //      backfills existing rows with the current timestamp.
    // New rows written by the application always supply created_at explicitly, so
    // there is no need for a database-level DEFAULT after the migration runs.
    let columns = sqlx::query("PRAGMA table_info(skill_installations)")
        .fetch_all(pool)
        .await
        .map_err(|e| e.to_string())?;

    let has_created_at = columns.iter().any(|row| {
        row.try_get::<String, _>("name")
            .map(|name| name == "created_at")
            .unwrap_or(false)
    });

    if !has_created_at {
        // Step 1: add the column (nullable, no expression default for compatibility).
        sqlx::query("ALTER TABLE skill_installations ADD COLUMN created_at TEXT")
            .execute(pool)
            .await
            .map_err(|e| e.to_string())?;

        // Step 2: backfill existing rows with the current timestamp.
        sqlx::query(
            "UPDATE skill_installations SET created_at = datetime('now') WHERE created_at IS NULL",
        )
        .execute(pool)
        .await
        .map_err(|e| e.to_string())?;
    }

    let mut managed_migration = pool.begin().await.map_err(|e| e.to_string())?;
    let installation_columns = sqlx::query("PRAGMA table_info(skill_installations)")
        .fetch_all(&mut *managed_migration)
        .await
        .map_err(|e| e.to_string())?;
    let has_is_managed = installation_columns.iter().any(|row| {
        row.try_get::<String, _>("name")
            .map(|name| name == "is_managed")
            .unwrap_or(false)
    });
    if !has_is_managed {
        sqlx::query(
            "ALTER TABLE skill_installations
             ADD COLUMN is_managed BOOLEAN NOT NULL DEFAULT 0",
        )
        .execute(&mut *managed_migration)
        .await
        .map_err(|e| e.to_string())?;
        sqlx::query(
            "UPDATE skill_installations
             SET is_managed = 1
             WHERE link_type = 'symlink'
               AND symlink_target IS NOT NULL
               AND EXISTS (
                 SELECT 1 FROM skills s
                 WHERE s.id = skill_installations.skill_id
                   AND s.canonical_path IS NOT NULL
                   AND s.canonical_path = skill_installations.symlink_target
             )",
        )
        .execute(&mut *managed_migration)
        .await
        .map_err(|e| e.to_string())?;
    }
    managed_migration
        .commit()
        .await
        .map_err(|e| e.to_string())?;

    // agents table
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS agents (
            id                 TEXT PRIMARY KEY,
            display_name       TEXT NOT NULL,
            category           TEXT NOT NULL,
            global_skills_dir  TEXT NOT NULL,
            project_skills_dir TEXT,
            is_detected        BOOLEAN NOT NULL DEFAULT 0,
            is_builtin         BOOLEAN NOT NULL DEFAULT 1,
            is_enabled         BOOLEAN NOT NULL DEFAULT 1
        )",
    )
    .execute(pool)
    .await
    .map_err(|e| e.to_string())?;

    // collections table
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS collections (
            id          TEXT PRIMARY KEY,
            name        TEXT NOT NULL,
            description TEXT,
            created_at  TEXT NOT NULL,
            updated_at  TEXT NOT NULL
        )",
    )
    .execute(pool)
    .await
    .map_err(|e| e.to_string())?;

    // collection_skills table
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS collection_skills (
            collection_id TEXT NOT NULL,
            skill_id      TEXT NOT NULL,
            added_at      TEXT NOT NULL,
            PRIMARY KEY (collection_id, skill_id)
        )",
    )
    .execute(pool)
    .await
    .map_err(|e| e.to_string())?;

    sqlx::query("CREATE TABLE IF NOT EXISTS github_repository_stats (repository TEXT PRIMARY KEY COLLATE NOCASE, stars INTEGER NOT NULL CHECK(stars >= 0), checked_at TEXT NOT NULL)")
        .execute(pool).await.map_err(|e| e.to_string())?;

    // Configuration is file-backed. Do not retain legacy configuration tables.
    crate::db::initialize_config(pool)?;
    let mut config_cleanup = pool.begin().await.map_err(|e| e.to_string())?;
    sqlx::query("PRAGMA secure_delete = ON")
        .execute(&mut *config_cleanup).await.map_err(|e| e.to_string())?;
    sqlx::query("DROP TABLE IF EXISTS settings")
        .execute(&mut *config_cleanup).await.map_err(|e| e.to_string())?;
    sqlx::query("DROP TABLE IF EXISTS scan_directories")
        .execute(&mut *config_cleanup).await.map_err(|e| e.to_string())?;
    config_cleanup.commit().await.map_err(|e| e.to_string())?;

    // skill_explanations table — cached AI-generated skill explanations
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS skill_explanations (
            skill_id    TEXT NOT NULL,
            explanation TEXT NOT NULL,
            lang        TEXT NOT NULL DEFAULT 'zh',
            model       TEXT,
            created_at  TEXT NOT NULL DEFAULT (datetime('now')),
            updated_at  TEXT NOT NULL DEFAULT (datetime('now')),
            PRIMARY KEY (skill_id, lang)
        )",
    )
    .execute(pool)
    .await
    .map_err(|e| e.to_string())?;

    Ok(())
}

/// Initialize schema, then seed built-in agents from the platform catalog.
pub async fn init_database(pool: &DbPool) -> Result<(), String> {
    init_schema(pool).await?;

    // Resetting configuration must not assign an old installation's target ID to a new project.
    let recorded_targets: Vec<String> = sqlx::query_scalar(
        "SELECT agent_id FROM skill_installations WHERE agent_id LIKE 'project:%'
         UNION SELECT agent_id FROM agent_skill_observations WHERE agent_id LIKE 'project:%'",
    ).fetch_all(pool).await.map_err(|e| e.to_string())?;
    let next_id = recorded_targets.iter()
        .filter_map(|id| id.strip_prefix("project:")?.parse::<i64>().ok())
        .filter_map(|id| id.checked_add(1)).max().unwrap_or(1);
    crate::db::update_config(pool, |config| {
        config.next_scan_directory_id = config.next_scan_directory_id.max(next_id);
        Ok(())
    })?;

    // Seed built-in agents (INSERT OR IGNORE so repeated init is safe)
    seed_builtin_agents(pool).await?;

    // Seed built-in scan directories from the built-in agent registry.
    seed_builtin_scan_directories(pool).await?;

    Ok(())
}

/// Write a complete default `.skillshub` tree for installers and portable ZIPs.
///
/// Agent rows are not seeded here so skill directories expand against the
/// end-user home directory on first launch.
pub async fn prepare_config_dir(config_dir: &Path) -> Result<(), String> {
    std::fs::create_dir_all(config_dir).map_err(|e| e.to_string())?;
    for obsolete in ["platform/platform.json", "builtin-agent-overrides.json"] {
        let path = config_dir.join(obsolete);
        if path.is_file() {
            std::fs::remove_file(path).map_err(|e| e.to_string())?;
        }
    }
    // Only remove old empty packaging placeholders; never remove skill contents.
    for name in ["library", "platform"] {
        let directory = config_dir.join(name);
        let placeholder = directory.join(".keep");
        if placeholder.is_file() && std::fs::metadata(&placeholder).map_err(|e| e.to_string())?.len() == 0 {
            std::fs::remove_file(placeholder).map_err(|e| e.to_string())?;
        }
        let _ = std::fs::remove_dir(directory.join("icons"));
        let _ = std::fs::remove_dir(&directory);
    }
    let db_path = config_dir.join("db.sqlite");
    let _ = std::fs::remove_file(&db_path);
    let _ = std::fs::remove_file(config_dir.join("db.sqlite-wal"));
    let _ = std::fs::remove_file(config_dir.join("db.sqlite-shm"));
    let pool = create_pool(&path_to_string(&db_path)).await?;
    crate::db::update_config(&pool, |config| {
        *config = crate::config_store::AppConfig::default();
        Ok(())
    })?;
    crate::platform_icons::seed(config_dir)?;
    init_schema(&pool).await?;
    sqlx::query("PRAGMA wal_checkpoint(TRUNCATE)")
        .execute(&pool)
        .await
        .map_err(|e| e.to_string())?;
    pool.close().await;
    for companion in ["db.sqlite-wal", "db.sqlite-shm"] {
        let _ = std::fs::remove_file(config_dir.join(companion));
    }
    Ok(())
}

async fn seed_builtin_agents(pool: &DbPool) -> Result<(), String> {
    let definitions = crate::platforms::load_platform_definitions()?;
    let agents = crate::platforms::agents_from_definitions(&definitions);
    let sync_existing = crate::platforms::platform_dir().is_some();
    seed_builtin_agents_from(pool, &agents, sync_existing).await
}

async fn seed_builtin_agents_from(
    pool: &DbPool,
    agents: &[Agent],
    sync_existing: bool,
) -> Result<(), String> {
    let active_builtin_ids: Vec<&str> = agents.iter().map(|agent| agent.id.as_str()).collect();

    for agent in agents {
        sqlx::query(
            "INSERT OR IGNORE INTO agents
             (id, display_name, category, global_skills_dir, project_skills_dir,
              is_detected, is_builtin, is_enabled)
             VALUES (?, ?, ?, ?, ?, 0, ?, ?)",
        )
        .bind(&agent.id)
        .bind(&agent.display_name)
        .bind(&agent.category)
        .bind(&agent.global_skills_dir)
        .bind(&agent.project_skills_dir)
        .bind(agent.is_builtin)
        .bind(agent.is_enabled)
        .execute(pool)
        .await
        .map_err(|e| e.to_string())?;

        if sync_existing {
            sqlx::query(
                "UPDATE agents
                 SET display_name = ?, category = ?, global_skills_dir = ?,
                     project_skills_dir = ?, is_enabled = ?, is_builtin = ?
                 WHERE id = ?",
            )
            .bind(&agent.display_name)
            .bind(&agent.category)
            .bind(&agent.global_skills_dir)
            .bind(&agent.project_skills_dir)
            .bind(agent.is_enabled)
            .bind(agent.is_builtin)
            .bind(&agent.id)
            .execute(pool)
            .await
            .map_err(|e| e.to_string())?;
        }
    }

    if !sync_existing {
        return Ok(());
    }

    let all_db_agents: Vec<(String,)> =
        sqlx::query_as("SELECT id FROM agents WHERE id NOT LIKE 'project:%'")
            .fetch_all(pool)
            .await
            .map_err(|e| e.to_string())?;

    for (id,) in &all_db_agents {
        if !active_builtin_ids.contains(&id.as_str()) {
            sqlx::query("DELETE FROM agents WHERE id = ?")
                .bind(id)
                .execute(pool)
                .await
                .map_err(|e| e.to_string())?;
        }
    }

    Ok(())
}

/// Seed `scan_directories` with one row per unique `global_skills_dir` path
/// across all built-in agents.  Rows are marked `is_builtin = 1` and cannot
/// be removed by the user.  `INSERT OR IGNORE` keeps the operation idempotent:
/// if two built-in agents share the same path, only the first insert takes
/// effect.
async fn seed_builtin_scan_directories(pool: &DbPool) -> Result<(), String> {
    let agents = get_all_agents(pool).await?;
    crate::db::update_config(pool, |config| {
        let paths: HashSet<_> = agents.iter().filter(|agent| agent.is_builtin)
            .map(|agent| agent.global_skills_dir.as_str()).collect();
        config.scan_directories.retain(|dir| !dir.is_builtin || paths.contains(dir.path.as_str()));
        for agent in agents.iter().filter(|agent| agent.is_builtin) {
            if !config.scan_directories.iter().any(|dir| dir.path == agent.global_skills_dir) {
                let id = next_scan_directory_id(config);
                config.scan_directories.push(ScanDirectory {
                    id, path: agent.global_skills_dir.clone(), label: Some(agent.display_name.clone()),
                    is_active: true, is_builtin: true, added_at: Utc::now().to_rfc3339(),
                });
            }
        }
        Ok(())
    })
}

// ─── Skills ───────────────────────────────────────────────────────────────────

/// Insert or update a skill record.
///
/// Uses `ON CONFLICT DO UPDATE` to preserve `is_central = true` if a prior
/// scan already marked the skill as central. Once a skill is flagged as
/// central it must never be downgraded to non-central by a subsequent scan of
/// the same directory by a non-central agent.
pub async fn upsert_skill(pool: &DbPool, skill: &Skill) -> Result<(), String> {
    upsert_skill_with_executor(pool, skill).await
}

pub(crate) async fn upsert_skill_with_executor<'e>(
    executor: impl sqlx::Executor<'e, Database = sqlx::Sqlite>,
    skill: &Skill,
) -> Result<(), String> {
    sqlx::query(
        "INSERT INTO skills
         (id, name, description, file_path, canonical_path, is_central, source, content, scanned_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
         ON CONFLICT(id) DO UPDATE SET
           name           = CASE
                              WHEN skills.is_central = 1 AND excluded.is_central = 0 THEN skills.name
                              ELSE excluded.name
                            END,
           description    = CASE
                              WHEN skills.is_central = 1 AND excluded.is_central = 0 THEN skills.description
                              ELSE excluded.description
                            END,
           file_path      = CASE
                              WHEN skills.is_central = 1 AND excluded.is_central = 0 THEN skills.file_path
                              ELSE excluded.file_path
                            END,
           canonical_path = CASE
                              WHEN skills.is_central = 1 AND excluded.is_central = 0 THEN skills.canonical_path
                              ELSE COALESCE(excluded.canonical_path, skills.canonical_path)
                            END,
           is_central     = MAX(skills.is_central, excluded.is_central),
           source         = CASE
                              WHEN skills.is_central = 1 AND excluded.is_central = 0 THEN skills.source
                              ELSE excluded.source
                            END,
           content        = CASE
                              WHEN skills.is_central = 1 AND excluded.is_central = 0 THEN skills.content
                              ELSE excluded.content
                            END,
           scanned_at     = excluded.scanned_at",
    )
    .bind(&skill.id)
    .bind(&skill.name)
    .bind(&skill.description)
    .bind(&skill.file_path)
    .bind(&skill.canonical_path)
    .bind(skill.is_central)
    .bind(&skill.source)
    .bind(&skill.content)
    .bind(&skill.scanned_at)
    .execute(executor)
    .await
    .map(|_| ())
    .map_err(|e| e.to_string())
}

/// Restore a resource-library skill that an older platform install mistakenly
/// represented as a central symlink. Callers must validate the filesystem link
/// and its installation record before using this explicit central downgrade.
pub async fn repair_legacy_centralized_resource_skill(
    pool: &DbPool,
    skill: &Skill,
    legacy_central_path: &str,
) -> Result<(), String> {
    let mut transaction = pool.begin().await.map_err(|e| e.to_string())?;

    sqlx::query(
        "UPDATE skills SET
           name = ?, description = ?, file_path = ?, canonical_path = ?,
           is_central = 0, source = ?, content = ?, scanned_at = ?
         WHERE id = ?",
    )
    .bind(&skill.name)
    .bind(&skill.description)
    .bind(&skill.file_path)
    .bind(&skill.canonical_path)
    .bind(&skill.source)
    .bind(&skill.content)
    .bind(&skill.scanned_at)
    .bind(&skill.id)
    .execute(&mut *transaction)
    .await
    .map_err(|e| e.to_string())?;

    sqlx::query(
        "DELETE FROM skill_installations
         WHERE skill_id = ? AND installed_path = ? AND link_type = 'symlink'",
    )
    .bind(&skill.id)
    .bind(legacy_central_path)
    .execute(&mut *transaction)
    .await
    .map_err(|e| e.to_string())?;

    transaction.commit().await.map_err(|e| e.to_string())
}

/// Retrieve all application-managed skills installed for a given agent.
pub async fn get_skills_by_agent(pool: &DbPool, agent_id: &str) -> Result<Vec<Skill>, String> {
    sqlx::query_as::<_, Skill>(
        "SELECT s.* FROM skills s
         JOIN skill_installations si ON s.id = si.skill_id
         WHERE si.agent_id = ?
           AND si.is_managed = 1",
    )
    .bind(agent_id)
    .fetch_all(pool)
    .await
    .map_err(|e| e.to_string())
}

/// Retrieve application-managed skills installed for a given agent, enriched
/// with installation metadata required by the platform-view skill cards.
pub async fn get_skills_for_agent(
    pool: &DbPool,
    agent_id: &str,
) -> Result<Vec<SkillForAgent>, String> {
    sqlx::query_as::<_, SkillForAgent>(
        "SELECT 'independent' AS installation_source,
                s.id,
                s.id AS row_id,
                s.name,
                s.description,
                s.file_path,
                si.installed_path AS dir_path,
                si.link_type,
                si.symlink_target,
                s.is_central,
                s.source,
                NULL AS source_kind,
                NULL AS source_root,
                0 AS is_read_only,
                NULL AS conflict_group,
                0 AS conflict_count,
                ss.source_url,
                ss.source_author,
                ss.source_repo,
                ss.source_path,
                si.created_at
         FROM skills s
         JOIN skill_installations si ON s.id = si.skill_id
         LEFT JOIN skill_sources ss ON ss.skill_id = s.id
         WHERE si.agent_id = ?
           AND si.is_managed = 1",
    )
    .bind(agent_id)
    .fetch_all(pool)
    .await
    .map_err(|e| e.to_string())
}

pub async fn upsert_agent_skill_observation(
    pool: &DbPool,
    observation: &AgentSkillObservation,
) -> Result<(), String> {
    sqlx::query(
        "INSERT INTO agent_skill_observations
         (row_id, agent_id, skill_id, name, description, file_path, dir_path,
          source_kind, source_root, link_type, symlink_target, is_read_only, scanned_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
         ON CONFLICT(row_id) DO UPDATE SET
           agent_id       = excluded.agent_id,
           skill_id       = excluded.skill_id,
           name           = excluded.name,
           description    = excluded.description,
           file_path      = excluded.file_path,
           dir_path       = excluded.dir_path,
           source_kind    = excluded.source_kind,
           source_root    = excluded.source_root,
           link_type      = excluded.link_type,
           symlink_target = excluded.symlink_target,
           is_read_only   = excluded.is_read_only,
           scanned_at     = excluded.scanned_at",
    )
    .bind(&observation.row_id)
    .bind(&observation.agent_id)
    .bind(&observation.skill_id)
    .bind(&observation.name)
    .bind(&observation.description)
    .bind(&observation.file_path)
    .bind(&observation.dir_path)
    .bind(&observation.source_kind)
    .bind(&observation.source_root)
    .bind(&observation.link_type)
    .bind(&observation.symlink_target)
    .bind(observation.is_read_only)
    .bind(&observation.scanned_at)
    .execute(pool)
    .await
    .map(|_| ())
    .map_err(|e| e.to_string())
}

pub async fn get_agent_skill_observations(
    pool: &DbPool,
    agent_id: &str,
) -> Result<Vec<AgentSkillObservation>, String> {
    sqlx::query_as::<_, AgentSkillObservation>(
        "SELECT * FROM agent_skill_observations
         WHERE agent_id = ?
         ORDER BY name, dir_path",
    )
    .bind(agent_id)
    .fetch_all(pool)
    .await
    .map_err(|e| e.to_string())
}

/// Retrieve all Central Skills (is_central = true).
pub async fn get_central_skills(pool: &DbPool) -> Result<Vec<Skill>, String> {
    sqlx::query_as::<_, Skill>("SELECT * FROM skills WHERE is_central = 1")
        .fetch_all(pool)
        .await
        .map_err(|e| e.to_string())
}

pub fn default_skill_resource_library_dir() -> std::path::PathBuf {
    app_data_dir().join("library")
}

pub async fn get_skill_resource_library_dir(pool: &DbPool) -> Result<std::path::PathBuf, String> {
    Ok(get_setting(pool, SKILL_RESOURCE_LIBRARY_DIR_SETTING_KEY)
        .await?
        .map(|path| expand_home_path(&path))
        .unwrap_or_else(default_skill_resource_library_dir))
}

pub async fn set_skill_resource_library_dir(
    pool: &DbPool,
    path: &str,
) -> Result<std::path::PathBuf, String> {
    let expanded = expand_home_path(path.trim());
    if expanded.as_os_str().is_empty() {
        return Err("Skill resource library path cannot be empty".to_string());
    }
    std::fs::create_dir_all(&expanded)
        .map_err(|e| format!("Failed to create Skill Resource Library directory: {}", e))?;
    set_setting(
        pool,
        SKILL_RESOURCE_LIBRARY_DIR_SETTING_KEY,
        &path_to_string(&expanded),
    )
    .await?;
    Ok(expanded)
}

pub async fn get_resource_library_skills(pool: &DbPool) -> Result<Vec<Skill>, String> {
    sqlx::query_as::<_, Skill>(
        "SELECT * FROM skills
         WHERE is_central = 0 AND canonical_path IS NOT NULL
         ORDER BY name",
    )
    .fetch_all(pool)
    .await
    .map_err(|e| e.to_string())
}

/// Retrieve a skill by its ID.
pub async fn get_skill_by_id(pool: &DbPool, skill_id: &str) -> Result<Option<Skill>, String> {
    sqlx::query_as::<_, Skill>("SELECT * FROM skills WHERE id = ?")
        .bind(skill_id)
        .fetch_optional(pool)
        .await
        .map_err(|e| e.to_string())
}

pub async fn upsert_skill_source(pool: &DbPool, source: &SkillSource) -> Result<(), String> {
    upsert_skill_source_with_executor(pool, source).await
}

pub(crate) async fn upsert_skill_source_with_executor<'e>(
    executor: impl sqlx::Executor<'e, Database = sqlx::Sqlite>,
    source: &SkillSource,
) -> Result<(), String> {
    sqlx::query(
        "INSERT INTO skill_sources
         (skill_id, source_type, source_url, source_author, source_repo, source_path, updated_at)
         VALUES (?, ?, ?, ?, ?, ?, ?)
         ON CONFLICT(skill_id) DO UPDATE SET
           source_type = excluded.source_type,
           source_url = excluded.source_url,
           source_author = excluded.source_author,
           source_repo = excluded.source_repo,
           source_path = excluded.source_path,
           updated_at = excluded.updated_at",
    )
    .bind(&source.skill_id)
    .bind(&source.source_type)
    .bind(&source.source_url)
    .bind(&source.source_author)
    .bind(&source.source_repo)
    .bind(&source.source_path)
    .bind(&source.updated_at)
    .execute(executor)
    .await
    .map(|_| ())
    .map_err(|e| e.to_string())
}

pub async fn get_skill_source(
    pool: &DbPool,
    skill_id: &str,
) -> Result<Option<SkillSource>, String> {
    sqlx::query_as::<_, SkillSource>("SELECT * FROM skill_sources WHERE skill_id = ?")
        .bind(skill_id)
        .fetch_optional(pool)
        .await
        .map_err(|e| e.to_string())
}

pub async fn get_all_skill_sources(pool: &DbPool) -> Result<Vec<SkillSource>, String> {
    sqlx::query_as::<_, SkillSource>("SELECT * FROM skill_sources ORDER BY skill_id")
        .fetch_all(pool)
        .await
        .map_err(|e| e.to_string())
}

pub async fn upsert_skill_source_sync(pool: &DbPool, sync: &SkillSourceSync) -> Result<(), String> {
    sqlx::query(
        "INSERT INTO skill_source_syncs
         (skill_id, sync_scope, remote_ref, skill_fingerprint, last_checked_at, last_sync_at,
          sync_status, sync_error, remote_deleted, updated_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
         ON CONFLICT(skill_id) DO UPDATE SET
           sync_scope = excluded.sync_scope,
           remote_ref = excluded.remote_ref,
           skill_fingerprint = excluded.skill_fingerprint,
           last_checked_at = excluded.last_checked_at,
           last_sync_at = excluded.last_sync_at,
           sync_status = excluded.sync_status,
           sync_error = excluded.sync_error,
           remote_deleted = excluded.remote_deleted,
           updated_at = excluded.updated_at",
    )
    .bind(&sync.skill_id)
    .bind(&sync.sync_scope)
    .bind(&sync.remote_ref)
    .bind(&sync.skill_fingerprint)
    .bind(&sync.last_checked_at)
    .bind(&sync.last_sync_at)
    .bind(&sync.sync_status)
    .bind(&sync.sync_error)
    .bind(sync.remote_deleted)
    .bind(&sync.updated_at)
    .execute(pool)
    .await
    .map(|_| ())
    .map_err(|e| e.to_string())
}

/// Record a successful check without replacing the installed commit, content
/// fingerprint, last sync time or error/status of an actual update operation.
pub async fn record_skill_check_times(
    pool: &DbPool,
    skill_ids: &[String],
    checked_at: &str,
) -> Result<(), String> {
    let mut transaction = pool.begin().await.map_err(|e| e.to_string())?;
    for skill_id in skill_ids {
        sqlx::query(
            "INSERT INTO skill_source_syncs (skill_id, last_checked_at, updated_at)
            VALUES (?, ?, ?) ON CONFLICT(skill_id) DO UPDATE SET
            last_checked_at = excluded.last_checked_at, updated_at = excluded.updated_at",
        )
        .bind(skill_id)
        .bind(checked_at)
        .bind(checked_at)
        .execute(&mut *transaction)
        .await
        .map_err(|e| e.to_string())?;
    }
    transaction.commit().await.map_err(|e| e.to_string())
}

pub async fn get_skill_source_sync(
    pool: &DbPool,
    skill_id: &str,
) -> Result<Option<SkillSourceSync>, String> {
    sqlx::query_as::<_, SkillSourceSync>("SELECT * FROM skill_source_syncs WHERE skill_id = ?")
        .bind(skill_id)
        .fetch_optional(pool)
        .await
        .map_err(|e| e.to_string())
}

pub async fn get_all_skill_source_syncs(pool: &DbPool) -> Result<Vec<SkillSourceSync>, String> {
    sqlx::query_as::<_, SkillSourceSync>("SELECT * FROM skill_source_syncs ORDER BY skill_id")
        .fetch_all(pool)
        .await
        .map_err(|e| e.to_string())
}

pub async fn get_skill_metadata(
    pool: &DbPool,
    skill_id: &str,
) -> Result<Option<SkillMetadata>, String> {
    sqlx::query_as::<_, SkillMetadata>("SELECT * FROM skill_metadata WHERE skill_id = ?")
        .bind(skill_id)
        .fetch_optional(pool)
        .await
        .map_err(|e| e.to_string())
}

pub async fn upsert_skill_metadata(
    pool: &DbPool,
    skill_id: &str,
    notes: Option<&str>,
    tags: &[String],
) -> Result<SkillMetadata, String> {
    let now = Utc::now().to_rfc3339();
    let tags_json = serde_json::to_string(tags).map_err(|e| e.to_string())?;
    sqlx::query(
        "INSERT INTO skill_metadata (skill_id, notes, tags, updated_at)
         VALUES (?, ?, ?, ?)
         ON CONFLICT(skill_id) DO UPDATE SET
           notes = excluded.notes,
           tags = excluded.tags,
           updated_at = excluded.updated_at",
    )
    .bind(skill_id)
    .bind(notes)
    .bind(&tags_json)
    .bind(&now)
    .execute(pool)
    .await
    .map_err(|e| e.to_string())?;

    Ok(SkillMetadata {
        skill_id: skill_id.to_string(),
        notes: notes.map(str::to_string),
        tags: tags_json,
        updated_at: now,
    })
}

pub fn parse_skill_metadata_tags(metadata: Option<&SkillMetadata>) -> Vec<String> {
    metadata
        .and_then(|metadata| serde_json::from_str::<Vec<String>>(&metadata.tags).ok())
        .unwrap_or_default()
}

/// Delete a skill and all its installation records.
pub async fn delete_skill(pool: &DbPool, skill_id: &str) -> Result<(), String> {
    sqlx::query("DELETE FROM skill_installations WHERE skill_id = ?")
        .bind(skill_id)
        .execute(pool)
        .await
        .map_err(|e| e.to_string())?;
    sqlx::query("DELETE FROM skill_source_syncs WHERE skill_id = ?")
        .bind(skill_id)
        .execute(pool)
        .await
        .map_err(|e| e.to_string())?;
    sqlx::query("DELETE FROM skill_sources WHERE skill_id = ?")
        .bind(skill_id)
        .execute(pool)
        .await
        .map_err(|e| e.to_string())?;
    sqlx::query("DELETE FROM skill_metadata WHERE skill_id = ?")
        .bind(skill_id)
        .execute(pool)
        .await
        .map_err(|e| e.to_string())?;
    sqlx::query("DELETE FROM skills WHERE id = ?")
        .bind(skill_id)
        .execute(pool)
        .await
        .map(|_| ())
        .map_err(|e| e.to_string())
}

/// Delete all local database state owned by a Central Skill removal.
pub async fn delete_central_skill_records(
    pool: &DbPool,
    skill_id: &str,
    _skill_name: &str,
) -> Result<(), String> {
    sqlx::query("DELETE FROM skill_installations WHERE skill_id = ?")
        .bind(skill_id)
        .execute(pool)
        .await
        .map_err(|e| e.to_string())?;
    sqlx::query("DELETE FROM collection_skills WHERE skill_id = ?")
        .bind(skill_id)
        .execute(pool)
        .await
        .map_err(|e| e.to_string())?;
    sqlx::query("DELETE FROM skill_explanations WHERE skill_id = ?")
        .bind(skill_id)
        .execute(pool)
        .await
        .map_err(|e| e.to_string())?;
    sqlx::query("DELETE FROM skill_source_syncs WHERE skill_id = ?")
        .bind(skill_id)
        .execute(pool)
        .await
        .map_err(|e| e.to_string())?;
    sqlx::query("DELETE FROM skill_sources WHERE skill_id = ?")
        .bind(skill_id)
        .execute(pool)
        .await
        .map_err(|e| e.to_string())?;
    sqlx::query("DELETE FROM skill_metadata WHERE skill_id = ?")
        .bind(skill_id)
        .execute(pool)
        .await
        .map_err(|e| e.to_string())?;
    sqlx::query("DELETE FROM skills WHERE id = ?")
        .bind(skill_id)
        .execute(pool)
        .await
        .map(|_| ())
        .map_err(|e| e.to_string())
}

/// Delete all local database state owned by a Skill Resource Library removal.
pub async fn delete_skill_owned_records(
    pool: &DbPool,
    skill_id: &str,
    _skill_name: &str,
) -> Result<(), String> {
    let mut transaction = pool.begin().await.map_err(|e|e.to_string())?;
    sqlx::query("DELETE FROM skill_installations WHERE skill_id = ?")
        .bind(skill_id)
        .execute(&mut *transaction)
        .await
        .map_err(|e| e.to_string())?;
    sqlx::query("DELETE FROM collection_skills WHERE skill_id = ?")
        .bind(skill_id)
        .execute(&mut *transaction)
        .await
        .map_err(|e| e.to_string())?;
    sqlx::query("DELETE FROM skill_explanations WHERE skill_id = ?")
        .bind(skill_id)
        .execute(&mut *transaction)
        .await
        .map_err(|e| e.to_string())?;
    sqlx::query("DELETE FROM skill_source_syncs WHERE skill_id = ?")
        .bind(skill_id)
        .execute(&mut *transaction)
        .await
        .map_err(|e| e.to_string())?;
    sqlx::query("DELETE FROM skill_sources WHERE skill_id = ?")
        .bind(skill_id)
        .execute(&mut *transaction)
        .await
        .map_err(|e| e.to_string())?;
    sqlx::query("DELETE FROM skill_metadata WHERE skill_id = ?")
        .bind(skill_id)
        .execute(&mut *transaction)
        .await
        .map_err(|e| e.to_string())?;
    sqlx::query("DELETE FROM skills WHERE id = ?")
        .bind(skill_id)
        .execute(&mut *transaction)
        .await
        .map_err(|e| e.to_string())?;
    transaction.commit().await.map_err(|e|e.to_string())
}

// ─── Skill Installations ──────────────────────────────────────────────────────

/// Insert or update a skill installation record.
///
/// On conflict (same skill_id + agent_id), updates the mutable fields
/// (installed_path, link_type, symlink_target) but **preserves the original
/// `created_at`** so the installation timestamp reflects when the skill was
/// first installed, not when it was last re-scanned.
pub async fn upsert_skill_installation(
    pool: &DbPool,
    installation: &SkillInstallation,
) -> Result<(), String> {
    sqlx::query(
        "INSERT INTO skill_installations
         (skill_id, agent_id, installed_path, link_type, symlink_target, is_managed, created_at)
         VALUES (?, ?, ?, ?, ?, 1, ?)
         ON CONFLICT(skill_id, agent_id) DO UPDATE SET
           installed_path = excluded.installed_path,
           link_type      = excluded.link_type,
           symlink_target = excluded.symlink_target,
           is_managed     = 1",
    )
    .bind(&installation.skill_id)
    .bind(&installation.agent_id)
    .bind(&installation.installed_path)
    .bind(&installation.link_type)
    .bind(&installation.symlink_target)
    .bind(&installation.created_at)
    .execute(pool)
    .await
    .map(|_| ())
    .map_err(|e| e.to_string())
}

/// Insert or update an installation observed during a platform scan.
///
/// Scanner observations are not application-managed unless the same row was
/// already claimed by an application installation. On conflict, managed
/// ownership and its recorded path metadata remain unchanged.
pub async fn upsert_scanned_skill_installation(
    pool: &DbPool,
    installation: &SkillInstallation,
) -> Result<(), String> {
    sqlx::query(
        "INSERT INTO skill_installations
         (skill_id, agent_id, installed_path, link_type, symlink_target, is_managed, created_at)
         VALUES (?, ?, ?, ?, ?, 0, ?)
         ON CONFLICT(skill_id, agent_id) DO UPDATE SET
           installed_path = CASE WHEN skill_installations.is_managed = 1
                                 THEN skill_installations.installed_path
                                 ELSE excluded.installed_path END,
           link_type      = CASE WHEN skill_installations.is_managed = 1
                                 THEN skill_installations.link_type
                                 ELSE excluded.link_type END,
           symlink_target = CASE WHEN skill_installations.is_managed = 1
                                 THEN skill_installations.symlink_target
                                 ELSE excluded.symlink_target END",
    )
    .bind(&installation.skill_id)
    .bind(&installation.agent_id)
    .bind(&installation.installed_path)
    .bind(&installation.link_type)
    .bind(&installation.symlink_target)
    .bind(&installation.created_at)
    .execute(pool)
    .await
    .map(|_| ())
    .map_err(|e| e.to_string())
}

/// Delete an installation record for a specific skill+agent pair.
pub async fn delete_skill_installation(
    pool: &DbPool,
    skill_id: &str,
    agent_id: &str,
) -> Result<(), String> {
    sqlx::query("DELETE FROM skill_installations WHERE skill_id = ? AND agent_id = ?")
        .bind(skill_id)
        .bind(agent_id)
        .execute(pool)
        .await
        .map(|_| ())
        .map_err(|e| e.to_string())
}

/// Delete installation records for a target agent without touching files.
pub async fn delete_skill_installations_by_agent(
    pool: &DbPool,
    agent_id: &str,
) -> Result<(), String> {
    sqlx::query("DELETE FROM skill_installations WHERE agent_id = ?")
        .bind(agent_id)
        .execute(pool)
        .await
        .map(|_| ())
        .map_err(|e| e.to_string())
}

/// Reconcile installation records for an agent after a scan.
///
/// Unmanaged scanner rows follow the current scan result. Managed rows are
/// removed only when their recorded installation path itself no longer exists.
pub async fn delete_stale_skill_installations(
    pool: &DbPool,
    agent_id: &str,
    found_skill_ids: &[String],
) -> Result<(), String> {
    let found_skill_ids: HashSet<&str> = found_skill_ids.iter().map(String::as_str).collect();
    let rows = sqlx::query(
        "SELECT si.skill_id, si.installed_path, si.link_type, si.symlink_target,
                si.is_managed, s.canonical_path, s.is_central
         FROM skill_installations si
         LEFT JOIN skills s ON s.id = si.skill_id
         WHERE si.agent_id = ?",
    )
    .bind(agent_id)
    .fetch_all(pool)
    .await
    .map_err(|e| e.to_string())?;

    let stale_skill_ids: Vec<String> = rows
        .into_iter()
        .filter_map(|row| {
            let skill_id = row.get::<String, _>("skill_id");
            let installed_path = row.get::<String, _>("installed_path");
            let is_managed = row.get::<bool, _>("is_managed");
            let is_stale = if is_managed {
                matches!(
                    std::fs::symlink_metadata(&installed_path),
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound
                )
            } else {
                let is_legacy_central_link =
                    row.get::<Option<bool>, _>("is_central").unwrap_or(false)
                        && row.get::<Option<String>, _>("canonical_path").as_deref()
                            == Some(installed_path.as_str())
                        && row.get::<String, _>("link_type") == "symlink"
                        && row
                            .get::<Option<String>, _>("symlink_target")
                            .is_some_and(|target| {
                                std::fs::symlink_metadata(&installed_path)
                                    .map(|metadata| metadata.file_type().is_symlink())
                                    .unwrap_or(false)
                                    && matches!(
                                        (
                                            std::fs::canonicalize(&installed_path),
                                            std::fs::canonicalize(target)
                                        ),
                                        (Ok(installed), Ok(target)) if installed == target
                                    )
                            });

                !found_skill_ids.contains(skill_id.as_str()) && !is_legacy_central_link
            };
            is_stale.then_some(skill_id)
        })
        .collect();

    let mut transaction = pool.begin().await.map_err(|e| e.to_string())?;
    for skill_id in stale_skill_ids {
        sqlx::query("DELETE FROM skill_installations WHERE skill_id = ? AND agent_id = ?")
            .bind(skill_id)
            .bind(agent_id)
            .execute(&mut *transaction)
            .await
            .map_err(|e| e.to_string())?;
    }
    transaction.commit().await.map_err(|e| e.to_string())
}

pub async fn delete_stale_agent_skill_observations(
    pool: &DbPool,
    agent_id: &str,
    found_row_ids: &[String],
) -> Result<(), String> {
    if found_row_ids.is_empty() {
        return sqlx::query("DELETE FROM agent_skill_observations WHERE agent_id = ?")
            .bind(agent_id)
            .execute(pool)
            .await
            .map(|_| ())
            .map_err(|e| e.to_string());
    }

    let placeholders = found_row_ids
        .iter()
        .map(|_| "?")
        .collect::<Vec<_>>()
        .join(",");
    let sql = format!(
        "DELETE FROM agent_skill_observations WHERE agent_id = ? AND row_id NOT IN ({})",
        placeholders
    );

    let mut q = sqlx::query(&sql).bind(agent_id);
    for row_id in found_row_ids {
        q = q.bind(row_id.as_str());
    }
    q.execute(pool).await.map(|_| ()).map_err(|e| e.to_string())
}

/// Delete skills whose IDs are NOT in `found_skill_ids`. Also cascades to
/// remove any orphaned `skill_installations` rows for those skills.
///
/// This is the global reconciliation step run after a full scan to purge rows
/// for skills that no longer exist on disk in any scanned scope.
///
/// Pass an empty slice to delete ALL skills (used only when every scanned
/// directory is empty or missing).
pub async fn delete_skills_not_in_scope(
    pool: &DbPool,
    found_skill_ids: &[String],
) -> Result<(), String> {
    if found_skill_ids.is_empty() {
        // Nothing found — delete dependent records first, then all skills.
        sqlx::query("DELETE FROM skill_installations")
            .execute(pool)
            .await
            .map_err(|e| e.to_string())?;
        sqlx::query("DELETE FROM skill_source_syncs")
            .execute(pool)
            .await
            .map_err(|e| e.to_string())?;
        sqlx::query("DELETE FROM skill_sources")
            .execute(pool)
            .await
            .map_err(|e| e.to_string())?;
        sqlx::query("DELETE FROM skill_metadata")
            .execute(pool)
            .await
            .map_err(|e| e.to_string())?;
        return sqlx::query("DELETE FROM skills")
            .execute(pool)
            .await
            .map(|_| ())
            .map_err(|e| e.to_string());
    }

    let placeholders = found_skill_ids
        .iter()
        .map(|_| "?")
        .collect::<Vec<_>>()
        .join(",");

    // Cascade: remove installation rows for skills that are no longer on disk.
    let install_sql = format!(
        "DELETE FROM skill_installations WHERE skill_id NOT IN ({})",
        placeholders
    );
    let mut q = sqlx::query(&install_sql);
    for id in found_skill_ids {
        q = q.bind(id.as_str());
    }
    q.execute(pool).await.map_err(|e| e.to_string())?;

    // Cascade: remove source metadata rows for skills that are no longer on disk.
    let sync_sql = format!(
        "DELETE FROM skill_source_syncs WHERE skill_id NOT IN ({})",
        placeholders
    );
    let mut q_syncs = sqlx::query(&sync_sql);
    for id in found_skill_ids {
        q_syncs = q_syncs.bind(id.as_str());
    }
    q_syncs.execute(pool).await.map_err(|e| e.to_string())?;

    let source_sql = format!(
        "DELETE FROM skill_sources WHERE skill_id NOT IN ({})",
        placeholders
    );
    let mut q_sources = sqlx::query(&source_sql);
    for id in found_skill_ids {
        q_sources = q_sources.bind(id.as_str());
    }
    q_sources.execute(pool).await.map_err(|e| e.to_string())?;

    // Cascade: remove editable metadata rows for skills that are no longer on disk.
    let metadata_sql = format!(
        "DELETE FROM skill_metadata WHERE skill_id NOT IN ({})",
        placeholders
    );
    let mut q_metadata = sqlx::query(&metadata_sql);
    for id in found_skill_ids {
        q_metadata = q_metadata.bind(id.as_str());
    }
    q_metadata.execute(pool).await.map_err(|e| e.to_string())?;

    // Remove the stale skills themselves.
    let skill_sql = format!("DELETE FROM skills WHERE id NOT IN ({})", placeholders);
    let mut q2 = sqlx::query(&skill_sql);
    for id in found_skill_ids {
        q2 = q2.bind(id.as_str());
    }
    q2.execute(pool)
        .await
        .map(|_| ())
        .map_err(|e| e.to_string())
}

/// Retrieve all installation records for a given skill.
pub async fn get_skill_installations(
    pool: &DbPool,
    skill_id: &str,
) -> Result<Vec<SkillInstallation>, String> {
    sqlx::query_as::<_, SkillInstallation>(
        "SELECT skill_id, agent_id, installed_path, link_type, symlink_target, created_at
         FROM skill_installations
         WHERE skill_id = ?
           AND is_managed = 1",
    )
    .bind(skill_id)
    .fetch_all(pool)
    .await
    .map_err(|e| e.to_string())
}

/// Retrieve historical installation rows only for legacy centralization repair.
///
/// This deliberately includes unmanaged rows. Callers must independently
/// validate the recorded path, link type, symlink target, and filesystem entry
/// before mutating either the filesystem or ownership state.
pub(crate) async fn get_skill_installations_for_legacy_repair(
    pool: &DbPool,
    skill_id: &str,
) -> Result<Vec<SkillInstallation>, String> {
    sqlx::query_as::<_, SkillInstallation>(
        "SELECT skill_id, agent_id, installed_path, link_type, symlink_target, created_at
         FROM skill_installations
         WHERE skill_id = ?",
    )
    .bind(skill_id)
    .fetch_all(pool)
    .await
    .map_err(|e| e.to_string())
}

/// Count application-managed installations for a given agent.
pub async fn count_managed_skill_installations(
    pool: &DbPool,
    agent_id: &str,
) -> Result<i64, String> {
    sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*)
         FROM skill_installations
         WHERE agent_id = ?
           AND is_managed = 1",
    )
    .bind(agent_id)
    .fetch_one(pool)
    .await
    .map_err(|e| e.to_string())
}

pub async fn get_managed_skill_ids(pool: &DbPool) -> Result<Vec<String>, String> {
    sqlx::query_scalar::<_, String>(
        "SELECT DISTINCT skill_id
         FROM skill_installations
         WHERE is_managed = 1",
    )
    .fetch_all(pool)
    .await
    .map_err(|e| e.to_string())
}

pub async fn get_read_only_observed_agent_ids_for_skill(
    pool: &DbPool,
    skill_id: &str,
) -> Result<Vec<String>, String> {
    sqlx::query_scalar::<_, String>(
        "SELECT DISTINCT agent_id
         FROM agent_skill_observations
         WHERE skill_id = ?
           AND is_read_only = 1
           AND source_kind = 'compatibility'
         ORDER BY agent_id",
    )
    .bind(skill_id)
    .fetch_all(pool)
    .await
    .map_err(|e| e.to_string())
}

// ─── Agents ───────────────────────────────────────────────────────────────────

/// Retrieve all agents.
pub async fn get_all_agents(pool: &DbPool) -> Result<Vec<Agent>, String> {
    sqlx::query_as::<_, Agent>("SELECT * FROM agents ORDER BY is_builtin DESC, display_name")
        .fetch_all(pool)
        .await
        .map_err(|e| e.to_string())
}

/// Retrieve a single agent by ID.
pub async fn get_agent_by_id(pool: &DbPool, agent_id: &str) -> Result<Option<Agent>, String> {
    sqlx::query_as::<_, Agent>("SELECT * FROM agents WHERE id = ?")
        .bind(agent_id)
        .fetch_optional(pool)
        .await
        .map_err(|e| e.to_string())
}

/// Update the `is_detected` flag for an agent.
pub async fn update_agent_detected(
    pool: &DbPool,
    agent_id: &str,
    is_detected: bool,
) -> Result<(), String> {
    sqlx::query("UPDATE agents SET is_detected = ? WHERE id = ?")
        .bind(is_detected)
        .bind(agent_id)
        .execute(pool)
        .await
        .map(|_| ())
        .map_err(|e| e.to_string())
}

/// Insert a new custom agent (non-builtin).
pub async fn insert_custom_agent(pool: &DbPool, agent: &Agent) -> Result<(), String> {
    sqlx::query(
        "INSERT INTO agents
         (id, display_name, category, global_skills_dir, project_skills_dir,
          is_detected, is_builtin, is_enabled)
         VALUES (?, ?, ?, ?, ?, ?, 0, 1)",
    )
    .bind(&agent.id)
    .bind(&agent.display_name)
    .bind(&agent.category)
    .bind(&agent.global_skills_dir)
    .bind(&agent.project_skills_dir)
    .bind(agent.is_detected)
    .execute(pool)
    .await
    .map_err(|e| e.to_string())?;
    crate::platforms::persist_platform_edit(agent)?;
    Ok(())
}

/// Delete an agent by ID.
pub async fn delete_custom_agent(pool: &DbPool, agent_id: &str) -> Result<(), String> {
    let agent = get_agent_by_id(pool, agent_id).await?;
    match agent {
        None => Err(format!("Agent '{}' not found", agent_id)),
        Some(agent) => {
            sqlx::query("DELETE FROM agents WHERE id = ?")
                .bind(agent_id)
                .execute(pool)
                .await
                .map_err(|e| e.to_string())?;
            crate::platforms::persist_platform_delete(&agent.id)?;
            Ok(())
        }
    }
}

/// Update an agent's mutable fields.
/// Returns the updated agent record, or an error if the agent is not found.
pub async fn update_custom_agent(
    pool: &DbPool,
    agent_id: &str,
    next_agent_id: &str,
    display_name: &str,
    global_skills_dir: &str,
) -> Result<Agent, String> {
    let agent = get_agent_by_id(pool, agent_id)
        .await?
        .ok_or_else(|| format!("Agent '{}' not found", agent_id))?;
    if next_agent_id.trim().is_empty() {
        return Err("Agent ID cannot be empty".to_string());
    }
    if next_agent_id != agent_id && get_agent_by_id(pool, next_agent_id).await?.is_some() {
        return Err(format!("Agent ID '{}' already exists", next_agent_id));
    }

    sqlx::query("UPDATE agents SET id = ?, display_name = ?, global_skills_dir = ? WHERE id = ?")
        .bind(next_agent_id)
        .bind(display_name)
        .bind(global_skills_dir)
        .bind(agent_id)
        .execute(pool)
        .await
        .map_err(|e| e.to_string())?;
    if next_agent_id != agent_id {
        sqlx::query("UPDATE skill_installations SET agent_id = ? WHERE agent_id = ?")
            .bind(next_agent_id)
            .bind(agent_id)
            .execute(pool)
            .await
            .map_err(|e| e.to_string())?;
        sqlx::query("UPDATE agent_skill_observations SET agent_id = ? WHERE agent_id = ?")
            .bind(next_agent_id)
            .bind(agent_id)
            .execute(pool)
            .await
            .map_err(|e| e.to_string())?;
    }

    let updated = get_agent_by_id(pool, next_agent_id)
        .await?
        .ok_or_else(|| "Failed to retrieve updated agent".to_string())?;
    if updated.id != agent.id {
        crate::platforms::persist_platform_rename(&agent.id, &updated)?;
    } else {
        crate::platforms::persist_platform_edit(&updated)?;
    }
    Ok(updated)
}

/// Update whether a software platform is enabled in the DB index and config.json.
pub async fn set_agent_enabled(
    pool: &DbPool,
    agent_id: &str,
    enabled: bool,
) -> Result<Agent, String> {
    if agent_id == "central" || agent_id.starts_with("project:") {
        return Err("This target cannot be enabled or disabled here".to_string());
    }
    get_agent_by_id(pool, agent_id)
        .await?
        .ok_or_else(|| format!("Agent '{}' not found", agent_id))?;
    sqlx::query("UPDATE agents SET is_enabled = ? WHERE id = ?")
        .bind(enabled)
        .bind(agent_id)
        .execute(pool)
        .await
        .map_err(|e| e.to_string())?;
    let updated = get_agent_by_id(pool, agent_id)
        .await?
        .ok_or_else(|| "Failed to retrieve updated agent".to_string())?;
    crate::platforms::persist_platform_edit(&updated)?;
    Ok(updated)
}

pub async fn update_central_agent_skills_dir(
    pool: &DbPool,
    global_skills_dir: &str,
) -> Result<Agent, String> {
    sqlx::query("UPDATE agents SET global_skills_dir = ? WHERE id = 'central'")
        .bind(global_skills_dir)
        .execute(pool)
        .await
        .map_err(|e| e.to_string())?;

    let updated = get_agent_by_id(pool, "central")
        .await?
        .ok_or_else(|| "Central agent not found".to_string())?;
    crate::platforms::persist_platform_edit(&updated)?;
    Ok(updated)
}

// ─── Collections ──────────────────────────────────────────────────────────────

/// Create a new collection and return it.
pub async fn create_collection(
    pool: &DbPool,
    name: &str,
    description: Option<&str>,
) -> Result<Collection, String> {
    let id = Uuid::new_v4().to_string();
    let now = Utc::now().to_rfc3339();

    sqlx::query(
        "INSERT INTO collections (id, name, description, created_at, updated_at)
         VALUES (?, ?, ?, ?, ?)",
    )
    .bind(&id)
    .bind(name)
    .bind(description)
    .bind(&now)
    .bind(&now)
    .execute(pool)
    .await
    .map_err(|e| e.to_string())?;

    get_collection_by_id(pool, &id)
        .await?
        .ok_or_else(|| "Failed to retrieve newly created collection".to_string())
}

/// Retrieve all collections.
pub async fn get_all_collections(pool: &DbPool) -> Result<Vec<Collection>, String> {
    sqlx::query_as::<_, Collection>("SELECT * FROM collections ORDER BY created_at")
        .fetch_all(pool)
        .await
        .map_err(|e| e.to_string())
}

/// Retrieve a collection by ID.
pub async fn get_collection_by_id(
    pool: &DbPool,
    collection_id: &str,
) -> Result<Option<Collection>, String> {
    sqlx::query_as::<_, Collection>("SELECT * FROM collections WHERE id = ?")
        .bind(collection_id)
        .fetch_optional(pool)
        .await
        .map_err(|e| e.to_string())
}

/// Update a collection's name and/or description.
pub async fn update_collection(
    pool: &DbPool,
    collection_id: &str,
    name: &str,
    description: Option<&str>,
) -> Result<(), String> {
    let now = Utc::now().to_rfc3339();
    sqlx::query("UPDATE collections SET name = ?, description = ?, updated_at = ? WHERE id = ?")
        .bind(name)
        .bind(description)
        .bind(&now)
        .bind(collection_id)
        .execute(pool)
        .await
        .map(|_| ())
        .map_err(|e| e.to_string())
}

/// Delete a collection and all its skill memberships.
pub async fn delete_collection(pool: &DbPool, collection_id: &str) -> Result<(), String> {
    sqlx::query("DELETE FROM collection_skills WHERE collection_id = ?")
        .bind(collection_id)
        .execute(pool)
        .await
        .map_err(|e| e.to_string())?;
    sqlx::query("DELETE FROM collections WHERE id = ?")
        .bind(collection_id)
        .execute(pool)
        .await
        .map(|_| ())
        .map_err(|e| e.to_string())
}

/// Add a skill to a collection (idempotent).
pub async fn add_skill_to_collection(
    pool: &DbPool,
    collection_id: &str,
    skill_id: &str,
) -> Result<(), String> {
    let now = Utc::now().to_rfc3339();
    sqlx::query(
        "INSERT OR IGNORE INTO collection_skills (collection_id, skill_id, added_at)
         VALUES (?, ?, ?)",
    )
    .bind(collection_id)
    .bind(skill_id)
    .bind(&now)
    .execute(pool)
    .await
    .map(|_| ())
    .map_err(|e| e.to_string())
}

/// Remove a skill from a collection.
pub async fn remove_skill_from_collection(
    pool: &DbPool,
    collection_id: &str,
    skill_id: &str,
) -> Result<(), String> {
    sqlx::query("DELETE FROM collection_skills WHERE collection_id = ? AND skill_id = ?")
        .bind(collection_id)
        .bind(skill_id)
        .execute(pool)
        .await
        .map(|_| ())
        .map_err(|e| e.to_string())
}

/// Retrieve all skills belonging to a collection.
pub async fn get_collection_skills(
    pool: &DbPool,
    collection_id: &str,
) -> Result<Vec<Skill>, String> {
    sqlx::query_as::<_, Skill>(
        "SELECT s.* FROM skills s
         JOIN collection_skills cs ON s.id = cs.skill_id
         WHERE cs.collection_id = ?
         ORDER BY cs.added_at",
    )
    .bind(collection_id)
    .fetch_all(pool)
    .await
    .map_err(|e| e.to_string())
}

/// Retrieve all collections that contain a given skill.
pub async fn get_skill_collections(
    pool: &DbPool,
    skill_id: &str,
) -> Result<Vec<Collection>, String> {
    sqlx::query_as::<_, Collection>(
        "SELECT c.* FROM collections c
         JOIN collection_skills cs ON c.id = cs.collection_id
         WHERE cs.skill_id = ?
         ORDER BY cs.added_at",
    )
    .bind(skill_id)
    .fetch_all(pool)
    .await
    .map_err(|e| e.to_string())
}

// ─── Scan Directories ─────────────────────────────────────────────────────────

/// Configuration accessors keep the existing IPC contract while storing no settings in SQLite.
pub async fn get_scan_directories(pool: &DbPool) -> Result<Vec<ScanDirectory>, String> {
    let mut dirs = crate::db::load_config(pool)?.scan_directories;
    dirs.sort_by(|a, b| b.is_builtin.cmp(&a.is_builtin).then(a.added_at.cmp(&b.added_at)));
    Ok(dirs)
}

pub async fn get_scan_directory_by_id(pool: &DbPool, id: i64) -> Result<Option<ScanDirectory>, String> {
    Ok(get_scan_directories(pool).await?.into_iter().find(|dir| dir.id == id))
}

fn next_scan_directory_id(config: &mut crate::config_store::AppConfig) -> i64 {
    let id = config.next_scan_directory_id.max(1).max(config.scan_directories.iter().map(|dir| dir.id + 1).max().unwrap_or(1));
    config.next_scan_directory_id = id + 1;
    id
}

pub async fn add_scan_directory(pool: &DbPool, path: &str, label: Option<&str>) -> Result<ScanDirectory, String> {
    crate::db::update_config(pool, |config| {
        if config.scan_directories.iter().any(|dir| dir.path == path) {
            return Err("Scan directory already exists".into());
        }
        let dir = ScanDirectory { id: next_scan_directory_id(config), path: path.into(), label: label.map(str::to_string),
            is_active: true, is_builtin: false, added_at: Utc::now().to_rfc3339() };
        config.scan_directories.push(dir.clone());
        Ok(dir)
    })
}

pub async fn update_scan_directory(pool: &DbPool, path: &str, new_path: &str, label: Option<&str>) -> Result<ScanDirectory, String> {
    if new_path.trim().is_empty() { return Err("Scan directory path cannot be empty".into()); }
    crate::db::update_config(pool, |config| {
        if config.scan_directories.iter().any(|dir| dir.path == new_path && dir.path != path) {
            return Err("Scan directory already exists".into());
        }
        let dir = config.scan_directories.iter_mut().find(|dir| dir.path == path).ok_or("Scan directory not found")?;
        if dir.is_builtin { return Err("Cannot update built-in scan directory".into()); }
        dir.path = new_path.into();
        dir.label = label.map(str::to_string);
        Ok(dir.clone())
    })
}

pub async fn remove_scan_directory(pool: &DbPool, path: &str) -> Result<(), String> {
    let dir = get_scan_directories(pool).await?.into_iter().find(|dir| dir.path == path).ok_or("Scan directory not found")?;
    if dir.is_builtin { return Err("Cannot remove built-in scan directory".into()); }
    crate::db::update_config(pool, |config| {
        config.scan_directories.retain(|entry| entry.id != dir.id);
        Ok(())
    })?;
    delete_skill_installations_by_agent(pool, &format!("project:{}", dir.id)).await
}

pub async fn toggle_scan_directory(pool: &DbPool, path: &str, is_active: bool) -> Result<(), String> {
    crate::db::update_config(pool, |config| {
        let dir = config.scan_directories.iter_mut().find(|dir| dir.path == path).ok_or("Scan directory not found")?;
        dir.is_active = is_active;
        Ok(())
    })
}

pub async fn upsert_builtin_scan_directory(pool: &DbPool, path: &str, label: &str) -> Result<(), String> {
    crate::db::update_config(pool, |config| {
        if let Some(dir) = config.scan_directories.iter_mut().find(|dir| dir.path == path) {
            dir.label = Some(label.into());
            dir.is_builtin = true;
            dir.is_active = true;
        } else {
            let id = next_scan_directory_id(config);
            config.scan_directories.push(ScanDirectory { id, path: path.into(), label: Some(label.into()),
                is_active: true, is_builtin: true, added_at: Utc::now().to_rfc3339() });
        }
        Ok(())
    })
}

pub async fn get_setting(pool: &DbPool, key: &str) -> Result<Option<String>, String> {
    Ok(crate::db::load_config(pool)?.settings.get(key).cloned())
}

pub async fn set_setting(pool: &DbPool, key: &str, value: &str) -> Result<(), String> {
    crate::db::update_config(pool, |config| {
        config.settings.insert(key.into(), value.into());
        Ok(())
    })
}

pub async fn save_github_stars(pool: &DbPool, repository: &str, stars: i64) -> Result<(), String> {
    save_github_stars_with_executor(pool, repository, stars).await
}

pub(crate) async fn save_github_stars_with_executor<'e>(executor: impl sqlx::Executor<'e, Database = sqlx::Sqlite>, repository: &str, stars: i64) -> Result<(), String> {
    sqlx::query("INSERT INTO github_repository_stats (repository, stars, checked_at) VALUES (?, ?, ?) ON CONFLICT(repository) DO UPDATE SET stars=excluded.stars, checked_at=excluded.checked_at")
        .bind(repository).bind(stars).bind(Utc::now().to_rfc3339()).execute(executor).await.map_err(|e| e.to_string())?;
    Ok(())
}

pub async fn get_github_stars(pool: &DbPool, repository: &str) -> Result<Option<i64>, String> {
    sqlx::query_scalar("SELECT stars FROM github_repository_stats WHERE repository = ?")
        .bind(repository).fetch_optional(pool).await.map_err(|e| e.to_string())
}

/// Preserve membership identities even if an old store contains an orphaned member.
pub async fn collection_member_ids(pool: &DbPool, collection_id: &str) -> Result<Vec<String>, String> {
    sqlx::query_scalar("SELECT skill_id FROM collection_skills WHERE collection_id = ?")
        .bind(collection_id).fetch_all(pool).await.map_err(|e| e.to_string())
}

pub async fn managed_central_paths(pool: &DbPool) -> Result<Vec<(String, String)>, String> {
    sqlx::query_as("SELECT skill_id,installed_path FROM skill_installations WHERE agent_id='central' AND is_managed=1")
        .fetch_all(pool).await.map_err(|e| e.to_string())
}

pub async fn has_pending_address_moves(pool: &DbPool) -> Result<bool, String> {
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM repository_address_moves")
        .fetch_one(pool).await.map_err(|e| e.to_string())?;
    Ok(count != 0)
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::path_utils::resolve_home_dir;
    use tempfile::tempdir;

    /// Create an in-memory SQLite pool and initialize the schema.
    async fn setup_test_db() -> DbPool {
        let pool = SqlitePool::connect(":memory:")
            .await
            .expect("Failed to create in-memory SQLite pool");
        init_database(&pool)
            .await
            .expect("Failed to initialize test database");
        pool
    }

    #[tokio::test]
    async fn successful_check_preserves_installed_sync_baseline() {
        let pool = setup_test_db().await;
        upsert_skill_source(&pool, &SkillSource {skill_id: "check-only".into(), source_type: "github".into(),
            source_url: None, source_author: None, source_repo: Some("example/repo".into()), source_path: None,
            updated_at: "2026-01-01T00:00:00Z".into()}).await.unwrap();
        let baseline = SkillSourceSync {skill_id: "check-only".into(), sync_scope: "github-repository".into(),
            remote_ref: Some("installed-commit".into()), skill_fingerprint: Some("installed-content".into()),
            last_checked_at: None, last_sync_at: Some("2026-01-01T00:00:00Z".into()),
            sync_status: "success".into(), sync_error: None, remote_deleted: false, updated_at: "2026-01-01T00:00:00Z".into()};
        upsert_skill_source_sync(&pool, &baseline).await.unwrap();
        let time = "2026-10-08T08:00:00Z";
        record_skill_check_times(&pool, &[baseline.skill_id.clone()], time).await.unwrap();
        let result = get_skill_source_sync(&pool, &baseline.skill_id).await.unwrap().unwrap();
        assert_eq!(result.last_checked_at.as_deref(), Some(time));
        assert_eq!(result.remote_ref, baseline.remote_ref);
        assert_eq!(result.skill_fingerprint, baseline.skill_fingerprint);
        assert_eq!(result.last_sync_at, baseline.last_sync_at);
        assert_eq!(result.sync_status, baseline.sync_status);
    }

    #[tokio::test]
    async fn prepare_config_dir_writes_platform_files_and_empty_schema() {
        let dir = tempdir().expect("temp dir");
        prepare_config_dir(dir.path()).await.unwrap();

        assert!(!dir.path().join("platform/platform.json").exists());
        assert!(dir.path().join("config.json").exists());
        assert!(!dir.path().join("platform").exists());
        assert!(!dir.path().join("library").exists());
        assert!(dir.path().join("db.sqlite").exists());
        assert!(dir.path().join("icons/cursor.svg").exists());
        assert!(dir.path().join("icons/SOURCES.md").exists());

        let catalog = std::fs::read_to_string(dir.path().join("config.json")).unwrap();
        assert!(catalog.contains("~/.claude/skills"));
        assert!(catalog.contains("\"enabled\": true"));

        let pool = create_pool(&path_to_string(&dir.path().join("db.sqlite")))
            .await
            .unwrap();
        let agent_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM agents")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(
            agent_count, 0,
            "packaged databases must not bake builder home paths into agent rows"
        );
    }

    #[tokio::test]
    async fn github_stars_cache_is_case_insensitive_and_keeps_zero() {
        let pool = setup_test_db().await;
        assert_eq!(get_github_stars(&pool, "Example/Repo").await.unwrap(), None);
        save_github_stars(&pool, "Example/Repo", 1200).await.unwrap();
        assert_eq!(get_github_stars(&pool, "example/repo").await.unwrap(), Some(1200));
        save_github_stars(&pool, "EXAMPLE/REPO", 0).await.unwrap();
        assert_eq!(get_github_stars(&pool, "Example/Repo").await.unwrap(), Some(0));
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM github_repository_stats").fetch_one(&pool).await.unwrap();
        assert_eq!(count, 1);
    }

    // ── Init ──────────────────────────────────────────────────────────────────

    #[tokio::test]
    async fn test_init_creates_all_tables() {
        let pool = setup_test_db().await;

        // Verify all core tables exist by counting rows (empty is fine)
        let tables = [
            "skills",
            "skill_installations",
            "skill_sources",
            "skill_source_syncs",
            "agent_skill_observations",
            "agents",
            "collections",
            "collection_skills",
        ];
        for table in &tables {
            let result = sqlx::query(&format!("SELECT COUNT(*) as cnt FROM {}", table))
                .fetch_one(&pool)
                .await;
            assert!(result.is_ok(), "Table '{}' should exist", table);
        }
    }

    #[tokio::test]
    async fn test_init_is_idempotent() {
        let pool = setup_test_db().await;
        // Calling init_database again should not fail
        let result = init_database(&pool).await;
        assert!(result.is_ok(), "Second init should be idempotent");
    }

    #[tokio::test]
    async fn test_upsert_and_get_skill_source_sync() {
        let pool = setup_test_db().await;
        let now = "2026-08-13T10:00:00Z".to_string();
        upsert_skill_source(
            &pool,
            &SkillSource {
                skill_id: "ask-matt".to_string(),
                source_type: "github".to_string(),
                source_url: Some("mattpocock/skills".to_string()),
                source_author: Some("skills@1.0.0".to_string()),
                source_repo: Some("mattpocock/skills".to_string()),
                source_path: Some("ask-matt".to_string()),
                updated_at: now.clone(),
            },
        )
        .await
        .unwrap();
        let sync = SkillSourceSync {
            skill_id: "ask-matt".to_string(),
            sync_scope: "github-repository".to_string(),
            remote_ref: Some("abc123".to_string()),
            skill_fingerprint: Some("hash-a".to_string()),
            last_checked_at: Some(now.clone()),
            last_sync_at: Some(now.clone()),
            sync_status: "success".to_string(),
            sync_error: None,
            remote_deleted: false,
            updated_at: now.clone(),
        };

        upsert_skill_source_sync(&pool, &sync).await.unwrap();
        let stored = get_skill_source_sync(&pool, "ask-matt")
            .await
            .unwrap()
            .expect("sync row");
        assert_eq!(stored.sync_scope, "github-repository");
        assert_eq!(stored.remote_ref.as_deref(), Some("abc123"));
        assert!(!stored.remote_deleted);

        upsert_skill_source_sync(
            &pool,
            &SkillSourceSync {
                sync_scope: "skill".to_string(),
                remote_ref: Some("def456".to_string()),
                sync_status: "unchanged".to_string(),
                remote_deleted: true,
                ..sync
            },
        )
        .await
        .unwrap();
        let updated = get_skill_source_sync(&pool, "ask-matt")
            .await
            .unwrap()
            .expect("updated sync row");
        assert_eq!(updated.sync_scope, "skill");
        assert_eq!(updated.remote_ref.as_deref(), Some("def456"));
        assert_eq!(updated.sync_status, "unchanged");
        assert!(updated.remote_deleted);
    }

    #[tokio::test]
    async fn test_builtin_agents_seeded() {
        let pool = setup_test_db().await;
        let agents = get_all_agents(&pool).await.unwrap();
        assert_eq!(
            agents.len(),
            builtin_agents().len(),
            "Should have exactly the configured built-in agents"
        );

        let ids: Vec<&str> = agents.iter().map(|a| a.id.as_str()).collect();
        // Coding platforms
        assert!(ids.contains(&"claude-code"));
        assert!(ids.contains(&"codex"));
        assert!(ids.contains(&"cursor"));
        assert!(ids.contains(&"antigravity"));
        assert!(ids.contains(&"cline"));
        assert!(!ids.contains(&"deep-agents"));
        assert!(!ids.contains(&"dexto"));
        assert!(!ids.contains(&"firebender"));
        assert!(ids.contains(&"gemini-cli"));
        assert!(ids.contains(&"kimi-code-cli"));
        assert!(ids.contains(&"trae"));
        assert!(!ids.contains(&"factory-droid"));
        assert!(!ids.contains(&"junie"));
        assert!(ids.contains(&"qwen"));
        assert!(ids.contains(&"trae-cn"));
        assert!(ids.contains(&"windsurf"));
        assert!(ids.contains(&"qoder"));
        assert!(!ids.contains(&"augment"));
        assert!(ids.contains(&"opencode"));
        assert!(!ids.contains(&"kilocode"));
        assert!(!ids.contains(&"ob1"));
        assert!(!ids.contains(&"amp"));
        assert!(ids.contains(&"kiro"));
        assert!(ids.contains(&"codebuddy"));
        assert!(!ids.contains(&"aider-desk"));
        assert!(!ids.contains(&"bob"));
        assert!(ids.contains(&"codearts-agent"));
        assert!(!ids.contains(&"codemaker"));
        assert!(!ids.contains(&"codestudio"));
        assert!(!ids.contains(&"command-code"));
        assert!(!ids.contains(&"continue"));
        assert!(!ids.contains(&"cortex"));
        assert!(!ids.contains(&"crush"));
        assert!(!ids.contains(&"devin"));
        assert!(!ids.contains(&"forgecode"));
        assert!(!ids.contains(&"goose"));
        assert!(!ids.contains(&"iflow-cli"));
        assert!(!ids.contains(&"kode"));
        assert!(!ids.contains(&"mcpjam"));
        assert!(!ids.contains(&"mistral-vibe"));
        assert!(!ids.contains(&"mux"));
        assert!(!ids.contains(&"openhands"));
        assert!(ids.contains(&"pi"));
        assert!(!ids.contains(&"rovodev"));
        assert!(!ids.contains(&"roo"));
        assert!(!ids.contains(&"tabnine-cli"));
        assert!(!ids.contains(&"zencoder"));
        assert!(!ids.contains(&"neovate"));
        assert!(!ids.contains(&"pochi"));
        assert!(!ids.contains(&"adal"));
        assert!(ids.contains(&"hermes"));
        assert!(ids.contains(&"copilot"));
        assert!(ids.contains(&"warp"));
        assert!(!ids.contains(&"aider"));
        // Additional supported platforms
        assert!(ids.contains(&"openclaw"));
        assert!(!ids.contains(&"qclaw"));
        assert!(!ids.contains(&"easyclaw"));
        assert!(!ids.contains(&"autoclaw"));
        assert!(ids.contains(&"workbuddy"));
        // Central
        assert!(ids.contains(&"central"));
    }

    #[test]
    fn test_builtin_agents_have_expected_retained_platform_skill_dirs() {
        let home = resolve_home_dir();
        let agents_by_id: std::collections::HashMap<String, Agent> = builtin_agents()
            .into_iter()
            .map(|agent| (agent.id.clone(), agent))
            .collect();

        let expected = [
            (
                "claude-code",
                "Claude Code",
                ".claude/skills",
                ".claude/skills",
            ),
            ("openclaw", "OpenClaw", ".openclaw/skills", "skills"),
            (
                "codearts-agent",
                "CodeArts Agent",
                ".codeartsdoer/skills",
                ".codeartsdoer/skills",
            ),
            (
                "codebuddy",
                "CodeBuddy",
                ".codebuddy/skills",
                ".codebuddy/skills",
            ),
            ("kiro", "Kiro CLI", ".kiro/skills", ".kiro/skills"),
            ("pi", "Pi", ".pi/agent/skills", ".pi/skills"),
            ("qoder", "Qoder", ".qoder/skills", ".qoder/skills"),
            ("qwen", "Qwen Code", ".qwen/skills", ".qwen/skills"),
            ("trae", "Trae", ".trae/skills", ".trae/skills"),
            ("trae-cn", "Trae CN", ".trae-cn/skills", ".trae/skills"),
            (
                "windsurf",
                "Windsurf",
                ".codeium/windsurf/skills",
                ".windsurf/skills",
            ),
        ];

        for (id, display_name, global_rel, project_rel) in expected {
            let agent = agents_by_id
                .get(id)
                .unwrap_or_else(|| panic!("missing built-in agent {id}"));
            assert_eq!(agent.display_name, display_name, "display_name for {id}");
            assert_eq!(
                agent.global_skills_dir,
                path_to_string(&home.join(global_rel)),
                "global_skills_dir for {id}"
            );
            assert_eq!(
                agent.project_skills_dir.as_deref(),
                Some(project_rel),
                "project_skills_dir for {id}"
            );
        }

        assert!(!agents_by_id.contains_key("droid"));
        assert!(!agents_by_id.contains_key("qwen-code"));
        assert!(!agents_by_id.contains_key("kilo"));
        assert!(!agents_by_id.contains_key("kiro-cli"));
    }

    #[test]
    fn test_codex_uses_private_skills_dir_separate_from_central() {
        let home = resolve_home_dir();
        let agents_by_id: std::collections::HashMap<String, Agent> = builtin_agents()
            .into_iter()
            .map(|agent| (agent.id.clone(), agent))
            .collect();

        let codex = agents_by_id.get("codex").expect("missing Codex agent");
        let central = agents_by_id.get("central").expect("missing central agent");

        assert_eq!(
            codex.global_skills_dir,
            path_to_string(&home.join(".codex/skills"))
        );
        assert_ne!(
            codex.global_skills_dir, central.global_skills_dir,
            "installing to Codex must not write into the central skills directory"
        );
        assert_eq!(codex.project_skills_dir.as_deref(), Some(".agents/skills"));
        assert!(agent_supports_universal_agents_skills("codex"));
    }

    #[tokio::test]
    async fn test_builtin_agents_are_marked_builtin() {
        let pool = setup_test_db().await;
        let agents = get_all_agents(&pool).await.unwrap();
        for agent in &agents {
            assert!(agent.is_builtin, "All seeded agents should be builtin");
        }
    }

    #[tokio::test]
    async fn test_init_does_not_duplicate_agents_on_reinit() {
        let pool = setup_test_db().await;
        init_database(&pool).await.unwrap(); // Call a second time
        let agents = get_all_agents(&pool).await.unwrap();
        assert_eq!(
            agents.len(),
            builtin_agents().len(),
            "Reinit must not duplicate agents"
        );
    }

    // ── Skills ────────────────────────────────────────────────────────────────

    fn make_skill(id: &str, name: &str, is_central: bool) -> Skill {
        Skill {
            id: id.to_string(),
            name: name.to_string(),
            description: Some(format!("Description for {}", name)),
            file_path: format!("/tmp/{}/SKILL.md", id),
            canonical_path: if is_central {
                Some(format!("/tmp/.agents/skills/{}", id))
            } else {
                None
            },
            is_central,
            source: None,
            content: Some("# Test Skill\n\nContent here.".to_string()),
            scanned_at: Utc::now().to_rfc3339(),
        }
    }

    #[tokio::test]
    async fn test_upsert_skill_insert() {
        let pool = setup_test_db().await;
        let skill = make_skill("test-skill", "Test Skill", false);
        upsert_skill(&pool, &skill).await.unwrap();

        let retrieved = get_skill_by_id(&pool, "test-skill").await.unwrap();
        assert!(retrieved.is_some());
        let s = retrieved.unwrap();
        assert_eq!(s.name, "Test Skill");
        assert!(!s.is_central);
    }

    #[tokio::test]
    async fn test_upsert_skill_update() {
        let pool = setup_test_db().await;
        let mut skill = make_skill("skill-1", "Original Name", false);
        upsert_skill(&pool, &skill).await.unwrap();

        skill.name = "Updated Name".to_string();
        upsert_skill(&pool, &skill).await.unwrap();

        let retrieved = get_skill_by_id(&pool, "skill-1").await.unwrap().unwrap();
        assert_eq!(retrieved.name, "Updated Name");
    }

    #[tokio::test]
    async fn test_upsert_skill_noncentral_scan_does_not_overwrite_central_metadata() {
        let pool = setup_test_db().await;
        let central = make_skill("shared-skill", "Central Name", true);
        upsert_skill(&pool, &central).await.unwrap();

        let mut platform = make_skill("shared-skill", "Platform Name", false);
        platform.description = Some("Platform description".to_string());
        platform.file_path = "/tmp/.hermes/skills/category/shared-skill/SKILL.md".to_string();
        platform.source = Some("copy".to_string());
        platform.scanned_at = "2026-04-28T00:00:00Z".to_string();
        upsert_skill(&pool, &platform).await.unwrap();

        let retrieved = get_skill_by_id(&pool, "shared-skill")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(retrieved.name, "Central Name");
        assert_eq!(retrieved.description, central.description);
        assert_eq!(retrieved.file_path, central.file_path);
        assert_eq!(retrieved.canonical_path, central.canonical_path);
        assert!(retrieved.is_central);
        assert_eq!(retrieved.scanned_at, "2026-04-28T00:00:00Z");
    }

    #[tokio::test]
    async fn test_get_skill_by_id_not_found() {
        let pool = setup_test_db().await;
        let result = get_skill_by_id(&pool, "nonexistent").await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_get_central_skills() {
        let pool = setup_test_db().await;
        upsert_skill(&pool, &make_skill("central-1", "Central One", true))
            .await
            .unwrap();
        upsert_skill(&pool, &make_skill("non-central", "Not Central", false))
            .await
            .unwrap();

        let central = get_central_skills(&pool).await.unwrap();
        assert_eq!(central.len(), 1);
        assert_eq!(central[0].id, "central-1");
    }

    #[tokio::test]
    async fn test_delete_skill() {
        let pool = setup_test_db().await;
        let skill = make_skill("to-delete", "Delete Me", false);
        upsert_skill(&pool, &skill).await.unwrap();

        delete_skill(&pool, "to-delete").await.unwrap();
        let result = get_skill_by_id(&pool, "to-delete").await.unwrap();
        assert!(result.is_none());
    }

    // ── Skill Installations ───────────────────────────────────────────────────

    fn make_installation(skill_id: &str, agent_id: &str, link_type: &str) -> SkillInstallation {
        SkillInstallation {
            skill_id: skill_id.to_string(),
            agent_id: agent_id.to_string(),
            installed_path: format!("/tmp/{}/{}", agent_id, skill_id),
            link_type: link_type.to_string(),
            symlink_target: if link_type == "symlink" {
                Some(format!("/tmp/.agents/skills/{}", skill_id))
            } else {
                None
            },
            created_at: Utc::now().to_rfc3339(),
        }
    }

    #[tokio::test]
    async fn test_upsert_and_get_skill_installation() {
        let pool = setup_test_db().await;
        let skill = make_skill("my-skill", "My Skill", false);
        upsert_skill(&pool, &skill).await.unwrap();

        let inst = make_installation("my-skill", "claude-code", "symlink");
        upsert_skill_installation(&pool, &inst).await.unwrap();

        let installations = get_skill_installations(&pool, "my-skill").await.unwrap();
        assert_eq!(installations.len(), 1);
        assert_eq!(installations[0].agent_id, "claude-code");
        assert_eq!(installations[0].link_type, "symlink");
    }

    #[tokio::test]
    async fn test_delete_skill_installation() {
        let pool = setup_test_db().await;
        let skill = make_skill("del-skill", "Del Skill", false);
        upsert_skill(&pool, &skill).await.unwrap();
        upsert_skill_installation(&pool, &make_installation("del-skill", "cursor", "copy"))
            .await
            .unwrap();

        delete_skill_installation(&pool, "del-skill", "cursor")
            .await
            .unwrap();

        let installations = get_skill_installations(&pool, "del-skill").await.unwrap();
        assert!(installations.is_empty());
    }

    #[tokio::test]
    async fn test_get_skills_by_agent() {
        let pool = setup_test_db().await;
        let skill_a = make_skill("skill-a", "Skill A", false);
        let skill_b = make_skill("skill-b", "Skill B", false);
        upsert_skill(&pool, &skill_a).await.unwrap();
        upsert_skill(&pool, &skill_b).await.unwrap();

        upsert_skill_installation(
            &pool,
            &make_installation("skill-a", "claude-code", "symlink"),
        )
        .await
        .unwrap();
        upsert_skill_installation(&pool, &make_installation("skill-b", "cursor", "copy"))
            .await
            .unwrap();

        let claude_skills = get_skills_by_agent(&pool, "claude-code").await.unwrap();
        assert_eq!(claude_skills.len(), 1);
        assert_eq!(claude_skills[0].id, "skill-a");

        let cursor_skills = get_skills_by_agent(&pool, "cursor").await.unwrap();
        assert_eq!(cursor_skills.len(), 1);
        assert_eq!(cursor_skills[0].id, "skill-b");

        let empty = get_skills_by_agent(&pool, "codex").await.unwrap();
        assert!(empty.is_empty());
    }

    #[tokio::test]
    async fn test_managed_installation_scanned_row_is_excluded_from_queries() {
        let pool = setup_test_db().await;
        upsert_skill(&pool, &make_skill("scanned-only", "Scanned Only", false))
            .await
            .unwrap();

        upsert_scanned_skill_installation(
            &pool,
            &make_installation("scanned-only", "cursor", "native"),
        )
        .await
        .unwrap();

        assert!(get_skill_installations(&pool, "scanned-only")
            .await
            .unwrap()
            .is_empty());
        assert!(get_skills_by_agent(&pool, "cursor")
            .await
            .unwrap()
            .is_empty());
        assert!(get_skills_for_agent(&pool, "cursor")
            .await
            .unwrap()
            .is_empty());
        assert_eq!(
            count_managed_skill_installations(&pool, "cursor")
                .await
                .unwrap(),
            0
        );
    }

    #[tokio::test]
    async fn test_managed_installation_application_upsert_upgrades_scanned_row() {
        let pool = setup_test_db().await;
        upsert_skill(&pool, &make_skill("upgraded", "Upgraded", false))
            .await
            .unwrap();
        let installation = make_installation("upgraded", "cursor", "native");

        upsert_scanned_skill_installation(&pool, &installation)
            .await
            .unwrap();
        upsert_skill_installation(&pool, &installation)
            .await
            .unwrap();

        let is_managed: i64 = sqlx::query_scalar(
            "SELECT is_managed FROM skill_installations
             WHERE skill_id = ? AND agent_id = ?",
        )
        .bind("upgraded")
        .bind("cursor")
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(is_managed, 1);
        assert_eq!(
            get_skill_installations(&pool, "upgraded")
                .await
                .unwrap()
                .len(),
            1
        );
        assert_eq!(
            count_managed_skill_installations(&pool, "cursor")
                .await
                .unwrap(),
            1
        );
    }

    #[tokio::test]
    async fn test_managed_installation_rescan_preserves_ownership_and_recorded_path() {
        let pool = setup_test_db().await;
        upsert_skill(&pool, &make_skill("re-scanned", "Re-scanned", false))
            .await
            .unwrap();
        let managed = make_installation("re-scanned", "cursor", "symlink");
        upsert_skill_installation(&pool, &managed).await.unwrap();

        let mut scanned = managed.clone();
        scanned.installed_path = "/tmp/cursor/re-scanned-updated".to_string();
        scanned.link_type = "native".to_string();
        scanned.symlink_target = None;
        upsert_scanned_skill_installation(&pool, &scanned)
            .await
            .unwrap();

        let row = sqlx::query(
            "SELECT is_managed, installed_path, link_type, symlink_target
             FROM skill_installations
             WHERE skill_id = ? AND agent_id = ?",
        )
        .bind("re-scanned")
        .bind("cursor")
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(row.get::<i64, _>("is_managed"), 1);
        assert_eq!(
            row.get::<String, _>("installed_path"),
            managed.installed_path
        );
        assert_eq!(row.get::<String, _>("link_type"), managed.link_type);
        assert_eq!(
            row.get::<Option<String>, _>("symlink_target"),
            managed.symlink_target
        );
        assert_eq!(
            count_managed_skill_installations(&pool, "cursor")
                .await
                .unwrap(),
            1
        );
    }

    // ── Agents ────────────────────────────────────────────────────────────────

    #[tokio::test]
    async fn test_get_agent_by_id() {
        let pool = setup_test_db().await;
        let agent = get_agent_by_id(&pool, "claude-code").await.unwrap();
        assert!(agent.is_some());
        let a = agent.unwrap();
        assert_eq!(a.display_name, "Claude Code");
        assert_eq!(a.category, "platform");
        assert!(a.is_builtin);
    }

    #[tokio::test]
    async fn test_get_agent_by_id_not_found() {
        let pool = setup_test_db().await;
        let agent = get_agent_by_id(&pool, "nonexistent-agent").await.unwrap();
        assert!(agent.is_none());
    }

    #[tokio::test]
    async fn test_update_agent_detected() {
        let pool = setup_test_db().await;
        update_agent_detected(&pool, "cursor", true).await.unwrap();
        let agent = get_agent_by_id(&pool, "cursor").await.unwrap().unwrap();
        assert!(agent.is_detected);

        update_agent_detected(&pool, "cursor", false).await.unwrap();
        let agent = get_agent_by_id(&pool, "cursor").await.unwrap().unwrap();
        assert!(!agent.is_detected);
    }

    #[tokio::test]
    async fn test_insert_custom_agent() {
        let pool = setup_test_db().await;
        let custom = Agent {
            id: "my-custom-agent".to_string(),
            display_name: "My Custom Agent".to_string(),
            category: "platform".to_string(),
            global_skills_dir: "/tmp/custom/skills".to_string(),
            project_skills_dir: None,
            is_detected: false,
            is_builtin: false,
            is_enabled: true,
        };
        insert_custom_agent(&pool, &custom).await.unwrap();

        let all = get_all_agents(&pool).await.unwrap();
        assert_eq!(
            all.len(),
            builtin_agents().len() + 1,
            "Should have builtins + 1 custom"
        );

        let retrieved = get_agent_by_id(&pool, "my-custom-agent")
            .await
            .unwrap()
            .unwrap();
        assert!(!retrieved.is_builtin);
        assert_eq!(retrieved.display_name, "My Custom Agent");
    }

    #[tokio::test]
    async fn test_delete_custom_agent() {
        let pool = setup_test_db().await;
        let custom = Agent {
            id: "deletable-agent".to_string(),
            display_name: "Deletable".to_string(),
            category: "platform".to_string(),
            global_skills_dir: "/tmp/deletable/skills".to_string(),
            project_skills_dir: None,
            is_detected: false,
            is_builtin: false,
            is_enabled: true,
        };
        insert_custom_agent(&pool, &custom).await.unwrap();
        delete_custom_agent(&pool, "deletable-agent").await.unwrap();

        let retrieved = get_agent_by_id(&pool, "deletable-agent").await.unwrap();
        assert!(retrieved.is_none());
    }

    #[tokio::test]
    async fn test_can_delete_builtin_agent() {
        let pool = setup_test_db().await;
        let result = delete_custom_agent(&pool, "claude-code").await;
        assert!(result.is_ok(), "Built-in agents can be removed by the user");

        let retrieved = get_agent_by_id(&pool, "claude-code").await.unwrap();
        assert!(retrieved.is_none());
    }

    #[tokio::test]
    async fn test_builtin_agent_edit_is_applied_during_seed() {
        let pool = setup_test_db().await;
        let mut agents = builtin_agents();
        let claude = agents
            .iter_mut()
            .find(|agent| agent.id == "claude-code")
            .expect("claude-code");
        claude.display_name = "Edited Claude".to_string();
        claude.global_skills_dir = "/tmp/edited-claude/skills".to_string();

        seed_builtin_agents_from(&pool, &agents, true)
            .await
            .unwrap();

        let updated = get_agent_by_id(&pool, "claude-code")
            .await
            .unwrap()
            .expect("claude-code should still exist");
        assert_eq!(updated.display_name, "Edited Claude");
        assert_eq!(updated.global_skills_dir, "/tmp/edited-claude/skills");
        assert!(updated.is_builtin);
    }

    #[tokio::test]
    async fn test_removed_catalog_platform_is_dropped_during_seed() {
        let pool = setup_test_db().await;
        let agents: Vec<Agent> = builtin_agents()
            .into_iter()
            .filter(|agent| agent.id != "cursor")
            .collect();

        seed_builtin_agents_from(&pool, &agents, true)
            .await
            .unwrap();

        let deleted = get_agent_by_id(&pool, "cursor").await.unwrap();
        assert!(
            deleted.is_none(),
            "platforms missing from the catalog should be removed during seed"
        );
    }

    #[tokio::test]
    async fn test_workbuddy_scans_correct_directory() {
        let pool = setup_test_db().await;
        let wb = get_agent_by_id(&pool, "workbuddy")
            .await
            .unwrap()
            .expect("WorkBuddy agent should exist");
        assert_eq!(wb.display_name, "WorkBuddy");
        let workbuddy_path = wb.global_skills_dir.replace('\\', "/");
        assert!(
            workbuddy_path.contains(".workbuddy/skills"),
            "WorkBuddy should scan ~/.workbuddy/skills, got: {}",
            wb.global_skills_dir
        );
    }

    #[tokio::test]
    async fn test_trimmed_autoclaw_is_not_seeded_but_workbuddy_remains() {
        let pool = setup_test_db().await;
        assert!(get_agent_by_id(&pool, "autoclaw").await.unwrap().is_none());
        assert!(get_agent_by_id(&pool, "workbuddy").await.unwrap().is_some());
    }

    // ── Collections ───────────────────────────────────────────────────────────

    #[tokio::test]
    async fn test_create_collection() {
        let pool = setup_test_db().await;
        let col = create_collection(&pool, "My Collection", Some("A test collection"))
            .await
            .unwrap();
        assert!(!col.id.is_empty());
        assert_eq!(col.name, "My Collection");
        assert_eq!(col.description.as_deref(), Some("A test collection"));
    }

    #[tokio::test]
    async fn test_get_all_collections() {
        let pool = setup_test_db().await;
        create_collection(&pool, "Collection A", None)
            .await
            .unwrap();
        create_collection(&pool, "Collection B", Some("Desc"))
            .await
            .unwrap();

        let all = get_all_collections(&pool).await.unwrap();
        assert_eq!(all.len(), 2);
    }

    #[tokio::test]
    async fn test_update_collection() {
        let pool = setup_test_db().await;
        let col = create_collection(&pool, "Old Name", None).await.unwrap();
        update_collection(&pool, &col.id, "New Name", Some("New desc"))
            .await
            .unwrap();

        let updated = get_collection_by_id(&pool, &col.id).await.unwrap().unwrap();
        assert_eq!(updated.name, "New Name");
        assert_eq!(updated.description.as_deref(), Some("New desc"));
    }

    #[tokio::test]
    async fn test_delete_collection() {
        let pool = setup_test_db().await;
        let col = create_collection(&pool, "To Delete", None).await.unwrap();
        delete_collection(&pool, &col.id).await.unwrap();

        let retrieved = get_collection_by_id(&pool, &col.id).await.unwrap();
        assert!(retrieved.is_none());
    }

    #[tokio::test]
    async fn test_add_and_remove_skill_from_collection() {
        let pool = setup_test_db().await;
        let skill = make_skill("collection-skill", "Collection Skill", false);
        upsert_skill(&pool, &skill).await.unwrap();
        let col = create_collection(&pool, "Test Col", None).await.unwrap();

        add_skill_to_collection(&pool, &col.id, "collection-skill")
            .await
            .unwrap();

        let skills = get_collection_skills(&pool, &col.id).await.unwrap();
        assert_eq!(skills.len(), 1);
        assert_eq!(skills[0].id, "collection-skill");

        remove_skill_from_collection(&pool, &col.id, "collection-skill")
            .await
            .unwrap();

        let skills_after = get_collection_skills(&pool, &col.id).await.unwrap();
        assert!(skills_after.is_empty());
    }

    #[tokio::test]
    async fn test_add_skill_to_collection_is_idempotent() {
        let pool = setup_test_db().await;
        let skill = make_skill("idem-skill", "Idem Skill", false);
        upsert_skill(&pool, &skill).await.unwrap();
        let col = create_collection(&pool, "Idem Col", None).await.unwrap();

        add_skill_to_collection(&pool, &col.id, "idem-skill")
            .await
            .unwrap();
        add_skill_to_collection(&pool, &col.id, "idem-skill")
            .await
            .unwrap();

        let skills = get_collection_skills(&pool, &col.id).await.unwrap();
        assert_eq!(skills.len(), 1, "Duplicate add should be a no-op");
    }

    #[tokio::test]
    async fn test_delete_collection_also_removes_skill_memberships() {
        let pool = setup_test_db().await;
        let skill = make_skill("cascade-skill", "Cascade Skill", false);
        upsert_skill(&pool, &skill).await.unwrap();
        let col = create_collection(&pool, "Cascade Col", None).await.unwrap();
        add_skill_to_collection(&pool, &col.id, "cascade-skill")
            .await
            .unwrap();

        delete_collection(&pool, &col.id).await.unwrap();

        // The collection_skills row should also be gone
        let rows: Vec<_> = sqlx::query("SELECT * FROM collection_skills WHERE collection_id = ?")
            .bind(&col.id)
            .fetch_all(&pool)
            .await
            .unwrap();
        assert!(rows.is_empty(), "Memberships should be cascade-deleted");
    }

    // ── Scan Directories ──────────────────────────────────────────────────────

    /// Returns the number of *unique* global_skills_dir paths across all
    /// built-in agents.  This is the number of rows that seed_builtin_scan_directories
    /// inserts.
    fn expected_builtin_scan_dir_count() -> usize {
        let mut paths = std::collections::HashSet::new();
        for agent in builtin_agents() {
            paths.insert(agent.global_skills_dir);
        }
        paths.len()
    }

    #[tokio::test]
    async fn test_builtin_scan_dirs_seeded() {
        let pool = setup_test_db().await;
        let dirs = get_scan_directories(&pool).await.unwrap();
        let builtin_count = expected_builtin_scan_dir_count();

        // Expect exactly one row per unique global_skills_dir across built-in agents.
        assert_eq!(
            dirs.len(),
            builtin_count,
            "Should have {} built-in scan directories after init (got {})",
            builtin_count,
            dirs.len()
        );

        // Every seeded row must be marked as built-in and active.
        for dir in &dirs {
            assert!(
                dir.is_builtin,
                "Seeded scan directory '{}' must have is_builtin=true",
                dir.path
            );
            assert!(
                dir.is_active,
                "Seeded scan directory '{}' must be active by default",
                dir.path
            );
        }

        // The paths must match the unique global_skills_dir values.
        let seeded_paths: std::collections::HashSet<&str> =
            dirs.iter().map(|d| d.path.as_str()).collect();
        for agent in builtin_agents() {
            assert!(
                seeded_paths.contains(agent.global_skills_dir.as_str()),
                "Built-in agent '{}' global_skills_dir '{}' must be in scan_directories",
                agent.id,
                agent.global_skills_dir
            );
        }
    }

    #[tokio::test]
    async fn test_builtin_scan_dirs_seeded_is_idempotent() {
        let pool = setup_test_db().await;
        // Second call to init_database must not create duplicate rows.
        init_database(&pool).await.unwrap();
        let dirs = get_scan_directories(&pool).await.unwrap();
        let builtin_count = expected_builtin_scan_dir_count();
        assert_eq!(
            dirs.len(),
            builtin_count,
            "Repeated init must not create duplicate scan directory rows"
        );
    }

    #[tokio::test]
    async fn test_add_scan_directory() {
        let pool = setup_test_db().await;
        let dir = add_scan_directory(&pool, "/tmp/my-project", Some("My Project"))
            .await
            .unwrap();
        assert_eq!(dir.path, "/tmp/my-project");
        assert_eq!(dir.label.as_deref(), Some("My Project"));
        assert!(dir.is_active);
        assert!(!dir.is_builtin);
    }

    #[tokio::test]
    async fn test_update_scan_directory_label_and_path() {
        let pool = setup_test_db().await;
        add_scan_directory(&pool, "/tmp/old-project", Some("Old"))
            .await
            .unwrap();
        let updated = update_scan_directory(
            &pool,
            "/tmp/old-project",
            "/tmp/new-project",
            Some("New Name"),
        )
        .await
        .unwrap();
        assert_eq!(updated.path, "/tmp/new-project");
        assert_eq!(updated.label.as_deref(), Some("New Name"));
    }

    #[tokio::test]
    async fn test_get_scan_directories() {
        let pool = setup_test_db().await;
        add_scan_directory(&pool, "/tmp/dir-a", None).await.unwrap();
        add_scan_directory(&pool, "/tmp/dir-b", Some("Dir B"))
            .await
            .unwrap();

        let dirs = get_scan_directories(&pool).await.unwrap();
        // There are N built-in dirs (seeded on init) plus the 2 we just added.
        let builtin_count = expected_builtin_scan_dir_count();
        assert_eq!(dirs.len(), builtin_count + 2);

        // Verify the custom ones are present.
        let paths: Vec<&str> = dirs.iter().map(|d| d.path.as_str()).collect();
        assert!(paths.contains(&"/tmp/dir-a"));
        assert!(paths.contains(&"/tmp/dir-b"));
    }

    #[tokio::test]
    async fn test_remove_scan_directory() {
        let pool = setup_test_db().await;
        add_scan_directory(&pool, "/tmp/removable", None)
            .await
            .unwrap();
        remove_scan_directory(&pool, "/tmp/removable")
            .await
            .unwrap();

        let dirs = get_scan_directories(&pool).await.unwrap();
        // Built-in dirs remain; only the custom one is removed.
        let builtin_count = expected_builtin_scan_dir_count();
        assert_eq!(dirs.len(), builtin_count);
        assert!(!dirs.iter().any(|d| d.path == "/tmp/removable"));
    }

    #[tokio::test]
    async fn test_cannot_remove_builtin_scan_directory() {
        let pool = setup_test_db().await;
        crate::db::upsert_builtin_scan_directory(&pool, "/builtin/path", "Builtin").await.unwrap();

        let result = remove_scan_directory(&pool, "/builtin/path").await;
        assert!(
            result.is_err(),
            "Should not remove a builtin scan directory"
        );
    }

    #[tokio::test]
    async fn test_remove_nonexistent_scan_directory_returns_error() {
        let pool = setup_test_db().await;
        let result = remove_scan_directory(&pool, "/nonexistent/path").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_toggle_scan_directory() {
        let pool = setup_test_db().await;
        add_scan_directory(&pool, "/tmp/toggle-dir", None)
            .await
            .unwrap();
        toggle_scan_directory(&pool, "/tmp/toggle-dir", false)
            .await
            .unwrap();

        let dirs = get_scan_directories(&pool).await.unwrap();
        let dir = dirs.iter().find(|d| d.path == "/tmp/toggle-dir").unwrap();
        assert!(!dir.is_active);
    }

    // ── Settings ──────────────────────────────────────────────────────────────

    #[tokio::test]
    async fn test_set_and_get_setting() {
        let pool = setup_test_db().await;
        set_setting(&pool, "theme", "dark").await.unwrap();
        let value = get_setting(&pool, "theme").await.unwrap();
        assert_eq!(value.as_deref(), Some("dark"));
    }

    #[tokio::test]
    async fn test_get_missing_setting_returns_none() {
        let pool = setup_test_db().await;
        let value = get_setting(&pool, "nonexistent_key").await.unwrap();
        assert!(value.is_none());
    }

    #[tokio::test]
    async fn test_set_setting_upserts() {
        let pool = setup_test_db().await;
        set_setting(&pool, "lang", "en").await.unwrap();
        set_setting(&pool, "lang", "zh").await.unwrap();
        let value = get_setting(&pool, "lang").await.unwrap();
        assert_eq!(value.as_deref(), Some("zh"));
    }

    #[tokio::test]
    async fn test_multiple_settings() {
        let pool = setup_test_db().await;
        set_setting(&pool, "key1", "val1").await.unwrap();
        set_setting(&pool, "key2", "val2").await.unwrap();
        set_setting(&pool, "key3", "val3").await.unwrap();

        assert_eq!(
            get_setting(&pool, "key1").await.unwrap().as_deref(),
            Some("val1")
        );
        assert_eq!(
            get_setting(&pool, "key2").await.unwrap().as_deref(),
            Some("val2")
        );
        assert_eq!(
            get_setting(&pool, "key3").await.unwrap().as_deref(),
            Some("val3")
        );
    }

    // ── Migration: created_at ─────────────────────────────────────────────────

    /// Verifies that `init_database` adds the `created_at` column to an existing
    /// `skill_installations` table that was created with the old schema (before
    /// the column was introduced), and that existing rows are backfilled.
    #[tokio::test]
    async fn test_migration_adds_created_at_to_skill_installations() {
        // Create a fresh in-memory pool WITHOUT calling init_database first.
        let pool = SqlitePool::connect(":memory:")
            .await
            .expect("Failed to create in-memory SQLite pool");

        // Build the OLD skill_installations schema — no created_at column.
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS skill_installations (
                skill_id       TEXT NOT NULL,
                agent_id       TEXT NOT NULL,
                installed_path TEXT NOT NULL,
                link_type      TEXT NOT NULL,
                symlink_target TEXT,
                PRIMARY KEY (skill_id, agent_id)
            )",
        )
        .execute(&pool)
        .await
        .expect("Failed to create old skill_installations table");

        // Create the skills table so the FK-style relationship is consistent.
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS skills (
                id             TEXT PRIMARY KEY,
                name           TEXT NOT NULL,
                description    TEXT,
                file_path      TEXT NOT NULL,
                canonical_path TEXT,
                is_central     BOOLEAN NOT NULL DEFAULT 0,
                source         TEXT,
                content        TEXT,
                scanned_at     TEXT NOT NULL
            )",
        )
        .execute(&pool)
        .await
        .expect("Failed to create skills table");

        // Insert a skill row (needed before the installation row references it).
        sqlx::query(
            "INSERT INTO skills (id, name, file_path, is_central, scanned_at)
             VALUES ('legacy-skill', 'Legacy Skill', '/tmp/legacy-skill/SKILL.md', 0, '2024-01-01T00:00:00Z')",
        )
        .execute(&pool)
        .await
        .expect("Failed to insert legacy skill");

        // Insert an installation row using the OLD schema (no created_at column).
        sqlx::query(
            "INSERT INTO skill_installations (skill_id, agent_id, installed_path, link_type)
             VALUES ('legacy-skill', 'claude-code', '/tmp/claude/legacy-skill', 'symlink')",
        )
        .execute(&pool)
        .await
        .expect("Failed to insert legacy skill_installations row");

        // Run init_database — should detect the missing created_at column and add it.
        init_database(&pool)
            .await
            .expect("init_database should succeed and apply the created_at migration");

        // Confirm the column now exists in PRAGMA table_info.
        let columns = sqlx::query("PRAGMA table_info(skill_installations)")
            .fetch_all(&pool)
            .await
            .expect("PRAGMA table_info should succeed");

        let has_created_at = columns.iter().any(|row| {
            row.try_get::<String, _>("name")
                .map(|name| name == "created_at")
                .unwrap_or(false)
        });
        assert!(
            has_created_at,
            "created_at column must exist in skill_installations after migration"
        );

        // Confirm that the pre-existing row has a non-empty created_at value
        // (backfilled by the DEFAULT (datetime('now')) expression).
        let row = sqlx::query(
            "SELECT created_at FROM skill_installations \
             WHERE skill_id = 'legacy-skill' AND agent_id = 'claude-code'",
        )
        .fetch_one(&pool)
        .await
        .expect("Pre-existing installation row should still be queryable after migration");

        let created_at: String = row
            .try_get("created_at")
            .expect("created_at should be readable from the pre-existing row");
        assert!(
            !created_at.is_empty(),
            "Pre-existing rows must have a non-empty created_at value after migration (got: '{}')",
            created_at
        );
    }

    #[tokio::test]
    async fn test_managed_installation_migration_requires_matching_symlink_target() {
        let pool = SqlitePool::connect(":memory:")
            .await
            .expect("Failed to create in-memory SQLite pool");

        sqlx::query(
            "CREATE TABLE skills (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                description TEXT,
                file_path TEXT NOT NULL,
                canonical_path TEXT,
                is_central BOOLEAN NOT NULL DEFAULT 0,
                source TEXT,
                content TEXT,
                scanned_at TEXT NOT NULL
            )",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "CREATE TABLE skill_installations (
                skill_id TEXT NOT NULL,
                agent_id TEXT NOT NULL,
                installed_path TEXT NOT NULL,
                link_type TEXT NOT NULL,
                symlink_target TEXT,
                created_at TEXT,
                PRIMARY KEY (skill_id, agent_id)
            )",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO skills
             (id, name, file_path, canonical_path, is_central, scanned_at)
             VALUES
             ('proven-symlink', 'Same Name', '/tmp/canonical/proven/SKILL.md', '/tmp/canonical/proven', 0, '2024-01-01'),
             ('external-symlink', 'Same Name', '/tmp/canonical/external/SKILL.md', '/tmp/canonical/external', 0, '2024-01-01'),
             ('legacy-copy', 'Legacy Copy', '/tmp/canonical/copy/SKILL.md', '/tmp/canonical/copy', 0, '2024-01-01'),
             ('legacy-native', 'Legacy Native', '/tmp/canonical/native/SKILL.md', '/tmp/canonical/native', 0, '2024-01-01'),
             ('unproven-symlink', 'Unproven Symlink', '/tmp/unproven/SKILL.md', NULL, 1, '2024-01-01')",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO skill_installations
             (skill_id, agent_id, installed_path, link_type, symlink_target, created_at)
             VALUES
             ('proven-symlink', 'cursor', '/tmp/cursor/proven', 'symlink', '/tmp/canonical/proven', '2024-01-01'),
             ('external-symlink', 'cursor', '/tmp/cursor/external', 'symlink', '/tmp/outside/external', '2024-01-01'),
             ('legacy-copy', 'cursor', '/tmp/cursor/copy', 'copy', NULL, '2024-01-01'),
             ('legacy-native', 'cursor', '/tmp/cursor/native', 'native', NULL, '2024-01-01'),
             ('unproven-symlink', 'cursor', '/tmp/cursor/unproven', 'symlink', '/tmp/unproven', '2024-01-01')",
        )
        .execute(&pool)
        .await
        .unwrap();

        init_database(&pool).await.unwrap();
        init_database(&pool)
            .await
            .expect("managed ownership migration must be retryable");

        let rows =
            sqlx::query("SELECT skill_id, is_managed FROM skill_installations ORDER BY skill_id")
                .fetch_all(&pool)
                .await
                .unwrap();
        assert_eq!(rows.len(), 5);
        let ownership: std::collections::HashMap<String, i64> = rows
            .into_iter()
            .map(|row| {
                (
                    row.get::<String, _>("skill_id"),
                    row.get::<i64, _>("is_managed"),
                )
            })
            .collect();
        assert_eq!(ownership.get("proven-symlink"), Some(&1));
        assert_eq!(ownership.get("external-symlink"), Some(&0));
        assert_eq!(ownership.get("legacy-copy"), Some(&0));
        assert_eq!(ownership.get("legacy-native"), Some(&0));
        assert_eq!(ownership.get("unproven-symlink"), Some(&0));
    }

    /// Verifies that calling `init_database` on a fresh database (one that already
    /// includes created_at in the CREATE TABLE) does NOT trigger the ALTER TABLE
    /// migration path — i.e., the second `init_database` call is fully idempotent
    /// and does not fail.
    #[tokio::test]
    async fn test_migration_skipped_when_created_at_already_exists() {
        // setup_test_db calls init_database, which creates the table WITH created_at.
        let pool = setup_test_db().await;

        // A second call to init_database must succeed without error (idempotent).
        let result = init_database(&pool).await;
        assert!(
            result.is_ok(),
            "Second init_database should be idempotent when created_at already exists"
        );

        // Confirm created_at is still present and there's exactly one occurrence.
        let columns = sqlx::query("PRAGMA table_info(skill_installations)")
            .fetch_all(&pool)
            .await
            .unwrap();
        let created_at_count = columns
            .iter()
            .filter(|row| {
                row.try_get::<String, _>("name")
                    .map(|name| name == "created_at")
                    .unwrap_or(false)
            })
            .count();
        assert_eq!(
            created_at_count, 1,
            "created_at column should appear exactly once after repeated init"
        );
    }
}
