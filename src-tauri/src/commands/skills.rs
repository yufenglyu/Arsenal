use super::repository_docs;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::path::{Component, Path, PathBuf};
use std::time::SystemTime;
use tauri::State;

use crate::db::{self, Collection, DbPool, SkillForAgent};
use crate::path_utils::{remove_symlink_path, resolve_path_for_file_manager};
use crate::AppState;

use super::linker::{
    copy_dir_all, prune_mirrored_relative_path_from_install_targets,
    uninstall_skill_from_agent_impl,
};
use super::scanner::{detect_link_type, parse_skill_md, scan_skill_root, ScanDirectoryOptions};

// ─── Types ────────────────────────────────────────────────────────────────────

/// A Central Skill with a list of agent IDs that currently have this skill
/// installed (via symlink or copy).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillWithLinks {
    pub github_stars: Option<i64>,
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub file_path: String,
    pub canonical_path: Option<String>,
    pub is_central: bool,
    pub source: Option<String>,
    pub source_url: Option<String>,
    pub source_author: Option<String>,
    pub source_repo: Option<String>,
    pub source_path: Option<String>,
    pub notes: Option<String>,
    pub tags: Vec<String>,
    pub scanned_at: String,
    pub created_at: String,
    pub updated_at: String,
    /// Agent IDs that have an installation record for this skill.
    pub linked_agents: Vec<String>,
    /// All targets served by Shared Hub, independent of manual installation ownership.
    pub shared_agents: Vec<String>,
    /// Agent IDs that observe this skill from a shared/read-only compatibility root.
    pub read_only_agents: Vec<String>,
}

/// An installation record enriched with the `installed_at` timestamp for
/// the skill detail IPC response. This is the frontend-facing version of
/// `db::SkillInstallation` — `created_at` from the DB is exposed as
/// `installed_at` for clarity.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillInstallationDetail {
    pub skill_id: String,
    pub agent_id: String,
    pub installed_path: String,
    pub link_type: String,
    pub symlink_target: Option<String>,
    /// ISO 8601 timestamp of when the skill was first installed.
    pub installed_at: String,
}

/// A skill with full installation details across all platforms.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillDetail {
    pub id: String,
    pub row_id: String,
    pub name: String,
    pub description: Option<String>,
    pub file_path: String,
    pub dir_path: String,
    pub canonical_path: Option<String>,
    pub is_central: bool,
    pub source: Option<String>,
    pub source_url: Option<String>,
    pub source_author: Option<String>,
    pub source_repo: Option<String>,
    pub source_path: Option<String>,
    pub notes: Option<String>,
    pub tags: Vec<String>,
    pub scanned_at: String,
    pub created_at: String,
    pub updated_at: String,
    pub source_kind: Option<String>,
    pub source_root: Option<String>,
    pub is_read_only: bool,
    pub conflict_group: Option<String>,
    pub conflict_count: i64,
    /// Agent IDs that can see this central skill through a read-only compatibility root.
    pub read_only_agents: Vec<String>,
    /// All installation records for this skill across agents.
    pub installations: Vec<SkillInstallationDetail>,
    /// Collections this skill currently belongs to.
    pub collections: Vec<Collection>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SkillDirectoryNode {
    pub name: String,
    pub path: String,
    pub relative_path: String,
    pub is_dir: bool,
    pub children: Vec<SkillDirectoryNode>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeleteCentralSkillOptions {
    pub cascade_uninstall: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeleteCentralSkillResult {
    pub skill_id: String,
    pub removed_canonical_path: String,
    pub uninstalled_agents: Vec<String>,
    pub skipped_read_only_agents: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeleteResourceSkillOptions {
    pub cascade_uninstall: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeleteResourceSkillResult {
    pub skill_id: String,
    pub removed_canonical_path: String,
    pub uninstalled_agents: Vec<String>,
    pub skipped_read_only_agents: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CentralSkillBundle {
    pub name: String,
    pub relative_path: String,
    pub path: String,
    pub is_symlink: bool,
    pub skill_count: usize,
    pub linked_agent_count: usize,
    pub read_only_agent_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CentralSkillBundleDeletePreview {
    pub bundle: CentralSkillBundle,
    pub skills: Vec<SkillWithLinks>,
    pub affected_agents: Vec<String>,
    pub skipped_read_only_agents: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CentralSkillBundleDetail {
    pub bundle: CentralSkillBundle,
    pub skills: Vec<SkillWithLinks>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeleteCentralSkillBundleOptions {
    pub cascade_uninstall: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeleteCentralSkillBundleResult {
    pub relative_path: String,
    pub removed_bundle_path: String,
    pub removed_kind: String,
    pub removed_skill_ids: Vec<String>,
    pub uninstalled_agents: Vec<String>,
    pub skipped_read_only_agents: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateSkillMetadataRequest {
    pub notes: Option<String>,
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateSkillSourceMetadataRequest {
    pub source_type: String,
    pub source_url: Option<String>,
    pub source_author: Option<String>,
    pub source_repo: Option<String>,
    pub source_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateManualResourceSkillRequest {
    pub skill_id: String,
    pub name: String,
    pub description: Option<String>,
    pub body: Option<String>,
    pub source_url: Option<String>,
    pub source_author: Option<String>,
    pub source_repo: Option<String>,
    pub source_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AddLocalResourceSkillsRequest {
    pub selected_skill_ids: Option<Vec<String>>,
    pub source_dir: String,
    pub overwrite: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LocalResourceImportKind {
    SingleSkill,
    Collection,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AddLocalResourceSkillsResult {
    pub source_dir: String,
    pub import_kind: LocalResourceImportKind,
    pub collection_name: Option<String>,
    pub added_skills: Vec<SkillWithLinks>,
    pub skipped_skills: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillMetadataResponse {
    pub skill_id: String,
    pub notes: Option<String>,
    pub tags: Vec<String>,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillSourceMetadataResponse {
    pub skill_id: String,
    pub source: Option<String>,
    pub source_type: String,
    pub source_url: Option<String>,
    pub source_author: Option<String>,
    pub source_repo: Option<String>,
    pub source_path: Option<String>,
    pub updated_at: String,
}

// ─── Tauri Commands ───────────────────────────────────────────────────────────

fn system_time_to_rfc3339(time: SystemTime) -> String {
    let datetime: DateTime<Utc> = time.into();
    datetime.to_rfc3339()
}

fn skill_filesystem_timestamps(skill: &db::Skill) -> (String, String) {
    let directory_metadata = skill
        .canonical_path
        .as_deref()
        .and_then(|path| std::fs::metadata(path).ok());
    let file_metadata = std::fs::metadata(&skill.file_path).ok();

    let created_at = directory_metadata
        .as_ref()
        .or(file_metadata.as_ref())
        .and_then(|metadata| metadata.created().ok())
        .map(system_time_to_rfc3339)
        .unwrap_or_else(|| skill.scanned_at.clone());

    let updated_at = file_metadata
        .as_ref()
        .or(directory_metadata.as_ref())
        .and_then(|metadata| metadata.modified().ok())
        .map(system_time_to_rfc3339)
        .unwrap_or_else(|| skill.scanned_at.clone());

    (created_at, updated_at)
}

async fn skill_updated_at_after_check(
    pool: &DbPool,
    skill_id: &str,
    filesystem_time: String,
) -> Result<String, String> {
    let checked_at = db::get_skill_source_sync(pool, skill_id)
        .await?
        .and_then(|sync| sync.last_checked_at);
    Ok(checked_at
        .filter(|time| {
            match (
                chrono::DateTime::parse_from_rfc3339(time),
                chrono::DateTime::parse_from_rfc3339(&filesystem_time),
            ) {
                (Ok(checked), Ok(modified)) => checked > modified,
                (Ok(_), Err(_)) => true,
                _ => false,
            }
        })
        .unwrap_or(filesystem_time))
}

fn normalize_skill_tags(tags: Vec<String>) -> Vec<String> {
    let mut seen = BTreeSet::new();
    tags.into_iter()
        .map(|tag| tag.trim().trim_start_matches('#').to_string())
        .filter(|tag| !tag.is_empty())
        .filter(|tag| seen.insert(tag.to_lowercase()))
        .take(30)
        .collect()
}

fn metadata_response(metadata: db::SkillMetadata) -> SkillMetadataResponse {
    let tags = db::parse_skill_metadata_tags(Some(&metadata));
    SkillMetadataResponse {
        skill_id: metadata.skill_id,
        notes: metadata.notes,
        tags,
        updated_at: metadata.updated_at,
    }
}

fn normalized_optional_text(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn normalize_source_type(value: &str) -> Result<String, String> {
    let source_type = value.trim().to_lowercase();
    if source_type.is_empty() {
        return Ok("manual".to_string());
    }
    if source_type.len() > 40
        || !source_type
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '-' || ch == '_')
    {
        return Err("Invalid source type".to_string());
    }
    Ok(source_type)
}

fn normalize_source_url(value: Option<String>) -> Result<Option<String>, String> {
    let Some(url) = normalized_optional_text(value) else {
        return Ok(None);
    };
    let parsed = reqwest::Url::parse(&url).map_err(|_| "Invalid source URL".to_string())?;
    if !matches!(parsed.scheme(), "http" | "https") {
        return Err("Source URL must use http or https".to_string());
    }
    if !parsed.username().is_empty() || parsed.password().is_some() {
        return Err("Source URL must not include credentials".to_string());
    }
    Ok(Some(url))
}

fn normalize_source_repo(value: Option<String>) -> Result<Option<String>, String> {
    let Some(repo) = normalized_optional_text(value) else {
        return Ok(None);
    };
    let parts: Vec<&str> = repo.split('/').collect();
    if parts.len() != 2
        || parts.iter().any(|part| part.is_empty())
        || !parts.iter().all(|part| {
            part.chars()
                .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.'))
        })
    {
        return Err("GitHub repository must use owner/repo format".to_string());
    }
    Ok(Some(repo))
}

fn source_label_from_parts(source_type: &str, source_repo: Option<&str>) -> String {
    match (source_type, source_repo) {
        ("github", Some(repo)) => format!("github:{repo}"),
        (_, Some(repo)) => repo.to_string(),
        ("manual", None) => "manual".to_string(),
        (other, None) => other.to_string(),
    }
}

fn source_label_from_metadata(source: &db::SkillSource) -> Option<String> {
    source
        .source_repo
        .as_deref()
        .map(|repo| source_label_from_parts(&source.source_type, Some(repo)))
}

fn is_github_repository_homepage_url(url: &str, repo: &str) -> bool {
    let Ok(parsed) = reqwest::Url::parse(url) else {
        return false;
    };
    parsed.scheme() == "https"
        && parsed.host_str() == Some("github.com")
        && parsed
            .path()
            .trim_matches('/')
            .eq_ignore_ascii_case(repo.trim_matches('/'))
}

fn scrub_non_updatable_inferred_source_url(source: &mut db::SkillSource) {
    if source.source_type != "github" {
        return;
    }
    let Some(url) = source.source_url.as_deref() else {
        return;
    };
    let Some(repo) = source.source_repo.as_deref() else {
        return;
    };
    if is_github_repository_homepage_url(url, repo) {
        source.source_url = None;
        source.updated_at = Utc::now().to_rfc3339();
    }
}

fn infer_resource_github_source(
    resource_root: &Path,
    skill_dir: &Path,
    skill_id: &str,
) -> Option<db::SkillSource> {
    let relative = skill_dir.strip_prefix(resource_root).ok()?;
    let parts: Vec<String> = relative
        .components()
        .filter_map(|component| match component {
            Component::Normal(value) => Some(value.to_string_lossy().to_string()),
            _ => None,
        })
        .collect();
    if parts.len() < 3 {
        return None;
    }

    let owner = parts[0].trim();
    let repo = parts[1].trim();
    if owner.is_empty() || repo.is_empty() {
        return None;
    }

    let relative_skill_path = parts[2..].join("/");
    let root_skill_dir = repo.strip_suffix("-skill").unwrap_or(repo);
    let source_path = if parts.len() == 3 && (parts[2] == repo || parts[2] == root_skill_dir) {
        "SKILL.md".to_string()
    } else {
        format!("{relative_skill_path}/SKILL.md")
    };

    Some(db::SkillSource {
        skill_id: skill_id.to_string(),
        source_type: "github".to_string(),
        source_url: None,
        source_author: Some(owner.to_string()),
        source_repo: Some(format!("{owner}/{repo}")),
        source_path: Some(source_path),
        updated_at: Utc::now().to_rfc3339(),
    })
}

fn source_response(
    source: db::SkillSource,
    skill_source: Option<String>,
) -> SkillSourceMetadataResponse {
    SkillSourceMetadataResponse {
        skill_id: source.skill_id,
        source: skill_source,
        source_type: source.source_type,
        source_url: source.source_url,
        source_author: source.source_author,
        source_repo: source.source_repo,
        source_path: source.source_path,
        updated_at: source.updated_at,
    }
}

fn skill_dir_path(skill: &db::Skill) -> String {
    skill
        .canonical_path
        .clone()
        .or_else(|| {
            Path::new(&skill.file_path)
                .parent()
                .map(|path| path.to_string_lossy().into_owned())
        })
        .unwrap_or_else(|| skill.file_path.clone())
}

fn symlink_target_dir(path: &Path) -> Option<PathBuf> {
    if !std::fs::symlink_metadata(path)
        .ok()?
        .file_type()
        .is_symlink()
    {
        return None;
    }

    // Resolve the full installation -> shared hub -> repository chain.
    // Keep the installation entry separate so the UI can show both locations.
    if let Ok(resolved) = std::fs::canonicalize(path) {
        return Some(PathBuf::from(crate::path_utils::path_for_file_manager(&resolved)));
    }

    let target = std::fs::read_link(path).ok()?;
    let absolute_target = if target.is_absolute() {
        target
    } else {
        path.parent().unwrap_or_else(|| Path::new("")).join(target)
    };
    Some(absolute_target)
}

fn skill_detail_file_and_dir_paths(
    skill: &db::Skill,
    installation: Option<&db::SkillInstallation>,
) -> (String, String, Option<String>) {
    if let Some(installation) =
        installation.filter(|installation| installation.link_type == "symlink")
    {
        let install_dir = PathBuf::from(&installation.installed_path);
        if let Some(target_dir) = symlink_target_dir(&install_dir) {
            return (
                target_dir.join("SKILL.md").to_string_lossy().into_owned(),
                target_dir.to_string_lossy().into_owned(),
                Some(installation.installed_path.clone()),
            );
        }
    }

    let link_dir = skill
        .canonical_path
        .as_deref()
        .map(PathBuf::from)
        .or_else(|| Path::new(&skill.file_path).parent().map(Path::to_path_buf));

    if skill.is_central {
        if let Some(target_dir) = link_dir.as_deref().and_then(symlink_target_dir) {
            return (
                target_dir.join("SKILL.md").to_string_lossy().into_owned(),
                target_dir.to_string_lossy().into_owned(),
                skill.canonical_path.clone(),
            );
        }
    }

    (
        skill.file_path.clone(),
        skill_dir_path(skill),
        skill.canonical_path.clone(),
    )
}

fn canonical_delete_dir(skill: &db::Skill, central_root: &Path) -> PathBuf {
    skill
        .canonical_path
        .as_deref()
        .map(PathBuf::from)
        .or_else(|| Path::new(&skill.file_path).parent().map(Path::to_path_buf))
        .unwrap_or_else(|| central_root.join(&skill.id))
}

fn resource_delete_dir(skill: &db::Skill) -> PathBuf {
    skill
        .canonical_path
        .as_deref()
        .map(PathBuf::from)
        .or_else(|| Path::new(&skill.file_path).parent().map(Path::to_path_buf))
        .unwrap_or_else(|| PathBuf::from(&skill.file_path))
}

fn ensure_under_central_root(path: &Path, central_root: &Path) -> Result<(), String> {
    if path.starts_with(central_root) && path != central_root {
        Ok(())
    } else {
        Err("Canonical path is outside Central Skills root".to_string())
    }
}

fn ensure_under_resource_root(path: &Path, resource_root: &Path) -> Result<(), String> {
    if path.starts_with(resource_root) && path != resource_root {
        Ok(())
    } else {
        Err("Canonical path is outside Skill Resource Library".to_string())
    }
}

fn validate_central_delete_target(
    canonical_dir: &Path,
    central_root: &Path,
) -> Result<PathBuf, String> {
    let metadata = std::fs::symlink_metadata(canonical_dir).map_err(|e| {
        format!(
            "Failed to read canonical path '{}': {}",
            canonical_dir.display(),
            e
        )
    })?;

    if metadata.file_type().is_symlink() {
        let parent = canonical_dir
            .parent()
            .ok_or_else(|| "Canonical path has no parent directory".to_string())?
            .canonicalize()
            .map_err(|e| {
                format!(
                    "Failed to resolve canonical path parent '{}': {}",
                    canonical_dir.display(),
                    e
                )
            })?;
        let file_name = canonical_dir
            .file_name()
            .ok_or_else(|| "Canonical path has no directory name".to_string())?;
        ensure_under_central_root(&parent.join(file_name), central_root)?;
        return Ok(canonical_dir.to_path_buf());
    }

    if !metadata.is_dir() {
        return Err(format!(
            "Canonical path '{}' is not a skill directory",
            canonical_dir.display()
        ));
    }

    let resolved = canonical_dir.canonicalize().map_err(|e| {
        format!(
            "Failed to resolve canonical path '{}': {}",
            canonical_dir.display(),
            e
        )
    })?;
    ensure_under_central_root(&resolved, central_root)?;

    if !resolved.join("SKILL.md").exists() {
        return Err(format!(
            "Canonical skill directory '{}' does not contain SKILL.md",
            resolved.display()
        ));
    }

    Ok(resolved)
}

fn validate_resource_delete_target(
    canonical_dir: &Path,
    resource_root: &Path,
) -> Result<PathBuf, String> {
    let metadata = std::fs::symlink_metadata(canonical_dir).map_err(|e| {
        format!(
            "Failed to read resource skill path '{}': {}",
            canonical_dir.display(),
            e
        )
    })?;

    if metadata.file_type().is_symlink() {
        let parent = canonical_dir
            .parent()
            .ok_or_else(|| "Resource skill path has no parent directory".to_string())?
            .canonicalize()
            .map_err(|e| {
                format!(
                    "Failed to resolve resource skill parent '{}': {}",
                    canonical_dir.display(),
                    e
                )
            })?;
        let file_name = canonical_dir
            .file_name()
            .ok_or_else(|| "Resource skill path has no directory name".to_string())?;
        ensure_under_resource_root(&parent.join(file_name), resource_root).map_err(|_| {
            format!(
                "Refusing to delete resource skill outside Skill Resource Library: {}",
                canonical_dir.display()
            )
        })?;
        return Ok(canonical_dir.to_path_buf());
    }

    if !metadata.is_dir() {
        return Err(format!(
            "Resource skill path '{}' is not a skill directory",
            canonical_dir.display()
        ));
    }

    let resolved = canonical_dir.canonicalize().map_err(|e| {
        format!(
            "Failed to resolve resource skill path '{}': {}",
            canonical_dir.display(),
            e
        )
    })?;
    ensure_under_resource_root(&resolved, resource_root).map_err(|_| {
        format!(
            "Refusing to delete resource skill outside Skill Resource Library: {}",
            resolved.display()
        )
    })?;

    if !resolved.join("SKILL.md").exists() {
        return Err(format!(
            "Resource skill directory '{}' does not contain SKILL.md",
            resolved.display()
        ));
    }

    Ok(resolved)
}

fn remove_central_skill_dir(target: &Path) -> Result<(), String> {
    let metadata = std::fs::symlink_metadata(target).map_err(|e| {
        format!(
            "Failed to read canonical path '{}': {}",
            target.display(),
            e
        )
    })?;

    if metadata.file_type().is_symlink() {
        remove_symlink_path(target)
            .map_err(|e| format!("Failed to remove central skill symlink: {}", e))
    } else if metadata.is_dir() {
        std::fs::remove_dir_all(target)
            .map_err(|e| format!("Failed to remove central skill directory: {}", e))
    } else {
        Err(format!(
            "Canonical path '{}' is not a removable skill directory",
            target.display()
        ))
    }
}

fn remove_resource_skill_dir(target: &Path) -> Result<(), String> {
    let metadata = std::fs::symlink_metadata(target).map_err(|e| {
        format!(
            "Failed to read resource skill path '{}': {}",
            target.display(),
            e
        )
    })?;

    if metadata.file_type().is_symlink() {
        remove_symlink_path(target)
            .map_err(|e| format!("Failed to remove resource skill symlink: {}", e))
    } else if metadata.is_dir() {
        std::fs::remove_dir_all(target)
            .map_err(|e| format!("Failed to remove resource skill directory: {}", e))
    } else {
        Err(format!(
            "Resource skill path '{}' is not a removable skill directory",
            target.display()
        ))
    }
}

#[derive(Debug, Clone)]
struct CentralBundleTarget {
    relative_path: String,
    entry_path: PathBuf,
    delete_path: PathBuf,
    content_root: Option<PathBuf>,
    is_symlink: bool,
}

impl CentralBundleTarget {
    fn removed_kind(&self) -> &'static str {
        if self.is_symlink {
            "symlink"
        } else {
            "directory"
        }
    }
}

#[derive(Debug, Clone)]
struct ResourceBundleTarget {
    relative_path: String,
    entry_path: PathBuf,
    delete_path: PathBuf,
    content_root: Option<PathBuf>,
    is_symlink: bool,
}

impl ResourceBundleTarget {
    fn removed_kind(&self) -> &'static str {
        if self.is_symlink {
            "symlink"
        } else {
            "directory"
        }
    }
}

fn normalize_central_bundle_relative_path(relative_path: &str) -> Result<PathBuf, String> {
    let trimmed = relative_path.trim();
    if trimmed.is_empty() {
        return Err("Invalid Central bundle path".to_string());
    }

    let input = Path::new(trimmed);
    if input.is_absolute() {
        return Err("Invalid Central bundle path".to_string());
    }

    let mut normalized = PathBuf::new();
    for component in input.components() {
        match component {
            std::path::Component::Normal(part) => normalized.push(part),
            _ => return Err("Invalid Central bundle path".to_string()),
        }
    }

    if normalized.as_os_str().is_empty() {
        return Err("Invalid Central bundle path".to_string());
    }

    Ok(normalized)
}

fn normalize_resource_bundle_relative_path(relative_path: &str) -> Result<PathBuf, String> {
    normalize_central_bundle_relative_path(relative_path)
        .map_err(|_| "Invalid Resource Library bundle path".to_string())
}

async fn central_root_path(pool: &DbPool) -> Result<PathBuf, String> {
    let central = db::get_agent_by_id(pool, "central")
        .await?
        .ok_or_else(|| "Central agent not found in database".to_string())?;

    PathBuf::from(&central.global_skills_dir)
        .canonicalize()
        .map_err(|e| format!("Failed to resolve Central Skills root: {}", e))
}

async fn resource_root_path(pool: &DbPool) -> Result<PathBuf, String> {
    let root = db::get_skill_resource_library_dir(pool).await?;
    std::fs::create_dir_all(&root)
        .map_err(|e| format!("Failed to create Skill Resource Library root: {}", e))?;
    root.canonicalize()
        .map_err(|e| format!("Failed to resolve Skill Resource Library root: {}", e))
}

fn validate_central_bundle_target(
    relative_path: &str,
    central_root: &Path,
) -> Result<CentralBundleTarget, String> {
    let normalized = normalize_central_bundle_relative_path(relative_path)?;
    let entry_path = central_root.join(&normalized);
    let metadata = std::fs::symlink_metadata(&entry_path).map_err(|e| {
        format!(
            "Failed to read Central bundle path '{}': {}",
            entry_path.display(),
            e
        )
    })?;

    let relative_path = normalized.to_string_lossy().into_owned();

    if metadata.file_type().is_symlink() {
        let parent = entry_path
            .parent()
            .ok_or_else(|| "Central bundle path has no parent directory".to_string())?
            .canonicalize()
            .map_err(|e| {
                format!(
                    "Failed to resolve Central bundle parent '{}': {}",
                    entry_path.display(),
                    e
                )
            })?;
        let file_name = entry_path
            .file_name()
            .ok_or_else(|| "Central bundle path has no directory name".to_string())?;
        ensure_under_central_root(&parent.join(file_name), central_root)?;

        return Ok(CentralBundleTarget {
            relative_path,
            entry_path: entry_path.clone(),
            delete_path: entry_path,
            content_root: std::fs::canonicalize(central_root.join(&normalized)).ok(),
            is_symlink: true,
        });
    }

    if !metadata.is_dir() {
        return Err(format!(
            "Central bundle path '{}' is not a directory or symlink",
            entry_path.display()
        ));
    }

    let resolved = entry_path.canonicalize().map_err(|e| {
        format!(
            "Failed to resolve Central bundle path '{}': {}",
            entry_path.display(),
            e
        )
    })?;
    ensure_under_central_root(&resolved, central_root)?;

    Ok(CentralBundleTarget {
        relative_path,
        entry_path,
        delete_path: resolved.clone(),
        content_root: Some(resolved),
        is_symlink: false,
    })
}

fn validate_resource_bundle_target(
    relative_path: &str,
    resource_root: &Path,
) -> Result<ResourceBundleTarget, String> {
    let normalized = normalize_resource_bundle_relative_path(relative_path)?;
    let entry_path = resource_root.join(&normalized);
    let metadata = std::fs::symlink_metadata(&entry_path).map_err(|e| {
        format!(
            "Failed to read Resource Library bundle path '{}': {}",
            entry_path.display(),
            e
        )
    })?;

    let relative_path = portable_relative_path(&normalized);
    if relative_path.is_empty() {
        return Err("Invalid Resource Library bundle path".to_string());
    }

    if metadata.file_type().is_symlink() {
        let parent = entry_path
            .parent()
            .ok_or_else(|| "Resource Library bundle path has no parent directory".to_string())?
            .canonicalize()
            .map_err(|e| {
                format!(
                    "Failed to resolve Resource Library bundle parent '{}': {}",
                    entry_path.display(),
                    e
                )
            })?;
        let file_name = entry_path
            .file_name()
            .ok_or_else(|| "Resource Library bundle path has no directory name".to_string())?;
        ensure_under_resource_root(&parent.join(file_name), resource_root)?;

        return Ok(ResourceBundleTarget {
            relative_path,
            entry_path: entry_path.clone(),
            delete_path: entry_path,
            content_root: std::fs::canonicalize(resource_root.join(&normalized)).ok(),
            is_symlink: true,
        });
    }

    if !metadata.is_dir() {
        return Err(format!(
            "Resource Library bundle path '{}' is not a directory or symlink",
            entry_path.display()
        ));
    }

    let resolved = entry_path.canonicalize().map_err(|e| {
        format!(
            "Failed to resolve Resource Library bundle path '{}': {}",
            entry_path.display(),
            e
        )
    })?;
    ensure_under_resource_root(&resolved, resource_root)?;
    if resolved == resource_root {
        return Err("Invalid Resource Library bundle path".to_string());
    }

    Ok(ResourceBundleTarget {
        relative_path,
        entry_path,
        delete_path: resolved.clone(),
        content_root: Some(resolved),
        is_symlink: false,
    })
}

fn skill_directory_path_buf(skill: &db::Skill) -> PathBuf {
    skill
        .canonical_path
        .as_deref()
        .map(PathBuf::from)
        .or_else(|| Path::new(&skill.file_path).parent().map(Path::to_path_buf))
        .unwrap_or_else(|| PathBuf::from(&skill.file_path))
}

fn portable_relative_path(path: &Path) -> String {
    path.components()
        .filter_map(|component| match component {
            Component::Normal(part) => Some(part.to_string_lossy().into_owned()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("/")
}

fn path_without_verbatim_prefix(path: &Path) -> PathBuf {
    #[cfg(windows)]
    {
        let display = path.to_string_lossy();
        if let Some(rest) = display.strip_prefix(r"\\?\UNC\") {
            return PathBuf::from(format!(r"\\{rest}"));
        }
        if let Some(rest) = display.strip_prefix(r"\\?\") {
            return PathBuf::from(rest);
        }
    }
    path.to_path_buf()
}

/// Resolve a path without following the final symlink, so a Central Skills
/// link is still compared as living under `~/.agents/skills/...`.
fn resolved_bundle_entry(path: &Path) -> PathBuf {
    let normalized = path_without_verbatim_prefix(path);
    match normalized.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => {
            let parent = std::fs::canonicalize(parent)
                .map(|resolved| path_without_verbatim_prefix(&resolved))
                .unwrap_or_else(|_| parent.to_path_buf());
            parent.join(normalized.file_name().unwrap_or_default())
        }
        _ => normalized,
    }
}

fn path_is_strict_descendant(path: &Path, ancestor: &Path) -> bool {
    let path = path_without_verbatim_prefix(path);
    let ancestor = path_without_verbatim_prefix(ancestor);
    path != ancestor && path.starts_with(&ancestor)
}

fn skill_dir_is_under_paths(skill_dir: &Path, ancestors: &[&Path]) -> bool {
    let resolved_skill = resolved_bundle_entry(skill_dir);
    if ancestors.iter().any(|ancestor| {
        path_is_strict_descendant(&resolved_skill, &resolved_bundle_entry(ancestor))
    }) {
        return true;
    }

    let Ok(followed_skill) = std::fs::canonicalize(skill_dir) else {
        return false;
    };
    ancestors.iter().any(|ancestor| {
        std::fs::canonicalize(ancestor)
            .ok()
            .is_some_and(|followed_ancestor| {
                path_is_strict_descendant(&followed_skill, &followed_ancestor)
            })
    })
}

fn skill_is_under_bundle(skill: &db::Skill, target: &CentralBundleTarget) -> bool {
    let mut ancestors = vec![target.entry_path.as_path(), target.delete_path.as_path()];
    if let Some(content_root) = target.content_root.as_ref() {
        ancestors.push(content_root);
    }
    skill_dir_is_under_paths(&skill_directory_path_buf(skill), &ancestors)
}

fn skill_is_under_resource_bundle(skill: &db::Skill, target: &ResourceBundleTarget) -> bool {
    let mut ancestors = vec![target.entry_path.as_path(), target.delete_path.as_path()];
    if let Some(content_root) = target.content_root.as_ref() {
        ancestors.push(content_root);
    }
    skill_dir_is_under_paths(&skill_directory_path_buf(skill), &ancestors)
}

fn resource_target_for_central_skill(
    skill: &db::Skill,
    central_root: &Path,
    resource_root: &Path,
) -> Option<PathBuf> {
    let central_dir = skill_directory_path_buf(skill);
    let resource_root = std::fs::canonicalize(resource_root)
        .map(|path| path_without_verbatim_prefix(&path))
        .unwrap_or_else(|_| path_without_verbatim_prefix(resource_root));

    if let Ok(metadata) = std::fs::symlink_metadata(&central_dir) {
        if metadata.file_type().is_symlink() {
            if let Ok(target) = std::fs::canonicalize(&central_dir) {
                let target = path_without_verbatim_prefix(&target);
                if path_is_strict_descendant(&target, &resource_root)
                    && target.join("SKILL.md").is_file()
                {
                    return Some(target);
                }
            }
        }
    }

    let relative = path_without_verbatim_prefix(&resolved_bundle_entry(&central_dir))
        .strip_prefix(path_without_verbatim_prefix(central_root))
        .ok()?
        .to_path_buf();
    if relative.as_os_str().is_empty() {
        return None;
    }
    let resource_dir = resource_root.join(relative);
    resource_dir
        .join("SKILL.md")
        .is_file()
        .then_some(resource_dir)
}

async fn central_skills_in_bundle(
    pool: &DbPool,
    target: &CentralBundleTarget,
) -> Result<Vec<db::Skill>, String> {
    let mut skills = db::get_central_skills(pool)
        .await?
        .into_iter()
        .filter(|skill| skill_is_under_bundle(skill, target))
        .collect::<Vec<_>>();
    let mut seen: HashSet<String> = skills.iter().map(|skill| skill.id.clone()).collect();
    for scanned in scan_skill_root(&target.entry_path, true, ScanDirectoryOptions::nested()) {
        if !seen.insert(scanned.id.clone()) {
            continue;
        }
        if let Some(skill) = db::get_skill_by_id(pool, &scanned.id).await? {
            skills.push(skill);
        }
    }
    skills.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(skills)
}

async fn resource_skills_in_bundle(
    pool: &DbPool,
    target: &ResourceBundleTarget,
) -> Result<Vec<db::Skill>, String> {
    let mut skills = db::get_resource_library_skills(pool)
        .await?
        .into_iter()
        .filter(|skill| skill_is_under_resource_bundle(skill, target))
        .collect::<Vec<_>>();
    skills.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(skills)
}

fn remove_central_bundle_target(target: &CentralBundleTarget) -> Result<(), String> {
    let metadata = std::fs::symlink_metadata(&target.delete_path).map_err(|e| {
        format!(
            "Failed to read Central bundle path '{}': {}",
            target.delete_path.display(),
            e
        )
    })?;

    if metadata.file_type().is_symlink() {
        remove_symlink_path(&target.delete_path)
            .map_err(|e| format!("Failed to remove Central bundle symlink: {}", e))
    } else if metadata.is_dir() {
        std::fs::remove_dir_all(&target.delete_path)
            .map_err(|e| format!("Failed to remove Central bundle directory: {}", e))
    } else {
        Err(format!(
            "Central bundle path '{}' is not removable",
            target.delete_path.display()
        ))
    }
}

fn remove_resource_bundle_target(target: &ResourceBundleTarget) -> Result<(), String> {
    let metadata = std::fs::symlink_metadata(&target.delete_path).map_err(|e| {
        format!(
            "Failed to read Resource Library bundle path '{}': {}",
            target.delete_path.display(),
            e
        )
    })?;

    if metadata.file_type().is_symlink() {
        remove_symlink_path(&target.delete_path)
            .map_err(|e| format!("Failed to remove Resource Library bundle symlink: {}", e))
    } else if metadata.is_dir() {
        std::fs::remove_dir_all(&target.delete_path)
            .map_err(|e| format!("Failed to remove Resource Library bundle directory: {}", e))
    } else {
        Err(format!(
            "Resource Library bundle path '{}' is not removable",
            target.delete_path.display()
        ))
    }
}

fn path_resolves_to(path: &Path, target: &Path) -> bool {
    path.canonicalize()
        .ok()
        .zip(target.canonicalize().ok())
        .is_some_and(|(left, right)| left == right)
}

async fn is_shared_central_installation(
    pool: &DbPool,
    installation: &db::SkillInstallation,
    central_root: &Path,
    central_skill_dir: &Path,
) -> Result<bool, String> {
    if installation.agent_id == "central" {
        return Ok(true);
    }

    let agent = match db::get_agent_by_id(pool, &installation.agent_id).await? {
        Some(agent) => agent,
        None => return Ok(false),
    };
    let agent_dir = PathBuf::from(agent.global_skills_dir);
    if paths_resolve_to_same_entry(&agent_dir, central_root) {
        return Ok(true);
    }

    Ok(installation.link_type == "copy"
        && path_resolves_to(Path::new(&installation.installed_path), central_skill_dir))
}

fn build_skill_directory_nodes(
    root: &Path,
    current: &Path,
    visited_dirs: &[PathBuf],
) -> Result<Vec<SkillDirectoryNode>, String> {
    let entries = std::fs::read_dir(current)
        .map_err(|e| format!("Failed to read directory '{}': {}", current.display(), e))?;

    let mut nodes = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|e| {
            format!(
                "Failed to read directory entry in '{}': {}",
                current.display(),
                e
            )
        })?;
        let path = entry.path();
        let metadata = std::fs::symlink_metadata(&path)
            .map_err(|e| format!("Failed to read metadata for '{}': {}", path.display(), e))?;
        let is_dir = std::fs::metadata(&path)
            .map(|target_metadata| target_metadata.file_type().is_dir())
            .unwrap_or_else(|_| metadata.file_type().is_dir());
        let canonical_dir = if is_dir {
            Some(path.canonicalize().map_err(|e| {
                format!(
                    "Failed to resolve directory target '{}': {}",
                    path.display(),
                    e
                )
            })?)
        } else {
            None
        };

        if canonical_dir
            .as_ref()
            .is_some_and(|canonical| visited_dirs.iter().any(|visited| visited == canonical))
        {
            continue;
        }

        let children = if is_dir {
            let mut next_visited = visited_dirs.to_vec();
            if let Some(canonical_dir) = canonical_dir.clone() {
                next_visited.push(canonical_dir);
            }
            build_skill_directory_nodes(root, &path, &next_visited)?
        } else {
            Vec::new()
        };
        let relative_path = portable_relative_path(path.strip_prefix(root).unwrap_or(&path));

        nodes.push(SkillDirectoryNode {
            name: entry.file_name().to_string_lossy().into_owned(),
            path: path.to_string_lossy().into_owned(),
            relative_path,
            is_dir,
            children,
        });
    }

    nodes.sort_by(|a, b| match (a.is_dir, b.is_dir) {
        (true, false) => std::cmp::Ordering::Less,
        (false, true) => std::cmp::Ordering::Greater,
        _ => a
            .name
            .to_lowercase()
            .cmp(&b.name.to_lowercase())
            .then_with(|| a.name.cmp(&b.name)),
    });

    Ok(nodes)
}

fn observation_conflict_group(agent_id: &str, skill_id: &str) -> String {
    format!("{agent_id}::{skill_id}")
}

fn observation_conflict_counts(observations: &[db::AgentSkillObservation]) -> HashMap<String, i64> {
    let mut counts = HashMap::new();
    for observation in observations {
        *counts.entry(observation.skill_id.clone()).or_insert(0) += 1;
    }
    counts
}

fn observation_conflict_metadata(
    agent_id: &str,
    skill_id: &str,
    counts: &HashMap<String, i64>,
) -> (Option<String>, i64) {
    let count = counts.get(skill_id).copied().unwrap_or(0);
    if count > 1 {
        (Some(observation_conflict_group(agent_id, skill_id)), count)
    } else {
        (None, 0)
    }
}

fn installation_details(installations: Vec<db::SkillInstallation>) -> Vec<SkillInstallationDetail> {
    installations
        .into_iter()
        .map(|i| SkillInstallationDetail {
            skill_id: i.skill_id,
            agent_id: i.agent_id,
            installed_path: i.installed_path,
            link_type: i.link_type,
            symlink_target: i.symlink_target,
            installed_at: i.created_at,
        })
        .collect()
}

async fn shared_agent_ids_for_skill(
    pool: &DbPool,
    skill_id: &str,
    is_central: bool,
) -> Result<Vec<String>, String> {
    let mut agent_ids: BTreeSet<String> =
        db::get_read_only_observed_agent_ids_for_skill(pool, skill_id)
            .await?
            .into_iter()
            .collect();

    if is_central {
        let central = db::get_agent_by_id(pool, "central").await?;
        for agent in db::get_all_agents(pool).await? {
            if agent.id != "central" && agent.is_enabled && (db::agent_supports_universal_agents_skills(&agent.id)
                || central.as_ref().is_some_and(|central| paths_resolve_to_same_entry(Path::new(&agent.global_skills_dir), Path::new(&central.global_skills_dir)))) {
                agent_ids.insert(agent.id);
            }
        }
    }

    Ok(agent_ids.into_iter().collect())
}

async fn read_only_agent_ids_for_skill(pool: &DbPool, skill_id: &str, is_central: bool) -> Result<Vec<String>, String> {
    let mut agent_ids: BTreeSet<String> = shared_agent_ids_for_skill(pool, skill_id, is_central).await?.into_iter().collect();
    for installation in db::get_skill_installations(pool, skill_id).await? {
        agent_ids.remove(&installation.agent_id);
    }
    Ok(agent_ids.into_iter().collect())
}

async fn get_observation_detail(
    pool: &DbPool,
    skill_id: &str,
    agent_id: &str,
    row_id: Option<&str>,
) -> Result<Option<SkillDetail>, String> {
    let observations = db::get_agent_skill_observations(pool, agent_id).await?;
    if observations.is_empty() {
        return Ok(None);
    }

    let conflict_counts = observation_conflict_counts(&observations);
    let matches: Vec<db::AgentSkillObservation> = observations
        .into_iter()
        .filter(|observation| observation.skill_id == skill_id)
        .collect();

    if matches.is_empty() {
        return Ok(None);
    }

    let observation = match row_id {
        Some(row_id) => matches
            .into_iter()
            .find(|observation| observation.row_id == row_id)
            .ok_or_else(|| {
                format!(
                    "Observation row '{}' not found for skill '{}'",
                    row_id, skill_id
                )
            })?,
        None if matches.len() == 1 => matches.into_iter().next().expect("single match"),
        None => {
            return Err(format!(
                "Multiple observed rows found for skill '{}'; row_id is required",
                skill_id
            ))
        }
    };

    let manageable_skill = db::get_skill_by_id(pool, &observation.skill_id).await?;
    let installations = if observation.is_read_only {
        Vec::new()
    } else {
        installation_details(db::get_skill_installations(pool, &observation.skill_id).await?)
    };
    let collections = if observation.is_read_only {
        Vec::new()
    } else {
        db::get_skill_collections(pool, &observation.skill_id).await?
    };
    let metadata = db::get_skill_metadata(pool, &observation.skill_id).await?;
    let tags = db::parse_skill_metadata_tags(metadata.as_ref());
    let (conflict_group, conflict_count) =
        observation_conflict_metadata(agent_id, &observation.skill_id, &conflict_counts);

    Ok(Some(SkillDetail {
        row_id: observation.row_id,
        id: observation.skill_id.clone(),
        name: observation.name,
        description: observation.description.or_else(|| {
            manageable_skill
                .as_ref()
                .and_then(|skill| skill.description.clone())
        }),
        file_path: observation.file_path,
        dir_path: observation.dir_path,
        canonical_path: if observation.is_read_only {
            None
        } else {
            manageable_skill
                .as_ref()
                .and_then(|skill| skill.canonical_path.clone())
        },
        is_central: manageable_skill
            .as_ref()
            .map(|skill| skill.is_central)
            .unwrap_or(false),
        source: manageable_skill
            .as_ref()
            .and_then(|skill| skill.source.clone())
            .or_else(|| Some(observation.link_type.clone())),
        source_url: None,
        source_author: None,
        source_repo: None,
        source_path: None,
        notes: metadata.and_then(|metadata| metadata.notes),
        tags,
        scanned_at: observation.scanned_at.clone(),
        created_at: observation.scanned_at.clone(),
        updated_at: observation.scanned_at,
        source_kind: Some(observation.source_kind),
        source_root: Some(observation.source_root),
        is_read_only: observation.is_read_only,
        conflict_group,
        conflict_count,
        read_only_agents: Vec::new(),
        installations,
        collections,
    }))
}

async fn get_skill_detail_with_row_impl(
    pool: &DbPool,
    skill_id: &str,
    agent_id: Option<&str>,
    row_id: Option<&str>,
) -> Result<SkillDetail, String> {
    if let Some(agent_id) = agent_id {
        let has_managed_installation = db::get_skill_installations(pool, skill_id)
            .await?
            .iter()
            .any(|installation| installation.agent_id == agent_id);
        let selects_observation = row_id.is_some_and(|row_id| row_id != skill_id);

        if selects_observation || (!has_managed_installation && row_id.is_none()) {
            if let Some(detail) = get_observation_detail(pool, skill_id, agent_id, row_id).await? {
                return Ok(detail);
            }
        }
    }

    let mut skill = db::get_skill_by_id(pool, skill_id).await?;
    if skill.is_none() {
        let resource_root = db::get_skill_resource_library_dir(pool).await?;
        sync_resource_library_skills(pool, &resource_root).await?;
        skill = db::get_skill_by_id(pool, skill_id).await?;
    }
    let skill = skill.ok_or_else(|| format!("Skill '{}' not found", skill_id))?;

    let row_id = skill.id.clone();
    let raw_installations = db::get_skill_installations(pool, skill_id).await?;
    let request_installation = agent_id.and_then(|agent_id| {
        raw_installations
            .iter()
            .find(|installation| installation.agent_id == agent_id)
    });
    let (file_path, dir_path, canonical_path) =
        skill_detail_file_and_dir_paths(&skill, request_installation);
    let installations = installation_details(raw_installations);
    let collections = db::get_skill_collections(pool, skill_id).await?;
    let read_only_agents = read_only_agent_ids_for_skill(pool, skill_id, skill.is_central).await?;
    let source_metadata = db::get_skill_source(pool, skill_id).await?;
    let metadata = db::get_skill_metadata(pool, skill_id).await?;
    let tags = db::parse_skill_metadata_tags(metadata.as_ref());
    let mut timestamp_skill = skill.clone();
    timestamp_skill.file_path = file_path.clone();
    timestamp_skill.canonical_path = Some(dir_path.clone());
    let (created_at, updated_at) = skill_filesystem_timestamps(&timestamp_skill);
    let updated_at = skill_updated_at_after_check(pool, &skill.id, updated_at).await?;

    Ok(SkillDetail {
        row_id,
        id: skill.id,
        name: skill.name,
        description: skill.description,
        file_path,
        dir_path,
        canonical_path,
        is_central: skill.is_central,
        source: skill.source,
        source_url: source_metadata
            .as_ref()
            .and_then(|source| source.source_url.clone()),
        source_author: source_metadata
            .as_ref()
            .and_then(|source| source.source_author.clone()),
        source_repo: source_metadata
            .as_ref()
            .and_then(|source| source.source_repo.clone()),
        source_path: source_metadata.and_then(|source| source.source_path),
        notes: metadata.and_then(|metadata| metadata.notes),
        tags,
        scanned_at: skill.scanned_at,
        created_at,
        updated_at,
        source_kind: None,
        source_root: None,
        is_read_only: false,
        conflict_group: None,
        conflict_count: 0,
        read_only_agents,
        installations,
        collections,
    })
}

/// Testable core implementation of `get_skills_by_agent`.
///
/// Returns skills for the given agent enriched with installation metadata
/// (`dir_path`, `link_type`, `symlink_target`) so the frontend `SkillCard`
/// can display the correct source indicator.
pub async fn get_skills_by_agent_impl(
    pool: &DbPool,
    agent_id: &str,
) -> Result<Vec<SkillForAgent>, String> {
    let installed = db::get_skills_for_agent(pool, agent_id).await?;
    if !installed.is_empty() {
        return Ok(installed);
    }

    let Some(agent) = db::get_agent_by_id(pool, agent_id).await? else {
        return Ok(installed);
    };
    let Some(central_agent) = db::get_agent_by_id(pool, "central").await? else {
        return Ok(installed);
    };

    if agent.id == "central"
        || !Path::new(&agent.global_skills_dir).exists()
        || !paths_resolve_to_same_entry(
            Path::new(&agent.global_skills_dir),
            Path::new(&central_agent.global_skills_dir),
        )
    {
        return Ok(installed);
    }

    let mut shared_skills = Vec::new();
    for skill in db::get_central_skills(pool).await? {
        let dir_path = skill
            .canonical_path
            .clone()
            .or_else(|| {
                Path::new(&skill.file_path)
                    .parent()
                    .map(|path| path.to_string_lossy().into_owned())
            })
            .unwrap_or_else(|| skill.file_path.clone());
        let (created_at, _) = skill_filesystem_timestamps(&skill);
        let source_metadata = db::get_skill_source(pool, &skill.id).await?;
        let (link_type, symlink_target) = detect_link_type(Path::new(&dir_path), true);

        shared_skills.push(SkillForAgent {
            installation_source: "shared".to_string(),
            id: skill.id.clone(),
            row_id: skill.id,
            name: skill.name,
            description: skill.description,
            file_path: skill.file_path,
            dir_path,
            link_type,
            symlink_target,
            is_central: skill.is_central,
            source: skill.source,
            source_kind: Some("shared-central".to_string()),
            source_root: Some(central_agent.global_skills_dir.clone()),
            is_read_only: false,
            conflict_group: None,
            conflict_count: 0,
            source_url: source_metadata
                .as_ref()
                .and_then(|source| source.source_url.clone()),
            source_author: source_metadata
                .as_ref()
                .and_then(|source| source.source_author.clone()),
            source_repo: source_metadata
                .as_ref()
                .and_then(|source| source.source_repo.clone()),
            source_path: source_metadata
                .as_ref()
                .and_then(|source| source.source_path.clone()),
            created_at: Some(created_at),
        });
    }

    Ok(shared_skills)
}

/// Tauri command: return all skills installed for a given agent, including
/// installation metadata needed by the platform-view skill cards.
#[tauri::command]
pub async fn get_skills_by_agent(
    state: State<'_, AppState>,
    agent_id: String,
) -> Result<Vec<SkillForAgent>, String> {
    get_skills_by_agent_impl(&state.db, &agent_id).await
}

async fn skill_with_links(pool: &DbPool, skill: db::Skill) -> Result<SkillWithLinks, String> {
    let installations = db::get_skill_installations(pool, &skill.id).await?;
    let linked_agents: Vec<String> = installations.into_iter().map(|i| i.agent_id).collect();
    let read_only_agents = read_only_agent_ids_for_skill(pool, &skill.id, skill.is_central).await?;
    let (created_at, updated_at) = skill_filesystem_timestamps(&skill);
    let updated_at = skill_updated_at_after_check(pool, &skill.id, updated_at).await?;
    let source_metadata = db::get_skill_source(pool, &skill.id).await?;
    let metadata = db::get_skill_metadata(pool, &skill.id).await?;
    let tags = db::parse_skill_metadata_tags(metadata.as_ref());

    let shared_agents = shared_agent_ids_for_skill(pool, &skill.id, skill.is_central).await?;
    let github_stars = match source_metadata.as_ref() {
        Some(source) if matches!(source.source_type.as_str(), "github" | "skills-cli") => {
            match source.source_repo.as_deref() {
                Some(repo) => db::get_github_stars(pool, repo).await?,
                None => None,
            }
        }
        _ => None,
    };
    Ok(SkillWithLinks {
        github_stars,
        id: skill.id,
        name: skill.name,
        description: skill.description,
        file_path: skill.file_path,
        canonical_path: skill.canonical_path,
        is_central: skill.is_central,
        source: skill.source,
        source_url: source_metadata
            .as_ref()
            .and_then(|source| source.source_url.clone()),
        source_author: source_metadata
            .as_ref()
            .and_then(|source| source.source_author.clone()),
        source_repo: source_metadata
            .as_ref()
            .and_then(|source| source.source_repo.clone()),
        source_path: source_metadata.and_then(|source| source.source_path),
        notes: metadata.and_then(|metadata| metadata.notes),
        tags,
        scanned_at: skill.scanned_at,
        created_at,
        updated_at,
        linked_agents,
        shared_agents,
        read_only_agents,
    })
}

fn scan_count_for_bundle(target: &CentralBundleTarget) -> usize {
    scan_skill_root(&target.entry_path, true, ScanDirectoryOptions::nested()).len()
}

async fn central_skill_bundle_from_target(
    pool: &DbPool,
    target: &CentralBundleTarget,
    skills: &[db::Skill],
    scanned_skill_count: usize,
) -> Result<CentralSkillBundle, String> {
    let mut linked_agents = BTreeSet::new();
    let mut read_only_agents = BTreeSet::new();

    for skill in skills {
        for installation in db::get_skill_installations(pool, &skill.id).await? {
            linked_agents.insert(installation.agent_id);
        }
        for agent_id in read_only_agent_ids_for_skill(pool, &skill.id, skill.is_central).await? {
            read_only_agents.insert(agent_id);
        }
    }

    let name = Path::new(&target.relative_path)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(&target.relative_path)
        .to_string();

    Ok(CentralSkillBundle {
        name,
        relative_path: target.relative_path.clone(),
        path: target.delete_path.to_string_lossy().into_owned(),
        is_symlink: target.is_symlink,
        skill_count: scanned_skill_count.max(skills.len()),
        linked_agent_count: linked_agents.len(),
        read_only_agent_count: read_only_agents.len(),
    })
}

async fn central_bundle_preview_for_target(
    pool: &DbPool,
    target: CentralBundleTarget,
) -> Result<CentralSkillBundleDeletePreview, String> {
    let skills = central_skills_in_bundle(pool, &target).await?;
    let scanned_skill_count = scan_count_for_bundle(&target);

    if skills.is_empty() && scanned_skill_count == 0 {
        return Err(format!(
            "No Central Skills found under bundle '{}'",
            target.relative_path
        ));
    }

    let bundle =
        central_skill_bundle_from_target(pool, &target, &skills, scanned_skill_count).await?;
    let mut affected_agents = BTreeSet::new();
    let mut skipped_read_only_agents = BTreeSet::new();
    let mut skills_with_links = Vec::with_capacity(skills.len());

    for skill in skills {
        let linked = skill_with_links(pool, skill).await?;
        affected_agents.extend(linked.linked_agents.iter().cloned());
        skipped_read_only_agents.extend(linked.read_only_agents.iter().cloned());
        skills_with_links.push(linked);
    }

    Ok(CentralSkillBundleDeletePreview {
        bundle,
        skills: skills_with_links,
        affected_agents: affected_agents.into_iter().collect(),
        skipped_read_only_agents: skipped_read_only_agents.into_iter().collect(),
    })
}

async fn resource_skill_bundle_from_target(
    pool: &DbPool,
    target: &ResourceBundleTarget,
    skills: &[db::Skill],
    scanned_skill_count: usize,
) -> Result<CentralSkillBundle, String> {
    let mut linked_agents = BTreeSet::new();

    for skill in skills {
        for installation in db::get_skill_installations(pool, &skill.id).await? {
            linked_agents.insert(installation.agent_id);
        }
    }

    let name = Path::new(&target.relative_path)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(&target.relative_path)
        .to_string();

    Ok(CentralSkillBundle {
        name,
        relative_path: target.relative_path.clone(),
        path: target.delete_path.to_string_lossy().into_owned(),
        is_symlink: target.is_symlink,
        skill_count: scanned_skill_count.max(skills.len()),
        linked_agent_count: linked_agents.len(),
        read_only_agent_count: 0,
    })
}

async fn resource_bundle_preview_for_target(
    pool: &DbPool,
    target: ResourceBundleTarget,
) -> Result<CentralSkillBundleDeletePreview, String> {
    let skills = resource_skills_in_bundle(pool, &target).await?;
    let scanned_skill_count =
        scan_skill_root(&target.entry_path, false, ScanDirectoryOptions::nested()).len();

    if skills.is_empty() && scanned_skill_count == 0 {
        return Err(format!(
            "No Resource Library Skills found under bundle '{}'",
            target.relative_path
        ));
    }

    let bundle =
        resource_skill_bundle_from_target(pool, &target, &skills, scanned_skill_count).await?;
    let mut affected_agents = BTreeSet::new();
    let mut skills_with_links = Vec::with_capacity(skills.len());

    for skill in skills {
        let linked = skill_with_links(pool, skill).await?;
        affected_agents.extend(linked.linked_agents.iter().cloned());
        skills_with_links.push(linked);
    }

    Ok(CentralSkillBundleDeletePreview {
        bundle,
        skills: skills_with_links,
        affected_agents: affected_agents.into_iter().collect(),
        skipped_read_only_agents: Vec::new(),
    })
}

/// Tauri command: return all Central Skills with per-platform link status.
///
/// For each skill in the central skills directory, the response includes a
/// `linked_agents` array listing every agent that has an installation record
/// for that skill (regardless of whether the link type is symlink or copy).
#[tauri::command]
pub async fn get_central_skills(state: State<'_, AppState>) -> Result<Vec<SkillWithLinks>, String> {
    let _ = super::linker::reconcile_orphaned_central_skills(&state.db).await;
    let skills = db::get_central_skills(&state.db).await?;

    let mut result = Vec::with_capacity(skills.len());
    for skill in skills {
        result.push(skill_with_links(&state.db, skill).await?);
    }

    Ok(result)
}

#[tauri::command]
pub async fn get_resource_library_skills(
    state: State<'_, AppState>,
) -> Result<Vec<SkillWithLinks>, String> {
    get_resource_library_skills_impl(&state.db).await
}

pub async fn get_resource_library_skills_impl(
    pool: &DbPool,
) -> Result<Vec<SkillWithLinks>, String> {
    let resource_root = db::get_skill_resource_library_dir(pool).await?;
    let scanned = sync_resource_library_skills(pool, &resource_root).await?;
    let records: HashMap<PathBuf, db::Skill> = db::get_resource_library_skills(pool)
        .await?
        .into_iter()
        .filter_map(|skill| {
            let path = skill.canonical_path.as_deref().map(PathBuf::from)?;
            Some((std::fs::canonicalize(&path).unwrap_or(path), skill))
        })
        .collect();
    let mut result = Vec::with_capacity(scanned.len());
    for resource_skill in scanned {
        let mut skill = match resource_record_from_snapshot(
            pool,
            &resource_skill.id,
            &resource_skill.dir_path,
            &records,
        )
        .await?
        {
            Some(skill) => skill,
            None => {
                let skill_id = resolve_resource_scanned_skill_id(
                    pool,
                    &resource_root,
                    Path::new(&resource_skill.dir_path),
                    &resource_skill.id,
                )
                .await?;
                let inferred_source = infer_resource_github_source(
                    &resource_root,
                    Path::new(&resource_skill.dir_path),
                    &skill_id,
                );
                if let Some(source) = inferred_source.as_ref() {
                    db::upsert_skill_source(pool, source).await?;
                }
                let skill = db::Skill {
                    id: skill_id,
                    name: resource_skill.name.clone(),
                    description: resource_skill.description.clone(),
                    file_path: resource_skill.file_path.clone(),
                    canonical_path: Some(resource_skill.dir_path.clone()),
                    is_central: false,
                    source: inferred_source
                        .as_ref()
                        .and_then(source_label_from_metadata)
                        .or_else(|| Some("resource-library".to_string())),
                    content: None,
                    scanned_at: Utc::now().to_rfc3339(),
                };
                db::upsert_skill(pool, &skill).await?;
                skill
            }
        };
        skill.name = resource_skill.name;
        skill.description = resource_skill.description;
        skill.file_path = resource_skill.file_path;
        skill.canonical_path = Some(resource_skill.dir_path);
        result.push(skill_with_links(pool, skill).await?);
    }

    Ok(result)
}

#[tauri::command]
pub async fn export_resource_skill_directory_list(
    state: State<'_, AppState>,
    output_path: String,
) -> Result<usize, String> {
    let skills = get_resource_library_skills_impl(&state.db).await?;
    let resource_root = db::get_skill_resource_library_dir(&state.db).await?;
    let mut rows: BTreeMap<String, Vec<SkillWithLinks>> = BTreeMap::new();

    for skill in skills {
        let Some(canonical_path) = skill.canonical_path.as_deref() else {
            continue;
        };
        let dir = Path::new(canonical_path);
        let relative = dir
            .strip_prefix(&resource_root)
            .ok()
            .map(|path| path.to_string_lossy().replace('\\', "/"))
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| canonical_path.replace('\\', "/"));
        rows.entry(relative).or_default().push(skill);
    }

    let exported_count = rows.len();
    let mut csv =
        String::from("Directory,Skill Count,Skills,Source Repositories,Source Paths,Local Path,GitHub Stars\n");
    for (relative, mut grouped_skills) in rows {
        grouped_skills.sort_by(|left, right| left.name.cmp(&right.name));
        let skill_names = grouped_skills
            .iter()
            .map(|skill| skill.name.as_str())
            .collect::<Vec<_>>()
            .join("; ");
        let source_repos = grouped_skills
            .iter()
            .filter_map(|skill| skill.source_repo.as_deref())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>()
            .join("; ");
        let source_paths = grouped_skills
            .iter()
            .filter_map(|skill| skill.source_path.as_deref())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>()
            .join("; ");
        let local_path = grouped_skills
            .first()
            .and_then(|skill| skill.canonical_path.as_deref())
            .unwrap_or("");
        csv.push_str(&format!(
            "{},{},{},{},{},{},{}\n",
            csv_cell(&relative),
            grouped_skills.len(),
            csv_cell(&skill_names),
            csv_cell(&source_repos),
            csv_cell(&source_paths),
            csv_cell(local_path),
            csv_cell(&grouped_skills.iter().filter_map(|skill| skill.github_stars).map(|stars| stars.to_string()).collect::<BTreeSet<_>>().into_iter().collect::<Vec<_>>().join("; "))
        ));
    }

    std::fs::write(&output_path, csv)
        .map_err(|e| format!("Failed to export skill directory list: {}", e))?;
    Ok(exported_count)
}

fn csv_cell(value: &str) -> String {
    if value.contains(',') || value.contains('"') || value.contains('\n') || value.contains('\r') {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.to_string()
    }
}

fn validate_manual_skill_id(skill_id: &str) -> Result<String, String> {
    let trimmed = skill_id.trim();
    if trimmed.is_empty()
        || trimmed.contains('/')
        || trimmed.contains('\\')
        || trimmed == "."
        || trimmed == ".."
        || !is_simple_source_label(trimmed)
    {
        return Err("Invalid manual skill id".to_string());
    }
    Ok(trimmed.to_string())
}

fn is_simple_source_label(value: &str) -> bool {
    !value.is_empty()
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
}

fn is_safe_source_url(value: &str) -> bool {
    let Ok(url) = reqwest::Url::parse(value) else {
        return false;
    };
    matches!(url.scheme(), "http" | "https")
        && url.username().is_empty()
        && url.password().is_none()
        && url.query().is_none()
        && url.fragment().is_none()
}

fn is_portable_source_path(value: &str) -> bool {
    let trimmed = value.trim();
    if trimmed.is_empty()
        || trimmed.starts_with('/')
        || trimmed.starts_with('\\')
        || trimmed.contains('\\')
    {
        return false;
    }
    let bytes = trimmed.as_bytes();
    if bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':' {
        return false;
    }
    trimmed
        .split('/')
        .all(|part| !part.is_empty() && part != "." && part != "..")
}

fn manual_skill_markdown(input: &CreateManualResourceSkillRequest) -> Result<String, String> {
    #[derive(Serialize)]
    struct Frontmatter<'a> {
        name: &'a str,
        #[serde(skip_serializing_if = "Option::is_none")]
        description: Option<&'a str>,
    }

    let name = input.name.trim();
    if name.is_empty() {
        return Err("Manual skill name cannot be empty".to_string());
    }
    let description = input
        .description
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let yaml = serde_yaml::to_string(&Frontmatter { name, description })
        .map_err(|e| format!("Failed to create skill frontmatter: {}", e))?;
    let body = input.body.as_deref().unwrap_or("").trim();
    Ok(format!("---\n{}---\n\n{}\n", yaml, body))
}

pub async fn create_manual_resource_skill_impl(
    pool: &DbPool,
    input: CreateManualResourceSkillRequest,
) -> Result<SkillWithLinks, String> {
    let skill_id = validate_manual_skill_id(&input.skill_id)?;
    let resource_root = resource_root_path(pool).await?;
    let target_dir = resource_root.join(&skill_id);
    ensure_under_resource_root(&target_dir, &resource_root)
        .map_err(|_| "Manual skill target is outside Skill Resource Library".to_string())?;
    if target_dir.exists() {
        return Err(format!("Manual skill target '{}' already exists", skill_id));
    }

    let markdown = manual_skill_markdown(&input)?;
    std::fs::create_dir_all(&target_dir)
        .map_err(|e| format!("Failed to create manual skill directory: {}", e))?;
    let skill_md_path = target_dir.join("SKILL.md");
    std::fs::write(&skill_md_path, markdown)
        .map_err(|e| format!("Failed to write manual SKILL.md: {}", e))?;

    if input
        .source_url
        .as_deref()
        .is_some_and(|url| !is_safe_source_url(url))
    {
        return Err("Manual skill source URL is not safe".to_string());
    }
    if input
        .source_path
        .as_deref()
        .is_some_and(|path| !is_portable_source_path(path))
    {
        return Err("Manual skill source path is not portable".to_string());
    }

    let now = Utc::now().to_rfc3339();
    let skill = db::Skill {
        id: skill_id.clone(),
        name: input.name.trim().to_string(),
        description: input
            .description
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string),
        file_path: skill_md_path.to_string_lossy().into_owned(),
        canonical_path: Some(target_dir.to_string_lossy().into_owned()),
        is_central: false,
        source: Some("manual".to_string()),
        content: None,
        scanned_at: now.clone(),
    };
    db::upsert_skill(pool, &skill).await?;
    db::upsert_skill_source(
        pool,
        &db::SkillSource {
            skill_id: skill_id.clone(),
            source_type: "manual".to_string(),
            source_url: input.source_url.filter(|value| !value.trim().is_empty()),
            source_author: input.source_author.filter(|value| !value.trim().is_empty()),
            source_repo: input.source_repo.filter(|value| !value.trim().is_empty()),
            source_path: input.source_path.filter(|value| !value.trim().is_empty()),
            updated_at: now,
        },
    )
    .await?;

    skill_with_links(pool, skill).await
}

#[tauri::command]
pub async fn create_manual_resource_skill(
    state: State<'_, AppState>,
    input: CreateManualResourceSkillRequest,
) -> Result<SkillWithLinks, String> {
    create_manual_resource_skill_impl(&state.db, input).await
}

#[derive(Debug, Clone)]
struct LocalSkillImportCandidate {
    source_dir: PathBuf,
    relative_dir: PathBuf,
    skill_id: String,
}

fn path_component_to_string(path: &Path) -> Result<String, String> {
    path.file_name()
        .and_then(|name| name.to_str())
        .map(str::to_string)
        .ok_or_else(|| format!("Path '{}' has no valid directory name", path.display()))
}

fn resource_target_dir(
    resource_root: &Path,
    base_segments: &[String],
    collection_name: Option<&str>,
    relative_dir: &Path,
) -> Result<PathBuf, String> {
    let mut base = resource_root.to_path_buf();
    for segment in base_segments {
        base.push(segment);
    }
    let target = if let Some(collection_name) = collection_name {
        base.join(collection_name).join(relative_dir)
    } else {
        base.join(relative_dir)
    };
    ensure_under_resource_root(&target, resource_root)
        .map_err(|_| "Local skill target is outside Skill Resource Library".to_string())?;
    Ok(target)
}

fn discover_local_skill_candidates(
    source_dir: &Path,
) -> Result<
    (
        LocalResourceImportKind,
        Option<String>,
        Vec<LocalSkillImportCandidate>,
    ),
    String,
> {
    if !source_dir.is_dir() {
        return Err(format!(
            "Local skill source '{}' is not a directory",
            source_dir.display()
        ));
    }

    if source_dir.join("SKILL.md").is_file() {
        let skill_id = validate_manual_skill_id(&path_component_to_string(source_dir)?)?;
        return Ok((
            LocalResourceImportKind::SingleSkill,
            None,
            vec![LocalSkillImportCandidate {
                source_dir: source_dir.to_path_buf(),
                relative_dir: PathBuf::from(&skill_id),
                skill_id,
            }],
        ));
    }

    let collection_name = validate_manual_skill_id(&path_component_to_string(source_dir)?)?;
    let scanned = scan_skill_root(source_dir, false, ScanDirectoryOptions::nested());
    let mut candidates = Vec::new();
    for skill in scanned {
        let skill_dir = PathBuf::from(&skill.dir_path);
        let relative_dir = skill_dir
            .strip_prefix(source_dir)
            .map_err(|_| {
                format!(
                    "Local skill '{}' is outside selected source folder",
                    skill_dir.display()
                )
            })?
            .to_path_buf();
        if relative_dir.as_os_str().is_empty()
            || relative_dir
                .components()
                .any(|component| !matches!(component, Component::Normal(_)))
        {
            return Err(format!(
                "Local skill '{}' has an unsupported relative path",
                skill_dir.display()
            ));
        }
        candidates.push(LocalSkillImportCandidate {
            source_dir: skill_dir,
            relative_dir,
            skill_id: skill.id,
        });
    }

    candidates.sort_by(|left, right| left.relative_dir.cmp(&right.relative_dir));
    candidates.dedup_by(|left, right| left.source_dir == right.source_dir);

    if candidates.is_empty() {
        return Err(format!(
            "Local folder '{}' does not contain any SKILL.md files",
            source_dir.display()
        ));
    }

    Ok((
        LocalResourceImportKind::Collection,
        Some(collection_name),
        candidates,
    ))
}

async fn add_resource_skills_from_local_dir_impl(
    pool: &DbPool,
    input: AddLocalResourceSkillsRequest,
    target_base_segments: &[String],
    preserve_collection_folder: bool,
    source_type: &str,
    source_author: Option<String>,
    source_repo: Option<String>,
) -> Result<AddLocalResourceSkillsResult, String> {
    let source_dir = PathBuf::from(input.source_dir.trim());
    let source_dir = source_dir.canonicalize().map_err(|e| {
        format!(
            "Failed to resolve local skill source '{}': {}",
            source_dir.display(),
            e
        )
    })?;
    let resource_root = resource_root_path(pool).await?;
    std::fs::create_dir_all(&resource_root)
        .map_err(|e| format!("Failed to create Skill Resource Library directory: {}", e))?;
    let resource_root = resource_root.canonicalize().map_err(|e| {
        format!(
            "Failed to resolve Skill Resource Library directory '{}': {}",
            resource_root.display(),
            e
        )
    })?;
    if source_dir.starts_with(&resource_root) {
        return Err("Local skill source is already inside the Skill Resource Library".to_string());
    }

    let (import_kind, collection_name, candidates) = discover_local_skill_candidates(&source_dir)?;
    let candidates=candidates.into_iter().filter(|candidate|input.selected_skill_ids.as_ref().is_none_or(|ids|ids.contains(&candidate.skill_id))).collect::<Vec<_>>();
    if candidates.is_empty(){return Err("No importable skills were selected".into());}
    let mut files_transaction=super::github_import::ImportDirectoryTransaction::new(&resource_root)?;
    let mut skipped_skills = Vec::new();

    for candidate in &candidates {
        let target_collection_name = preserve_collection_folder
            .then_some(collection_name.as_deref())
            .flatten();
        let target_dir = resource_target_dir(
            &resource_root,
            target_base_segments,
            target_collection_name,
            &candidate.relative_dir,
        )?;
        if target_dir.exists() && !input.overwrite {
            skipped_skills.push(candidate.skill_id.clone());continue;
        }
        let staged=files_transaction.root.join(format!("new-{}",candidate.skill_id));
        copy_dir_all(&candidate.source_dir,&staged)?;
        if parse_skill_md(&staged.join("SKILL.md")).is_none(){return Err("Invalid local SKILL.md; existing files were preserved".into());}
        files_transaction.install(&staged,&target_dir,input.overwrite)?;

    }

    if skipped_skills.len() < candidates.len() {
        let mut directory = resource_root.clone();
        for segment in target_base_segments { directory.push(segment); }
        if preserve_collection_folder {
            if let Some(name) = collection_name.as_deref() { directory.push(name); }
        }
        for (index, (name, bytes)) in repository_docs::files(&source_dir)?.into_iter().enumerate() {
            let staged = files_transaction.root.join(format!("repository-readme-{index}"));
            std::fs::write(&staged, bytes).map_err(|e| format!("Failed to stage repository README: {e}"))?;
            files_transaction.install(&staged, &directory.join(name), true)?;
        }
    }

    let now = Utc::now().to_rfc3339();
    let mut added_skills = Vec::new();
    for candidate in &candidates {
        if skipped_skills.contains(&candidate.skill_id) {
            continue;
        }
        let target_collection_name = preserve_collection_folder
            .then_some(collection_name.as_deref())
            .flatten();
        let target_dir = resource_target_dir(
            &resource_root,
            target_base_segments,
            target_collection_name,
            &candidate.relative_dir,
        )?;
        let skill_md_path = target_dir.join("SKILL.md");
        let info = parse_skill_md(&skill_md_path).ok_or_else(|| {
            format!(
                "Imported local skill '{}' is not a valid SKILL.md file",
                skill_md_path.display()
            )
        })?;
        let skill = db::Skill {
            id: candidate.skill_id.clone(),
            name: info.name,
            description: info.description,
            file_path: skill_md_path.to_string_lossy().into_owned(),
            canonical_path: Some(target_dir.to_string_lossy().into_owned()),
            is_central: false,
            source: Some(source_type.to_string()),
            content: None,
            scanned_at: now.clone(),
        };
        db::upsert_skill(pool, &skill).await?;
        db::upsert_skill_source(
            pool,
            &db::SkillSource {
                skill_id: candidate.skill_id.clone(),
                source_type: source_type.to_string(),
                source_url: None,
                source_author: source_author.clone().or_else(|| collection_name.clone()),
                source_repo: source_repo.clone(),
                source_path: Some(candidate.source_dir.to_string_lossy().into_owned()),
                updated_at: now.clone(),
            },
        )
        .await?;
        added_skills.push(skill_with_links(pool, skill).await?);
    }

    files_transaction.commit()?;
    Ok(AddLocalResourceSkillsResult {
        source_dir: source_dir.to_string_lossy().into_owned(),
        import_kind,
        collection_name,
        added_skills,
        skipped_skills,
    })
}

#[derive(Serialize)]
#[serde(rename_all="camelCase")]
pub struct LocalImportPreviewItem {skill_id:String,name:String,conflict:bool}
#[tauri::command]
pub async fn preview_local_resource_skills(state:State<'_,AppState>,source_dir:String)->Result<Vec<LocalImportPreviewItem>,String>{
    let source=PathBuf::from(source_dir).canonicalize().map_err(|e|e.to_string())?;
    let root=resource_root_path(&state.db).await?;
    if source.starts_with(&root){return Err("Local source is already in the library".into());}
    let (_,collection,candidates)=discover_local_skill_candidates(&source)?;
    let mut result=Vec::new();
    for candidate in candidates {
        let info=parse_skill_md(&candidate.source_dir.join("SKILL.md")).ok_or("Invalid SKILL.md")?;
        let target=resource_target_dir(&root,&["local".into()],collection.as_deref(),&candidate.relative_dir)?;
        result.push(LocalImportPreviewItem{skill_id:candidate.skill_id,name:info.name,conflict:target.exists()});
    }
    Ok(result)
}

pub async fn add_local_resource_skills_impl(
    pool: &DbPool,
    input: AddLocalResourceSkillsRequest,
) -> Result<AddLocalResourceSkillsResult, String> {
    add_resource_skills_from_local_dir_impl(
        pool,
        input,
        &["local".to_string()],
        true,
        "local-folder",
        None,
        None,
    )
    .await
}

pub async fn add_repo_resource_skills_impl(
    pool: &DbPool,
    input: AddLocalResourceSkillsRequest,
    owner: &str,
    repo: &str,
) -> Result<AddLocalResourceSkillsResult, String> {
    let owner = validate_manual_skill_id(owner)?;
    let repo = validate_manual_skill_id(repo)?;
    add_resource_skills_from_local_dir_impl(
        pool,
        input,
        &[owner.clone(), repo.clone()],
        false,
        "github",
        Some(owner.clone()),
        Some(format!("{owner}/{repo}")),
    )
    .await
}

#[tauri::command]
pub async fn add_local_resource_skills(
    state: State<'_, AppState>,
    input: AddLocalResourceSkillsRequest,
) -> Result<AddLocalResourceSkillsResult, String> {
    add_local_resource_skills_impl(&state.db, input).await
}

async fn sync_resource_library_skills(
    pool: &DbPool,
    resource_root: &Path,
) -> Result<Vec<super::scanner::ScannedSkill>, String> {
    std::fs::create_dir_all(resource_root)
        .map_err(|e| format!("Failed to create Skill Resource Library directory: {}", e))?;

    let central_root = db::get_agent_by_id(pool, "central")
        .await?
        .map(|agent| PathBuf::from(agent.global_skills_dir));

    let scanned = scan_skill_root(
        resource_root,
        false,
        ScanDirectoryOptions::nested().preserving_duplicate_ids(),
    );
    let records: HashMap<PathBuf, db::Skill> = db::get_resource_library_skills(pool)
        .await?
        .into_iter()
        .filter_map(|skill| {
            let path = skill.canonical_path.as_deref().map(PathBuf::from)?;
            Some((std::fs::canonicalize(&path).unwrap_or(path), skill))
        })
        .collect();
    for skill in scanned.iter().cloned() {
        let existing =
            resource_record_from_snapshot(pool, &skill.id, &skill.dir_path, &records).await?;
        let skill_id = match existing.as_ref() {
            Some(existing) => existing.id.clone(),
            None => {
                resolve_resource_scanned_skill_id(
                    pool,
                    resource_root,
                    Path::new(&skill.dir_path),
                    &skill.id,
                )
                .await?
            }
        };
        let existing_source = existing.as_ref().and_then(|skill| skill.source.clone());
        let mut source_metadata = db::get_skill_source(pool, &skill_id).await?;
        if source_metadata.is_none() {
            if let Some(inferred) =
                infer_resource_github_source(resource_root, Path::new(&skill.dir_path), &skill_id)
            {
                db::upsert_skill_source(pool, &inferred).await?;
                source_metadata = Some(inferred);
            }
        } else if let Some(source) = source_metadata.as_mut() {
            let before = source.source_url.clone();
            scrub_non_updatable_inferred_source_url(source);
            if source.source_url != before {
                db::upsert_skill_source(pool, source).await?;
            }
        }
        let inferred_source = source_metadata
            .as_ref()
            .and_then(source_label_from_metadata);
        let source = match existing_source.as_deref() {
            None | Some("") | Some("resource-library") => {
                inferred_source.or_else(|| Some("resource-library".to_string()))
            }
            Some(_) => existing_source,
        };
        let db_skill = db::Skill {
            id: skill_id,
            name: skill.name,
            description: skill.description,
            file_path: skill.file_path,
            canonical_path: Some(skill.dir_path),
            is_central: false,
            source,
            content: None,
            scanned_at: Utc::now().to_rfc3339(),
        };

        if repair_legacy_resource_skill_centralization(
            pool,
            existing.as_ref(),
            &db_skill,
            central_root.as_deref(),
        )
        .await?
        {
            continue;
        }

        let has_managed_central_entry = existing.as_ref().is_some_and(|skill| {
            if !skill.is_central {
                return false;
            }
            let Some(central_root) = central_root.as_deref() else {
                return false;
            };
            let Some(canonical_path) = skill.canonical_path.as_deref().map(Path::new) else {
                return false;
            };
            canonical_path.starts_with(central_root) && canonical_path.join("SKILL.md").exists()
        });
        if has_managed_central_entry {
            continue;
        }
        db::upsert_skill(pool, &db_skill).await?;
    }

    Ok(scanned)
}

async fn resource_record_from_snapshot(
    pool: &DbPool,
    scanned_skill_id: &str,
    scanned_dir_path: &str,
    records: &HashMap<PathBuf, db::Skill>,
) -> Result<Option<db::Skill>, String> {
    let scanned_dir = Path::new(scanned_dir_path);
    let key = std::fs::canonicalize(scanned_dir).unwrap_or_else(|_| scanned_dir.to_path_buf());
    if let Some(candidate) = records.get(&key) {
        return Ok(Some(candidate.clone()));
    }

    let existing = db::get_skill_by_id(pool, scanned_skill_id).await?;
    if existing.as_ref().is_some_and(|skill| {
        !skill.is_central
            && skill.canonical_path.as_deref().is_some_and(|path| {
                Path::new(path).join("SKILL.md").is_file()
                    && !paths_resolve_to_same_entry(Path::new(path), scanned_dir)
            })
    }) {
        return Ok(None);
    }
    Ok(existing)
}

async fn resolve_resource_scanned_skill_id(
    pool: &DbPool,
    resource_root: &Path,
    scanned_dir: &Path,
    scanned_skill_id: &str,
) -> Result<String, String> {
    let existing = db::get_skill_by_id(pool, scanned_skill_id).await?;
    let can_use_scanned_id = existing
        .as_ref()
        .map(|skill| {
            skill.is_central
                || skill
                    .canonical_path
                    .as_deref()
                    .map(|path| {
                        !Path::new(path).join("SKILL.md").is_file()
                            || paths_resolve_to_same_entry(Path::new(path), scanned_dir)
                    })
                    .unwrap_or(true)
        })
        .unwrap_or(true);
    if can_use_scanned_id {
        return Ok(scanned_skill_id.to_string());
    }

    let relative_parts = scanned_dir
        .strip_prefix(resource_root)
        .ok()
        .map(|relative| {
            relative
                .components()
                .filter_map(|component| match component {
                    Component::Normal(value) => value.to_str(),
                    _ => None,
                })
                .filter(|part| is_simple_source_label(part))
                .map(|part| part.to_ascii_lowercase())
                .collect::<Vec<_>>()
        })
        .filter(|parts| parts.len() >= 2)
        .unwrap_or_else(|| vec![scanned_skill_id.to_ascii_lowercase()]);
    let base = relative_parts.join("-");
    let mut candidate = base.clone();
    let mut suffix = 2usize;
    while db::get_skill_by_id(pool, &candidate).await?.is_some() {
        candidate = format!("{base}-{suffix}");
        suffix += 1;
    }
    Ok(candidate)
}

async fn repair_legacy_resource_skill_centralization(
    pool: &DbPool,
    existing: Option<&db::Skill>,
    resource_skill: &db::Skill,
    central_root: Option<&Path>,
) -> Result<bool, String> {
    let (Some(central_root), Some(legacy_path)) = (
        central_root,
        existing
            .filter(|skill| skill.is_central)
            .and_then(|skill| skill.canonical_path.as_deref()),
    ) else {
        return Ok(false);
    };

    let legacy_path = PathBuf::from(legacy_path);
    let Some(resource_path) = resource_skill.canonical_path.as_deref().map(PathBuf::from) else {
        return Ok(false);
    };
    if !legacy_path.starts_with(central_root) || resource_path.starts_with(central_root) {
        return Ok(false);
    }

    let mut matching_legacy_platform_install = false;
    for installation in
        db::get_skill_installations_for_legacy_repair(pool, &resource_skill.id).await?
    {
        if installation.link_type != "symlink"
            || Path::new(&installation.installed_path) != legacy_path
        {
            continue;
        }
        let Some(target) = installation.symlink_target.as_deref().map(Path::new) else {
            continue;
        };
        if !paths_resolve_to_same_entry(target, &resource_path) {
            continue;
        }
        // Scanner records the Central Skills entry itself (agent_id=central, or a
        // platform that shares ~/.agents/skills). That is the current model, not
        // the old bug where a *platform* install was created inside Central Skills.
        if is_shared_central_installation(pool, &installation, central_root, &legacy_path).await? {
            continue;
        }
        matching_legacy_platform_install = true;
        break;
    }
    if !matching_legacy_platform_install {
        return Ok(false);
    }

    match std::fs::symlink_metadata(&legacy_path) {
        Ok(metadata)
            if metadata.file_type().is_symlink()
                && paths_resolve_to_same_entry(&legacy_path, &resource_path) =>
        {
            remove_symlink_path(&legacy_path).map_err(|error| {
                format!(
                    "Failed to remove legacy platform link from Central Skills '{}': {}",
                    legacy_path.display(),
                    error
                )
            })?;
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        _ => return Ok(false),
    }

    db::repair_legacy_centralized_resource_skill(
        pool,
        resource_skill,
        &legacy_path.to_string_lossy(),
    )
    .await?;
    Ok(true)
}

fn paths_resolve_to_same_entry(left: &Path, right: &Path) -> bool {
    matches!(
        (std::fs::canonicalize(left), std::fs::canonicalize(right)),
        (Ok(left), Ok(right)) if left == right
    )
}

#[tauri::command]
pub async fn update_skill_metadata(
    state: State<'_, AppState>,
    skill_id: String,
    metadata: UpdateSkillMetadataRequest,
) -> Result<SkillMetadataResponse, String> {
    db::get_skill_by_id(&state.db, &skill_id)
        .await?
        .ok_or_else(|| format!("Skill '{}' not found", skill_id))?;

    let notes = metadata
        .notes
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    let tags = normalize_skill_tags(metadata.tags);
    let saved = db::upsert_skill_metadata(&state.db, &skill_id, notes.as_deref(), &tags).await?;
    Ok(metadata_response(saved))
}

pub async fn update_resource_skill_source_metadata_impl(
    pool: &DbPool,
    skill_id: &str,
    metadata: UpdateSkillSourceMetadataRequest,
) -> Result<SkillSourceMetadataResponse, String> {
    let mut skill = db::get_skill_by_id(pool, skill_id)
        .await?
        .ok_or_else(|| format!("Skill '{}' not found", skill_id))?;
    if skill.is_central {
        return Err("Only Skill Resource Library skills can edit source metadata".to_string());
    }

    let resource_root = db::get_skill_resource_library_dir(pool).await?;
    let skill_dir = skill_directory_path_buf(&skill);
    ensure_under_resource_root(&skill_dir, &resource_root)?;

    let source_type = normalize_source_type(&metadata.source_type)?;
    let source_url = normalize_source_url(metadata.source_url)?;
    let source_repo = normalize_source_repo(metadata.source_repo)?;
    let mut source_author = normalized_optional_text(metadata.source_author);
    if source_author.is_none() && source_type == "github" {
        source_author = source_repo
            .as_deref()
            .and_then(|repo| repo.split('/').next())
            .map(str::to_string);
    }
    let source_path = normalized_optional_text(metadata.source_path);
    let source_label = source_label_from_parts(&source_type, source_repo.as_deref());

    skill.source = Some(source_label.clone());
    db::upsert_skill(pool, &skill).await?;

    let saved_source = db::SkillSource {
        skill_id: skill_id.to_string(),
        source_type,
        source_url,
        source_author,
        source_repo,
        source_path,
        updated_at: Utc::now().to_rfc3339(),
    };
    db::upsert_skill_source(pool, &saved_source).await?;
    Ok(source_response(saved_source, Some(source_label)))
}

#[tauri::command]
pub async fn update_resource_skill_source_metadata(
    state: State<'_, AppState>,
    skill_id: String,
    metadata: UpdateSkillSourceMetadataRequest,
) -> Result<SkillSourceMetadataResponse, String> {
    update_resource_skill_source_metadata_impl(&state.db, &skill_id, metadata).await
}

pub async fn get_central_skill_bundles_impl(
    pool: &DbPool,
) -> Result<Vec<CentralSkillBundle>, String> {
    let central_root = central_root_path(pool).await?;
    let entries = std::fs::read_dir(&central_root).map_err(|e| {
        format!(
            "Failed to read Central Skills root '{}': {}",
            central_root.display(),
            e
        )
    })?;

    let mut bundles = Vec::new();
    for entry in entries.flatten() {
        let entry_path = entry.path();
        let Ok(metadata) = std::fs::symlink_metadata(&entry_path) else {
            continue;
        };
        if !metadata.file_type().is_symlink() && !metadata.is_dir() {
            continue;
        }
        if entry_path.join("SKILL.md").exists() {
            continue;
        }

        let relative_path = entry.file_name().to_string_lossy().into_owned();
        let Ok(target) = validate_central_bundle_target(&relative_path, &central_root) else {
            continue;
        };
        let scanned_skill_count = scan_count_for_bundle(&target);
        let skills = central_skills_in_bundle(pool, &target).await?;
        if scanned_skill_count == 0 && skills.is_empty() {
            continue;
        }

        bundles.push(
            central_skill_bundle_from_target(pool, &target, &skills, scanned_skill_count).await?,
        );
    }

    bundles.sort_by(|a, b| a.relative_path.cmp(&b.relative_path));
    Ok(bundles)
}

#[tauri::command]
pub async fn get_central_skill_bundles(
    state: State<'_, AppState>,
) -> Result<Vec<CentralSkillBundle>, String> {
    get_central_skill_bundles_impl(&state.db).await
}

pub async fn get_central_skill_bundle_detail_impl(
    pool: &DbPool,
    relative_path: &str,
) -> Result<CentralSkillBundleDetail, String> {
    let central_root = central_root_path(pool).await?;
    let target = validate_central_bundle_target(relative_path, &central_root)?;
    let skills = central_skills_in_bundle(pool, &target).await?;
    let scanned_skill_count = scan_count_for_bundle(&target);

    if skills.is_empty() && scanned_skill_count == 0 {
        return Err(format!(
            "No Central Skills found under bundle '{}'",
            target.relative_path
        ));
    }

    let bundle =
        central_skill_bundle_from_target(pool, &target, &skills, scanned_skill_count).await?;
    let mut skills_with_links = Vec::with_capacity(skills.len());
    for skill in skills {
        skills_with_links.push(skill_with_links(pool, skill).await?);
    }

    Ok(CentralSkillBundleDetail {
        bundle,
        skills: skills_with_links,
    })
}

#[tauri::command]
pub async fn get_central_skill_bundle_detail(
    state: State<'_, AppState>,
    relative_path: String,
) -> Result<CentralSkillBundleDetail, String> {
    get_central_skill_bundle_detail_impl(&state.db, &relative_path).await
}

pub async fn preview_delete_central_skill_bundle_impl(
    pool: &DbPool,
    relative_path: &str,
) -> Result<CentralSkillBundleDeletePreview, String> {
    let central_root = central_root_path(pool).await?;
    let target = validate_central_bundle_target(relative_path, &central_root)?;
    central_bundle_preview_for_target(pool, target).await
}

#[tauri::command]
pub async fn preview_delete_central_skill_bundle(
    state: State<'_, AppState>,
    relative_path: String,
) -> Result<CentralSkillBundleDeletePreview, String> {
    preview_delete_central_skill_bundle_impl(&state.db, &relative_path).await
}

pub async fn delete_central_skill_bundle_impl(
    pool: &DbPool,
    relative_path: &str,
    options: DeleteCentralSkillBundleOptions,
) -> Result<DeleteCentralSkillBundleResult, String> {
    let central_root = central_root_path(pool).await?;
    let target = validate_central_bundle_target(relative_path, &central_root)?;
    let preview = central_bundle_preview_for_target(pool, target.clone()).await?;
    let skills = central_skills_in_bundle(pool, &target).await?;

    if !options.cascade_uninstall && !preview.affected_agents.is_empty() {
        return Err(format!(
            "Bundle skills are installed on agents: {}",
            preview.affected_agents.join(", ")
        ));
    }

    let mut uninstalled_agents = BTreeSet::new();
    if options.cascade_uninstall {
        for skill in &skills {
            let central_skill_dir = skill_directory_path_buf(skill);
            let installations = db::get_skill_installations(pool, &skill.id).await?;
            for installation in installations {
                if is_shared_central_installation(
                    pool,
                    &installation,
                    &central_root,
                    &central_skill_dir,
                )
                .await?
                {
                    continue;
                }

                uninstall_skill_from_agent_impl(pool, &skill.id, &installation.agent_id).await?;
                uninstalled_agents.insert(installation.agent_id);
            }
        }
        prune_mirrored_relative_path_from_install_targets(pool, &target.relative_path).await?;
    }

    let resource_root = db::get_skill_resource_library_dir(pool).await.ok();
    let mut demoted_skill_ids = HashSet::new();
    if let Some(resource_root) = resource_root.as_ref() {
        for skill in &skills {
            if let Some(resource_dir) =
                resource_target_for_central_skill(skill, &central_root, resource_root)
            {
                let mut restored = skill.clone();
                restored.file_path = resource_dir.join("SKILL.md").to_string_lossy().into_owned();
                restored.canonical_path = Some(resource_dir.to_string_lossy().into_owned());
                restored.is_central = false;
                restored.scanned_at = Utc::now().to_rfc3339();
                db::repair_legacy_centralized_resource_skill(
                    pool,
                    &restored,
                    &skill_directory_path_buf(skill).to_string_lossy(),
                )
                .await?;
                demoted_skill_ids.insert(skill.id.clone());
            }
        }
    }

    remove_central_bundle_target(&target)?;

    let mut removed_skill_ids = Vec::with_capacity(skills.len());
    for skill in &skills {
        if !demoted_skill_ids.contains(&skill.id) {
            db::delete_central_skill_records(pool, &skill.id, &skill.name).await?;
        }
        removed_skill_ids.push(skill.id.clone());
    }

    let removed_kind = target.removed_kind().to_string();

    Ok(DeleteCentralSkillBundleResult {
        relative_path: target.relative_path,
        removed_bundle_path: target.delete_path.to_string_lossy().into_owned(),
        removed_kind,
        removed_skill_ids,
        uninstalled_agents: uninstalled_agents.into_iter().collect(),
        skipped_read_only_agents: preview.skipped_read_only_agents,
    })
}

#[tauri::command]
pub async fn delete_central_skill_bundle(
    state: State<'_, AppState>,
    relative_path: String,
    options: Option<DeleteCentralSkillBundleOptions>,
) -> Result<DeleteCentralSkillBundleResult, String> {
    delete_central_skill_bundle_impl(
        &state.db,
        &relative_path,
        options.unwrap_or(DeleteCentralSkillBundleOptions {
            cascade_uninstall: false,
        }),
    )
    .await
}

pub async fn preview_delete_resource_skill_bundle_impl(
    pool: &DbPool,
    relative_path: &str,
) -> Result<CentralSkillBundleDeletePreview, String> {
    let resource_root = resource_root_path(pool).await?;
    sync_resource_library_skills(pool, &resource_root).await?;
    let target = validate_resource_bundle_target(relative_path, &resource_root)?;
    resource_bundle_preview_for_target(pool, target).await
}

#[tauri::command]
pub async fn preview_delete_resource_skill_bundle(
    state: State<'_, AppState>,
    relative_path: String,
) -> Result<CentralSkillBundleDeletePreview, String> {
    preview_delete_resource_skill_bundle_impl(&state.db, &relative_path).await
}

pub async fn delete_resource_skill_bundle_impl(
    pool: &DbPool,
    relative_path: &str,
    options: DeleteCentralSkillBundleOptions,
) -> Result<DeleteCentralSkillBundleResult, String> {
    let resource_root = resource_root_path(pool).await?;
    sync_resource_library_skills(pool, &resource_root).await?;
    let target = validate_resource_bundle_target(relative_path, &resource_root)?;
    let preview = resource_bundle_preview_for_target(pool, target.clone()).await?;
    let skills = resource_skills_in_bundle(pool, &target).await?;

    if !options.cascade_uninstall && !preview.affected_agents.is_empty() {
        return Err(format!(
            "Bundle skills are installed on agents: {}",
            preview.affected_agents.join(", ")
        ));
    }

    let mut uninstalled_agents = BTreeSet::new();
    if options.cascade_uninstall {
        for skill in &skills {
            let installations = db::get_skill_installations(pool, &skill.id).await?;
            for installation in installations {
                uninstall_skill_from_agent_impl(pool, &skill.id, &installation.agent_id).await?;
                uninstalled_agents.insert(installation.agent_id);
            }
        }
        prune_mirrored_relative_path_from_install_targets(pool, &target.relative_path).await?;
    }

    remove_resource_bundle_target(&target)?;

    let mut removed_skill_ids = Vec::with_capacity(skills.len());
    for skill in &skills {
        db::delete_skill_owned_records(pool, &skill.id, &skill.name).await?;
        removed_skill_ids.push(skill.id.clone());
    }

    let removed_kind = target.removed_kind().to_string();

    Ok(DeleteCentralSkillBundleResult {
        relative_path: target.relative_path,
        removed_bundle_path: target.delete_path.to_string_lossy().into_owned(),
        removed_kind,
        removed_skill_ids,
        uninstalled_agents: uninstalled_agents.into_iter().collect(),
        skipped_read_only_agents: preview.skipped_read_only_agents,
    })
}

#[tauri::command]
pub async fn delete_resource_skill_bundle(
    state: State<'_, AppState>,
    relative_path: String,
    options: Option<DeleteCentralSkillBundleOptions>,
) -> Result<DeleteCentralSkillBundleResult, String> {
    delete_resource_skill_bundle_impl(
        &state.db,
        &relative_path,
        options.unwrap_or(DeleteCentralSkillBundleOptions {
            cascade_uninstall: false,
        }),
    )
    .await
}

pub async fn delete_central_skill_impl(
    pool: &DbPool,
    skill_id: &str,
    options: DeleteCentralSkillOptions,
) -> Result<DeleteCentralSkillResult, String> {
    let skill = db::get_skill_by_id(pool, skill_id)
        .await?
        .ok_or_else(|| format!("Skill '{}' not found", skill_id))?;

    if !skill.is_central {
        return Err(format!("Skill '{}' is not central", skill_id));
    }

    let central = db::get_agent_by_id(pool, "central")
        .await?
        .ok_or_else(|| "Central agent not found in database".to_string())?;
    let central_root = PathBuf::from(&central.global_skills_dir)
        .canonicalize()
        .map_err(|e| format!("Failed to resolve Central Skills root: {}", e))?;
    let canonical_dir = canonical_delete_dir(&skill, &central_root);
    let delete_target = validate_central_delete_target(&canonical_dir, &central_root)?;

    let installations = db::get_skill_installations(pool, skill_id).await?;
    if !options.cascade_uninstall && !installations.is_empty() {
        let agents = installations
            .iter()
            .map(|installation| installation.agent_id.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        return Err(format!("Skill is installed on agents: {}", agents));
    }

    let skipped_read_only_agents = read_only_agent_ids_for_skill(pool, skill_id, true).await?;
    let mut uninstalled_agents = Vec::new();

    if options.cascade_uninstall {
        for installation in &installations {
            if is_shared_central_installation(pool, installation, &central_root, &delete_target)
                .await?
            {
                continue;
            }

            uninstall_skill_from_agent_impl(pool, skill_id, &installation.agent_id).await?;
            uninstalled_agents.push(installation.agent_id.clone());
        }
    }

    remove_central_skill_dir(&delete_target)?;
    db::delete_central_skill_records(pool, skill_id, &skill.name).await?;

    Ok(DeleteCentralSkillResult {
        skill_id: skill_id.to_string(),
        removed_canonical_path: delete_target.to_string_lossy().into_owned(),
        uninstalled_agents,
        skipped_read_only_agents,
    })
}

#[tauri::command]
pub async fn delete_central_skill(
    state: State<'_, AppState>,
    skill_id: String,
    options: Option<DeleteCentralSkillOptions>,
) -> Result<DeleteCentralSkillResult, String> {
    delete_central_skill_impl(
        &state.db,
        &skill_id,
        options.unwrap_or(DeleteCentralSkillOptions {
            cascade_uninstall: false,
        }),
    )
    .await
}

pub async fn delete_resource_skill_impl(
    pool: &DbPool,
    skill_id: &str,
    options: DeleteResourceSkillOptions,
) -> Result<DeleteResourceSkillResult, String> {
    let skill = db::get_skill_by_id(pool, skill_id)
        .await?
        .ok_or_else(|| format!("Skill '{}' not found", skill_id))?;

    if skill.is_central {
        return Err(format!(
            "Skill '{}' is central; use Central Skills deletion",
            skill_id
        ));
    }

    let resource_root = db::get_skill_resource_library_dir(pool)
        .await?
        .canonicalize()
        .map_err(|e| format!("Failed to resolve Skill Resource Library root: {}", e))?;
    let requested = resource_delete_dir(&skill);
    let missing = matches!(std::fs::symlink_metadata(&requested), Err(error) if error.kind() == std::io::ErrorKind::NotFound);
    let delete_target = if missing {
        // A previous interrupted delete may have removed the directory before its SQL commit.
        let parent = requested.parent().ok_or("Invalid resource directory")?.canonicalize().map_err(|e|e.to_string())?;
        let target = parent.join(requested.file_name().ok_or("Invalid resource directory")?);
        ensure_under_resource_root(&target, &resource_root)?;
        target
    } else { validate_resource_delete_target(&requested, &resource_root)? };

    let installations = db::get_skill_installations(pool, skill_id).await?;
    if !options.cascade_uninstall && !installations.is_empty() {
        let agents = installations
            .iter()
            .map(|installation| installation.agent_id.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        return Err(format!("Skill is installed on agents: {}", agents));
    }

    let skipped_read_only_agents = read_only_agent_ids_for_skill(pool, skill_id, false).await?;
    let mut uninstalled_agents = Vec::new();

    if options.cascade_uninstall {
        for installation in &installations {
            uninstall_skill_from_agent_impl(pool, skill_id, &installation.agent_id).await?;
            uninstalled_agents.push(installation.agent_id.clone());
        }
    }

    if !missing { remove_resource_skill_dir(&delete_target)?; }
    db::delete_skill_owned_records(pool, skill_id, &skill.name).await?;

    Ok(DeleteResourceSkillResult {
        skill_id: skill_id.to_string(),
        removed_canonical_path: delete_target.to_string_lossy().into_owned(),
        uninstalled_agents,
        skipped_read_only_agents,
    })
}

#[tauri::command]
pub async fn delete_resource_skill(
    state: State<'_, AppState>,
    skill_id: String,
    options: Option<DeleteResourceSkillOptions>,
) -> Result<DeleteResourceSkillResult, String> {
    delete_resource_skill_impl(
        &state.db,
        &skill_id,
        options.unwrap_or(DeleteResourceSkillOptions {
            cascade_uninstall: false,
        }),
    )
    .await
}

/// Tauri command: return detailed information about a skill, including all
/// installation records across agents. Each installation includes `installed_at`
/// (the `created_at` timestamp from the DB, renamed for frontend clarity).
#[tauri::command]
pub async fn get_skill_detail(
    state: State<'_, AppState>,
    skill_id: String,
    agent_id: Option<String>,
    row_id: Option<String>,
) -> Result<SkillDetail, String> {
    get_skill_detail_with_row_impl(&state.db, &skill_id, agent_id.as_deref(), row_id.as_deref())
        .await
}

/// Tauri command: read and return the raw content of a skill's `SKILL.md` file.
#[tauri::command]
pub async fn read_skill_content(
    state: State<'_, AppState>,
    skill_id: String,
) -> Result<String, String> {
    let skill = db::get_skill_by_id(&state.db, &skill_id)
        .await?
        .ok_or_else(|| format!("Skill '{}' not found", skill_id))?;

    std::fs::read_to_string(&skill.file_path)
        .map_err(|e| format!("Failed to read '{}': {}", skill.file_path, e))
}

fn repository_readme_path(directory: &Path) -> Result<Option<PathBuf>, String> {
    if !directory.exists() { return Ok(None); }
    let mut paths = std::fs::read_dir(directory).map_err(|e| format!("Failed to inspect repository documentation: {e}"))?
        .filter_map(Result::ok).filter(|entry| entry.file_name().to_string_lossy().eq_ignore_ascii_case("README.md"))
        .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_file()))
        .map(|entry| entry.path()).collect::<Vec<_>>();
    paths.sort();
    Ok(paths.into_iter().next())
}

pub(crate) fn read_repository_readme_impl(directory: &Path) -> Result<Option<String>, String> {
    repository_readme_path(directory)?.map(|path| std::fs::read_to_string(path)
        .map_err(|e| format!("Failed to read repository README: {e}"))).transpose()
}

pub(crate) async fn repository_document_directory(pool: &DbPool, directory: String, skill_id: Option<String>) -> Result<PathBuf, String> {
    // Shared/platform views install skill links, so read their repository document
    // from the main resource library instead of requiring a second installed copy.
    if let Some(id) = skill_id {
        if let Some(skill) = db::get_skill_by_id(pool, &id).await? {
            let root = resource_root_path(pool).await?;
            let root = root.canonicalize().unwrap_or(root);
            if let Some(path) = skill.canonical_path {
                let path = PathBuf::from(path);
                let path = path.canonicalize().unwrap_or(path);
                if let Some(repository) = repository_docs::group_directory(&root, &path) { return Ok(repository); }
            }
        }
    }
    Ok(PathBuf::from(directory))
}

#[tauri::command]
pub async fn read_repository_readme(state: State<'_, AppState>, directory: String, skill_id: Option<String>) -> Result<Option<String>, String> {
    let directory = repository_document_directory(&state.db, directory, skill_id).await?;
    read_repository_readme_impl(&directory)
}

#[tauri::command]
pub async fn read_repository_documents(state: State<'_, AppState>, directory: String, skill_id: Option<String>) -> Result<Vec<repository_docs::RepositoryDocument>, String> {
    let directory = repository_document_directory(&state.db, directory, skill_id).await?;
    repository_docs::read(&directory)
}

#[tauri::command]
pub async fn read_file_by_path(path: String) -> Result<String, String> {
    std::fs::read_to_string(&path).map_err(|e| format!("Failed to read '{}': {}", path, e))
}

#[tauri::command]
pub async fn list_skill_directory(dir_path: String) -> Result<Vec<SkillDirectoryNode>, String> {
    let root = Path::new(&dir_path);
    if !root.exists() {
        return Err(format!("Path does not exist: {}", dir_path));
    }
    if !root.is_dir() {
        return Err(format!("Path is not a directory: {}", dir_path));
    }

    let canonical_root = root
        .canonicalize()
        .map_err(|e| format!("Failed to resolve directory '{}': {}", dir_path, e))?;

    build_skill_directory_nodes(root, root, &[canonical_root])
}

#[tauri::command]
pub async fn open_in_file_manager(path: String) -> Result<(), String> {
    let native = resolve_path_for_file_manager(&path)?;
    open_in_file_manager_impl(&native)
}

fn open_in_file_manager_impl(path: &str) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg(path)
            .spawn()
            .map_err(|e| format!("Failed to open: {}", e))?;
    }

    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("explorer")
            .arg(path)
            .spawn()
            .map_err(|e| format!("Failed to open: {}", e))?;
    }

    #[cfg(target_os = "linux")]
    {
        std::process::Command::new("xdg-open")
            .arg(path)
            .spawn()
            .map_err(|e| format!("Failed to open: {}", e))?;
    }

    Ok(())
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::linker::create_symlink;
    use crate::db::{self, AgentSkillObservation, Skill, SkillInstallation};
    use chrono::Utc;
    use sqlx::SqlitePool;
    use std::{fs, path::Path};
    use tempfile::TempDir;

    #[tokio::test]
    async fn successful_check_advances_display_time_without_touching_skill_files() {
        let pool = setup_test_db().await;
        let directory = TempDir::new().unwrap();
        let path = directory.path().join("SKILL.md");
        fs::write(&path, "---\nname: Same\n---\nUnchanged").unwrap();
        let before = fs::metadata(&path).unwrap().modified().unwrap();
        let mut skill = make_skill("checked-skill", "Same", false);
        skill.file_path = path.to_string_lossy().into_owned();
        skill.canonical_path = Some(directory.path().to_string_lossy().into_owned());
        db::upsert_skill(&pool, &skill).await.unwrap();
        db::upsert_skill_source(&pool, &db::SkillSource {
            skill_id: skill.id.clone(), source_type: "github".into(), source_url: None,
            source_author: Some("example".into()), source_repo: Some("example/repo".into()),
            source_path: Some("SKILL.md".into()), updated_at: Utc::now().to_rfc3339(),
        }).await.unwrap();
        let checked_at = (Utc::now() + chrono::Duration::minutes(1)).to_rfc3339();
        db::record_skill_check_times(&pool, &[skill.id.clone()], &checked_at).await.unwrap();
        let result = skill_with_links(&pool, skill).await.unwrap();
        assert_eq!(result.updated_at, checked_at);
        assert_eq!(fs::metadata(&path).unwrap().modified().unwrap(), before);
        assert!(fs::read_to_string(path).unwrap().ends_with("Unchanged"));
    }

    async fn setup_test_db() -> SqlitePool {
        let pool = SqlitePool::connect(":memory:").await.unwrap();
        db::init_database(&pool).await.unwrap();
        pool
    }

    fn expected_universal_read_only_agents(extra: &[&str]) -> Vec<String> {
        let mut agent_ids = crate::platforms::universal_agents_skills_agent_ids();
        agent_ids.extend(extra.iter().map(|agent_id| (*agent_id).to_string()));
        agent_ids.sort();
        agent_ids.dedup();
        agent_ids
    }

    fn make_skill(id: &str, name: &str, is_central: bool) -> Skill {
        Skill {
            id: id.to_string(),
            name: name.to_string(),
            description: Some(format!("Desc for {}", name)),
            file_path: format!("/tmp/{}/SKILL.md", id),
            canonical_path: if is_central {
                Some(format!("/tmp/central/{}", id))
            } else {
                None
            },
            is_central,
            source: if is_central {
                Some("native".to_string())
            } else {
                Some("copy".to_string())
            },
            content: None,
            scanned_at: Utc::now().to_rfc3339(),
        }
    }

    fn make_skill_with_path(
        id: &str,
        name: &str,
        file_path: &Path,
        canonical_path: &Path,
        is_central: bool,
    ) -> Skill {
        Skill {
            id: id.to_string(),
            name: name.to_string(),
            description: Some(format!("Desc for {}", name)),
            file_path: file_path.to_string_lossy().into_owned(),
            canonical_path: Some(canonical_path.to_string_lossy().into_owned()),
            is_central,
            source: Some(if is_central { "native" } else { "resource" }.to_string()),
            content: None,
            scanned_at: Utc::now().to_rfc3339(),
        }
    }

    fn make_observation_for_agent(
        agent_id: &str,
        row_id: &str,
        skill_id: &str,
        name: &str,
        dir_path: &str,
        source_kind: &str,
        source_root: &str,
        read_only: bool,
    ) -> AgentSkillObservation {
        AgentSkillObservation {
            row_id: row_id.to_string(),
            agent_id: agent_id.to_string(),
            skill_id: skill_id.to_string(),
            name: name.to_string(),
            description: Some(format!("{source_kind} copy")),
            file_path: format!("{dir_path}/SKILL.md"),
            dir_path: dir_path.to_string(),
            source_kind: source_kind.to_string(),
            source_root: source_root.to_string(),
            link_type: "copy".to_string(),
            symlink_target: None,
            is_read_only: read_only,
            scanned_at: Utc::now().to_rfc3339(),
        }
    }

    async fn set_agent_dir(pool: &SqlitePool, agent_id: &str, dir: &Path) {
        sqlx::query("UPDATE agents SET global_skills_dir = ? WHERE id = ?")
            .bind(dir.to_string_lossy().into_owned())
            .bind(agent_id)
            .execute(pool)
            .await
            .unwrap();
    }

    async fn create_central_skill(pool: &SqlitePool, central_dir: &Path, skill_id: &str) -> Skill {
        let skill_dir = central_dir.join(skill_id);
        fs::create_dir_all(&skill_dir).unwrap();
        let skill_md_path = skill_dir.join("SKILL.md");
        fs::write(
            &skill_md_path,
            format!("---\nname: {skill_id}\ndescription: Test skill\n---\n\n# {skill_id}\n"),
        )
        .unwrap();

        let skill = Skill {
            id: skill_id.to_string(),
            name: skill_id.to_string(),
            description: Some("Test skill".to_string()),
            file_path: skill_md_path.to_string_lossy().into_owned(),
            canonical_path: Some(skill_dir.to_string_lossy().into_owned()),
            is_central: true,
            source: Some("native".to_string()),
            content: None,
            scanned_at: Utc::now().to_rfc3339(),
        };
        db::upsert_skill(pool, &skill).await.unwrap();
        skill
    }

    async fn create_nested_central_skill(
        pool: &SqlitePool,
        bundle_dir: &Path,
        skill_id: &str,
    ) -> Skill {
        let skill_dir = bundle_dir.join(skill_id);
        fs::create_dir_all(&skill_dir).unwrap();
        let skill_md_path = skill_dir.join("SKILL.md");
        fs::write(
            &skill_md_path,
            format!("---\nname: {skill_id}\ndescription: Nested test skill\n---\n\n# {skill_id}\n"),
        )
        .unwrap();

        let skill = Skill {
            id: skill_id.to_string(),
            name: skill_id.to_string(),
            description: Some("Nested test skill".to_string()),
            file_path: skill_md_path.to_string_lossy().into_owned(),
            canonical_path: Some(skill_dir.to_string_lossy().into_owned()),
            is_central: true,
            source: Some("native".to_string()),
            content: None,
            scanned_at: Utc::now().to_rfc3339(),
        };
        db::upsert_skill(pool, &skill).await.unwrap();
        skill
    }

    async fn create_nested_resource_skill(
        pool: &SqlitePool,
        bundle_dir: &Path,
        skill_id: &str,
    ) -> Skill {
        let skill_dir = bundle_dir.join(skill_id);
        fs::create_dir_all(&skill_dir).unwrap();
        let skill_md_path = skill_dir.join("SKILL.md");
        fs::write(
            &skill_md_path,
            format!(
                "---\nname: {skill_id}\ndescription: Resource test skill\n---\n\n# {skill_id}\n"
            ),
        )
        .unwrap();

        let skill = Skill {
            id: skill_id.to_string(),
            name: skill_id.to_string(),
            description: Some("Resource test skill".to_string()),
            file_path: skill_md_path.to_string_lossy().into_owned(),
            canonical_path: Some(skill_dir.to_string_lossy().into_owned()),
            is_central: false,
            source: Some("resource-library".to_string()),
            content: None,
            scanned_at: Utc::now().to_rfc3339(),
        };
        db::upsert_skill(pool, &skill).await.unwrap();
        skill
    }

    #[tokio::test]
    async fn test_get_resource_library_skills_impl_indexes_skills_from_disk() {
        let pool = setup_test_db().await;
        let tmp = TempDir::new().unwrap();
        let resource_root = tmp.path().join("resource-library");
        let skill_dir = resource_root
            .join("author")
            .join("repo")
            .join("manual-skill");
        fs::create_dir_all(&skill_dir).unwrap();
        fs::write(
            skill_dir.join("SKILL.md"),
            "---\nname: Manual Skill\ndescription: Imported on disk only\n---\n\n# Manual Skill\n",
        )
        .unwrap();
        db::set_skill_resource_library_dir(&pool, &resource_root.to_string_lossy())
            .await
            .unwrap();

        let skills = get_resource_library_skills_impl(&pool).await.unwrap();

        let skill = skills
            .iter()
            .find(|skill| skill.id == "manual-skill")
            .expect("disk-only resource skill should be indexed");
        assert_eq!(skill.name, "Manual Skill");
        assert_eq!(skill.description.as_deref(), Some("Imported on disk only"));
        assert!(!skill.is_central);
        assert_eq!(skill.source.as_deref(), Some("github:author/repo"));
        assert_eq!(skill.source_author.as_deref(), Some("author"));
        assert_eq!(skill.source_repo.as_deref(), Some("author/repo"));
        assert_eq!(skill.source_path.as_deref(), Some("manual-skill/SKILL.md"));
        assert_eq!(skill.source_url.as_deref(), None);
        assert_eq!(
            skill.canonical_path.as_deref(),
            Some(skill_dir.to_string_lossy().as_ref())
        );
    }

    #[tokio::test]
    async fn test_get_resource_library_skills_impl_infers_root_repo_source_path() {
        let pool = setup_test_db().await;
        let tmp = TempDir::new().unwrap();
        let resource_root = tmp.path().join("resource-library");
        let skill_dir = resource_root
            .join("op7418")
            .join("guizang-ppt-skill")
            .join("guizang-ppt-skill");
        fs::create_dir_all(&skill_dir).unwrap();
        fs::write(
            skill_dir.join("SKILL.md"),
            "---\nname: guizang-ppt-skill\ndescription: Root repo skill\n---\n\n# Guizang PPT\n",
        )
        .unwrap();
        db::set_skill_resource_library_dir(&pool, &resource_root.to_string_lossy())
            .await
            .unwrap();

        let skills = get_resource_library_skills_impl(&pool).await.unwrap();

        let skill = skills
            .iter()
            .find(|skill| skill.id == "guizang-ppt-skill")
            .expect("root repo skill should be indexed");
        assert_eq!(
            skill.source.as_deref(),
            Some("github:op7418/guizang-ppt-skill")
        );
        assert_eq!(skill.source_path.as_deref(), Some("SKILL.md"));
        let source = db::get_skill_source(&pool, "guizang-ppt-skill")
            .await
            .unwrap()
            .expect("source metadata should be inferred");
        assert_eq!(
            source.source_repo.as_deref(),
            Some("op7418/guizang-ppt-skill")
        );
        assert_eq!(source.source_path.as_deref(), Some("SKILL.md"));
    }

    #[tokio::test]
    async fn test_get_resource_library_skills_impl_preserves_import_id_when_frontmatter_name_differs(
    ) {
        let pool = setup_test_db().await;
        let tmp = TempDir::new().unwrap();
        let resource_root = tmp.path().join("resource-library");
        let skill_dir = resource_root
            .join("Agents365-ai")
            .join("365-skills")
            .join("excalidraw-skill");
        fs::create_dir_all(&skill_dir).unwrap();
        let skill_md_path = skill_dir.join("SKILL.md");
        fs::write(
            &skill_md_path,
            "---\nname: excalidraw\ndescription: Excalidraw plugin skill\n---\n\n# Excalidraw\n",
        )
        .unwrap();
        db::set_skill_resource_library_dir(&pool, &resource_root.to_string_lossy())
            .await
            .unwrap();
        let colliding_dir = resource_root.join("other").join("repo").join("excalidraw");
        fs::create_dir_all(&colliding_dir).unwrap();
        fs::write(
            colliding_dir.join("SKILL.md"),
            "---\nname: excalidraw\ndescription: Different repository skill\n---\n\n# Excalidraw\n",
        )
        .unwrap();
        db::upsert_skill(
            &pool,
            &Skill {
                id: "excalidraw".to_string(),
                name: "excalidraw".to_string(),
                description: Some("Different repository skill".to_string()),
                file_path: colliding_dir
                    .join("SKILL.md")
                    .to_string_lossy()
                    .into_owned(),
                canonical_path: Some(colliding_dir.to_string_lossy().into_owned()),
                is_central: false,
                source: Some("github:other/repo".to_string()),
                content: None,
                scanned_at: Utc::now().to_rfc3339(),
            },
        )
        .await
        .unwrap();
        db::upsert_skill_source(
            &pool,
            &db::SkillSource {
                skill_id: "excalidraw".to_string(),
                source_type: "github".to_string(),
                source_url: None,
                source_author: Some("other".to_string()),
                source_repo: Some("other/repo".to_string()),
                source_path: Some("excalidraw/SKILL.md".to_string()),
                updated_at: Utc::now().to_rfc3339(),
            },
        )
        .await
        .unwrap();

        db::upsert_skill(
            &pool,
            &Skill {
                id: "excalidraw-skill".to_string(),
                name: "excalidraw".to_string(),
                description: Some("Excalidraw plugin skill".to_string()),
                file_path: skill_md_path.to_string_lossy().into_owned(),
                canonical_path: Some(skill_dir.to_string_lossy().into_owned()),
                is_central: false,
                source: Some("github:Agents365-ai/365-skills".to_string()),
                content: None,
                scanned_at: Utc::now().to_rfc3339(),
            },
        )
        .await
        .unwrap();
        db::upsert_skill_source(
            &pool,
            &db::SkillSource {
                skill_id: "excalidraw-skill".to_string(),
                source_type: "github".to_string(),
                source_url: None,
                source_author: Some("Agents365-ai".to_string()),
                source_repo: Some("Agents365-ai/365-skills".to_string()),
                source_path: Some(
                    "plugins/excalidraw/skills/excalidraw-skill/SKILL.md".to_string(),
                ),
                updated_at: Utc::now().to_rfc3339(),
            },
        )
        .await
        .unwrap();

        let skills = get_resource_library_skills_impl(&pool).await.unwrap();

        assert_eq!(skills.len(), 2);
        let skill = skills
            .iter()
            .find(|skill| skill.id == "excalidraw-skill")
            .expect("import id should be preserved");
        assert_eq!(skill.id, "excalidraw-skill");
        assert_eq!(skill.name, "excalidraw");
        assert_eq!(
            skill.source_repo.as_deref(),
            Some("Agents365-ai/365-skills")
        );
        assert_eq!(
            skill.source_path.as_deref(),
            Some("plugins/excalidraw/skills/excalidraw-skill/SKILL.md")
        );
        let colliding = skills
            .iter()
            .find(|skill| skill.id == "excalidraw")
            .expect("colliding skill should remain separate");
        assert_eq!(colliding.source_repo.as_deref(), Some("other/repo"));
    }

    #[tokio::test]
    async fn test_get_resource_library_skills_impl_scopes_same_id_from_different_repo_paths() {
        let pool = setup_test_db().await;
        let tmp = TempDir::new().unwrap();
        let resource_root = tmp.path().join("resource-library");
        let first_dir = resource_root
            .join("other-owner")
            .join("other-repo")
            .join("assetseeker");
        let second_dir = resource_root
            .join("Agents365-ai")
            .join("365-skills")
            .join("assetseeker");
        for (dir, description) in [
            (&first_dir, "First assetseeker"),
            (&second_dir, "Second assetseeker"),
        ] {
            fs::create_dir_all(dir).unwrap();
            let content = if dir == &second_dir {
                format!(
                    "---\nname: assetseeker\ndescription: {description}\ncompatibility: Optional: install extra packages\n---\n\n# Assetseeker\n"
                )
            } else {
                format!(
                    "---\nname: assetseeker\ndescription: {description}\n---\n\n# Assetseeker\n"
                )
            };
            fs::write(dir.join("SKILL.md"), content).unwrap();
        }
        db::set_skill_resource_library_dir(&pool, &resource_root.to_string_lossy())
            .await
            .unwrap();
        db::upsert_skill(
            &pool,
            &Skill {
                id: "assetseeker".to_string(),
                name: "assetseeker".to_string(),
                description: Some("First assetseeker".to_string()),
                file_path: first_dir.join("SKILL.md").to_string_lossy().into_owned(),
                canonical_path: Some(first_dir.to_string_lossy().into_owned()),
                is_central: false,
                source: Some("github:other-owner/other-repo".to_string()),
                content: None,
                scanned_at: Utc::now().to_rfc3339(),
            },
        )
        .await
        .unwrap();

        let skills = get_resource_library_skills_impl(&pool).await.unwrap();

        assert_eq!(skills.len(), 2);
        assert!(skills.iter().any(|skill| {
            skill.id == "assetseeker"
                && skill.canonical_path.as_deref() == Some(first_dir.to_string_lossy().as_ref())
        }));
        assert!(skills.iter().any(|skill| {
            skill.id == "agents365-ai-365-skills-assetseeker"
                && skill.canonical_path.as_deref() == Some(second_dir.to_string_lossy().as_ref())
        }));
    }

    #[tokio::test]
    async fn test_get_skill_detail_indexes_resource_skill_from_disk_when_db_is_stale() {
        let pool = setup_test_db().await;
        let tmp = TempDir::new().unwrap();
        let resource_root = tmp.path().join("resource-library");
        let skill_dir = resource_root
            .join("author")
            .join("repo")
            .join("academic-paper");
        fs::create_dir_all(&skill_dir).unwrap();
        fs::write(
            skill_dir.join("SKILL.md"),
            "---\nname: academic-paper\ndescription: Indexed lazily from disk\n---\n\n# Academic Paper\n",
        )
        .unwrap();
        db::set_skill_resource_library_dir(&pool, &resource_root.to_string_lossy())
            .await
            .unwrap();

        let detail = get_skill_detail_with_row_impl(&pool, "academic-paper", None, None)
            .await
            .expect("detail lookup should repair stale resource library index");

        assert_eq!(detail.id, "academic-paper");
        assert_eq!(
            detail.description.as_deref(),
            Some("Indexed lazily from disk")
        );
        assert_eq!(
            detail.file_path,
            skill_dir.join("SKILL.md").to_string_lossy().into_owned()
        );
        assert_eq!(detail.source_repo.as_deref(), Some("author/repo"));
    }

    #[tokio::test]
    async fn test_resource_scan_preserves_real_central_copy_and_resource_listing() {
        let pool = setup_test_db().await;
        let tmp = TempDir::new().unwrap();
        let resource_root = tmp.path().join("resource-library");
        let central_root = tmp.path().join("central");
        let resource_skill_dir = resource_root.join("shared-skill");
        let central_skill_dir = central_root.join("shared-skill");
        fs::create_dir_all(&resource_skill_dir).unwrap();
        fs::create_dir_all(&central_skill_dir).unwrap();
        fs::write(
            resource_skill_dir.join("SKILL.md"),
            "---\nname: Shared Skill\ndescription: Resource copy\n---\n",
        )
        .unwrap();
        fs::write(
            central_skill_dir.join("SKILL.md"),
            "---\nname: Shared Skill\ndescription: Central copy\n---\n",
        )
        .unwrap();
        db::set_skill_resource_library_dir(&pool, &resource_root.to_string_lossy())
            .await
            .unwrap();
        set_agent_dir(&pool, "central", &central_root).await;
        db::upsert_skill(
            &pool,
            &Skill {
                id: "shared-skill".to_string(),
                name: "Shared Skill".to_string(),
                description: Some("Central copy".to_string()),
                file_path: central_skill_dir
                    .join("SKILL.md")
                    .to_string_lossy()
                    .into_owned(),
                canonical_path: Some(central_skill_dir.to_string_lossy().into_owned()),
                is_central: true,
                source: Some("resource-library".to_string()),
                content: None,
                scanned_at: Utc::now().to_rfc3339(),
            },
        )
        .await
        .unwrap();

        let resources = get_resource_library_skills_impl(&pool).await.unwrap();

        assert!(resources.iter().any(|skill| {
            skill.id == "shared-skill"
                && skill.canonical_path.as_deref()
                    == Some(resource_skill_dir.to_string_lossy().as_ref())
        }));
        let persisted = db::get_skill_by_id(&pool, "shared-skill")
            .await
            .unwrap()
            .unwrap();
        assert!(persisted.is_central);
        assert_eq!(
            persisted.canonical_path.as_deref(),
            Some(central_skill_dir.to_string_lossy().as_ref())
        );
    }

    #[tokio::test]
    async fn test_resource_scan_preserves_central_symlink_and_resource_listing() {
        let pool = setup_test_db().await;
        let tmp = TempDir::new().unwrap();
        let resource_root = tmp.path().join("resource-library");
        let central_root = tmp.path().join("central");
        let resource_skill_dir = resource_root.join("shared-skill");
        let central_skill_dir = central_root.join("shared-skill");
        fs::create_dir_all(&resource_skill_dir).unwrap();
        fs::create_dir_all(&central_root).unwrap();
        fs::write(
            resource_skill_dir.join("SKILL.md"),
            "---\nname: Shared Skill\ndescription: Resource skill\n---\n",
        )
        .unwrap();
        create_symlink(&resource_skill_dir, &central_skill_dir).unwrap();
        db::set_skill_resource_library_dir(&pool, &resource_root.to_string_lossy())
            .await
            .unwrap();
        set_agent_dir(&pool, "central", &central_root).await;
        db::upsert_skill(
            &pool,
            &Skill {
                id: "shared-skill".to_string(),
                name: "Shared Skill".to_string(),
                description: Some("Resource skill".to_string()),
                file_path: central_skill_dir
                    .join("SKILL.md")
                    .to_string_lossy()
                    .into_owned(),
                canonical_path: Some(central_skill_dir.to_string_lossy().into_owned()),
                is_central: true,
                source: Some("resource-library".to_string()),
                content: None,
                scanned_at: Utc::now().to_rfc3339(),
            },
        )
        .await
        .unwrap();
        // Full-disk scan writes this unmanaged row for the Central Skills agent.
        // Repair must not treat it as a leftover platform install in ~/.agents/skills.
        sqlx::query(
            "INSERT INTO skill_installations
             (skill_id, agent_id, installed_path, link_type, symlink_target, is_managed, created_at)
             VALUES (?, 'central', ?, 'symlink', ?, 0, ?)",
        )
        .bind("shared-skill")
        .bind(central_skill_dir.to_string_lossy().into_owned())
        .bind(resource_skill_dir.to_string_lossy().into_owned())
        .bind(Utc::now().to_rfc3339())
        .execute(&pool)
        .await
        .unwrap();

        let resources = get_resource_library_skills_impl(&pool).await.unwrap();

        assert!(resources.iter().any(|skill| {
            skill.id == "shared-skill"
                && skill.is_central
                && skill.canonical_path.as_deref()
                    == Some(resource_skill_dir.to_string_lossy().as_ref())
        }));
        let persisted = db::get_skill_by_id(&pool, "shared-skill")
            .await
            .unwrap()
            .unwrap();
        assert!(persisted.is_central);
        assert!(fs::symlink_metadata(&central_skill_dir)
            .unwrap()
            .file_type()
            .is_symlink());
        assert_eq!(
            persisted.canonical_path.as_deref(),
            Some(central_skill_dir.to_string_lossy().as_ref())
        );
        assert!(
            db::get_skill_installations_for_legacy_repair(&pool, "shared-skill")
                .await
                .unwrap()
                .iter()
                .any(|installation| installation.agent_id == "central"),
            "central scan installation must remain"
        );
    }

    #[tokio::test]
    async fn test_resource_scan_preserves_central_symlink_for_shared_root_platform() {
        let pool = setup_test_db().await;
        let tmp = TempDir::new().unwrap();
        let resource_root = tmp.path().join("resource-library");
        let central_root = tmp.path().join("central");
        let resource_skill_dir = resource_root.join("shared-skill");
        let central_skill_dir = central_root.join("shared-skill");
        fs::create_dir_all(&resource_skill_dir).unwrap();
        fs::create_dir_all(&central_root).unwrap();
        fs::write(
            resource_skill_dir.join("SKILL.md"),
            "---\nname: Shared Skill\ndescription: Resource skill\n---\n",
        )
        .unwrap();
        create_symlink(&resource_skill_dir, &central_skill_dir).unwrap();
        db::set_skill_resource_library_dir(&pool, &resource_root.to_string_lossy())
            .await
            .unwrap();
        set_agent_dir(&pool, "central", &central_root).await;
        set_agent_dir(&pool, "antigravity", &central_root).await;
        db::upsert_skill(
            &pool,
            &Skill {
                id: "shared-skill".to_string(),
                name: "Shared Skill".to_string(),
                description: Some("Resource skill".to_string()),
                file_path: central_skill_dir
                    .join("SKILL.md")
                    .to_string_lossy()
                    .into_owned(),
                canonical_path: Some(central_skill_dir.to_string_lossy().into_owned()),
                is_central: true,
                source: Some("resource-library".to_string()),
                content: None,
                scanned_at: Utc::now().to_rfc3339(),
            },
        )
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO skill_installations
             (skill_id, agent_id, installed_path, link_type, symlink_target, is_managed, created_at)
             VALUES (?, 'antigravity', ?, 'symlink', ?, 0, ?)",
        )
        .bind("shared-skill")
        .bind(central_skill_dir.to_string_lossy().into_owned())
        .bind(resource_skill_dir.to_string_lossy().into_owned())
        .bind(Utc::now().to_rfc3339())
        .execute(&pool)
        .await
        .unwrap();

        get_resource_library_skills_impl(&pool).await.unwrap();

        let persisted = db::get_skill_by_id(&pool, "shared-skill")
            .await
            .unwrap()
            .unwrap();
        assert!(persisted.is_central);
        assert!(fs::symlink_metadata(&central_skill_dir)
            .unwrap()
            .file_type()
            .is_symlink());
    }

    #[tokio::test]
    async fn test_resource_scan_repairs_legacy_platform_link_created_in_central() {
        let pool = setup_test_db().await;
        let tmp = TempDir::new().unwrap();
        let resource_root = tmp.path().join("resource-library");
        let central_root = tmp.path().join("central");
        let resource_skill_dir = resource_root.join("legacy-skill");
        let central_link = central_root.join("legacy-skill");
        fs::create_dir_all(&resource_skill_dir).unwrap();
        fs::create_dir_all(&central_root).unwrap();
        fs::write(
            resource_skill_dir.join("SKILL.md"),
            "---\nname: Legacy Skill\ndescription: Resource skill\n---\n",
        )
        .unwrap();
        db::set_skill_resource_library_dir(&pool, &resource_root.to_string_lossy())
            .await
            .unwrap();
        set_agent_dir(&pool, "central", &central_root).await;

        create_symlink(&resource_skill_dir, &central_link).unwrap();
        db::upsert_skill(
            &pool,
            &Skill {
                id: "legacy-skill".to_string(),
                name: "Legacy Skill".to_string(),
                description: Some("Resource skill".to_string()),
                file_path: central_link.join("SKILL.md").to_string_lossy().into_owned(),
                canonical_path: Some(central_link.to_string_lossy().into_owned()),
                is_central: true,
                source: Some("resource-library".to_string()),
                content: None,
                scanned_at: Utc::now().to_rfc3339(),
            },
        )
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO skill_installations
             (skill_id, agent_id, installed_path, link_type, symlink_target, is_managed, created_at)
             VALUES (?, 'codex', ?, 'symlink', ?, 0, ?)",
        )
        .bind("legacy-skill")
        .bind(central_link.to_string_lossy().into_owned())
        .bind(resource_skill_dir.to_string_lossy().into_owned())
        .bind(Utc::now().to_rfc3339())
        .execute(&pool)
        .await
        .unwrap();

        db::delete_stale_skill_installations(&pool, "codex", &[])
            .await
            .unwrap();

        let skills = get_resource_library_skills_impl(&pool).await.unwrap();

        assert!(skills.iter().any(|skill| skill.id == "legacy-skill"));
        assert!(
            fs::symlink_metadata(&central_link).is_err(),
            "legacy platform link must be removed from the central library"
        );
        let repaired = db::get_skill_by_id(&pool, "legacy-skill")
            .await
            .unwrap()
            .unwrap();
        assert!(!repaired.is_central);
        assert_eq!(
            repaired.canonical_path.as_deref(),
            Some(resource_skill_dir.to_string_lossy().as_ref())
        );
        assert!(
            db::get_skill_installations(&pool, "legacy-skill")
                .await
                .unwrap()
                .is_empty(),
            "obsolete installation record must be removed"
        );
    }

    #[tokio::test]
    async fn test_get_resource_library_skills_impl_preserves_existing_source() {
        let pool = setup_test_db().await;
        let tmp = TempDir::new().unwrap();
        let resource_root = tmp.path().join("resource-library");
        let skill_dir = resource_root
            .join("author")
            .join("repo")
            .join("source-skill");
        let skill_md_path = skill_dir.join("SKILL.md");
        fs::create_dir_all(&skill_dir).unwrap();
        fs::write(
            &skill_md_path,
            "---\nname: Source Skill\ndescription: Existing source\n---\n\n# Source Skill\n",
        )
        .unwrap();
        db::set_skill_resource_library_dir(&pool, &resource_root.to_string_lossy())
            .await
            .unwrap();
        db::upsert_skill(
            &pool,
            &Skill {
                id: "source-skill".to_string(),
                name: "Source Skill".to_string(),
                description: Some("Existing source".to_string()),
                file_path: skill_md_path.to_string_lossy().into_owned(),
                canonical_path: Some(skill_dir.to_string_lossy().into_owned()),
                is_central: false,
                source: Some("github:owner/repo".to_string()),
                content: None,
                scanned_at: Utc::now().to_rfc3339(),
            },
        )
        .await
        .unwrap();

        let skills = get_resource_library_skills_impl(&pool).await.unwrap();

        let skill = skills
            .iter()
            .find(|skill| skill.id == "source-skill")
            .expect("resource skill should be returned");
        assert_eq!(skill.source.as_deref(), Some("github:owner/repo"));
    }

    #[tokio::test]
    async fn test_get_resource_library_skills_impl_restores_github_source_from_metadata() {
        let pool = setup_test_db().await;
        let tmp = TempDir::new().unwrap();
        let resource_root = tmp.path().join("resource-library");
        let skill_dir = resource_root
            .join("owner")
            .join("repo")
            .join("source-skill");
        let skill_md_path = skill_dir.join("SKILL.md");
        fs::create_dir_all(&skill_dir).unwrap();
        fs::write(
            &skill_md_path,
            "---\nname: Source Skill\ndescription: Existing source\n---\n\n# Source Skill\n",
        )
        .unwrap();
        db::set_skill_resource_library_dir(&pool, &resource_root.to_string_lossy())
            .await
            .unwrap();
        db::upsert_skill(
            &pool,
            &Skill {
                id: "source-skill".to_string(),
                name: "Source Skill".to_string(),
                description: Some("Existing source".to_string()),
                file_path: skill_md_path.to_string_lossy().into_owned(),
                canonical_path: Some(skill_dir.to_string_lossy().into_owned()),
                is_central: false,
                source: Some("resource-library".to_string()),
                content: None,
                scanned_at: Utc::now().to_rfc3339(),
            },
        )
        .await
        .unwrap();
        db::upsert_skill_source(
            &pool,
            &db::SkillSource {
                skill_id: "source-skill".to_string(),
                source_type: "github".to_string(),
                source_url: None,
                source_author: Some("owner".to_string()),
                source_repo: Some("owner/repo".to_string()),
                source_path: Some("skills/source-skill/SKILL.md".to_string()),
                updated_at: Utc::now().to_rfc3339(),
            },
        )
        .await
        .unwrap();

        let skills = get_resource_library_skills_impl(&pool).await.unwrap();

        let skill = skills
            .iter()
            .find(|skill| skill.id == "source-skill")
            .expect("resource skill should be returned");
        assert_eq!(skill.source.as_deref(), Some("github:owner/repo"));
        let saved = db::get_skill_by_id(&pool, "source-skill")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(saved.source.as_deref(), Some("github:owner/repo"));
    }

    #[tokio::test]
    async fn test_resource_scan_clears_repository_homepage_update_url() {
        let pool = setup_test_db().await;
        let tmp = TempDir::new().unwrap();
        let resource_root = tmp.path().join("resource-library");
        let skill_dir = resource_root
            .join("owner")
            .join("repo")
            .join("source-skill");
        let skill_md_path = skill_dir.join("SKILL.md");
        fs::create_dir_all(&skill_dir).unwrap();
        fs::write(&skill_md_path, "---\nname: Source Skill\n---\n").unwrap();
        db::set_skill_resource_library_dir(&pool, &resource_root.to_string_lossy())
            .await
            .unwrap();
        db::upsert_skill(
            &pool,
            &Skill {
                id: "source-skill".to_string(),
                name: "Source Skill".to_string(),
                description: None,
                file_path: skill_md_path.to_string_lossy().into_owned(),
                canonical_path: Some(skill_dir.to_string_lossy().into_owned()),
                is_central: false,
                source: Some("resource-library".to_string()),
                content: None,
                scanned_at: Utc::now().to_rfc3339(),
            },
        )
        .await
        .unwrap();
        db::upsert_skill_source(
            &pool,
            &db::SkillSource {
                skill_id: "source-skill".to_string(),
                source_type: "github".to_string(),
                source_url: Some("https://github.com/owner/repo".to_string()),
                source_author: Some("owner".to_string()),
                source_repo: Some("owner/repo".to_string()),
                source_path: Some("source-skill/SKILL.md".to_string()),
                updated_at: Utc::now().to_rfc3339(),
            },
        )
        .await
        .unwrap();

        let skills = get_resource_library_skills_impl(&pool).await.unwrap();

        let skill = skills
            .iter()
            .find(|skill| skill.id == "source-skill")
            .expect("resource skill should be returned");
        assert_eq!(skill.source.as_deref(), Some("github:owner/repo"));
        assert_eq!(skill.source_url.as_deref(), None);
        let source = db::get_skill_source(&pool, "source-skill")
            .await
            .unwrap()
            .expect("source metadata should remain");
        assert_eq!(source.source_url.as_deref(), None);
    }

    #[tokio::test]
    async fn test_update_resource_skill_source_metadata_saves_github_repo_as_source() {
        let pool = setup_test_db().await;
        let tmp = TempDir::new().unwrap();
        let resource_root = tmp.path().join("resource-library");
        let skill_dir = resource_root.join("manual-skill");
        let skill_md_path = skill_dir.join("SKILL.md");
        fs::create_dir_all(&skill_dir).unwrap();
        fs::write(&skill_md_path, "---\nname: Manual Skill\n---\n").unwrap();
        db::set_skill_resource_library_dir(&pool, &resource_root.to_string_lossy())
            .await
            .unwrap();
        db::upsert_skill(
            &pool,
            &Skill {
                id: "manual-skill".to_string(),
                name: "Manual Skill".to_string(),
                description: None,
                file_path: skill_md_path.to_string_lossy().into_owned(),
                canonical_path: Some(skill_dir.to_string_lossy().into_owned()),
                is_central: false,
                source: Some("resource-library".to_string()),
                content: None,
                scanned_at: Utc::now().to_rfc3339(),
            },
        )
        .await
        .unwrap();

        let saved = update_resource_skill_source_metadata_impl(
            &pool,
            "manual-skill",
            UpdateSkillSourceMetadataRequest {
                source_type: "github".to_string(),
                source_url: Some("https://github.com/example/manual-skills".to_string()),
                source_author: None,
                source_repo: Some(" example/manual-skills ".to_string()),
                source_path: Some("skills/manual-skill/SKILL.md".to_string()),
            },
        )
        .await
        .unwrap();

        assert_eq!(
            saved.source.as_deref(),
            Some("github:example/manual-skills")
        );
        assert_eq!(saved.source_repo.as_deref(), Some("example/manual-skills"));
        assert_eq!(saved.source_author.as_deref(), Some("example"));
        let skill = db::get_skill_by_id(&pool, "manual-skill")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            skill.source.as_deref(),
            Some("github:example/manual-skills")
        );
    }

    // ── get_skills_by_agent ───────────────────────────────────────────────────

    #[tokio::test]
    async fn test_get_skills_by_agent_returns_correct_skills() {
        let pool = setup_test_db().await;

        let skill_a = make_skill("skill-a", "Skill A", false);
        let skill_b = make_skill("skill-b", "Skill B", false);
        db::upsert_skill(&pool, &skill_a).await.unwrap();
        db::upsert_skill(&pool, &skill_b).await.unwrap();

        db::upsert_skill_installation(
            &pool,
            &SkillInstallation {
                skill_id: "skill-a".to_string(),
                agent_id: "claude-code".to_string(),
                installed_path: "/tmp/claude/skill-a/SKILL.md".to_string(),
                link_type: "symlink".to_string(),
                symlink_target: Some("/tmp/central/skill-a".to_string()),
                created_at: Utc::now().to_rfc3339(),
            },
        )
        .await
        .unwrap();

        let skills = db::get_skills_by_agent(&pool, "claude-code").await.unwrap();
        assert_eq!(skills.len(), 1);
        assert_eq!(skills[0].id, "skill-a");
    }

    #[tokio::test]
    async fn test_get_skills_by_agent_empty_for_unknown_agent() {
        let pool = setup_test_db().await;
        let skills = db::get_skills_by_agent(&pool, "nonexistent-agent")
            .await
            .unwrap();
        assert!(skills.is_empty());
    }

    // ── get_central_skills ────────────────────────────────────────────────────

    #[tokio::test]
    async fn test_get_central_skills_includes_linked_agents() {
        let pool = setup_test_db().await;

        let central_skill = make_skill("central-a", "Central A", true);
        db::upsert_skill(&pool, &central_skill).await.unwrap();

        // Install to claude-code and cursor.
        for agent_id in &["claude-code", "cursor"] {
            db::upsert_skill_installation(
                &pool,
                &SkillInstallation {
                    skill_id: "central-a".to_string(),
                    agent_id: agent_id.to_string(),
                    installed_path: format!("/tmp/{}/central-a/SKILL.md", agent_id),
                    link_type: "symlink".to_string(),
                    symlink_target: Some("/tmp/central/central-a".to_string()),
                    created_at: Utc::now().to_rfc3339(),
                },
            )
            .await
            .unwrap();
        }

        let skills_with_links = get_central_skills_impl(&pool).await.unwrap();
        assert_eq!(skills_with_links.len(), 1);

        let mut linked = skills_with_links[0].linked_agents.clone();
        linked.sort();
        assert_eq!(linked, vec!["claude-code", "cursor"]);
    }

    #[tokio::test]
    async fn test_get_central_skills_no_links() {
        let pool = setup_test_db().await;

        let central_skill = make_skill("central-solo", "Solo Central", true);
        db::upsert_skill(&pool, &central_skill).await.unwrap();

        let skills_with_links = get_central_skills_impl(&pool).await.unwrap();
        assert_eq!(skills_with_links.len(), 1);
        assert!(
            skills_with_links[0].linked_agents.is_empty(),
            "no links when no installations"
        );
    }

    #[tokio::test]
    async fn test_get_central_skills_ignores_compatibility_observations() {
        let pool = setup_test_db().await;

        let central_skill = make_skill("shared-skill", "Shared Skill", true);
        db::upsert_skill(&pool, &central_skill).await.unwrap();
        db::upsert_agent_skill_observation(
            &pool,
            &make_observation_for_agent(
                "factory-droid",
                "factory-droid::/tmp/.agents/skills/shared-skill",
                "shared-skill",
                "Shared Skill",
                "/tmp/.agents/skills/shared-skill",
                "compatibility",
                "/tmp/.agents/skills",
                true,
            ),
        )
        .await
        .unwrap();

        let skills_with_links = get_central_skills_impl(&pool).await.unwrap();
        assert_eq!(skills_with_links.len(), 1);
        assert!(
            skills_with_links[0].linked_agents.is_empty(),
            "compatibility observations must not pollute linked_agents state"
        );
    }

    #[tokio::test]
    async fn test_unmanaged_installation_is_not_exposed_as_managed_status() {
        let pool = setup_test_db().await;
        let central_skill = make_skill("external-skill", "External Skill", true);
        db::upsert_skill(&pool, &central_skill).await.unwrap();
        sqlx::query(
            "INSERT INTO skill_installations
             (skill_id, agent_id, installed_path, link_type, symlink_target, is_managed, created_at)
             VALUES (?, ?, ?, ?, ?, 0, ?)",
        )
        .bind("external-skill")
        .bind("claude-code")
        .bind("/tmp/.claude/skills/external-skill")
        .bind("symlink")
        .bind("/tmp/outside/external-skill")
        .bind("2026-01-01T00:00:00Z")
        .execute(&pool)
        .await
        .unwrap();

        let central_skills = get_central_skills_impl(&pool).await.unwrap();
        let detail = get_skill_detail_impl(&pool, "external-skill")
            .await
            .unwrap();

        assert!(central_skills[0].linked_agents.is_empty());
        assert!(detail.installations.is_empty());
    }

    #[tokio::test]
    async fn test_get_central_skills_reports_factory_compatibility_as_read_only_agent() {
        let pool = setup_test_db().await;

        let central_skill = make_skill("shared-skill", "Shared Skill", true);
        db::upsert_skill(&pool, &central_skill).await.unwrap();
        db::upsert_agent_skill_observation(
            &pool,
            &make_observation_for_agent(
                "factory-droid",
                "factory-droid::/tmp/.agents/skills/shared-skill",
                "shared-skill",
                "Shared Skill",
                "/tmp/.agents/skills/shared-skill",
                "compatibility",
                "/tmp/.agents/skills",
                true,
            ),
        )
        .await
        .unwrap();

        let skills_with_links = get_central_skills_impl(&pool).await.unwrap();
        assert_eq!(skills_with_links.len(), 1);
        assert!(
            skills_with_links[0].linked_agents.is_empty(),
            "read-only compatibility observations are not removable installation links"
        );
        assert_eq!(
            skills_with_links[0].read_only_agents,
            expected_universal_read_only_agents(&["factory-droid"])
        );
    }

    #[tokio::test]
    async fn test_get_central_skills_excludes_non_central() {
        let pool = setup_test_db().await;

        let central = make_skill("c-skill", "Central", true);
        let non_central = make_skill("nc-skill", "Non-Central", false);
        db::upsert_skill(&pool, &central).await.unwrap();
        db::upsert_skill(&pool, &non_central).await.unwrap();

        let skills_with_links = get_central_skills_impl(&pool).await.unwrap();
        assert_eq!(
            skills_with_links.len(),
            1,
            "only central skills should be returned"
        );
        assert_eq!(skills_with_links[0].id, "c-skill");
    }

    // ── delete_central_skill ──────────────────────────────────────────────────

    #[tokio::test]
    async fn test_delete_central_skill_removes_files_and_related_rows() {
        let tmp = TempDir::new().unwrap();
        let central_dir = tmp.path().join("central");
        fs::create_dir_all(&central_dir).unwrap();
        let pool = setup_test_db().await;
        set_agent_dir(&pool, "central", &central_dir).await;
        create_central_skill(&pool, &central_dir, "delete-me").await;

        let collection = db::create_collection(&pool, "Cleanup", None).await.unwrap();
        db::add_skill_to_collection(&pool, &collection.id, "delete-me")
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO skill_explanations (skill_id, explanation, lang, model, created_at, updated_at)
             VALUES ('delete-me', 'cached', 'en', 'test-model', 'now', 'now')",
        )
        .execute(&pool)
        .await
        .unwrap();

        let result = delete_central_skill_impl(
            &pool,
            "delete-me",
            DeleteCentralSkillOptions {
                cascade_uninstall: false,
            },
        )
        .await
        .unwrap();

        assert_eq!(result.skill_id, "delete-me");
        assert!(!central_dir.join("delete-me").exists());
        assert!(db::get_skill_by_id(&pool, "delete-me")
            .await
            .unwrap()
            .is_none());

        let collection_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM collection_skills WHERE skill_id = 'delete-me'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        let explanation_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM skill_explanations WHERE skill_id = 'delete-me'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();

        assert_eq!(collection_count, 0);
        assert_eq!(explanation_count, 0);
    }

    #[tokio::test]
    async fn test_delete_central_skill_refuses_linked_skill_without_cascade() {
        let tmp = TempDir::new().unwrap();
        let central_dir = tmp.path().join("central");
        fs::create_dir_all(&central_dir).unwrap();
        let pool = setup_test_db().await;
        set_agent_dir(&pool, "central", &central_dir).await;
        create_central_skill(&pool, &central_dir, "linked-skill").await;

        db::upsert_skill_installation(
            &pool,
            &SkillInstallation {
                skill_id: "linked-skill".to_string(),
                agent_id: "claude-code".to_string(),
                installed_path: tmp
                    .path()
                    .join("claude")
                    .join("linked-skill")
                    .to_string_lossy()
                    .into_owned(),
                link_type: "symlink".to_string(),
                symlink_target: Some(
                    central_dir
                        .join("linked-skill")
                        .to_string_lossy()
                        .into_owned(),
                ),
                created_at: Utc::now().to_rfc3339(),
            },
        )
        .await
        .unwrap();

        let err = delete_central_skill_impl(
            &pool,
            "linked-skill",
            DeleteCentralSkillOptions {
                cascade_uninstall: false,
            },
        )
        .await
        .unwrap_err();

        assert!(err.contains("installed on agents"));
        assert!(central_dir.join("linked-skill").exists());
        assert!(db::get_skill_by_id(&pool, "linked-skill")
            .await
            .unwrap()
            .is_some());
    }

    #[tokio::test]
    async fn test_delete_central_skill_cascades_platform_symlinks() {
        let tmp = TempDir::new().unwrap();
        let central_dir = tmp.path().join("central");
        let claude_dir = tmp.path().join("claude");
        fs::create_dir_all(&central_dir).unwrap();
        fs::create_dir_all(&claude_dir).unwrap();
        let pool = setup_test_db().await;
        set_agent_dir(&pool, "central", &central_dir).await;
        set_agent_dir(&pool, "claude-code", &claude_dir).await;
        create_central_skill(&pool, &central_dir, "cascade-me").await;

        let install_path = claude_dir.join("cascade-me");
        create_symlink(&central_dir.join("cascade-me"), &install_path).unwrap();
        db::upsert_skill_installation(
            &pool,
            &SkillInstallation {
                skill_id: "cascade-me".to_string(),
                agent_id: "claude-code".to_string(),
                installed_path: install_path.to_string_lossy().into_owned(),
                link_type: "symlink".to_string(),
                symlink_target: Some(
                    central_dir
                        .join("cascade-me")
                        .to_string_lossy()
                        .into_owned(),
                ),
                created_at: Utc::now().to_rfc3339(),
            },
        )
        .await
        .unwrap();

        let result = delete_central_skill_impl(
            &pool,
            "cascade-me",
            DeleteCentralSkillOptions {
                cascade_uninstall: true,
            },
        )
        .await
        .unwrap();

        assert_eq!(result.uninstalled_agents, vec!["claude-code".to_string()]);
        assert!(!install_path.exists());
        assert!(!central_dir.join("cascade-me").exists());
        assert!(db::get_skill_installations(&pool, "cascade-me")
            .await
            .unwrap()
            .is_empty());
    }

    #[tokio::test]
    async fn test_delete_central_skill_refuses_canonical_path_outside_central_root() {
        let tmp = TempDir::new().unwrap();
        let central_dir = tmp.path().join("central");
        let outside_dir = tmp.path().join("outside").join("escape-skill");
        fs::create_dir_all(&central_dir).unwrap();
        fs::create_dir_all(&outside_dir).unwrap();
        fs::write(outside_dir.join("SKILL.md"), "---\nname: escape\n---\n").unwrap();
        let pool = setup_test_db().await;
        set_agent_dir(&pool, "central", &central_dir).await;

        let mut skill = make_skill("escape-skill", "escape-skill", true);
        skill.file_path = outside_dir.join("SKILL.md").to_string_lossy().into_owned();
        skill.canonical_path = Some(outside_dir.to_string_lossy().into_owned());
        db::upsert_skill(&pool, &skill).await.unwrap();

        let err = delete_central_skill_impl(
            &pool,
            "escape-skill",
            DeleteCentralSkillOptions {
                cascade_uninstall: true,
            },
        )
        .await
        .unwrap_err();

        assert!(err.contains("outside Central Skills root"));
        assert!(outside_dir.exists());
        assert!(db::get_skill_by_id(&pool, "escape-skill")
            .await
            .unwrap()
            .is_some());
    }

    #[tokio::test]
    async fn test_delete_central_skill_does_not_uninstall_shared_codex_root_as_copy() {
        let tmp = TempDir::new().unwrap();
        let central_dir = tmp.path().join("central");
        fs::create_dir_all(&central_dir).unwrap();
        let pool = setup_test_db().await;
        set_agent_dir(&pool, "central", &central_dir).await;
        set_agent_dir(&pool, "codex", &central_dir).await;
        create_central_skill(&pool, &central_dir, "shared-root").await;

        db::upsert_skill_installation(
            &pool,
            &SkillInstallation {
                skill_id: "shared-root".to_string(),
                agent_id: "codex".to_string(),
                installed_path: central_dir
                    .join("shared-root")
                    .to_string_lossy()
                    .into_owned(),
                link_type: "copy".to_string(),
                symlink_target: None,
                created_at: Utc::now().to_rfc3339(),
            },
        )
        .await
        .unwrap();

        let result = delete_central_skill_impl(
            &pool,
            "shared-root",
            DeleteCentralSkillOptions {
                cascade_uninstall: true,
            },
        )
        .await
        .unwrap();

        assert!(result.uninstalled_agents.is_empty());
        assert!(!central_dir.join("shared-root").exists());
        assert!(db::get_skill_installations(&pool, "shared-root")
            .await
            .unwrap()
            .is_empty());
    }

    // ── delete_resource_skill ─────────────────────────────────────────────────

    #[tokio::test]
    async fn test_delete_resource_skill_removes_files_and_related_rows() {
        let pool = setup_test_db().await;
        let tmp = TempDir::new().unwrap();
        let resource_root = tmp.path().join("library");
        let skill_dir = resource_root.join("openai").join("skills").join("demo");
        fs::create_dir_all(&skill_dir).unwrap();
        let skill_file = skill_dir.join("SKILL.md");
        fs::write(&skill_file, "---\nname: demo\n---\nDemo").unwrap();
        db::set_skill_resource_library_dir(&pool, resource_root.to_str().unwrap())
            .await
            .unwrap();

        let skill = make_skill_with_path("demo", "Demo", &skill_file, &skill_dir, false);
        db::upsert_skill(&pool, &skill).await.unwrap();
        db::upsert_skill_source(
            &pool,
            &db::SkillSource {
                skill_id: "demo".to_string(),
                source_type: "raw".to_string(),
                source_url: Some("https://example.com/demo/SKILL.md".to_string()),
                source_author: Some("openai".to_string()),
                source_repo: Some("openai/skills".to_string()),
                source_path: Some("skills/demo/SKILL.md".to_string()),
                updated_at: Utc::now().to_rfc3339(),
            },
        )
        .await
        .unwrap();

        let result = delete_resource_skill_impl(
            &pool,
            "demo",
            DeleteResourceSkillOptions {
                cascade_uninstall: false,
            },
        )
        .await
        .unwrap();

        assert_eq!(result.skill_id, "demo");
        assert!(!skill_dir.exists());
        assert!(db::get_skill_by_id(&pool, "demo").await.unwrap().is_none());
        let source_count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM skill_sources WHERE skill_id = ?")
                .bind("demo")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(source_count, 0);
    }

    #[tokio::test]
    async fn test_delete_resource_skill_refuses_linked_without_cascade() {
        let pool = setup_test_db().await;
        let tmp = TempDir::new().unwrap();
        let resource_root = tmp.path().join("library");
        let skill_dir = resource_root.join("demo");
        fs::create_dir_all(&skill_dir).unwrap();
        let skill_file = skill_dir.join("SKILL.md");
        fs::write(&skill_file, "---\nname: demo\n---\nDemo").unwrap();
        db::set_skill_resource_library_dir(&pool, resource_root.to_str().unwrap())
            .await
            .unwrap();

        let skill = make_skill_with_path("demo", "Demo", &skill_file, &skill_dir, false);
        db::upsert_skill(&pool, &skill).await.unwrap();
        db::upsert_skill_installation(
            &pool,
            &SkillInstallation {
                skill_id: "demo".to_string(),
                agent_id: "cursor".to_string(),
                installed_path: tmp
                    .path()
                    .join("cursor")
                    .join("demo")
                    .to_string_lossy()
                    .into_owned(),
                link_type: "copy".to_string(),
                symlink_target: None,
                created_at: Utc::now().to_rfc3339(),
            },
        )
        .await
        .unwrap();

        let err = delete_resource_skill_impl(
            &pool,
            "demo",
            DeleteResourceSkillOptions {
                cascade_uninstall: false,
            },
        )
        .await
        .unwrap_err();

        assert!(err.contains("Skill is installed on agents"));
        assert!(skill_dir.exists());
    }

    #[tokio::test]
    async fn test_delete_resource_skill_cascades_platform_copy() {
        let pool = setup_test_db().await;
        let tmp = TempDir::new().unwrap();
        let resource_root = tmp.path().join("library");
        let cursor_root = tmp.path().join("cursor");
        let skill_dir = resource_root.join("demo");
        let installed_dir = cursor_root.join("demo");
        fs::create_dir_all(&skill_dir).unwrap();
        fs::create_dir_all(&installed_dir).unwrap();
        let skill_file = skill_dir.join("SKILL.md");
        fs::write(&skill_file, "---\nname: demo\n---\nDemo").unwrap();
        fs::write(installed_dir.join("SKILL.md"), "---\nname: demo\n---\nDemo").unwrap();
        db::set_skill_resource_library_dir(&pool, resource_root.to_str().unwrap())
            .await
            .unwrap();
        set_agent_dir(&pool, "cursor", &cursor_root).await;

        let skill = make_skill_with_path("demo", "Demo", &skill_file, &skill_dir, false);
        db::upsert_skill(&pool, &skill).await.unwrap();
        db::upsert_skill_installation(
            &pool,
            &SkillInstallation {
                skill_id: "demo".to_string(),
                agent_id: "cursor".to_string(),
                installed_path: installed_dir.to_string_lossy().into_owned(),
                link_type: "copy".to_string(),
                symlink_target: None,
                created_at: Utc::now().to_rfc3339(),
            },
        )
        .await
        .unwrap();

        let result = delete_resource_skill_impl(
            &pool,
            "demo",
            DeleteResourceSkillOptions {
                cascade_uninstall: true,
            },
        )
        .await
        .unwrap();

        assert_eq!(result.uninstalled_agents, vec!["cursor".to_string()]);
        assert!(!skill_dir.exists());
        assert!(!installed_dir.exists());
        assert!(db::get_skill_installations(&pool, "demo")
            .await
            .unwrap()
            .is_empty());
        assert!(db::get_skill_by_id(&pool, "demo").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn test_delete_resource_skill_rejects_path_outside_resource_root() {
        let pool = setup_test_db().await;
        let tmp = TempDir::new().unwrap();
        let resource_root = tmp.path().join("library");
        let outside_dir = tmp.path().join("outside").join("demo");
        fs::create_dir_all(&resource_root).unwrap();
        fs::create_dir_all(&outside_dir).unwrap();
        let skill_file = outside_dir.join("SKILL.md");
        fs::write(&skill_file, "---\nname: demo\n---\nDemo").unwrap();
        db::set_skill_resource_library_dir(&pool, resource_root.to_str().unwrap())
            .await
            .unwrap();

        let skill = make_skill_with_path("demo", "Demo", &skill_file, &outside_dir, false);
        db::upsert_skill(&pool, &skill).await.unwrap();

        let err = delete_resource_skill_impl(
            &pool,
            "demo",
            DeleteResourceSkillOptions {
                cascade_uninstall: false,
            },
        )
        .await
        .unwrap_err();

        assert!(err.contains("outside Skill Resource Library"));
        assert!(outside_dir.exists());
    }

    #[tokio::test]
    async fn test_preview_delete_central_skill_bundle_reports_nested_skills_and_agents() {
        let tmp = TempDir::new().unwrap();
        let central_dir = tmp.path().join("central");
        let claude_dir = tmp.path().join("claude");
        let bundle_dir = central_dir.join("Superpowers");
        fs::create_dir_all(&central_dir).unwrap();
        fs::create_dir_all(&claude_dir).unwrap();
        let pool = setup_test_db().await;
        set_agent_dir(&pool, "central", &central_dir).await;
        set_agent_dir(&pool, "claude-code", &claude_dir).await;
        create_nested_central_skill(&pool, &bundle_dir, "using-superpowers").await;
        create_nested_central_skill(&pool, &bundle_dir, "writing-plans").await;

        let install_path = claude_dir.join("using-superpowers");
        create_symlink(&bundle_dir.join("using-superpowers"), &install_path).unwrap();
        db::upsert_skill_installation(
            &pool,
            &SkillInstallation {
                skill_id: "using-superpowers".to_string(),
                agent_id: "claude-code".to_string(),
                installed_path: install_path.to_string_lossy().into_owned(),
                link_type: "symlink".to_string(),
                symlink_target: Some(
                    bundle_dir
                        .join("using-superpowers")
                        .to_string_lossy()
                        .into_owned(),
                ),
                created_at: Utc::now().to_rfc3339(),
            },
        )
        .await
        .unwrap();

        let preview = preview_delete_central_skill_bundle_impl(&pool, "Superpowers")
            .await
            .unwrap();

        assert_eq!(preview.bundle.relative_path, "Superpowers");
        assert!(!preview.bundle.is_symlink);
        assert_eq!(preview.bundle.skill_count, 2);
        assert_eq!(preview.affected_agents, vec!["claude-code".to_string()]);
        assert_eq!(
            preview
                .skills
                .iter()
                .map(|skill| skill.id.as_str())
                .collect::<Vec<_>>(),
            vec!["using-superpowers", "writing-plans"]
        );
    }

    #[tokio::test]
    async fn test_get_central_skill_bundle_detail_returns_skills_and_links() {
        let tmp = TempDir::new().unwrap();
        let central_dir = tmp.path().join("central");
        let cursor_dir = tmp.path().join("cursor");
        let bundle_dir = central_dir.join("Superpowers");
        fs::create_dir_all(&central_dir).unwrap();
        fs::create_dir_all(&cursor_dir).unwrap();
        let pool = setup_test_db().await;
        set_agent_dir(&pool, "central", &central_dir).await;
        set_agent_dir(&pool, "cursor", &cursor_dir).await;
        create_nested_central_skill(&pool, &bundle_dir, "using-superpowers").await;
        create_nested_central_skill(&pool, &bundle_dir, "writing-plans").await;

        let install_path = cursor_dir.join("using-superpowers");
        create_symlink(&bundle_dir.join("using-superpowers"), &install_path).unwrap();
        db::upsert_skill_installation(
            &pool,
            &SkillInstallation {
                skill_id: "using-superpowers".to_string(),
                agent_id: "cursor".to_string(),
                installed_path: install_path.to_string_lossy().into_owned(),
                link_type: "symlink".to_string(),
                symlink_target: Some(
                    bundle_dir
                        .join("using-superpowers")
                        .to_string_lossy()
                        .into_owned(),
                ),
                created_at: Utc::now().to_rfc3339(),
            },
        )
        .await
        .unwrap();

        let detail = get_central_skill_bundle_detail_impl(&pool, "Superpowers")
            .await
            .unwrap();

        assert_eq!(detail.bundle.relative_path, "Superpowers");
        assert_eq!(detail.bundle.skill_count, 2);
        assert_eq!(detail.bundle.linked_agent_count, 1);
        assert_eq!(
            detail
                .skills
                .iter()
                .map(|skill| skill.id.as_str())
                .collect::<Vec<_>>(),
            vec!["using-superpowers", "writing-plans"]
        );
        assert_eq!(detail.skills[0].linked_agents, vec!["cursor".to_string()]);
    }

    #[tokio::test]
    async fn test_get_central_skill_bundle_detail_rejects_unsafe_paths() {
        let tmp = TempDir::new().unwrap();
        let central_dir = tmp.path().join("central");
        fs::create_dir_all(&central_dir).unwrap();
        let pool = setup_test_db().await;
        set_agent_dir(&pool, "central", &central_dir).await;

        let err = get_central_skill_bundle_detail_impl(&pool, "../Superpowers")
            .await
            .unwrap_err();

        assert!(err.contains("Invalid Central bundle path"));
    }

    #[tokio::test]
    async fn test_delete_central_skill_bundle_removes_local_dir_records_and_platform_links() {
        let tmp = TempDir::new().unwrap();
        let central_dir = tmp.path().join("central");
        let claude_dir = tmp.path().join("claude");
        let bundle_dir = central_dir.join("Superpowers");
        fs::create_dir_all(&central_dir).unwrap();
        fs::create_dir_all(&claude_dir).unwrap();
        let pool = setup_test_db().await;
        set_agent_dir(&pool, "central", &central_dir).await;
        set_agent_dir(&pool, "claude-code", &claude_dir).await;
        create_nested_central_skill(&pool, &bundle_dir, "using-superpowers").await;
        create_nested_central_skill(&pool, &bundle_dir, "writing-plans").await;

        let install_path = claude_dir.join("using-superpowers");
        create_symlink(&bundle_dir.join("using-superpowers"), &install_path).unwrap();
        db::upsert_skill_installation(
            &pool,
            &SkillInstallation {
                skill_id: "using-superpowers".to_string(),
                agent_id: "claude-code".to_string(),
                installed_path: install_path.to_string_lossy().into_owned(),
                link_type: "symlink".to_string(),
                symlink_target: Some(
                    bundle_dir
                        .join("using-superpowers")
                        .to_string_lossy()
                        .into_owned(),
                ),
                created_at: Utc::now().to_rfc3339(),
            },
        )
        .await
        .unwrap();

        let result = delete_central_skill_bundle_impl(
            &pool,
            "Superpowers",
            DeleteCentralSkillBundleOptions {
                cascade_uninstall: true,
            },
        )
        .await
        .unwrap();

        assert_eq!(result.relative_path, "Superpowers");
        assert_eq!(result.removed_kind, "directory");
        assert_eq!(
            result.removed_skill_ids,
            vec!["using-superpowers".to_string(), "writing-plans".to_string()]
        );
        assert_eq!(result.uninstalled_agents, vec!["claude-code".to_string()]);
        assert!(!bundle_dir.exists());
        assert!(!install_path.exists());
        assert!(db::get_skill_by_id(&pool, "using-superpowers")
            .await
            .unwrap()
            .is_none());
        assert!(db::get_skill_by_id(&pool, "writing-plans")
            .await
            .unwrap()
            .is_none());
    }

    #[tokio::test]
    async fn test_delete_central_skill_bundle_removes_resource_symlink_children_and_platform_links()
    {
        let tmp = TempDir::new().unwrap();
        let central_dir = tmp.path().join("central");
        let resource_dir = tmp.path().join("resource-library");
        let claude_dir = tmp.path().join("claude").join("skills");
        let resource_skill = resource_dir
            .join("mattpocock")
            .join("skills")
            .join("ask-matt");
        fs::create_dir_all(&central_dir).unwrap();
        fs::create_dir_all(&claude_dir).unwrap();
        fs::create_dir_all(&resource_skill).unwrap();
        fs::write(
            resource_skill.join("SKILL.md"),
            "---\nname: ask-matt\ndescription: Resource skill\n---\n",
        )
        .unwrap();

        let pool = setup_test_db().await;
        set_agent_dir(&pool, "central", &central_dir).await;
        set_agent_dir(&pool, "claude-code", &claude_dir).await;
        sqlx::query(
            "UPDATE agents SET is_enabled = 0 WHERE id != 'central' AND id != 'claude-code'",
        )
        .execute(&pool)
        .await
        .unwrap();
        db::set_skill_resource_library_dir(&pool, &resource_dir.to_string_lossy())
            .await
            .unwrap();
        db::upsert_skill(
            &pool,
            &Skill {
                id: "ask-matt".to_string(),
                name: "ask-matt".to_string(),
                description: Some("Resource skill".to_string()),
                file_path: resource_skill
                    .join("SKILL.md")
                    .to_string_lossy()
                    .into_owned(),
                canonical_path: Some(resource_skill.to_string_lossy().into_owned()),
                is_central: false,
                source: Some("resource-library".to_string()),
                content: None,
                scanned_at: Utc::now().to_rfc3339(),
            },
        )
        .await
        .unwrap();

        let central_skill = central_dir
            .join("mattpocock")
            .join("skills")
            .join("ask-matt");
        fs::create_dir_all(central_skill.parent().unwrap()).unwrap();
        crate::commands::linker::create_symlink(&resource_skill, &central_skill).unwrap();
        db::upsert_skill(
            &pool,
            &db::Skill {
                id: "ask-matt".to_string(),
                name: "ask-matt".to_string(),
                description: Some("Ask Matt".to_string()),
                file_path: central_skill
                    .join("SKILL.md")
                    .to_string_lossy()
                    .into_owned(),
                canonical_path: Some(central_skill.to_string_lossy().into_owned()),
                is_central: true,
                source: Some("resource-library".to_string()),
                content: None,
                scanned_at: Utc::now().to_rfc3339(),
            },
        )
        .await
        .unwrap();

        let platform_skill = claude_dir
            .join("mattpocock")
            .join("skills")
            .join("ask-matt");
        fs::create_dir_all(platform_skill.parent().unwrap()).unwrap();
        crate::commands::linker::create_symlink(&central_skill, &platform_skill).unwrap();
        db::upsert_skill_installation(
            &pool,
            &SkillInstallation {
                skill_id: "ask-matt".to_string(),
                agent_id: "claude-code".to_string(),
                installed_path: platform_skill.to_string_lossy().into_owned(),
                link_type: "symlink".to_string(),
                symlink_target: Some(central_skill.to_string_lossy().into_owned()),
                created_at: Utc::now().to_rfc3339(),
            },
        )
        .await
        .unwrap();

        assert!(fs::symlink_metadata(&central_skill)
            .unwrap()
            .file_type()
            .is_symlink());
        assert!(platform_skill.join("SKILL.md").exists());
        fs::create_dir_all(claude_dir.join("mattpocock").join("leftover-empty")).unwrap();

        let preview = preview_delete_central_skill_bundle_impl(&pool, "mattpocock")
            .await
            .unwrap();
        assert_eq!(preview.bundle.skill_count, 1);
        assert_eq!(
            preview
                .skills
                .iter()
                .map(|skill| skill.id.as_str())
                .collect::<Vec<_>>(),
            vec!["ask-matt"]
        );
        assert_eq!(preview.affected_agents, vec!["claude-code".to_string()]);

        let result = delete_central_skill_bundle_impl(
            &pool,
            "mattpocock",
            DeleteCentralSkillBundleOptions {
                cascade_uninstall: true,
            },
        )
        .await
        .unwrap();

        assert_eq!(result.removed_skill_ids, vec!["ask-matt".to_string()]);
        assert!(result
            .uninstalled_agents
            .iter()
            .any(|agent_id| agent_id == "claude-code"));
        assert!(!central_dir.join("mattpocock").exists());
        assert!(!platform_skill.exists());
        assert!(
            !claude_dir.join("mattpocock").exists(),
            "directory uninstall must also remove the mirrored author folder on platforms"
        );
        assert!(resource_skill.join("SKILL.md").exists());
        assert!(
            db::get_central_skills(&pool).await.unwrap().is_empty(),
            "directory delete must drop nested Central Skills records"
        );
        let restored = db::get_skill_by_id(&pool, "ask-matt")
            .await
            .unwrap()
            .unwrap();
        assert!(!restored.is_central);
        assert_eq!(
            restored.canonical_path.as_deref(),
            Some(resource_skill.to_string_lossy().as_ref())
        );
        assert!(db::get_skill_installations(&pool, "ask-matt")
            .await
            .unwrap()
            .is_empty());
    }

    #[tokio::test]
    async fn test_delete_central_skill_bundle_removes_symlink_but_keeps_target() {
        let tmp = TempDir::new().unwrap();
        let central_dir = tmp.path().join("central");
        let real_bundle_dir = tmp.path().join("real-superpowers");
        let central_bundle_link = central_dir.join("Superpowers");
        fs::create_dir_all(&central_dir).unwrap();
        fs::create_dir_all(&real_bundle_dir).unwrap();
        let pool = setup_test_db().await;
        set_agent_dir(&pool, "central", &central_dir).await;
        create_nested_central_skill(&pool, &central_bundle_link, "using-superpowers").await;
        fs::remove_dir_all(&central_bundle_link).unwrap();
        create_nested_central_skill(&pool, &real_bundle_dir, "using-superpowers").await;
        create_symlink(&real_bundle_dir, &central_bundle_link).unwrap();

        let mut skill = db::get_skill_by_id(&pool, "using-superpowers")
            .await
            .unwrap()
            .unwrap();
        skill.file_path = central_bundle_link
            .join("using-superpowers/SKILL.md")
            .to_string_lossy()
            .into_owned();
        skill.canonical_path = Some(
            central_bundle_link
                .join("using-superpowers")
                .to_string_lossy()
                .into_owned(),
        );
        db::upsert_skill(&pool, &skill).await.unwrap();

        let result = delete_central_skill_bundle_impl(
            &pool,
            "Superpowers",
            DeleteCentralSkillBundleOptions {
                cascade_uninstall: true,
            },
        )
        .await
        .unwrap();

        assert_eq!(result.removed_kind, "symlink");
        assert!(std::fs::symlink_metadata(&central_bundle_link).is_err());
        assert!(real_bundle_dir.join("using-superpowers/SKILL.md").exists());
        assert!(db::get_skill_by_id(&pool, "using-superpowers")
            .await
            .unwrap()
            .is_none());
    }

    #[tokio::test]
    async fn test_delete_central_skill_bundle_rejects_unsafe_paths() {
        let tmp = TempDir::new().unwrap();
        let central_dir = tmp.path().join("central");
        fs::create_dir_all(&central_dir).unwrap();
        let pool = setup_test_db().await;
        set_agent_dir(&pool, "central", &central_dir).await;

        let err = preview_delete_central_skill_bundle_impl(&pool, "../Superpowers")
            .await
            .unwrap_err();
        assert!(err.contains("Invalid Central bundle path"));

        let err = delete_central_skill_bundle_impl(
            &pool,
            "",
            DeleteCentralSkillBundleOptions {
                cascade_uninstall: true,
            },
        )
        .await
        .unwrap_err();
        assert!(err.contains("Invalid Central bundle path"));
    }

    #[tokio::test]
    async fn test_preview_delete_resource_skill_bundle_reports_nested_skills_and_agents() {
        let tmp = TempDir::new().unwrap();
        let resource_dir = tmp.path().join("resource-library");
        let claude_dir = tmp.path().join("claude");
        let bundle_dir = resource_dir.join("owner").join("repo");
        fs::create_dir_all(&resource_dir).unwrap();
        fs::create_dir_all(&claude_dir).unwrap();
        let pool = setup_test_db().await;
        db::set_skill_resource_library_dir(&pool, &resource_dir.to_string_lossy())
            .await
            .unwrap();
        set_agent_dir(&pool, "claude-code", &claude_dir).await;
        create_nested_resource_skill(&pool, &bundle_dir, "first-skill").await;
        create_nested_resource_skill(&pool, &bundle_dir, "second-skill").await;

        let install_path = claude_dir.join("first-skill");
        create_symlink(&bundle_dir.join("first-skill"), &install_path).unwrap();
        db::upsert_skill_installation(
            &pool,
            &SkillInstallation {
                skill_id: "first-skill".to_string(),
                agent_id: "claude-code".to_string(),
                installed_path: install_path.to_string_lossy().into_owned(),
                link_type: "symlink".to_string(),
                symlink_target: Some(
                    bundle_dir
                        .join("first-skill")
                        .to_string_lossy()
                        .into_owned(),
                ),
                created_at: Utc::now().to_rfc3339(),
            },
        )
        .await
        .unwrap();

        let preview = preview_delete_resource_skill_bundle_impl(&pool, "owner/repo")
            .await
            .unwrap();

        assert_eq!(preview.bundle.relative_path, "owner/repo");
        assert_eq!(preview.bundle.skill_count, 2);
        assert_eq!(preview.affected_agents, vec!["claude-code".to_string()]);
        assert_eq!(
            preview
                .skills
                .iter()
                .map(|skill| skill.id.as_str())
                .collect::<Vec<_>>(),
            vec!["first-skill", "second-skill"]
        );
    }

    #[tokio::test]
    async fn test_delete_resource_skill_bundle_removes_dir_records_and_platform_links() {
        let tmp = TempDir::new().unwrap();
        let resource_dir = tmp.path().join("resource-library");
        let claude_dir = tmp.path().join("claude");
        let bundle_dir = resource_dir.join("owner").join("repo");
        fs::create_dir_all(&resource_dir).unwrap();
        fs::create_dir_all(&claude_dir).unwrap();
        let pool = setup_test_db().await;
        db::set_skill_resource_library_dir(&pool, &resource_dir.to_string_lossy())
            .await
            .unwrap();
        set_agent_dir(&pool, "claude-code", &claude_dir).await;
        create_nested_resource_skill(&pool, &bundle_dir, "first-skill").await;
        create_nested_resource_skill(&pool, &bundle_dir, "second-skill").await;

        let install_path = claude_dir.join("first-skill");
        create_symlink(&bundle_dir.join("first-skill"), &install_path).unwrap();
        db::upsert_skill_installation(
            &pool,
            &SkillInstallation {
                skill_id: "first-skill".to_string(),
                agent_id: "claude-code".to_string(),
                installed_path: install_path.to_string_lossy().into_owned(),
                link_type: "symlink".to_string(),
                symlink_target: Some(
                    bundle_dir
                        .join("first-skill")
                        .to_string_lossy()
                        .into_owned(),
                ),
                created_at: Utc::now().to_rfc3339(),
            },
        )
        .await
        .unwrap();

        let result = delete_resource_skill_bundle_impl(
            &pool,
            "owner/repo",
            DeleteCentralSkillBundleOptions {
                cascade_uninstall: true,
            },
        )
        .await
        .unwrap();

        assert_eq!(result.relative_path, "owner/repo");
        assert_eq!(result.removed_kind, "directory");
        assert_eq!(
            result.removed_skill_ids,
            vec!["first-skill".to_string(), "second-skill".to_string()]
        );
        assert_eq!(result.uninstalled_agents, vec!["claude-code".to_string()]);
        assert!(!bundle_dir.exists());
        assert!(!install_path.exists());
        assert!(db::get_skill_by_id(&pool, "first-skill")
            .await
            .unwrap()
            .is_none());
        assert!(db::get_skill_by_id(&pool, "second-skill")
            .await
            .unwrap()
            .is_none());
    }

    #[tokio::test]
    async fn test_delete_resource_skill_bundle_rejects_root_and_unsafe_paths() {
        let tmp = TempDir::new().unwrap();
        let resource_dir = tmp.path().join("resource-library");
        fs::create_dir_all(&resource_dir).unwrap();
        let pool = setup_test_db().await;
        db::set_skill_resource_library_dir(&pool, &resource_dir.to_string_lossy())
            .await
            .unwrap();

        let err = preview_delete_resource_skill_bundle_impl(&pool, "../repo")
            .await
            .unwrap_err();
        assert!(err.contains("Invalid Resource Library bundle path"));

        let err = delete_resource_skill_bundle_impl(
            &pool,
            "",
            DeleteCentralSkillBundleOptions {
                cascade_uninstall: true,
            },
        )
        .await
        .unwrap_err();
        assert!(err.contains("Invalid Resource Library bundle path"));
    }

    #[tokio::test]
    async fn test_create_manual_resource_skill_writes_skill_md_and_metadata() {
        let pool = setup_test_db().await;
        let tmp = TempDir::new().unwrap();
        let resource_dir = tmp.path().join("resource-library");
        fs::create_dir_all(&resource_dir).unwrap();
        db::set_skill_resource_library_dir(&pool, &resource_dir.to_string_lossy())
            .await
            .unwrap();

        let created = create_manual_resource_skill_impl(
            &pool,
            CreateManualResourceSkillRequest {
                skill_id: "manual-demo".to_string(),
                name: "Manual Demo".to_string(),
                description: Some("Created by hand".to_string()),
                body: Some("Use this manually created skill.".to_string()),
                source_url: None,
                source_author: Some("local-author".to_string()),
                source_repo: None,
                source_path: Some("manual-demo/SKILL.md".to_string()),
            },
        )
        .await
        .unwrap();

        let skill_dir = resource_dir.join("manual-demo");
        let skill_md = fs::read_to_string(skill_dir.join("SKILL.md")).unwrap();
        assert!(skill_md.contains("name: Manual Demo"));
        assert!(skill_md.contains("description: Created by hand"));
        assert!(skill_md.contains("Use this manually created skill."));
        assert_eq!(created.id, "manual-demo");
        assert_eq!(created.source.as_deref(), Some("manual"));
        let source = db::get_skill_source(&pool, "manual-demo")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(source.source_type, "manual");
        assert_eq!(source.source_author.as_deref(), Some("local-author"));
        assert_eq!(source.source_path.as_deref(), Some("manual-demo/SKILL.md"));
    }

    #[tokio::test]
    async fn test_create_manual_resource_skill_rejects_existing_and_unsafe_targets() {
        let pool = setup_test_db().await;
        let tmp = TempDir::new().unwrap();
        let resource_dir = tmp.path().join("resource-library");
        fs::create_dir_all(resource_dir.join("manual-demo")).unwrap();
        db::set_skill_resource_library_dir(&pool, &resource_dir.to_string_lossy())
            .await
            .unwrap();

        let existing_error = create_manual_resource_skill_impl(
            &pool,
            CreateManualResourceSkillRequest {
                skill_id: "manual-demo".to_string(),
                name: "Manual Demo".to_string(),
                description: None,
                body: None,
                source_url: None,
                source_author: None,
                source_repo: None,
                source_path: None,
            },
        )
        .await
        .unwrap_err();
        assert!(existing_error.contains("already exists"));

        let unsafe_error = create_manual_resource_skill_impl(
            &pool,
            CreateManualResourceSkillRequest {
                skill_id: "../outside".to_string(),
                name: "Outside".to_string(),
                description: None,
                body: None,
                source_url: None,
                source_author: None,
                source_repo: None,
                source_path: None,
            },
        )
        .await
        .unwrap_err();
        assert!(unsafe_error.contains("Invalid manual skill id"));
        assert!(!tmp.path().join("outside").exists());
    }

    #[tokio::test]
    async fn test_add_local_resource_skills_imports_single_skill_directory() {
        let pool = setup_test_db().await;
        let tmp = TempDir::new().unwrap();
        let resource_dir = tmp.path().join("resource-library");
        let source_dir = tmp.path().join("local-source").join("local-demo");
        fs::create_dir_all(source_dir.join("scripts")).unwrap();
        fs::write(
            source_dir.join("SKILL.md"),
            "---\nname: Local Demo\ndescription: Added from a local folder\n---\n\n# Local Demo\n",
        )
        .unwrap();
        fs::write(
            source_dir.join("scripts").join("run.ps1"),
            "Write-Host local",
        )
        .unwrap();
        db::set_skill_resource_library_dir(&pool, &resource_dir.to_string_lossy())
            .await
            .unwrap();

        let result = add_local_resource_skills_impl(
            &pool,
            AddLocalResourceSkillsRequest {
                selected_skill_ids: None,
                source_dir: source_dir.to_string_lossy().into_owned(),
                overwrite: false,
            },
        )
        .await
        .unwrap();

        assert_eq!(result.import_kind, LocalResourceImportKind::SingleSkill);
        assert_eq!(result.collection_name, None);
        assert_eq!(result.added_skills.len(), 1);
        assert_eq!(result.added_skills[0].id, "local-demo");
        assert!(resource_dir
            .join("local")
            .join("local-demo")
            .join("scripts")
            .join("run.ps1")
            .exists());

        let source = db::get_skill_source(&pool, "local-demo")
            .await
            .unwrap()
            .unwrap();
        let canonical_source_dir = source_dir.canonicalize().unwrap();
        assert_eq!(source.source_type, "local-folder");
        assert_eq!(
            source.source_path.as_deref(),
            Some(canonical_source_dir.to_string_lossy().as_ref())
        );
        assert_eq!(source.source_repo, None);
    }

    #[tokio::test]
    async fn test_add_local_resource_skills_imports_collection_directory() {
        let pool = setup_test_db().await;
        let tmp = TempDir::new().unwrap();
        let resource_dir = tmp.path().join("resource-library");
        let source_dir = tmp.path().join("starter-pack");
        let alpha_dir = source_dir.join("alpha-skill");
        let beta_dir = source_dir.join("nested").join("beta-skill");
        fs::create_dir_all(&alpha_dir).unwrap();
        fs::create_dir_all(beta_dir.join("references")).unwrap();
        fs::write(
            alpha_dir.join("SKILL.md"),
            "---\nname: Alpha Skill\n---\n\n# Alpha\n",
        )
        .unwrap();
        fs::write(
            beta_dir.join("SKILL.md"),
            "---\nname: Beta Skill\n---\n\n# Beta\n",
        )
        .unwrap();
        fs::write(source_dir.join("readme.md"), "# Local repository").unwrap();
        fs::write(source_dir.join("README_CN.md"), "# 本地仓库").unwrap();
        fs::write(source_dir.join("README.en.md"), "# English repository").unwrap();
        fs::write(beta_dir.join("references").join("notes.md"), "# Notes").unwrap();
        db::set_skill_resource_library_dir(&pool, &resource_dir.to_string_lossy())
            .await
            .unwrap();

        let result = add_local_resource_skills_impl(
            &pool,
            AddLocalResourceSkillsRequest {
                selected_skill_ids: None,
                source_dir: source_dir.to_string_lossy().into_owned(),
                overwrite: false,
            },
        )
        .await
        .unwrap();

        assert_eq!(read_repository_readme_impl(&resource_dir.join("local/starter-pack")).unwrap(), Some("# Local repository".into()));
        let documents = repository_docs::read(&resource_dir.join("local/starter-pack")).unwrap();
        assert_eq!(documents.iter().map(|doc| doc.language).collect::<Vec<_>>(), vec!["default", "zh", "en"]);
        assert_eq!(result.import_kind, LocalResourceImportKind::Collection);
        assert_eq!(result.collection_name.as_deref(), Some("starter-pack"));
        assert_eq!(result.added_skills.len(), 2);
        assert!(resource_dir
            .join("local")
            .join("starter-pack")
            .join("alpha-skill")
            .join("SKILL.md")
            .exists());
        assert!(resource_dir
            .join("local")
            .join("starter-pack")
            .join("nested")
            .join("beta-skill")
            .join("references")
            .join("notes.md")
            .exists());

        let beta_source = db::get_skill_source(&pool, "beta-skill")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(beta_source.source_type, "local-folder");
        assert_eq!(beta_source.source_author.as_deref(), Some("starter-pack"));
    }

    #[tokio::test]
    async fn test_add_repo_resource_skills_imports_under_owner_repo() {
        let pool = setup_test_db().await;
        let tmp = TempDir::new().unwrap();
        let resource_dir = tmp.path().join("resource-library");
        let source_dir = tmp.path().join("staging").join(".agents").join("skills");
        let alpha_dir = source_dir.join("alpha-skill");
        let beta_dir = source_dir.join("nested").join("beta-skill");
        fs::create_dir_all(&alpha_dir).unwrap();
        fs::create_dir_all(&beta_dir).unwrap();
        fs::write(
            alpha_dir.join("SKILL.md"),
            "---\nname: Alpha Skill\n---\n\n# Alpha\n",
        )
        .unwrap();
        fs::write(
            beta_dir.join("SKILL.md"),
            "---\nname: Beta Skill\n---\n\n# Beta\n",
        )
        .unwrap();
        db::set_skill_resource_library_dir(&pool, &resource_dir.to_string_lossy())
            .await
            .unwrap();

        let result = add_repo_resource_skills_impl(
            &pool,
            AddLocalResourceSkillsRequest {
                selected_skill_ids: None,
                source_dir: source_dir.to_string_lossy().into_owned(),
                overwrite: false,
            },
            "owner",
            "repo",
        )
        .await
        .unwrap();

        assert_eq!(result.import_kind, LocalResourceImportKind::Collection);
        assert_eq!(result.collection_name.as_deref(), Some("skills"));
        assert_eq!(result.added_skills.len(), 2);
        assert!(resource_dir
            .join("owner")
            .join("repo")
            .join("alpha-skill")
            .join("SKILL.md")
            .exists());
        assert!(resource_dir
            .join("owner")
            .join("repo")
            .join("nested")
            .join("beta-skill")
            .join("SKILL.md")
            .exists());
        assert!(!resource_dir.join("local").join("skills").exists());

        let alpha_source = db::get_skill_source(&pool, "alpha-skill")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(alpha_source.source_type, "github");
        assert_eq!(alpha_source.source_author.as_deref(), Some("owner"));
        assert_eq!(alpha_source.source_repo.as_deref(), Some("owner/repo"));
    }

    // ── get_skill_detail ──────────────────────────────────────────────────────

    #[tokio::test]
    async fn test_get_skill_detail_returns_installations() {
        let pool = setup_test_db().await;

        let skill = make_skill("detail-skill", "Detail Skill", false);
        db::upsert_skill(&pool, &skill).await.unwrap();

        let now = Utc::now().to_rfc3339();
        db::upsert_skill_installation(
            &pool,
            &SkillInstallation {
                skill_id: "detail-skill".to_string(),
                agent_id: "claude-code".to_string(),
                installed_path: "/tmp/claude/detail-skill/SKILL.md".to_string(),
                link_type: "copy".to_string(),
                symlink_target: None,
                created_at: now.clone(),
            },
        )
        .await
        .unwrap();

        let detail = get_skill_detail_impl(&pool, "detail-skill").await.unwrap();
        assert_eq!(detail.id, "detail-skill");
        assert_eq!(detail.installations.len(), 1);
        assert_eq!(detail.installations[0].agent_id, "claude-code");
        // installed_at should be populated from created_at
        assert!(
            !detail.installations[0].installed_at.is_empty(),
            "installed_at must be set"
        );
        assert!(
            detail.collections.is_empty(),
            "skill should have no collections by default"
        );
    }

    #[tokio::test]
    async fn test_get_skill_detail_returns_collections() {
        let pool = setup_test_db().await;

        let skill = make_skill("detail-skill", "Detail Skill", false);
        db::upsert_skill(&pool, &skill).await.unwrap();

        let alpha = db::create_collection(&pool, "Alpha", Some("First collection"))
            .await
            .unwrap();
        let beta = db::create_collection(&pool, "Beta", None).await.unwrap();

        db::add_skill_to_collection(&pool, &alpha.id, "detail-skill")
            .await
            .unwrap();
        db::add_skill_to_collection(&pool, &beta.id, "detail-skill")
            .await
            .unwrap();

        let detail = get_skill_detail_impl(&pool, "detail-skill").await.unwrap();
        let collection_names: Vec<&str> =
            detail.collections.iter().map(|c| c.name.as_str()).collect();

        assert_eq!(collection_names, vec!["Alpha", "Beta"]);
    }

    #[tokio::test]
    async fn test_get_skill_detail_not_found() {
        let pool = setup_test_db().await;
        let result = get_skill_detail_impl(&pool, "nonexistent").await;
        assert!(result.is_err(), "should error for unknown skill_id");
    }

    // ── read_skill_content ────────────────────────────────────────────────────

    #[tokio::test]
    async fn test_read_skill_content_returns_file_content() {
        let tmp = TempDir::new().unwrap();
        let pool = setup_test_db().await;

        let skill_dir = tmp.path().join("my-skill");
        fs::create_dir_all(&skill_dir).unwrap();
        let skill_md_path = skill_dir.join("SKILL.md");
        let expected_content = "---\nname: My Skill\n---\n\n# My Skill\n\nContent here.";
        fs::write(&skill_md_path, expected_content).unwrap();

        let skill = Skill {
            id: "my-skill".to_string(),
            name: "My Skill".to_string(),
            description: None,
            file_path: skill_md_path.to_string_lossy().into_owned(),
            canonical_path: None,
            is_central: false,
            source: None,
            content: None,
            scanned_at: Utc::now().to_rfc3339(),
        };
        db::upsert_skill(&pool, &skill).await.unwrap();

        let content = read_skill_content_impl(&pool, "my-skill").await.unwrap();
        assert_eq!(content, expected_content);
    }

    #[tokio::test]
    async fn test_read_skill_content_file_not_found() {
        let pool = setup_test_db().await;

        let skill = Skill {
            id: "missing-file-skill".to_string(),
            name: "Missing File".to_string(),
            description: None,
            file_path: "/nonexistent/SKILL.md".to_string(),
            canonical_path: None,
            is_central: false,
            source: None,
            content: None,
            scanned_at: Utc::now().to_rfc3339(),
        };
        db::upsert_skill(&pool, &skill).await.unwrap();

        let result = read_skill_content_impl(&pool, "missing-file-skill").await;
        assert!(result.is_err(), "should error when file does not exist");
    }

    // ── Testable core implementations (without Tauri State) ───────────────────

    async fn get_central_skills_impl(pool: &SqlitePool) -> Result<Vec<SkillWithLinks>, String> {
        let skills = db::get_central_skills(pool).await?;
        let mut result = Vec::with_capacity(skills.len());
        for skill in skills {
            let installations = db::get_skill_installations(pool, &skill.id).await?;
            let linked_agents: Vec<String> =
                installations.into_iter().map(|i| i.agent_id).collect();
            let read_only_agents =
                read_only_agent_ids_for_skill(pool, &skill.id, skill.is_central).await?;
            let (created_at, updated_at) = skill_filesystem_timestamps(&skill);
            let shared_agents = shared_agent_ids_for_skill(pool, &skill.id, skill.is_central).await?;
            result.push(SkillWithLinks {
                github_stars: None,
                id: skill.id,
                name: skill.name,
                description: skill.description,
                file_path: skill.file_path,
                canonical_path: skill.canonical_path,
                is_central: skill.is_central,
                source: skill.source,
                source_url: None,
                source_author: None,
                source_repo: None,
                source_path: None,
                notes: None,
                tags: Vec::new(),
                scanned_at: skill.scanned_at,
                created_at,
                updated_at,
                linked_agents,
                shared_agents,
                read_only_agents,
            });
        }
        Ok(result)
    }

    async fn get_skill_detail_impl(
        pool: &SqlitePool,
        skill_id: &str,
    ) -> Result<SkillDetail, String> {
        super::get_skill_detail_with_row_impl(pool, skill_id, None, None).await
    }

    async fn read_skill_content_impl(pool: &SqlitePool, skill_id: &str) -> Result<String, String> {
        let skill = db::get_skill_by_id(pool, skill_id)
            .await?
            .ok_or_else(|| format!("Skill '{}' not found", skill_id))?;
        std::fs::read_to_string(&skill.file_path)
            .map_err(|e| format!("Failed to read '{}': {}", skill.file_path, e))
    }

    // ── Regression: get_skills_by_agent_impl returns installation metadata ─────

    /// `get_skills_by_agent_impl` must return `SkillForAgent` objects that
    /// include `link_type`, `dir_path`, and `symlink_target` from the
    /// installation record so the frontend `SkillCard` can show the correct
    /// source indicator.
    #[tokio::test]
    async fn test_get_skills_by_agent_impl_includes_installation_metadata() {
        let pool = setup_test_db().await;

        let skill = make_skill("meta-skill", "Meta Skill", false);
        db::upsert_skill(&pool, &skill).await.unwrap();

        db::upsert_skill_installation(
            &pool,
            &SkillInstallation {
                skill_id: "meta-skill".to_string(),
                agent_id: "claude-code".to_string(),
                installed_path: "/tmp/claude/meta-skill".to_string(),
                link_type: "symlink".to_string(),
                symlink_target: Some("/tmp/central/meta-skill".to_string()),
                created_at: Utc::now().to_rfc3339(),
            },
        )
        .await
        .unwrap();

        let skills = get_skills_by_agent_impl(&pool, "claude-code")
            .await
            .unwrap();
        assert_eq!(skills.len(), 1, "should find one skill for claude-code");

        let s = &skills[0];
        assert_eq!(s.id, "meta-skill");
        assert_eq!(s.installation_source, "independent");
        assert_eq!(
            s.link_type, "symlink",
            "link_type must come from installation record"
        );
        assert_eq!(
            s.dir_path, "/tmp/claude/meta-skill",
            "dir_path must be installed_path from installation record"
        );
        assert_eq!(
            s.symlink_target.as_deref(),
            Some("/tmp/central/meta-skill"),
            "symlink_target must be forwarded from installation record"
        );
        assert_eq!(
            s.source.as_deref(),
            Some("copy"),
            "logical skill source must be forwarded independently of link type"
        );
    }

    #[tokio::test]
    async fn test_get_skills_by_agent_impl_empty_for_unknown_agent() {
        let pool = setup_test_db().await;
        let skills = get_skills_by_agent_impl(&pool, "nobody").await.unwrap();
        assert!(
            skills.is_empty(),
            "no skills for an agent with no installations"
        );
    }

    #[tokio::test]
    async fn test_get_skills_by_agent_impl_returns_central_skills_for_shared_central_root() {
        let tmp = TempDir::new().unwrap();
        let pool = setup_test_db().await;
        let central_root = tmp.path().join(".agents").join("skills");
        let skill_dir = central_root.join("shared-skill");
        fs::create_dir_all(&skill_dir).unwrap();
        fs::write(skill_dir.join("SKILL.md"), "# Shared Skill\n").unwrap();

        sqlx::query(
            "UPDATE agents SET global_skills_dir = ? WHERE id IN ('central', 'antigravity')",
        )
        .bind(central_root.to_string_lossy().to_string())
        .execute(&pool)
        .await
        .unwrap();

        let skill = make_skill_with_path(
            "shared-skill",
            "Shared Skill",
            &skill_dir.join("SKILL.md"),
            &skill_dir,
            true,
        );
        db::upsert_skill(&pool, &skill).await.unwrap();

        let skills = get_skills_by_agent_impl(&pool, "antigravity")
            .await
            .unwrap();

        assert_eq!(skills.len(), 1);
        assert_eq!(skills[0].id, "shared-skill");
        db::upsert_skill_installation(&pool, &db::SkillInstallation {
            skill_id: "shared-skill".into(), agent_id: "antigravity".into(),
            installed_path: skill_dir.to_string_lossy().into_owned(), link_type: "copy".into(),
            symlink_target: None, created_at: Utc::now().to_rfc3339(),
        }).await.unwrap();
        assert!(shared_agent_ids_for_skill(&pool, "shared-skill", true).await.unwrap().contains(&"antigravity".to_string()));
        assert!(!read_only_agent_ids_for_skill(&pool, "shared-skill", true).await.unwrap().contains(&"antigravity".to_string()));
        assert_eq!(skills[0].link_type, "native");
        assert_eq!(skills[0].source_kind.as_deref(), Some("shared-central"));
        assert_eq!(skills[0].installation_source, "shared");
        assert_eq!(
            skills[0].dir_path,
            skill_dir.to_string_lossy(),
            "shared Central Skills must use the real central skill directory"
        );
    }

    #[tokio::test]
    async fn test_get_skill_detail_with_row_impl_opens_shared_central_platform_row() {
        let tmp = TempDir::new().unwrap();
        let pool = setup_test_db().await;
        let central_root = tmp.path().join(".agents").join("skills");
        let skill_dir = central_root.join("canvas-design");
        fs::create_dir_all(&skill_dir).unwrap();
        fs::write(skill_dir.join("SKILL.md"), "# Canvas Design\n").unwrap();

        sqlx::query(
            "UPDATE agents SET global_skills_dir = ? WHERE id IN ('central', 'antigravity')",
        )
        .bind(central_root.to_string_lossy().to_string())
        .execute(&pool)
        .await
        .unwrap();

        let skill = make_skill_with_path(
            "canvas-design",
            "Canvas Design",
            &skill_dir.join("SKILL.md"),
            &skill_dir,
            true,
        );
        db::upsert_skill(&pool, &skill).await.unwrap();

        let detail = get_skill_detail_with_row_impl(
            &pool,
            "canvas-design",
            Some("antigravity"),
            Some("canvas-design"),
        )
        .await
        .unwrap();

        assert_eq!(detail.row_id, "canvas-design");
        assert_eq!(detail.id, "canvas-design");
        assert_eq!(detail.dir_path, skill_dir.to_string_lossy());
        assert!(detail.is_central);
        assert!(detail.source_kind.is_none());
    }

    #[tokio::test]
    async fn test_get_skill_detail_for_central_symlink_uses_resource_directory() {
        let tmp = TempDir::new().unwrap();
        let pool = setup_test_db().await;
        let central_root = tmp.path().join(".agents").join("skills");
        let resource_dir = tmp.path().join("resource-library").join("canvas-design");
        let central_link = central_root.join("canvas-design");
        fs::create_dir_all(&central_root).unwrap();
        fs::create_dir_all(&resource_dir).unwrap();
        fs::write(resource_dir.join("SKILL.md"), "# Canvas Design\n").unwrap();
        create_symlink(&resource_dir, &central_link).unwrap();

        sqlx::query("UPDATE agents SET global_skills_dir = ? WHERE id = 'central'")
            .bind(central_root.to_string_lossy().to_string())
            .execute(&pool)
            .await
            .unwrap();

        let skill = make_skill_with_path(
            "canvas-design",
            "Canvas Design",
            &central_link.join("SKILL.md"),
            &central_link,
            true,
        );
        db::upsert_skill(&pool, &skill).await.unwrap();

        let detail = get_skill_detail_impl(&pool, "canvas-design").await.unwrap();

        assert_eq!(detail.dir_path, resource_dir.to_string_lossy());
        assert_eq!(
            detail.file_path,
            resource_dir.join("SKILL.md").to_string_lossy()
        );
        let central_link_string = central_link.to_string_lossy().to_string();
        assert_eq!(
            detail.canonical_path.as_deref(),
            Some(central_link_string.as_str())
        );
        assert!(detail.is_central);
    }

    #[tokio::test]
    async fn test_get_skill_detail_for_platform_symlink_uses_resource_directory() {
        let tmp = TempDir::new().unwrap();
        let pool = setup_test_db().await;
        let platform_root = tmp.path().join(".cursor").join("skills");
        let resource_dir = tmp.path().join("resource-library").join("frontend-design");
        let platform_link = platform_root.join("frontend-design");
        fs::create_dir_all(&platform_root).unwrap();
        fs::create_dir_all(&resource_dir).unwrap();
        fs::write(resource_dir.join("SKILL.md"), "# Frontend Design\n").unwrap();
        let central_root = tmp.path().join(".agents").join("skills");
        fs::create_dir_all(&central_root).unwrap();
        let central_link = central_root.join("frontend-design");
        create_symlink(&resource_dir, &central_link).unwrap();
        let relative_target = PathBuf::from("..").join("..").join(".agents").join("skills").join("frontend-design");
        create_symlink(&relative_target, &platform_link).unwrap();

        let skill = make_skill_with_path(
            "frontend-design",
            "Frontend Design",
            &resource_dir.join("SKILL.md"),
            &resource_dir,
            false,
        );
        db::upsert_skill(&pool, &skill).await.unwrap();
        db::upsert_skill_installation(
            &pool,
            &SkillInstallation {
                skill_id: "frontend-design".to_string(),
                agent_id: "cursor".to_string(),
                installed_path: platform_link.to_string_lossy().into_owned(),
                link_type: "symlink".to_string(),
                symlink_target: Some(relative_target.to_string_lossy().into_owned()),
                created_at: Utc::now().to_rfc3339(),
            },
        )
        .await
        .unwrap();

        let detail = get_skill_detail_with_row_impl(
            &pool,
            "frontend-design",
            Some("cursor"),
            Some("frontend-design"),
        )
        .await
        .unwrap();

        assert_eq!(detail.dir_path, resource_dir.to_string_lossy());
        assert_eq!(
            detail.file_path,
            resource_dir.join("SKILL.md").to_string_lossy()
        );
        assert_eq!(
            detail.canonical_path.as_deref(),
            Some(platform_link.to_string_lossy().as_ref())
        );
        assert!(!detail.is_central);
    }

    #[tokio::test]
    async fn test_get_skills_by_agent_impl_detects_shared_central_symlink() {
        let tmp = TempDir::new().unwrap();
        let pool = setup_test_db().await;
        let central_root = tmp.path().join(".agents").join("skills");
        let resource_dir = tmp.path().join("resource-library").join("shared-skill");
        let skill_dir = central_root.join("shared-skill");
        fs::create_dir_all(&central_root).unwrap();
        fs::create_dir_all(&resource_dir).unwrap();
        fs::write(resource_dir.join("SKILL.md"), "# Shared Skill\n").unwrap();
        create_symlink(&resource_dir, &skill_dir).unwrap();

        sqlx::query(
            "UPDATE agents SET global_skills_dir = ? WHERE id IN ('central', 'antigravity')",
        )
        .bind(central_root.to_string_lossy().to_string())
        .execute(&pool)
        .await
        .unwrap();

        let skill = make_skill_with_path(
            "shared-skill",
            "Shared Skill",
            &skill_dir.join("SKILL.md"),
            &skill_dir,
            true,
        );
        db::upsert_skill(&pool, &skill).await.unwrap();

        let skills = get_skills_by_agent_impl(&pool, "antigravity")
            .await
            .unwrap();

        assert_eq!(skills.len(), 1);
        assert_eq!(skills[0].link_type, "symlink");
        assert!(
            skills[0].symlink_target.is_some(),
            "shared Central Skills should expose the resource library symlink target"
        );
    }

    #[tokio::test]
    async fn test_get_skills_by_agent_impl_excludes_generic_read_only_observations() {
        let pool = setup_test_db().await;

        let skill = make_skill("shared-skill", "Shared Skill", true);
        db::upsert_skill(&pool, &skill).await.unwrap();
        db::upsert_agent_skill_observation(
            &pool,
            &make_observation_for_agent(
                "factory-droid",
                "factory-droid::/tmp/.agents/skills/shared-skill",
                "shared-skill",
                "Shared Skill",
                "/tmp/.agents/skills/shared-skill",
                "compatibility",
                "/tmp/.agents/skills",
                true,
            ),
        )
        .await
        .unwrap();

        let skills = get_skills_by_agent_impl(&pool, "factory-droid")
            .await
            .unwrap();
        assert!(
            skills.is_empty(),
            "compatibility observations must stay out of the platform management list"
        );
    }

    #[tokio::test]
    async fn test_get_skills_by_agent_impl_prefers_manageable_install_over_read_only_observation() {
        let pool = setup_test_db().await;

        let skill = make_skill("shared-skill", "Shared Skill", false);
        db::upsert_skill(&pool, &skill).await.unwrap();
        db::upsert_skill_installation(
            &pool,
            &SkillInstallation {
                skill_id: "shared-skill".to_string(),
                agent_id: "factory-droid".to_string(),
                installed_path: "/tmp/.factory/skills/shared-skill".to_string(),
                link_type: "copy".to_string(),
                symlink_target: None,
                created_at: Utc::now().to_rfc3339(),
            },
        )
        .await
        .unwrap();
        db::upsert_agent_skill_observation(
            &pool,
            &make_observation_for_agent(
                "factory-droid",
                "factory-droid::/tmp/.agents/skills/shared-skill",
                "shared-skill",
                "Shared Skill",
                "/tmp/.agents/skills/shared-skill",
                "compatibility",
                "/tmp/.agents/skills",
                true,
            ),
        )
        .await
        .unwrap();

        let skills = get_skills_by_agent_impl(&pool, "factory-droid")
            .await
            .unwrap();
        assert_eq!(skills.len(), 1);
        assert_eq!(skills[0].id, "shared-skill");
        assert!(!skills[0].is_read_only);
        assert_eq!(skills[0].dir_path, "/tmp/.factory/skills/shared-skill");
        assert!(skills[0].source_kind.is_none());
    }

    #[tokio::test]
    async fn test_get_skill_detail_with_row_impl_prefers_managed_install_for_skill_row_id() {
        let pool = setup_test_db().await;

        let skill = make_skill("shared-skill", "Shared Skill", false);
        db::upsert_skill(&pool, &skill).await.unwrap();
        db::upsert_skill_installation(
            &pool,
            &SkillInstallation {
                skill_id: "shared-skill".to_string(),
                agent_id: "claude-code".to_string(),
                installed_path: "/tmp/.claude/skills/shared-skill".to_string(),
                link_type: "copy".to_string(),
                symlink_target: None,
                created_at: Utc::now().to_rfc3339(),
            },
        )
        .await
        .unwrap();
        db::upsert_agent_skill_observation(
            &pool,
            &make_observation_for_agent(
                "claude-code",
                "claude-code::/tmp/.agents/skills/shared-skill",
                "shared-skill",
                "Shared Skill",
                "/tmp/.agents/skills/shared-skill",
                "compatibility",
                "/tmp/.agents/skills",
                true,
            ),
        )
        .await
        .unwrap();

        let detail = get_skill_detail_with_row_impl(
            &pool,
            "shared-skill",
            Some("claude-code"),
            Some("shared-skill"),
        )
        .await
        .unwrap();

        assert_eq!(detail.row_id, "shared-skill");
        assert_eq!(detail.dir_path, "/tmp/shared-skill");
        assert!(detail.source_kind.is_none());
        assert!(!detail.is_read_only);
        assert_eq!(detail.installations.len(), 1);
    }

    #[tokio::test]
    async fn test_get_skill_detail_with_row_impl_factory_compatibility_row_is_read_only() {
        let pool = setup_test_db().await;

        let skill = make_skill("shared-skill", "Shared Skill", true);
        db::upsert_skill(&pool, &skill).await.unwrap();
        db::upsert_agent_skill_observation(
            &pool,
            &make_observation_for_agent(
                "factory-droid",
                "factory-droid::/tmp/.agents/skills/shared-skill",
                "shared-skill",
                "Shared Skill",
                "/tmp/.agents/skills/shared-skill",
                "compatibility",
                "/tmp/.agents/skills",
                true,
            ),
        )
        .await
        .unwrap();

        let detail = get_skill_detail_with_row_impl(
            &pool,
            "shared-skill",
            Some("factory-droid"),
            Some("factory-droid::/tmp/.agents/skills/shared-skill"),
        )
        .await
        .unwrap();

        assert_eq!(
            detail.row_id,
            "factory-droid::/tmp/.agents/skills/shared-skill"
        );
        assert_eq!(detail.source_kind.as_deref(), Some("compatibility"));
        assert_eq!(detail.source_root.as_deref(), Some("/tmp/.agents/skills"));
        assert!(detail.is_read_only);
        assert!(
            detail.installations.is_empty(),
            "Factory .agents compatibility rows must not expose removable install records"
        );
        assert!(
            detail.collections.is_empty(),
            "read-only compatibility rows should not expose collection mutation state"
        );
    }

    #[tokio::test]
    async fn test_get_skills_by_agent_impl_copy_link_type() {
        let pool = setup_test_db().await;

        let skill = make_skill("copy-skill", "Copy Skill", false);
        db::upsert_skill(&pool, &skill).await.unwrap();

        db::upsert_skill_installation(
            &pool,
            &SkillInstallation {
                skill_id: "copy-skill".to_string(),
                agent_id: "cursor".to_string(),
                installed_path: "/tmp/cursor/copy-skill".to_string(),
                link_type: "copy".to_string(),
                symlink_target: None,
                created_at: Utc::now().to_rfc3339(),
            },
        )
        .await
        .unwrap();

        let skills = get_skills_by_agent_impl(&pool, "cursor").await.unwrap();
        assert_eq!(skills.len(), 1);
        assert_eq!(skills[0].link_type, "copy");
        assert!(
            skills[0].symlink_target.is_none(),
            "copy skills have no symlink target"
        );
    }

    // ── read_file_by_path ──────────────────────────────────────────────────────

    #[tokio::test]
    async fn test_read_file_by_path_success() {
        let tmp = TempDir::new().unwrap();
        let file_path = tmp.path().join("test-skill.md");
        let content = "---\nname: Test\n---\n\n# Test Skill";
        fs::write(&file_path, content).unwrap();

        let result = read_file_by_path(file_path.to_string_lossy().into_owned()).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), content);
    }

    #[tokio::test]
    async fn test_read_file_by_path_not_found() {
        let result = read_file_by_path("/nonexistent/file.md".to_string()).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_list_skill_directory_returns_nested_sorted_tree() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path().join("frontend-design");
        let docs_dir = root.join("docs");
        let nested_dir = docs_dir.join("guides");

        fs::create_dir_all(&nested_dir).unwrap();
        fs::write(root.join("SKILL.md"), "# Skill").unwrap();
        fs::write(root.join("notes.txt"), "notes").unwrap();
        fs::write(docs_dir.join("README.md"), "# Docs").unwrap();
        fs::write(nested_dir.join("tips.md"), "# Tips").unwrap();

        let nodes = list_skill_directory(root.to_string_lossy().into_owned())
            .await
            .unwrap();

        assert_eq!(nodes.len(), 3);
        assert_eq!(nodes[0].name, "docs");
        assert!(nodes[0].is_dir);
        assert_eq!(nodes[0].relative_path, "docs");
        assert_eq!(nodes[0].children.len(), 2);
        assert_eq!(nodes[0].children[0].name, "guides");
        assert!(nodes[0].children[0].is_dir);
        assert_eq!(
            nodes[0].children[0].children[0].relative_path,
            "docs/guides/tips.md"
        );
        assert_eq!(nodes[1].name, "notes.txt");
        assert!(!nodes[1].is_dir);
        assert_eq!(nodes[2].name, "SKILL.md");
        assert!(!nodes[2].is_dir);
    }

    #[tokio::test]
    async fn test_list_skill_directory_rejects_missing_path() {
        let result = list_skill_directory("/nonexistent/directory".to_string()).await;
        assert!(result.is_err());
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn test_list_skill_directory_skips_recursive_directory_symlink() {
        use std::os::unix::fs::symlink;

        let tmp = TempDir::new().unwrap();
        let root = tmp.path().join("planning-with-files-zh");
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("SKILL.md"), "# Skill").unwrap();
        symlink(&root, root.join("planning-with-files-zh")).unwrap();

        let nodes = list_skill_directory(root.to_string_lossy().into_owned())
            .await
            .unwrap();

        assert_eq!(nodes.len(), 1);
        assert_eq!(nodes[0].name, "SKILL.md");
    }

    // ── open_in_file_manager ───────────────────────────────────────────────────

    #[tokio::test]
    async fn test_open_in_file_manager_nonexistent_path() {
        let result =
            open_in_file_manager("/nonexistent/path/that/does/not/exist".to_string()).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_open_in_file_manager_rejects_empty_path() {
        let result = open_in_file_manager("   ".to_string()).await;
        assert!(result.is_err());
    }
}
