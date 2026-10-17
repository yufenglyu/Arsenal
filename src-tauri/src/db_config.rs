//! Configuration location follows the opened JSON store.
use crate::config_store::{load_path, update_path, AppConfig};
use crate::db::DbPool;
use std::path::PathBuf;
pub fn config_path(pool: &DbPool) -> Result<PathBuf, String> {
    Ok(pool.directory().join("config.json"))
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
pub fn initialize(pool: &DbPool) -> Result<(), String> {
    update(pool, |_| Ok(()))
}
