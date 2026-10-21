//! Recoverable deletion of resource-library data. Remote update removals deliberately bypass this.
use super::{github_import as github, linker, remote_sources, repository_mirror, skills};
use crate::{
    db::{self, AppStore, SkillInstallation},
    AppState,
};
use base64::Engine;
use flate2::read::GzDecoder;
#[path = "resource_trash_archive.rs"]
mod archive;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
#[cfg(test)]
use std::io::Write;
use std::{
    collections::{BTreeMap, HashSet},
    io::Read,
    path::{Component, Path, PathBuf},
};
use tauri::State;

// Only the old inline JSON format used this bound. New file archives have no such cap.
const LEGACY_JSON_LIMIT: u64 = 256 * 1024 * 1024;
const OWNED_TABLES: &[&str] = &[
    "skills",
    "skill_sources",
    "skill_source_syncs",
    "skill_metadata",
    "collection_skills",
    "skill_explanations",
];

#[derive(Clone, Serialize, Deserialize)]
struct Directory {
    path: PathBuf,
    files: BTreeMap<String, Vec<u8>>,
}
#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct Payload {
    root: PathBuf,
    directories: Vec<Directory>,
    tables: BTreeMap<String, BTreeMap<String, Value>>,
    installations: Vec<SkillInstallation>,
    #[serde(default)]
    copies: Vec<Directory>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Entry {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub repositories: Vec<String>,
    pub skill_count: usize,
    pub deleted_at: String,
    pub reason: String,
    pub phase: String,
}

#[cfg(test)]
fn encode(payload: &Payload) -> Result<String, String> {
    use flate2::{write::GzEncoder, Compression};
    let bytes = serde_json::to_vec(payload).map_err(|e| e.to_string())?;
    if bytes.len() as u64 > LEGACY_JSON_LIMIT {
        return Err("TRASH_TOO_LARGE".into());
    }
    let mut encoder = GzEncoder::new(Vec::new(), Compression::fast());
    encoder.write_all(&bytes).map_err(|e| e.to_string())?;
    Ok(base64::engine::general_purpose::STANDARD
        .encode(encoder.finish().map_err(|e| e.to_string())?))
}
fn decode(value: &Value) -> Result<Payload, String> {
    let text = value["payload"].as_str().ok_or("TRASH_UNAVAILABLE")?;
    if text.len() as u64 > LEGACY_JSON_LIMIT * 2 {
        return Err("TRASH_TOO_LARGE".into());
    }
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(text)
        .map_err(|_| "TRASH_UNAVAILABLE")?;
    let mut output = Vec::new();
    GzDecoder::new(bytes.as_slice())
        .take(LEGACY_JSON_LIMIT + 1)
        .read_to_end(&mut output)
        .map_err(|_| "TRASH_UNAVAILABLE")?;
    if output.len() as u64 > LEGACY_JSON_LIMIT {
        return Err("TRASH_TOO_LARGE".into());
    }
    serde_json::from_slice(&output).map_err(|_| "TRASH_UNAVAILABLE".into())
}
fn ids(payload: &Payload) -> HashSet<String> {
    payload
        .tables
        .get("skills")
        .into_iter()
        .flat_map(|rows| rows.keys().cloned())
        .collect()
}
fn member_key(row: &Value) -> &'static str {
    if row.get("skillIds").is_some() {
        "skillIds"
    } else {
        "skill_ids"
    }
}
fn annotation_matches(row: &Value, selected: &HashSet<String>) -> bool {
    row[member_key(row)].as_array().is_some_and(|values| {
        values
            .iter()
            .any(|value| value.as_str().is_some_and(|id| selected.contains(id)))
    })
}
pub(crate) fn cleanup_archives(pool: &AppStore) {
    archive::cleanup_unreferenced(pool);
}

pub(crate) async fn capture(
    pool: &AppStore,
    name: String,
    kind: &str,
    selected: HashSet<String>,
    paths: Vec<PathBuf>,
    reason: &str,
) -> Result<Entry, String> {
    if selected.is_empty() {
        return Err("TRASH_NOT_FOUND".into());
    }
    cleanup_archives(pool);
    let root = crate::path_utils::path_without_verbatim_prefix(
        &db::get_skill_resource_library_dir(pool).await?,
    );
    let mut directories = Vec::new();
    for path in paths {
        let path = crate::path_utils::path_without_verbatim_prefix(&path);
        repository_mirror::ensure_owned_path(&root, &path)?;
        let files = if path.exists() {
            remote_sources::local_skill_files(&path)?
        } else {
            BTreeMap::new()
        };
        directories.push(Directory { path, files });
    }
    let mut installations = Vec::new();
    for id in &selected {
        installations.extend(db::get_skill_installations(pool, id).await?);
    }
    let mut copies = Vec::new();
    for installation in &installations {
        if installation.link_type == "copy" && Path::new(&installation.installed_path).exists() {
            let files = remote_sources::local_skill_files(Path::new(&installation.installed_path))?;
            copies.push(Directory {
                path: installation.installed_path.clone().into(),
                files,
            });
        }
    }
    let tables: BTreeMap<String, BTreeMap<String, Value>> = pool.read(|data| {
        let mut tables = BTreeMap::new();
        for &table in OWNED_TABLES {
            let rows = data
                .table(table)
                .iter()
                .filter(|(key, row)| {
                    if table == "skills" {
                        selected.contains(*key)
                    } else {
                        row["skill_id"]
                            .as_str()
                            .is_some_and(|id| selected.contains(id))
                    }
                })
                .map(|(key, row)| (key.clone(), row.clone()))
                .collect();
            tables.insert(table.to_owned(), rows);
        }
        for table in ["folder_notes", "repository_notebooks"] {
            tables.insert(
                table.into(),
                data.table(table)
                    .iter()
                    .filter(|(_, row)| annotation_matches(row, &selected))
                    .map(|(key, row)| (key.clone(), row.clone()))
                    .collect(),
            );
        }
        Ok(tables)
    })?;
    let repositories: HashSet<String> = tables
        .get("skill_sources")
        .into_iter()
        .flat_map(|rows| rows.values())
        .filter_map(|row| row["source_repo"].as_str().map(str::to_lowercase))
        .collect();
    let mut repositories: Vec<_> = repositories.into_iter().collect();
    repositories.sort();
    let payload = Payload {
        root,
        directories,
        tables,
        installations,
        copies,
    };
    let entry = Entry {
        id: uuid::Uuid::new_v4().to_string(),
        name,
        kind: kind.into(),
        repositories,
        skill_count: selected.len(),
        deleted_at: chrono::Utc::now().to_rfc3339(),
        reason: reason.into(),
        phase: "deleting".into(),
    };
    archive::write(pool, &entry.id, &payload)?;
    if let Err(error) = pool
        .write(|data| {
            data.table_mut("resource_trash")
                .insert(entry.id.clone(), json!({"entry":entry,"archiveVersion":2}));
            Ok(())
        })
        .await
    {
        archive::remove(pool, &entry.id);
        return Err(error);
    }
    Ok(entry)
}
fn exclusions(
    data: &mut crate::storage::store::Data,
    payload: &Payload,
    id: &str,
    exclude: bool,
) -> Result<(), String> {
    let mut sources: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for source in payload
        .tables
        .get("skill_sources")
        .into_iter()
        .flat_map(|rows| rows.values())
    {
        if let (Some(repo), Some(path)) = (
            source["source_repo"].as_str(),
            source["source_path"].as_str(),
        ) {
            sources
                .entry(repo.to_lowercase())
                .or_default()
                .push(path.into());
        }
    }
    for (repository, paths) in sources {
        let row = data
            .table_mut("repository_mirrors")
            .entry(repository.clone())
            .or_insert_with(|| json!({"repository":repository,"last_snapshot_id":null}));
        let mut rules =
            if let Some(value) = row.get("import_rules").filter(|value| !value.is_null()) {
                serde_json::from_value::<super::repository_import_rules::ImportRules>(value.clone())
                    .map_err(|_| "TRASH_UNAVAILABLE")?
            } else {
                super::repository_import_rules::ImportRules {
                    legacy_passthrough: true,
                    ..Default::default()
                }
            };
        for path in paths {
            if exclude {
                rules.excluded_paths.insert(path, id.into());
            } else if rules
                .excluded_paths
                .get(&path)
                .is_some_and(|entry| entry == id)
            {
                rules.excluded_paths.remove(&path);
            }
        }
        if rules.legacy_passthrough && rules.excluded_paths.is_empty() {
            row.as_object_mut()
                .ok_or("TRASH_UNAVAILABLE")?
                .remove("import_rules");
        } else {
            row["import_rules"] = serde_json::to_value(rules).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}
async fn phase(pool: &AppStore, id: &str, value: &str) -> Result<(), String> {
    let (entry, payload) = read_metadata(pool, id)?;
    pool.write(|data| {
        if value == "active" && entry.reason == "delete" {
            exclusions(data, &payload, id, true)?;
            let removed = ids(&payload);
            for table in ["folder_notes", "repository_notebooks"] {
                let keys: Vec<_> = data
                    .table(table)
                    .iter()
                    .filter(|(_, row)| annotation_matches(row, &removed))
                    .map(|(key, _)| key.clone())
                    .collect();
                for key in keys {
                    if let Some(row) = data.table_mut(table).get_mut(&key) {
                        let field = member_key(row);
                        let remaining: Vec<_> = row[field]
                            .as_array()
                            .into_iter()
                            .flatten()
                            .filter(|value| !value.as_str().is_some_and(|id| removed.contains(id)))
                            .cloned()
                            .collect();
                        if remaining.is_empty() {
                            data.table_mut(table).remove(&key);
                        } else {
                            data.table_mut(table)
                                .get_mut(&key)
                                .ok_or("TRASH_UNAVAILABLE")?[field] = json!(remaining);
                        }
                    }
                }
            }
        }
        let row = data
            .table_mut("resource_trash")
            .get_mut(id)
            .ok_or("TRASH_NOT_FOUND")?;
        row["entry"]["phase"] = value.into();
        Ok(())
    })
    .await
}
fn read(pool: &AppStore, id: &str) -> Result<(Entry, Payload), String> {
    read_payload(pool, id, true)
}
fn read_metadata(pool: &AppStore, id: &str) -> Result<(Entry, Payload), String> {
    read_payload(pool, id, false)
}
fn read_payload(
    pool: &AppStore,
    id: &str,
    include_files: bool,
) -> Result<(Entry, Payload), String> {
    let row = pool.read(|data| {
        data.table("resource_trash")
            .get(id)
            .cloned()
            .ok_or_else(|| "TRASH_NOT_FOUND".to_owned())
    })?;
    let entry = serde_json::from_value(row["entry"].clone()).map_err(|_| "TRASH_UNAVAILABLE")?;
    let payload = if row["archiveVersion"].as_u64() == Some(2) {
        archive::read(pool, id, include_files)?
    } else {
        decode(&row)?
    };
    Ok((entry, payload))
}
pub(crate) fn list(pool: &AppStore) -> Result<Vec<Entry>, String> {
    pool.read(|data| {
        let mut entries = data
            .table("resource_trash")
            .values()
            .map(|row| {
                serde_json::from_value::<Entry>(row["entry"].clone())
                    .map_err(|_| "TRASH_UNAVAILABLE".to_owned())
            })
            .collect::<Result<Vec<_>, _>>()?;
        entries.sort_by(|a, b| b.deleted_at.cmp(&a.deleted_at));
        Ok(entries)
    })
}
pub(crate) async fn trash_skill(
    pool: &AppStore,
    id: &str,
    options: skills::DeleteResourceSkillOptions,
) -> Result<skills::DeleteResourceSkillResult, String> {
    let _guard = repository_mirror::MIRROR_LOCK.lock().await;
    github::recover_import_transactions(pool).await?;
    let skill = db::get_skill_by_id(pool, id)
        .await?
        .filter(|skill| !skill.is_central)
        .ok_or("TRASH_NOT_FOUND")?;
    if !options.cascade_uninstall && !db::get_skill_installations(pool, id).await?.is_empty() {
        return Err("TRASH_INSTALLED".into());
    }
    let path = Path::new(&skill.file_path)
        .parent()
        .ok_or("UNSAFE_TARGET")?
        .to_owned();
    let entry = capture(
        pool,
        skill.name,
        "skill",
        HashSet::from([id.into()]),
        vec![path],
        "delete",
    )
    .await?;
    match skills::delete_resource_skill_impl(pool, id, options).await {
        Ok(result) => {
            phase(pool, &entry.id, "active").await?;
            Ok(result)
        }
        Err(error) => {
            restore_locked(pool, &entry.id, true)
                .await
                .map_err(|_| "TRASH_RECOVERY_REQUIRED")?;
            Err(error)
        }
    }
}
pub(crate) async fn trash_bundle(
    pool: &AppStore,
    relative: &str,
    options: skills::DeleteCentralSkillBundleOptions,
) -> Result<skills::DeleteCentralSkillBundleResult, String> {
    let _guard = repository_mirror::MIRROR_LOCK.lock().await;
    github::recover_import_transactions(pool).await?;
    let preview = skills::preview_delete_resource_skill_bundle_impl(pool, relative).await?;
    if !options.cascade_uninstall && !preview.affected_agents.is_empty() {
        return Err("TRASH_INSTALLED".into());
    }
    let entry = capture(
        pool,
        preview.bundle.name,
        "repository",
        preview
            .skills
            .iter()
            .map(|skill| skill.id.clone())
            .collect(),
        vec![PathBuf::from(&preview.bundle.path)],
        "delete",
    )
    .await?;
    match skills::delete_resource_skill_bundle_impl(pool, relative, options).await {
        Ok(result) => {
            phase(pool, &entry.id, "active").await?;
            Ok(result)
        }
        Err(error) => {
            restore_locked(pool, &entry.id, true)
                .await
                .map_err(|_| "TRASH_RECOVERY_REQUIRED")?;
            Err(error)
        }
    }
}
fn merge_annotation(current: Option<&Value>, saved: &Value) -> Value {
    let mut row = current.unwrap_or(saved).clone();
    let mut members: HashSet<String> = row[member_key(&row)]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|value| value.as_str().map(str::to_owned))
        .collect();
    members.extend(
        saved[member_key(saved)]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|value| value.as_str().map(str::to_owned)),
    );
    let mut members: Vec<_> = members.into_iter().collect();
    members.sort();
    let key = member_key(&row);
    row[key] = json!(members);
    row
}
fn write_files(directory: &Path, files: &BTreeMap<String, Vec<u8>>) -> Result<(), String> {
    std::fs::create_dir_all(directory).map_err(|e| e.to_string())?;
    for (relative, bytes) in files {
        if relative.contains('\\')
            || relative.is_empty()
            || !Path::new(relative)
                .components()
                .all(|part| matches!(part, Component::Normal(_)))
            || relative
                .split('/')
                .any(|part| part.contains(':') || part.ends_with(['.', ' ']))
        {
            return Err("UNSAFE_SNAPSHOT_PATH".into());
        }
        let path = directory.join(relative);
        std::fs::create_dir_all(path.parent().ok_or("UNSAFE_TARGET")?)
            .map_err(|e| e.to_string())?;
        if relative.ends_with('/') {
            if !bytes.is_empty() {
                return Err("UNSAFE_SNAPSHOT_PATH".into());
            }
            std::fs::create_dir_all(path).map_err(|e| e.to_string())?;
        } else {
            std::fs::write(path, bytes).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}
pub(crate) async fn restore_locked(
    pool: &AppStore,
    id: &str,
    rollback: bool,
) -> Result<(), String> {
    let (entry, payload) = read(pool, id)?;
    let root = crate::path_utils::path_without_verbatim_prefix(
        &db::get_skill_resource_library_dir(pool).await?,
    );
    if !crate::path_utils::paths_resolve_to_same_entry(&root, &payload.root) {
        return Err("TRASH_ROOT_CHANGED".into());
    }
    let root = payload.root.clone();
    let selected = ids(&payload);
    let retry = rollback || entry.phase == "restoring" || entry.phase == "deleting";
    let current = db::get_resource_library_skills(pool).await?;
    for skill in &current {
        if selected.contains(&skill.id) && !retry {
            return Err("TRASH_RESTORE_CONFLICT".into());
        }
        if payload.directories.iter().any(|directory| {
            crate::path_utils::path_without_verbatim_prefix(Path::new(&skill.file_path))
                .starts_with(&directory.path)
        }) && (!retry || !selected.contains(&skill.id))
        {
            return Err("TRASH_RESTORE_CONFLICT".into());
        }
    }
    for directory in &payload.directories {
        repository_mirror::ensure_owned_path(&root, &directory.path)?;
        if directory.path.exists() && !retry {
            return Err("TRASH_RESTORE_CONFLICT".into());
        }
        if directory.path.exists()
            && !rollback
            && remote_sources::local_skill_files(&directory.path)? != directory.files
        {
            return Err("TRASH_RESTORE_CONFLICT".into());
        }
    }
    phase(pool, id, "restoring").await?;
    let mut files = github::ImportDirectoryTransaction::new(&root)?;
    for (index, directory) in payload.directories.iter().enumerate() {
        let staged = files.root.join(format!("restore-{index}"));
        write_files(&staged, &directory.files)?;
        files.install(&staged, &directory.path, retry)?;
    }
    let mut transaction = pool.begin().await?;
    for (table, rows) in &payload.tables {
        for (key, row) in rows {
            if table == "collection_skills"
                && !transaction
                    .data_mut()
                    .table("collections")
                    .contains_key(row["collection_id"].as_str().unwrap_or(""))
            {
                continue;
            }
            let value = if table == "folder_notes" || table == "repository_notebooks" {
                merge_annotation(transaction.data_mut().table(table).get(key), row)
            } else {
                row.clone()
            };
            transaction
                .data_mut()
                .table_mut(table)
                .insert(key.clone(), value);
        }
    }
    transaction
        .data_mut()
        .table_mut("import_transaction_receipts")
        .insert(
            files.root.to_string_lossy().into_owned(),
            json!({"id":files.root.to_string_lossy()}),
        );
    transaction.commit().await?;
    files.commit()?;
    for installation in &payload.installations {
        linker::install_resource_skill_to_agent_impl(
            pool,
            &installation.skill_id,
            &installation.agent_id,
            &installation.link_type,
        )
        .await?;
        if let Some(copy) = payload
            .copies
            .iter()
            .find(|copy| copy.path == Path::new(&installation.installed_path))
        {
            let installed = db::get_skill_installations(pool, &installation.skill_id)
                .await?
                .into_iter()
                .find(|current| {
                    current.agent_id == installation.agent_id && current.link_type == "copy"
                })
                .ok_or("TRASH_RECOVERY_REQUIRED")?;
            let target = Path::new(&installed.installed_path);
            let mut transaction = github::ImportDirectoryTransaction::new(target)?;
            let staged = transaction.root.join("copy");
            write_files(&staged, &copy.files)?;
            transaction.install(&staged, target, true)?;
            pool.write(|data| {
                data.table_mut("import_transaction_receipts").insert(
                    transaction.root.to_string_lossy().into_owned(),
                    json!({"id":transaction.root.to_string_lossy()}),
                );
                Ok(())
            })
            .await?;
            transaction.commit()?;
        }
    }
    pool.write(|data| {
        exclusions(data, &payload, id, false)?;
        data.table_mut("resource_trash").remove(id);
        Ok(())
    })
    .await?;
    archive::remove(pool, id);
    Ok(())
}

fn directory_relative(root: &Path, entry: &Entry, pool: &AppStore) -> Result<String, String> {
    let (_, payload) = read_metadata(pool, &entry.id)?;
    let path = &payload.directories.first().ok_or("UNSAFE_TARGET")?.path;
    Ok(path
        .strip_prefix(root)
        .map_err(|_| "UNSAFE_TARGET")?
        .to_string_lossy()
        .replace('\\', "/"))
}
pub(crate) async fn reimport_prepared(
    pool: &AppStore,
    repo: github::GitHubRepoRef,
    snapshot: &github::GitHubRepoSnapshot,
    selections: Vec<github::GitHubSkillImportSelection>,
    app: Option<&tauri::AppHandle>,
) -> Result<github::GitHubRepoImportResult, String> {
    // Reimport creates fresh records. An overwrite must not target an old record
    // removed below, including requests persisted by older clients.
    let selections: Vec<_> = selections
        .into_iter()
        .map(|mut selection| {
            if selection.resolution != github::DuplicateResolution::Rename {
                selection.renamed_skill_id = None;
            }
            selection
        })
        .collect();
    // Validate the entire incoming selection before the old repository is touched.
    let candidates = super::repository_import_rules::candidates(&repo, snapshot)?;
    if selections.is_empty()
        || !selections
            .iter()
            .any(|item| item.resolution != github::DuplicateResolution::Skip)
        || selections.iter().any(|item| {
            !candidates
                .iter()
                .any(|candidate| candidate.source_path == item.source_path)
        })
    {
        return Err("PREVIEW_EXPIRED".into());
    }
    let repository = format!("{}/{}", repo.owner, repo.repo);
    let sources = remote_sources::github_repo_source_groups(pool, false)
        .await?
        .remove(&repository.to_lowercase())
        .ok_or("TRASH_NOT_FOUND")?;
    let root = crate::path_utils::path_without_verbatim_prefix(
        &db::get_skill_resource_library_dir(pool).await?,
    );
    let directory =
        super::repository_address::directory(pool, &root, &repo.owner, &repository).await?;
    // Every managed member must live inside the one directory being replaced.
    if sources.iter().any(|(_, skill)| {
        !crate::path_utils::path_without_verbatim_prefix(Path::new(&skill.file_path))
            .starts_with(&directory)
    }) {
        return Err("UNSAFE_TARGET".into());
    }
    let selected = sources.iter().map(|(_, skill)| skill.id.clone()).collect();
    let entry = capture(
        pool,
        repository,
        "repository",
        selected,
        vec![directory],
        "reimport",
    )
    .await?;
    let relative = directory_relative(&root, &entry, pool)?;
    let mut removed = false;
    let outcome=async {
        skills::delete_resource_skill_bundle_impl(pool,&relative,skills::DeleteCentralSkillBundleOptions{cascade_uninstall:true}).await?;
        removed=true;
        let result=github::import_prepared_snapshot(pool,repo,snapshot,selections,app,None).await?;
        let (_,payload)=read_metadata(pool,&entry.id)?;
        let new_ids:Vec<_>=result.imported_skills.iter().map(|skill|skill.imported_skill_id.clone()).collect();
        let old_ids=ids(&payload);
        pool.write(|data|{
            for table in ["folder_notes","repository_notebooks"] {
                for (key,saved) in payload.tables.get(table).into_iter().flatten(){
                    let mut row=data.table(table).get(key).unwrap_or(saved).clone();
                    let mut members:HashSet<String>=row[member_key(&row)].as_array().into_iter().flatten().filter_map(|value|value.as_str().filter(|id|!old_ids.contains(*id)).map(str::to_owned)).collect();
                    members.extend(new_ids.iter().cloned());let mut members:Vec<_>=members.into_iter().collect();members.sort();let field=member_key(&row);row[field]=json!(members);
                    data.table_mut(table).insert(key.clone(),row);
                }
            }
            let groups:HashSet<_>=payload.tables.get("collection_skills").into_iter().flat_map(|rows|rows.values()).filter_map(|row|row["collection_id"].as_str()).collect();
            for group in groups {if data.table("collections").contains_key(group){for id in &new_ids{data.table_mut("collection_skills").insert(db::key2(group,id),json!({"collection_id":group,"skill_id":id,"added_at":chrono::Utc::now().to_rfc3339()}));}}}
            Ok(())
        }).await?;
        let mut linked=HashSet::new();
        for installation in &payload.installations {
            let original=payload.tables.get("skills").and_then(|rows|rows.get(&installation.skill_id));
            let source=payload.tables.get("skill_sources").and_then(|rows|rows.values().find(|row|row["skill_id"]==installation.skill_id));
            let target=result.imported_skills.iter().find(|skill|source.and_then(|row|row["source_path"].as_str())==Some(skill.source_path.as_str()))
                .or_else(||result.imported_skills.iter().find(|skill|original.and_then(|row|row["name"].as_str()).is_some_and(|name|name.eq_ignore_ascii_case(&skill.skill_name))));
            if let Some(target)=target {if linked.insert((target.imported_skill_id.clone(),installation.agent_id.clone())){linker::install_resource_skill_to_agent_impl(pool,&target.imported_skill_id,&installation.agent_id,&installation.link_type).await?;}}
        }
        phase(pool,&entry.id,"active").await?;
        Ok(result)
    }.await;
    if outcome.is_err() {
        if removed {
            if skills::preview_delete_resource_skill_bundle_impl(pool, &relative)
                .await
                .is_ok()
            {
                skills::delete_resource_skill_bundle_impl(
                    pool,
                    &relative,
                    skills::DeleteCentralSkillBundleOptions {
                        cascade_uninstall: true,
                    },
                )
                .await
                .map_err(|_| "TRASH_RECOVERY_REQUIRED")?;
            }
        }
        restore_locked(pool, &entry.id, true)
            .await
            .map_err(|_| "TRASH_RECOVERY_REQUIRED")?;
    }
    outcome
}

#[tauri::command]
pub async fn list_resource_trash(state: State<'_, AppState>) -> Result<Vec<Entry>, String> {
    list(&state.db)
}
#[tauri::command]
pub async fn restore_resource_trash(state: State<'_, AppState>, id: String) -> Result<(), String> {
    let _guard = repository_mirror::MIRROR_LOCK.lock().await;
    github::recover_import_transactions(&state.db).await?;
    restore_locked(&state.db, &id, false).await
}
#[tauri::command]
pub async fn purge_resource_trash(
    state: State<'_, AppState>,
    ids: Vec<String>,
) -> Result<(), String> {
    let _guard = repository_mirror::MIRROR_LOCK.lock().await;
    purge(&state.db, ids).await
}
pub(crate) async fn purge(pool: &AppStore, ids: Vec<String>) -> Result<(), String> {
    pool.write(|data| {
        for id in &ids {
            data.table_mut("resource_trash").remove(id);
        }
        Ok(())
    })
    .await?;
    for id in &ids {
        archive::remove(pool, id);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    async fn fixture() -> (
        tempfile::TempDir,
        AppStore,
        github::GitHubRepoRef,
        github::GitHubRepoSnapshot,
    ) {
        let temp = tempfile::tempdir().unwrap();
        let pool = AppStore::open(&temp.path().join("state")).unwrap();
        db::init_database(&pool).await.unwrap();
        db::set_skill_resource_library_dir(&pool, &temp.path().join("library").to_string_lossy())
            .await
            .unwrap();
        let repo = github::GitHubRepoRef {
            owner: "example".into(),
            repo: "tools".into(),
            branch: "a".repeat(40),
            normalized_url: "https://github.com/example/tools".into(),
            stars: None,
        };
        let mut snapshot = github::GitHubRepoSnapshot::default();
        for path in [
            "skills/shared/SKILL.md",
            "plugins/demo/skills/shared/SKILL.md",
        ] {
            snapshot.files.insert(
                path.into(),
                b"---\nname: shared\ndescription: demo\n---\nOriginal".to_vec(),
            );
        }
        let candidates =
            github::build_repo_skill_candidates_from_snapshot(&repo, &snapshot).unwrap();
        let selections = candidates
            .iter()
            .map(|candidate| github::GitHubSkillImportSelection {
                source_path: candidate.source_path.clone(),
                resolution: github::DuplicateResolution::Overwrite,
                renamed_skill_id: None,
            })
            .collect();
        github::import_prepared_snapshot_with_directories(
            &pool,
            repo.clone(),
            &snapshot,
            selections,
            None,
            None,
            &Default::default(),
            None,
        )
        .await
        .unwrap();
        let skill_ids: Vec<_> = db::get_resource_library_skills(&pool)
            .await
            .unwrap()
            .iter()
            .map(|skill| skill.id.clone())
            .collect();
        pool.write(|data|{
            data.table_mut("folder_notes").insert("remark".into(),json!({"id":"remark","notes":"preserved remark","skillIds":skill_ids}));
            data.table_mut("repository_notebooks").insert("1".into(),json!({"id":1,"sourceKey":"github:example/tools","repositoryUid":"stable-repository","fullName":"example/tools","url":"","description":"","tags":[],"stars":null,"skillIds":skill_ids,"notebook":"preserved notebook","obsidian":null}));Ok(())
        }).await.unwrap();
        (temp, pool, repo, snapshot)
    }
    #[tokio::test]
    async fn trash_and_restore_keep_files_annotations_and_block_upstream_resurrection() {
        let (_temp, pool, repo, snapshot) = fixture().await;
        let original = db::get_resource_library_skills(&pool)
            .await
            .unwrap()
            .remove(0);
        let parent = Path::new(&original.file_path).parent().unwrap();
        std::fs::create_dir_all(parent.join("empty")).unwrap();
        trash_skill(
            &pool,
            &original.id,
            skills::DeleteResourceSkillOptions {
                cascade_uninstall: true,
            },
        )
        .await
        .unwrap();
        assert!(!Path::new(&original.file_path).exists());
        assert_eq!(
            db::get_resource_library_skills(&pool).await.unwrap().len(),
            1
        );
        let entry = list(&pool).unwrap().remove(0);
        assert_eq!(entry.phase, "active");
        let rules = super::super::repository_import_rules::load(&pool, "example/tools")
            .await
            .unwrap();
        assert_eq!(
            super::super::repository_import_rules::update_candidates(
                &repo,
                &snapshot,
                rules.as_ref()
            )
            .unwrap()
            .len(),
            1
        );
        restore_locked(&pool, &entry.id, false).await.unwrap();
        assert_eq!(
            db::get_resource_library_skills(&pool).await.unwrap().len(),
            2
        );
        assert!(Path::new(&original.file_path).exists());
        assert!(parent.join("empty").is_dir());
        assert!(list(&pool).unwrap().is_empty());
        assert_eq!(
            db::metadata::folder_notes(&pool).await.unwrap()[0].notes,
            "preserved remark"
        );
        assert_eq!(
            db::notebooks::list(&pool).await.unwrap()[0].notebook,
            "preserved notebook"
        );
        assert!(
            super::super::repository_import_rules::load(&pool, "example/tools")
                .await
                .unwrap()
                .is_none()
        );
    }
    #[tokio::test]
    async fn reimport_replaces_duplicates_and_rebinds_notes_and_groups() {
        let (_temp, pool, repo, snapshot) = fixture().await;
        let original = db::get_resource_library_skills(&pool).await.unwrap();
        let group = db::create_collection(&pool, "Group", None).await.unwrap();
        db::add_skill_to_collection(&pool, &group.id, &original[0].id)
            .await
            .unwrap();
        let candidates =
            super::super::repository_import_rules::candidates(&repo, &snapshot).unwrap();
        let selections = candidates
            .iter()
            .map(|candidate| github::GitHubSkillImportSelection {
                source_path: candidate.source_path.clone(),
                resolution: github::DuplicateResolution::Overwrite,
                renamed_skill_id: None,
            })
            .collect();
        let result = reimport_prepared(&pool, repo, &snapshot, selections, None)
            .await
            .unwrap();
        assert_eq!(result.imported_skills.len(), 1);
        assert_eq!(
            db::get_resource_library_skills(&pool).await.unwrap().len(),
            1
        );
        let id = &result.imported_skills[0].imported_skill_id;
        let remarks = db::metadata::folder_notes(&pool).await.unwrap();
        assert_eq!(remarks[0].notes, "preserved remark");
        assert_eq!(remarks[0].skill_ids, vec![id.clone()]);
        let notes = db::notebooks::list(&pool).await.unwrap();
        assert_eq!(notes[0].notebook, "preserved notebook");
        assert_eq!(notes[0].repository_uid, "stable-repository");
        assert_eq!(notes[0].skill_ids, vec![id.clone()]);
        assert!(pool
            .read(|data| Ok(data
                .table("collection_skills")
                .contains_key(&db::key2(&group.id, id))))
            .unwrap());
        let entry = list(&pool).unwrap().remove(0);
        assert_eq!(entry.reason, "reimport");
        assert_eq!(entry.skill_count, 2);
        assert_eq!(
            restore_locked(&pool, &entry.id, false).await.unwrap_err(),
            "TRASH_RESTORE_CONFLICT"
        );
    }
    #[tokio::test]
    async fn reimport_selected_variant_does_not_treat_preview_id_as_deleted_update_target() {
        let (_temp, pool, repo, mut snapshot) = fixture().await;
        snapshot.files.insert(
            "plugins/demo/skills/shared/SKILL.md".into(),
            b"---\nname: shared\ndescription: variant\n---\nDifferent variant".to_vec(),
        );
        let candidates =
            super::super::repository_import_rules::candidates(&repo, &snapshot).unwrap();
        assert_eq!(candidates.len(), 2);
        let chosen = &candidates[0];
        // Older clients sent the default rename input even for Overwrite.
        let selections = vec![github::GitHubSkillImportSelection {
            source_path: chosen.source_path.clone(),
            resolution: github::DuplicateResolution::Overwrite,
            renamed_skill_id: Some(chosen.skill_id.clone()),
        }];
        let result = reimport_prepared(&pool, repo.clone(), &snapshot, selections, None)
            .await
            .unwrap();
        assert_eq!(result.imported_skills.len(), 1);
        assert_eq!(
            db::get_resource_library_skills(&pool).await.unwrap().len(),
            1
        );
        let rules = super::super::repository_import_rules::load(&pool, "example/tools")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(rules.groups["shared"], vec![chosen.source_path.clone()]);
        let next = super::super::repository_import_rules::update_candidates(
            &repo,
            &snapshot,
            Some(&rules),
        )
        .unwrap();
        assert_eq!(next.len(), 1);
        assert_eq!(next[0].source_path, chosen.source_path);
        assert_eq!(
            db::notebooks::list(&pool).await.unwrap()[0].notebook,
            "preserved notebook"
        );
    }
    #[tokio::test]
    async fn failed_reimport_restores_original_repository_and_annotations() {
        let (_temp, pool, repo, snapshot) = fixture().await;
        let original = db::get_resource_library_skills(&pool).await.unwrap();
        let candidates =
            super::super::repository_import_rules::candidates(&repo, &snapshot).unwrap();
        let selections = candidates
            .iter()
            .map(|candidate| github::GitHubSkillImportSelection {
                source_path: candidate.source_path.clone(),
                resolution: github::DuplicateResolution::Overwrite,
                renamed_skill_id: None,
            })
            .collect();
        pool.fail_table_write("import_transaction_receipts");
        assert!(reimport_prepared(&pool, repo, &snapshot, selections, None)
            .await
            .is_err());
        assert_eq!(
            db::get_resource_library_skills(&pool).await.unwrap().len(),
            2
        );
        for skill in original {
            assert!(Path::new(&skill.file_path).exists());
        }
        assert!(list(&pool).unwrap().is_empty());
        assert_eq!(
            db::metadata::folder_notes(&pool).await.unwrap()[0].notes,
            "preserved remark"
        );
    }
    #[tokio::test]
    async fn repository_restore_preserves_copy_installation_edits_and_refuses_foreign_content() {
        let (temp, pool, repo, _snapshot) = fixture().await;
        crate::storage::fixtures::disable_except(&pool, &["central", "claude-code"]).await;
        pool.write(|data| {
            data.table_mut("agents").get_mut("claude-code").unwrap()["global_skills_dir"] =
                json!(temp.path().join("platform").to_string_lossy());
            data.table_mut("agents").get_mut("central").unwrap()["global_skills_dir"] =
                json!(temp.path().join("central").to_string_lossy());
            Ok(())
        })
        .await
        .unwrap();
        let skill = db::get_resource_library_skills(&pool)
            .await
            .unwrap()
            .remove(0);
        linker::install_resource_skill_to_agent_impl(&pool, &skill.id, "claude-code", "copy")
            .await
            .unwrap();
        let installed = db::get_skill_installations(&pool, &skill.id)
            .await
            .unwrap()
            .remove(0);
        std::fs::write(
            Path::new(&installed.installed_path).join("custom.txt"),
            "copy edit",
        )
        .unwrap();
        let root = db::get_skill_resource_library_dir(&pool).await.unwrap();
        let directory =
            super::super::repository_address::directory(&pool, &root, &repo.owner, "example/tools")
                .await
                .unwrap();
        let relative = directory
            .strip_prefix(&root)
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/");
        trash_bundle(
            &pool,
            &relative,
            skills::DeleteCentralSkillBundleOptions {
                cascade_uninstall: true,
            },
        )
        .await
        .unwrap();
        assert!(db::get_resource_library_skills(&pool)
            .await
            .unwrap()
            .is_empty());
        assert!(db::metadata::folder_notes(&pool).await.unwrap().is_empty());
        assert!(db::notebooks::list(&pool).await.unwrap().is_empty());
        let entry = list(&pool).unwrap().remove(0);
        std::fs::create_dir_all(&directory).unwrap();
        std::fs::write(directory.join("foreign.txt"), "keep").unwrap();
        assert_eq!(
            restore_locked(&pool, &entry.id, false).await.unwrap_err(),
            "TRASH_RESTORE_CONFLICT"
        );
        assert_eq!(
            std::fs::read_to_string(directory.join("foreign.txt")).unwrap(),
            "keep"
        );
        std::fs::remove_file(directory.join("foreign.txt")).unwrap();
        std::fs::remove_dir(&directory).unwrap();
        restore_locked(&pool, &entry.id, false).await.unwrap();
        assert_eq!(
            std::fs::read_to_string(Path::new(&installed.installed_path).join("custom.txt"))
                .unwrap(),
            "copy edit"
        );
        assert_eq!(
            db::get_resource_library_skills(&pool).await.unwrap().len(),
            2
        );
    }
    #[tokio::test]
    async fn repository_archive_does_not_expand_binary_files_into_json_number_arrays() {
        let (_temp, pool, repo, _snapshot) = fixture().await;
        let root = db::get_skill_resource_library_dir(&pool).await.unwrap();
        let directory =
            super::super::repository_address::directory(&pool, &root, &repo.owner, "example/tools")
                .await
                .unwrap();
        let file = directory.join("large-resource.bin");
        let mut output = std::fs::File::create(&file).unwrap();
        let chunk = vec![122u8; 1024 * 1024];
        for _ in 0..73 {
            output.write_all(&chunk).unwrap();
        }
        drop(output);
        let relative = directory
            .strip_prefix(&root)
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/");
        trash_bundle(
            &pool,
            &relative,
            skills::DeleteCentralSkillBundleOptions {
                cascade_uninstall: true,
            },
        )
        .await
        .unwrap();
        let entry = list(&pool).unwrap().remove(0);
        assert!(archive::path(&pool, &entry.id).unwrap().exists());
        assert!(pool
            .read(|data| Ok(data.table("resource_trash")[&entry.id]
                .get("payload")
                .is_none()))
            .unwrap());
        restore_locked(&pool, &entry.id, false).await.unwrap();
        assert!(!archive::path(&pool, &entry.id).unwrap().exists());
        assert_eq!(std::fs::metadata(&file).unwrap().len(), 73 * 1024 * 1024);
        let mut restored = std::fs::File::open(&file).unwrap();
        let mut buffer = vec![0; 1024 * 1024];
        for _ in 0..73 {
            restored.read_exact(&mut buffer).unwrap();
            assert!(buffer.iter().all(|byte| *byte == 122));
        }
    }
    #[tokio::test]
    async fn archive_cleanup_preserves_live_records_and_only_discards_owned_orphans() {
        let (_temp, pool, _repo, _snapshot) = fixture().await;
        let skill = db::get_resource_library_skills(&pool)
            .await
            .unwrap()
            .remove(0);
        trash_skill(
            &pool,
            &skill.id,
            skills::DeleteResourceSkillOptions {
                cascade_uninstall: true,
            },
        )
        .await
        .unwrap();
        let entry = list(&pool).unwrap().remove(0);
        let live = archive::path(&pool, &entry.id).unwrap();
        let orphan = archive::path(&pool, &uuid::Uuid::new_v4().to_string()).unwrap();
        std::fs::write(&orphan, b"orphan").unwrap();
        let other = live.parent().unwrap().join("personal.zip");
        std::fs::write(&other, b"keep").unwrap();
        cleanup_archives(&pool);
        assert!(live.exists());
        assert!(!orphan.exists());
        assert!(other.exists());
        restore_locked(&pool, &entry.id, false).await.unwrap();
    }
    #[tokio::test]
    async fn legacy_inline_archive_remains_restorable() {
        let (_temp, pool, _repo, _snapshot) = fixture().await;
        let skill = db::get_resource_library_skills(&pool)
            .await
            .unwrap()
            .remove(0);
        trash_skill(
            &pool,
            &skill.id,
            skills::DeleteResourceSkillOptions {
                cascade_uninstall: true,
            },
        )
        .await
        .unwrap();
        let entry = list(&pool).unwrap().remove(0);
        let (_, payload) = read(&pool, &entry.id).unwrap();
        let encoded = encode(&payload).unwrap();
        pool.write(|data| {
            data.table_mut("resource_trash")
                .insert(entry.id.clone(), json!({"entry":entry,"payload":encoded}));
            Ok(())
        })
        .await
        .unwrap();
        archive::remove(&pool, &entry.id);
        restore_locked(&pool, &entry.id, false).await.unwrap();
        assert!(Path::new(&skill.file_path).exists());
    }
    #[tokio::test]
    async fn archive_write_failure_leaves_original_files_and_records_untouched() {
        let (_temp, pool, _repo, _snapshot) = fixture().await;
        let skills = db::get_resource_library_skills(&pool).await.unwrap();
        std::fs::write(pool.directory().join("trash"), b"not a directory").unwrap();
        assert!(trash_skill(
            &pool,
            &skills[0].id,
            skills::DeleteResourceSkillOptions {
                cascade_uninstall: true
            }
        )
        .await
        .is_err());
        assert_eq!(
            db::get_resource_library_skills(&pool).await.unwrap().len(),
            skills.len()
        );
        assert!(skills
            .iter()
            .all(|skill| Path::new(&skill.file_path).exists()));
        assert!(list(&pool).unwrap().is_empty());
    }
    #[tokio::test]
    async fn trash_survives_store_restart_without_exposing_payload_in_the_listing() {
        let (_temp, pool, _repo, _snapshot) = fixture().await;
        let skill = db::get_resource_library_skills(&pool)
            .await
            .unwrap()
            .remove(0);
        trash_skill(
            &pool,
            &skill.id,
            skills::DeleteResourceSkillOptions {
                cascade_uninstall: true,
            },
        )
        .await
        .unwrap();
        let directory = pool.directory().to_path_buf();
        drop(pool);
        let pool = AppStore::open(&directory).unwrap();
        let entries = list(&pool).unwrap();
        assert_eq!(entries.len(), 1);
        let public = serde_json::to_value(&entries[0]).unwrap();
        assert!(public.get("payload").is_none());
        assert!(public.get("directories").is_none());
        restore_locked(&pool, &entries[0].id, false).await.unwrap();
        assert!(Path::new(&skill.file_path).exists());
    }
    #[tokio::test]
    async fn permanent_delete_removes_only_the_archive_and_keeps_upstream_exclusion() {
        let (_temp, pool, repo, snapshot) = fixture().await;
        let skill = db::get_resource_library_skills(&pool)
            .await
            .unwrap()
            .remove(0);
        trash_skill(
            &pool,
            &skill.id,
            skills::DeleteResourceSkillOptions {
                cascade_uninstall: true,
            },
        )
        .await
        .unwrap();
        let entry = list(&pool).unwrap().remove(0);
        assert!(archive::path(&pool, &entry.id).unwrap().exists());
        purge(&pool, vec![entry.id.clone()]).await.unwrap();
        assert!(!archive::path(&pool, &entry.id).unwrap().exists());
        assert!(list(&pool).unwrap().is_empty());
        assert!(restore_locked(&pool, &entry.id, false).await.is_err());
        let rules = super::super::repository_import_rules::load(&pool, "example/tools")
            .await
            .unwrap();
        assert_eq!(
            super::super::repository_import_rules::update_candidates(
                &repo,
                &snapshot,
                rules.as_ref()
            )
            .unwrap()
            .len(),
            1
        );
    }
}
