//! File-backed application preferences with explicit paths and no database dependency.
use std::{
    collections::BTreeMap,
    fs,
    io::Write,
    path::{Path, PathBuf},
    sync::Mutex,
};

use serde::{Deserialize, Serialize};

use crate::storage::models::ScanDirectory;

static CONFIG_LOCK: Mutex<()> = Mutex::new(());

/// An explicit configuration location, usable before opening any storage engine.
#[derive(Clone, Debug)]
pub struct ConfigStore {
    path: PathBuf,
}

impl ConfigStore {
    pub fn new(directory: &Path) -> Self {
        Self { path: directory.join("config.json") }
    }

    pub fn path(&self) -> &Path { &self.path }

    pub fn load(&self) -> Result<AppConfig, String> { load_path(&self.path) }

    pub fn update<T>(&self, change: impl FnOnce(&mut AppConfig) -> Result<T, String>) -> Result<T, String> {
        update_path(&self.path, change)
    }
}

#[derive(Serialize, Deserialize)]
pub struct AppConfig {
    #[serde(default = "default_platforms")]
    pub platforms: Vec<crate::platforms::PlatformDefinition>,
    #[serde(default)]
    pub settings: BTreeMap<String, String>,
    #[serde(default)]
    pub scan_directories: Vec<ScanDirectory>,
    #[serde(default)]
    pub next_scan_directory_id: i64,
    #[serde(flatten)]
    extra: BTreeMap<String, serde_json::Value>,
}

fn default_platforms() -> Vec<crate::platforms::PlatformDefinition> {
    crate::platforms::default_platform_definitions()
        .expect("Bundled platform defaults must be valid")
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            platforms: default_platforms(),
            settings: BTreeMap::new(),
            scan_directories: Vec::new(),
            next_scan_directory_id: 1,
            extra: BTreeMap::new(),
        }
    }
}

fn read(path: &Path) -> Result<AppConfig, String> {
    match fs::read(path) {
        Ok(bytes) => serde_json::from_slice(&bytes)
            .map_err(|_| "Invalid config.json; existing configuration was left unchanged".into()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(AppConfig::default()),
        Err(_) => Err("Cannot read config.json".into()),
    }
}

fn write(path: &Path, config: &AppConfig) -> Result<(), String> {
    let bytes = serde_json::to_vec_pretty(config).map_err(|_| "Cannot serialize config.json")?;
    let temp = path.with_file_name(format!(".config-{}.tmp", uuid::Uuid::new_v4()));
    let result = (|| {
        let mut options = fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(&temp)?;
        file.write_all(&bytes)?;
        file.sync_all()?;
        drop(file);
        fs::rename(&temp, path)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temp);
        return Err("Cannot save config.json; previous configuration was left unchanged".into());
    }
    Ok(())
}

pub fn load_path(path: &Path) -> Result<AppConfig, String> {
    let _guard = CONFIG_LOCK
        .lock()
        .map_err(|_| "Configuration lock failed")?;
    read(path)
}

pub fn update_path<T>(
    path: &Path,
    change: impl FnOnce(&mut AppConfig) -> Result<T, String>,
) -> Result<T, String> {
    let _guard = CONFIG_LOCK
        .lock()
        .map_err(|_| "Configuration lock failed")?;
    let mut config = read(path)?;
    let result = change(&mut config)?;
    write(path, &config)?;
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;
    use crate::db_config::{config_path, load, update};

    #[test]
    fn explicit_location_round_trips_without_a_database() {
        let first = tempfile::tempdir().unwrap();
        let second = tempfile::tempdir().unwrap();
        let config = ConfigStore::new(first.path());
        config.update(|data| {
            data.settings.insert("language".into(), "en".into());
            Ok(())
        }).unwrap();
        assert_eq!(ConfigStore::new(first.path()).load().unwrap().settings["language"], "en");
        assert!(ConfigStore::new(second.path()).load().unwrap().settings.is_empty());
        assert!(!first.path().join("db.sqlite").exists());
    }

    #[tokio::test]
    async fn configuration_survives_reopen_and_is_absent_from_database() {
        let dir = tempfile::tempdir().unwrap();
        let database = dir.path().join("db.sqlite");
        let pool = db::create_pool(database.to_str().unwrap()).await.unwrap();
        db::init_database(&pool).await.unwrap();
        db::set_setting(&pool, "language", "en").await.unwrap();
        let project = db::add_scan_directory(&pool, "/example/project", Some("Example"))
            .await
            .unwrap();
        db::toggle_scan_directory(&pool, &project.path, false)
            .await
            .unwrap();
        pool.close().await;
        let pool = db::create_pool(database.to_str().unwrap()).await.unwrap();
        db::init_database(&pool).await.unwrap();
        assert_eq!(
            db::get_setting(&pool, "language").await.unwrap().as_deref(),
            Some("en")
        );
        let restored = db::get_scan_directory_by_id(&pool, project.id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(restored.path, project.path);
        assert!(!restored.is_active);
        let tables: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM sqlite_master WHERE name IN ('settings', 'scan_directories')",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(tables, 0);
        assert!(dir.path().join("config.json").is_file());
    }

    #[tokio::test]
    async fn legacy_database_configuration_is_not_imported() {
        let dir = tempfile::tempdir().unwrap();
        let pool = db::create_pool(dir.path().join("db.sqlite").to_str().unwrap())
            .await
            .unwrap();
        sqlx::query("CREATE TABLE settings (key TEXT, value TEXT)")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO settings VALUES ('language', 'legacy')")
            .execute(&pool)
            .await
            .unwrap();
        db::init_database(&pool).await.unwrap();
        assert_eq!(db::get_setting(&pool, "language").await.unwrap(), None);
    }

    #[tokio::test]
    async fn malformed_file_is_not_overwritten() {
        let dir = tempfile::tempdir().unwrap();
        let pool = db::create_pool(dir.path().join("db.sqlite").to_str().unwrap())
            .await
            .unwrap();
        let path = config_path(&pool).unwrap();
        fs::write(&path, "{invalid-json").unwrap();
        assert!(db::init_database(&pool).await.is_err());
        assert!(db::set_setting(&pool, "language", "en").await.is_err());
        assert_eq!(fs::read_to_string(path).unwrap(), "{invalid-json");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn parallel_writes_preserve_all_settings_and_unknown_fields() {
        let dir = tempfile::tempdir().unwrap();
        let pool = db::create_pool(dir.path().join("db.sqlite").to_str().unwrap())
            .await
            .unwrap();
        fs::write(config_path(&pool).unwrap(), r#"{"future_option":true}"#).unwrap();
        let mut tasks = Vec::new();
        for index in 0..20 {
            let pool = pool.clone();
            tasks.push(tokio::spawn(async move {
                db::set_setting(&pool, &format!("option-{index}"), "example")
                    .await
                    .unwrap();
            }));
        }
        for task in tasks {
            task.await.unwrap();
        }
        let config = load(&pool).unwrap();
        assert_eq!(config.settings.len(), 20);
        assert_eq!(config.extra["future_option"], true);
    }

    #[tokio::test]
    async fn rejected_change_preserves_file_and_removed_project_ids_are_not_reused() {
        let dir = tempfile::tempdir().unwrap();
        let pool = db::create_pool(dir.path().join("db.sqlite").to_str().unwrap())
            .await
            .unwrap();
        db::init_database(&pool).await.unwrap();
        let first = db::add_scan_directory(&pool, "/example/one", None)
            .await
            .unwrap();
        db::remove_scan_directory(&pool, &first.path).await.unwrap();
        let second = db::add_scan_directory(&pool, "/example/two", None)
            .await
            .unwrap();
        assert!(second.id > first.id);
        let before = fs::read(config_path(&pool).unwrap()).unwrap();
        assert!(update::<()>(&pool, |config| {
            config.settings.clear();
            Err("rejected".into())
        })
        .is_err());
        assert_eq!(fs::read(config_path(&pool).unwrap()).unwrap(), before);
    }
}
