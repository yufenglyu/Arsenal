//! Journaled source and directory migration preserves stable skill identities.
use super::{github_import as github, linker, notebook, notebook_repository, repository_mirror};
use crate::{
    db::{self, DbPool},
    path_utils::source_grouped_skill_dir,
    AppState,
};
use serde::{Deserialize, Serialize};

use std::path::{Path, PathBuf};
use tauri::State;

pub(crate) async fn directory(
    pool: &DbPool,
    root: &Path,
    owner: &str,
    repository: &str,
) -> Result<PathBuf, String> {
    let saved = pool.read(|d| {
        Ok(d.table("repository_local_directories")
            .get(repository)
            .and_then(|r| r["directory"].as_str())
            .map(str::to_owned))
    })?;
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
    let occupied = directory_conflicts(pool, &relative, repository)?;
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
    let spelling = pool.read(|d| {
        Ok(d.table("skill_sources")
            .values()
            .filter_map(|r| r["source_repo"].as_str())
            .find(|v| v.eq_ignore_ascii_case(old))
            .map(str::to_owned))
    })?;
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
    let directory_conflict = directory_conflicts(pool, &relative, old)?;
    if directory_conflict != 0 {
        return Err("addressConflict".into());
    }
    let _notes = notebook::WRITE.lock().await;
    let notebooks = notebook_repository::list(pool).await?;
    let new_key = format!("github:{}", new.to_lowercase());
    if notebooks
        .iter()
        .any(|row| row.source_key.eq_ignore_ascii_case(&new_key))
    {
        return Err("addressConflict".into());
    }
    let busy = pool.read(|d| {
        Ok(d.table("repository_mirror_snapshots")
            .values()
            .filter(|r| {
                let repo = r["repository"].as_str().unwrap_or("");
                let state = r["state"].as_str().unwrap_or("");
                (repo.eq_ignore_ascii_case(old) || repo.eq_ignore_ascii_case(new))
                    && (state == "applying"
                        || (state == "failed"
                            && d.table("repository_mirror_recovery")
                                .contains_key(r["id"].as_str().unwrap_or(""))))
            })
            .count())
    })?;
    if busy != 0 {
        return Err("addressBusy".into());
    }
    let exists = repository_count(pool, old, false)?;
    if exists == 0 {
        return Err("addressMissing".into());
    }
    let conflict = repository_count(pool, new, true)?;
    if conflict != 0 {
        return Err("addressConflict".into());
    }
    // Ensure all members use this repository container before preserving it.
    let rows = pool.read(|d| {
        Ok(d.table("skill_sources")
            .values()
            .filter(|r| {
                r["source_repo"]
                    .as_str()
                    .is_some_and(|v| v.eq_ignore_ascii_case(old))
            })
            .filter_map(|r| d.table("skills").get(r["skill_id"].as_str()?))
            .cloned()
            .collect::<Vec<_>>())
    })?;
    let mut updates = Vec::new();
    let mut links = Vec::new();
    for row in rows {
        let id: String = row["id"].as_str().ok_or("addressDatabase")?.to_owned();
        let target = PathBuf::from(row["canonical_path"].as_str().ok_or("addressLayout")?);
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
        let file = PathBuf::from(row["file_path"].as_str().ok_or("addressDatabase")?);
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
    for row in pool.read(|d| Ok(d.table("skills").values().cloned().collect::<Vec<_>>()))? {
        let id: String = row["id"].as_str().ok_or("addressDatabase")?.to_owned();
        if row["canonical_path"]
            .as_str()
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
    let payload = serde_json::to_string(&journal).map_err(|_| "addressDatabase")?;
    pool.write(|d| {
        d.table_mut("repository_address_moves").insert(
            move_id.clone(),
            serde_json::json!({"id":move_id,"payload":payload,"committed":false}),
        );
        Ok(())
    })
    .await?;
    let result: Result<(), String> = async {
        if !same_path {
            std::fs::create_dir_all(new_path.parent().ok_or("addressLayout")?)
                .map_err(|_| "addressMove")?;
            if journal.had_directory {
                std::fs::rename(&path, &new_path).map_err(|_| "addressMove")?;
            } else {
                std::fs::create_dir(&new_path).map_err(|_| "addressMove")?;
            }
        }
        db::repository::commit_address(
            pool,
            db::repository::AddressCommit {
                old,
                new,
                relative: &relative,
                move_id: &move_id,
                updates: &updates,
                notebooks,
            },
        )
        .await
    }
    .await;
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
    let rows = pool.read(|d| {
        Ok(d.table("repository_address_moves")
            .values()
            .cloned()
            .collect::<Vec<_>>())
    })?;
    let mut completed = Vec::new();
    for row in rows {
        let journal: MoveJournal =
            serde_json::from_str(row["payload"].as_str().ok_or("addressRecovery")?)
                .map_err(|_| "addressRecovery")?;
        let from = root.join(journal.from.split('/').collect::<PathBuf>());
        let to = root.join(journal.to.split('/').collect::<PathBuf>());
        repository_mirror::ensure_owned_path(&root, &from).map_err(|_| "addressRecovery")?;
        repository_mirror::ensure_owned_path(&root, &to).map_err(|_| "addressRecovery")?;
        let committed: bool = row["committed"].as_bool().ok_or("addressRecovery")?;
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
        pool.write(|d| {
            d.table_mut("repository_address_moves")
                .remove(row["id"].as_str().ok_or("addressRecovery")?);
            Ok(())
        })
        .await?;
    }
    Ok(completed)
}

fn directory_conflicts(pool: &DbPool, relative: &str, repository: &str) -> Result<usize, String> {
    pool.read(|d| {
        Ok(d.table("repository_local_directories")
            .values()
            .filter(|r| {
                r["directory"]
                    .as_str()
                    .is_some_and(|v| v.eq_ignore_ascii_case(relative))
                    && !r["repository"]
                        .as_str()
                        .is_some_and(|v| v.eq_ignore_ascii_case(repository))
            })
            .count())
    })
}
fn repository_count(pool: &DbPool, repository: &str, directories: bool) -> Result<usize, String> {
    pool.read(|d| {
        Ok([
            ("skill_sources", "source_repo"),
            ("repository_mirrors", "repository"),
            ("repository_local_directories", "repository"),
        ]
        .into_iter()
        .take(if directories { 3 } else { 2 })
        .map(|(table, key)| {
            d.table(table)
                .values()
                .filter(|r| {
                    r[key]
                        .as_str()
                        .is_some_and(|v| v.eq_ignore_ascii_case(repository))
                })
                .count()
        })
        .sum())
    })
}
