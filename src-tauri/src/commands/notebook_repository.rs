//! Repository notebook persistence. Short overview notes remain in folder_notes.
use crate::{db::DbPool, AppState};
use serde::{Deserialize, Serialize};
use sqlx::Row;
use std::collections::{BTreeMap, HashSet};
use tauri::State;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Repository {
    pub id: i64,
    pub source_key: String,
    pub repository_uid: String,
    pub full_name: String,
    pub url: String,
    pub description: String,
    pub tags: Vec<String>,
    pub stars: Option<i64>,
    #[serde(default)]
    pub skill_ids: Vec<String>,
    #[serde(default)]
    pub notebook: String,
    #[serde(default)]
    pub obsidian: Option<super::notebook::Binding>,
}

#[cfg(test)]
impl Repository {
    pub fn new(id: i64, full_name: String) -> Self {
        Self { id, source_key: format!("github:{full_name}"), repository_uid: uuid::Uuid::new_v4().to_string(), full_name,
            url: String::new(), description: String::new(), tags: Vec::new(), stars: None, skill_ids: Vec::new(), notebook: String::new(), obsidian: None }
    }
}
#[cfg(test)]
pub async fn create_pool(path: &std::path::Path) -> Result<DbPool, String> {
    let options = sqlx::sqlite::SqliteConnectOptions::new().filename(path).create_if_missing(true);
    let pool = sqlx::sqlite::SqlitePoolOptions::new().max_connections(1).connect_with(options).await.map_err(|_| "noteDatabase")?;
    crate::db::init_database(&pool).await?;
    Ok(pool)
}

pub async fn schema(pool: &DbPool) -> Result<(), String> {
    sqlx::query("CREATE TABLE IF NOT EXISTS repository_notebooks (id INTEGER PRIMARY KEY AUTOINCREMENT, source_key TEXT NOT NULL UNIQUE, repository_uid TEXT NOT NULL UNIQUE, data TEXT NOT NULL)")
        .execute(pool).await.map_err(|_| "noteDatabase")?;
    Ok(())
}
pub async fn list(pool: &DbPool) -> Result<Vec<Repository>, String> {
    schema(pool).await?;
    sqlx::query("SELECT data FROM repository_notebooks ORDER BY id").fetch_all(pool).await.map_err(|_| "noteDatabase")?
        .into_iter().map(|r| serde_json::from_str(r.get("data")).map_err(|_| "noteDatabase".into())).collect()
}
pub async fn get(pool: &DbPool, id: i64) -> Result<Repository, String> {
    list(pool).await?.into_iter().find(|r| r.id == id).ok_or_else(|| "noteRepositoryMissing".into())
}
pub fn validate(rows: &[Repository]) -> Result<(), String> {
    let (mut sources, mut uids, mut notes) = (HashSet::new(), HashSet::new(), HashSet::new());
    for row in rows {
        let uid = uuid::Uuid::parse_str(&row.repository_uid).map_err(|_| "noteIdentity")?.to_string();
        if row.source_key.is_empty() || row.notebook.len() > 4 * 1024 * 1024
            || !sources.insert(&row.source_key) || !uids.insert(uid) {
            return Err("noteIdentity".into());
        }
        if let Some(binding) = &row.obsidian {
            if !super::notebook::validate_binding(binding) || binding.note_id.as_ref().is_some_and(|id| !notes.insert(id.to_lowercase())) {
                return Err("noteIdentity".into());
            }
        }
    }
    Ok(())
}
pub async fn put(pool: &DbPool, row: &Repository) -> Result<(), String> {
    schema(pool).await?;
    let data = serde_json::to_string(row).map_err(|_| "noteDatabase")?;
    sqlx::query("INSERT INTO repository_notebooks (id, source_key, repository_uid, data) VALUES (?, ?, ?, ?) ON CONFLICT(id) DO UPDATE SET source_key=excluded.source_key, repository_uid=excluded.repository_uid, data=excluded.data")
        .bind(row.id).bind(&row.source_key).bind(&row.repository_uid).bind(data).execute(pool).await.map_err(|_| "noteDatabase")?;
    Ok(())
}

// Local integer IDs may differ after restore; stable UUIDs and source keys survive.
pub async fn restore(pool: &DbPool, rows: &[Repository]) -> Result<(), String> {
    let _guard = super::notebook::WRITE.lock().await;
    validate(rows)?;
    schema(pool).await?;
    let existing = list(pool).await?;
    let mut merged = existing.clone();
    for row in rows {
        merged.retain(|r| r.source_key != row.source_key);
        merged.push(row.clone());
    }
    validate(&merged)?;
    let mut tx = pool.begin().await.map_err(|_| "noteDatabase")?;
    for row in rows {
        let mut row = row.clone();
        row.id = if let Some(old) = existing.iter().find(|r| r.source_key == row.source_key) { old.id } else {
            let max: i64 = sqlx::query_scalar("SELECT COALESCE(MAX(id), 0) FROM repository_notebooks").fetch_one(&mut *tx).await.map_err(|_| "noteDatabase")?;
            max + 1
        };
        sqlx::query("INSERT INTO repository_notebooks (id, source_key, repository_uid, data) VALUES (?, ?, ?, ?) ON CONFLICT(source_key) DO UPDATE SET repository_uid=excluded.repository_uid, data=excluded.data")
            .bind(row.id).bind(&row.source_key).bind(&row.repository_uid).bind(serde_json::to_string(&row).map_err(|_| "noteDatabase")?)
            .execute(&mut *tx).await.map_err(|_| "noteDatabase")?;
    }
    tx.commit().await.map_err(|_| "noteDatabase".into())
}

pub struct Category { pub id: String, pub name: String, pub parent_id: Option<String>, pub repository_ids: Vec<i64> }
pub async fn categories(pool: &DbPool) -> Result<Vec<Category>, String> {
    let rows = list(pool).await?;
    let collections = crate::db::get_all_collections(pool).await?;
    let mut result = Vec::new();
    for c in collections {
        let members: Vec<String> = sqlx::query_scalar("SELECT skill_id FROM collection_skills WHERE collection_id = ?")
            .bind(&c.id).fetch_all(pool).await.map_err(|_| "noteDatabase")?;
        result.push(Category { id: c.id, name: c.name, parent_id: None,
            repository_ids: rows.iter().filter(|r| r.skill_ids.iter().any(|id| members.contains(id))).map(|r| r.id).collect() });
    }
    Ok(result)
}

#[tauri::command]
pub async fn list_repository_notebooks(state: State<'_, AppState>) -> Result<Vec<Repository>, String> { list(&state.db).await }

#[tauri::command]
pub async fn ensure_repository_notebook(state: State<'_, AppState>, directory: String, skill_id: String) -> Result<Repository, String> {
    let _guard = super::notebook::WRITE.lock().await;
    ensure_repository_notebook_impl(&state.db, directory, skill_id).await
}

async fn ensure_repository_notebook_impl(pool: &DbPool, directory: String, skill_id: String) -> Result<Repository, String> {
    let skill = crate::db::get_skill_by_id(pool, &skill_id).await?.ok_or("noteRepositoryMissing")?;
    let dir = super::skills::repository_document_directory(pool, directory, Some(skill_id.clone())).await?;
    let dir = dir.canonicalize().map_err(|_| "noteRepositoryMissing")?;
    let root = crate::db::get_skill_resource_library_dir(pool).await?.canonicalize().map_err(|_| "noteRepositoryMissing")?;
    let relative = dir.strip_prefix(&root).map_err(|_| "noteRepositoryMissing")?;
    if relative.as_os_str().is_empty() { return Err("noteRepositoryMissing".into()); }
    let source: Option<String> = sqlx::query_scalar("SELECT source_repo FROM skill_sources WHERE skill_id = ?")
        .bind(&skill_id).fetch_optional(pool).await.map_err(|_| "noteDatabase")?.flatten();
    let full_name = source.clone().unwrap_or_else(|| relative.to_string_lossy().replace('\\', "/"));
    let key = if source.is_some() { format!("github:{}", full_name.to_lowercase()) } else { format!("local:{}", if cfg!(windows) { full_name.to_lowercase() } else { full_name.clone() }) };
    let mut row = match list(pool).await?.into_iter().find(|r| r.source_key == key || (source.is_none() && r.source_key.starts_with("local:") && r.skill_ids.contains(&skill_id))) {
        Some(row) => row,
        None => {
            let uid = uuid::Uuid::new_v4().to_string();
            let id: i64 = sqlx::query_scalar("SELECT COALESCE(MAX(id), 0) + 1 FROM repository_notebooks")
                .fetch_one(pool).await.map_err(|_| "noteDatabase")?;
            Repository { id, source_key: key.clone(), repository_uid: uid, full_name: full_name.clone(), url: String::new(), description: String::new(), tags: vec![], stars: None, skill_ids: vec![], notebook: String::new(), obsidian: None }
        }
    };
    row.source_key = key;
    row.full_name = full_name;
    row.url = source.as_ref().map(|s| format!("https://github.com/{s}")).unwrap_or_default();
    row.description = skill.description.unwrap_or_default();
    if source.is_some() { row.stars = crate::db::get_github_stars(pool, &row.full_name).await?; }
    let members = crate::db::get_resource_library_skills(pool).await?;
    row.skill_ids = members.into_iter().filter(|s| {
        s.canonical_path.as_ref().map(std::path::Path::new).is_some_and(|p| p.starts_with(&dir))
    }).map(|s| s.id).collect();
    if !row.skill_ids.contains(&skill_id) { row.skill_ids.push(skill_id); }
    row.tags.clear();
    for id in &row.skill_ids {
        let metadata = crate::db::get_skill_metadata(pool, id).await?;
        row.tags.extend(crate::db::parse_skill_metadata_tags(metadata.as_ref()));
    }
    row.tags.sort(); row.tags.dedup();
    put(pool, &row).await?;
    Ok(row)
}

#[tauri::command]
pub async fn save_internal_notebook(state: State<'_, AppState>, id: i64, body: String, expected_body: String) -> Result<(), String> {
    let _guard = super::notebook::WRITE.lock().await;
    save_internal_impl(&state.db, id, body, expected_body).await
}

async fn save_internal_impl(pool: &DbPool, id: i64, body: String, expected_body: String) -> Result<(), String> {
    let mut row = get(pool, id).await?;
    if row.obsidian.is_some() { return Err("notebookLinked".into()); }
    if row.notebook != expected_body { return Err("noteConflict".into()); }
    if body.len() > 4 * 1024 * 1024 { return Err("tooLarge".into()); }
    row.notebook = body;
    put(pool, &row).await
}

#[tauri::command]
pub fn get_notebook_settings(state: State<'_, AppState>) -> Result<BTreeMap<String, String>, String> {
    Ok(crate::db::load_config(&state.db)?.settings.into_iter().filter(|(key, _)| key.starts_with("obsidian_")).collect())
}
#[tauri::command]
pub fn save_notebook_settings(state: State<'_, AppState>, vault_path: String, folder: String, template: String) -> Result<(), String> {
    let folder = super::notebook::normalize_note_folder(&folder)?;
    super::notebook::validate_template(&template)?;
    if !vault_path.trim().is_empty() && !std::path::Path::new(&vault_path).join(".obsidian").is_dir() { return Err("obsidianSetup".into()); }
    crate::db::update_config(&state.db, |config| {
        config.settings.insert("obsidian_vault_path".into(), vault_path);
        config.settings.insert("obsidian_note_folder".into(), folder);
        config.settings.insert("obsidian_property_template".into(), template);
        Ok(())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn internal_writes_reject_stale_drafts_and_linked_notes() {
        let dir = tempfile::tempdir().unwrap();
        let pool = create_pool(&dir.path().join("db.sqlite")).await.unwrap();
        let mut row = Repository::new(1, "fixture/project".into());
        row.notebook = "original".into();
        put(&pool, &row).await.unwrap();
        save_internal_impl(&pool, 1, "new".into(), "original".into()).await.unwrap();
        assert_eq!(save_internal_impl(&pool, 1, "stale".into(), "original".into()).await.unwrap_err(), "noteConflict");
        assert_eq!(get(&pool, 1).await.unwrap().notebook, "new");
        row.obsidian = Some(super::super::notebook::Binding {
            binding_version: 2, note_id: Some(uuid::Uuid::new_v4().to_string()), vault_key: "default".into(),
            relative_path: "notes/a.md".into(), obsidian_uri: "unused".into(),
        });
        put(&pool, &row).await.unwrap();
        assert_eq!(save_internal_impl(&pool, 1, "overwrite".into(), "original".into()).await.unwrap_err(), "notebookLinked");
    }

    #[tokio::test]
    async fn local_repository_identity_is_shared_by_members_and_survives_root_remapping() {
        let dir = tempfile::tempdir().unwrap();
        let pool = create_pool(&dir.path().join("db.sqlite")).await.unwrap();
        let root = dir.path().join("library");
        let skill_dir = root.join("fixture/project/one");
        std::fs::create_dir_all(&skill_dir).unwrap();
        crate::db::set_skill_resource_library_dir(&pool, root.to_str().unwrap()).await.unwrap();
        let mut skill = crate::db::Skill { id: "fixture-member".into(), name: "one".into(), description: None,
            file_path: skill_dir.join("SKILL.md").to_string_lossy().into(), canonical_path: Some(skill_dir.to_string_lossy().into()),
            is_central: false, source: None, content: None, scanned_at: String::new() };
        crate::db::upsert_skill(&pool, &skill).await.unwrap();
        let first = ensure_repository_notebook_impl(&pool, String::new(), skill.id.clone()).await.unwrap();
        let repeated = ensure_repository_notebook_impl(&pool, "ignored installed path".into(), skill.id.clone()).await.unwrap();
        assert_eq!(first.repository_uid, repeated.repository_uid);
        let moved_root = dir.path().join("moved-library");
        std::fs::rename(&root, &moved_root).unwrap();
        let moved = moved_root.join("fixture/project/one");
        skill.canonical_path = Some(moved.to_string_lossy().into());
        skill.file_path = moved.join("SKILL.md").to_string_lossy().into();
        crate::db::upsert_skill(&pool, &skill).await.unwrap();
        crate::db::set_skill_resource_library_dir(&pool, moved_root.to_str().unwrap()).await.unwrap();
        let remapped = ensure_repository_notebook_impl(&pool, String::new(), skill.id.clone()).await.unwrap();
        assert_eq!(first.repository_uid, remapped.repository_uid);
        assert_eq!(list(&pool).await.unwrap().len(), 1);
        assert!(ensure_repository_notebook_impl(&pool, String::new(), "missing".into()).await.is_err());
    }
}
