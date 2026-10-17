//! Transitional SQLite-to-config path adapter; removed with the SQLite backend.
use std::path::{Path, PathBuf};
use crate::db::DbPool;
use crate::config_store::{AppConfig, load_path, update_path};

pub fn config_path(pool: &DbPool) -> Result<PathBuf, String> {
    let options = pool.connect_options();
    let filename = options.get_filename();
    if filename == Path::new(":memory:")
        || filename
            .to_string_lossy()
            .starts_with("file:sqlx-in-memory")
    {
        #[cfg(test)]
        {
            use std::sync::{Arc, Mutex, OnceLock, Weak};
            type TestEntry = (Weak<sqlx::sqlite::SqliteConnectOptions>, tempfile::TempDir);
            static DIRS: OnceLock<Mutex<Vec<TestEntry>>> = OnceLock::new();
            let mut dirs = DIRS.get_or_init(|| Mutex::new(Vec::new())).lock().unwrap();
            dirs.retain(|(options, _)| options.strong_count() > 0);
            if let Some((_, dir)) = dirs
                .iter()
                .find(|(stored, _)| stored.ptr_eq(&Arc::downgrade(&options)))
            {
                return Ok(dir.path().join("config.json"));
            }
            let dir = tempfile::tempdir().map_err(|_| "Cannot create test config directory")?;
            let path = dir.path().join("config.json");
            dirs.push((Arc::downgrade(&options), dir));
            return Ok(path);
        }
        #[cfg(not(test))]
        return Err("Application config requires a file-backed database".into());
    }
    Ok(filename
        .parent()
        .unwrap_or(Path::new("."))
        .join("config.json"))
}

pub fn load(pool: &DbPool) -> Result<AppConfig, String> {
    load_path(&config_path(pool)?)
}

pub fn update<T>(
    pool: &DbPool,
    change: impl FnOnce(&mut AppConfig) -> Result<T, String>,
) -> Result<T, String> {
    update_path(&config_path(pool)?, change)
}

/// Start with the clean file format; legacy database configuration is not imported.
pub fn initialize(pool: &DbPool) -> Result<(), String> {
    update(pool, |_| Ok(()))
}

