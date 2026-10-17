use tauri::State;

use crate::db::{self, Agent, AppStore, ScanDirectory};
use crate::path_utils::{
    app_data_dir, copy_app_data_dir, expand_home_path, normalize_app_data_dir, path_to_string,
    write_app_data_dir_override,
};
use crate::AppState;

const GITHUB_LATEST_RELEASE_API: &str =
    "https://api.github.com/repos/yufenglyu/skillshub/releases/latest";
const GITHUB_LATEST_RELEASE_URL: &str = "https://github.com/yufenglyu/skillshub/releases/latest";

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppUpdateInfo {
    pub current_version: String,
    pub latest_version: String,
    pub latest_url: String,
    pub is_update_available: bool,
    pub release_name: Option<String>,
    pub published_at: Option<String>,
}

#[derive(Debug, serde::Deserialize)]
struct GitHubReleaseResponse {
    tag_name: String,
    html_url: Option<String>,
    name: Option<String>,
    published_at: Option<String>,
}

// ─── Core Implementations (testable without Tauri State) ──────────────────────

/// Return all scan directories, built-in first then custom ordered by added_at.
pub async fn get_scan_directories_impl(pool: &AppStore) -> Result<Vec<ScanDirectory>, String> {
    db::get_scan_directories(pool).await
}

/// Add a new custom (non-builtin) scan directory.
/// Returns the newly created record.
pub async fn add_scan_directory_impl(
    pool: &AppStore,
    path: &str,
    label: Option<&str>,
) -> Result<ScanDirectory, String> {
    let path = path.trim();
    if path.is_empty() {
        return Err("Scan directory path cannot be empty".to_string());
    }
    let expanded_path = path_to_string(&expand_home_path(path));
    db::add_scan_directory(pool, &expanded_path, label).await
}

pub async fn update_scan_directory_impl(
    pool: &AppStore,
    path: &str,
    new_path: &str,
    label: Option<&str>,
) -> Result<ScanDirectory, String> {
    let new_path = new_path.trim();
    if new_path.is_empty() {
        return Err("Scan directory path cannot be empty".to_string());
    }
    let expanded_path = path_to_string(&expand_home_path(new_path));
    let normalized_label = label.map(str::trim).filter(|value| !value.is_empty());
    db::update_scan_directory(pool, path, &expanded_path, normalized_label).await
}

/// Remove a custom (non-builtin) scan directory by path.
/// Returns an error if the directory is built-in or not found.
pub async fn remove_scan_directory_impl(pool: &AppStore, path: &str) -> Result<(), String> {
    db::remove_scan_directory(pool, path).await
}

/// Toggle the `is_active` flag on a scan directory by path.
pub async fn set_scan_directory_active_impl(
    pool: &AppStore,
    path: &str,
    is_active: bool,
) -> Result<(), String> {
    db::toggle_scan_directory(pool, path, is_active).await
}

/// Get a settings value by key. Returns `None` if the key is not set.
pub async fn get_setting_impl(pool: &AppStore, key: &str) -> Result<Option<String>, String> {
    db::get_setting(pool, key).await
}

/// Set (upsert) a settings value.
pub async fn set_setting_impl(pool: &AppStore, key: &str, value: &str) -> Result<(), String> {
    if key.trim().is_empty() {
        return Err("Settings key cannot be empty".to_string());
    }
    if key == "ai_note_prompt" && db::get_setting(pool, key).await?.as_deref() != Some(value) {
        db::explanations::clear(pool).await?;
    }
    db::set_setting(pool, key, value).await
}

pub async fn update_central_skills_dir_impl(pool: &AppStore, path: &str) -> Result<Agent, String> {
    let path = path.trim();
    if path.is_empty() {
        return Err("Central Skills path cannot be empty".to_string());
    }

    let expanded_path = path_to_string(&expand_home_path(path));
    std::fs::create_dir_all(&expanded_path)
        .map_err(|e| format!("Failed to create Central Skills directory: {}", e))?;

    let old_central = db::get_agent_by_id(pool, "central")
        .await?
        .ok_or_else(|| "Central agent not found".to_string())?;
    let updated = db::update_central_agent_skills_dir(pool, &expanded_path).await?;
    demote_skills_outside_new_central_root(pool, &old_central.global_skills_dir, &expanded_path)
        .await?;
    db::upsert_builtin_scan_directory(pool, &expanded_path, "Central Skills").await?;

    Ok(updated)
}

pub async fn get_skill_resource_library_dir_impl(pool: &AppStore) -> Result<String, String> {
    Ok(path_to_string(
        &db::get_skill_resource_library_dir(pool).await?,
    ))
}

pub async fn update_skill_resource_library_dir_impl(
    pool: &AppStore,
    path: &str,
) -> Result<String, String> {
    let path = path.trim();
    if path.is_empty() {
        return Err("Skill Resource Library path cannot be empty".to_string());
    }
    Ok(path_to_string(
        &db::set_skill_resource_library_dir(pool, path).await?,
    ))
}

pub fn get_app_data_dir_impl() -> String {
    path_to_string(&app_data_dir())
}

pub fn get_platform_dir_impl() -> String {
    crate::platforms::platform_dir()
        .map(|dir| path_to_string(&dir))
        .unwrap_or_default()
}

pub async fn update_app_data_dir_impl(pool: &AppStore, path: &str) -> Result<String, String> {
    update_app_data_dir_with_publish(pool, path, write_app_data_dir_override).await
}

async fn update_app_data_dir_with_publish(
    pool: &AppStore,
    path: &str,
    publish: impl FnOnce(&std::path::Path) -> Result<(), String>,
) -> Result<String, String> {
    let next = normalize_app_data_dir(path)?;
    let current = pool.directory();
    if next == current {
        return Ok(path_to_string(&next));
    }
    // Hold the source writer lock while copying one committed generation.
    let _snapshot = pool.begin().await?;
    if !next.join("store/manifest.json").exists() {
        if next.join("db.sqlite").exists() {
            return Err("STORAGE_MIGRATION_REQUIRED".into());
        }
        crate::config_store::with_snapshot(|| copy_app_data_dir(current, &next))?;
    }
    // Opening validates checksums and also rejects a directory owned by another process.
    let validated = crate::storage::store::Store::open(&next)?;
    publish(&next)?;
    drop(validated);
    Ok(path_to_string(&next))
}

pub async fn check_app_update_impl(pool: &AppStore) -> Result<AppUpdateInfo, String> {
    let current_version = env!("CARGO_PKG_VERSION").to_string();
    let client = reqwest::Client::new();
    let mut request = client
        .get(GITHUB_LATEST_RELEASE_API)
        .header(
            reqwest::header::USER_AGENT,
            format!("SkillsHub/{}", current_version),
        )
        .header(reqwest::header::ACCEPT, "application/vnd.github+json");

    if let Some(token) = get_setting_impl(pool, "github_pat").await? {
        let token = token.trim();
        if !token.is_empty() {
            request = request.bearer_auth(token);
        }
    }

    let response = request
        .send()
        .await
        .map_err(|e| format!("Failed to check updates: {}", e))?;

    if !response.status().is_success() {
        return Err(format!(
            "Failed to check updates: GitHub returned {}",
            response.status()
        ));
    }

    let release = response
        .json::<GitHubReleaseResponse>()
        .await
        .map_err(|e| format!("Failed to parse update response: {}", e))?;
    let latest_version = normalize_version_tag(&release.tag_name);
    let is_update_available = is_newer_version(&current_version, &latest_version);

    Ok(AppUpdateInfo {
        current_version,
        latest_version,
        latest_url: release
            .html_url
            .unwrap_or_else(|| GITHUB_LATEST_RELEASE_URL.to_string()),
        is_update_available,
        release_name: release.name,
        published_at: release.published_at,
    })
}

fn normalize_version_tag(tag: &str) -> String {
    tag.trim()
        .trim_start_matches('v')
        .trim_start_matches('V')
        .to_string()
}

fn is_newer_version(current: &str, latest: &str) -> bool {
    compare_versions(latest, current).is_gt()
}

fn compare_versions(left: &str, right: &str) -> std::cmp::Ordering {
    let left_parts = version_parts(left);
    let right_parts = version_parts(right);
    let max_len = left_parts.len().max(right_parts.len()).max(3);

    for index in 0..max_len {
        let left_value = *left_parts.get(index).unwrap_or(&0);
        let right_value = *right_parts.get(index).unwrap_or(&0);
        match left_value.cmp(&right_value) {
            std::cmp::Ordering::Equal => continue,
            ordering => return ordering,
        }
    }

    std::cmp::Ordering::Equal
}

fn version_parts(version: &str) -> Vec<u64> {
    normalize_version_tag(version)
        .split(|ch: char| !ch.is_ascii_digit())
        .filter(|part| !part.is_empty())
        .map(|part| part.parse::<u64>().unwrap_or(0))
        .collect()
}

async fn demote_skills_outside_new_central_root(
    pool: &AppStore,
    old_root: &str,
    new_root: &str,
) -> Result<(), String> {
    let old_root = std::path::PathBuf::from(old_root);
    let new_root = std::path::PathBuf::from(new_root);
    if old_root == new_root {
        return Ok(());
    }

    for skill in db::get_central_skills(pool).await? {
        let skill_dir = skill
            .canonical_path
            .as_deref()
            .map(std::path::PathBuf::from)
            .or_else(|| {
                std::path::Path::new(&skill.file_path)
                    .parent()
                    .map(std::path::Path::to_path_buf)
            })
            .unwrap_or_else(|| std::path::PathBuf::from(&skill.file_path));

        if skill_dir.starts_with(&old_root) && !skill_dir.starts_with(&new_root) {
            pool.write(|data| {
                if let Some(row) = data.table_mut("skills").get_mut(&skill.id) {
                    row["is_central"] = serde_json::json!(false);
                }
                Ok(())
            })
            .await?;
        }
    }

    Ok(())
}

// ─── Tauri Commands ───────────────────────────────────────────────────────────

/// Tauri command: return all scan directories.
#[tauri::command]
pub async fn get_scan_directories(
    state: State<'_, AppState>,
) -> Result<Vec<ScanDirectory>, String> {
    get_scan_directories_impl(&state.db).await
}

/// Tauri command: add a new custom scan directory.
#[tauri::command]
pub async fn add_scan_directory(
    state: State<'_, AppState>,
    path: String,
    label: Option<String>,
) -> Result<ScanDirectory, String> {
    add_scan_directory_impl(&state.db, &path, label.as_deref()).await
}

#[tauri::command]
pub async fn update_scan_directory(
    state: State<'_, AppState>,
    path: String,
    new_path: String,
    label: Option<String>,
) -> Result<ScanDirectory, String> {
    update_scan_directory_impl(&state.db, &path, &new_path, label.as_deref()).await
}

/// Tauri command: remove a custom scan directory by path.
#[tauri::command]
pub async fn remove_scan_directory(state: State<'_, AppState>, path: String) -> Result<(), String> {
    remove_scan_directory_impl(&state.db, &path).await
}

/// Tauri command: set the is_active flag on a scan directory.
#[tauri::command]
pub async fn set_scan_directory_active(
    state: State<'_, AppState>,
    path: String,
    is_active: bool,
) -> Result<(), String> {
    set_scan_directory_active_impl(&state.db, &path, is_active).await
}

/// Tauri command: get a settings value by key.
#[tauri::command]
pub async fn get_setting(
    state: State<'_, AppState>,
    key: String,
) -> Result<Option<String>, String> {
    get_setting_impl(&state.db, &key).await
}

/// Tauri command: set (upsert) a settings value.
#[tauri::command]
pub async fn set_setting(
    state: State<'_, AppState>,
    key: String,
    value: String,
) -> Result<(), String> {
    set_setting_impl(&state.db, &key, &value).await
}

#[tauri::command]
pub async fn update_central_skills_dir(
    state: State<'_, AppState>,
    path: String,
) -> Result<Agent, String> {
    update_central_skills_dir_impl(&state.db, &path).await
}

#[tauri::command]
pub async fn get_skill_resource_library_dir(state: State<'_, AppState>) -> Result<String, String> {
    get_skill_resource_library_dir_impl(&state.db).await
}

#[tauri::command]
pub async fn update_skill_resource_library_dir(
    state: State<'_, AppState>,
    path: String,
) -> Result<String, String> {
    update_skill_resource_library_dir_impl(&state.db, &path).await
}

#[tauri::command]
pub fn get_app_data_dir() -> String {
    get_app_data_dir_impl()
}

#[tauri::command]
pub fn get_platform_dir() -> String {
    get_platform_dir_impl()
}

#[tauri::command]
pub async fn update_app_data_dir(
    state: State<'_, AppState>,
    path: String,
) -> Result<String, String> {
    update_app_data_dir_impl(&state.db, &path).await
}

#[tauri::command]
pub async fn check_app_update(state: State<'_, AppState>) -> Result<AppUpdateInfo, String> {
    check_app_update_impl(&state.db).await
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;
    use crate::storage::store::Store as AppStore;
    use std::path::Path;

    async fn setup_test_db() -> AppStore {
        let pool = AppStore::connect(":memory:").await.unwrap();
        db::init_database(&pool).await.unwrap();
        pool
    }

    // ── get_scan_directories_impl ─────────────────────────────────────────────

    /// Counts unique global_skills_dir paths across all built-in agents — the
    /// same number that seed_builtin_scan_directories inserts.
    fn expected_builtin_count() -> usize {
        let mut paths = std::collections::HashSet::new();
        for agent in db::builtin_agents() {
            paths.insert(agent.global_skills_dir);
        }
        paths.len()
    }

    #[tokio::test]
    async fn test_get_scan_directories_has_builtin_dirs_initially() {
        let pool = setup_test_db().await;
        let dirs = get_scan_directories_impl(&pool).await.unwrap();
        let builtin_count = expected_builtin_count();
        // After init, built-in scan directories are seeded automatically.
        assert_eq!(
            dirs.len(),
            builtin_count,
            "Fresh database should have {} built-in scan directories, got {}",
            builtin_count,
            dirs.len()
        );
        // All seeded rows must be marked built-in.
        for dir in &dirs {
            assert!(
                dir.is_builtin,
                "Scan directory '{}' seeded during init must have is_builtin=true",
                dir.path
            );
        }
    }

    #[tokio::test]
    async fn test_get_scan_directories_returns_added() {
        let pool = setup_test_db().await;
        add_scan_directory_impl(&pool, "/tmp/proj-a", Some("Project A"))
            .await
            .unwrap();
        add_scan_directory_impl(&pool, "/tmp/proj-b", None)
            .await
            .unwrap();

        let dirs = get_scan_directories_impl(&pool).await.unwrap();
        // N built-in dirs are already there; we added 2 custom ones.
        let builtin_count = expected_builtin_count();
        assert_eq!(dirs.len(), builtin_count + 2);
        let paths: Vec<&str> = dirs.iter().map(|d| d.path.as_str()).collect();
        let proj_a = path_to_string(Path::new("/tmp/proj-a"));
        let proj_b = path_to_string(Path::new("/tmp/proj-b"));
        assert!(paths.contains(&proj_a.as_str()));
        assert!(paths.contains(&proj_b.as_str()));
    }

    // ── add_scan_directory_impl ───────────────────────────────────────────────

    #[tokio::test]
    async fn test_add_scan_directory_creates_non_builtin() {
        let pool = setup_test_db().await;
        let dir = add_scan_directory_impl(&pool, "/tmp/my-project", Some("My Project"))
            .await
            .unwrap();

        assert_eq!(dir.path, path_to_string(Path::new("/tmp/my-project")));
        assert_eq!(dir.label.as_deref(), Some("My Project"));
        assert!(dir.is_active);
        assert!(
            !dir.is_builtin,
            "Newly added directory should not be built-in"
        );
    }

    #[tokio::test]
    async fn test_add_scan_directory_without_label() {
        let pool = setup_test_db().await;
        let dir = add_scan_directory_impl(&pool, "/tmp/no-label", None)
            .await
            .unwrap();
        assert!(dir.label.is_none());
    }

    #[tokio::test]
    async fn test_update_scan_directory_changes_label() {
        let pool = setup_test_db().await;
        let existing = add_scan_directory_impl(&pool, "/tmp/named", Some("Old"))
            .await
            .unwrap();
        let updated =
            update_scan_directory_impl(&pool, &existing.path, &existing.path, Some("New"))
                .await
                .unwrap();
        assert_eq!(updated.label.as_deref(), Some("New"));
        assert_eq!(updated.path, existing.path);
    }

    #[tokio::test]
    async fn test_add_scan_directory_expands_tilde() {
        let pool = setup_test_db().await;
        let dir = add_scan_directory_impl(&pool, "~/.skillshub/custom-scan", None)
            .await
            .unwrap();
        assert!(
            !dir.path.starts_with('~'),
            "tilde paths must be expanded before persistence"
        );
        assert!(dir.path.contains(".skillshub"));
    }

    #[tokio::test]
    async fn test_add_scan_directory_empty_path_fails() {
        let pool = setup_test_db().await;
        let result = add_scan_directory_impl(&pool, "   ", None).await;
        assert!(result.is_err(), "Empty path should fail validation");
    }

    #[tokio::test]
    async fn test_add_scan_directory_duplicate_path_fails() {
        let pool = setup_test_db().await;
        add_scan_directory_impl(&pool, "/tmp/same-path", None)
            .await
            .unwrap();
        let result = add_scan_directory_impl(&pool, "/tmp/same-path", None).await;
        assert!(
            result.is_err(),
            "Duplicate path should fail (UNIQUE constraint)"
        );
    }

    // ── remove_scan_directory_impl ────────────────────────────────────────────

    #[tokio::test]
    async fn test_remove_scan_directory_success() {
        let pool = setup_test_db().await;
        let removable = add_scan_directory_impl(&pool, "/tmp/removable", None)
            .await
            .unwrap();

        remove_scan_directory_impl(&pool, &removable.path)
            .await
            .unwrap();

        let dirs = get_scan_directories_impl(&pool).await.unwrap();
        // Built-in dirs remain; only the custom /tmp/removable should be gone.
        let builtin_count = expected_builtin_count();
        assert_eq!(
            dirs.len(),
            builtin_count,
            "Only the custom directory should be removed"
        );
        assert!(
            !dirs.iter().any(|d| d.path == removable.path),
            "Removed directory must not appear in the list"
        );
    }

    #[tokio::test]
    async fn test_remove_nonexistent_scan_directory_fails() {
        let pool = setup_test_db().await;
        let result = remove_scan_directory_impl(&pool, "/nonexistent/path").await;
        assert!(
            result.is_err(),
            "Removing a nonexistent directory should fail"
        );
    }

    #[tokio::test]
    async fn test_remove_builtin_scan_directory_fails() {
        let pool = setup_test_db().await;
        crate::db::upsert_builtin_scan_directory(&pool, "/builtin/path", "Builtin")
            .await
            .unwrap();

        let result = remove_scan_directory_impl(&pool, "/builtin/path").await;
        assert!(result.is_err(), "Removing a built-in directory should fail");
    }

    // ── set_scan_directory_active_impl ────────────────────────────────────────

    #[tokio::test]
    async fn test_set_scan_directory_active_disables() {
        let pool = setup_test_db().await;
        let added = add_scan_directory_impl(&pool, "/tmp/toggle-me", None)
            .await
            .unwrap();
        set_scan_directory_active_impl(&pool, &added.path, false)
            .await
            .unwrap();
        let dirs = get_scan_directories_impl(&pool).await.unwrap();
        let dir = dirs.iter().find(|d| d.path == added.path).unwrap();
        assert!(!dir.is_active, "Directory should be inactive");
    }

    #[tokio::test]
    async fn test_set_scan_directory_active_enables() {
        let pool = setup_test_db().await;
        let added = add_scan_directory_impl(&pool, "/tmp/re-enable-me", None)
            .await
            .unwrap();
        // First disable
        set_scan_directory_active_impl(&pool, &added.path, false)
            .await
            .unwrap();
        // Then re-enable
        set_scan_directory_active_impl(&pool, &added.path, true)
            .await
            .unwrap();
        let dirs = get_scan_directories_impl(&pool).await.unwrap();
        let dir = dirs.iter().find(|d| d.path == added.path).unwrap();
        assert!(dir.is_active, "Directory should be active again");
    }

    // ── get_setting_impl ──────────────────────────────────────────────────────

    #[tokio::test]
    async fn test_get_setting_not_set_returns_none() {
        let pool = setup_test_db().await;
        let value = get_setting_impl(&pool, "unset_key").await.unwrap();
        assert!(value.is_none(), "Unset key should return None");
    }

    #[tokio::test]
    async fn test_get_setting_after_set() {
        let pool = setup_test_db().await;
        set_setting_impl(&pool, "theme", "dark").await.unwrap();

        let value = get_setting_impl(&pool, "theme").await.unwrap();
        assert_eq!(value.as_deref(), Some("dark"));
    }

    // ── set_setting_impl ──────────────────────────────────────────────────────

    #[tokio::test]
    async fn note_prompt_changes_clear_cached_explanations() {
        let pool = setup_test_db().await;
        set_setting_impl(&pool, "ai_note_prompt", "old")
            .await
            .unwrap();
        crate::storage::fixtures::insert(&pool,"skill_explanations",serde_json::json!({"skill_id":"test","lang":"zh","model":"model","explanation":"cached"})).await.unwrap();
        set_setting_impl(&pool, "ai_note_prompt", "old")
            .await
            .unwrap();
        let count: i64 = crate::storage::fixtures::count(&pool, "skill_explanations", None);
        assert_eq!(count, 1);
        set_setting_impl(&pool, "ai_note_prompt", "new")
            .await
            .unwrap();
        let count: i64 = crate::storage::fixtures::count(&pool, "skill_explanations", None);
        assert_eq!(count, 0);
        assert_eq!(
            get_setting_impl(&pool, "ai_note_prompt")
                .await
                .unwrap()
                .as_deref(),
            Some("new")
        );
    }

    #[tokio::test]
    async fn test_set_setting_upserts() {
        let pool = setup_test_db().await;
        set_setting_impl(&pool, "lang", "en").await.unwrap();
        set_setting_impl(&pool, "lang", "zh").await.unwrap();

        let value = get_setting_impl(&pool, "lang").await.unwrap();
        assert_eq!(
            value.as_deref(),
            Some("zh"),
            "Second set should overwrite first"
        );
    }

    #[tokio::test]
    async fn test_set_setting_empty_key_fails() {
        let pool = setup_test_db().await;
        let result = set_setting_impl(&pool, "  ", "some-value").await;
        assert!(result.is_err(), "Empty key should fail validation");
    }

    #[tokio::test]
    async fn test_set_and_get_multiple_settings() {
        let pool = setup_test_db().await;
        set_setting_impl(&pool, "a", "1").await.unwrap();
        set_setting_impl(&pool, "b", "2").await.unwrap();
        set_setting_impl(&pool, "c", "3").await.unwrap();

        assert_eq!(
            get_setting_impl(&pool, "a").await.unwrap().as_deref(),
            Some("1")
        );
        assert_eq!(
            get_setting_impl(&pool, "b").await.unwrap().as_deref(),
            Some("2")
        );
        assert_eq!(
            get_setting_impl(&pool, "c").await.unwrap().as_deref(),
            Some("3")
        );
    }

    #[tokio::test]
    async fn test_set_setting_empty_value_is_allowed() {
        let pool = setup_test_db().await;
        // Empty value is valid — it means the key is explicitly set to empty string.
        let result = set_setting_impl(&pool, "empty-val", "").await;
        assert!(result.is_ok(), "Setting an empty value should succeed");
        let value = get_setting_impl(&pool, "empty-val").await.unwrap();
        assert_eq!(value.as_deref(), Some(""));
    }

    #[tokio::test]
    async fn test_update_central_skills_dir_persists_across_init() {
        let pool = setup_test_db().await;
        let tmp = tempfile::TempDir::new().unwrap();
        let old_central_dir = tmp.path().join("old-central-skills");
        let old_skill_dir = old_central_dir.join("legacy-skill");
        std::fs::create_dir_all(&old_skill_dir).unwrap();
        std::fs::write(
            old_skill_dir.join("SKILL.md"),
            "---\nname: legacy-skill\n---\n",
        )
        .unwrap();
        update_central_skills_dir_impl(&pool, &old_central_dir.to_string_lossy())
            .await
            .expect("old central dir should update");
        db::upsert_skill(
            &pool,
            &db::Skill {
                id: "legacy-skill".to_string(),
                name: "legacy-skill".to_string(),
                description: None,
                file_path: old_skill_dir
                    .join("SKILL.md")
                    .to_string_lossy()
                    .into_owned(),
                canonical_path: Some(old_skill_dir.to_string_lossy().into_owned()),
                is_central: true,
                source: None,
                content: None,
                scanned_at: chrono::Utc::now().to_rfc3339(),
            },
        )
        .await
        .unwrap();

        let central_dir = tmp.path().join("managed-central-skills");
        let central_dir_str = central_dir.to_string_lossy().to_string();

        let updated = update_central_skills_dir_impl(&pool, &central_dir_str)
            .await
            .expect("central dir should update");
        assert_eq!(updated.id, "central");
        assert_eq!(updated.global_skills_dir, central_dir_str);

        db::init_database(&pool)
            .await
            .expect("second init should not reset central dir");

        let central = db::get_agent_by_id(&pool, "central")
            .await
            .unwrap()
            .expect("central agent exists");
        assert_eq!(
            central.global_skills_dir, central_dir_str,
            "central path must not be overwritten by builtin seeding"
        );

        let legacy = db::get_skill_by_id(&pool, "legacy-skill")
            .await
            .unwrap()
            .expect("legacy skill record remains available to platform views");
        assert!(
            !legacy.is_central,
            "skills under the old Central root must not remain in the Central library after changing roots"
        );
    }

    #[test]
    fn test_normalize_version_tag_strips_v_prefix() {
        assert_eq!(normalize_version_tag("v0.12.1"), "0.12.1");
        assert_eq!(normalize_version_tag("V1.0.0"), "1.0.0");
    }

    #[test]
    fn test_version_comparison_detects_newer_release() {
        assert!(is_newer_version("0.12.0", "0.12.1"));
        assert!(is_newer_version("0.12.0", "0.13.0"));
        assert!(is_newer_version("0.12.9", "0.13.0"));
        assert!(!is_newer_version("0.12.0", "0.12.0"));
        assert!(!is_newer_version("0.12.1", "0.12.0"));
    }

    #[tokio::test]
    async fn test_update_app_data_dir_rejects_empty() {
        let pool = setup_test_db().await;
        let err = update_app_data_dir_impl(&pool, "  ").await.unwrap_err();
        assert!(err.contains("empty"));
    }
}

#[cfg(test)]
mod json_directory_tests {
    use super::*;
    use crate::storage::store::Store;
    use serde_json::json;
    #[tokio::test]
    async fn directory_copy_reopens_same_committed_data() {
        let fixture = tempfile::tempdir().unwrap();
        let source = fixture.path().join("source/.skillshub");
        let destination = fixture.path().join("target/.skillshub");
        let store = Store::open(&source).unwrap();
        store
            .write(|d| {
                d.table_mut("skills")
                    .insert("fixture".into(), json!({"id":"fixture"}));
                d.table_mut("skill_metadata")
                    .insert("fixture".into(), json!({"notes":"synthetic"}));
                Ok(())
            })
            .await
            .unwrap();
        crate::storage::fixtures::shard_metadata(&store).await;
        std::fs::write(source.join("config.json"), b"{}").unwrap();
        let called = std::cell::Cell::new(false);
        update_app_data_dir_with_publish(&store, destination.to_str().unwrap(), |path| {
            assert_eq!(path, destination);
            called.set(true);
            assert!(Store::open(path).is_err());
            Ok(())
        })
        .await
        .unwrap();
        assert!(called.get());
        let copied = Store::open(&destination).unwrap();
        assert_eq!(
            store
                .read(|d| Ok(serde_json::to_value(d).unwrap()))
                .unwrap(),
            copied
                .read(|d| Ok(serde_json::to_value(d).unwrap()))
                .unwrap()
        );
    }
    #[tokio::test]
    async fn occupied_destination_does_not_publish_pointer() {
        let fixture = tempfile::tempdir().unwrap();
        let store = Store::open(&fixture.path().join("source/.skillshub")).unwrap();
        let destination = fixture.path().join("target/.skillshub");
        let occupied = Store::open(&destination).unwrap();
        occupied
            .write(|d| {
                d.table_mut("skills").insert("owner".into(), json!(1));
                Ok(())
            })
            .await
            .unwrap();
        let called = std::cell::Cell::new(false);
        assert!(
            update_app_data_dir_with_publish(&store, destination.to_str().unwrap(), |_| {
                called.set(true);
                Ok(())
            })
            .await
            .is_err()
        );
        assert!(!called.get());
        assert_eq!(
            occupied
                .read(|d| Ok(d.table("skills")["owner"].clone()))
                .unwrap(),
            json!(1)
        );
    }

    #[tokio::test]
    async fn occupied_empty_destination_rejects_copy_before_writing() {
        let fixture = tempfile::tempdir().unwrap();
        let source = fixture.path().join("source/.skillshub");
        let store = Store::open(&source).unwrap();
        std::fs::write(source.join("config.json"), b"{}").unwrap();
        let destination = fixture.path().join("target/.skillshub");
        let _occupied = Store::open(&destination).unwrap();
        let called = std::cell::Cell::new(false);
        assert!(
            update_app_data_dir_with_publish(&store, destination.to_str().unwrap(), |_| {
                called.set(true);
                Ok(())
            })
            .await
            .is_err()
        );
        assert!(!called.get());
        assert!(!destination.join("config.json").exists());
    }
    #[tokio::test]
    async fn corrupt_destination_does_not_publish_pointer() {
        let fixture = tempfile::tempdir().unwrap();
        let store = Store::open(&fixture.path().join("source/.skillshub")).unwrap();
        let destination = fixture.path().join("target/.skillshub");
        std::fs::create_dir_all(destination.join("store")).unwrap();
        std::fs::write(destination.join("store/manifest.json"), b"corrupt").unwrap();
        let called = std::cell::Cell::new(false);
        assert!(
            update_app_data_dir_with_publish(&store, destination.to_str().unwrap(), |_| {
                called.set(true);
                Ok(())
            })
            .await
            .is_err()
        );
        assert!(!called.get());
        assert_eq!(
            std::fs::read(destination.join("store/manifest.json")).unwrap(),
            b"corrupt"
        );
    }
}
