//! Source migration keeps physical paths and stable skill identities intact.
use super::{github_import as github, notebook, notebook_repository, repository_mirror};
use crate::{
    db::{self, DbPool},
    path_utils::source_grouped_skill_dir,
    AppState,
};
use sqlx::Row;
use std::path::{Path, PathBuf};
use tauri::State;

pub(crate) async fn directory(
    pool: &DbPool,
    root: &Path,
    owner: &str,
    repository: &str,
) -> Result<PathBuf, String> {
    let saved: Option<String> =
        sqlx::query_scalar("SELECT directory FROM repository_local_directories WHERE repository=?")
            .bind(repository)
            .fetch_optional(pool)
            .await
            .map_err(|_| "addressDatabase")?;
    let path = if let Some(saved) = saved {
        root.join(saved.split('/').collect::<PathBuf>())
    } else {
        source_grouped_skill_dir(root, Some(owner), Some(repository), None, "placeholder")
            .parent()
            .ok_or("UNSAFE_TARGET")?
            .to_path_buf()
    };
    repository_mirror::ensure_owned_path(root, &path)?;
    let relative = path
        .strip_prefix(root)
        .map_err(|_| "UNSAFE_TARGET")?
        .to_string_lossy()
        .replace('\\', "/");
    let occupied: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM repository_local_directories WHERE directory=? COLLATE NOCASE AND repository<>? COLLATE NOCASE")
        .bind(&relative).bind(repository).fetch_one(pool).await.map_err(|_| "addressDatabase")?;
    if occupied != 0 {
        return Err("addressConflict".into());
    }
    Ok(path)
}

#[tauri::command]
pub async fn change_repository_address(
    state: State<'_, AppState>,
    repository: String,
    new_address: String,
) -> Result<String, String> {
    let _guard = repository_mirror::MIRROR_LOCK
        .try_lock()
        .map_err(|_| "addressBusy")?;
    let url = github::normalize_github_repo_input(&new_address).map_err(|_| "addressInvalid")?;
    let auth = github::github_direct_auth_from_settings(&state.db)
        .await
        .map_err(|_| "addressValidation")?;
    let reference = github::resolve_repo_ref(&url, auth.as_deref())
        .await
        .map_err(|_| "addressValidation")?;
    let target = format!("{}/{}", reference.owner, reference.repo);
    let revision = github::resolve_update_revision(&target, auth.as_deref(), None)
        .await
        .map_err(|_| "addressValidation")?;
    github::fetch_mirror_snapshot(&revision, auth.as_deref())
        .await
        .map_err(|_| "addressValidation")?;
    migrate(&state.db, &repository, &target).await?;
    Ok(target)
}

// Caller holds MIRROR_LOCK, including while validating the new remote snapshot.
pub(crate) async fn migrate(pool: &DbPool, old: &str, new: &str) -> Result<(), String> {
    if old.eq_ignore_ascii_case(new) {
        return Err("addressUnchanged".into());
    }
    let root = db::get_skill_resource_library_dir(pool).await?;
    let spelling: Option<String> = sqlx::query_scalar(
        "SELECT source_repo FROM skill_sources WHERE source_repo=? COLLATE NOCASE LIMIT 1",
    )
    .bind(old)
    .fetch_optional(pool)
    .await
    .map_err(|_| "addressDatabase")?;
    let display = spelling.as_deref().unwrap_or(old);
    let owner = display.split('/').next().ok_or("addressInvalid")?;
    let path = directory(pool, &root, owner, display).await?;
    let relative = path
        .strip_prefix(&root)
        .map_err(|_| "UNSAFE_TARGET")?
        .to_string_lossy()
        .replace('\\', "/");
    let _notes = notebook::WRITE.lock().await;
    let notebooks = notebook_repository::list(pool).await?;
    let old_key = format!("github:{}", old.to_lowercase());
    let new_key = format!("github:{}", new.to_lowercase());
    if notebooks
        .iter()
        .any(|row| row.source_key.eq_ignore_ascii_case(&new_key))
    {
        return Err("addressConflict".into());
    }
    let mut tx = pool.begin().await.map_err(|_| "addressDatabase")?;
    let busy: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM repository_mirror_snapshots WHERE (repository=? COLLATE NOCASE OR repository=? COLLATE NOCASE) AND (state='applying' OR (state='failed' AND id IN (SELECT snapshot_id FROM repository_mirror_recovery)))")
        .bind(old).bind(new).fetch_one(&mut *tx).await.map_err(|_| "addressDatabase")?;
    if busy != 0 {
        return Err("addressBusy".into());
    }
    let exists: i64 = sqlx::query_scalar("SELECT (SELECT COUNT(*) FROM skill_sources WHERE source_repo=? COLLATE NOCASE) + (SELECT COUNT(*) FROM repository_mirrors WHERE repository=? COLLATE NOCASE)")
        .bind(old).bind(old).fetch_one(&mut *tx).await.map_err(|_| "addressDatabase")?;
    if exists == 0 {
        return Err("addressMissing".into());
    }
    let conflict: i64 = sqlx::query_scalar("SELECT (SELECT COUNT(*) FROM skill_sources WHERE source_repo=? COLLATE NOCASE) + (SELECT COUNT(*) FROM repository_mirrors WHERE repository=? COLLATE NOCASE) + (SELECT COUNT(*) FROM repository_local_directories WHERE repository=? COLLATE NOCASE)")
        .bind(new).bind(new).bind(new).fetch_one(&mut *tx).await.map_err(|_| "addressDatabase")?;
    if conflict != 0 {
        return Err("addressConflict".into());
    }
    // Ensure all members use this repository container before preserving it.
    let rows = sqlx::query("SELECT s.canonical_path FROM skills s JOIN skill_sources ss ON ss.skill_id=s.id WHERE ss.source_repo=? COLLATE NOCASE")
        .bind(old).fetch_all(&mut *tx).await.map_err(|_| "addressDatabase")?;
    for row in rows {
        let target = PathBuf::from(
            row.get::<Option<String>, _>("canonical_path")
                .ok_or("addressLayout")?,
        );
        // Shared-center records may point at a symlink to the same resource.
        let resolved = target.canonicalize().unwrap_or(target);
        let container = path.canonicalize().unwrap_or(path.clone());
        if resolved.parent() != Some(container.as_path()) {
            return Err("addressLayout".into());
        }
    }
    sqlx::query("UPDATE skills SET source='github:' || ? WHERE id IN (SELECT skill_id FROM skill_sources WHERE source_repo=? COLLATE NOCASE)")
        .bind(new).bind(old).execute(&mut *tx).await.map_err(|_| "addressDatabase")?;
    // Clear old raw download URLs; source_repo + source_path resolve the new remote.
    sqlx::query("UPDATE skill_sources SET source_repo=?,source_author=?,source_url=NULL,updated_at=? WHERE source_repo=? COLLATE NOCASE")
        .bind(new).bind(new.split('/').next()).bind(chrono::Utc::now().to_rfc3339()).bind(old).execute(&mut *tx).await.map_err(|_| "addressDatabase")?;
    for table in [
        "repository_mirrors",
        "repository_local_directories",
        "github_repository_stats",
    ] {
        sqlx::query(&format!(
            "DELETE FROM {table} WHERE repository=? COLLATE NOCASE"
        ))
        .bind(old)
        .execute(&mut *tx)
        .await
        .map_err(|_| "addressDatabase")?;
    }
    sqlx::query("INSERT INTO repository_mirrors(repository) VALUES(?)")
        .bind(new.to_lowercase())
        .execute(&mut *tx)
        .await
        .map_err(|_| "addressDatabase")?;
    sqlx::query("INSERT INTO repository_local_directories(repository,directory) VALUES(?,?)")
        .bind(new.to_lowercase())
        .bind(relative)
        .execute(&mut *tx)
        .await
        .map_err(|_| "addressDatabase")?;
    // Retain recovery data, but make all old previews unusable, including queued IDs.
    sqlx::query("UPDATE repository_mirror_snapshots SET state='invalidated' WHERE repository=? COLLATE NOCASE")
        .bind(old).execute(&mut *tx).await.map_err(|_| "addressDatabase")?;
    for mut row in notebooks
        .into_iter()
        .filter(|row| row.source_key.eq_ignore_ascii_case(&old_key))
    {
        row.source_key = new_key.clone();
        row.full_name = new.into();
        row.url = format!("https://github.com/{new}");
        row.stars = None;
        sqlx::query("UPDATE repository_notebooks SET source_key=?,data=? WHERE id=?")
            .bind(&new_key)
            .bind(serde_json::to_string(&row).map_err(|_| "addressDatabase")?)
            .bind(row.id)
            .execute(&mut *tx)
            .await
            .map_err(|_| "addressDatabase")?;
    }
    tx.commit().await.map_err(|_| "addressDatabase".into())
}
