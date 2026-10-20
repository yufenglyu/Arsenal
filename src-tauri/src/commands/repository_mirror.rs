//! Strict, immutable whole-repository synchronization through the existing task queue.
use super::{
    github_import as github, linker,
    remote_sources::{
        self, RepositorySyncPreview, RepositorySyncPreviewItem, RepositorySyncPreviewReport,
    },
    repository_docs, skills,
};
use crate::{
    db::{self, AppStore},
    AppState,
};
use base64::Engine;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use std::{
    collections::{BTreeMap, HashSet},
    io::{Read, Write},
    path::{Path, PathBuf},
};
use tauri::State;

// Also protects checks from observing an in-progress mirror. Task queue cancellation
// is honored between repository steps; a started repository finishes its current write.
pub(crate) static MIRROR_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
const MAX_SNAPSHOT_BYTES: usize = 128 * 1024 * 1024;
const MAX_CACHE_BYTES: usize = 1024 * 1024 * 1024;
const CACHE_HIGH_WATER_BYTES: usize = MAX_CACHE_BYTES * 4 / 5;
const SNAPSHOT_ENCODING: &str = "gzip-base64-v1";

fn encode_payload(payload: &str) -> Result<String, String> {
    let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
    encoder
        .write_all(payload.as_bytes())
        .map_err(|_| "WRITE_FAILED")?;
    Ok(base64::engine::general_purpose::STANDARD
        .encode(encoder.finish().map_err(|_| "WRITE_FAILED")?))
}

fn read_snapshot(row: &serde_json::Value) -> Result<Snapshot, String> {
    let payload = row["payload"].as_str().ok_or("SNAPSHOT_UNAVAILABLE")?;
    if digest(payload.as_bytes()) != row["digest"].as_str().ok_or("SNAPSHOT_UNAVAILABLE")? {
        return Err("SNAPSHOT_UNAVAILABLE".into());
    }
    match row["encoding"].as_str() {
        None => serde_json::from_str(payload).map_err(|_| "SNAPSHOT_UNAVAILABLE".into()),
        Some(SNAPSHOT_ENCODING) => {
            let bytes = base64::engine::general_purpose::STANDARD
                .decode(payload)
                .map_err(|_| "SNAPSHOT_UNAVAILABLE")?;
            let mut decoded = Vec::new();
            flate2::read::GzDecoder::new(bytes.as_slice())
                .take(MAX_CACHE_BYTES as u64 + 1)
                .read_to_end(&mut decoded)
                .map_err(|_| "SNAPSHOT_UNAVAILABLE")?;
            if decoded.len() > MAX_CACHE_BYTES {
                return Err("SNAPSHOT_TOO_LARGE".into());
            }
            serde_json::from_slice(&decoded).map_err(|_| "SNAPSHOT_UNAVAILABLE".into())
        }
        _ => Err("SNAPSHOT_UNAVAILABLE".into()),
    }
}

// IDs come from the live preview and unfinished apply tasks. A recovery journal
// and any applying snapshot always take precedence over frontend retention.
async fn reconcile_snapshots(pool: &AppStore, retained: &[String]) -> Result<(), String> {
    let retained: HashSet<_> = retained.iter().map(String::as_str).collect();
    let obsolete = pool.read(|d| {
        Ok(d.table("repository_mirror_snapshots")
            .iter()
            .filter(|(id, row)| {
                !retained.contains(id.as_str())
                    && row["state"] != "applying"
                    && !d
                        .table("repository_mirror_recovery")
                        .contains_key(id.as_str())
            })
            .map(|(id, _)| id.clone())
            .collect::<Vec<_>>())
    })?;
    if obsolete.is_empty() {
        return Ok(());
    }
    pool.write(|d| {
        for id in &obsolete {
            d.table_mut("repository_mirror_snapshots").remove(id);
        }
        Ok(())
    })
    .await
}

// Upgrade retained legacy snapshots without changing IDs, content or write consent.
pub async fn compact_snapshot_cache(pool: &AppStore) -> Result<usize, String> {
    let _guard = MIRROR_LOCK.lock().await;
    compact_snapshot_cache_locked(pool).await
}
pub async fn reconcile_snapshot_cache(pool: &AppStore, retained: &[String]) -> Result<(), String> {
    let _guard = MIRROR_LOCK.lock().await;
    reconcile_snapshots(pool, retained).await
}
async fn compact_snapshot_cache_locked(pool: &AppStore) -> Result<usize, String> {
    finish_completed_snapshots(pool).await?;
    let replacements = pool.read(|d| {
        d.table("repository_mirror_snapshots")
            .iter()
            .filter(|(_, row)| row["encoding"].is_null() && row["payload"].is_string())
            .map(|(id, row)| {
                let payload = row["payload"].as_str().ok_or("SNAPSHOT_UNAVAILABLE")?;
                if digest(payload.as_bytes())
                    != row["digest"].as_str().ok_or("SNAPSHOT_UNAVAILABLE")?
                {
                    return Err("SNAPSHOT_UNAVAILABLE".into());
                }
                Ok((id.clone(), encode_payload(payload)?))
            })
            .collect::<Result<Vec<_>, String>>()
    })?;
    let count = replacements.len();
    if count > 0 {
        pool.write(|d| {
            for (id, payload) in &replacements {
                let row = d
                    .table_mut("repository_mirror_snapshots")
                    .get_mut(id)
                    .ok_or("PREVIEW_EXPIRED")?;
                row["digest"] = digest(payload.as_bytes()).into();
                row["payload"] = payload.clone().into();
                row["encoding"] = SNAPSHOT_ENCODING.into();
            }
            Ok(())
        })
        .await?;
    }
    Ok(count)
}

fn clear_completed_payload(d: &mut crate::storage::store::Data, id: &str) {
    if let Some(row) = d.table_mut("repository_mirror_snapshots").get_mut(id) {
        if let Some(row) = row.as_object_mut() {
            for field in ["payload", "digest", "encoding"] {
                row.remove(field);
            }
        }
    }
    d.table_mut("repository_mirror_recovery").remove(id);
}

// Only the committed applied state proves recovery bytes are no longer needed.
// Keep a small receipt so a replay after a lost IPC response remains idempotent.
async fn finish_completed_snapshots(pool: &AppStore) -> Result<(), String> {
    let ids = pool.read(|d| {
        Ok(d.table("repository_mirror_snapshots")
            .iter()
            .filter(|(id, row)| {
                row["state"] == "applied"
                    && (!row["payload"].is_null()
                        || d.table("repository_mirror_recovery")
                            .contains_key(id.as_str()))
            })
            .map(|(id, _)| id.clone())
            .collect::<Vec<_>>())
    })?;
    if ids.is_empty() {
        return Ok(());
    }
    pool.write(|d| {
        for id in &ids {
            clear_completed_payload(d, id);
        }
        Ok(())
    })
    .await
}

fn cache_payload_bytes(pool: &AppStore) -> Result<usize, String> {
    pool.read(|d| {
        Ok(d.table("repository_mirror_snapshots")
            .values()
            .filter_map(|row| row["payload"].as_str())
            .map(str::len)
            .sum())
    })
}

async fn ensure_cache_capacity(pool: &AppStore, incoming: usize) -> Result<(), String> {
    if cache_payload_bytes(pool)?.saturating_add(incoming) >= CACHE_HIGH_WATER_BYTES {
        finish_completed_snapshots(pool).await?;
    }
    if cache_payload_bytes(pool)?.saturating_add(incoming) > MAX_CACHE_BYTES {
        return Err("SNAPSHOT_CACHE_FULL".into());
    }
    Ok(())
}

#[tauri::command]
pub async fn list_repository_snapshot_cache(
    state: State<'_, AppState>,
) -> Result<Vec<String>, MirrorError> {
    let _guard = MIRROR_LOCK.lock().await;
    finish_completed_snapshots(&state.db)
        .await
        .map_err(MirrorError::from)?;
    state
        .db
        .read(|d| {
            Ok(d.table("repository_mirror_snapshots")
                .keys()
                .cloned()
                .collect())
        })
        .map_err(MirrorError::from)
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Coverage {
    pub skill_id: String,
    pub name: String,
    pub repository: Option<String>,
    pub status: String,
    pub reason: Option<String>,
}
#[derive(Clone, Serialize, Deserialize)]
struct Snapshot {
    version: u32,
    repo: github::GitHubRepoRef,
    contents: github::GitHubRepoSnapshot,
    originals: Vec<(db::SkillSource, db::Skill)>,
    installations: Vec<db::SkillInstallation>,
}
fn digest(bytes: &[u8]) -> String {
    format!("sha256-v1:{:x}", Sha256::digest(bytes))
}
fn files_digest(files: &BTreeMap<String, Vec<u8>>) -> String {
    let mut hash = Sha256::new();
    for (name, bytes) in files {
        hash.update((name.len() as u64).to_le_bytes());
        hash.update(name.as_bytes());
        hash.update((bytes.len() as u64).to_le_bytes());
        hash.update(bytes);
    }
    format!("sha256-v1:{:x}", hash.finalize())
}
fn source_key(path: &str) -> String {
    let path = path.trim_matches('/');
    if path == "." {
        "SKILL.md".into()
    } else if path.to_ascii_lowercase().ends_with("skill.md") {
        path.into()
    } else {
        format!("{path}/SKILL.md")
    }
}
fn matching<'a>(group: &'a [(db::SkillSource, db::Skill)], path: &str) -> Option<&'a db::Skill> {
    group
        .iter()
        .find(|(source, _)| {
            source
                .source_path
                .as_deref()
                .is_some_and(|p| source_key(p).eq_ignore_ascii_case(&source_key(path)))
        })
        .map(|(_, skill)| skill)
}
#[cfg(test)]
fn repo_directory(root: &Path, repo: &github::GitHubRepoRef) -> PathBuf {
    crate::path_utils::source_grouped_skill_dir(
        root,
        Some(&repo.owner),
        Some(&format!("{}/{}", repo.owner, repo.repo)),
        None,
        "placeholder",
    )
    .parent()
    .unwrap()
    .to_path_buf()
}
fn empty_report(repository: &str) -> RepositorySyncPreview {
    RepositorySyncPreview {
        local_modified: None,
        repository: repository.into(),
        snapshot_id: None,
        document_changes: vec![],
        current_ref: None,
        remote_ref: None,
        added: vec![],
        modified: vec![],
        deleted: vec![],
        unchanged: vec![],
        error: None,
    }
}
fn validate_snapshot(snapshot: &Snapshot) -> Result<Vec<github::RemoteSkillCandidate>, String> {
    if snapshot.version != 1 || !github::is_commit_ref(&snapshot.repo.branch) {
        return Err("PREVIEW_EXPIRED".into());
    }
    let mut bytes = 0usize;
    let mut paths = HashSet::new();
    for (path, content) in &snapshot.contents.files {
        bytes = bytes
            .checked_add(content.len())
            .ok_or("SNAPSHOT_TOO_LARGE")?;
        if bytes > MAX_SNAPSHOT_BYTES {
            return Err("SNAPSHOT_TOO_LARGE".into());
        }
        if path.contains('\\')
            || !Path::new(path)
                .components()
                .all(|p| matches!(p, std::path::Component::Normal(_)))
            || path.is_empty()
            || path
                .split('/')
                .any(|p| p.contains(':') || p.ends_with(['.', ' ']))
            || !paths.insert(path.to_lowercase())
        {
            return Err("UNSAFE_SNAPSHOT_PATH".into());
        }
    }
    let candidates =
        github::build_repo_skill_candidates_from_snapshot(&snapshot.repo, &snapshot.contents)?;
    let mut targets = HashSet::new();
    for candidate in &candidates {
        if !targets.insert(candidate.skill_directory_name.to_lowercase()) {
            return Err("Conflicting skill directories in repository".into());
        }
        github::mirror_skill_files(&snapshot.contents, &candidate.source_path)?;
    }
    Ok(candidates)
}
async fn preview(pool: &AppStore, snapshot: &Snapshot) -> Result<RepositorySyncPreview, String> {
    let candidates = validate_snapshot(snapshot)?;
    let mut report = empty_report(&format!("{}/{}", snapshot.repo.owner, snapshot.repo.repo));
    report.remote_ref = Some(snapshot.repo.branch.clone());
    report.local_modified = Some(false);
    let mut matched = HashSet::new();
    for candidate in candidates {
        let remote = github::mirror_skill_files(&snapshot.contents, &candidate.source_path)?;
        let mut item = RepositorySyncPreviewItem {
            source_path: Some(candidate.source_path.clone()),
            version: files_digest(&remote),
            files: vec![],
            skill_id: candidate.skill_id,
            name: candidate.skill_name,
        };
        if let Some(skill) = matching(&snapshot.originals, &candidate.source_path) {
            matched.insert(skill.id.clone());
            item.skill_id = skill.id.clone();
            let directory = Path::new(&skill.file_path)
                .parent()
                .ok_or("Invalid skill directory")?;
            let local = if directory.exists() {
                remote_sources::local_skill_files(directory)?
            } else {
                BTreeMap::new()
            };
            item.files = remote_sources::compare_files(&local, &remote);
            if !item.files.is_empty() {
                let baseline = db::get_skill_source_sync(pool, &skill.id).await?;
                if baseline.and_then(|b| b.skill_fingerprint).as_deref() != Some(files_digest(&local).as_str()) {
                    report.local_modified = Some(true);
                }
            }
            let mut installation_changed = false;
            for installation in snapshot
                .installations
                .iter()
                .filter(|i| i.skill_id == skill.id)
            {
                let path = Path::new(&installation.installed_path);
                if installation.link_type == "copy" && path.exists() && remote_sources::local_skill_files(path)? != local {
                    report.local_modified = Some(true);
                }
                if (installation.link_type == "copy"
                    && (!path.exists() || remote_sources::local_skill_files(path)? != remote))
                    || (installation.link_type != "copy"
                        && !crate::path_utils::paths_resolve_to_same_entry(path, directory))
                {
                    installation_changed = true;
                }
            }
            if item.files.is_empty() && !installation_changed {
                report.unchanged.push(item);
            } else {
                report.modified.push(item);
            }
        } else {
            item.files = remote_sources::compare_files(&BTreeMap::new(), &remote);
            report.added.push(item);
        }
    }
    for (source, skill) in &snapshot.originals {
        if !matched.contains(&skill.id) {
            report.deleted.push(RepositorySyncPreviewItem {
                skill_id: skill.id.clone(),
                name: skill.name.clone(),
                source_path: source.source_path.clone(),
                version: "deleted".into(),
                files: vec![],
            });
        }
    }
    let root = db::get_skill_resource_library_dir(pool).await?;
    report.document_changes = remote_sources::compare_files(
        &repository_docs::files(
            &super::repository_address::directory(
                pool,
                &root,
                &snapshot.repo.owner,
                &format!("{}/{}", snapshot.repo.owner, snapshot.repo.repo),
            )
            .await?,
        )?,
        &github::snapshot_readmes(&snapshot.contents),
    );
    // Document edits have no stored per-file sync baseline; require explicit review.
    if report.document_changes.iter().any(|file| file.status == "modified") {
        report.local_modified = Some(true);
    }
    Ok(report)
}
async fn persist_snapshot(pool: &AppStore, snapshot: &Snapshot) -> Result<String, String> {
    let json = serde_json::to_string(snapshot).map_err(|e| e.to_string())?;
    if json.len() > MAX_CACHE_BYTES {
        return Err("SNAPSHOT_TOO_LARGE".into());
    }
    let payload = encode_payload(&json)?;
    let payload_digest = digest(payload.as_bytes());
    if let Some(id) = pool.read(|d| {
        Ok(d.table("repository_mirror_snapshots")
            .iter()
            .find(|(_, row)| {
                row["state"] == "checked"
                    && row["digest"] == payload_digest
                    && row["payload"] == payload
            })
            .map(|(id, _)| id.clone()))
    })? {
        return Ok(id);
    }
    ensure_cache_capacity(pool, payload.len()).await?;
    let id = uuid::Uuid::new_v4().to_string();
    let repository = format!("{}/{}", snapshot.repo.owner, snapshot.repo.repo).to_lowercase();
    pool.write(|d| {
        d.table_mut("repository_mirror_snapshots").insert(id.clone(), serde_json::json!({"id":id,"repository":repository,"payload":payload,"encoding":SNAPSHOT_ENCODING,"digest":payload_digest,"created_at":chrono::Utc::now().to_rfc3339(),"state":"checked"}));
        d.table_mut("repository_mirrors").entry(repository.clone()).or_insert_with(|| serde_json::json!({"repository":repository,"last_snapshot_id":null})); Ok(())
    }).await?;
    Ok(id)
}
async fn retain_actionable_snapshot(
    pool: &AppStore,
    snapshot: &Snapshot,
    report: &mut RepositorySyncPreview,
) -> Result<(), String> {
    // A no-change report needs only its checked commit, never an apply payload.
    if !(report.added.is_empty()
        && report.modified.is_empty()
        && report.deleted.is_empty()
        && report.document_changes.is_empty())
    {
        report.snapshot_id = Some(persist_snapshot(pool, snapshot).await?);
    }
    Ok(())
}
pub async fn check(
    pool: &AppStore,
    repositories: Option<Vec<String>>,
) -> Result<RepositorySyncPreviewReport, String> {
    check_with_retained(pool, repositories, None).await
}
pub async fn check_with_retained(
    pool: &AppStore,
    repositories: Option<Vec<String>>,
    retained_snapshot_ids: Option<Vec<String>>,
) -> Result<RepositorySyncPreviewReport, String> {
    let _guard = MIRROR_LOCK.lock().await;
    super::repository_address::recover(pool).await?;
    resume_interrupted(pool).await?;
    if let Some(ids) = retained_snapshot_ids {
        reconcile_snapshots(pool, &ids).await?;
    }
    compact_snapshot_cache_locked(pool).await?;
    linker::normalize_resource_backed_central_records_impl(pool).await?;
    // Scan before freezing the denominator, including independent local skills.
    skills::get_resource_library_skills_impl(pool).await?;
    linker::normalize_resource_backed_central_records_impl(pool).await?;
    let all = db::get_resource_library_skills(pool).await?;
    let sources = db::get_all_skill_sources(pool).await?;
    let mut groups = remote_sources::github_repo_source_groups(pool, false).await?;
    for repository in pool.read(|d| {
        Ok(d.table("repository_mirrors")
            .values()
            .filter_map(|r| r["repository"].as_str().map(str::to_owned))
            .collect::<Vec<_>>())
    })? {
        groups.entry(repository).or_default();
    }
    let scope = repositories.filter(|r| !r.is_empty()).map(|r| {
        r.into_iter()
            .map(|r| r.to_lowercase())
            .collect::<HashSet<_>>()
    });
    let mut coverage = all
        .iter()
        .map(|skill| {
            let source = sources.iter().find(|source| source.skill_id == skill.id);
            let repository = source
                .and_then(|s| s.source_repo.clone())
                .map(|r| r.to_lowercase());
            let reason = match source {
                None => Some("local"),
                Some(s) if matches!(s.source_type.as_str(), "local" | "manual") => Some("local"),
                Some(s) if !matches!(s.source_type.as_str(), "github" | "skills-cli") => {
                    Some("unsupported")
                }
                Some(_) if repository.is_none() => Some("missing_source"),
                Some(_)
                    if scope.as_ref().is_some_and(|scope| {
                        !repository.as_ref().is_some_and(|r| scope.contains(r))
                    }) =>
                {
                    Some("outside_scope")
                }
                _ => None,
            };
            Coverage {
                skill_id: skill.id.clone(),
                name: skill.name.clone(),
                repository,
                status: if reason.is_some() {
                    "excluded"
                } else {
                    "pending"
                }
                .into(),
                reason: reason.map(str::to_string),
            }
        })
        .collect::<Vec<_>>();
    let auth = github::github_direct_auth_from_settings(pool).await?;
    drop(_guard); // Network requests for independent repositories may overlap.
    let mut reports = Vec::new();
    let mut groups = groups.into_iter().collect::<Vec<_>>();
    groups.sort_by(|a, b| a.0.cmp(&b.0));
    for (repository, originals) in groups {
        if scope.as_ref().is_some_and(|s| !s.contains(&repository)) {
            continue;
        }
        let result = async {
            let display = originals
                .first()
                .and_then(|(source, _)| source.source_repo.as_deref())
                .unwrap_or(&repository);
            let repo = github::resolve_update_revision(display, auth.as_deref(), None).await?;
            let contents = github::fetch_mirror_snapshot(&repo, auth.as_deref()).await?;
            let _guard = MIRROR_LOCK.lock().await; // Serialize local inspection and snapshot publication.
            let mut installations = Vec::new();
            for (_, skill) in &originals {
                installations.extend(db::get_skill_installations(pool, &skill.id).await?);
            }
            let snapshot = Snapshot {
                version: 1,
                repo,
                contents,
                originals,
                installations,
            };
            let mut report = preview(pool, &snapshot).await?;
            let ids = snapshot
                .originals
                .iter()
                .map(|(_, skill)| skill.id.clone())
                .collect::<Vec<_>>();
            db::record_skill_check_times(pool, &ids, &chrono::Utc::now().to_rfc3339()).await?;
            retain_actionable_snapshot(pool, &snapshot, &mut report).await?;
            Ok::<_, String>(report)
        }
        .await;
        for item in coverage
            .iter_mut()
            .filter(|c| c.repository.as_deref() == Some(&repository) && c.status == "pending")
        {
            item.status = if result.is_ok() {
                "completed"
            } else {
                "failed"
            }
            .into();
        }
        reports.push(result.unwrap_or_else(|error| {
            let mut report = empty_report(&repository);
            report.error = Some(error);
            report
        }));
    }
    for item in coverage.iter_mut().filter(|c| c.status == "pending") {
        item.status = "excluded".into();
        item.reason = Some("missing_source".into());
    }
    Ok(RepositorySyncPreviewReport {
        repositories: reports,
        coverage,
    })
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MirrorError {
    code: &'static str,
}
impl From<String> for MirrorError {
    fn from(error: String) -> Self {
        let code = [
            "PREVIEW_EXPIRED",
            "SNAPSHOT_UNAVAILABLE",
            "LOCAL_IDENTITY_CHANGED",
            "UNSAFE_TARGET",
            "UNSAFE_INSTALL_TARGET",
            "UNSAFE_SNAPSHOT_PATH",
            "SNAPSHOT_CACHE_FULL",
            "SNAPSHOT_TOO_LARGE",
        ]
        .into_iter()
        .find(|code| error.starts_with(code))
        .unwrap_or("WRITE_FAILED");
        Self { code }
    }
}
#[tauri::command]
pub async fn apply_repository_snapshot(
    state: State<'_, AppState>,
    snapshot_id: String,
) -> Result<(), MirrorError> {
    apply(&state.db, &snapshot_id).await.map_err(Into::into)
}
pub(crate) async fn apply(pool: &AppStore, id: &str) -> Result<(), String> {
    let _guard = MIRROR_LOCK.lock().await;
    super::repository_address::recover(pool).await?;
    resume_interrupted(pool).await?;
    let row = pool
        .read(|d| {
            Ok(d.table("repository_mirror_snapshots")
                .get(id)
                .filter(|r| r["state"] != "invalidated")
                .cloned())
        })?
        .ok_or("PREVIEW_EXPIRED")?;
    if row["state"] == "applied" {
        return Ok(());
    }
    let snapshot = read_snapshot(&row)?;
    let result = apply_contents(pool, id, &snapshot).await;
    if result.is_err() {
        set_snapshot_state(pool, id, "failed").await?;
    }
    result
}
async fn apply_contents(pool: &AppStore, id: &str, snapshot: &Snapshot) -> Result<(), String> {
    linker::normalize_resource_backed_central_records_impl(pool).await?;
    let candidates = validate_snapshot(snapshot)?;
    let repository = format!("{}/{}", snapshot.repo.owner, snapshot.repo.repo);
    let root = db::get_skill_resource_library_dir(pool).await?;
    std::fs::create_dir_all(&root).map_err(|e| e.to_string())?;
    let mut current = remote_sources::github_repo_source_groups(pool, false)
        .await?
        .remove(&repository.to_lowercase())
        .unwrap_or_default();
    // Ownership must survive the check/apply gap. Content edits are intentionally overwritten.
    for (source, original) in &snapshot.originals {
        if let Some(now) = db::get_skill_by_id(pool, &original.id).await? {
            let now_source = db::get_skill_source(pool, &original.id)
                .await?
                .ok_or("LOCAL_IDENTITY_CHANGED")?;
            if now.canonical_path != original.canonical_path
                || now_source.source_repo != source.source_repo
                || now_source
                    .source_path
                    .as_deref()
                    .map(source_key)
                    .map(|p| p.to_lowercase())
                    != source
                        .source_path
                        .as_deref()
                        .map(source_key)
                        .map(|p| p.to_lowercase())
            {
                return Err("LOCAL_IDENTITY_CHANGED".into());
            }
        }
    }
    let mut owned_targets = HashSet::new();
    let mut owned_sources = HashSet::new();
    for (source, skill) in &current {
        let target = Path::new(
            skill
                .canonical_path
                .as_deref()
                .ok_or("LOCAL_IDENTITY_CHANGED")?,
        );
        ensure_owned_path(&root, target)?;
        let key = target.to_string_lossy().to_lowercase();
        if !owned_targets.insert(key)
            || !owned_sources.insert(
                source
                    .source_path
                    .as_deref()
                    .map(source_key)
                    .map(|p| p.to_lowercase()),
            )
        {
            return Err("LOCAL_IDENTITY_CHANGED: duplicate ownership".into());
        }
    }
    let repository_directory =
        super::repository_address::directory(pool, &root, &snapshot.repo.owner, &repository)
            .await?;
    let mut selections = Vec::new();
    let mut retained = HashSet::new();
    for candidate in &candidates {
        let existing = matching(&current, &candidate.source_path);
        let target = existing
            .and_then(|s| s.canonical_path.as_deref())
            .map(PathBuf::from)
            .unwrap_or_else(|| repository_directory.join(&candidate.skill_directory_name));
        ensure_owned_path(&root, &target)?;
        if existing.is_none() && target.exists() {
            return Err("LOCAL_IDENTITY_CHANGED: untracked target directory".into());
        }
        let remote = github::mirror_skill_files(&snapshot.contents, &candidate.source_path)?;
        if let Some(skill) = existing {
            retained.insert(skill.id.clone());
        }
        if existing.is_some()
            && target.exists()
            && remote_sources::local_skill_files(&target)? == remote
            && current.iter().any(|(source, skill)| {
                Some(skill.id.as_str()) == existing.map(|s| s.id.as_str())
                    && source.source_path.as_deref()
                        == Some(candidate.source_manifest_path.as_str())
            })
        {
            continue;
        }
        selections.push(github::GitHubSkillImportSelection {
            source_path: candidate.source_path.clone(),
            resolution: github::DuplicateResolution::Overwrite,
            renamed_skill_id: existing.map(|s| s.id.clone()),
        });
    }
    save_recovery(pool, id, &current).await?;
    set_snapshot_state(pool, id, "applying").await?;
    if !selections.is_empty() {
        github::import_prepared_snapshot(
            pool,
            snapshot.repo.clone(),
            &snapshot.contents,
            selections,
            None,
            None,
        )
        .await?;
    } else {
        github::mirror_documents(pool, &snapshot.repo, &snapshot.contents).await?;
    }
    // Reload IDs assigned to newly imported skills. Every remote candidate must exist.
    current = remote_sources::github_repo_source_groups(pool, false)
        .await?
        .remove(&repository.to_lowercase())
        .unwrap_or_default();
    for candidate in &candidates {
        let skill = matching(&current, &candidate.source_path)
            .ok_or("WRITE_FAILED: missing imported skill")?;
        retained.insert(skill.id.clone());
        let remote = github::mirror_skill_files(&snapshot.contents, &candidate.source_path)?;
        if remote_sources::local_skill_files(
            Path::new(&skill.file_path)
                .parent()
                .ok_or("Invalid skill path")?,
        )? != remote
        {
            return Err("WRITE_FAILED: content verification failed".into());
        }
        linker::refresh_resource_installations_impl(pool, &skill.id).await?;
        let now = chrono::Utc::now().to_rfc3339();
        db::upsert_skill_source_sync(
            pool,
            &db::SkillSourceSync {
                skill_id: skill.id.clone(),
                sync_scope: "github-repository".into(),
                remote_ref: Some(snapshot.repo.branch.clone()),
                skill_fingerprint: Some(files_digest(&remote)),
                last_checked_at: Some(now.clone()),
                last_sync_at: Some(now.clone()),
                sync_status: "synced".into(),
                sync_error: None,
                remote_deleted: false,
                updated_at: now,
            },
        )
        .await?;
    }
    for (_, skill) in current {
        if !retained.contains(&skill.id) {
            skills::delete_resource_skill_impl(
                pool,
                &skill.id,
                skills::DeleteResourceSkillOptions {
                    cascade_uninstall: true,
                },
            )
            .await?;
        }
    }
    if repository_docs::files(
        &super::repository_address::directory(
            pool,
            &root,
            &snapshot.repo.owner,
            &format!("{}/{}", snapshot.repo.owner, snapshot.repo.repo),
        )
        .await?,
    )? != github::snapshot_readmes(&snapshot.contents)
    {
        return Err("WRITE_FAILED: repository document verification failed".into());
    }
    pool.write(|d| {
        // Publish completion and retire its recovery/cache bytes atomically.
        if let Some(row) = d.table_mut("repository_mirror_snapshots").get_mut(id) {
            row["state"] = "applied".into();
        }
        clear_completed_payload(d, id);
        if let Some(row) = d
            .table_mut("repository_mirrors")
            .get_mut(&repository.to_ascii_lowercase())
        {
            row["last_snapshot_id"] = id.into();
        }
        Ok(())
    })
    .await?;
    Ok(())
}
pub(crate) fn ensure_owned_path(root: &Path, target: &Path) -> Result<(), String> {
    if std::fs::symlink_metadata(root).is_ok_and(|m| m.file_type().is_symlink()) {
        return Err("UNSAFE_TARGET".into());
    }
    let relative = target.strip_prefix(root).map_err(|_| "UNSAFE_TARGET")?;
    if relative.as_os_str().is_empty() {
        return Err("UNSAFE_TARGET".into());
    }
    let mut path = root.to_path_buf();
    for component in relative.components() {
        if !matches!(component, std::path::Component::Normal(_)) {
            return Err("UNSAFE_TARGET".into());
        }
        path.push(component);
        if std::fs::symlink_metadata(&path).is_ok_and(|m| m.file_type().is_symlink()) {
            return Err("UNSAFE_TARGET".into());
        }
    }
    Ok(())
}

pub(crate) async fn resume_interrupted(pool: &AppStore) -> Result<(), String> {
    let mut rows = pool.read(|d| {
        Ok(d.table("repository_mirror_snapshots")
            .values()
            .filter(|r| r["state"] == "applying")
            .cloned()
            .collect::<Vec<_>>())
    })?;
    rows.sort_by(|a, b| a["created_at"].as_str().cmp(&b["created_at"].as_str()));
    for row in rows {
        let id = row["id"].as_str().ok_or("SNAPSHOT_UNAVAILABLE")?.to_owned();
        let snapshot = read_snapshot(&row)?;
        if let Err(error) = apply_contents(pool, &id, &snapshot).await {
            set_snapshot_state(pool, &id, "failed").await?;
            return Err(error);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn successful_apply_releases_payload_and_recovery_but_keeps_replay_receipt() {
        let (_dir, pool, snapshot) = fixture().await;
        let id = persist_snapshot(&pool, &snapshot).await.unwrap();
        apply(&pool, &id).await.unwrap();
        pool.read(|d| {
            let row = &d.table("repository_mirror_snapshots")[&id];
            assert_eq!(row["state"], "applied");
            assert!(row["payload"].is_null());
            assert!(!d.table("repository_mirror_recovery").contains_key(&id));
            Ok(())
        })
        .unwrap();
        apply(&pool, &id).await.unwrap();
        assert_eq!(compact_snapshot_cache(&pool).await.unwrap(), 0);
    }
    #[tokio::test]
    async fn pressure_reclaims_completed_bytes_without_evicting_unfinished_work() {
        let (_dir, pool, _) = fixture().await;
        pool.write(|d| {
            d.table_mut("repository_mirror_snapshots").insert(
                "done".into(),
                serde_json::json!({"state":"applied","payload":"old"}),
            );
            d.table_mut("repository_mirror_recovery")
                .insert("done".into(), serde_json::json!({}));
            Ok(())
        })
        .await
        .unwrap();
        ensure_cache_capacity(&pool, MAX_CACHE_BYTES).await.unwrap();
        for (id, state) in [
            ("review", "checked"),
            ("retry", "failed"),
            ("active", "applying"),
        ] {
            pool.write(|d| {
                d.table_mut("repository_mirror_snapshots").insert(
                    id.into(),
                    serde_json::json!({"state":state,"payload":"keep"}),
                );
                Ok(())
            })
            .await
            .unwrap();
        }
        assert_eq!(
            ensure_cache_capacity(&pool, MAX_CACHE_BYTES)
                .await
                .unwrap_err(),
            "SNAPSHOT_CACHE_FULL"
        );
        assert_eq!(cache_payload_bytes(&pool).unwrap(), 12);
        assert!(!pool
            .read(|d| Ok(d.table("repository_mirror_recovery").contains_key("done")))
            .unwrap());
    }
    #[tokio::test]
    async fn explicit_release_rechecks_apply_and_recovery_protection() {
        let (_dir, pool, _) = fixture().await;
        pool.write(|d| {
            for (id, state) in [
                ("old", "checked"),
                ("active", "applying"),
                ("recover", "failed"),
            ] {
                d.table_mut("repository_mirror_snapshots").insert(
                    id.into(),
                    serde_json::json!({"state":state,"payload":"keep"}),
                );
            }
            d.table_mut("repository_mirror_recovery")
                .insert("recover".into(), serde_json::json!({}));
            Ok(())
        })
        .await
        .unwrap();
        release_snapshots_locked(&pool, &["old".into(), "active".into(), "recover".into()])
            .await
            .unwrap();
        assert_eq!(
            pool.read(|d| Ok(d
                .table("repository_mirror_snapshots")
                .keys()
                .cloned()
                .collect::<Vec<_>>()))
                .unwrap(),
            vec!["active", "recover"]
        );
    }
    #[tokio::test]
    async fn unchanged_repository_needs_no_retained_apply_payload() {
        let (_dir, pool, snapshot) = fixture().await;
        let mut report = empty_report("example/fixtures");
        report.remote_ref = Some(snapshot.repo.branch.clone());
        retain_actionable_snapshot(&pool, &snapshot, &mut report)
            .await
            .unwrap();
        assert!(report.snapshot_id.is_none());
        assert!(report.remote_ref.is_some());
        assert_eq!(
            pool.read(|d| Ok(d.table("repository_mirror_snapshots").len()))
                .unwrap(),
            0
        );
    }
    #[tokio::test]
    async fn unchanged_check_reuses_immutable_snapshot_instead_of_filling_cache() {
        let (_dir, pool, snapshot) = fixture().await;
        let first = persist_snapshot(&pool, &snapshot).await.unwrap();
        let second = persist_snapshot(&pool, &snapshot).await.unwrap();
        assert_eq!(
            first, second,
            "identical checked snapshots must not accumulate"
        );
        assert_eq!(
            pool.read(|d| Ok(d.table("repository_mirror_snapshots").len()))
                .unwrap(),
            1
        );
    }
    #[tokio::test]
    async fn cache_compaction_preserves_legacy_snapshot_identity_and_bytes() {
        let (_dir, pool, mut snapshot) = fixture().await;
        snapshot
            .contents
            .files
            .insert("skills/one/large.txt".into(), vec![65; 100_000]);
        let payload = serde_json::to_string(&snapshot).unwrap();
        pool.write(|d| { d.table_mut("repository_mirror_snapshots").insert("legacy".into(),
            serde_json::json!({"id":"legacy","state":"checked","payload":payload,"digest":digest(payload.as_bytes())})); Ok(()) }).await.unwrap();
        assert_eq!(compact_snapshot_cache(&pool).await.unwrap(), 1);
        let row = pool
            .read(|d| Ok(d.table("repository_mirror_snapshots")["legacy"].clone()))
            .unwrap();
        assert!(row["payload"].as_str().unwrap().len() < payload.len() / 10);
        assert_eq!(
            read_snapshot(&row).unwrap().contents.files,
            snapshot.contents.files
        );
        assert_eq!(compact_snapshot_cache(&pool).await.unwrap(), 0);
        let mut corrupt = row;
        corrupt["payload"] = "corrupt".into();
        assert!(read_snapshot(&corrupt).is_err());
    }
    #[tokio::test]
    async fn cache_reconciliation_preserves_retained_applying_and_recovery_snapshots() {
        let (_dir, pool, _) = fixture().await;
        pool.write(|d| {
            for id in ["retained", "applying", "recoverable", "orphan"] {
                d.table_mut("repository_mirror_snapshots").insert(
                    id.into(),
                    serde_json::json!({"state":if id=="applying" {"applying"} else {"checked"}}),
                );
            }
            d.table_mut("repository_mirror_recovery")
                .insert("recoverable".into(), serde_json::json!({}));
            Ok(())
        })
        .await
        .unwrap();
        reconcile_snapshots(&pool, &["retained".into()])
            .await
            .unwrap();
        let ids = pool
            .read(|d| {
                Ok(d.table("repository_mirror_snapshots")
                    .keys()
                    .cloned()
                    .collect::<Vec<_>>())
            })
            .unwrap();
        assert_eq!(ids, vec!["applying", "recoverable", "retained"]);
    }
    #[tokio::test]
    #[ignore = "explicit public network diagnostic; never reads installed user data"]
    async fn live_public_repository_check_probe() {
        let repository =
            std::env::var("ARSENAL_DIAGNOSTIC_REPOSITORY").expect("explicit repository required");
        let (_dir, pool, _) = fixture().await;
        let auth = std::env::var("ARSENAL_DIAGNOSTIC_AUTH").ok();
        let resolved = github::resolve_update_revision(&repository, auth.as_deref(), None)
            .await
            .map_err(|_| "revision request failed")
            .unwrap();
        let contents = github::fetch_mirror_snapshot(&resolved, auth.as_deref())
            .await
            .map_err(|error| {
                if error.starts_with("UNSAFE_SNAPSHOT_PATH") {
                    "unsupported remote file type"
                } else {
                    "snapshot request failed"
                }
            })
            .unwrap();
        let snapshot = Snapshot {
            version: 1,
            repo: resolved,
            contents,
            originals: vec![],
            installations: vec![],
        };
        validate_snapshot(&snapshot).unwrap();
        let id = persist_snapshot(&pool, &snapshot).await.unwrap();
        let row = pool
            .read(|d| Ok(d.table("repository_mirror_snapshots")[&id].clone()))
            .unwrap();
        assert_eq!(
            read_snapshot(&row).unwrap().contents.files,
            snapshot.contents.files
        );
        println!("Network resolve + download + validation + persisted roundtrip: passed");
    }
    async fn fixture() -> (tempfile::TempDir, AppStore, Snapshot) {
        let dir = tempfile::tempdir().unwrap();
        let pool = crate::db::AppStore::connect(":memory:").await.unwrap();
        db::init_database(&pool).await.unwrap();
        crate::storage::fixtures::disable_except(&pool, &["central", "claude-code"]).await;
        db::set_skill_resource_library_dir(&pool, &dir.path().join("library").to_string_lossy())
            .await
            .unwrap();
        pool.write(|d| {
            if let Some(row) = d.table_mut("agents").get_mut("central") {
                row["global_skills_dir"] =
                    serde_json::json!(dir.path().join("central").to_string_lossy().as_ref());
            }
            Ok(())
        })
        .await
        .unwrap();
        pool.write(|d| {
            if let Some(row) = d.table_mut("agents").get_mut("claude-code") {
                row["global_skills_dir"] =
                    serde_json::json!(dir.path().join("platform").to_string_lossy().as_ref());
            }
            Ok(())
        })
        .await
        .unwrap();
        let repo = github::GitHubRepoRef {
            owner: "example".into(),
            repo: "fixtures".into(),
            branch: "a".repeat(40),
            normalized_url: "https://github.com/example/fixtures".into(),
            stars: None,
        };
        let contents = github::GitHubRepoSnapshot {
            files: [
                (
                    "skills/one/SKILL.md".into(),
                    b"---\nname: one\ndescription: first\n---\nremote".to_vec(),
                ),
                (
                    "skills/two/SKILL.md".into(),
                    b"---\nname: two\ndescription: second\n---\nremote".to_vec(),
                ),
                ("README.md".into(), b"remote docs".to_vec()),
            ]
            .into(),
        };
        (
            dir,
            pool,
            Snapshot {
                version: 1,
                repo,
                contents,
                originals: vec![],
                installations: vec![],
            },
        )
    }
    async fn capture(pool: &AppStore, mut snapshot: Snapshot) -> Snapshot {
        linker::normalize_resource_backed_central_records_impl(pool)
            .await
            .unwrap();
        snapshot.originals = remote_sources::github_repo_source_groups(pool, false)
            .await
            .unwrap()
            .remove("example/fixtures")
            .unwrap_or_default();
        snapshot.installations.clear();
        for (_, skill) in &snapshot.originals {
            snapshot
                .installations
                .extend(db::get_skill_installations(pool, &skill.id).await.unwrap());
        }
        snapshot
    }
    #[tokio::test]
    async fn preview_distinguishes_upstream_updates_from_local_edits() {
        let (_dir, pool, snapshot) = fixture().await;
        let id = persist_snapshot(&pool, &snapshot).await.unwrap();
        apply(&pool, &id).await.unwrap();
        let snapshot = capture(&pool, snapshot).await;
        let mut upstream = snapshot.clone();
        upstream.contents.files.get_mut("skills/one/SKILL.md").unwrap().extend_from_slice(b"\nupstream change");
        let report = preview(&pool, &upstream).await.unwrap();
        assert!(!report.modified.is_empty());
        assert_eq!(report.local_modified, Some(false));
        let skill = &upstream.originals[0].1;
        std::fs::write(&skill.file_path, "local edit").unwrap();
        let report = preview(&pool, &upstream).await.unwrap();
        assert_eq!(report.local_modified, Some(true));
    }

    #[tokio::test]
    async fn mirror_imports_all_overwrites_edits_and_removes_extra_files_and_skills() {
        let (_dir, pool, snapshot) = fixture().await;
        // Simulate a previous selective import of only one remote skill.
        github::import_prepared_snapshot(
            &pool,
            snapshot.repo.clone(),
            &snapshot.contents,
            vec![github::GitHubSkillImportSelection {
                source_path: "skills/one/SKILL.md".into(),
                resolution: github::DuplicateResolution::Overwrite,
                renamed_skill_id: None,
            }],
            None,
            None,
        )
        .await
        .unwrap();
        let snapshot = capture(&pool, snapshot).await;
        let original = &snapshot.originals[0].1;
        let path = Path::new(&original.file_path).parent().unwrap();
        std::fs::write(path.join("SKILL.md"), "local edit").unwrap();
        std::fs::write(path.join("local-only.txt"), "extra").unwrap();
        let report = preview(&pool, &snapshot).await.unwrap();
        assert_eq!(report.added.len(), 1);
        assert_eq!(report.modified.len(), 1);
        let id = persist_snapshot(&pool, &snapshot).await.unwrap();
        apply(&pool, &id).await.unwrap();
        assert_eq!(
            db::get_resource_library_skills(&pool).await.unwrap().len(),
            2
        );
        assert!(!path.join("local-only.txt").exists());
        assert_eq!(
            std::fs::read(path.join("SKILL.md")).unwrap(),
            snapshot.contents.files["skills/one/SKILL.md"]
        );
        // Retry the same immutable snapshot after a lost acknowledgement.
        apply(&pool, &id).await.unwrap();
        let mut empty = capture(&pool, snapshot).await;
        empty.contents.files.clear();
        let id = persist_snapshot(&pool, &empty).await.unwrap();
        apply(&pool, &id).await.unwrap();
        assert!(db::get_resource_library_skills(&pool)
            .await
            .unwrap()
            .is_empty());
        let count: i64 = crate::storage::fixtures::count(&pool, "repository_mirrors", None);
        assert_eq!(count, 1);
        apply(&pool, &id).await.unwrap();
    }
    #[tokio::test]
    async fn mirror_copy_installation_is_strict_and_deletion_uninstalls_it() {
        let (_dir, pool, snapshot) = fixture().await;
        let id = persist_snapshot(&pool, &snapshot).await.unwrap();
        apply(&pool, &id).await.unwrap();
        linker::install_resource_skill_to_agent_impl(&pool, "one", "claude-code", "copy")
            .await
            .unwrap();
        let snapshot = capture(&pool, snapshot).await;
        let installed = Path::new(&snapshot.installations[0].installed_path);
        std::fs::write(installed.join("extra.txt"), "user edit").unwrap();
        let report = preview(&pool, &snapshot).await.unwrap();
        assert_eq!(report.modified.len(), 1);
        assert_eq!(report.local_modified, Some(true));
        let id = persist_snapshot(&pool, &snapshot).await.unwrap();
        apply(&pool, &id).await.unwrap();
        assert!(!installed.join("extra.txt").exists());
        let mut removed = snapshot.clone();
        removed.contents.files.remove("skills/one/SKILL.md");
        let id = persist_snapshot(&pool, &removed).await.unwrap();
        apply(&pool, &id).await.unwrap();
        assert!(!installed.exists());
        assert!(db::get_skill_installations(&pool, "one")
            .await
            .unwrap()
            .is_empty());
    }
    #[tokio::test]
    async fn corrupted_snapshot_cannot_write_and_documents_are_separate() {
        let (_dir, pool, snapshot) = fixture().await;
        let id = persist_snapshot(&pool, &snapshot).await.unwrap();
        apply(&pool, &id).await.unwrap();
        let mut next = capture(&pool, snapshot).await;
        next.contents
            .files
            .insert("README.md".into(), b"new docs".to_vec());
        let report = preview(&pool, &next).await.unwrap();
        assert!(report.modified.is_empty());
        assert_eq!(report.document_changes.len(), 1);
        assert_eq!(report.local_modified, Some(true));
        let id = persist_snapshot(&pool, &next).await.unwrap();
        pool.write(|d| {
            d.table_mut("repository_mirror_snapshots")
                .get_mut(&id)
                .unwrap()["digest"] = "bad".into();
            Ok(())
        })
        .await
        .unwrap();
        assert_eq!(apply(&pool, &id).await.unwrap_err(), "SNAPSHOT_UNAVAILABLE");
        let root = db::get_skill_resource_library_dir(&pool).await.unwrap();
        assert_eq!(
            std::fs::read(repo_directory(&root, &next.repo).join("README.md")).unwrap(),
            b"remote docs"
        );
    }
    #[tokio::test]
    async fn interrupted_import_swaps_are_restored_before_retry() {
        let (_dir, pool, snapshot) = fixture().await;
        let id = persist_snapshot(&pool, &snapshot).await.unwrap();
        apply(&pool, &id).await.unwrap();
        let snapshot = capture(&pool, snapshot).await;
        let target = Path::new(&snapshot.originals[0].1.file_path)
            .parent()
            .unwrap();
        let root = db::get_skill_resource_library_dir(&pool).await.unwrap();
        let mut transaction = github::ImportDirectoryTransaction::new(&root).unwrap();
        let staged = transaction.root.join("new");
        std::fs::create_dir(&staged).unwrap();
        std::fs::write(staged.join("SKILL.md"), "incomplete").unwrap();
        transaction.install(&staged, target, true).unwrap();
        std::mem::forget(transaction);
        github::recover_import_transactions(&pool).await.unwrap();
        assert_ne!(
            std::fs::read(target.join("SKILL.md")).unwrap(),
            b"incomplete"
        );
        let id = persist_snapshot(&pool, &snapshot).await.unwrap();
        pool.write(|d| {
            d.table_mut("repository_mirror_snapshots")
                .get_mut(&id)
                .unwrap()["state"] = "applying".into();
            Ok(())
        })
        .await
        .unwrap();
        resume_interrupted(&pool).await.unwrap();
        let state: String = pool
            .read(|d| {
                Ok(d.table("repository_mirror_snapshots")[&id]["state"]
                    .as_str()
                    .unwrap()
                    .to_owned())
            })
            .unwrap();
        assert_eq!(state, "applied");
    }
    #[tokio::test]
    async fn root_skill_keeps_nested_resources_and_case_only_rename_keeps_identity() {
        let (_dir, pool, mut snapshot) = fixture().await;
        snapshot.contents.files.insert(
            "SKILL.md".into(),
            b"---\nname: root\ndescription: root\n---\nroot".to_vec(),
        );
        snapshot
            .contents
            .files
            .insert("references/guide.md".into(), b"guide".to_vec());
        let id = persist_snapshot(&pool, &snapshot).await.unwrap();
        apply(&pool, &id).await.unwrap();
        let snapshot = capture(&pool, snapshot).await;
        let root = matching(&snapshot.originals, "SKILL.md").unwrap();
        assert!(Path::new(&root.file_path)
            .parent()
            .unwrap()
            .join("references/guide.md")
            .is_file());
        let original = matching(&snapshot.originals, "skills/one/SKILL.md")
            .unwrap()
            .clone();
        let mut next = snapshot;
        let bytes = next.contents.files.remove("skills/one/SKILL.md").unwrap();
        next.contents
            .files
            .insert("skills/ONE/SKILL.md".into(), bytes);
        let id = persist_snapshot(&pool, &next).await.unwrap();
        apply(&pool, &id).await.unwrap();
        apply(&pool, &id).await.unwrap();
        let updated = db::get_skill_by_id(&pool, &original.id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(updated.canonical_path, original.canonical_path);
        assert_eq!(
            db::get_skill_source(&pool, &original.id)
                .await
                .unwrap()
                .unwrap()
                .source_path
                .as_deref(),
            Some("skills/ONE/SKILL.md")
        );
    }
    #[tokio::test]
    async fn local_git_and_empty_directories_are_not_exempt_from_mirroring() {
        let (_dir, pool, snapshot) = fixture().await;
        let id = persist_snapshot(&pool, &snapshot).await.unwrap();
        apply(&pool, &id).await.unwrap();
        let snapshot = capture(&pool, snapshot).await;
        let target = Path::new(&snapshot.originals[0].1.file_path)
            .parent()
            .unwrap();
        std::fs::create_dir(target.join("empty")).unwrap();
        std::fs::create_dir(target.join(".git")).unwrap();
        std::fs::write(target.join(".git/local"), b"extra").unwrap();
        let id = persist_snapshot(&pool, &snapshot).await.unwrap();
        apply(&pool, &id).await.unwrap();
        assert!(!target.join("empty").exists());
        assert!(!target.join(".git").exists());
    }
    #[tokio::test]
    async fn duplicate_skill_names_in_different_plugins_are_both_imported() {
        let (_dir, pool, mut snapshot) = fixture().await;
        snapshot.contents.files.clear();
        for plugin in ["a", "b"] {
            snapshot.contents.files.insert(
                format!("plugins/{plugin}/skills/shared/SKILL.md"),
                format!("---\nname: shared\ndescription: {plugin}\n---\n{plugin}").into_bytes(),
            );
        }
        let report = preview(&pool, &snapshot).await.unwrap();
        assert_eq!(report.added.len(), 2);
        assert_ne!(report.added[0].skill_id, report.added[1].skill_id);
        let id = persist_snapshot(&pool, &snapshot).await.unwrap();
        apply(&pool, &id).await.unwrap();
        apply(&pool, &id).await.unwrap();
        assert_eq!(
            db::get_resource_library_skills(&pool).await.unwrap().len(),
            2
        );
    }
    #[tokio::test]
    async fn central_links_survive_updates_and_are_removed_with_remote_deletion() {
        let (_dir, pool, snapshot) = fixture().await;
        let id = persist_snapshot(&pool, &snapshot).await.unwrap();
        apply(&pool, &id).await.unwrap();
        let link = linker::add_resource_skill_to_central_impl(&pool, "one")
            .await
            .unwrap();
        let mut next = capture(&pool, snapshot).await;
        next.contents
            .files
            .insert("skills/one/extra.txt".into(), b"new".to_vec());
        let id = persist_snapshot(&pool, &next).await.unwrap();
        apply(&pool, &id).await.unwrap();
        assert_eq!(
            std::fs::read(Path::new(&link.central_path).join("extra.txt")).unwrap(),
            b"new"
        );
        next = capture(&pool, next).await;
        next.contents
            .files
            .retain(|path, _| !path.starts_with("skills/one/"));
        let id = persist_snapshot(&pool, &next).await.unwrap();
        apply(&pool, &id).await.unwrap();
        assert!(std::fs::symlink_metadata(&link.central_path).is_err());
    }
    #[tokio::test]
    async fn address_migration_moves_directory_preserves_relations_and_syncs_in_new_directory() {
        let (_dir, pool, snapshot) = fixture().await;
        let id = persist_snapshot(&pool, &snapshot).await.unwrap();
        apply(&pool, &id).await.unwrap();
        let original = db::get_skill_by_id(&pool, "one").await.unwrap().unwrap();
        db::upsert_skill_metadata(&pool, "one", Some("keep note"), &["keep tag".into()])
            .await
            .unwrap();
        let collection = db::create_collection(&pool, "fixture", None).await.unwrap();
        db::add_skill_to_collection(&pool, &collection.id, "one")
            .await
            .unwrap();
        linker::install_resource_skill_to_agent_impl(&pool, "one", "claude-code", "copy")
            .await
            .unwrap();
        let link = linker::add_resource_skill_to_central_impl(&pool, "one")
            .await
            .unwrap();
        let mut notebook =
            super::super::notebook_repository::Repository::new(1, "example/fixtures".into());
        notebook.notebook = "keep notebook".into();
        super::super::notebook_repository::put(&pool, &notebook)
            .await
            .unwrap();
        linker::normalize_resource_backed_central_records_impl(&pool)
            .await
            .unwrap();
        let project = _dir.path().join("project");
        std::fs::create_dir_all(&project).unwrap();
        let project =
            db::add_scan_directory(&pool, &project.to_string_lossy(), Some("fixture project"))
                .await
                .unwrap();
        linker::install_resource_skill_to_agent_impl(
            &pool,
            "one",
            &format!("project:{}", project.id),
            "symlink",
        )
        .await
        .unwrap();
        linker::install_resource_skill_to_agent_impl(&pool, "one", "claude-code", "copy")
            .await
            .unwrap();
        let before_installs = db::get_skill_installations(&pool, "one").await.unwrap();
        let copy = before_installs
            .iter()
            .find(|item| item.link_type == "copy")
            .unwrap();
        std::fs::write(
            Path::new(&copy.installed_path).join("local-copy.txt"),
            "keep local copy until sync",
        )
        .unwrap();
        super::super::repository_address::migrate(&pool, "example/fixtures", "new-owner/new-repo")
            .await
            .unwrap();
        assert_eq!(apply(&pool, &id).await.unwrap_err(), "PREVIEW_EXPIRED");
        assert_eq!(
            db::get_skill_installations(&pool, "one")
                .await
                .unwrap()
                .len(),
            before_installs.len()
        );
        let root = db::get_skill_resource_library_dir(&pool).await.unwrap();
        let moved = root.join("new-owner").join("new-repo").join("one");
        assert!(!Path::new(original.canonical_path.as_ref().unwrap()).exists());
        assert!(moved.join("SKILL.md").is_file());
        assert!(Path::new(&copy.installed_path)
            .join("local-copy.txt")
            .is_file());
        for install in &before_installs {
            assert!(Path::new(&install.installed_path)
                .join("SKILL.md")
                .is_file());
            if install.link_type == "symlink" {
                assert!(crate::path_utils::paths_resolve_to_same_entry(
                    Path::new(&install.installed_path),
                    &moved
                ));
            }
        }
        assert_eq!(
            std::fs::read(root.join("new-owner/new-repo/README.md")).unwrap(),
            b"remote docs"
        );
        assert!(skills::get_central_skills_impl(&pool)
            .await
            .unwrap()
            .iter()
            .any(|skill| skill.id == "one"));
        let saved = super::super::notebook_repository::get(&pool, 1)
            .await
            .unwrap();
        assert_eq!(saved.repository_uid, notebook.repository_uid);
        assert_eq!(saved.notebook, notebook.notebook);
        assert_eq!(saved.full_name, "new-owner/new-repo");
        let mut next = capture(&pool, snapshot).await;
        next.repo.owner = "new-owner".into();
        next.repo.repo = "new-repo".into();
        next.repo.normalized_url = "https://github.com/new-owner/new-repo".into();
        next.originals = remote_sources::github_repo_source_groups(&pool, false)
            .await
            .unwrap()
            .remove("new-owner/new-repo")
            .unwrap();
        next.contents
            .files
            .insert("skills/one/extra.txt".into(), b"new file".to_vec());
        next.contents.files.insert(
            "skills/three/SKILL.md".into(),
            b"---\nname: three\ndescription: third\n---\nnew".to_vec(),
        );
        next.contents
            .files
            .insert("README.md".into(), b"new docs".to_vec());
        let id = persist_snapshot(&pool, &next).await.unwrap();
        apply(&pool, &id).await.unwrap();
        let after = db::get_skill_by_id(&pool, "one").await.unwrap().unwrap();
        assert_eq!(
            after.canonical_path.as_deref(),
            Some(moved.to_string_lossy().as_ref())
        );
        let metadata = db::get_skill_metadata(&pool, "one").await.unwrap();
        assert_eq!(
            db::parse_skill_metadata_tags(metadata.as_ref()),
            vec!["keep tag"]
        );
        let count: i64 = pool
            .read(|d| {
                Ok(d.table("collection_skills")
                    .values()
                    .filter(|r| r["skill_id"] == "one" && r["collection_id"] == collection.id)
                    .count() as i64)
            })
            .unwrap();
        assert_eq!(count, 1);
        assert_eq!(
            std::fs::read(Path::new(&link.central_path).join("extra.txt")).unwrap(),
            b"new file"
        );
        for install in db::get_skill_installations(&pool, "one").await.unwrap() {
            assert_eq!(
                std::fs::read(Path::new(&install.installed_path).join("extra.txt")).unwrap(),
                b"new file"
            );
        }
        let container = moved.parent().unwrap();
        assert_eq!(
            std::fs::read(container.join("README.md")).unwrap(),
            b"new docs"
        );
        let third = db::get_skill_by_id(&pool, "three").await.unwrap().unwrap();
        assert_eq!(
            Path::new(third.canonical_path.as_ref().unwrap())
                .parent()
                .unwrap(),
            container
        );
        assert!(!root.join("example/fixtures").exists());
        assert!(!Path::new(&copy.installed_path)
            .join("local-copy.txt")
            .exists());
    }

    #[tokio::test]
    async fn address_migration_rejects_conflicts_and_interrupted_sync_without_changes() {
        let (_dir, pool, snapshot) = fixture().await;
        let id = persist_snapshot(&pool, &snapshot).await.unwrap();
        apply(&pool, &id).await.unwrap();
        crate::storage::fixtures::insert(
            &pool,
            "repository_mirrors",
            serde_json::json!({"repository":"new/repo"}),
        )
        .await
        .unwrap();
        assert_eq!(
            super::super::repository_address::migrate(&pool, "example/fixtures", "new/repo")
                .await
                .unwrap_err(),
            "addressConflict"
        );
        pool.write(|d| {
            d.table_mut("repository_mirror_snapshots")
                .get_mut(&id)
                .unwrap()["state"] = "applying".into();
            Ok(())
        })
        .await
        .unwrap();
        assert_eq!(
            super::super::repository_address::migrate(&pool, "EXAMPLE/FIXTURES", "other/repo")
                .await
                .unwrap_err(),
            "addressBusy"
        );
        assert_eq!(
            db::get_skill_source(&pool, "one")
                .await
                .unwrap()
                .unwrap()
                .source_repo
                .as_deref(),
            Some("example/fixtures")
        );
        let count: i64 =
            crate::storage::fixtures::count(&pool, "repository_local_directories", None);
        assert_eq!(count, 0);
        pool.write(|d| {
            d.table_mut("repository_mirror_snapshots")
                .get_mut(&id)
                .unwrap()["state"] = "applied".into();
            Ok(())
        })
        .await
        .unwrap();
        pool.fail_table_write("repository_local_directories");
        assert_eq!(
            super::super::repository_address::migrate(&pool, "example/fixtures", "other/repo")
                .await
                .unwrap_err(),
            "addressDatabase"
        );
        assert_eq!(
            db::get_skill_source(&pool, "one")
                .await
                .unwrap()
                .unwrap()
                .source_repo
                .as_deref(),
            Some("example/fixtures")
        );
        let count: i64 = crate::storage::fixtures::count(
            &pool,
            "repository_mirrors",
            Some(("repository", "example/fixtures")),
        );
        assert_eq!(count, 1);
        let root = db::get_skill_resource_library_dir(&pool).await.unwrap();
        assert!(root.join("example/fixtures/one/SKILL.md").is_file());
        assert!(!root.join("other/repo").exists());
        let count: i64 = crate::storage::fixtures::count(&pool, "repository_address_moves", None);
        assert_eq!(count, 0);
    }

    #[tokio::test]
    async fn address_migration_supports_empty_repository_and_repeated_moves() {
        let (_dir, pool, mut snapshot) = fixture().await;
        snapshot.contents.files.clear();
        let id = persist_snapshot(&pool, &snapshot).await.unwrap();
        apply(&pool, &id).await.unwrap();
        super::super::repository_address::migrate(&pool, "example/fixtures", "next/repo")
            .await
            .unwrap();
        super::super::repository_address::migrate(&pool, "next/repo", "third/repo")
            .await
            .unwrap();
        let root = db::get_skill_resource_library_dir(&pool).await.unwrap();
        assert_eq!(
            super::super::repository_address::directory(&pool, &root, "third", "third/repo")
                .await
                .unwrap(),
            root.join("third").join("repo")
        );
    }

    #[tokio::test]
    async fn address_migration_rejects_existing_directory_without_touching_files() {
        let (_dir, pool, snapshot) = fixture().await;
        let id = persist_snapshot(&pool, &snapshot).await.unwrap();
        apply(&pool, &id).await.unwrap();
        let root = db::get_skill_resource_library_dir(&pool).await.unwrap();
        std::fs::create_dir_all(root.join("occupied/repo")).unwrap();
        std::fs::write(root.join("occupied/repo/keep.txt"), "unrelated").unwrap();
        assert_eq!(
            super::super::repository_address::migrate(&pool, "example/fixtures", "occupied/repo")
                .await
                .unwrap_err(),
            "addressConflict"
        );
        assert_eq!(
            std::fs::read(root.join("occupied/repo/keep.txt")).unwrap(),
            b"unrelated"
        );
        assert!(root.join("example/fixtures/one/SKILL.md").is_file());
    }

    #[tokio::test]
    async fn address_migration_recovers_precommit_rename_before_scan() {
        let (_dir, pool, snapshot) = fixture().await;
        let id = persist_snapshot(&pool, &snapshot).await.unwrap();
        apply(&pool, &id).await.unwrap();
        let root = db::get_skill_resource_library_dir(&pool).await.unwrap();
        let payload = serde_json::json!({"old":"example/fixtures","new":"new/repo","from":"example/fixtures","to":"new/repo","had_directory":true,"links":[]});
        crate::storage::fixtures::insert(
            &pool,
            "repository_address_moves",
            serde_json::json!({"id":"fixture","payload":payload.to_string()}),
        )
        .await
        .unwrap();
        std::fs::create_dir_all(root.join("new")).unwrap();
        std::fs::rename(root.join("example/fixtures"), root.join("new/repo")).unwrap();
        assert!(
            matches!(skills::get_resource_library_skills_impl(&pool).await, Err(error) if error == "addressRecovery")
        );
        assert!(db::get_skill_by_id(&pool, "one").await.unwrap().is_some());
        super::super::repository_address::recover(&pool)
            .await
            .unwrap();
        let skills = skills::get_resource_library_skills_impl(&pool)
            .await
            .unwrap();
        assert_eq!(skills.len(), 2);
        assert!(skills.iter().any(|skill| skill.id == "one"));
        assert!(root.join("example/fixtures/one/SKILL.md").is_file());
        assert!(!root.join("new/repo").exists());
        assert!(super::super::repository_address::recover(&pool)
            .await
            .unwrap()
            .is_empty());
    }

    #[tokio::test]
    async fn address_migration_recovers_committed_move_and_interrupted_link_replacement() {
        let (_dir, pool, snapshot) = fixture().await;
        let id = persist_snapshot(&pool, &snapshot).await.unwrap();
        apply(&pool, &id).await.unwrap();
        let link =
            linker::install_resource_skill_to_agent_impl(&pool, "one", "claude-code", "symlink")
                .await
                .unwrap();
        let root = db::get_skill_resource_library_dir(&pool).await.unwrap();
        let old = root.join("example").join("fixtures").join("one");
        let new = root.join("new").join("repo").join("one");
        let links = linker::prepare_installation_relocation_impl(&pool, "one", &old, &new)
            .await
            .unwrap();
        let payload = serde_json::json!({"old":"example/fixtures","new":"new/repo","from":"example/fixtures","to":"new/repo","had_directory":true,"links":links});
        std::fs::create_dir_all(root.join("new")).unwrap();
        std::fs::rename(root.join("example/fixtures"), root.join("new/repo")).unwrap();
        pool.write(|d| {
            let row = d.table_mut("skills").get_mut("one").unwrap();
            row["canonical_path"] = serde_json::json!(new.to_string_lossy());
            row["file_path"] = serde_json::json!(new.join("SKILL.md").to_string_lossy());
            Ok(())
        })
        .await
        .unwrap();
        crate::storage::fixtures::insert(
            &pool,
            "repository_address_moves",
            serde_json::json!({"id":"fixture","payload":payload.to_string(),"committed":true}),
        )
        .await
        .unwrap();
        crate::path_utils::remove_symlink_path(Path::new(&link.symlink_path)).unwrap();
        // A foreign entry is never removed by recovery.
        std::fs::create_dir(&link.symlink_path).unwrap();
        std::fs::write(Path::new(&link.symlink_path).join("keep.txt"), "foreign").unwrap();
        assert_eq!(
            super::super::repository_address::recover(&pool)
                .await
                .unwrap_err(),
            "addressRecovery"
        );
        assert_eq!(
            std::fs::read(Path::new(&link.symlink_path).join("keep.txt")).unwrap(),
            b"foreign"
        );
        std::fs::remove_file(Path::new(&link.symlink_path).join("keep.txt")).unwrap();
        std::fs::remove_dir(&link.symlink_path).unwrap();
        assert_eq!(
            super::super::repository_address::recover(&pool)
                .await
                .unwrap(),
            vec![("example/fixtures".into(), "new/repo".into())]
        );
        assert!(crate::path_utils::paths_resolve_to_same_entry(
            Path::new(&link.symlink_path),
            &new
        ));
        let records = db::get_skill_installations(&pool, "one").await.unwrap();
        assert_eq!(records[0].installed_path, link.symlink_path);
        assert_eq!(
            records[0].symlink_target.as_deref(),
            Some(new.to_string_lossy().as_ref())
        );
        assert!(!root.join("example/fixtures").exists());
    }

    #[test]
    fn rejects_case_collisions_and_escaping_paths() {
        let contents = github::GitHubRepoSnapshot {
            files: [("../outside".into(), vec![])].into(),
        };
        let snapshot = Snapshot {
            version: 1,
            repo: github::GitHubRepoRef {
                owner: "example".into(),
                repo: "fixtures".into(),
                branch: "a".repeat(40),
                normalized_url: String::new(),
                stars: None,
            },
            contents,
            originals: vec![],
            installations: vec![],
        };
        assert!(validate_snapshot(&snapshot).is_err());
        let mut snapshot = snapshot;
        snapshot.contents.files =
            [("README.md".into(), vec![]), ("readme.md".into(), vec![])].into();
        assert!(validate_snapshot(&snapshot).is_err());
    }
}

async fn save_recovery(
    pool: &AppStore,
    id: &str,
    group: &[(db::SkillSource, db::Skill)],
) -> Result<(), String> {
    if pool.read(|d| Ok(d.table("repository_mirror_recovery").contains_key(id)))? {
        return Ok(());
    }
    let mut items = Vec::new();
    for (source, skill) in group {
        let path = Path::new(&skill.file_path)
            .parent()
            .ok_or("Invalid skill directory")?;
        let files = if path.exists() {
            remote_sources::local_skill_files(path)?
        } else {
            BTreeMap::new()
        };
        let installations = db::get_skill_installations(pool, &skill.id).await?;
        let mut copies = Vec::new();
        for installation in &installations {
            if installation.link_type == "copy" && Path::new(&installation.installed_path).exists()
            {
                copies.push(serde_json::json!({"installation":installation,"files":remote_sources::local_skill_files(Path::new(&installation.installed_path))?}));
            }
        }
        let metadata = pool.read(|d| Ok(d.table("skill_metadata").get(&skill.id).cloned()))?;
        let collections = pool.read(|d| {
            Ok(d.table("collection_skills")
                .values()
                .filter(|r| r["skill_id"] == skill.id)
                .filter_map(|r| r["collection_id"].as_str().map(str::to_owned))
                .collect::<Vec<_>>())
        })?;
        items.push(serde_json::json!({"source":source,"skill":skill,"files":files,"installations":installations,"copies":copies,"metadata":metadata,"collections":collections}));
    }
    let payload = serde_json::to_string(&items).map_err(|e| e.to_string())?;
    pool.write(|d| {
        d.table_mut("repository_mirror_recovery").insert(
            id.to_owned(),
            serde_json::json!({"snapshot_id":id,"payload":payload}),
        );
        Ok(())
    })
    .await?;
    Ok(())
}

/// Released reports no longer authorize writes. Never evict an unfinished mirror or recovery data.
#[tauri::command]
pub async fn release_repository_snapshots(
    state: State<'_, AppState>,
    snapshot_ids: Vec<String>,
) -> Result<(), MirrorError> {
    let _guard = MIRROR_LOCK.lock().await;
    release_snapshots_locked(&state.db, &snapshot_ids)
        .await
        .map_err(MirrorError::from)
}
async fn release_snapshots_locked(pool: &AppStore, snapshot_ids: &[String]) -> Result<(), String> {
    if snapshot_ids.is_empty() {
        return Ok(());
    }
    pool.write(|d| {
        for id in snapshot_ids {
            if d.table("repository_mirror_snapshots")
                .get(id)
                .is_some_and(|r| r["state"] != "applying")
                && !d.table("repository_mirror_recovery").contains_key(id)
            {
                d.table_mut("repository_mirror_snapshots").remove(id);
            }
        }
        Ok(())
    })
    .await
}

async fn set_snapshot_state(pool: &AppStore, id: &str, state: &str) -> Result<(), String> {
    pool.write(|d| {
        if let Some(row) = d.table_mut("repository_mirror_snapshots").get_mut(id) {
            row["state"] = state.into();
        }
        Ok(())
    })
    .await
}
