//! Journaled source and directory migration preserves stable skill identities.
use super::{github_import as github, linker, notebook, notebook_repository, repository_mirror};
use crate::{
    db::{self, DbPool},
    path_utils::source_grouped_skill_dir,
    AppState,
};
use serde::{Deserialize, Serialize};
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
    let recovered = recover(&state.db).await?;
    let url = github::normalize_github_repo_input(&new_address).map_err(|_| "addressInvalid")?;
    if let Some((_, target)) = recovered.iter().find(|(old, new)| {
        old.eq_ignore_ascii_case(&repository)
            && url.eq_ignore_ascii_case(&format!("https://github.com/{new}"))
    }) {
        return Ok(target.clone());
    }
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
    recover(pool).await?;
    linker::normalize_resource_backed_central_records_impl(pool).await?;
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
    let new_path =
        source_grouped_skill_dir(&root, new.split('/').next(), Some(new), None, "placeholder")
            .parent()
            .ok_or("addressLayout")?
            .to_path_buf();
    repository_mirror::ensure_owned_path(&root, &new_path)?;
    let same_path = crate::path_utils::paths_resolve_to_same_entry(&path, &new_path);
    if !same_path && std::fs::symlink_metadata(&new_path).is_ok() {
        return Err("addressConflict".into());
    }
    let relative = new_path
        .strip_prefix(&root)
        .map_err(|_| "UNSAFE_TARGET")?
        .to_string_lossy()
        .replace('\\', "/");
    let directory_conflict: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM repository_local_directories WHERE directory=? COLLATE NOCASE AND repository<>? COLLATE NOCASE")
        .bind(&relative).bind(old).fetch_one(pool).await.map_err(|_| "addressDatabase")?;
    if directory_conflict != 0 {
        return Err("addressConflict".into());
    }
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
    let busy: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM repository_mirror_snapshots WHERE (repository=? COLLATE NOCASE OR repository=? COLLATE NOCASE) AND (state='applying' OR (state='failed' AND id IN (SELECT snapshot_id FROM repository_mirror_recovery)))")
        .bind(old).bind(new).fetch_one(pool).await.map_err(|_| "addressDatabase")?;
    if busy != 0 {
        return Err("addressBusy".into());
    }
    let exists: i64 = sqlx::query_scalar("SELECT (SELECT COUNT(*) FROM skill_sources WHERE source_repo=? COLLATE NOCASE) + (SELECT COUNT(*) FROM repository_mirrors WHERE repository=? COLLATE NOCASE)")
        .bind(old).bind(old).fetch_one(pool).await.map_err(|_| "addressDatabase")?;
    if exists == 0 {
        return Err("addressMissing".into());
    }
    let conflict: i64 = sqlx::query_scalar("SELECT (SELECT COUNT(*) FROM skill_sources WHERE source_repo=? COLLATE NOCASE) + (SELECT COUNT(*) FROM repository_mirrors WHERE repository=? COLLATE NOCASE) + (SELECT COUNT(*) FROM repository_local_directories WHERE repository=? COLLATE NOCASE)")
        .bind(new).bind(new).bind(new).fetch_one(pool).await.map_err(|_| "addressDatabase")?;
    if conflict != 0 {
        return Err("addressConflict".into());
    }
    // Ensure all members use this repository container before preserving it.
    let rows = sqlx::query("SELECT s.id,s.file_path,s.canonical_path FROM skills s JOIN skill_sources ss ON ss.skill_id=s.id WHERE ss.source_repo=? COLLATE NOCASE")
        .bind(old).fetch_all(pool).await.map_err(|_| "addressDatabase")?;
    let mut updates = Vec::new();
    let mut links = Vec::new();
    for row in rows {
        let id: String = row.get("id");
        let target = PathBuf::from(
            row.get::<Option<String>, _>("canonical_path")
                .ok_or("addressLayout")?,
        );
        repository_mirror::ensure_owned_path(&root, &target)?;
        let resolved = target.canonicalize().unwrap_or(target.clone());
        let container = path.canonicalize().unwrap_or(path.clone());
        if resolved.parent() != Some(container.as_path()) || !target.join("SKILL.md").is_file() {
            return Err("addressLayout".into());
        }
        let destination = new_path.join(target.file_name().ok_or("addressLayout")?);
        links.extend(
            linker::prepare_installation_relocation_impl(pool, &id, &target, &destination)
                .await
                .map_err(|_| "addressLinks")?,
        );
        let file = PathBuf::from(row.get::<String, _>("file_path"));
        if !crate::path_utils::paths_resolve_to_same_entry(
            file.parent().ok_or("addressLayout")?,
            &target,
        ) {
            return Err("addressLayout".into());
        }
        updates.push((
            id,
            destination.join(file.file_name().ok_or("addressLayout")?),
            destination,
        ));
    }
    // Refuse to move a directory that contains records owned by another source.
    for row in sqlx::query("SELECT id,canonical_path FROM skills")
        .fetch_all(pool)
        .await
        .map_err(|_| "addressDatabase")?
    {
        let id: String = row.get("id");
        if row
            .get::<Option<String>, _>("canonical_path")
            .as_ref()
            .is_some_and(|p| Path::new(p).starts_with(&path))
            && !updates.iter().any(|(own, _, _)| own == &id)
        {
            return Err("addressLayout".into());
        }
    }
    if std::fs::symlink_metadata(&path).is_ok() && !path.is_dir() {
        return Err("addressLayout".into());
    }
    let journal = MoveJournal {
        old: old.into(),
        new: new.into(),
        from: path
            .strip_prefix(&root)
            .map_err(|_| "UNSAFE_TARGET")?
            .to_string_lossy()
            .replace('\\', "/"),
        to: relative.clone(),
        had_directory: path.is_dir(),
        links,
    };
    let move_id = uuid::Uuid::new_v4().to_string();
    sqlx::query("INSERT INTO repository_address_moves(id,payload) VALUES(?,?)")
        .bind(&move_id)
        .bind(serde_json::to_string(&journal).map_err(|_| "addressDatabase")?)
        .execute(pool)
        .await
        .map_err(|_| "addressDatabase")?;
    let result: Result<(),String> = async {
        if !same_path {
            std::fs::create_dir_all(new_path.parent().ok_or("addressLayout")?).map_err(|_| "addressMove")?;
            if journal.had_directory { std::fs::rename(&path, &new_path).map_err(|_| "addressMove")?; }
            else { std::fs::create_dir(&new_path).map_err(|_| "addressMove")?; }
        }
        let mut tx = pool.begin().await.map_err(|_| "addressDatabase")?;
        for (id, file, canonical) in &updates {
            sqlx::query("UPDATE skills SET canonical_path=?,file_path=? WHERE id=?")
                .bind(canonical.to_string_lossy().as_ref()).bind(file.to_string_lossy().as_ref()).bind(id).execute(&mut *tx).await.map_err(|_| "addressDatabase")?;
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
    sqlx::query("UPDATE repository_address_moves SET committed=1 WHERE id=?").bind(&move_id).execute(&mut *tx).await.map_err(|_| "addressDatabase")?;
    tx.commit().await.map_err(|_| "addressDatabase".into())
    }.await;
    if let Err(error) = result {
        let completed = recover(pool).await.map_err(|_| "addressRecovery")?;
        if completed
            .iter()
            .any(|(from, to)| from.eq_ignore_ascii_case(old) && to.eq_ignore_ascii_case(new))
        {
            return Ok(());
        }
        return Err(error);
    }
    recover(pool).await.map_err(|_| "addressRecovery")?;
    Ok(())
}

#[derive(Serialize, Deserialize)]
struct MoveJournal {
    old: String,
    new: String,
    from: String,
    to: String,
    had_directory: bool,
    links: Vec<linker::InstallationRelocation>,
}

/// Roll back an uncommitted rename, or finish links after a committed database move.
/// Must run before any scanner interprets temporarily missing directories.
pub(crate) async fn recover(pool: &DbPool) -> Result<Vec<(String, String)>, String> {
    let root = db::get_skill_resource_library_dir(pool).await?;
    let rows = sqlx::query("SELECT id,payload,committed FROM repository_address_moves")
        .fetch_all(pool)
        .await
        .map_err(|_| "addressDatabase")?;
    let mut completed = Vec::new();
    for row in rows {
        let journal: MoveJournal =
            serde_json::from_str(row.get("payload")).map_err(|_| "addressRecovery")?;
        let from = root.join(journal.from.split('/').collect::<PathBuf>());
        let to = root.join(journal.to.split('/').collect::<PathBuf>());
        repository_mirror::ensure_owned_path(&root, &from).map_err(|_| "addressRecovery")?;
        repository_mirror::ensure_owned_path(&root, &to).map_err(|_| "addressRecovery")?;
        let committed: bool = row.get("committed");
        if committed {
            if !to.is_dir() {
                return Err("addressRecovery".into());
            }
            for link in &journal.links {
                linker::apply_installation_relocation_impl(pool, link)
                    .await
                    .map_err(|_| "addressRecovery")?;
            }
            completed.push((journal.old, journal.new));
            let _ = crate::path_utils::prune_empty_parent_dirs(&from, &root);
        } else if !crate::path_utils::paths_resolve_to_same_entry(&from, &to) {
            if journal.had_directory {
                match (from.is_dir(), to.is_dir()) {
                    (false, true) => {
                        std::fs::rename(&to, &from).map_err(|_| "addressRecovery")?;
                    }
                    (true, false) => {}
                    _ => return Err("addressRecovery".into()),
                }
            } else if to.is_dir() {
                std::fs::remove_dir(&to).map_err(|_| "addressRecovery")?;
            }
            let _ = crate::path_utils::prune_empty_parent_dirs(&to, &root);
        }
        sqlx::query("DELETE FROM repository_address_moves WHERE id=?")
            .bind(row.get::<String, _>("id"))
            .execute(pool)
            .await
            .map_err(|_| "addressDatabase")?;
    }
    Ok(completed)
}
