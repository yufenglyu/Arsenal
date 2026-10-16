use super::repository_docs;
use chrono::Utc;
use flate2::read::GzDecoder;
use serde::{Deserialize, Serialize};
use sqlx::Row;
use std::collections::{HashMap, HashSet};
use std::fmt;
use std::io::{Cursor, Read};
use std::path::{Component, Path, PathBuf};
use tauri::{AppHandle, Emitter, State};

use crate::{
    db::{self, DbPool, Skill},
    path_utils::{paths_resolve_to_same_entry, source_grouped_skill_dir},
    AppState,
};

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GitHubRepoRef {
    pub owner: String,
    pub repo: String,
    pub branch: String,
    pub normalized_url: String,
    #[serde(default)]
    pub stars: Option<i64>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DuplicateResolution {
    Overwrite,
    Skip,
    Rename,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GitHubSkillConflict {
    pub existing_skill_id: String,
    pub existing_name: String,
    pub existing_canonical_path: Option<String>,
    pub proposed_skill_id: String,
    pub proposed_name: String,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GitHubSkillPreview {
    pub source_path: String,
    pub skill_id: String,
    pub skill_name: String,
    pub description: Option<String>,
    pub root_directory: String,
    pub skill_directory_name: String,
    pub download_url: String,
    pub conflict: Option<GitHubSkillConflict>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GitHubRepoPreview {
    pub repo: GitHubRepoRef,
    pub skills: Vec<GitHubSkillPreview>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GitHubSkillImportSelection {
    pub source_path: String,
    pub resolution: DuplicateResolution,
    pub renamed_skill_id: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ImportedGitHubSkillSummary {
    pub source_path: String,
    pub original_skill_id: String,
    pub imported_skill_id: String,
    pub skill_name: String,
    pub target_directory: String,
    pub resolution: DuplicateResolution,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GitHubRepoImportResult {
    pub repo: GitHubRepoRef,
    pub imported_skills: Vec<ImportedGitHubSkillSummary>,
    pub skipped_skills: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GitHubSnapshotImportRequest {
    pub input: String,
    pub skill: Option<String>,
    pub overwrite: bool,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum GitHubImportProgressPhase {
    Preparing,
    Writing,
    Finalizing,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GitHubImportProgressPayload {
    pub operation_id: Option<String>,
    pub phase: GitHubImportProgressPhase,
    pub current_skill: Option<String>,
    pub current_path: Option<String>,
    pub completed_files: usize,
    pub total_files: usize,
    pub completed_bytes: u64,
    pub total_bytes: u64,
}

#[derive(Debug, Deserialize)]
struct SkillFrontmatter {
    name: String,
    description: Option<String>,
}

#[derive(Debug, Clone)]
pub(crate) struct RemoteSkillCandidate {
    pub(crate) source_path: String,
    pub(crate) source_manifest_path: String,
    pub(crate) skill_id: String,
    pub(crate) skill_name: String,
    pub(crate) description: Option<String>,
    pub(crate) root_directory: String,
    pub(crate) skill_directory_name: String,
    pub(crate) download_url: String,
}

#[derive(Debug, Clone, Default)]
struct GitHubRepoSnapshot {
    files: HashMap<String, Vec<u8>>,
}

const GITHUB_PAT_SETTING_KEY: &str = "github_pat";

#[derive(Debug, Clone, PartialEq, Eq)]
enum GitHubAccessDenialKind {
    RateLimited {
        reset_at: Option<String>,
        remaining: Option<String>,
    },
    AuthenticationOrPermission,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct GitHubAccessDenial {
    kind: GitHubAccessDenialKind,
    operation: &'static str,
    status: reqwest::StatusCode,
    github_message: Option<String>,
}

impl fmt::Display for GitHubAccessDenial {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let status = self.status.as_u16();
        match &self.kind {
            GitHubAccessDenialKind::RateLimited {
                reset_at,
                remaining,
            } => {
                write!(
                    f,
                    "GitHub API access was denied while {} because the rate limit was exceeded (HTTP {}). Retry later",
                    self.operation, status
                )?;
                if let Some(reset_at) = reset_at {
                    write!(f, " after {} UTC", reset_at)?;
                }
                write!(f, " or use authenticated GitHub requests")?;
                if let Some(remaining) = remaining {
                    write!(f, " (remaining quota: {})", remaining)?;
                }
                if let Some(message) = &self.github_message {
                    write!(f, ". GitHub said: {}", message)?;
                } else {
                    write!(f, ".")?;
                }
                Ok(())
            }
            GitHubAccessDenialKind::AuthenticationOrPermission => {
                write!(
                    f,
                    "GitHub denied access while {} (HTTP {}). The repository may require authentication, your API quota may need authenticated requests, or the token/permissions are insufficient. Verify repository access, sign in with a GitHub token that can read the repo, or retry later",
                    self.operation, status
                )?;
                if let Some(message) = &self.github_message {
                    write!(f, ". GitHub said: {}", message)?;
                } else {
                    write!(f, ".")?;
                }
                Ok(())
            }
        }
    }
}

#[derive(Debug, Deserialize)]
struct GitHubErrorResponse {
    message: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum GitHubFetchSurface {
    Api,
    Raw,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct MirrorAttemptOutcome {
    status: Option<reqwest::StatusCode>,
    error_message: String,
}

#[derive(Debug, Clone, Copy)]
struct GitHubMirrorEndpoint {
    label: &'static str,
    api_base: &'static str,
    raw_base: &'static str,
}

const GITHUB_MIRROR_ENDPOINTS: &[GitHubMirrorEndpoint] = &[
    GitHubMirrorEndpoint {
        label: "github",
        api_base: "https://api.github.com",
        raw_base: "https://raw.githubusercontent.com",
    },
    GitHubMirrorEndpoint {
        label: "ghfast",
        api_base: "https://ghfast.top/https://api.github.com",
        raw_base: "https://ghfast.top/https://raw.githubusercontent.com",
    },
    GitHubMirrorEndpoint {
        label: "ghproxy",
        api_base: "https://ghproxy.net/https://api.github.com",
        raw_base: "https://ghproxy.net/https://raw.githubusercontent.com",
    },
    GitHubMirrorEndpoint {
        label: "gitproxy",
        api_base: "https://mirror.ghproxy.com/https://api.github.com",
        raw_base: "https://mirror.ghproxy.com/https://raw.githubusercontent.com",
    },
];

#[tauri::command]
pub async fn preview_github_repo_import(
    state: State<'_, AppState>,
    repo_url: String,
    operation_id: Option<String>,
) -> Result<GitHubRepoPreview, String> {
    if let Some(id) = operation_id {
        let operation = super::github_cancellation::Operation::start(id);
        operation.during(preview_github_repo_import_impl(&state.db, &repo_url)).await
    } else {
        preview_github_repo_import_impl(&state.db, &repo_url).await
    }
}

#[tauri::command]
pub async fn import_github_repo_skills(
    app: AppHandle,
    state: State<'_, AppState>,
    repo_url: String,
    selections: Vec<GitHubSkillImportSelection>,
    operation_id: Option<String>,
) -> Result<GitHubRepoImportResult, String> {
    let operation = operation_id.map(super::github_cancellation::Operation::start);
    import_github_repo_skills_cancellable(&state.db, &repo_url, selections, Some(&app), operation.as_ref()).await
}

#[tauri::command]
pub async fn import_github_repo_snapshot(
    app: AppHandle,
    state: State<'_, AppState>,
    input: GitHubSnapshotImportRequest,
) -> Result<GitHubRepoImportResult, String> {
    import_github_repo_snapshot_impl(&state.db, input, Some(&app)).await
}

#[tauri::command]
pub async fn fetch_github_skill_markdown(
    state: State<'_, AppState>,
    download_url: String,
) -> Result<String, String> {
    let client = github_client()?;
    let auth = github_direct_auth_from_settings(&state.db).await?;
    fetch_raw_text(&client, &download_url, auth.as_deref()).await
}

async fn preview_github_repo_import_impl(
    pool: &DbPool,
    repo_url: &str,
) -> Result<GitHubRepoPreview, String> {
    let auth = github_direct_auth_from_settings(pool).await?;
    let mut repo = resolve_repo_ref(repo_url, auth.as_deref()).await?;
    // Preview and import share exactly the same immutable archive.
    if !is_commit_ref(&repo.branch) {
        repo.branch = fetch_repo_head_ref(&repo, auth.as_deref()).await?;
    }
    cache_repository_stars(pool, &repo).await?;
    let candidates = fetch_repo_skill_candidates(&repo, auth.as_deref()).await?;
    let skills = build_preview_skills(pool, &repo, &candidates).await?;

    if skills.is_empty() {
        return Err(
            "No importable skills found in this repository. Supported layouts are repo-root skill directories, a skills/ or skill-package/ directory, or plugin skills under plugins/*/skills/."
                .to_string(),
        );
    }

    Ok(GitHubRepoPreview { repo, skills })
}

pub(crate) async fn import_github_repo_snapshot_impl(
    pool: &DbPool,
    input: GitHubSnapshotImportRequest,
    app: Option<&AppHandle>,
) -> Result<GitHubRepoImportResult, String> {
    let repo_url = normalize_github_repo_input(&input.input)?;
    let preview = preview_github_repo_import_impl(pool, &repo_url).await?;
    let filter = input
        .skill
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let mut selections = Vec::new();

    for skill in preview.skills {
        if let Some(filter) = filter {
            let matches_filter = skill.skill_id.eq_ignore_ascii_case(filter)
                || skill.skill_directory_name.eq_ignore_ascii_case(filter)
                || skill.source_path.eq_ignore_ascii_case(filter)
                || skill
                    .source_path
                    .eq_ignore_ascii_case(filter.trim_end_matches("/SKILL.md"));
            if !matches_filter {
                continue;
            }
        }
        selections.push(GitHubSkillImportSelection {
            source_path: skill.source_path,
            resolution: if skill.conflict.is_some() && !input.overwrite {
                DuplicateResolution::Skip
            } else {
                DuplicateResolution::Overwrite
            },
            renamed_skill_id: None,
        });
    }

    if selections.is_empty() {
        if let Some(filter) = filter {
            return Err(format!(
                "Skill '{filter}' was not found in this repository."
            ));
        }
        return Err("No importable skills were selected.".to_string());
    }

    import_github_repo_skills_impl(pool, &repo_url, selections, app).await
}

pub(crate) async fn import_github_repo_skills_impl(
    pool: &DbPool,
    repo_url: &str,
    selections: Vec<GitHubSkillImportSelection>,
    app: Option<&AppHandle>,
) -> Result<GitHubRepoImportResult, String> {
    import_github_repo_skills_cancellable(pool, repo_url, selections, app, None).await
}

async fn import_github_repo_skills_cancellable(
    pool: &DbPool,
    repo_url: &str,
    selections: Vec<GitHubSkillImportSelection>,
    app: Option<&AppHandle>,
    operation: Option<&super::github_cancellation::Operation>,
) -> Result<GitHubRepoImportResult, String> {
    emit_github_import_progress(
        app,
        GitHubImportProgressPayload {
            operation_id: operation.map(|operation| operation.id().to_string()),
            phase: GitHubImportProgressPhase::Preparing,
            current_skill: None,
            current_path: None,
            completed_files: 0,
            total_files: 0,
            completed_bytes: 0,
            total_bytes: 0,
        },
    );

    let prepare = async {
        let auth = github_direct_auth_from_settings(pool).await?;
        let mut repo = resolve_repo_ref(repo_url, auth.as_deref()).await?;
        if !is_commit_ref(&repo.branch) {
            repo.branch = fetch_repo_head_ref(&repo, auth.as_deref()).await?;
        }
        cache_repository_stars(pool, &repo).await?;
        let client = github_client()?;
        let snapshot = download_repo_snapshot(&client, &repo, auth.as_deref()).await?;
        Ok((repo, snapshot))
    };
    let (repo, snapshot) = if let Some(operation) = operation {
        operation.during(prepare).await?
    } else {
        prepare.await?
    };
    let candidates = build_repo_skill_candidates_from_snapshot(&repo, &snapshot)?;
    if candidates.is_empty() {
        return Err(
            "No importable skills found in this repository. Supported layouts are repo-root skill directories, a skills/ or skill-package/ directory, or plugin skills under plugins/*/skills/."
                .to_string(),
        );
    }

    if selections.is_empty() {
        return Err("Select at least one skill to import.".to_string());
    }

    let mut selected_paths = HashSet::new();
    let mut selected = Vec::new();
    for selection in selections {
        let candidate = candidates
            .iter()
            .find(|candidate| candidate.source_path == selection.source_path)
            .ok_or_else(|| {
                format!(
                    "Selected skill '{}' is no longer available in the preview.",
                    selection.source_path
                )
            })?
            .clone();

        if !selected_paths.insert(candidate.source_path.clone()) {
            return Err(format!(
                "Skill '{}' was selected more than once.",
                candidate.source_path
            ));
        }

        selected.push((candidate, selection));
    }

    let resource_root = skill_resource_library_root(pool).await?;
    std::fs::create_dir_all(&resource_root)
        .map_err(|e| format!("Failed to create skill resource library directory: {}", e))?;

    let mut occupied_ids = current_managed_skill_ids(pool).await?;
    let mut staging_ops = Vec::new();
    let mut skipped_skills = Vec::new();

    for (candidate, selection) in &selected {
        if let Some(operation) = operation { operation.check()?; }
        match selection.resolution {
            DuplicateResolution::Skip => {
                skipped_skills.push(candidate.source_path.clone());
                continue;
            }
            DuplicateResolution::Overwrite => {
                let source_repo = format!("{}/{}", repo.owner, repo.repo);
                let target_dir = source_grouped_skill_dir(
                    &resource_root,
                    Some(&repo.owner),
                    Some(&source_repo),
                    None,
                    &candidate.skill_directory_name,
                );
                let (final_skill_id, target_directory_name) = if let Some(id) = selection.renamed_skill_id.as_deref() {
                    resolve_update_target(pool, id, &source_repo, &resource_root, &repo.owner).await?
                } else {
                    (resolve_import_skill_id(pool, candidate, &mut occupied_ids, &target_dir, &repo.owner, &repo.repo).await?, candidate.skill_directory_name.clone())
                };
                if final_skill_id == candidate.skill_id {
                    occupied_ids.insert(final_skill_id.clone());
                }
                if let Some(existing) = db::get_skill_by_id(pool, &final_skill_id).await? {
                    let existing_canonical_is_live = existing
                        .canonical_path
                        .as_deref()
                        .is_some_and(canonical_skill_path_exists);
                    if existing_canonical_is_live && existing.is_central {
                        return Err(format!(
                            "Skill '{}' conflicts with an existing record and cannot be overwritten safely. Rename it to import into the resource library.",
                            candidate.skill_id
                        ));
                    }
                }
                staging_ops.push(StagedImport {
                    candidate: candidate.clone(),
                    final_skill_id,
                    target_directory_name,
                    resolution: DuplicateResolution::Overwrite,
                    source_files: Vec::new(),
                });
            }
            DuplicateResolution::Rename => {
                let requested_id =
                    sanitize_skill_id(selection.renamed_skill_id.as_deref().ok_or_else(|| {
                        format!(
                            "Skill '{}' requires a renamed skill id for rename resolution.",
                            candidate.source_path
                        )
                    })?)?;
                if occupied_ids.contains(&requested_id) {
                    return Err(format!(
                        "Renamed skill id '{}' is already in use.",
                        requested_id
                    ));
                }
                occupied_ids.insert(requested_id.clone());
                staging_ops.push(StagedImport {
                    candidate: candidate.clone(),
                    final_skill_id: requested_id,
                    target_directory_name: candidate.skill_directory_name.clone(),
                    resolution: DuplicateResolution::Rename,
                    source_files: Vec::new(),
                });
            }
        }
    }

    if staging_ops.is_empty() && skipped_skills.is_empty() {
        return Err("No valid import operations were requested.".to_string());
    }

    for op in &mut staging_ops {
        op.source_files = collect_snapshot_source_files(&snapshot, &op.candidate.source_path)?;
    }

    let total_files = staging_ops
        .iter()
        .map(|op| op.source_files.len())
        .sum::<usize>();
    let total_bytes = staging_ops
        .iter()
        .flat_map(|op| op.source_files.iter())
        .map(|file| file.byte_len as u64)
        .sum::<u64>();
    let mut progress_state = GitHubImportProgressState {
        operation_id: operation.map(|operation| operation.id().to_string()),
        completed_files: 0,
        total_files,
        completed_bytes: 0,
        total_bytes,
    };

    emit_github_import_progress(
        app,
        GitHubImportProgressPayload {
            operation_id: progress_state.operation_id.clone(),
            phase: GitHubImportProgressPhase::Writing,
            current_skill: None,
            current_path: None,
            completed_files: 0,
            total_files,
            completed_bytes: 0,
            total_bytes,
        },
    );

    // Prepare every skill before replacing any existing directory.
    let mut files_transaction = ImportDirectoryTransaction::new(&resource_root)?;
    let mut prepared = Vec::new();
    for (index, op) in staging_ops.iter().enumerate() {
        if let Some(operation) = operation { operation.check()?; }
        let source_repo = format!("{}/{}", repo.owner, repo.repo);
        let target_dir = source_grouped_skill_dir(
            &resource_root,
            Some(&repo.owner),
            Some(&source_repo),
            None,
            &op.target_directory_name,
        );
        let staged_dir = files_transaction.root.join(format!("new-{index}"));
        write_snapshot_source_to_target(
            &snapshot,
            &op.source_files,
            &staged_dir,
            &op.candidate.source_path,
            &mut progress_state,
            app,
        )?;
        let raw = std::fs::read_to_string(staged_dir.join("SKILL.md"))
            .map_err(|e| format!("Failed to read staged SKILL.md: {e}"))?;
        let frontmatter = parse_frontmatter(&raw).ok_or_else(|| {
            format!(
                "Imported skill '{}' is missing valid frontmatter.",
                op.candidate.source_path
            )
        })?;
        prepared.push((op, target_dir, staged_dir, frontmatter));
    }

    if let Some(operation) = operation { operation.check()?; }
    // Once installation starts, finish the atomic file/database commit before stopping.
    let mut imported_skills = Vec::new();
    let mut database_transaction = pool.begin().await.map_err(|e| e.to_string())?;
    for (op, target_dir, staged_dir, frontmatter) in prepared {
        emit_github_import_progress(app, GitHubImportProgressPayload {
            operation_id: progress_state.operation_id.clone(),
            phase: GitHubImportProgressPhase::Finalizing,
            current_skill: Some(op.candidate.source_path.clone()),
            current_path: None,
            completed_files: progress_state.completed_files,
            total_files: progress_state.total_files,
            completed_bytes: progress_state.completed_bytes,
            total_bytes: progress_state.total_bytes,
        });
        files_transaction.install(
            &staged_dir,
            &target_dir,
            op.resolution == DuplicateResolution::Overwrite,
        )?;
        let skill_md_path = target_dir.join("SKILL.md");
        let db_skill = Skill {
            id: op.final_skill_id.clone(),
            name: frontmatter.name.clone(),
            description: frontmatter.description.clone(),
            file_path: skill_md_path.to_string_lossy().into_owned(),
            canonical_path: Some(target_dir.to_string_lossy().into_owned()),
            is_central: false,
            source: Some(format!("github:{}/{}", repo.owner, repo.repo)),
            content: None,
            scanned_at: Utc::now().to_rfc3339(),
        };
        db::upsert_skill_with_executor(&mut *database_transaction, &db_skill).await?;
        db::upsert_skill_source_with_executor(
            &mut *database_transaction,
            &db::SkillSource {
                skill_id: op.final_skill_id.clone(),
                source_type: "github".to_string(),
                source_url: Some(op.candidate.download_url.clone()),
                source_author: Some(repo.owner.clone()),
                source_repo: Some(format!("{}/{}", repo.owner, repo.repo)),
                source_path: Some(op.candidate.source_manifest_path.clone()),
                updated_at: Utc::now().to_rfc3339(),
            },
        )
        .await?;

        imported_skills.push(ImportedGitHubSkillSummary {
            source_path: op.candidate.source_path.clone(),
            original_skill_id: op.candidate.skill_id.clone(),
            imported_skill_id: op.final_skill_id.clone(),
            skill_name: frontmatter.name,
            target_directory: target_dir.to_string_lossy().into_owned(),
            resolution: op.resolution.clone(),
        });
    }

    if !imported_skills.is_empty() {
        let repository_dir = source_grouped_skill_dir(&resource_root, Some(&repo.owner),
            Some(&format!("{}/{}", repo.owner, repo.repo)), None, "placeholder")
            .parent().ok_or("Repository directory is missing")?.to_path_buf();
        stage_repository_readme(&snapshot, &repository_dir, &mut files_transaction)?;
    }

    database_transaction
        .commit()
        .await
        .map_err(|e| e.to_string())?;
    files_transaction.commit();

    emit_github_import_progress(
        app,
        GitHubImportProgressPayload {
            operation_id: progress_state.operation_id.clone(),
            phase: GitHubImportProgressPhase::Finalizing,
            current_skill: None,
            current_path: None,
            completed_files: progress_state.completed_files,
            total_files: progress_state.total_files,
            completed_bytes: progress_state.completed_bytes,
            total_bytes: progress_state.total_bytes,
        },
    );

    Ok(GitHubRepoImportResult {
        repo,
        imported_skills,
        skipped_skills,
    })
}

#[derive(Debug, Clone)]
struct StagedImport {
    candidate: RemoteSkillCandidate,
    final_skill_id: String,
    target_directory_name: String,
    resolution: DuplicateResolution,
    source_files: Vec<SnapshotSourceFile>,
}

/// Old directories remain recoverable until all file swaps and database writes succeed.
pub(crate) struct ImportDirectoryTransaction {
    pub(crate) root: PathBuf,
    swaps: Vec<(PathBuf, Option<PathBuf>)>,
    committed: bool,
}

impl ImportDirectoryTransaction {
    pub(crate) fn new(resource_root: &Path) -> Result<Self, String> {
        let parent = resource_root
            .parent()
            .ok_or("Resource library has no parent directory")?;
        let root = parent.join(format!(".skillshub-import-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&root).map_err(|e| format!("Failed to stage skill update: {e}"))?;
        Ok(Self {
            root,
            swaps: Vec::new(),
            committed: false,
        })
    }

    pub(crate) fn install(&mut self, staged: &Path, target: &Path, overwrite: bool) -> Result<(), String> {
        let parent = target
            .parent()
            .ok_or("Import target has no parent directory")?;
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        let previous = match std::fs::symlink_metadata(target) {
            Ok(metadata) => {
                if !overwrite || metadata.file_type().is_symlink()
                    || metadata.is_dir() != staged.is_dir() || metadata.is_file() != staged.is_file() {
                    return Err(
                        "Import target cannot be safely replaced; existing files were preserved"
                            .to_string(),
                    );
                }
                let previous = self.root.join(format!("old-{}", self.swaps.len()));
                std::fs::rename(target, &previous)
                    .map_err(|e| format!("Failed to preserve previous skill directory: {e}"))?;
                Some(previous)
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
            Err(error) => return Err(error.to_string()),
        };
        self.swaps.push((target.to_path_buf(), previous));
        std::fs::rename(staged, target).map_err(|e| format!("Failed to install staged skill: {e}"))
    }

    pub(crate) fn remove_file(&mut self, target: &Path) -> Result<(), String> {
        match std::fs::symlink_metadata(target) {
            Ok(metadata) if metadata.is_file() && !metadata.file_type().is_symlink() => {
                let previous = self.root.join(format!("old-{}", self.swaps.len()));
                std::fs::rename(target, &previous).map_err(|e| e.to_string())?;
                self.swaps.push((target.to_path_buf(), Some(previous)));
                Ok(())
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            _ => Err("Repository README cannot be safely replaced".into()),
        }
    }

    pub(crate) fn commit(&mut self) {
        self.committed = true;
    }
}

impl Drop for ImportDirectoryTransaction {
    fn drop(&mut self) {
        let mut restored = true;
        if !self.committed {
            for (target, previous) in self.swaps.iter().rev() {
                let removal = if target.is_dir() { std::fs::remove_dir_all(target) } else if target.exists() { std::fs::remove_file(target) } else { Ok(()) };
                if removal.is_err() {
                    restored = false;
                    continue;
                }
                if let Some(previous) = previous {
                    if std::fs::rename(previous, target).is_err() {
                        restored = false;
                    }
                }
            }
        }
        // Never erase recovery copies if the OS prevents rollback.
        if restored {
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }
}

async fn skill_resource_library_root(pool: &DbPool) -> Result<PathBuf, String> {
    db::get_skill_resource_library_dir(pool).await
}

async fn current_managed_skill_ids(pool: &DbPool) -> Result<HashSet<String>, String> {
    let rows =
        sqlx::query("SELECT id, canonical_path FROM skills WHERE canonical_path IS NOT NULL")
            .fetch_all(pool)
            .await
            .map_err(|e| e.to_string())?;
    Ok(rows
        .iter()
        .filter(|row| {
            row.get::<Option<String>, _>("canonical_path")
                .as_deref()
                .is_some_and(canonical_skill_path_exists)
        })
        .map(|row| row.get::<String, _>("id"))
        .collect::<HashSet<_>>())
}

// Explicit update targets retain their original identity and directory after renamed imports.
async fn resolve_update_target(pool: &DbPool, id: &str, repository: &str, resource_root: &Path, owner: &str) -> Result<(String, String), String> {
    let existing = db::get_skill_by_id(pool, id).await?.ok_or("Update target not found")?;
    let source = db::get_skill_source(pool, id).await?.ok_or("Update source not found")?;
    if existing.is_central || !source.source_repo.as_deref().is_some_and(|repo| repo.eq_ignore_ascii_case(repository)) {
        return Err("Update target does not belong to this repository".into());
    }
    let canonical = existing.canonical_path.as_deref().ok_or("Update target path not found")?;
    let target = Path::new(canonical);
    let name = target.file_name().and_then(|s|s.to_str()).ok_or("Invalid update target")?;
    let expected = source_grouped_skill_dir(resource_root, Some(owner), Some(repository), None, name);
    if !paths_resolve_to_same_entry(target, &expected) {
        return Err("Update target moved; import it again after resolving the conflict".into());
    }
    Ok((id.to_string(), name.to_string()))
}

async fn resolve_import_skill_id(
    pool: &DbPool,
    candidate: &RemoteSkillCandidate,
    occupied_ids: &mut HashSet<String>,
    target_dir: &Path,
    owner: &str,
    repo: &str,
) -> Result<String, String> {
    if let Some(existing) = db::get_resource_library_skills(pool).await?.into_iter().find(|skill| {
        skill.canonical_path.as_deref().is_some_and(|path| paths_resolve_to_same_entry(Path::new(path), target_dir))
    }) {
        occupied_ids.insert(existing.id.clone());
        return Ok(existing.id);
    }
    let Some(existing) = db::get_skill_by_id(pool, &candidate.skill_id).await? else {
        occupied_ids.insert(candidate.skill_id.clone());
        return Ok(candidate.skill_id.clone());
    };

    let existing_same_path = existing
        .canonical_path
        .as_deref()
        .is_some_and(|path| paths_resolve_to_same_entry(Path::new(path), target_dir));
    if existing_same_path {
        occupied_ids.insert(candidate.skill_id.clone());
        return Ok(candidate.skill_id.clone());
    }

    let source_repo = format!("{owner}-{repo}");
    let base = sanitize_skill_id(&format!("{source_repo}-{}", candidate.skill_id))?;
    let mut resolved = base.clone();
    let mut suffix = 2usize;
    while occupied_ids.contains(&resolved) || db::get_skill_by_id(pool, &resolved).await?.is_some()
    {
        resolved = format!("{base}-{suffix}");
        suffix += 1;
    }
    occupied_ids.insert(resolved.clone());
    Ok(resolved)
}

fn canonical_skill_path_exists(path: &str) -> bool {
    let canonical = Path::new(path);
    canonical.join("SKILL.md").is_file()
}

async fn build_preview_skills(
    pool: &DbPool,
    repo: &GitHubRepoRef,
    candidates: &[RemoteSkillCandidate],
) -> Result<Vec<GitHubSkillPreview>, String> {
    let resource_root = skill_resource_library_root(pool).await?;
    let existing_skills = db::get_resource_library_skills(pool).await?;
    let repository = format!("{}/{}", repo.owner, repo.repo);
    let mut skills = Vec::with_capacity(candidates.len());
    for candidate in candidates {
        // An ID collision elsewhere is resolved by namespacing during import.
        // Only files at the actual destination would be overwritten.
        let target = source_grouped_skill_dir(&resource_root, Some(&repo.owner), Some(&repository), None, &candidate.skill_directory_name);
        let existing = existing_skills.iter().find(|skill| skill.canonical_path.as_deref()
            .is_some_and(|path| paths_resolve_to_same_entry(Path::new(path), &target)));
        let conflict = target.join("SKILL.md").is_file().then(|| GitHubSkillConflict {
            existing_skill_id: existing.map(|skill| skill.id.clone()).unwrap_or_else(|| candidate.skill_id.clone()),
            existing_name: existing.map(|skill| skill.name.clone()).unwrap_or_else(|| candidate.skill_directory_name.clone()),
            existing_canonical_path: Some(target.to_string_lossy().into_owned()),
            proposed_skill_id: candidate.skill_id.clone(),
            proposed_name: candidate.skill_name.clone(),
        });
        skills.push(GitHubSkillPreview {
            source_path: candidate.source_path.clone(),
            skill_id: candidate.skill_id.clone(),
            skill_name: candidate.skill_name.clone(),
            description: candidate.description.clone(),
            root_directory: candidate.root_directory.clone(),
            skill_directory_name: candidate.skill_directory_name.clone(),
            download_url: candidate.download_url.clone(),
            conflict,
        });
    }
    Ok(skills)
}

pub(crate) async fn cache_repository_stars(pool: &DbPool, repo: &GitHubRepoRef) -> Result<(), String> {
    if let Some(stars) = repo.stars {
        db::save_github_stars(pool, &format!("{}/{}", repo.owner, repo.repo), stars).await?;
    }
    Ok(())
}

#[tauri::command]
pub async fn refresh_repository_stars(state: State<'_, AppState>, repository: String) -> Result<i64, String> {
    let auth = github_direct_auth_from_settings(&state.db).await?;
    let repo = resolve_repo_ref(&repository, auth.as_deref()).await?;
    let stars = repo.stars.ok_or_else(|| "Repository star count is unavailable".to_string())?;
    // Keep the requested source key working after a GitHub repository redirect.
    let (owner, name) = parse_github_url(&repository)?;
    db::save_github_stars(&state.db, &format!("{owner}/{name}"), stars).await?;
    cache_repository_stars(&state.db, &repo).await?;
    Ok(stars)
}

#[tauri::command]
pub async fn refresh_repository(state: State<'_, AppState>, repository: String) -> Result<i64, String> {
    refresh_repository_impl(&state.db, &repository).await
}

async fn refresh_repository_impl(pool: &DbPool, repository: &str) -> Result<i64, String> {
    let auth = github_direct_auth_from_settings(pool).await?;
    let mut repo = resolve_repo_ref(repository, auth.as_deref()).await?;
    let stars = repo.stars.ok_or("Repository star count is unavailable")?;
    repo.branch = fetch_repo_head_ref(&repo, auth.as_deref()).await?;
    let client = github_client()?;
    // Fetch documentation only, independently of skill downloads and updates.
    let tree_bytes = send_github_bytes_with_fallback(&client, GitHubFetchSurface::Api,
        |endpoint| github_endpoint_url(endpoint, GitHubFetchSurface::Api, &format!("/repos/{}/{}/git/trees/{}", repo.owner, repo.repo, repo.branch)),
        "Failed to inspect repository documents", auth.as_deref()).await?;
    let tree: serde_json::Value = serde_json::from_slice(&tree_bytes).map_err(|_| "Invalid repository file tree")?;
    if tree["truncated"].as_bool() != Some(false) { return Err("Repository file tree is incomplete; existing documents were preserved".into()); }
    let entries = tree["tree"].as_array().ok_or("Invalid repository file tree")?;
    let mut files = HashMap::new();
    for entry in entries {
        if entry["type"] != "blob" { continue; }
        let Some(path) = entry["path"].as_str().filter(|path| repository_docs::language(path).is_some()) else { continue; };
        let bytes = send_github_bytes_with_fallback(&client, GitHubFetchSurface::Raw,
            |endpoint| raw_file_url(endpoint, &repo, path), "Failed to download repository README", auth.as_deref()).await?;
        files.insert(path.to_string(), bytes);
    }
    apply_repository_metadata(pool, repository, &repo, &GitHubRepoSnapshot {files}).await?;
    Ok(stars)
}

async fn apply_repository_metadata(pool: &DbPool, repository: &str, repo: &GitHubRepoRef, snapshot: &GitHubRepoSnapshot) -> Result<(), String> {
    let stars = repo.stars.ok_or("Repository star count is unavailable")?;
    let (owner, name) = parse_github_url(repository)?;
    let source_key = format!("{owner}/{name}");
    let ids = db::get_all_skill_sources(pool).await?.into_iter()
        .filter(|source| source.source_repo.as_deref().is_some_and(|source| source.eq_ignore_ascii_case(&source_key)))
        .map(|source| source.skill_id).collect::<HashSet<_>>();
    let root = skill_resource_library_root(pool).await?;
    let root = root.canonicalize().unwrap_or(root);
    let directories = db::get_resource_library_skills(pool).await?.into_iter()
        .filter(|skill| ids.contains(&skill.id)).filter_map(|skill| skill.canonical_path)
        .filter_map(|path| { let path = PathBuf::from(path); let path = path.canonicalize().unwrap_or(path); repository_docs::group_directory(&root, &path) })
        .collect::<HashSet<_>>();
    if directories.is_empty() { return Err("Repository is no longer in the resource library".into()); }
    let mut transaction = ImportDirectoryTransaction::new(&root)?;
    for directory in directories { stage_repository_readme(snapshot, &directory, &mut transaction)?; }
    let mut database = pool.begin().await.map_err(|e| e.to_string())?;
    db::save_github_stars_with_executor(&mut *database, &source_key, stars).await?;
    db::save_github_stars_with_executor(&mut *database, &format!("{}/{}", repo.owner, repo.repo), stars).await?;
    database.commit().await.map_err(|e| e.to_string())?;
    transaction.commit();
    Ok(())
}

fn pinned_commit_from_url(url: &str) -> Option<&str> {
    let parts: Vec<_> = url.trim().trim_end_matches('/').split('/').collect();
    if parts.len() == 7 && parts[5] == "tree" && parts[6].len() == 40 && parts[6].bytes().all(|b|b.is_ascii_hexdigit()) { Some(parts[6]) } else {None}
}

pub(crate) async fn resolve_repo_ref(
    repo_url: &str,
    auth_token: Option<&str>,
) -> Result<GitHubRepoRef, String> {
    let (owner, repo) = parse_github_url(repo_url)?;
    if let Some(commit) = pinned_commit_from_url(repo_url) {
        return Ok(GitHubRepoRef {
            normalized_url: format!("https://github.com/{owner}/{repo}"),
            owner, repo, branch: commit.to_string(), stars: None,
        });
    }
    let client = github_client()?;
    let response = send_github_request_with_fallback(
        &client,
        GitHubFetchSurface::Api,
        |endpoint| {
            github_endpoint_url(
                endpoint,
                GitHubFetchSurface::Api,
                &format!("/repos/{owner}/{repo}"),
            )
        },
        "Failed to inspect GitHub repository",
        auth_token,
    )
    .await?;

    if response.status() == reqwest::StatusCode::NOT_FOUND {
        return Err("GitHub repository not found.".to_string());
    }
    if !response.status().is_success() {
        let status = response.status();
        return Err(
            classify_github_denial_response(response, "inspecting the repository")
                .await
                .unwrap_or_else(|| format!("Failed to inspect GitHub repository: HTTP {}", status)),
        );
    }

    let payload: serde_json::Value = response.json().await.map_err(|e| e.to_string())?;
    let (owner, repo) = repository_display_names(&payload, &owner, &repo);
    let branch = pinned_commit_from_url(repo_url).or_else(|| payload
        .get("default_branch")
        .and_then(|v| v.as_str())
        .filter(|value| !value.is_empty()))
        .unwrap_or("main")
        .to_string();

    Ok(GitHubRepoRef {
        owner: owner.clone(),
        repo: repo.clone(),
        branch,
        normalized_url: format!("https://github.com/{owner}/{repo}"),
            stars: payload.get("stargazers_count").and_then(|value| value.as_i64()).filter(|stars| *stars >= 0),
    })
}

fn repository_display_names(payload: &serde_json::Value, owner: &str, repo: &str) -> (String, String) {
    if let Some(full_name) = payload.get("full_name").and_then(|value| value.as_str()) {
        let parts: Vec<_> = full_name.split('/').collect();
        if parts.len() == 2 && parts.iter().all(|part| !part.is_empty()) {
            return (parts[0].to_string(), parts[1].to_string());
        }
    }
    (owner.to_string(), repo.to_string())
}

pub(crate) async fn fetch_repo_head_ref(
    repo: &GitHubRepoRef,
    auth_token: Option<&str>,
) -> Result<String, String> {
    let client = github_client()?;
    let response = send_github_request_with_fallback(
        &client,
        GitHubFetchSurface::Api,
        |endpoint| {
            github_endpoint_url(
                endpoint,
                GitHubFetchSurface::Api,
                &format!(
                    "/repos/{}/{}/commits/{}",
                    repo.owner, repo.repo, repo.branch
                ),
            )
        },
        "Failed to inspect GitHub repository head",
        auth_token,
    )
    .await?;

    if response.status() == reqwest::StatusCode::NOT_FOUND {
        return Err("GitHub repository head not found.".to_string());
    }
    if !response.status().is_success() {
        let status = response.status();
        return Err(
            classify_github_denial_response(response, "inspecting the repository head")
                .await
                .unwrap_or_else(|| {
                    format!("Failed to inspect GitHub repository head: HTTP {}", status)
                }),
        );
    }

    let payload: serde_json::Value = response.json().await.map_err(|e| e.to_string())?;
    payload
        .get("sha")
        .and_then(|value| value.as_str())
        .filter(|value| !value.trim().is_empty())
        .map(str::to_string)
        .ok_or_else(|| "GitHub repository head response did not include a commit SHA.".to_string())
}

pub(crate) async fn github_direct_auth_from_settings(
    pool: &DbPool,
) -> Result<Option<String>, String> {
    Ok(db::get_setting(pool, GITHUB_PAT_SETTING_KEY)
        .await?
        .map(|token| token.trim().to_string())
        .filter(|token| !token.is_empty()))
}

fn github_client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .user_agent(concat!("SkillsHub/", env!("CARGO_PKG_VERSION")))
        .connect_timeout(std::time::Duration::from_secs(15))
        .read_timeout(std::time::Duration::from_secs(45))
        .build()
        .map_err(|e| e.to_string())
}

fn parse_github_url(url: &str) -> Result<(String, String), String> {
    let trimmed = url.trim().trim_end_matches('/');
    let parts: Vec<_> = trimmed.split('/').collect();
    let expanded;
    let trimmed = if parts.len() == 2 && parts.iter().all(|part| !part.is_empty() && part.bytes().all(|b| b.is_ascii_alphanumeric() || b"-_.".contains(&b))) {
        expanded = format!("https://github.com/{trimmed}");
        expanded.as_str()
    } else { trimmed };
    let parsed =
        reqwest::Url::parse(trimmed).map_err(|_| "Invalid GitHub repository URL.".to_string())?;

    if parsed.scheme() != "https" {
        return Err("Only https:// GitHub repository URLs are supported.".to_string());
    }
    if parsed.host_str() != Some("github.com") {
        return Err("Only github.com repository URLs are supported.".to_string());
    }

    let mut segments = parsed
        .path_segments()
        .ok_or_else(|| "Invalid GitHub repository URL.".to_string())?;
    let owner = segments
        .next()
        .filter(|segment| !segment.is_empty())
        .ok_or_else(|| "GitHub repository URL must include an owner.".to_string())?;
    let repo = segments
        .next()
        .filter(|segment| !segment.is_empty())
        .ok_or_else(|| "GitHub repository URL must include a repository name.".to_string())?;

    let repo = repo.strip_suffix(".git").unwrap_or(repo);
    if owner.is_empty() || repo.is_empty() {
        return Err("GitHub repository URL is missing owner or repository.".to_string());
    }

    Ok((owner.to_string(), repo.to_string()))
}

pub(crate) fn normalize_github_repo_input(input: &str) -> Result<String, String> {
    let trimmed = input.trim().trim_end_matches('/');
    if trimmed.starts_with("https://") {
        let (owner, repo) = parse_github_url(trimmed)?;
        return Ok(format!("https://github.com/{owner}/{repo}"));
    }
    let without_git = trimmed.strip_suffix(".git").unwrap_or(trimmed);
    let parts = without_git.split('/').collect::<Vec<_>>();
    if parts.len() == 2
        && parts
            .iter()
            .all(|part| !part.trim().is_empty() && !part.contains('\\'))
    {
        return Ok(format!(
            "https://github.com/{}/{}",
            parts[0].trim(),
            parts[1].trim()
        ));
    }
    Err("Enter a GitHub repository as owner/repo or https://github.com/owner/repo.".to_string())
}

pub(crate) async fn fetch_repo_skill_candidates(
    repo: &GitHubRepoRef,
    auth_token: Option<&str>,
) -> Result<Vec<RemoteSkillCandidate>, String> {
    let client = github_client()?;
    let snapshot = download_repo_snapshot(&client, repo, auth_token).await?;
    build_repo_skill_candidates_from_snapshot(repo, &snapshot)
}

pub(crate) async fn fetch_repo_skill_manifest_paths(
    repo: &GitHubRepoRef,
    auth_token: Option<&str>,
) -> Result<Vec<String>, String> {
    let client = github_client()?;
    let snapshot = download_repo_snapshot(&client, repo, auth_token).await?;
    let mut paths = snapshot
        .files
        .keys()
        .filter(|path| {
            let lower = path.to_ascii_lowercase();
            lower.ends_with("/skill.md") || lower == "skill.md"
        })
        .cloned()
        .collect::<Vec<_>>();
    paths.sort();
    Ok(paths)
}

pub(crate) async fn fetch_repo_preview_files(repo: &GitHubRepoRef, auth: Option<&str>) -> Result<(Vec<RemoteSkillCandidate>, HashMap<String, std::collections::BTreeMap<String, Vec<u8>>>), String> {
    fetch_repo_preview_files_scoped(repo, auth, None).await
}

pub(crate) async fn fetch_repo_preview_files_scoped(repo: &GitHubRepoRef, auth: Option<&str>, keys: Option<&HashSet<String>>) -> Result<(Vec<RemoteSkillCandidate>, HashMap<String, std::collections::BTreeMap<String, Vec<u8>>>), String> {
    let client = github_client()?;
    let snapshot = download_repo_snapshot(&client, repo, auth).await?;
    let mut candidates = build_repo_skill_candidates_from_snapshot(repo, &snapshot)?;
    if let Some(keys) = keys {
        candidates.retain(|candidate| keys.contains(&candidate.skill_id) ||
            keys.contains(&candidate.source_path) || keys.contains(&candidate.source_manifest_path));
    }
    let mut contents = HashMap::new();
    for candidate in &candidates {
        let mut files = std::collections::BTreeMap::new();
        for file in collect_snapshot_source_files(&snapshot, &candidate.source_path)? {
            let name = if file.relative_path.eq_ignore_ascii_case("skill.md") { "SKILL.md".into() } else { file.relative_path };
            files.insert(name, snapshot.files[&file.repo_path].clone());
        }
        // Include shared documentation in update detection and content versions.
        for (name, bytes) in snapshot_readmes(&snapshot) {
            files.insert(format!("../{name}"), bytes);
        }
        contents.insert(candidate.source_path.clone(), files);
    }
    Ok((candidates, contents))
}

fn build_repo_skill_candidates_from_snapshot(
    repo: &GitHubRepoRef,
    snapshot: &GitHubRepoSnapshot,
) -> Result<Vec<RemoteSkillCandidate>, String> {
    let direct_endpoint = GITHUB_MIRROR_ENDPOINTS.first().expect("github endpoint");
    let mut manifests = snapshot
        .files
        .keys()
        .filter_map(|path| classify_skill_manifest_path(path))
        .collect::<Vec<_>>();
    manifests.sort_by(|left, right| left.source_path.cmp(&right.source_path));

    let mut candidates = Vec::with_capacity(manifests.len());
    for manifest in manifests {
        let is_root_manifest = manifest.is_root_manifest();
        let raw = snapshot
            .files
            .get(&manifest.skill_md_path)
            .ok_or_else(|| format!("Missing snapshot file '{}'.", manifest.skill_md_path))?;
        let content = String::from_utf8(raw.clone())
            .map_err(|_| format!("Skill '{}' is not valid UTF-8.", manifest.source_path))?;
        let frontmatter = parse_frontmatter(&content).ok_or_else(|| {
            if is_root_manifest {
                "Repository root SKILL.md is missing valid frontmatter.".to_string()
            } else {
                format!(
                    "Skill '{}' is missing valid frontmatter.",
                    manifest.source_path
                )
            }
        })?;

        let skill_id = if is_root_manifest {
            let repo_skill_id = sanitize_skill_id(&repo.repo)?;
            repo_skill_id
                .strip_suffix("-skill")
                .unwrap_or(&repo_skill_id)
                .to_string()
        } else {
            sanitize_skill_id(&manifest.skill_directory_name)?
        };

        candidates.push(RemoteSkillCandidate {
            source_path: manifest.source_path.clone(),
            source_manifest_path: manifest.source_manifest_path.clone(),
            skill_id,
            skill_name: frontmatter.name,
            description: frontmatter.description,
            root_directory: manifest.root_directory,
            skill_directory_name: if is_root_manifest {
                repo.repo.clone()
            } else {
                manifest.skill_directory_name
            },
            download_url: raw_file_url(direct_endpoint, repo, &manifest.skill_md_path),
        });
    }

    Ok(candidates)
}

#[derive(Debug, Clone)]
struct SnapshotSkillManifest {
    source_path: String,
    source_manifest_path: String,
    root_directory: String,
    skill_directory_name: String,
    skill_md_path: String,
}

impl SnapshotSkillManifest {
    fn is_root_manifest(&self) -> bool {
        self.skill_md_path.eq_ignore_ascii_case("SKILL.md")
    }
}

fn classify_skill_manifest_path(path: &str) -> Option<SnapshotSkillManifest> {
    let normalized = path.trim_matches('/');
    if normalized.is_empty() {
        return None;
    }

    if normalized.eq_ignore_ascii_case("SKILL.md") {
        return Some(SnapshotSkillManifest {
            source_path: "SKILL.md".to_string(),
            source_manifest_path: "SKILL.md".to_string(),
            root_directory: "/".to_string(),
            skill_directory_name: String::new(),
            skill_md_path: "SKILL.md".to_string(),
        });
    }

    let parts = normalized.split('/').collect::<Vec<_>>();
    let (skill_md, source_parts) = parts.split_last()?;
    if !skill_md.eq_ignore_ascii_case("SKILL.md") {
        return None;
    }

    let source_path = normalized.to_string();
    match source_parts {
        [skill_dir] if *skill_dir != ".github" && *skill_dir != "skills" => {
            Some(SnapshotSkillManifest {
                source_path,
                source_manifest_path: normalized.to_string(),
                root_directory: "/".to_string(),
                skill_directory_name: (*skill_dir).to_string(),
                skill_md_path: normalized.to_string(),
            })
        }
        _ if source_parts
            .iter()
            .position(|part| matches!(*part, "skills" | "skill-package"))
            .is_some_and(|index| index + 1 < source_parts.len()) =>
        {
            Some(SnapshotSkillManifest {
                source_path,
                source_manifest_path: normalized.to_string(),
                root_directory: source_parts[..source_parts.len() - 1].join("/"),
                skill_directory_name: source_parts.last()?.to_string(),
                skill_md_path: normalized.to_string(),
            })
        }
        _ => None,
    }
}

async fn download_repo_snapshot(
    client: &reqwest::Client,
    repo: &GitHubRepoRef,
    auth_token: Option<&str>,
) -> Result<std::sync::Arc<GitHubRepoSnapshot>, String> {
    load_repo_snapshot(repo, auth_token, || async {
        // Media-heavy repositories may contain only a tiny installable skill.
        // Inspect the pinned tree before committing to a whole-repository archive.
        if let Ok(tree) = fetch_repository_tree(client, repo, auth_token).await {
            if let Some(snapshot) = snapshot_from_tree_if_sparse(&tree, |path| async move {
                send_github_bytes_with_fallback(client, GitHubFetchSurface::Raw,
                    |endpoint| raw_file_url(endpoint, repo, &path),
                    "Failed to download skill resource", auth_token).await
            }).await? {
                return Ok(snapshot);
            }
        }
        let archive = download_repository_archive(client, repo, auth_token).await?;
        snapshot_from_repository_archive(&archive)
    }).await
}

async fn fetch_repository_tree(
    client: &reqwest::Client, repo: &GitHubRepoRef, auth: Option<&str>,
) -> Result<serde_json::Value, String> {
    let bytes = send_github_bytes_with_fallback(client, GitHubFetchSurface::Api,
        |endpoint| github_endpoint_url(endpoint, GitHubFetchSurface::Api,
            &format!("/repos/{}/{}/git/trees/{}?recursive=1", repo.owner, repo.repo, repo.branch)),
        "Failed to inspect repository file tree", auth).await?;
    serde_json::from_slice(&bytes).map_err(|_| "Invalid repository file tree response".into())
}

async fn snapshot_from_tree_if_sparse<F, Fut>(
    tree: &serde_json::Value, fetch: F,
) -> Result<Option<GitHubRepoSnapshot>, String>
where F: Fn(String) -> Fut,
    Fut: std::future::Future<Output = Result<Vec<u8>, String>>,
{
    // An incomplete tree cannot prove that a resource or skill is absent.
    if tree.get("truncated").and_then(serde_json::Value::as_bool) != Some(false) {
        return Ok(None);
    }
    let Some(entries) = tree.get("tree").and_then(serde_json::Value::as_array) else { return Ok(None) };
    let blobs = entries.iter().filter(|entry| entry["type"] == "blob").collect::<Vec<_>>();
    let mut paths = Vec::new();
    let mut total_bytes = 0_u64;
    for blob in blobs {
        let (Some(path), Some(size)) = (blob["path"].as_str(), blob["size"].as_u64()) else { return Ok(None) };
        if !is_safe_repo_relative_path(path) { return Err("Repository tree contains an unsupported path".into()) }
        paths.push((path, size));
        total_bytes = total_bytes.saturating_add(size);
    }
    let roots = paths.iter().filter_map(|(path, _)| classify_skill_manifest_path(path))
        .map(|manifest| snapshot_source_directory(&manifest.source_path)).collect::<HashSet<_>>();
    let selected = paths.iter().filter(|(path, _)| repository_docs::language(path).is_some() || roots.iter().any(|root| {
        if root.is_empty() { !path.contains('/') } else { path.starts_with(&format!("{root}/")) }
    })).collect::<Vec<_>>();
    let selected_bytes = selected.iter().map(|(_, size)| *size).sum::<u64>();
    // Keep one archive for large skill collections, avoiding per-file HTTP overhead.
    if selected.is_empty() || selected.len() > 64 || total_bytes < 16 * 1024 * 1024
        || selected_bytes > total_bytes / 4 {
        return Ok(None);
    }
    let mut files = HashMap::new();
    for (path, _) in selected {
        let bytes = fetch(path.to_string()).await?;
        files.insert(path.to_string(), bytes);
    }
    Ok(Some(GitHubRepoSnapshot { files }))
}

fn snapshot_readmes(snapshot: &GitHubRepoSnapshot) -> std::collections::BTreeMap<String, Vec<u8>> {
    let mut entries = snapshot.files.iter().filter(|(path, _)| repository_docs::language(path).is_some()).collect::<Vec<_>>();
    entries.sort_by_key(|(path, _)| *path);
    let mut names = HashSet::new();
    entries.into_iter().filter_map(|(path, bytes)| {
        let name = repository_docs::canonical_name(path);
        names.insert(name.to_lowercase()).then(|| (name, bytes.clone()))
    }).collect()
}

fn stage_repository_readme(snapshot: &GitHubRepoSnapshot, directory: &Path, transaction: &mut ImportDirectoryTransaction) -> Result<(), String> {
    let documents = snapshot_readmes(snapshot);
    let names = documents.keys().map(|name| name.to_lowercase()).collect::<HashSet<_>>();
    for path in repository_docs::paths(directory)? {
        if !names.contains(&path.file_name().unwrap().to_string_lossy().to_lowercase()) { transaction.remove_file(&path)?; }
    }
    for (index, (name, bytes)) in documents.iter().enumerate() {
        let staged = transaction.root.join(format!("repository-readme-{index}"));
        std::fs::write(&staged, bytes).map_err(|e| format!("Failed to stage repository README: {e}"))?;
        transaction.install(&staged, &directory.join(name), true)?;
    }
    Ok(())
}

type SnapshotCell = std::sync::Arc<tokio::sync::OnceCell<std::sync::Arc<GitHubRepoSnapshot>>>;
type SnapshotKey = (String, String, String, Option<String>);
static SNAPSHOTS: tokio::sync::Mutex<Vec<(SnapshotKey, SnapshotCell)>> = tokio::sync::Mutex::const_new(Vec::new());
const SNAPSHOT_CACHE_BYTES: usize = 512 * 1024 * 1024;

fn is_commit_ref(reference: &str) -> bool {
    reference.len() == 40 && reference.bytes().all(|byte| byte.is_ascii_hexdigit())
}

pub(crate) async fn resolve_update_revision(repo: &str, auth: Option<&str>, batch: Option<&str>) -> Result<GitHubRepoRef, String> {
    load_batch_revision(repo, auth, batch, || async {
        let mut resolved = resolve_repo_ref(&format!("https://github.com/{repo}"), auth).await?;
        resolved.branch = fetch_repo_head_ref(&resolved, auth).await?;
        Ok(resolved)
    }).await
}

type RevisionCell = std::sync::Arc<tokio::sync::OnceCell<GitHubRepoRef>>;
type RevisionKey = (String, String, Option<String>);
static BATCH_REVISIONS: tokio::sync::Mutex<Vec<(RevisionKey, RevisionCell)>> = tokio::sync::Mutex::const_new(Vec::new());

async fn load_batch_revision<F, Fut>(
    repo: &str, auth: Option<&str>, batch: Option<&str>, load: F,
) -> Result<GitHubRepoRef, String>
where F: FnOnce() -> Fut, Fut: std::future::Future<Output = Result<GitHubRepoRef, String>> {
    let Some(batch) = batch else { return load().await; };
    let key = (repo.to_ascii_lowercase(), batch.to_owned(), auth.map(str::to_owned));
    let cell = {
        let mut entries = BATCH_REVISIONS.lock().await;
        let cell = entries.iter().position(|(existing, _)| existing == &key)
            .map(|index| entries.remove(index).1)
            .unwrap_or_else(|| std::sync::Arc::new(tokio::sync::OnceCell::new()));
        entries.push((key, cell.clone()));
        if entries.len() > 32 { entries.remove(0); }
        cell
    };
    let result = cell.get_or_try_init(load).await.cloned();
    if result.is_err() {
        BATCH_REVISIONS.lock().await.retain(|(_, existing)| !std::sync::Arc::ptr_eq(existing, &cell));
    }
    result
}

async fn load_repo_snapshot<F, Fut>(
    repo: &GitHubRepoRef,
    auth: Option<&str>,
    load: F,
) -> Result<std::sync::Arc<GitHubRepoSnapshot>, String>
where
    F: FnOnce() -> Fut,
    Fut: std::future::Future<Output = Result<GitHubRepoSnapshot, String>>,
{
    // Branches are mutable: never cache them or suppress a fresh update check.
    if !is_commit_ref(&repo.branch) {
        return load().await.map(std::sync::Arc::new);
    }
    // Auth scopes stay isolated. Keys and contents are memory-only, never logged.
    let key = (repo.owner.to_ascii_lowercase(), repo.repo.to_ascii_lowercase(),
        repo.branch.to_ascii_lowercase(), auth.map(str::to_owned));
    let cell = {
        let mut entries = SNAPSHOTS.lock().await;
        let cell = entries.iter().position(|(existing, _)| existing == &key)
            .map(|index| entries.remove(index).1)
            .unwrap_or_else(|| std::sync::Arc::new(tokio::sync::OnceCell::new()));
        entries.push((key, cell.clone()));
        cell
    };
    // Concurrent operations for one revision share the download and unpack too.
    let result = cell.get_or_try_init(|| async { load().await.map(std::sync::Arc::new) }).await.cloned();
    let mut entries = SNAPSHOTS.lock().await;
    if result.is_err() {
        entries.retain(|(_, existing)| !std::sync::Arc::ptr_eq(existing, &cell));
    }
    let mut bytes = 0usize;
    let mut count = 0;
    // Keep recent completed snapshots within both a byte and entry budget.
    for index in (0..entries.len()).rev() {
        if let Some(snapshot) = entries[index].1.get() {
            let size = snapshot.files.iter().map(|(path, data)| path.len() + data.len()).sum::<usize>();
            if count >= 4 || size > SNAPSHOT_CACHE_BYTES.saturating_sub(bytes) {
                entries.remove(index);
            } else {
                bytes += size;
                count += 1;
            }
        }
    }
    result
}

async fn download_repository_archive(
    client: &reqwest::Client,
    repo: &GitHubRepoRef,
    auth_token: Option<&str>,
) -> Result<Vec<u8>, String> {
    send_github_bytes_with_fallback(
        client,
        GitHubFetchSurface::Api,
        |endpoint| {
            github_endpoint_url(
                endpoint,
                GitHubFetchSurface::Api,
                &format!(
                    "/repos/{}/{}/tarball/{}",
                    repo.owner, repo.repo, repo.branch
                ),
            )
        },
        "Failed to download GitHub repository archive",
        auth_token,
    )
    .await
}

fn snapshot_from_repository_archive(archive_bytes: &[u8]) -> Result<GitHubRepoSnapshot, String> {
    let cursor = Cursor::new(archive_bytes);
    let decoder = GzDecoder::new(cursor);
    let mut archive = tar::Archive::new(decoder);
    let mut files = HashMap::new();

    for entry_result in archive
        .entries()
        .map_err(|e| format!("Failed to inspect GitHub repository archive: {}", e))?
    {
        let mut entry = entry_result
            .map_err(|e| format!("Failed to inspect GitHub repository archive: {}", e))?;

        if !entry.header().entry_type().is_file() {
            continue;
        }

        let relative_path = relative_archive_path(&entry)?;
        let mut content = Vec::new();
        entry.read_to_end(&mut content).map_err(|e| {
            format!(
                "Failed to read GitHub repository archive entry '{}': {}",
                relative_path, e
            )
        })?;
        files.insert(relative_path, content);
    }

    Ok(GitHubRepoSnapshot { files })
}

fn relative_archive_path<R: Read>(entry: &tar::Entry<'_, R>) -> Result<String, String> {
    let archive_path = entry
        .path()
        .map_err(|e| format!("Failed to inspect GitHub repository archive: {}", e))?;
    let relative = archive_path
        .components()
        .skip(1)
        .map(|component| match component {
            Component::Normal(value) => Ok(value.to_string_lossy().into_owned()),
            _ => Err("GitHub repository archive contains an unsupported path.".to_string()),
        })
        .collect::<Result<Vec<_>, _>>()?;

    if relative.is_empty() {
        return Err("GitHub repository archive contains an unsupported path.".to_string());
    }

    let joined = relative.join("/");
    if !is_safe_repo_relative_path(&joined) {
        return Err(format!(
            "GitHub repository archive contains an unsupported path '{}'.",
            joined
        ));
    }

    Ok(joined)
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SnapshotSourceFile {
    repo_path: String,
    relative_path: String,
    byte_len: usize,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct GitHubImportProgressState {
    operation_id: Option<String>,
    completed_files: usize,
    total_files: usize,
    completed_bytes: u64,
    total_bytes: u64,
}

fn collect_snapshot_source_files(
    snapshot: &GitHubRepoSnapshot,
    source_path: &str,
) -> Result<Vec<SnapshotSourceFile>, String> {
    let source_directory = snapshot_source_directory(source_path);
    let mut files = snapshot
        .files
        .iter()
        .filter_map(|(path, bytes)| {
            let relative_path = if source_directory.is_empty() {
                if path.contains('/') {
                    return None;
                }
                path.clone()
            } else {
                let prefix = format!("{}/", source_directory);
                let relative = path.strip_prefix(&prefix)?;
                if relative.is_empty() {
                    return None;
                }
                relative.to_string()
            };

            Some(SnapshotSourceFile {
                repo_path: path.clone(),
                relative_path,
                byte_len: bytes.len(),
            })
        })
        .collect::<Vec<_>>();

    files.sort_by(|left, right| left.repo_path.cmp(&right.repo_path));

    if files.is_empty() {
        return Err(format!(
            "Repository path '{}' is no longer available in the archive.",
            source_path
        ));
    }

    Ok(files)
}

fn snapshot_source_directory(source_path: &str) -> String {
    let normalized = source_path.trim().trim_matches('/').replace('\\', "/");
    if normalized.is_empty() || normalized == "." || normalized.eq_ignore_ascii_case("SKILL.md") {
        return String::new();
    }

    normalized
        .strip_suffix("/SKILL.md")
        .or_else(|| normalized.strip_suffix("/skill.md"))
        .unwrap_or(&normalized)
        .to_string()
}

fn write_snapshot_source_to_target(
    snapshot: &GitHubRepoSnapshot,
    files: &[SnapshotSourceFile],
    target_dir: &Path,
    source_path: &str,
    progress_state: &mut GitHubImportProgressState,
    app: Option<&AppHandle>,
) -> Result<(), String> {
    std::fs::create_dir_all(target_dir)
        .map_err(|e| format!("Failed to create import target directory: {}", e))?;

    for file in files {
        if !is_safe_repo_relative_path(&file.relative_path) {
            return Err(format!(
                "Repository contains an unsupported path '{}'.",
                file.repo_path
            ));
        }

        let bytes = snapshot.files.get(&file.repo_path).ok_or_else(|| {
            format!(
                "Repository file '{}' is no longer available in the archive.",
                file.repo_path
            )
        })?;

        let destination_name = if file.relative_path.eq_ignore_ascii_case("skill.md") {
            "SKILL.md".to_string()
        } else {
            file.relative_path.clone()
        };
        let destination = target_dir.join(&destination_name);
        let parent = destination
            .parent()
            .ok_or_else(|| "Failed to determine imported file parent directory.".to_string())?;
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create imported file parent directory: {}", e))?;
        std::fs::write(&destination, bytes).map_err(|e| {
            format!(
                "Failed to write imported file '{}': {}",
                destination.display(),
                e
            )
        })?;

        progress_state.completed_files += 1;
        progress_state.completed_bytes += file.byte_len as u64;
        emit_github_import_progress(
            app,
            GitHubImportProgressPayload {
                operation_id: progress_state.operation_id.clone(),
                phase: GitHubImportProgressPhase::Writing,
                current_skill: Some(source_path.to_string()),
                current_path: Some(file.relative_path.clone()),
                completed_files: progress_state.completed_files,
                total_files: progress_state.total_files,
                completed_bytes: progress_state.completed_bytes,
                total_bytes: progress_state.total_bytes,
            },
        );
    }

    Ok(())
}

fn emit_github_import_progress(app: Option<&AppHandle>, payload: GitHubImportProgressPayload) {
    if let Some(app) = app {
        let _ = app.emit("github-import:progress", payload);
    }
}

fn is_safe_repo_relative_path(path: &str) -> bool {
    let relative = Path::new(path);
    !relative.is_absolute()
        && relative
            .components()
            .all(|component| matches!(component, Component::Normal(_)))
}

pub(crate) async fn fetch_raw_text(
    client: &reqwest::Client,
    url: &str,
    auth_token: Option<&str>,
) -> Result<String, String> {
    let response = send_github_request_with_fallback(
        client,
        GitHubFetchSurface::Raw,
        |endpoint| {
            if let Some(path) = raw_url_to_repo_path(url) {
                raw_file_url(endpoint, &path.repo, &path.file_path)
            } else {
                url.to_string()
            }
        },
        "Failed to download skill metadata",
        auth_token,
    )
    .await?;

    if !response.status().is_success() {
        return Err(
            classify_github_denial_response(response, "downloading skill metadata")
                .await
                .unwrap_or_else(|| "Failed to download skill metadata.".to_string()),
        );
    }

    response
        .text()
        .await
        .map_err(|e| format!("Failed to read skill metadata: {}", e))
}

#[derive(Debug, Clone)]
struct RawRepoPath {
    repo: GitHubRepoRef,
    file_path: String,
}

fn raw_url_to_repo_path(url: &str) -> Option<RawRepoPath> {
    let parsed = reqwest::Url::parse(url).ok()?;
    let host = parsed.host_str()?;
    if host != "raw.githubusercontent.com" {
        return None;
    }

    let segments = parsed.path_segments()?;
    let parts = segments.collect::<Vec<_>>();
    if parts.len() < 4 {
        return None;
    }

    Some(RawRepoPath {
        repo: GitHubRepoRef {
            owner: parts[0].to_string(),
            repo: parts[1].to_string(),
            branch: parts[2].to_string(),
            normalized_url: format!("https://github.com/{}/{}", parts[0], parts[1]),
            stars: None,
        },
        file_path: parts[3..].join("/"),
    })
}

fn github_endpoint_url(
    endpoint: &GitHubMirrorEndpoint,
    surface: GitHubFetchSurface,
    path: &str,
) -> String {
    let base = match surface {
        GitHubFetchSurface::Api => endpoint.api_base,
        GitHubFetchSurface::Raw => endpoint.raw_base,
    };
    format!("{}{}", base.trim_end_matches('/'), path)
}

fn raw_file_url(endpoint: &GitHubMirrorEndpoint, repo: &GitHubRepoRef, file_path: &str) -> String {
    github_endpoint_url(
        endpoint,
        GitHubFetchSurface::Raw,
        &format!(
            "/{}/{}/{}/{}",
            repo.owner,
            repo.repo,
            repo.branch,
            file_path.trim_start_matches('/')
        ),
    )
}

async fn send_github_bytes_with_fallback<F>(
    client: &reqwest::Client,
    surface: GitHubFetchSurface,
    build_url: F,
    failure_prefix: &str,
    auth_token: Option<&str>,
) -> Result<Vec<u8>, String>
where
    F: Fn(&GitHubMirrorEndpoint) -> String,
{
    send_github_request_and_read_with_fallback(
        client,
        surface,
        build_url,
        failure_prefix,
        auth_token,
        true,
        |response| async {
            response
                .error_for_status()?
                .bytes()
                .await
                .map(|bytes| bytes.to_vec())
        },
    )
    .await
}

async fn send_github_request_with_fallback<F>(
    client: &reqwest::Client,
    surface: GitHubFetchSurface,
    build_url: F,
    failure_prefix: &str,
    auth_token: Option<&str>,
) -> Result<reqwest::Response, String>
where
    F: Fn(&GitHubMirrorEndpoint) -> String,
{
    send_github_request_and_read_with_fallback(
        client,
        surface,
        build_url,
        failure_prefix,
        auth_token,
        false,
        |response| async { Ok(response) },
    )
    .await
}

// Reading the body belongs to the attempt: HTTP 200 alone is not a completed
// download. Retry discards the partial buffer and starts a new authenticated GET.
async fn send_github_request_and_read_with_fallback<F, R, Fut, T>(
    client: &reqwest::Client,
    surface: GitHubFetchSurface,
    build_url: F,
    failure_prefix: &str,
    auth_token: Option<&str>,
    retry_body: bool,
    read: R,
) -> Result<T, String>
where
    F: Fn(&GitHubMirrorEndpoint) -> String,
    R: Fn(reqwest::Response) -> Fut,
    Fut: std::future::Future<Output = Result<T, reqwest::Error>>,
{
    let mut attempts = Vec::new();
    let mut last_retryable_denial = None;
    // The five-minute download budget is shared by every retry/mirror, not
    // multiplied by the number of endpoints. Read-idle timeout remains 45s.
    let download_deadline =
        retry_body.then(|| std::time::Instant::now() + std::time::Duration::from_secs(300));

    'endpoints: for endpoint in GITHUB_MIRROR_ENDPOINTS {
        // One retry on the original endpoint; mirrors each get one attempt.
        let max_attempts = if retry_body && endpoint.label == "github" {
            2
        } else {
            1
        };
        for attempt in 0..max_attempts {
            if attempt > 0 {
                tokio::time::sleep(std::time::Duration::from_millis(500)).await;
            }
            let request_timeout = if let Some(deadline) = download_deadline {
                let remaining = deadline.saturating_duration_since(std::time::Instant::now());
                if remaining.is_zero() {
                    return Err(format!(
                        "{failure_prefix}: network download timed out; retry budget exhausted"
                    ));
                }
                remaining
            } else {
                std::time::Duration::from_secs(60)
            };
            let url = build_url(endpoint);
            let mut request = client.get(url).timeout(request_timeout);
            if endpoint.label == "github" {
                if let Some(token) = auth_token {
                    request = request.bearer_auth(token);
                }
            }
            match request.send().await {
                Ok(response) => {
                    let status = response.status();
                    if matches!(
                        status,
                        reqwest::StatusCode::UNAUTHORIZED
                            | reqwest::StatusCode::FORBIDDEN
                            | reqwest::StatusCode::TOO_MANY_REQUESTS
                    ) {
                        // A mirror denial says nothing about the user's GitHub credential.
                        // Try the remaining mirrors and preserve the direct failure category.
                        if endpoint.label != "github" {
                            attempts.push(MirrorAttemptOutcome {
                                status: Some(status),
                                error_message: format!(
                                    "{} mirror '{}' denied the network request (HTTP {})",
                                    surface_label(surface),
                                    endpoint.label,
                                    status
                                ),
                            });
                            continue 'endpoints;
                        }
                        let denial =
                            parse_github_denial_response(response, "contacting GitHub").await;
                        let can_retry_public_mirror = auth_token.is_none()
                            && denial.as_ref().is_some_and(|denial| {
                                matches!(denial.kind, GitHubAccessDenialKind::RateLimited { .. })
                            });
                        if can_retry_public_mirror {
                            last_retryable_denial = denial;
                            attempts.push(MirrorAttemptOutcome {
                                status: Some(status),
                                error_message: format!(
                                    "{} mirror '{}' returned HTTP {} due to rate limiting",
                                    surface_label(surface),
                                    endpoint.label,
                                    status
                                ),
                            });
                            continue 'endpoints;
                        }

                        return Err(denial
                            .map(|denial| denial.to_string())
                            .unwrap_or_else(|| format!("{}: HTTP {}", failure_prefix, status)));
                    }

                    if status.is_success() {
                        match read(response).await {
                            Ok(result) => return Ok(result),
                            Err(error) if is_retryable_github_transport_error(&error) => {
                                attempts.push(MirrorAttemptOutcome {
                                    status: Some(status),
                                    error_message: format!(
                                        "{} mirror '{}' response body failed: {}",
                                        surface_label(surface),
                                        endpoint.label,
                                        github_transport_error_detail(&error)
                                    ),
                                });
                                continue;
                            }
                            Err(error) => {
                                return Err(format!(
                                    "{failure_prefix}: {}",
                                    github_transport_error_detail(&error)
                                ))
                            }
                        }
                    }

                    if status == reqwest::StatusCode::NOT_FOUND {
                        if endpoint.label != "github" && !attempts.is_empty() {
                            attempts.push(MirrorAttemptOutcome {
                                status: Some(status),
                                error_message: format!(
                                    "{} mirror '{}' returned HTTP 404 after a prior direct failure",
                                    surface_label(surface),
                                    endpoint.label
                                ),
                            });
                            continue 'endpoints;
                        }
                        return read(response).await.map_err(|error| {
                            format!(
                                "{failure_prefix}: {}",
                                github_transport_error_detail(&error)
                            )
                        });
                    }

                    if should_retry_via_mirror_status(surface, status) {
                        attempts.push(MirrorAttemptOutcome {
                            status: Some(status),
                            error_message: format!(
                                "{} mirror '{}' returned HTTP {}",
                                surface_label(surface),
                                endpoint.label,
                                status
                            ),
                        });
                        continue 'endpoints;
                    }

                    return Err(format!("{}: HTTP {}", failure_prefix, status));
                }
                Err(error) => {
                    if is_retryable_github_transport_error(&error) {
                        attempts.push(MirrorAttemptOutcome {
                            status: error.status(),
                            error_message: format!(
                                "{} mirror '{}' failed: {}",
                                surface_label(surface),
                                endpoint.label,
                                github_transport_error_detail(&error)
                            ),
                        });
                        continue;
                    }

                    return Err(format!(
                        "{}: {}",
                        failure_prefix,
                        github_transport_error_detail(&error)
                    ));
                }
            }
        }
    }
    if let Some(denial) = last_retryable_denial {
        return Err(denial.to_string());
    }

    Err(format!(
        "{}. Direct GitHub access and built-in mirrors were unreachable. Retry later or try a different network path. Last errors: {}",
        failure_prefix,
        summarize_mirror_attempts(&attempts)
    ))
}

fn should_retry_via_mirror_status(
    surface: GitHubFetchSurface,
    status: reqwest::StatusCode,
) -> bool {
    match surface {
        GitHubFetchSurface::Api | GitHubFetchSurface::Raw => {
            status.is_server_error()
                || status == reqwest::StatusCode::BAD_GATEWAY
                || status == reqwest::StatusCode::SERVICE_UNAVAILABLE
                || status == reqwest::StatusCode::GATEWAY_TIMEOUT
        }
    }
}

fn is_retryable_github_transport_error(error: &reqwest::Error) -> bool {
    error.is_timeout()
        || error.is_connect()
        || error.is_request()
        || error.is_body()
        || error.is_decode()
}

// Preserve diagnostic categories without exposing request URLs, credentials or
// arbitrary proxy/server text from a nested error chain.
fn github_transport_error_detail(error: &reqwest::Error) -> String {
    use std::error::Error;
    if let Some(status) = error.status() {
        return format!("HTTP {status}");
    }
    if error.is_timeout() {
        return "network request or download timed out".into();
    }
    let mut chain = String::new();
    let mut source = error.source();
    while let Some(cause) = source {
        chain.push_str(&cause.to_string().to_ascii_lowercase());
        source = cause.source();
    }
    if chain.contains("certificate") || chain.contains("tls") || chain.contains("ssl") {
        "network TLS connection failed; check the proxy and certificate configuration".into()
    } else if chain.contains("dns") || chain.contains("resolve") || chain.contains("lookup") {
        "network DNS resolution failed".into()
    } else if error.is_body() || error.is_decode() {
        "network download interrupted or response body incomplete".into()
    } else {
        "network connection failed; check the system proxy and network path".into()
    }
}

fn summarize_mirror_attempts(attempts: &[MirrorAttemptOutcome]) -> String {
    attempts
        .iter()
        .map(|attempt| attempt.error_message.clone())
        .collect::<Vec<_>>()
        .join("; ")
}

fn surface_label(surface: GitHubFetchSurface) -> &'static str {
    match surface {
        GitHubFetchSurface::Api => "API",
        GitHubFetchSurface::Raw => "raw",
    }
}

async fn classify_github_denial_response(
    response: reqwest::Response,
    operation: &'static str,
) -> Option<String> {
    parse_github_denial_response(response, operation)
        .await
        .map(|denial| denial.to_string())
}

async fn parse_github_denial_response(
    response: reqwest::Response,
    operation: &'static str,
) -> Option<GitHubAccessDenial> {
    let status = response.status();
    if status != reqwest::StatusCode::UNAUTHORIZED
        && status != reqwest::StatusCode::FORBIDDEN
        && status != reqwest::StatusCode::TOO_MANY_REQUESTS
    {
        return None;
    }

    let headers = response.headers().clone();
    let body = response.text().await.ok();
    let github_message = body.as_deref().and_then(parse_github_error_message);

    let remaining = header_value(&headers, "x-ratelimit-remaining");
    let reset_at = header_value(&headers, "x-ratelimit-reset")
        .as_deref()
        .and_then(parse_rate_limit_reset_epoch);

    let message_lower = github_message
        .as_deref()
        .unwrap_or_default()
        .to_ascii_lowercase();
    let remaining_is_zero = remaining.as_deref() == Some("0");
    let kind = if status == reqwest::StatusCode::TOO_MANY_REQUESTS
        || remaining_is_zero
        || message_lower.contains("rate limit")
        || message_lower.contains("api rate limit exceeded")
        || header_value(&headers, "x-ratelimit-resource").is_some()
    {
        GitHubAccessDenialKind::RateLimited {
            reset_at,
            remaining,
        }
    } else {
        GitHubAccessDenialKind::AuthenticationOrPermission
    };

    Some(GitHubAccessDenial {
        kind,
        operation,
        status,
        github_message,
    })
}

fn parse_github_error_message(body: &str) -> Option<String> {
    serde_json::from_str::<GitHubErrorResponse>(body)
        .ok()
        .and_then(|payload| payload.message)
}

fn header_value(headers: &reqwest::header::HeaderMap, name: &str) -> Option<String> {
    headers
        .get(name)
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn parse_rate_limit_reset_epoch(raw: &str) -> Option<String> {
    let epoch = raw.parse::<i64>().ok()?;
    chrono::DateTime::<Utc>::from_timestamp(epoch, 0)
        .map(|dt| dt.format("%Y-%m-%d %H:%M:%S").to_string())
}

fn parse_frontmatter(content: &str) -> Option<SkillFrontmatter> {
    let normalized = content.strip_prefix('\u{feff}').unwrap_or(content);
    let mut lines = normalized.lines();
    if lines.next()?.trim() != "---" {
        return None;
    }
    let mut yaml = String::new();
    for line in lines {
        if line.trim() == "---" {
            return serde_yaml::from_str::<SkillFrontmatter>(&yaml)
                .ok()
                .or_else(|| parse_frontmatter_required_fields(&yaml));
        }
        yaml.push_str(line);
        yaml.push('\n');
    }
    None
}

fn parse_frontmatter_required_fields(yaml: &str) -> Option<SkillFrontmatter> {
    let name = parse_frontmatter_scalar(yaml, "name")?;
    let description = parse_frontmatter_scalar(yaml, "description");
    Some(SkillFrontmatter { name, description })
}

fn parse_frontmatter_scalar(yaml: &str, key: &str) -> Option<String> {
    let prefix = format!("{key}:");
    yaml.lines().find_map(|line| {
        let trimmed = line.trim_start();
        let value = trimmed.strip_prefix(&prefix)?.trim();
        if value.is_empty() {
            return None;
        }
        Some(
            value
                .trim_matches('"')
                .trim_matches('\'')
                .trim()
                .to_string(),
        )
    })
}

fn sanitize_skill_id(raw: &str) -> Result<String, String> {
    let lowered = raw.trim().to_lowercase();
    let mut sanitized = String::new();
    let mut last_was_dash = false;
    for ch in lowered.chars() {
        if ch.is_ascii_alphanumeric() {
            sanitized.push(ch);
            last_was_dash = false;
        } else if !last_was_dash {
            sanitized.push('-');
            last_was_dash = true;
        }
    }
    let sanitized = sanitized.trim_matches('-').to_string();
    if sanitized.is_empty() {
        return Err(format!("Skill identifier '{}' is not supported.", raw));
    }
    Ok(sanitized)
}

#[cfg(test)]
mod tests {

    #[tokio::test]
    #[ignore = "Requires live access to a public GitHub repository"]
    async fn live_repository_documents_refresh() {
        let pool = setup_test_db().await;
        let tmp = tempdir().unwrap();
        let root = tmp.path().join("library");
        let directory = root.join("KingKongRobotics/jumper");
        let skill_dir = directory.join("test-fixture");
        std::fs::create_dir_all(&skill_dir).unwrap();
        std::fs::write(skill_dir.join("SKILL.md"), "test fixture must stay unchanged").unwrap();
        db::set_skill_resource_library_dir(&pool, &root.to_string_lossy()).await.unwrap();
        db::upsert_skill(&pool,&Skill {id:"readme-live-fixture".into(),name:"Readme live fixture".into(),description:None,
            file_path:skill_dir.join("SKILL.md").to_string_lossy().into_owned(),canonical_path:Some(skill_dir.to_string_lossy().into_owned()),
            is_central:false,source:Some("github:KingKongRobotics/jumper".into()),content:None,scanned_at:"fixture".into()}).await.unwrap();
        db::upsert_skill_source(&pool,&db::SkillSource {skill_id:"readme-live-fixture".into(),source_type:"github".into(),source_repo:Some("KingKongRobotics/jumper".into()),source_author:Some("KingKongRobotics".into()),source_url:None,source_path:None,updated_at:String::new()}).await.unwrap();
        let stars = refresh_repository_impl(&pool,"KingKongRobotics/jumper").await.unwrap_or_else(|_| panic!("Public GitHub refresh failed; remote error details withheld"));
        let documents = repository_docs::read(&directory).unwrap();
        assert!(documents.iter().any(|doc| doc.language=="default" && !doc.content.is_empty()));
        assert!(documents.iter().any(|doc| doc.language=="zh" && !doc.content.is_empty()));
        assert_eq!(db::get_github_stars(&pool,"kingkongrobotics/jumper").await.unwrap(),Some(stars));
        assert_eq!(std::fs::read_to_string(skill_dir.join("SKILL.md")).unwrap(),"test fixture must stay unchanged");
        println!("Public repository refresh passed: {} README documents; star count saved; skill file unchanged", documents.len());
    }

    #[tokio::test]
    async fn repository_metadata_refresh_updates_multilingual_docs_without_touching_skills() {
        let pool = setup_test_db().await;
        let tmp = tempdir().unwrap();
        let root = tmp.path().join("library");
        let directory = root.join("Example/Skills");
        let skill_dir = directory.join("demo");
        std::fs::create_dir_all(&skill_dir).unwrap();
        std::fs::write(skill_dir.join("SKILL.md"), "original skill").unwrap();
        std::fs::write(directory.join("README.md"), "old default").unwrap();
        std::fs::write(directory.join("README_CN.md"), "old Chinese").unwrap();
        db::set_skill_resource_library_dir(&pool, &root.to_string_lossy()).await.unwrap();
        db::upsert_skill(&pool, &Skill {id:"metadata-demo".into(), name:"Demo".into(), description:None,
            file_path:skill_dir.join("SKILL.md").to_string_lossy().into_owned(), canonical_path:Some(skill_dir.to_string_lossy().into_owned()),
            is_central:false, source:Some("github:Example/Skills".into()),content:None,scanned_at:"original-time".into()}).await.unwrap();
        db::upsert_skill_source(&pool, &db::SkillSource {skill_id:"metadata-demo".into(),source_type:"github".into(),source_repo:Some("Example/Skills".into()),source_author:Some("Example".into()),source_url:None,source_path:Some("skills/demo/SKILL.md".into()),updated_at:String::new()}).await.unwrap();
        let repo = GitHubRepoRef {owner:"Example".into(),repo:"Skills".into(),branch:"main".into(),normalized_url:"https://github.com/Example/Skills".into(),stars:Some(42)};
        let snapshot = GitHubRepoSnapshot {files:HashMap::from([
            ("README.md".into(), b"new default".to_vec()),
            ("README.zh.md".into(), "中文说明".as_bytes().to_vec()),
            ("README.en.md".into(), b"English guide".to_vec()),
            ("skills/demo/SKILL.md".into(), b"must not be imported".to_vec()),
        ])};
        apply_repository_metadata(&pool,"example/skills",&repo,&snapshot).await.unwrap();
        assert_eq!(std::fs::read_to_string(skill_dir.join("SKILL.md")).unwrap(), "original skill");
        assert!(!directory.join("README_CN.md").exists());
        let documents = repository_docs::read(&directory).unwrap();
        assert_eq!(documents.iter().map(|doc| doc.name.as_str()).collect::<Vec<_>>(), vec!["README.md", "README.zh.md", "README.en.md"]);
        assert_eq!(documents[1].content, "中文说明");
        assert_eq!(db::get_skill_by_id(&pool,"metadata-demo").await.unwrap().unwrap().scanned_at, "original-time");
        assert_eq!(db::get_github_stars(&pool,"example/skills").await.unwrap(),Some(42));
        sqlx::query("CREATE TRIGGER reject_metadata BEFORE INSERT ON github_repository_stats BEGIN SELECT RAISE(ABORT, 'test failure'); END").execute(&pool).await.unwrap();
        let empty = GitHubRepoSnapshot {files:HashMap::new()};
        assert!(apply_repository_metadata(&pool,"example/skills",&repo,&empty).await.is_err());
        assert_eq!(repository_docs::read(&directory).unwrap().len(),3,"a database failure must restore all previous READMEs");
        assert_eq!(db::get_github_stars(&pool,"example/skills").await.unwrap(),Some(42));
    }

    #[tokio::test]
    async fn repository_readme_only_revision_is_detected_and_imported() {
        let pool = setup_test_db().await;
        let tmp = tempdir().unwrap();
        let library = tmp.path().join("library");
        std::fs::create_dir_all(&library).unwrap();
        db::set_skill_resource_library_dir(&pool, &library.to_string_lossy()).await.unwrap();
        let repository = format!("readme-{}", uuid::Uuid::new_v4());
        let mut versions = Vec::new();
        for (commit, readme) in [("a".repeat(40), Some("# First")), ("b".repeat(40), Some("# Second")), ("c".repeat(40), None)] {
            let repo = GitHubRepoRef { owner:"example".into(), repo:repository.clone(), branch:commit.clone(), normalized_url:format!("https://github.com/example/{repository}"), stars:None };
            let mut files = HashMap::from([("skills/demo/SKILL.md".into(), b"---\nname: Demo\n---\nDemo".to_vec())]);
            if let Some(readme) = readme {
                files.insert("README.md".into(), readme.as_bytes().to_vec());
                files.insert("README.zh.md".into(), format!("中文 {readme}").into_bytes());
                files.insert("README.en.md".into(), format!("English {readme}").into_bytes());
            }
            load_repo_snapshot(&repo, None, || async { Ok(GitHubRepoSnapshot {files}) }).await.unwrap();
            let (candidates, contents) = fetch_repo_preview_files(&repo, None).await.unwrap();
            let remote = &contents[&candidates[0].source_path];
            assert_eq!(remote.get("../README.md").map(|bytes| std::str::from_utf8(bytes).unwrap()), readme);
            versions.push(remote.clone());
            import_github_repo_skills_impl(&pool, &format!("https://github.com/example/{repository}/tree/{commit}"),
                vec![GitHubSkillImportSelection {source_path:"skills/demo/SKILL.md".into(), resolution:DuplicateResolution::Overwrite, renamed_skill_id:None}], None).await.unwrap();
            let path = library.join("example").join(&repository).join("README.md");
            assert_eq!(std::fs::read_to_string(path).ok().as_deref(), readme);
            assert_eq!(std::fs::read_to_string(library.join("example").join(&repository).join("README.zh.md")).ok(), readme.map(|text| format!("中文 {text}")));
            assert_eq!(std::fs::read_to_string(library.join("example").join(&repository).join("README.en.md")).ok(), readme.map(|text| format!("English {text}")));
        }
        assert_ne!(versions[0], versions[1]);
        assert_ne!(versions[1], versions[2]);
    }

    #[test]
    fn repository_readme_update_and_removal_roll_back_with_skills() {
        let tmp = tempdir().unwrap();
        let library = tmp.path().join("library");
        let directory = library.join("example/repo");
        std::fs::create_dir_all(&directory).unwrap();
        let target = directory.join("README.md");
        std::fs::write(&target, b"old").unwrap();
        let snapshot = GitHubRepoSnapshot { files: HashMap::from([("readme.md".into(), b"new".to_vec())]) };
        {
            let mut transaction = ImportDirectoryTransaction::new(&library).unwrap();
            stage_repository_readme(&snapshot, &directory, &mut transaction).unwrap();
            assert_eq!(std::fs::read(&target).unwrap(), b"new");
        }
        assert_eq!(std::fs::read(&target).unwrap(), b"old");
        {
            let mut transaction = ImportDirectoryTransaction::new(&library).unwrap();
            stage_repository_readme(&snapshot, &directory, &mut transaction).unwrap();
            transaction.commit();
        }
        assert_eq!(std::fs::read(&target).unwrap(), b"new");
        let empty = GitHubRepoSnapshot { files: HashMap::new() };
        {
            let mut transaction = ImportDirectoryTransaction::new(&library).unwrap();
            stage_repository_readme(&empty, &directory, &mut transaction).unwrap();
            assert!(!target.exists());
        }
        assert_eq!(std::fs::read(&target).unwrap(), b"new");
        let mut transaction = ImportDirectoryTransaction::new(&library).unwrap();
        stage_repository_readme(&empty, &directory, &mut transaction).unwrap();
        transaction.commit();
        assert!(!target.exists());
    }

    #[tokio::test]
    async fn sparse_skill_snapshot_does_not_download_unrelated_repository_media() {
        let tree = serde_json::json!({"truncated":false,"tree":[
            {"path":"public/demo.mp4","type":"blob","size":700_000_000},
            {"path":"README.md","type":"blob","size":100},
            {"path":"README.zh.md","type":"blob","size":100},
            {"path":"skill-package/story/SKILL.md","type":"blob","size":100},
            {"path":"skill-package/story/references/guide.md","type":"blob","size":100},
            {"path":"skill-package/story/assets/example.png","type":"blob","size":200},
            {"path":"skill-package/other.txt","type":"blob","size":100}
        ]});
        let snapshot = snapshot_from_tree_if_sparse(&tree, |path| async move {
            assert!(repository_docs::language(&path).is_some() || path.starts_with("skill-package/story/"), "must not fetch unrelated media");
            Ok(path.into_bytes())
        }).await.unwrap().expect("large media repository must use skill-only downloads");
        assert_eq!(snapshot.files.len(), 5);
        assert!(snapshot.files.contains_key("README.zh.md"));
        assert!(snapshot.files.contains_key("README.md"));
        assert!(snapshot.files.contains_key("skill-package/story/assets/example.png"));
        let files = collect_snapshot_source_files(&snapshot, "skill-package/story/SKILL.md").unwrap();
        assert_eq!(files.len(), 3, "import and update must retain nested skill resources");
    }

    #[tokio::test]
    async fn sparse_snapshot_keeps_archive_for_incomplete_trees_and_many_skill_files() {
        let mut many_files = vec![serde_json::json!({"path":"skills/demo/SKILL.md","type":"blob","size":100}),
            serde_json::json!({"path":"public/video.mp4","type":"blob","size":700_000_000})];
        many_files.extend((0..64).map(|index| serde_json::json!({"path":format!("skills/demo/references/{index}.md"),"type":"blob","size":100})));
        for tree in [serde_json::json!({"truncated":true,"tree":[]}),
            serde_json::json!({"truncated":false,"tree":[{"path":"SKILL.md","type":"blob","size":100}]}),
            serde_json::json!({"truncated":false,"tree":many_files})] {
            assert!(snapshot_from_tree_if_sparse(&tree, |_| async { panic!("archive path must not fetch individual files") }).await.unwrap().is_none());
        }
    }

    #[tokio::test]
    #[ignore = "Requires live access to the reported public repository"]
    async fn live_story_repository_preview_completes_promptly() {
        let pool = setup_test_db().await;
        let started = std::time::Instant::now();
        let result = tokio::time::timeout(std::time::Duration::from_secs(60),
            preview_github_repo_import_impl(&pool, "https://github.com/gnipbao/story-to-handdrawn-video")).await;
        match result {
            Ok(Ok(preview)) => {
                println!("preview: skills={}, elapsed_ms={}", preview.skills.len(), started.elapsed().as_millis());
                assert!(preview.skills.iter().any(|skill| skill.source_path == "skill-package/story-to-handdrawn-video/SKILL.md"));
                let snapshot = download_repo_snapshot(&github_client().unwrap(), &preview.repo, None).await.unwrap();
                let candidate = &preview.skills[0];
                let files = collect_snapshot_source_files(&snapshot, &candidate.source_path).unwrap();
                let target = tempdir().unwrap();
                write_snapshot_source_to_target(&snapshot, &files, target.path(), &candidate.source_path,
                    &mut GitHubImportProgressState::default(), None).unwrap();
                assert!(target.path().join("SKILL.md").is_file());
                let (update_candidates, update_files) = fetch_repo_preview_files(&preview.repo, None).await.unwrap();
                assert_eq!(update_candidates.len(), 1);
                assert_eq!(update_files[&candidate.source_path].len(), files.len());
                println!("staged_skill_files={}, cached_update_preview_ms={}", files.len(), started.elapsed().as_millis());
            }
            Ok(Err(error)) => panic!("public preview failed: {}", error),
            Err(_) => panic!("public preview still preparing after 60 seconds"),
        }
    }

    #[tokio::test]
    #[ignore = "Downloads a public large repository; run explicitly for performance verification"]
    async fn live_large_repository_snapshot_reuse() {
        let repo = resolve_update_revision("k-dense-ai/scientific-agent-skills", None, None).await
            .expect("public repository revision lookup failed");
        let client = github_client().unwrap();
        let started = std::time::Instant::now();
        let snapshot = download_repo_snapshot(&client, &repo, None).await.expect("public snapshot download failed");
        let first = started.elapsed();
        let skills = build_repo_skill_candidates_from_snapshot(&repo, &snapshot).unwrap();
        let started = std::time::Instant::now();
        for _ in 0..100 {
            let reused = download_repo_snapshot(&client, &repo, None).await.unwrap();
            assert!(std::sync::Arc::ptr_eq(&snapshot, &reused));
        }
        println!("large repository: skills={}, files={}, bytes={}, first_snapshot_ms={}, cached_100_operations_ms={}",
            skills.len(), snapshot.files.len(), snapshot.files.values().map(Vec::len).sum::<usize>(),
            first.as_millis(), started.elapsed().as_millis());
    }

    #[tokio::test]
    async fn update_batch_resolves_head_once_and_a_new_check_resolves_it_again() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        let repository = format!("example/{}", uuid::Uuid::new_v4());
        let requests = AtomicUsize::new(0);
        let load = || async {
            requests.fetch_add(1, Ordering::SeqCst);
            Ok(GitHubRepoRef {owner:"example".into(),repo:"fixture".into(),branch:"a".repeat(40),normalized_url:String::new(),stars:None})
        };
        for _ in 0..100 {
            load_batch_revision(&repository, None, Some("first-batch"), load).await.unwrap();
        }
        assert_eq!(requests.load(Ordering::SeqCst), 1, "one update batch must not inspect repository metadata per skill");
        load_batch_revision(&repository, None, Some("next-batch"), load).await.unwrap();
        load_batch_revision(&repository, None, None, load).await.unwrap();
        load_batch_revision(&repository, None, None, load).await.unwrap();
        assert_eq!(requests.load(Ordering::SeqCst), 4, "new batches and fresh checks must resolve HEAD again");
    }

    #[tokio::test]
    async fn repeated_skill_operations_download_and_unpack_a_revision_once() {
        let repo = GitHubRepoRef {owner:"example".into(),repo:"performance-fixture".into(),
            branch:"1234567890123456789012345678901234567890".into(),normalized_url:String::new(),stars:None};
        let archive = repository_archive(&[("skills/demo/SKILL.md", b"---\nname: Demo\n---\nDemo")]);
        let downloads = std::sync::atomic::AtomicUsize::new(0);
        for _ in 0..20 {
            let snapshot = load_repo_snapshot(&repo, None, || async {
                downloads.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                snapshot_from_repository_archive(&archive)
            }).await.unwrap();
            assert!(snapshot.files.contains_key("skills/demo/SKILL.md"));
        }
        assert_eq!(downloads.load(std::sync::atomic::Ordering::SeqCst), 1,
            "per-skill import/update operations must reuse the same immutable repository archive");
    }

    #[tokio::test]
    async fn snapshot_cache_shares_concurrent_loads_but_isolates_revisions_and_auth_scopes() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        let mut repo = GitHubRepoRef {owner:"example".into(),repo:uuid::Uuid::new_v4().to_string(),
            branch:"1234567890123456789012345678901234567890".into(),normalized_url:String::new(),stars:None};
        let downloads = AtomicUsize::new(0);
        let load = || async {
            downloads.fetch_add(1, Ordering::SeqCst);
            tokio::task::yield_now().await;
            Ok(GitHubRepoSnapshot::default())
        };
        let (first, second) = tokio::join!(load_repo_snapshot(&repo, None, load), load_repo_snapshot(&repo, None, load));
        assert!(std::sync::Arc::ptr_eq(&first.unwrap(), &second.unwrap()));
        assert_eq!(downloads.load(Ordering::SeqCst), 1);
        load_repo_snapshot(&repo, Some("fixture-auth-scope"), load).await.unwrap();
        repo.branch = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".into();
        load_repo_snapshot(&repo, None, load).await.unwrap();
        assert_eq!(downloads.load(Ordering::SeqCst), 3);
        repo.branch = "main".into();
        load_repo_snapshot(&repo, None, load).await.unwrap();
        load_repo_snapshot(&repo, None, load).await.unwrap();
        assert_eq!(downloads.load(Ordering::SeqCst), 5, "mutable branches must be fetched afresh");
    }

    #[tokio::test]
    async fn snapshot_cache_retries_failures_and_reuses_the_success() {
        let repo = GitHubRepoRef {owner:"example".into(),repo:uuid::Uuid::new_v4().to_string(),
            branch:"1234567890123456789012345678901234567890".into(),normalized_url:String::new(),stars:None};
        assert!(load_repo_snapshot(&repo, None, || async {Err("fixture network failure".into())}).await.is_err());
        let loaded = load_repo_snapshot(&repo, None, || async {Ok(GitHubRepoSnapshot::default())}).await.unwrap();
        let cached = load_repo_snapshot(&repo, None, || async {panic!("must reuse the successful snapshot")}).await.unwrap();
        assert!(std::sync::Arc::ptr_eq(&loaded, &cached));
    }

    fn archive_test_server(
        responses: Vec<(&'static str, u64)>,
    ) -> (String, std::thread::JoinHandle<Vec<String>>) {
        use std::io::{Read, Write};
        use std::net::TcpListener;
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let address = format!("http://{}", listener.local_addr().unwrap());
        let server = std::thread::spawn(move || {
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
            let mut requests = Vec::new();
            for (response, delay) in responses {
                loop {
                    if std::time::Instant::now() > deadline {
                        return requests;
                    }
                    let Ok((mut stream, _)) = listener.accept() else {
                        std::thread::sleep(std::time::Duration::from_millis(5));
                        continue;
                    };
                    stream.set_nonblocking(false).unwrap();
                    stream
                        .set_read_timeout(Some(std::time::Duration::from_secs(1)))
                        .unwrap();
                    let mut buffer = [0; 2048];
                    let n = stream.read(&mut buffer).unwrap();
                    requests.push(String::from_utf8_lossy(&buffer[..n]).to_string());
                    let (head, body) = response.split_once("\r\n\r\n").unwrap();
                    let _ = stream.write_all(format!("{head}\r\n\r\n").as_bytes());
                    std::thread::sleep(std::time::Duration::from_millis(delay));
                    let _ = stream.write_all(body.as_bytes());
                    break;
                }
            }
            requests
        });
        (address, server)
    }

    #[tokio::test]
    async fn archive_body_retry_recovers_on_direct_endpoint() {
        let (address, server) = archive_test_server(vec![
            (
                "HTTP/1.1 200 OK\r\nConnection: close\r\nContent-Length: 100\r\n\r\npartial",
                0,
            ),
            (
                "HTTP/1.1 200 OK\r\nConnection: close\r\nContent-Length: 8\r\n\r\ncomplete",
                0,
            ),
        ]);
        let client = reqwest::Client::builder().no_proxy().build().unwrap();
        let result = send_github_bytes_with_fallback(
            &client,
            GitHubFetchSurface::Api,
            |_| address.clone(),
            "archive download failed",
            None,
        )
        .await
        .unwrap();
        assert_eq!(result, b"complete");
        assert_eq!(server.join().unwrap().len(), 2);
    }

    #[tokio::test]
    async fn archive_body_stall_retries_with_read_timeout() {
        let (address, server) = archive_test_server(vec![
            (
                "HTTP/1.1 200 OK\r\nConnection: close\r\nContent-Length: 8\r\n\r\ncomplete",
                150,
            ),
            (
                "HTTP/1.1 200 OK\r\nConnection: close\r\nContent-Length: 8\r\n\r\ncomplete",
                0,
            ),
        ]);
        let client = reqwest::Client::builder()
            .no_proxy()
            .read_timeout(std::time::Duration::from_millis(75))
            .build()
            .unwrap();
        let result = send_github_bytes_with_fallback(
            &client,
            GitHubFetchSurface::Api,
            |_| address.clone(),
            "archive download failed",
            None,
        )
        .await
        .unwrap();
        assert_eq!(result, b"complete");
        assert_eq!(server.join().unwrap().len(), 2);
    }

    #[tokio::test]
    async fn archive_body_retry_is_bounded_and_mirror_denial_is_not_auth_failure() {
        let mut responses = vec![
            (
                "HTTP/1.1 200 OK\r\nConnection: close\r\nContent-Length: 100\r\n\r\npartial",
                0
            );
            2
        ];
        responses.extend(vec![
            (
                "HTTP/1.1 403 Forbidden\r\nConnection: close\r\nContent-Length: 0\r\n\r\n",
                0
            );
            3
        ]);
        let (address, server) = archive_test_server(responses);
        let client = reqwest::Client::builder().no_proxy().build().unwrap();
        let error = send_github_bytes_with_fallback(
            &client,
            GitHubFetchSurface::Api,
            |_| format!("{address}/private-repository?secret=test-only-token"),
            "archive download failed",
            Some("test-only-token"),
        )
        .await
        .unwrap_err();
        assert!(error.contains("response body failed"));
        assert!(!error.contains("token/permissions"));
        assert!(!error.contains("test-only-token"));
        assert!(!error.contains("private-repository"));
        assert_eq!(server.join().unwrap().len(), 5);
    }

    #[tokio::test]
    async fn archive_permission_denial_is_not_retried() {
        let (address, server) = archive_test_server(vec![(
            "HTTP/1.1 403 Forbidden\r\nConnection: close\r\nContent-Length: 0\r\n\r\n",
            0,
        )]);
        let client = reqwest::Client::builder().no_proxy().build().unwrap();
        let error = send_github_bytes_with_fallback(
            &client,
            GitHubFetchSurface::Api,
            |_| address.clone(),
            "archive download failed",
            Some("test-only-token"),
        )
        .await
        .unwrap_err();
        assert!(error.contains("denied access"));
        assert_eq!(server.join().unwrap().len(), 1);
    }
    // Real HTTP responses: a successful status followed by an incomplete body
    // must retry the whole download instead of accepting or appending partial data.
    #[tokio::test]
    async fn archive_body_interruption_retries_and_discards_partial_bytes() {
        let (address, server) = archive_test_server(vec![
            ("HTTP/1.1 200 OK\r\nConnection: close\r\nContent-Length: 100\r\n\r\npartial", 0),
            ("HTTP/1.1 200 OK\r\nConnection: close\r\nContent-Length: 100\r\n\r\npartial", 0),
            ("HTTP/1.1 200 OK\r\nConnection: close\r\nContent-Length: 8\r\n\r\ncomplete", 0),
        ]);
        let client = reqwest::Client::builder().no_proxy().build().unwrap();
        let result = send_github_bytes_with_fallback(&client, GitHubFetchSurface::Api,
            |endpoint| format!("{address}/{}", if endpoint.label == "github" {"direct"} else {"mirror"}),
            "Failed to download GitHub repository archive", Some("test-only-token")).await;
        let requests = server.join().unwrap();
        assert_eq!(result.unwrap(), b"complete");
        assert_eq!(requests.len(), 3);
        assert!(requests[..2].iter().all(|r| r.to_lowercase().contains("authorization: bearer test-only-token")));
        assert!(!requests[2].to_lowercase().contains("authorization"));
    }
    use super::*;
    use flate2::{write::GzEncoder, Compression};
    use std::collections::HashMap;
    use tempfile::tempdir;

    async fn setup_test_db() -> DbPool {
        let dir = tempdir().expect("tempdir");
        let db_path = dir.path().join("github-import.sqlite");
        let pool = db::create_pool(db_path.to_str().unwrap())
            .await
            .expect("create db");
        db::init_database(&pool).await.expect("init db");
        std::mem::forget(dir);
        pool
    }

    fn sample_frontmatter(name: &str, description: &str) -> String {
        format!("---\nname: {name}\ndescription: {description}\n---\n\n# {name}\n")
    }

    fn repo_snapshot(files: &[(&str, String)]) -> GitHubRepoSnapshot {
        GitHubRepoSnapshot {
            files: files
                .iter()
                .map(|(path, content)| (path.to_string(), content.as_bytes().to_vec()))
                .collect::<HashMap<_, _>>(),
        }
    }

    fn root_repo_snapshot() -> GitHubRepoSnapshot {
        repo_snapshot(&[
            (
                "SKILL.md",
                sample_frontmatter("twitterapi-io", "root skill"),
            ),
            ("README.md", "# repo\n".to_string()),
        ])
    }

    fn multi_skill_snapshot() -> GitHubRepoSnapshot {
        repo_snapshot(&[
            (
                "skills/agent-planner/SKILL.md",
                sample_frontmatter("Agent Planner", "Agent Planner description"),
            ),
            (
                "skills/commit/SKILL.md",
                sample_frontmatter("Commit", "Commit description"),
            ),
            (
                "skills/code-review/SKILL.md",
                sample_frontmatter("Code Review", "Code Review description"),
            ),
            ("skills/commit/README.md", "# commit\n".to_string()),
        ])
    }

    fn namespaced_skill_snapshot() -> GitHubRepoSnapshot {
        repo_snapshot(&[
            (
                "skills/.curated/openai-docs/SKILL.md",
                sample_frontmatter("openai-docs", "OpenAI docs skill"),
            ),
            (
                "skills/.curated/openai-docs/references/api.md",
                "# api\n".to_string(),
            ),
            (
                "skills/.system/skill-creator/SKILL.md",
                sample_frontmatter("skill-creator", "Create skills"),
            ),
            (
                "skills/.system/skill-creator/scripts/init_skill.py",
                "print('hi')\n".to_string(),
            ),
        ])
    }

    fn plugin_skill_snapshot() -> GitHubRepoSnapshot {
        repo_snapshot(&[
            (
                "plugins/agent-native-design/skills/agent-native-design/SKILL.md",
                sample_frontmatter("agent-native-design", "Agent-native CLI design"),
            ),
            (
                "plugins/agent-native-design/skills/agent-native-design/references/cli.md",
                "# CLI\n".to_string(),
            ),
            (
                "plugins/pi/skills/pi-cli-runtime/SKILL.md",
                sample_frontmatter("pi-cli-runtime", "Pi runtime"),
            ),
            (
                "plugins/pi/skills/pi-prompting/SKILL.md",
                sample_frontmatter("pi-prompting", "Pi prompting"),
            ),
            ("plugins/pi/plugin.json", "{}\n".to_string()),
        ])
    }

    fn repository_archive(files: &[(&str, &[u8])]) -> Vec<u8> {
        let encoder = GzEncoder::new(Vec::new(), Compression::default());
        let mut builder = tar::Builder::new(encoder);
        for (path, content) in files {
            let archive_path = format!("repo-snapshot/{}", path);
            let mut header = tar::Header::new_gnu();
            header.set_size(content.len() as u64);
            header.set_mode(0o644);
            header.set_entry_type(tar::EntryType::Regular);
            header.set_cksum();
            builder
                .append_data(&mut header, archive_path, *content)
                .expect("append archive entry");
        }
        let encoder = builder.into_inner().expect("finalize tar");
        encoder.finish().expect("finalize gzip")
    }

    #[tokio::test]
    async fn database_failure_rolls_back_skill_record_and_directory() {
        let pool = setup_test_db().await;
        let temp = tempdir().unwrap();
        let library = temp.path().join("library");
        let target = library.join("demo");
        std::fs::create_dir_all(&target).unwrap();
        std::fs::write(target.join("SKILL.md"), "original").unwrap();
        let original = Skill {
            id: "demo".into(),
            name: "Original".into(),
            description: None,
            file_path: target.join("SKILL.md").to_string_lossy().into_owned(),
            canonical_path: Some(target.to_string_lossy().into_owned()),
            is_central: false,
            source: None,
            content: None,
            scanned_at: Utc::now().to_rfc3339(),
        };
        db::upsert_skill(&pool, &original).await.unwrap();
        sqlx::query("CREATE TRIGGER reject_source_write BEFORE INSERT ON skill_sources BEGIN SELECT RAISE(ABORT, 'simulated metadata failure'); END")
            .execute(&pool).await.unwrap();
        {
            let mut files = ImportDirectoryTransaction::new(&library).unwrap();
            let staged = files.root.join("new");
            std::fs::create_dir(&staged).unwrap();
            std::fs::write(staged.join("SKILL.md"), "new").unwrap();
            let mut transaction = pool.begin().await.unwrap();
            files.install(&staged, &target, true).unwrap();
            let updated = Skill {
                name: "Updated".into(),
                ..original.clone()
            };
            db::upsert_skill_with_executor(&mut *transaction, &updated)
                .await
                .unwrap();
            // Simulate a metadata write failure after the directory and skill-row changes.
            let result = db::upsert_skill_source_with_executor(
                &mut *transaction,
                &db::SkillSource {
                    skill_id: "demo".into(),
                    source_type: "github".into(),
                    source_url: None,
                    source_author: None,
                    source_repo: None,
                    source_path: None,
                    updated_at: Utc::now().to_rfc3339(),
                },
            )
            .await;
            assert!(result.is_err());
        }
        assert_eq!(
            db::get_skill_by_id(&pool, "demo")
                .await
                .unwrap()
                .unwrap()
                .name,
            "Original"
        );
        assert_eq!(
            std::fs::read_to_string(target.join("SKILL.md")).unwrap(),
            "original"
        );
    }

    #[tokio::test]
    async fn cache_stars_refreshes_metadata_without_erasing_it_when_missing() {
        let pool = setup_test_db().await;
        let mut repo = GitHubRepoRef {
            owner: "Example".into(), repo: "Repo".into(), branch: "main".into(),
            normalized_url: "https://github.com/Example/Repo".into(), stars: Some(17),
        };
        cache_repository_stars(&pool, &repo).await.unwrap();
        assert_eq!(db::get_github_stars(&pool, "example/repo").await.unwrap(), Some(17));
        repo.stars = None;
        cache_repository_stars(&pool, &repo).await.unwrap();
        assert_eq!(db::get_github_stars(&pool, "Example/Repo").await.unwrap(), Some(17));
        repo.stars = Some(0);
        cache_repository_stars(&pool, &repo).await.unwrap();
        assert_eq!(db::get_github_stars(&pool, "Example/Repo").await.unwrap(), Some(0));
    }

    #[test]
    fn failed_batch_swap_restores_previously_replaced_skill() {
        let temp = tempdir().unwrap();
        let library = temp.path().join("library");
        let existing = library.join("cangjie-skill");
        std::fs::create_dir_all(&existing).unwrap();
        std::fs::write(existing.join("SKILL.md"), "original skill").unwrap();
        let staging_root;
        {
            let mut transaction = ImportDirectoryTransaction::new(&library).unwrap();
            staging_root = transaction.root.clone();
            let staged = transaction.root.join("new-0");
            std::fs::create_dir(&staged).unwrap();
            std::fs::write(staged.join("SKILL.md"), "updated skill").unwrap();
            transaction.install(&staged, &existing, true).unwrap();
            // A later skill fails after the first swap. The first skill must not disappear.
            assert!(transaction
                .install(
                    &transaction.root.join("missing"),
                    &library.join("second"),
                    true
                )
                .is_err());
        }
        assert_eq!(
            std::fs::read_to_string(existing.join("SKILL.md")).unwrap(),
            "original skill"
        );
        assert!(!library.join("second").exists());
        assert!(!staging_root.exists());
    }

    #[test]
    fn failed_overwrite_preserves_original_directory() {
        let temp = tempdir().unwrap();
        let library = temp.path().join("library");
        let existing = library.join("demo");
        std::fs::create_dir_all(&existing).unwrap();
        std::fs::write(existing.join("SKILL.md"), "original").unwrap();
        {
            let mut transaction = ImportDirectoryTransaction::new(&library).unwrap();
            assert!(transaction
                .install(&transaction.root.join("missing"), &existing, true)
                .is_err());
        }
        assert_eq!(
            std::fs::read_to_string(existing.join("SKILL.md")).unwrap(),
            "original"
        );
    }

    #[test]
    fn committed_swap_keeps_complete_new_skill_and_removes_recovery_copy() {
        let temp = tempdir().unwrap();
        let library = temp.path().join("library");
        let existing = library.join("demo");
        std::fs::create_dir_all(&existing).unwrap();
        std::fs::write(existing.join("SKILL.md"), "old").unwrap();
        let staging_root;
        {
            let mut transaction = ImportDirectoryTransaction::new(&library).unwrap();
            staging_root = transaction.root.clone();
            let staged = transaction.root.join("new");
            std::fs::create_dir(&staged).unwrap();
            std::fs::write(staged.join("SKILL.md"), "new").unwrap();
            std::fs::write(staged.join("asset.txt"), "asset").unwrap();
            transaction.install(&staged, &existing, true).unwrap();
            transaction.commit();
        }
        assert_eq!(
            std::fs::read_to_string(existing.join("SKILL.md")).unwrap(),
            "new"
        );
        assert_eq!(
            std::fs::read_to_string(existing.join("asset.txt")).unwrap(),
            "asset"
        );
        assert!(!staging_root.exists());
    }

    #[test]
    fn import_retains_verified_commit_instead_of_following_default_branch() {
        let sha="0123456789abcdef0123456789abcdef01234567";
        let url=format!("https://github.com/owner/repo/tree/{sha}");
        assert_eq!(pinned_commit_from_url(&url),Some(sha));
        assert_eq!(pinned_commit_from_url("https://github.com/owner/repo"),None);
        assert_eq!(pinned_commit_from_url("https://github.com/owner/repo/tree/main"),None);
    }
    #[test]
    fn parse_github_url_accepts_repository_shorthand() {
        assert_eq!(parse_github_url("  Owner/Repo.git  ").unwrap(), ("Owner".into(), "Repo".into()));
        assert!(parse_github_url("owner").is_err());
        assert!(parse_github_url("owner/repo?token=secret").is_err());
    }

    #[test]
    fn parse_github_url_preserves_owner_and_repo_case() {
        let (owner, repo) =
            parse_github_url("https://github.com/Anthropics/Skills/").expect("parse");
        assert_eq!(owner, "Anthropics");
        assert_eq!(repo, "Skills");
    }

    #[test]
    fn repository_metadata_restores_canonical_case() {
        let payload = serde_json::json!({"full_name": "Example/SkillRepo"});
        assert_eq!(repository_display_names(&payload, "example", "skillrepo"), ("Example".into(), "SkillRepo".into()));
        assert_eq!(repository_display_names(&serde_json::json!({}), "Example", "SkillRepo"), ("Example".into(), "SkillRepo".into()));
    }

    #[test]
    fn parse_github_url_rejects_non_github_hosts() {
        let error = parse_github_url("https://gitlab.com/example/repo").unwrap_err();
        assert!(error.contains("github.com"));
    }

    #[test]
    fn sanitize_skill_id_collapses_symbols() {
        let skill_id = sanitize_skill_id("My Cool_Skill!").expect("sanitize");
        assert_eq!(skill_id, "my-cool-skill");
    }

    #[test]
    fn parse_frontmatter_requires_yaml_block() {
        assert!(parse_frontmatter("# nope").is_none());
        let parsed = parse_frontmatter(&sample_frontmatter("alpha", "desc")).expect("fm");
        assert_eq!(parsed.name, "alpha");
        assert_eq!(parsed.description.as_deref(), Some("desc"));
    }

    #[test]
    fn parse_frontmatter_accepts_realistic_plugin_metadata() {
        let content = r#"---
name: assetseeker
description: Search free commercial-use creative assets across multiple online sources — photos, illustrations, icons, video footage, music, sound effects, and fonts.
argument-hint: "[asset type] [keyword]"
author: Agents365-ai
category: Content Creation
version: 1.0.0
metadata: {"openclaw":{"requires":{"bins":["python3"]},"env":["PEXELS_API_KEY"]},"primaryEnv":"PEXELS_API_KEY","emoji":"🔍"}
---
# Asset Seeker
"#;

        let parsed = parse_frontmatter(content).expect("frontmatter");
        assert_eq!(parsed.name, "assetseeker");
    }

    #[test]
    fn classify_github_rate_limit_denial_returns_actionable_message() {
        let denial = GitHubAccessDenial {
            kind: GitHubAccessDenialKind::RateLimited {
                reset_at: Some("2026-04-17 12:34:56".to_string()),
                remaining: Some("0".to_string()),
            },
            operation: "inspecting the repository",
            status: reqwest::StatusCode::FORBIDDEN,
            github_message: Some("API rate limit exceeded for 1.2.3.4.".to_string()),
        };

        let message = denial.to_string();

        assert!(message.contains("rate limit was exceeded"));
        assert!(message.contains("Retry later after 2026-04-17 12:34:56 UTC"));
        assert!(message.contains("authenticated GitHub requests"));
        assert!(message.contains("API rate limit exceeded"));
    }

    #[test]
    fn classify_github_permission_denial_returns_actionable_message() {
        let denial = GitHubAccessDenial {
            kind: GitHubAccessDenialKind::AuthenticationOrPermission,
            operation: "reading repository contents",
            status: reqwest::StatusCode::UNAUTHORIZED,
            github_message: Some("Requires authentication".to_string()),
        };

        let message = denial.to_string();

        assert!(message.contains("denied access"));
        assert!(message.contains("require authentication"));
        assert!(message.contains("token/permissions are insufficient"));
        assert!(message.contains("Requires authentication"));
    }

    #[test]
    fn raw_url_to_repo_path_parses_github_raw_urls() {
        let parsed = raw_url_to_repo_path(
            "https://raw.githubusercontent.com/owner/repo/main/skills/demo/SKILL.md",
        )
        .expect("parsed");

        assert_eq!(parsed.repo.owner, "owner");
        assert_eq!(parsed.repo.repo, "repo");
        assert_eq!(parsed.repo.branch, "main");
        assert_eq!(parsed.file_path, "skills/demo/SKILL.md");
    }

    #[test]
    fn raw_url_to_repo_path_ignores_non_github_raw_hosts() {
        assert!(raw_url_to_repo_path("https://example.com/file.txt").is_none());
    }

    #[test]
    fn mirror_status_retry_excludes_auth_denials() {
        assert!(should_retry_via_mirror_status(
            GitHubFetchSurface::Api,
            reqwest::StatusCode::BAD_GATEWAY
        ));
        assert!(!should_retry_via_mirror_status(
            GitHubFetchSurface::Api,
            reqwest::StatusCode::FORBIDDEN
        ));
        assert!(!should_retry_via_mirror_status(
            GitHubFetchSurface::Raw,
            reqwest::StatusCode::TOO_MANY_REQUESTS
        ));
    }

    #[test]
    fn summarize_mirror_attempts_reports_all_failures() {
        let message = summarize_mirror_attempts(&[
            MirrorAttemptOutcome {
                status: None,
                error_message: "API mirror 'github' failed: timeout".to_string(),
            },
            MirrorAttemptOutcome {
                status: Some(reqwest::StatusCode::BAD_GATEWAY),
                error_message: "API mirror 'ghfast' returned HTTP 502".to_string(),
            },
        ]);

        assert!(message.contains("timeout"));
        assert!(message.contains("HTTP 502"));
    }

    #[test]
    fn snapshot_from_repository_archive_strips_archive_root_directory() {
        let archive = repository_archive(&[
            (
                "skills/demo/SKILL.md",
                sample_frontmatter("Demo", "Archive demo").as_bytes(),
            ),
            ("README.md", b"# readme\n"),
        ]);

        let snapshot = snapshot_from_repository_archive(&archive).expect("snapshot");

        assert!(snapshot.files.contains_key("skills/demo/SKILL.md"));
        assert!(snapshot.files.contains_key("README.md"));
    }

    #[tokio::test]
    async fn preview_does_not_conflict_with_same_id_in_another_directory() {
        let pool = setup_test_db().await;
        let central_root = tempdir().expect("central");
        sqlx::query("UPDATE agents SET global_skills_dir = ? WHERE id = 'central'")
            .bind(central_root.path().to_string_lossy().into_owned())
            .execute(&pool)
            .await
            .expect("update central");

        let existing_dir = central_root.path().join("twitterapi-io");
        std::fs::create_dir_all(&existing_dir).expect("mkdir");
        std::fs::write(
            existing_dir.join("SKILL.md"),
            sample_frontmatter("twitterapi-io", "existing"),
        )
        .expect("write skill");

        db::upsert_skill(
            &pool,
            &Skill {
                id: "twitterapi-io".to_string(),
                name: "twitterapi-io".to_string(),
                description: Some("existing".to_string()),
                file_path: existing_dir.join("SKILL.md").to_string_lossy().into_owned(),
                canonical_path: Some(existing_dir.to_string_lossy().into_owned()),
                is_central: true,
                source: Some("local".to_string()),
                content: None,
                scanned_at: Utc::now().to_rfc3339(),
            },
        )
        .await
        .expect("upsert skill");

        let repo = GitHubRepoRef {
            owner: "dorukardahan".to_string(),
            repo: "twitterapi-io-skill".to_string(),
            branch: "main".to_string(),
            normalized_url: "https://github.com/dorukardahan/twitterapi-io-skill".to_string(),
            stars: None,
        };
        let candidates = build_repo_skill_candidates_from_snapshot(&repo, &root_repo_snapshot())
            .expect("candidates");
        let preview = GitHubRepoPreview {
            repo: repo.clone(),
            skills: build_preview_skills(&pool, &repo, &candidates)
                .await
                .expect("preview skills"),
        };

        assert!(!preview.skills.is_empty());
        let conflict = preview
            .skills
            .iter()
            .find(|skill| skill.skill_id == "twitterapi-io")
            .and_then(|skill| skill.conflict.clone());
        assert!(conflict.is_none(), "an ID collision outside the import destination is not an overwrite");

        let central_entries = std::fs::read_dir(central_root.path())
            .expect("read dir")
            .count();
        assert_eq!(central_entries, 1, "preview should not write to central");

        let library = tempdir().expect("isolated library");
        db::set_skill_resource_library_dir(&pool, &library.path().to_string_lossy()).await.unwrap();
        let candidate = &candidates[0];
        let repository = format!("{}/{}", repo.owner, repo.repo);
        let target = source_grouped_skill_dir(library.path(), Some(&repo.owner), Some(&repository), None, &candidate.skill_directory_name);
        std::fs::create_dir_all(&target).unwrap();
        std::fs::write(target.join("SKILL.md"), sample_frontmatter("Imported", "existing")).unwrap();
        let mut imported = db::get_skill_by_id(&pool, "twitterapi-io").await.unwrap().unwrap();
        imported.id = "namespaced-import".into();
        imported.is_central = false;
        imported.file_path = target.join("SKILL.md").to_string_lossy().into_owned();
        imported.canonical_path = Some(target.to_string_lossy().into_owned());
        db::upsert_skill(&pool, &imported).await.unwrap();
        let preview = build_preview_skills(&pool, &repo, &candidates).await.unwrap();
        assert_eq!(preview[0].conflict.as_ref().unwrap().existing_skill_id, "namespaced-import");
        let id = resolve_import_skill_id(&pool, candidate, &mut HashSet::new(), &target, &repo.owner, &repo.repo).await.unwrap();
        assert_eq!(id, "namespaced-import", "reimport must retain the destination record");
    }

    #[tokio::test]
    async fn import_repo_skills_honors_skip_rename_and_overwrite() {
        let pool = setup_test_db().await;
        let existing_root = tempdir().expect("existing root");
        let snapshot = multi_skill_snapshot();
        let repo = GitHubRepoRef {
            owner: "anthropics".to_string(),
            repo: "skills".to_string(),
            branch: "main".to_string(),
            normalized_url: "https://github.com/anthropics/skills".to_string(),
            stars: None,
        };

        let candidates =
            build_repo_skill_candidates_from_snapshot(&repo, &snapshot).expect("candidates");

        let agent_planner = candidates
            .iter()
            .find(|candidate| candidate.source_path == "skills/agent-planner/SKILL.md")
            .expect("agent planner");
        let commit = candidates
            .iter()
            .find(|candidate| candidate.source_path == "skills/commit/SKILL.md")
            .expect("commit");
        let code_review = candidates
            .iter()
            .find(|candidate| candidate.source_path == "skills/code-review/SKILL.md")
            .expect("code review");

        let agent_planner_dir = existing_root.path().join("agent-planner");
        let commit_dir = existing_root.path().join("commit");
        let code_review_dir = existing_root.path().join("code-review");
        for dir in [&agent_planner_dir, &commit_dir, &code_review_dir] {
            std::fs::create_dir_all(dir).expect("mkdir existing skill");
            std::fs::write(
                dir.join("SKILL.md"),
                sample_frontmatter("Existing", "existing"),
            )
            .expect("write existing skill");
        }

        db::upsert_skill(
            &pool,
            &Skill {
                id: agent_planner.skill_id.clone(),
                name: "Agent Planner".to_string(),
                description: Some("existing".to_string()),
                file_path: agent_planner_dir
                    .join("SKILL.md")
                    .to_string_lossy()
                    .into_owned(),
                canonical_path: Some(agent_planner_dir.to_string_lossy().into_owned()),
                is_central: true,
                source: Some("local".to_string()),
                content: None,
                scanned_at: Utc::now().to_rfc3339(),
            },
        )
        .await
        .expect("seed rename conflict");
        db::upsert_skill(
            &pool,
            &Skill {
                id: commit.skill_id.clone(),
                name: "Commit".to_string(),
                description: Some("existing".to_string()),
                file_path: commit_dir.join("SKILL.md").to_string_lossy().into_owned(),
                canonical_path: Some(commit_dir.to_string_lossy().into_owned()),
                is_central: true,
                source: Some("local".to_string()),
                content: None,
                scanned_at: Utc::now().to_rfc3339(),
            },
        )
        .await
        .expect("seed skip conflict");
        db::upsert_skill(
            &pool,
            &Skill {
                id: code_review.skill_id.clone(),
                name: "Code Review".to_string(),
                description: Some("existing".to_string()),
                file_path: code_review_dir
                    .join("SKILL.md")
                    .to_string_lossy()
                    .into_owned(),
                canonical_path: Some(code_review_dir.to_string_lossy().into_owned()),
                is_central: true,
                source: Some("local".to_string()),
                content: None,
                scanned_at: Utc::now().to_rfc3339(),
            },
        )
        .await
        .expect("seed overwrite conflict");

        let mut occupied = current_managed_skill_ids(&pool).await.expect("occupied");
        assert!(occupied.contains(&agent_planner.skill_id));
        assert!(occupied.contains(&commit.skill_id));
        assert!(occupied.contains(&code_review.skill_id));

        let rename_target = sanitize_skill_id("agent-planner-imported").expect("rename target");
        assert!(
            !occupied.contains(&rename_target),
            "rename target should be available before import"
        );
        occupied.insert(rename_target.clone());

        assert!(
            occupied.contains(&rename_target),
            "rename should reserve the requested canonical id"
        );
        assert!(
            occupied.contains(&code_review.skill_id),
            "overwrite keeps the original canonical id occupied"
        );
        assert!(
            occupied.contains(&commit.skill_id),
            "skip leaves the existing canonical id occupied without needing a new id"
        );
    }

    #[tokio::test]
    async fn import_repo_skills_keeps_same_named_skills_from_different_repos() {
        let pool = setup_test_db().await;
        let resource_root = tempdir().expect("resource root");
        db::set_skill_resource_library_dir(&pool, &resource_root.path().to_string_lossy())
            .await
            .expect("set resource root");

        let existing_dir = resource_root
            .path()
            .join("other-owner")
            .join("other-repo")
            .join("assetseeker");
        std::fs::create_dir_all(&existing_dir).expect("mkdir existing");
        std::fs::write(
            existing_dir.join("SKILL.md"),
            sample_frontmatter("assetseeker", "existing"),
        )
        .expect("write existing");
        db::upsert_skill(
            &pool,
            &Skill {
                id: "assetseeker".to_string(),
                name: "assetseeker".to_string(),
                description: Some("existing".to_string()),
                file_path: existing_dir.join("SKILL.md").to_string_lossy().into_owned(),
                canonical_path: Some(existing_dir.to_string_lossy().into_owned()),
                is_central: false,
                source: Some("github:other-owner/other-repo".to_string()),
                content: None,
                scanned_at: Utc::now().to_rfc3339(),
            },
        )
        .await
        .expect("seed existing");

        let snapshot = repo_snapshot(&[(
            "plugins/assetseeker/skills/assetseeker/SKILL.md",
            sample_frontmatter("assetseeker", "imported"),
        )]);
        let repo = GitHubRepoRef {
            owner: "agents365-ai".to_string(),
            repo: "365-skills".to_string(),
            branch: "main".to_string(),
            normalized_url: "https://github.com/Agents365-ai/365-skills".to_string(),
            stars: None,
        };
        let candidates =
            build_repo_skill_candidates_from_snapshot(&repo, &snapshot).expect("candidates");
        assert_eq!(candidates[0].skill_id, "assetseeker");

        let mut occupied = current_managed_skill_ids(&pool).await.expect("occupied");
        let source_repo = format!("{}/{}", repo.owner, repo.repo);
        let target_dir = source_grouped_skill_dir(
            resource_root.path(),
            Some(&repo.owner),
            Some(&source_repo),
            None,
            &candidates[0].skill_directory_name,
        );
        let resolved = resolve_import_skill_id(
            &pool,
            &candidates[0],
            &mut occupied,
            &target_dir,
            &repo.owner,
            &repo.repo,
        )
        .await
        .expect("resolve id");
        assert_eq!(resolved, "agents365-ai-365-skills-assetseeker");

        let original = db::get_skill_by_id(&pool, "assetseeker")
            .await
            .expect("query original")
            .expect("original exists");
        assert_eq!(original.description.as_deref(), Some("existing"));
    }

    #[tokio::test]
    async fn stale_missing_canonical_record_does_not_mark_root_repo_conflict() {
        let pool = setup_test_db().await;
        let resource_root = tempdir().expect("resource root");
        db::set_skill_resource_library_dir(&pool, &resource_root.path().to_string_lossy())
            .await
            .expect("set resource root");

        let stale_dir = resource_root.path().join("missing-twitterapi-io");
        db::upsert_skill(
            &pool,
            &Skill {
                id: "twitterapi-io".to_string(),
                name: "Stale twitterapi-io".to_string(),
                description: Some("stale".to_string()),
                file_path: stale_dir.join("SKILL.md").to_string_lossy().into_owned(),
                canonical_path: Some(stale_dir.to_string_lossy().into_owned()),
                is_central: true,
                source: Some("local".to_string()),
                content: None,
                scanned_at: Utc::now().to_rfc3339(),
            },
        )
        .await
        .expect("seed stale record");

        let repo = GitHubRepoRef {
            owner: "dorukardahan".to_string(),
            repo: "twitterapi-io-skill".to_string(),
            branch: "main".to_string(),
            normalized_url: "https://github.com/dorukardahan/twitterapi-io-skill".to_string(),
            stars: None,
        };
        let candidates = build_repo_skill_candidates_from_snapshot(&repo, &root_repo_snapshot())
            .expect("candidates");
        let preview = build_preview_skills(&pool, &repo, &candidates)
            .await
            .expect("preview");
        let root_skill = preview
            .iter()
            .find(|skill| skill.skill_id == "twitterapi-io")
            .expect("root skill");
        assert_eq!(root_skill.source_path, "SKILL.md");
        assert!(root_skill.conflict.is_none());
        let occupied = current_managed_skill_ids(&pool).await.expect("occupied");
        assert!(!occupied.contains("twitterapi-io"));
    }

    #[tokio::test]
    async fn cancelled_import_stops_before_network_or_library_writes() {
        let pool = setup_test_db().await;
        let before: i64 = sqlx::query("SELECT COUNT(*) AS count FROM skills").fetch_one(&pool).await.unwrap().get("count");
        let operation = super::super::github_cancellation::Operation::start("cancelled-import-test".into());
        super::super::github_cancellation::cancel_github_operation("cancelled-import-test".into());
        let result = import_github_repo_skills_cancellable(&pool, "invalid-url", Vec::new(), None, Some(&operation)).await;
        assert_eq!(result.unwrap_err(), "Operation cancelled");
        let after: i64 = sqlx::query("SELECT COUNT(*) AS count FROM skills").fetch_one(&pool).await.unwrap().get("count");
        assert_eq!(before, after);
    }

    #[tokio::test]
    async fn import_invalid_repo_leaves_central_storage_unchanged() {
        let pool = setup_test_db().await;
        let central_root = tempdir().expect("central");
        sqlx::query("UPDATE agents SET global_skills_dir = ? WHERE id = 'central'")
            .bind(central_root.path().to_string_lossy().into_owned())
            .execute(&pool)
            .await
            .expect("update central");

        let result = import_github_repo_skills_impl(
            &pool,
            "https://github.com/example/definitely-missing-repo",
            vec![GitHubSkillImportSelection {
                source_path: "skills/foo/SKILL.md".to_string(),
                resolution: DuplicateResolution::Skip,
                renamed_skill_id: None,
            }],
            None,
        )
        .await;

        assert!(result.is_err());
        assert_eq!(
            std::fs::read_dir(central_root.path())
                .expect("read central")
                .count(),
            0
        );
        let central_skills = db::get_central_skills(&pool).await.expect("central skills");
        assert!(central_skills.is_empty());
    }

    #[tokio::test]
    async fn denied_import_selection_performs_no_writes_or_db_mutations() {
        let pool = setup_test_db().await;
        let central_root = tempdir().expect("central");
        sqlx::query("UPDATE agents SET global_skills_dir = ? WHERE id = 'central'")
            .bind(central_root.path().to_string_lossy().into_owned())
            .execute(&pool)
            .await
            .expect("update central");

        let before_skills = db::get_central_skills(&pool).await.expect("before skills");
        let before_entries = std::fs::read_dir(central_root.path())
            .expect("read central before")
            .count();

        let result = import_github_repo_skills_impl(
            &pool,
            "https://github.com/example/restricted-repo",
            vec![GitHubSkillImportSelection {
                source_path: "skills/private-skill/SKILL.md".to_string(),
                resolution: DuplicateResolution::Overwrite,
                renamed_skill_id: None,
            }],
            None,
        )
        .await;

        let error = result.expect_err("denied import should fail");
        assert!(
            !error.trim().is_empty(),
            "failure should return an error message"
        );

        let after_skills = db::get_central_skills(&pool).await.expect("after skills");
        let after_entries = std::fs::read_dir(central_root.path())
            .expect("read central after")
            .count();
        assert_eq!(
            before_entries, after_entries,
            "denied import should not write files"
        );
        assert_eq!(
            before_skills.len(),
            after_skills.len(),
            "denied import should not mutate DB"
        );
    }

    #[tokio::test]
    async fn preview_skill_package_directory_discovers_candidate_and_resources() {
        let pool = setup_test_db().await;
        let repo = GitHubRepoRef {
            owner: "gnipbao".into(),
            repo: "story-to-handdrawn-video".into(),
            branch: "main".into(),
            normalized_url: "https://github.com/gnipbao/story-to-handdrawn-video".into(),
            stars: None,
        };
        let snapshot = repo_snapshot(&[
            ("skill-package/story-to-handdrawn-video/SKILL.md", sample_frontmatter("story-to-handdrawn-video", "Video skill")),
            ("skill-package/story-to-handdrawn-video/references/style.md", "# Style".into()),
            ("site/index.html", "renderer site".into()),
        ]);
        let candidates = build_repo_skill_candidates_from_snapshot(&repo, &snapshot).unwrap();
        assert_eq!(candidates.len(), 1, "packaged skill must appear in import preview");
        let candidate = &candidates[0];
        assert_eq!(candidate.source_path, "skill-package/story-to-handdrawn-video/SKILL.md");
        assert_eq!(candidate.skill_id, "story-to-handdrawn-video");
        assert_eq!(candidate.root_directory, "skill-package");
        let preview = build_preview_skills(&pool, &repo, &candidates).await.unwrap();
        assert_eq!(preview.len(), 1);
        let files = collect_snapshot_source_files(&snapshot, &candidate.source_path).unwrap();
        assert_eq!(files.len(), 2);
        assert!(files.iter().any(|file| file.relative_path == "references/style.md"));
        assert!(!files.iter().any(|file| file.repo_path.starts_with("site/")));
    }

    #[tokio::test]
    async fn preview_top_level_skills_directory_discovers_candidates() {
        let pool = setup_test_db().await;
        let repo = GitHubRepoRef {
            owner: "anthropics".to_string(),
            repo: "skills".to_string(),
            branch: "main".to_string(),
            normalized_url: "https://github.com/anthropics/skills".to_string(),
            stars: None,
        };
        let candidates = build_repo_skill_candidates_from_snapshot(&repo, &multi_skill_snapshot())
            .expect("candidates");
        let preview = GitHubRepoPreview {
            repo: repo.clone(),
            skills: build_preview_skills(&pool, &repo, &candidates)
                .await
                .expect("skills"),
        };

        assert!(preview
            .skills
            .iter()
            .any(|skill| skill.source_path.starts_with("skills/")));
    }

    #[tokio::test]
    async fn preview_namespaced_skills_directory_discovers_candidates() {
        let pool = setup_test_db().await;
        let repo = GitHubRepoRef {
            owner: "openai".to_string(),
            repo: "skills".to_string(),
            branch: "main".to_string(),
            normalized_url: "https://github.com/openai/skills".to_string(),
            stars: None,
        };

        let candidates =
            build_repo_skill_candidates_from_snapshot(&repo, &namespaced_skill_snapshot())
                .expect("candidates");

        assert_eq!(
            candidates.len(),
            2,
            "expected two namespaced skill candidates"
        );

        let curated = candidates
            .iter()
            .find(|candidate| candidate.source_path == "skills/.curated/openai-docs/SKILL.md")
            .expect("curated skill");
        assert_eq!(curated.root_directory, "skills/.curated");
        assert_eq!(curated.skill_directory_name, "openai-docs");
        assert_eq!(curated.skill_id, "openai-docs");

        let system = candidates
            .iter()
            .find(|candidate| candidate.source_path == "skills/.system/skill-creator/SKILL.md")
            .expect("system skill");
        assert_eq!(system.root_directory, "skills/.system");
        assert_eq!(system.skill_directory_name, "skill-creator");
        assert_eq!(system.skill_id, "skill-creator");

        let preview = GitHubRepoPreview {
            repo: repo.clone(),
            skills: build_preview_skills(&pool, &repo, &candidates)
                .await
                .expect("preview skills"),
        };

        assert!(preview
            .skills
            .iter()
            .any(|skill| skill.source_path == "skills/.curated/openai-docs/SKILL.md"));
        assert!(preview
            .skills
            .iter()
            .any(|skill| skill.source_path == "skills/.system/skill-creator/SKILL.md"));
    }

    #[tokio::test]
    async fn preview_plugin_skills_directory_discovers_all_candidates() {
        let pool = setup_test_db().await;
        let repo = GitHubRepoRef {
            owner: "Agents365-ai".to_string(),
            repo: "365-skills".to_string(),
            branch: "main".to_string(),
            normalized_url: "https://github.com/Agents365-ai/365-skills".to_string(),
            stars: None,
        };

        let candidates = build_repo_skill_candidates_from_snapshot(&repo, &plugin_skill_snapshot())
            .expect("candidates");

        assert_eq!(candidates.len(), 3);
        let native = candidates
            .iter()
            .find(|candidate| candidate.skill_id == "agent-native-design")
            .expect("agent native");
        assert_eq!(
            native.source_path,
            "plugins/agent-native-design/skills/agent-native-design/SKILL.md"
        );
        assert_eq!(
            native.source_manifest_path,
            "plugins/agent-native-design/skills/agent-native-design/SKILL.md"
        );
        assert_eq!(native.root_directory, "plugins/agent-native-design/skills");

        let pi_skills = candidates
            .iter()
            .filter(|candidate| candidate.source_path.starts_with("plugins/pi/skills/"))
            .count();
        assert_eq!(pi_skills, 2);

        let preview = GitHubRepoPreview {
            repo: repo.clone(),
            skills: build_preview_skills(&pool, &repo, &candidates)
                .await
                .expect("preview skills"),
        };

        assert_eq!(preview.skills.len(), 3);
        assert!(preview
            .skills
            .iter()
            .any(|skill| skill.source_path == "plugins/pi/skills/pi-cli-runtime/SKILL.md"));
    }

    #[tokio::test]
    async fn github_pat_setting_is_trimmed_and_empty_values_are_ignored() {
        let pool = setup_test_db().await;

        db::set_setting(&pool, GITHUB_PAT_SETTING_KEY, "  test-token  ")
            .await
            .expect("set token");
        assert_eq!(
            github_direct_auth_from_settings(&pool)
                .await
                .expect("read token"),
            Some("test-token".to_string())
        );

        db::set_setting(&pool, GITHUB_PAT_SETTING_KEY, "   ")
            .await
            .expect("clear token");
        assert_eq!(
            github_direct_auth_from_settings(&pool)
                .await
                .expect("read empty"),
            None
        );
    }

    #[tokio::test]
    async fn authenticated_api_fallback_does_not_forward_bearer_auth_to_mirror() {
        use std::io::{Read, Write};
        use std::net::TcpListener;
        use std::sync::{
            atomic::{AtomicUsize, Ordering},
            Arc, Mutex,
        };

        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let address = listener.local_addr().expect("addr");
        let requests = Arc::new(Mutex::new(Vec::<String>::new()));
        let accepted = Arc::new(AtomicUsize::new(0));
        let requests_clone = Arc::clone(&requests);
        let accepted_clone = Arc::clone(&accepted);

        let server = std::thread::spawn(move || {
            while accepted_clone.load(Ordering::SeqCst) < 2 {
                let (mut stream, _) = listener.accept().expect("accept");
                let mut buffer = [0_u8; 2048];
                let bytes_read = stream.read(&mut buffer).expect("read");
                let request_text = String::from_utf8_lossy(&buffer[..bytes_read]).to_string();
                requests_clone
                    .lock()
                    .expect("lock")
                    .push(request_text.clone());
                accepted_clone.fetch_add(1, Ordering::SeqCst);

                if request_text.contains("GET /direct") {
                    let response =
                        "HTTP/1.1 502 Bad Gateway\r\nContent-Length: 11\r\n\r\nbad gateway";
                    stream.write_all(response.as_bytes()).expect("write direct");
                } else {
                    let response = "HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nok";
                    stream.write_all(response.as_bytes()).expect("write mirror");
                }
            }
        });

        let client = github_client().expect("client");
        let direct_url = format!("http://{}/direct", address);
        let mirror_url = format!("http://{}/mirror", address);

        let response = send_github_request_with_fallback(
            &client,
            GitHubFetchSurface::Api,
            |endpoint| {
                if endpoint.label == "github" {
                    direct_url.clone()
                } else {
                    mirror_url.clone()
                }
            },
            "direct request failed",
            Some("direct-token"),
        )
        .await
        .expect("fallback response");
        assert!(response.status().is_success());

        server.join().expect("server join");
        let captured = requests.lock().expect("captured");
        let direct_request = captured
            .iter()
            .find(|request| request.contains("GET /direct"))
            .expect("captured direct request");
        let mirror_request = captured
            .iter()
            .find(|request| request.contains("GET /mirror"))
            .expect("captured mirror request");
        assert!(
            direct_request.contains("authorization: Bearer direct-token")
                || direct_request.contains("Authorization: Bearer direct-token"),
            "direct github request should include bearer auth"
        );
        assert!(
            !mirror_request.contains("authorization: Bearer direct-token")
                && !mirror_request.contains("Authorization: Bearer direct-token"),
            "mirror request should not include bearer auth"
        );
    }

    #[tokio::test]
    async fn authenticated_raw_fallback_does_not_forward_bearer_auth_to_mirror() {
        use std::io::{Read, Write};
        use std::net::TcpListener;
        use std::sync::{
            atomic::{AtomicUsize, Ordering},
            Arc, Mutex,
        };

        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let address = listener.local_addr().expect("addr");
        let requests = Arc::new(Mutex::new(Vec::<String>::new()));
        let accepted = Arc::new(AtomicUsize::new(0));
        let requests_clone = Arc::clone(&requests);
        let accepted_clone = Arc::clone(&accepted);

        let server = std::thread::spawn(move || {
            while accepted_clone.load(Ordering::SeqCst) < 2 {
                let (mut stream, _) = listener.accept().expect("accept");
                let mut buffer = [0_u8; 2048];
                let bytes_read = stream.read(&mut buffer).expect("read");
                let request_text = String::from_utf8_lossy(&buffer[..bytes_read]).to_string();
                requests_clone
                    .lock()
                    .expect("lock")
                    .push(request_text.clone());
                accepted_clone.fetch_add(1, Ordering::SeqCst);

                if request_text.contains("GET /raw-direct") {
                    let response = "HTTP/1.1 503 Service Unavailable\r\nContent-Length: 19\r\n\r\nservice unavailable";
                    stream.write_all(response.as_bytes()).expect("write direct");
                } else {
                    let response = "HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nok";
                    stream.write_all(response.as_bytes()).expect("write mirror");
                }
            }
        });

        let client = github_client().expect("client");
        let direct_url = format!("http://{}/raw-direct", address);
        let mirror_url = format!("http://{}/raw-mirror", address);

        let response = send_github_request_with_fallback(
            &client,
            GitHubFetchSurface::Raw,
            |endpoint| {
                if endpoint.label == "github" {
                    direct_url.clone()
                } else {
                    mirror_url.clone()
                }
            },
            "raw request failed",
            Some("direct-token"),
        )
        .await
        .expect("fallback response");
        assert!(response.status().is_success());

        server.join().expect("server join");
        let captured = requests.lock().expect("captured");
        let direct_request = captured
            .iter()
            .find(|request| request.contains("GET /raw-direct"))
            .expect("captured direct request");
        let mirror_request = captured
            .iter()
            .find(|request| request.contains("GET /raw-mirror"))
            .expect("captured mirror request");
        assert!(
            direct_request.contains("authorization: Bearer direct-token")
                || direct_request.contains("Authorization: Bearer direct-token"),
            "direct raw request should include bearer auth"
        );
        assert!(
            !mirror_request.contains("authorization: Bearer direct-token")
                && !mirror_request.contains("Authorization: Bearer direct-token"),
            "mirror raw request should not include bearer auth"
        );
    }

    #[tokio::test]
    async fn unauthenticated_rate_limit_retries_public_mirror_before_failing() {
        use std::io::{Read, Write};
        use std::net::TcpListener;
        use std::sync::{
            atomic::{AtomicUsize, Ordering},
            Arc, Mutex,
        };

        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let address = listener.local_addr().expect("addr");
        let requests = Arc::new(Mutex::new(Vec::<String>::new()));
        let accepted = Arc::new(AtomicUsize::new(0));
        let requests_clone = Arc::clone(&requests);
        let accepted_clone = Arc::clone(&accepted);

        let server = std::thread::spawn(move || {
            while accepted_clone.load(Ordering::SeqCst) < 2 {
                let (mut stream, _) = listener.accept().expect("accept");
                let mut buffer = [0_u8; 2048];
                let bytes_read = stream.read(&mut buffer).expect("read");
                let request_text = String::from_utf8_lossy(&buffer[..bytes_read]).to_string();
                let is_direct = request_text.contains("GET /direct");
                requests_clone.lock().expect("lock").push(request_text);
                accepted_clone.fetch_add(1, Ordering::SeqCst);

                if is_direct {
                    let response = concat!(
                        "HTTP/1.1 403 Forbidden\r\n",
                        "Content-Type: application/json\r\n",
                        "X-RateLimit-Remaining: 0\r\n",
                        "X-RateLimit-Reset: 1786576453\r\n",
                        "Content-Length: 48\r\n\r\n",
                        "{\"message\":\"API rate limit exceeded for 1.2.3.4\"}"
                    );
                    stream.write_all(response.as_bytes()).expect("write direct");
                } else {
                    let response = "HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nok";
                    stream.write_all(response.as_bytes()).expect("write mirror");
                }
            }
        });

        let client = github_client().expect("client");
        let direct_url = format!("http://{}/direct", address);
        let mirror_url = format!("http://{}/mirror", address);

        let response = send_github_request_with_fallback(
            &client,
            GitHubFetchSurface::Api,
            |endpoint| {
                if endpoint.label == "github" {
                    direct_url.clone()
                } else {
                    mirror_url.clone()
                }
            },
            "request failed",
            None,
        )
        .await
        .expect("mirror retry response");
        assert!(response.status().is_success());

        server.join().expect("server join");
        let captured = requests.lock().expect("captured");
        assert!(captured
            .iter()
            .any(|request| request.contains("GET /direct")));
        assert!(captured
            .iter()
            .any(|request| request.contains("GET /mirror")));
    }
    #[tokio::test]
    async fn update_target_preserves_renamed_identity_and_rejects_other_repositories() {
        let pool=setup_test_db().await;
        let root=tempdir().unwrap();
        let target=source_grouped_skill_dir(root.path(),Some("owner"),Some("owner/repo"),None,"renamed");
        std::fs::create_dir_all(&target).unwrap();
        db::upsert_skill(&pool,&Skill{id:"renamed-id".into(),name:"Renamed".into(),description:None,file_path:target.join("SKILL.md").to_string_lossy().into_owned(),canonical_path:Some(target.to_string_lossy().into_owned()),is_central:false,source:None,content:None,scanned_at:String::new()}).await.unwrap();
        db::upsert_skill_source(&pool,&db::SkillSource{skill_id:"renamed-id".into(),source_type:"github".into(),source_url:None,source_author:Some("owner".into()),source_repo:Some("owner/repo".into()),source_path:Some("skills/original/SKILL.md".into()),updated_at:String::new()}).await.unwrap();
        assert_eq!(resolve_update_target(&pool,"renamed-id","owner/repo",root.path(),"owner").await.unwrap(),("renamed-id".into(),"renamed".into()));
        assert!(resolve_update_target(&pool,"renamed-id","other/repo",root.path(),"other").await.is_err());
    }

    #[tokio::test]
    #[ignore = "Requires live access to a public GitHub repository"]
    async fn live_public_update_preview_downloads_snapshot() {
        let mut repo=resolve_repo_ref("https://github.com/1weiho/open-slide",None).await.expect("public metadata request failed");
        repo.branch=fetch_repo_head_ref(&repo,None).await.expect("public revision request failed");
        let result=fetch_repo_preview_files(&repo,None).await;
        match result {
            Ok((skills,_))=>assert!(!skills.is_empty()),
            Err(error)=>panic!("snapshot failed: HTTP403={} HTTP401={} rate_limit={}",error.contains("403"),error.contains("401"),error.contains("rate limit")),
        }
    }

}
