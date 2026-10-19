use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};
use tauri::State;

use super::agents::is_agent_detected;
use crate::db::{self, AppStore, ScanDirectory, SkillInstallation};
use crate::path_utils::{
    path_to_string, prune_empty_parent_dirs, remove_path_if_no_skill_markdown, remove_symlink_path,
};
use crate::AppState;

// ─── Types ────────────────────────────────────────────────────────────────────

/// Result of a single skill install operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstallResult {
    pub symlink_path: String,
}

/// Result of a batch install across multiple agents.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchInstallResult {
    pub succeeded: Vec<String>,
    pub failed: Vec<FailedInstall>,
}

/// Describes a single failed install within a batch operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FailedInstall {
    pub agent_id: String,
    pub error: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddResourceSkillToCentralResult {
    pub skill_id: String,
    pub central_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoveResourceSkillFromCentralResult {
    pub skill_id: String,
    pub removed_central_path: String,
    pub resource_path: String,
    pub uninstalled_agents: Vec<String>,
}

pub(crate) const PROJECT_AGENT_PREFIX: &str = "project:";
pub(crate) const PROJECT_SKILLS_SUBDIR: &str = ".agents/skills";

#[derive(Debug, Clone)]
pub(crate) struct InstallTarget {
    pub(crate) id: String,
    pub(crate) display_name: String,
    pub(crate) skills_dir: PathBuf,
    pub(crate) supports_universal_availability: bool,
}

pub(crate) fn project_agent_id(scan_directory_id: i64) -> String {
    format!("{PROJECT_AGENT_PREFIX}{scan_directory_id}")
}

pub(crate) fn parse_project_agent_id(agent_id: &str) -> Option<i64> {
    agent_id
        .strip_prefix(PROJECT_AGENT_PREFIX)
        .and_then(|id| id.parse::<i64>().ok())
}

pub(crate) fn project_skills_dir_for_scan_directory(scan_dir: &ScanDirectory) -> PathBuf {
    PathBuf::from(&scan_dir.path).join(PROJECT_SKILLS_SUBDIR)
}

pub(crate) async fn project_target_for_scan_directory(
    scan_dir: &ScanDirectory,
) -> Result<Option<InstallTarget>, String> {
    if scan_dir.is_builtin || !scan_dir.is_active {
        return Ok(None);
    }

    let project_root = PathBuf::from(&scan_dir.path);
    if !project_root.exists() {
        return Ok(None);
    }

    Ok(Some(InstallTarget {
        id: project_agent_id(scan_dir.id),
        display_name: scan_dir
            .label
            .clone()
            .unwrap_or_else(|| scan_dir.path.clone()),
        skills_dir: project_skills_dir_for_scan_directory(scan_dir),
        supports_universal_availability: false,
    }))
}

async fn install_target_for_agent_id(
    pool: &AppStore,
    agent_id: &str,
) -> Result<InstallTarget, String> {
    if let Some(scan_directory_id) = parse_project_agent_id(agent_id) {
        let scan_dir = db::get_scan_directory_by_id(pool, scan_directory_id)
            .await?
            .ok_or_else(|| format!("Project directory target '{}' not found", agent_id))?;
        return project_target_for_scan_directory(&scan_dir)
            .await?
            .ok_or_else(|| {
                format!(
                    "Project directory target '{}' is not active or does not exist",
                    agent_id
                )
            });
    }

    let agent = db::get_agent_by_id(pool, agent_id)
        .await?
        .ok_or_else(|| format!("Agent '{}' not found", agent_id))?;

    Ok(InstallTarget {
        id: agent.id,
        display_name: agent.display_name,
        skills_dir: PathBuf::from(agent.global_skills_dir),
        supports_universal_availability: true,
    })
}

pub(crate) async fn agent_skills_dir_shares_central(
    pool: &AppStore,
    agent_id: &str,
) -> Result<bool, String> {
    let target = install_target_for_agent_id(pool, agent_id).await?;
    let central_root = db::get_agent_by_id(pool, "central")
        .await?
        .ok_or_else(|| "Central agent not found in database".to_string())?
        .global_skills_dir;
    Ok(resolved_path(&target.skills_dir) == resolved_path(Path::new(&central_root)))
}

// ─── Path Utilities ───────────────────────────────────────────────────────────

/// Compute a relative path from `from_dir` to `to_path`.
///
/// Both paths must be absolute. The resulting path can be used as a symlink
/// target placed inside `from_dir`.
///
/// Examples:
/// - `make_relative_path("/a/b/c", "/a/d/e/f")` -> `"../../d/e/f"`
/// - `make_relative_path("/home/user/.claude/skills", "/home/user/.agents/skills/my-skill")`
///   -> `"../../.agents/skills/my-skill"`
pub fn make_relative_path(from_dir: &Path, to_path: &Path) -> PathBuf {
    let from_components: Vec<_> = from_dir.components().collect();
    let to_components: Vec<_> = to_path.components().collect();

    // Find the length of the common path prefix.
    let common_len = from_components
        .iter()
        .zip(to_components.iter())
        .take_while(|(a, b)| a == b)
        .count();

    // Number of ".." hops needed to climb out of `from_dir`.
    let up_count = from_components.len() - common_len;

    let mut result = PathBuf::new();
    for _ in 0..up_count {
        result.push("..");
    }
    for component in &to_components[common_len..] {
        result.push(component.as_os_str());
    }

    if result.as_os_str().is_empty() {
        PathBuf::from(".")
    } else {
        result
    }
}

// ─── Platform-specific symlink creation ──────────────────────────────────────

#[cfg(unix)]
pub fn create_symlink(target: &Path, link: &Path) -> Result<(), String> {
    std::os::unix::fs::symlink(target, link).map_err(|e| format!("Failed to create symlink: {}", e))
}

#[cfg(windows)]
pub fn create_symlink(target: &Path, link: &Path) -> Result<(), String> {
    std::os::windows::fs::symlink_dir(target, link)
        .map_err(|e| format!("Failed to create symlink: {}", e))
}

#[cfg(not(any(unix, windows)))]
pub fn create_symlink(_target: &Path, _link: &Path) -> Result<(), String> {
    Err("Symlink creation is only supported on Unix systems".to_string())
}

pub fn symlink_target_path(from_dir: &Path, to_path: &Path) -> PathBuf {
    #[cfg(windows)]
    {
        let from_prefix = from_dir.components().next();
        let to_prefix = to_path.components().next();
        if from_prefix != to_prefix {
            return to_path.to_path_buf();
        }
    }

    make_relative_path(from_dir, to_path)
}

fn relative_skill_path_for_install(skill_id: &str) -> PathBuf {
    PathBuf::from(skill_id)
}

fn cleanup_replaced_installation_path(
    installation: &SkillInstallation,
    next_path: &Path,
    install_root: &Path,
) -> Result<(), String> {
    let previous_path = PathBuf::from(&installation.installed_path);
    let previous_path_display = path_to_string(&previous_path);
    if previous_path == next_path && installation.link_type != "copy" {
        return Ok(());
    }
    if !previous_path.exists() {
        return Ok(());
    }

    let metadata = std::fs::symlink_metadata(&previous_path).map_err(|e| {
        format!(
            "Failed to inspect previous installation '{}': {}",
            previous_path_display, e
        )
    })?;

    if metadata.file_type().is_symlink() {
        remove_symlink_path(&previous_path).map_err(|e| {
            format!(
                "Failed to remove previous symlink installation '{}': {}",
                previous_path_display, e
            )
        })?;
    } else if metadata.is_dir() && installation.link_type == "copy" {
        std::fs::remove_dir_all(&previous_path).map_err(|e| {
            format!(
                "Failed to remove previous copied installation '{}': {}",
                previous_path_display, e
            )
        })?;
    } else if metadata.is_dir()
        && is_safe_managed_installation_dir(installation, &previous_path, install_root)
    {
        std::fs::remove_dir_all(&previous_path).map_err(|e| {
            format!(
                "Failed to remove previous managed installation '{}': {}",
                previous_path_display, e
            )
        })?;
        let _ = prune_empty_parent_dirs(&previous_path, install_root);
    } else {
        return Err(format!(
            "Previous installation path '{}' is not safely removable",
            previous_path_display
        ));
    }

    Ok(())
}

fn is_safe_managed_installation_dir(
    installation: &SkillInstallation,
    path: &Path,
    install_root: &Path,
) -> bool {
    let resolved_install_path = resolved_symlink_entry(path);
    let resolved_root = resolved_path(install_root);
    if resolved_install_path == resolved_root || !resolved_install_path.starts_with(&resolved_root)
    {
        return false;
    }
    if path.file_name().and_then(|name| name.to_str()) != Some(installation.skill_id.as_str()) {
        return false;
    }
    path.join("SKILL.md").exists()
}

// ─── Recursive Directory Copy ─────────────────────────────────────────────────

/// Recursively copy a directory tree from `src` to `dst`.
///
/// `dst` must not exist prior to the call (or may be an empty dir).
/// The behaviour mirrors `cp -r src dst` on Unix.
pub fn copy_dir_all(src: &Path, dst: &Path) -> Result<(), String> {
    std::fs::create_dir_all(dst).map_err(|e| {
        format!(
            "Failed to create destination directory '{}': {}",
            dst.display(),
            e
        )
    })?;

    for entry in std::fs::read_dir(src)
        .map_err(|e| format!("Failed to read source directory '{}': {}", src.display(), e))?
    {
        let entry = entry.map_err(|e| format!("Failed to read directory entry: {}", e))?;
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());

        let file_type = entry
            .file_type()
            .map_err(|e| format!("Failed to determine file type: {}", e))?;

        if file_type.is_dir() {
            copy_dir_all(&src_path, &dst_path)?;
        } else {
            std::fs::copy(&src_path, &dst_path).map_err(|e| {
                format!(
                    "Failed to copy '{}' -> '{}': {}",
                    src_path.display(),
                    dst_path.display(),
                    e
                )
            })?;
        }
    }

    Ok(())
}

fn skill_dir_from_file_path(file_path: &str) -> String {
    Path::new(file_path)
        .parent()
        .map(|path| path.to_string_lossy().into_owned())
        .unwrap_or_else(|| file_path.to_string())
}

fn symlink_target_entry(path: &Path) -> Option<PathBuf> {
    let target = std::fs::read_link(path).ok()?;
    let absolute_target = if target.is_absolute() {
        target
    } else {
        path.parent().unwrap_or_else(|| Path::new("")).join(target)
    };
    Some(resolved_path(&absolute_target))
}

fn resource_dir_for_central_entry(
    central_dir: &Path,
    central_relative_path: &Path,
    resource_root: &Path,
) -> PathBuf {
    let resolved_resource_root = resolved_path(resource_root);
    if let Some(target) = symlink_target_entry(central_dir) {
        if target.starts_with(&resolved_resource_root) && target.join("SKILL.md").exists() {
            return target;
        }
    }
    resource_root.join(central_relative_path)
}

pub async fn add_resource_skill_to_central_impl(
    pool: &AppStore,
    skill_id: &str,
) -> Result<AddResourceSkillToCentralResult, String> {
    let mut skill = db::get_skill_by_id(pool, skill_id)
        .await?
        .ok_or_else(|| format!("Skill '{}' not found in database", skill_id))?;

    if skill.is_central {
        let central_path = skill
            .canonical_path
            .clone()
            .unwrap_or_else(|| skill_dir_from_file_path(&skill.file_path));
        let central_root = db::get_agent_by_id(pool, "central")
            .await?
            .ok_or_else(|| "Central Skills agent not found".to_string())?
            .global_skills_dir;
        sync_central_skill_to_platforms(pool, skill_id, Path::new(&central_root)).await?;
        return Ok(AddResourceSkillToCentralResult {
            skill_id: skill_id.to_string(),
            central_path,
        });
    }

    let central_agent = db::get_agent_by_id(pool, "central")
        .await?
        .ok_or_else(|| "Central Skills agent not found".to_string())?;
    let central_root = PathBuf::from(central_agent.global_skills_dir);

    let source_dir = skill
        .canonical_path
        .as_ref()
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(skill_dir_from_file_path(&skill.file_path)));
    if !source_dir.join("SKILL.md").exists() {
        return Err(format!(
            "Skill source '{}' does not contain SKILL.md",
            source_dir.display()
        ));
    }

    let central_dir = central_root.join(relative_skill_path_for_install(skill_id));
    if central_dir != source_dir && central_dir.exists() {
        return Err(format!(
            "Central skill target already exists: {}",
            central_dir.display()
        ));
    }

    if central_dir != source_dir {
        if let Some(parent) = central_dir.parent() {
            std::fs::create_dir_all(parent).map_err(|error| {
                format!(
                    "Failed to create Central Skills parent directory '{}': {}",
                    parent.display(),
                    error
                )
            })?;
        }
        let from_dir = central_dir.parent().unwrap_or(&central_root);
        let relative_target = symlink_target_path(from_dir, &source_dir);
        create_symlink(&relative_target, &central_dir)?;
    }

    skill.canonical_path = Some(path_to_string(&central_dir));
    skill.file_path = path_to_string(&central_dir.join("SKILL.md"));
    skill.is_central = true;
    db::upsert_skill(pool, &skill).await?;

    sync_central_skill_to_platforms(pool, skill_id, &central_root).await?;

    Ok(AddResourceSkillToCentralResult {
        skill_id: skill_id.to_string(),
        central_path: path_to_string(&central_dir),
    })
}

pub async fn remove_resource_skill_from_central_impl(
    pool: &AppStore,
    skill_id: &str,
) -> Result<RemoveResourceSkillFromCentralResult, String> {
    let mut skill = db::get_skill_by_id(pool, skill_id)
        .await?
        .ok_or_else(|| format!("Skill '{}' not found in database", skill_id))?;
    if !skill.is_central {
        return Err(format!("Skill '{}' is not central", skill_id));
    }

    let central_root = PathBuf::from(
        db::get_agent_by_id(pool, "central")
            .await?
            .ok_or_else(|| "Central Skills agent not found".to_string())?
            .global_skills_dir,
    );
    let resource_root = db::get_skill_resource_library_dir(pool).await?;
    let central_dir = PathBuf::from(
        skill
            .canonical_path
            .clone()
            .unwrap_or_else(|| skill_dir_from_file_path(&skill.file_path)),
    );
    let resolved_central_root = resolved_path(&central_root);
    let resolved_central_entry = resolved_symlink_entry(&central_dir);
    let relative_path = resolved_central_entry
        .strip_prefix(&resolved_central_root)
        .map_err(|_| "Central skill path is outside the Central Skills root".to_string())?;
    if relative_path.as_os_str().is_empty()
        || !relative_path
            .components()
            .all(|component| matches!(component, Component::Normal(_)))
    {
        return Err("Central skill relative path is invalid".to_string());
    }

    let resource_dir = resource_dir_for_central_entry(&central_dir, relative_path, &resource_root);
    if !resource_dir.join("SKILL.md").exists() {
        return Err(format!(
            "Original resource skill '{}' no longer exists",
            resource_dir.display()
        ));
    }
    if resolved_symlink_entry(&resource_dir) == resolved_central_entry {
        return Err(
            "Resource library and Central Skills paths resolve to the same directory".to_string(),
        );
    }

    let installations = db::get_skill_installations(pool, skill_id).await?;
    let mut uninstalled_agents = Vec::with_capacity(installations.len());
    for installation in installations {
        uninstall_skill_from_agent_impl(pool, skill_id, &installation.agent_id).await?;
        uninstalled_agents.push(installation.agent_id);
    }

    remove_central_skill_link_or_dir(&central_dir)?;

    skill.file_path = path_to_string(&resource_dir.join("SKILL.md"));
    skill.canonical_path = Some(path_to_string(&resource_dir));
    skill.is_central = false;
    skill.scanned_at = chrono::Utc::now().to_rfc3339();
    db::repair_legacy_centralized_resource_skill(pool, &skill, &path_to_string(&central_dir))
        .await?;

    Ok(RemoveResourceSkillFromCentralResult {
        skill_id: skill_id.to_string(),
        removed_central_path: path_to_string(&central_dir),
        resource_path: path_to_string(&resource_dir),
        uninstalled_agents,
    })
}

async fn sync_central_skill_to_platforms(
    pool: &AppStore,
    skill_id: &str,
    central_root: &Path,
) -> Result<(), String> {
    let mut failures = Vec::new();

    for agent in db::get_all_agents(pool).await? {
        if agent.id == "central"
            || !agent.is_enabled
            || !is_agent_detected(&agent.global_skills_dir)
            || resolved_path(Path::new(&agent.global_skills_dir)) == resolved_path(central_root)
        {
            continue;
        }

        if let Err(error) = install_central_skill_to_agent_for_sync(pool, skill_id, &agent.id).await
        {
            failures.push(format!("{}: {}", agent.display_name, error));
        }
    }

    for scan_dir in db::get_scan_directories(pool).await? {
        let Some(target) = project_target_for_scan_directory(&scan_dir).await? else {
            continue;
        };

        if resolved_path(&target.skills_dir) == resolved_path(central_root) {
            continue;
        }

        if let Err(error) =
            install_central_skill_to_agent_for_sync(pool, skill_id, &target.id).await
        {
            failures.push(format!("{}: {}", target.display_name, error));
        }
    }

    if failures.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "Skill was added to Central Skills, but platform synchronization failed: {}",
            failures.join("; ")
        ))
    }
}

/// Drop Central Skills rows whose directory/link is already gone, and uninstall
/// leftover platform/project copies. This heals the case where a parent folder
/// was deleted without matching the nested symlink skills in the database.
pub(crate) async fn reconcile_orphaned_central_skills(pool: &AppStore) -> Result<(), String> {
    let Some(central) = db::get_agent_by_id(pool, "central").await? else {
        return Ok(());
    };
    let central_root = PathBuf::from(&central.global_skills_dir);
    let resolved_central_root = resolved_path(&central_root);

    for skill in db::get_central_skills(pool).await? {
        let central_dir = skill
            .canonical_path
            .as_deref()
            .map(PathBuf::from)
            .or_else(|| Path::new(&skill.file_path).parent().map(Path::to_path_buf))
            .unwrap_or_else(|| central_root.join(&skill.id));
        let still_in_central = central_dir.join("SKILL.md").exists()
            && resolved_symlink_entry(&central_dir).starts_with(&resolved_central_root);
        if still_in_central {
            continue;
        }

        let _ = remove_dangling_central_symlink(&central_dir);

        for installation in db::get_skill_installations(pool, &skill.id).await? {
            if installation.agent_id == "central" {
                continue;
            }
            let _ = uninstall_skill_from_agent_impl(pool, &skill.id, &installation.agent_id).await;
        }

        match resource_source_dir_for_skill(pool, &skill.id).await {
            Ok(resource_dir) => {
                let mut restored = skill.clone();
                restored.file_path = path_to_string(&resource_dir.join("SKILL.md"));
                restored.canonical_path = Some(path_to_string(&resource_dir));
                restored.is_central = false;
                restored.scanned_at = chrono::Utc::now().to_rfc3339();
                db::repair_legacy_centralized_resource_skill(
                    pool,
                    &restored,
                    &path_to_string(&central_dir),
                )
                .await?;
            }
            Err(_) => {
                db::delete_central_skill_records(pool, &skill.id, &skill.name).await?;
            }
        }
    }

    Ok(())
}

pub(crate) async fn sync_all_central_skills_to_detected_platforms(
    pool: &AppStore,
) -> Result<(), String> {
    let central_root = db::get_agent_by_id(pool, "central")
        .await?
        .ok_or_else(|| "Central Skills agent not found".to_string())?
        .global_skills_dir;
    let central_root = PathBuf::from(central_root);
    let mut failures = Vec::new();

    for skill in db::get_central_skills(pool).await? {
        if let Err(error) = sync_central_skill_to_platforms(pool, &skill.id, &central_root).await {
            failures.push(format!("{}: {}", skill.name, error));
        }
    }

    if failures.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "Central Skills synchronization failed: {}",
            failures.join("; ")
        ))
    }
}

async fn install_source_dir_for_skill(
    pool: &AppStore,
    skill_id: &str,
    central_root: &Path,
) -> Result<PathBuf, String> {
    if let Some(skill) = db::get_skill_by_id(pool, skill_id).await? {
        if let Some(canonical_path) = skill.canonical_path {
            let canonical_dir = PathBuf::from(canonical_path);
            if canonical_dir.join("SKILL.md").exists() {
                return Ok(canonical_dir);
            }
        }

        let source_file = PathBuf::from(&skill.file_path);
        if source_file.exists() {
            return source_file
                .parent()
                .map(Path::to_path_buf)
                .ok_or_else(|| format!("Invalid file_path for skill '{}'", skill_id));
        }
    }

    let central_dir = central_root.join(skill_id);
    if central_dir.join("SKILL.md").exists() {
        return Ok(central_dir);
    }

    Err(format!("Skill source not found for '{}'", skill_id))
}

fn resolved_path(path: &Path) -> PathBuf {
    let resolved = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    #[cfg(windows)]
    {
        let display = resolved.to_string_lossy();
        if let Some(path) = display.strip_prefix(r"\\?\UNC\") {
            return PathBuf::from(format!(r"\\{path}"));
        }
        if let Some(path) = display.strip_prefix(r"\\?\") {
            return PathBuf::from(path);
        }
    }
    resolved
}

/// Canonicalize a path without following the final symlink, so removing a
/// Central Skills link does not resolve into the Resource Library.
fn resolved_symlink_entry(path: &Path) -> PathBuf {
    match path.parent() {
        Some(parent) if parent.as_os_str().is_empty() => path.to_path_buf(),
        Some(parent) => resolved_path(parent).join(path.file_name().unwrap_or_default()),
        None => path.to_path_buf(),
    }
}

fn remove_central_skill_link_or_dir(path: &Path) -> Result<(), String> {
    let metadata = std::fs::symlink_metadata(path).map_err(|error| {
        format!(
            "Failed to read Central Skills path '{}': {}",
            path.display(),
            error
        )
    })?;
    if metadata.file_type().is_symlink() {
        remove_symlink_path(path).map_err(|error| {
            format!(
                "Failed to remove Central Skills symlink '{}': {}",
                path.display(),
                error
            )
        })
    } else if metadata.is_dir() {
        std::fs::remove_dir_all(path).map_err(|error| {
            format!(
                "Failed to remove Central Skills directory '{}': {}",
                path.display(),
                error
            )
        })
    } else {
        Err(format!(
            "Central Skills path '{}' is not a removable skill directory",
            path.display()
        ))
    }
}

fn remove_dangling_central_symlink(path: &Path) -> Result<(), String> {
    let Ok(metadata) = std::fs::symlink_metadata(path) else {
        return Ok(());
    };
    if !metadata.file_type().is_symlink() || std::fs::metadata(path).is_ok() {
        return Ok(());
    }

    remove_symlink_path(path).map_err(|error| {
        format!(
            "Failed to remove dangling Central Skills symlink '{}': {}",
            path.display(),
            error
        )
    })
}

pub(crate) fn validate_platform_install_target(
    agent_id: &str,
    source_dir: &Path,
    agent_dir: &Path,
    central_root: &Path,
) -> Result<(), String> {
    let source_dir = resolved_path(source_dir);
    let agent_dir = resolved_path(agent_dir);
    let central_root = resolved_path(central_root);

    if !source_dir.starts_with(&central_root) && agent_dir.starts_with(&central_root) {
        return Err(format!(
            "Agent '{}' uses the central skills directory; refusing to install a non-central skill there",
            agent_id
        ));
    }

    Ok(())
}

async fn existing_install_path_for_agent(
    pool: &AppStore,
    skill_id: &str,
    agent_id: &str,
) -> Result<Option<String>, String> {
    Ok(existing_installation_for_agent(pool, skill_id, agent_id)
        .await?
        .map(|installation| installation.installed_path))
}

async fn existing_installation_for_agent(
    pool: &AppStore,
    skill_id: &str,
    agent_id: &str,
) -> Result<Option<SkillInstallation>, String> {
    Ok(db::get_skill_installations(pool, skill_id)
        .await?
        .into_iter()
        .find(|installation| installation.agent_id == agent_id))
}

async fn repair_legacy_platform_link_in_central(
    pool: &AppStore,
    skill_id: &str,
    agent_id: &str,
    central_root: &Path,
) -> Result<Option<PathBuf>, String> {
    let Some(mut skill) = db::get_skill_by_id(pool, skill_id)
        .await?
        .filter(|skill| skill.is_central)
    else {
        return Ok(None);
    };
    let Some(legacy_path) = skill.canonical_path.as_deref().map(PathBuf::from) else {
        return Ok(None);
    };
    let Some(installation) = db::get_skill_installations_for_legacy_repair(pool, skill_id)
        .await?
        .into_iter()
        .find(|installation| installation.agent_id == agent_id)
    else {
        return Ok(None);
    };
    let Some(source_dir) = installation.symlink_target.as_deref().map(PathBuf::from) else {
        return Ok(None);
    };

    if installation.link_type != "symlink"
        || Path::new(&installation.installed_path) != legacy_path
        || !legacy_path.starts_with(central_root)
        || source_dir.starts_with(central_root)
        || !source_dir.join("SKILL.md").exists()
    {
        return Ok(None);
    }

    match std::fs::symlink_metadata(&legacy_path) {
        Ok(metadata)
            if metadata.file_type().is_symlink()
                && resolved_path(&legacy_path) == resolved_path(&source_dir) =>
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
        _ => return Ok(None),
    }

    skill.file_path = path_to_string(&source_dir.join("SKILL.md"));
    skill.canonical_path = Some(path_to_string(&source_dir));
    skill.is_central = false;
    skill.scanned_at = chrono::Utc::now().to_rfc3339();
    db::repair_legacy_centralized_resource_skill(pool, &skill, &path_to_string(&legacy_path))
        .await?;

    Ok(Some(source_dir))
}

async fn universal_available_install_result(
    pool: &AppStore,
    skill_id: &str,
    agent_id: &str,
    canonical_dir: &Path,
    central_root: &Path,
) -> Result<Option<InstallResult>, String> {
    if !db::agent_supports_universal_agents_skills(agent_id) {
        return Ok(None);
    }
    if !canonical_dir.starts_with(central_root) {
        return Ok(None);
    }

    let symlink_path = existing_install_path_for_agent(pool, skill_id, agent_id)
        .await?
        .unwrap_or_else(|| path_to_string(canonical_dir));
    Ok(Some(InstallResult { symlink_path }))
}

// ─── Core Logic ───────────────────────────────────────────────────────────────

/// Core install logic, separated from the Tauri layer for testability.
///
/// Creates a relative symlink at `agent.global_skills_dir/<skill_id>` that
/// points to the skill's current source directory.
///
/// Returns an error if:
/// - The agent or central agent is not found in the database.
/// - The skill source does not exist (no SKILL.md).
/// - A real (non-symlink) directory already exists at the target path.
/// - `agent_id` is "central" (would create a self-referencing symlink).
async fn install_skill_to_agent_from_source_impl(
    pool: &AppStore,
    skill_id: &str,
    agent_id: &str,
    source_override: Option<&Path>,
    allow_universal_availability: bool,
) -> Result<InstallResult, String> {
    // Guard: cannot install to the central agent itself.
    if agent_id == "central" {
        return Err("Cannot install a skill to the central agent itself".to_string());
    }

    // 1. Look up the install target. This can be a software platform or a
    // project directory exposed as `project:<scan_directory_id>`.
    let target = install_target_for_agent_id(pool, agent_id).await?;

    // 2. Look up the central agent to determine the canonical root.
    let central = db::get_agent_by_id(pool, "central")
        .await?
        .ok_or_else(|| "Central agent not found in database".to_string())?;

    let central_root = PathBuf::from(&central.global_skills_dir);
    let canonical_dir = if let Some(source_dir) = source_override {
        source_dir.to_path_buf()
    } else {
        match repair_legacy_platform_link_in_central(pool, skill_id, agent_id, &central_root)
            .await?
        {
            Some(source_dir) => source_dir,
            None => install_source_dir_for_skill(pool, skill_id, &central_root).await?,
        }
    };
    let agent_dir = target.skills_dir.clone();
    validate_platform_install_target(agent_id, &canonical_dir, &agent_dir, &central_root)?;

    if allow_universal_availability && target.supports_universal_availability {
        if let Some(result) = universal_available_install_result(
            pool,
            skill_id,
            &target.id,
            &canonical_dir,
            &central_root,
        )
        .await?
        {
            return Ok(result);
        }
    }

    // 4. Compute symlink location.
    let relative_skill_path = relative_skill_path_for_install(skill_id);
    let symlink_path = agent_dir.join(relative_skill_path);

    // 5. Ensure the target parent directory exists.
    if let Some(parent) = symlink_path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create agent skills directory: {}", e))?;
    }

    if let Some(existing) = existing_installation_for_agent(pool, skill_id, &target.id).await? {
        cleanup_replaced_installation_path(&existing, &symlink_path, &agent_dir)?;
    }

    // 6. Handle any existing entry at the symlink path.
    match std::fs::symlink_metadata(&symlink_path) {
        Ok(meta) if meta.file_type().is_symlink() => {
            // Remove stale symlink so we can replace it.
            remove_symlink_path(&symlink_path)
                .map_err(|e| format!("Failed to remove existing symlink: {}", e))?;
        }
        Ok(meta) if meta.is_dir() => {
            return Err(format!(
                "A real directory already exists at '{}'. Refusing to overwrite.",
                symlink_path.display()
            ));
        }
        Ok(_) => {
            return Err(format!(
                "A file already exists at '{}'. Refusing to overwrite.",
                symlink_path.display()
            ));
        }
        Err(_) => {} // Path does not exist — proceed normally.
    }

    // 7. Compute the relative path from the agent directory to the canonical dir.
    let symlink_parent = symlink_path.parent().unwrap_or(&agent_dir);
    let relative_target = symlink_target_path(symlink_parent, &canonical_dir);

    // 8. Create the symlink.
    create_symlink(&relative_target, &symlink_path)?;

    // 9. Persist the installation record.
    let installation = SkillInstallation {
        skill_id: skill_id.to_string(),
        agent_id: target.id.clone(),
        installed_path: path_to_string(&symlink_path),
        link_type: "symlink".to_string(),
        symlink_target: Some(path_to_string(&canonical_dir)),
        created_at: chrono::Utc::now().to_rfc3339(),
    };
    db::upsert_skill_installation(pool, &installation).await?;

    Ok(InstallResult {
        symlink_path: path_to_string(&symlink_path),
    })
}

pub async fn install_skill_to_agent_impl(
    pool: &AppStore,
    skill_id: &str,
    agent_id: &str,
) -> Result<InstallResult, String> {
    install_skill_to_agent_from_source_impl(pool, skill_id, agent_id, None, true).await
}

async fn resource_source_dir_for_skill(pool: &AppStore, skill_id: &str) -> Result<PathBuf, String> {
    let skill = db::get_skill_by_id(pool, skill_id)
        .await?
        .ok_or_else(|| format!("Skill '{}' not found in database", skill_id))?;
    let resource_root = db::get_skill_resource_library_dir(pool).await?;

    let mut candidates = Vec::new();
    if let Some(canonical_path) = skill.canonical_path.as_deref() {
        candidates.push(PathBuf::from(canonical_path));
    }
    if let Some(file_parent) = Path::new(&skill.file_path).parent() {
        candidates.push(file_parent.to_path_buf());
    }

    let resolved_resource_root = resolved_path(&resource_root);
    for candidate in &candidates {
        if resolved_symlink_entry(candidate).starts_with(&resolved_resource_root)
            && candidate.join("SKILL.md").exists()
        {
            return Ok(candidate.clone());
        }
    }

    let central_root = db::get_agent_by_id(pool, "central")
        .await?
        .ok_or_else(|| "Central agent not found in database".to_string())?
        .global_skills_dir;
    let central_root = resolved_path(Path::new(&central_root));
    for candidate in candidates {
        let resolved_candidate = resolved_symlink_entry(&candidate);
        let Ok(relative_path) = resolved_candidate.strip_prefix(&central_root) else {
            continue;
        };
        let resource_candidate = resource_root.join(relative_path);
        if resource_candidate.join("SKILL.md").exists() {
            return Ok(resource_candidate);
        }
    }

    if let Some(found) = find_resource_skill_dir_by_id(&resource_root, skill_id) {
        return Ok(found);
    }

    Err(format!(
        "Resource Library source not found for skill '{}'",
        skill_id
    ))
}

fn find_resource_skill_dir_by_id(resource_root: &Path, skill_id: &str) -> Option<PathBuf> {
    let mut stack = vec![resource_root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let Ok(file_type) = entry.file_type() else {
                continue;
            };
            if !file_type.is_dir() {
                continue;
            }
            if path.file_name().and_then(|name| name.to_str()) == Some(skill_id)
                && path.join("SKILL.md").exists()
            {
                return Some(path);
            }
            stack.push(path);
        }
    }
    None
}

pub async fn install_resource_skill_to_agent_impl(
    pool: &AppStore,
    skill_id: &str,
    agent_id: &str,
    method: &str,
) -> Result<InstallResult, String> {
    if agent_skills_dir_shares_central(pool, agent_id).await? {
        let result = add_resource_skill_to_central_impl(pool, skill_id).await?;
        return Ok(InstallResult {
            symlink_path: result.central_path,
        });
    }

    let source_dir = resource_source_dir_for_skill(pool, skill_id).await?;

    match method {
        "copy" => {
            install_skill_to_agent_copy_from_source_impl(
                pool,
                skill_id,
                agent_id,
                Some(&source_dir),
                false,
            )
            .await
        }
        "symlink" => {
            install_skill_to_agent_from_source_impl(
                pool,
                skill_id,
                agent_id,
                Some(&source_dir),
                false,
            )
            .await
        }
        _ => match install_skill_to_agent_from_source_impl(
            pool,
            skill_id,
            agent_id,
            Some(&source_dir),
            false,
        )
        .await
        {
            Ok(result) => Ok(result),
            Err(error) if should_fallback_to_copy(&error) => {
                install_skill_to_agent_copy_from_source_impl(
                    pool,
                    skill_id,
                    agent_id,
                    Some(&source_dir),
                    false,
                )
                .await
            }
            Err(error) => Err(error),
        },
    }
}

pub async fn install_skill_to_agent_auto_impl(
    pool: &AppStore,
    skill_id: &str,
    agent_id: &str,
) -> Result<InstallResult, String> {
    match install_skill_to_agent_impl(pool, skill_id, agent_id).await {
        Ok(result) => Ok(result),
        Err(error) if should_fallback_to_copy(&error) => {
            install_skill_to_agent_copy_impl(pool, skill_id, agent_id).await
        }
        Err(error) => Err(error),
    }
}

async fn install_central_skill_to_agent_for_sync(
    pool: &AppStore,
    skill_id: &str,
    agent_id: &str,
) -> Result<InstallResult, String> {
    match install_skill_to_agent_from_source_impl(pool, skill_id, agent_id, None, false).await {
        Ok(result) => Ok(result),
        Err(error) if should_fallback_to_copy(&error) => {
            install_skill_to_agent_copy_from_source_impl(pool, skill_id, agent_id, None, false)
                .await
        }
        Err(error) => Err(error),
    }
}

#[cfg(windows)]
fn should_fallback_to_copy(error: &str) -> bool {
    error.contains("Failed to create symlink")
}

#[cfg(not(windows))]
fn should_fallback_to_copy(_error: &str) -> bool {
    false
}

/// Core copy-install logic — copies the skill directory instead of symlinking.
///
/// Copies the skill's current source directory recursively into
/// `agent.global_skills_dir/<skill_id>`. Existing symlinks at the target are
/// replaced; existing real directories cause an error.
async fn install_skill_to_agent_copy_from_source_impl(
    pool: &AppStore,
    skill_id: &str,
    agent_id: &str,
    source_override: Option<&Path>,
    allow_universal_availability: bool,
) -> Result<InstallResult, String> {
    // Guard: cannot install to the central agent itself.
    if agent_id == "central" {
        return Err("Cannot install a skill to the central agent itself".to_string());
    }

    // 1. Look up the install target. This can be a software platform or a
    // project directory exposed as `project:<scan_directory_id>`.
    let target = install_target_for_agent_id(pool, agent_id).await?;

    // 2. Look up the central agent to determine the canonical root.
    let central = db::get_agent_by_id(pool, "central")
        .await?
        .ok_or_else(|| "Central agent not found in database".to_string())?;

    let central_root = PathBuf::from(&central.global_skills_dir);
    let canonical_dir = if let Some(source_dir) = source_override {
        source_dir.to_path_buf()
    } else {
        match repair_legacy_platform_link_in_central(pool, skill_id, agent_id, &central_root)
            .await?
        {
            Some(source_dir) => source_dir,
            None => install_source_dir_for_skill(pool, skill_id, &central_root).await?,
        }
    };
    let agent_dir = target.skills_dir.clone();
    validate_platform_install_target(agent_id, &canonical_dir, &agent_dir, &central_root)?;

    if allow_universal_availability && target.supports_universal_availability {
        if let Some(result) = universal_available_install_result(
            pool,
            skill_id,
            &target.id,
            &canonical_dir,
            &central_root,
        )
        .await?
        {
            return Ok(result);
        }
    }

    // 4. Compute target location.
    let relative_skill_path = relative_skill_path_for_install(skill_id);
    let target_path = agent_dir.join(relative_skill_path);

    // 5. Ensure the target parent directory exists.
    if let Some(parent) = target_path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create agent skills directory: {}", e))?;
    }

    if let Some(existing) = existing_installation_for_agent(pool, skill_id, &target.id).await? {
        cleanup_replaced_installation_path(&existing, &target_path, &agent_dir)?;
    }

    // 6. Handle any existing entry at the target path.
    match std::fs::symlink_metadata(&target_path) {
        Ok(meta) if meta.file_type().is_symlink() => {
            // Remove stale symlink so we can replace it with a real copy.
            remove_symlink_path(&target_path)
                .map_err(|e| format!("Failed to remove existing symlink: {}", e))?;
        }
        Ok(meta) if meta.is_dir() => {
            return Err(format!(
                "A real directory already exists at '{}'. Refusing to overwrite.",
                target_path.display()
            ));
        }
        Ok(_) => {
            return Err(format!(
                "A file already exists at '{}'. Refusing to overwrite.",
                target_path.display()
            ));
        }
        Err(_) => {} // Path does not exist — proceed normally.
    }

    // 7. Recursively copy the canonical skill directory.
    copy_dir_all(&canonical_dir, &target_path)?;

    // 8. Persist the installation record.
    let installation = SkillInstallation {
        skill_id: skill_id.to_string(),
        agent_id: target.id.clone(),
        installed_path: path_to_string(&target_path),
        link_type: "copy".to_string(),
        symlink_target: None,
        created_at: chrono::Utc::now().to_rfc3339(),
    };
    db::upsert_skill_installation(pool, &installation).await?;

    Ok(InstallResult {
        symlink_path: path_to_string(&target_path),
    })
}

pub async fn install_skill_to_agent_copy_impl(
    pool: &AppStore,
    skill_id: &str,
    agent_id: &str,
) -> Result<InstallResult, String> {
    install_skill_to_agent_copy_from_source_impl(pool, skill_id, agent_id, None, true).await
}

/// Core uninstall logic, separated from the Tauri layer for testability.
///
/// Removes the path recorded by a managed `skill_installations` row and then
/// deletes that row. Without managed ownership, no filesystem path is touched.
///
/// For symlinked skills: removes the symlink.
/// For copied skills: removes the copied directory (tracked in the DB as link_type='copy').
/// Refuses to delete real directories not tracked as copies in the DB.
pub async fn uninstall_skill_from_agent_impl(
    pool: &AppStore,
    skill_id: &str,
    agent_id: &str,
) -> Result<(), String> {
    // 1. Look up the target to reject stale or unknown IDs before touching
    // filesystem paths.
    let target = install_target_for_agent_id(pool, agent_id).await?;

    // 2. Look up the installation record to determine where and how it was installed.
    let installations = db::get_skill_installations(pool, skill_id).await?;
    let record = match installations
        .iter()
        .find(|record| record.agent_id == target.id)
    {
        Some(record) => record,
        None if target.supports_universal_availability
            && db::agent_supports_universal_agents_skills(&target.id) =>
        {
            return Ok(())
        }
        None => {
            return Err(format!(
                "No managed installation found for skill '{}' on agent '{}'",
                skill_id, target.id
            ))
        }
    };
    let install_path = PathBuf::from(&record.installed_path);
    let link_type = record.link_type.as_str();

    // 3. Inspect the entry at that path and remove it appropriately.
    match std::fs::symlink_metadata(&install_path) {
        Ok(meta) if meta.file_type().is_symlink() => {
            // Always safe to remove symlinks.
            remove_symlink_path(&install_path)
                .map_err(|e| format!("Failed to remove symlink: {}", e))?;
        }
        Ok(meta) if meta.is_dir() => {
            // Only remove real directories that were explicitly installed as copies.
            if link_type == "copy" {
                std::fs::remove_dir_all(&install_path)
                    .map_err(|e| format!("Failed to remove copied skill directory: {}", e))?;
            } else {
                return Err(format!(
                    "Path '{}' exists but is not a symlink. Refusing to delete.",
                    install_path.display()
                ));
            }
        }
        Ok(_) => {
            return Err(format!(
                "Path '{}' exists but is not a symlink. Refusing to delete.",
                install_path.display()
            ));
        }
        Err(_) => {
            // Path doesn't exist — still clean up the DB record.
        }
    }

    // 4. Remove the installation record from the database.
    db::delete_skill_installation(pool, skill_id, &target.id).await?;
    let _ = prune_empty_parent_dirs(&install_path, &target.skills_dir);

    Ok(())
}

/// Remove leftover mirrored folders under each platform/project skills root
/// after a Central Skills directory group has been deleted.
pub(crate) async fn prune_mirrored_relative_path_from_install_targets(
    pool: &AppStore,
    relative_path: &str,
) -> Result<(), String> {
    let relative = Path::new(relative_path);
    if relative.as_os_str().is_empty()
        || !relative
            .components()
            .all(|component| matches!(component, Component::Normal(_)))
    {
        return Ok(());
    }

    let central_root = db::get_agent_by_id(pool, "central")
        .await?
        .map(|agent| PathBuf::from(agent.global_skills_dir));
    let resolved_central = central_root.as_ref().map(|path| resolved_path(path));

    for agent in db::get_all_agents(pool).await? {
        if agent.id == "central" || !agent.is_enabled {
            continue;
        }
        let skills_dir = PathBuf::from(&agent.global_skills_dir);
        prune_mirrored_relative_path_under_root(&skills_dir, relative, resolved_central.as_ref());
    }

    for scan_dir in db::get_scan_directories(pool).await? {
        let Some(target) = project_target_for_scan_directory(&scan_dir).await? else {
            continue;
        };
        prune_mirrored_relative_path_under_root(
            &target.skills_dir,
            relative,
            resolved_central.as_ref(),
        );
    }

    Ok(())
}

fn install_root_is_central_or_nested(
    skills_dir: &Path,
    resolved_central: Option<&PathBuf>,
) -> bool {
    let Some(central) = resolved_central else {
        return false;
    };
    let resolved_skills = resolved_path(skills_dir);
    resolved_skills == *central || resolved_skills.starts_with(central)
}

fn prune_mirrored_relative_path_under_root(
    skills_dir: &Path,
    relative: &Path,
    resolved_central: Option<&PathBuf>,
) {
    if !skills_dir.exists() || install_root_is_central_or_nested(skills_dir, resolved_central) {
        return;
    }
    let mirrored = skills_dir.join(relative);
    let _ = remove_path_if_no_skill_markdown(&mirrored);
    let _ = prune_empty_parent_dirs(&mirrored, skills_dir);
}

// ─── Tauri Commands ───────────────────────────────────────────────────────────

/// Tauri command: install a skill to a single agent via relative symlink.
#[tauri::command]
pub async fn install_skill_to_agent(
    state: State<'_, AppState>,
    skill_id: String,
    agent_id: String,
    method: Option<String>,
) -> Result<InstallResult, String> {
    match method.as_deref().unwrap_or("auto") {
        "copy" => install_skill_to_agent_copy_impl(&state.db, &skill_id, &agent_id).await,
        "symlink" => install_skill_to_agent_impl(&state.db, &skill_id, &agent_id).await,
        _ => install_skill_to_agent_auto_impl(&state.db, &skill_id, &agent_id).await,
    }
}

#[tauri::command]
pub async fn add_resource_skill_to_central(
    state: State<'_, AppState>,
    skill_id: String,
) -> Result<AddResourceSkillToCentralResult, String> {
    add_resource_skill_to_central_impl(&state.db, &skill_id).await
}

#[tauri::command]
pub async fn remove_resource_skill_from_central(
    state: State<'_, AppState>,
    skill_id: String,
) -> Result<RemoveResourceSkillFromCentralResult, String> {
    remove_resource_skill_from_central_impl(&state.db, &skill_id).await
}

/// Tauri command: remove a skill's symlink from an agent.
#[tauri::command]
pub async fn uninstall_skill_from_agent(
    state: State<'_, AppState>,
    skill_id: String,
    agent_id: String,
) -> Result<(), String> {
    uninstall_skill_from_agent_impl(&state.db, &skill_id, &agent_id).await
}

/// Tauri command: install a skill to multiple agents in one call.
///
/// `method` must be either `"symlink"` (default, creates a relative symlink) or
/// `"copy"` (copies the skill directory). Each agent install is attempted
/// independently; failures are collected in the `failed` list rather than
/// short-circuiting the entire batch.
#[tauri::command]
pub async fn batch_install_to_agents(
    state: State<'_, AppState>,
    skill_id: String,
    agent_ids: Vec<String>,
    method: Option<String>,
) -> Result<BatchInstallResult, String> {
    let method = method.as_deref().unwrap_or("auto");
    let mut succeeded = Vec::new();
    let mut failed = Vec::new();

    for agent_id in &agent_ids {
        let install_result = match method {
            "copy" => install_skill_to_agent_copy_impl(&state.db, &skill_id, agent_id).await,
            "symlink" => install_skill_to_agent_impl(&state.db, &skill_id, agent_id).await,
            _ => install_skill_to_agent_auto_impl(&state.db, &skill_id, agent_id).await,
        };
        match install_result {
            Ok(_) => succeeded.push(agent_id.clone()),
            Err(e) => failed.push(FailedInstall {
                agent_id: agent_id.clone(),
                error: e,
            }),
        }
    }

    Ok(BatchInstallResult { succeeded, failed })
}

/// Install a Resource Library skill directly into only the requested agents.
/// The source is always resolved inside the Resource Library, even when the
/// same skill also has a Central Skills copy.
#[tauri::command]
pub async fn batch_install_resource_skill_to_agents(
    state: State<'_, AppState>,
    skill_id: String,
    agent_ids: Vec<String>,
    method: Option<String>,
) -> Result<BatchInstallResult, String> {
    let method = method.as_deref().unwrap_or("auto");
    let mut succeeded = Vec::new();
    let mut failed = Vec::new();

    for agent_id in &agent_ids {
        match install_resource_skill_to_agent_impl(&state.db, &skill_id, agent_id, method).await {
            Ok(_) => succeeded.push(agent_id.clone()),
            Err(error) => failed.push(FailedInstall {
                agent_id: agent_id.clone(),
                error,
            }),
        }
    }

    Ok(BatchInstallResult { succeeded, failed })
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;
    use crate::db::AppStore as AppStore;
    use std::fs;
    use tempfile::TempDir;

    #[cfg(windows)]
    #[test]
    fn test_resolved_path_removes_windows_verbatim_prefix() {
        assert_eq!(
            resolved_path(Path::new(r"\\?\C:\Users\alice\.agents\skills")),
            PathBuf::from(r"C:\Users\alice\.agents\skills")
        );
    }

    // ── Test helpers ──────────────────────────────────────────────────────────

    /// Create an in-memory SQLite pool with the full schema initialised and
    /// the central/claude-code agent directories redirected to `central_dir`
    /// and `agent_dir` respectively.
    async fn setup_db(central_dir: &Path, agent_dir: &Path) -> AppStore {
        let pool = AppStore::connect(":memory:").await.unwrap();
        db::init_database(&pool).await.unwrap();

        pool.write(|d| {
            if let Some(row) = d.table_mut("agents").get_mut("central") {
                row["global_skills_dir"] = serde_json::json!(central_dir.to_str().unwrap());
            }
            Ok(())
        })
        .await
        .unwrap();

        pool.write(|d| {
            if let Some(row) = d.table_mut("agents").get_mut("claude-code") {
                row["global_skills_dir"] = serde_json::json!(agent_dir.to_str().unwrap());
            }
            Ok(())
        })
        .await
        .unwrap();

        crate::storage::fixtures::disable_except(&pool, &["central", "claude-code"]).await;

        pool
    }

    /// Create a minimal skill directory containing a valid `SKILL.md`.
    fn create_central_skill(central_dir: &Path, skill_id: &str) -> PathBuf {
        let skill_dir = central_dir.join(skill_id);
        fs::create_dir_all(&skill_dir).unwrap();
        fs::write(
            skill_dir.join("SKILL.md"),
            format!(
                "---\nname: {}\ndescription: Test skill\n---\n\n# {}\n",
                skill_id, skill_id
            ),
        )
        .unwrap();
        skill_dir
    }

    async fn create_resource_skill(pool: &AppStore, resource_dir: &Path, skill_id: &str) -> PathBuf {
        let skill_dir = create_central_skill(resource_dir, skill_id);
        let skill = db::Skill {
            id: skill_id.to_string(),
            name: skill_id.to_string(),
            description: Some("Resource library skill".to_string()),
            file_path: skill_dir.join("SKILL.md").to_string_lossy().into_owned(),
            canonical_path: Some(skill_dir.to_string_lossy().into_owned()),
            is_central: false,
            source: Some("resource-library".to_string()),
            content: None,
            scanned_at: chrono::Utc::now().to_rfc3339(),
        };
        db::upsert_skill(pool, &skill).await.unwrap();
        skill_dir
    }

    async fn create_noncentral_skill(pool: &AppStore, source_root: &Path, skill_id: &str) -> PathBuf {
        let skill_dir = create_central_skill(source_root, skill_id);
        db::upsert_skill(
            pool,
            &db::Skill {
                id: skill_id.to_string(),
                name: skill_id.to_string(),
                description: Some("Platform-only skill".to_string()),
                file_path: skill_dir.join("SKILL.md").to_string_lossy().into_owned(),
                canonical_path: None,
                is_central: false,
                source: Some("native".to_string()),
                content: None,
                scanned_at: chrono::Utc::now().to_rfc3339(),
            },
        )
        .await
        .unwrap();
        skill_dir
    }

    // ── make_relative_path ────────────────────────────────────────────────────

    #[test]
    fn test_make_relative_path_sibling_dirs() {
        let from = Path::new("/home/user/claude/skills");
        let to = Path::new("/home/user/.agents/skills/my-skill");
        let rel = make_relative_path(from, to);
        assert_eq!(rel, PathBuf::from("../../.agents/skills/my-skill"));
    }

    #[test]
    fn test_make_relative_path_same_parent() {
        let from = Path::new("/tmp/test/agent");
        let to = Path::new("/tmp/test/central/skill-x");
        let rel = make_relative_path(from, to);
        assert_eq!(rel, PathBuf::from("../central/skill-x"));
    }

    #[test]
    fn test_make_relative_path_deep_nesting() {
        let from = Path::new("/a/b/c/d");
        let to = Path::new("/a/x/y");
        let rel = make_relative_path(from, to);
        assert_eq!(rel, PathBuf::from("../../../x/y"));
    }

    // ── install_skill_to_agent_impl ───────────────────────────────────────────

    #[tokio::test]
    async fn test_install_creates_symlink() {
        let tmp = TempDir::new().unwrap();
        let central_dir = tmp.path().join("central");
        let agent_dir = tmp.path().join("claude");
        fs::create_dir_all(&central_dir).unwrap();

        let pool = setup_db(&central_dir, &agent_dir).await;

        create_central_skill(&central_dir, "my-skill");

        let result = install_skill_to_agent_impl(&pool, "my-skill", "claude-code").await;
        assert!(result.is_ok(), "install should succeed: {:?}", result);

        let symlink_path = agent_dir.join("my-skill");
        let meta = fs::symlink_metadata(&symlink_path).unwrap();
        assert!(meta.file_type().is_symlink(), "entry should be a symlink");
    }

    #[tokio::test]
    async fn test_install_to_universal_agent_returns_central_availability_without_link() {
        let tmp = TempDir::new().unwrap();
        let central_dir = tmp.path().join(".agents/skills");
        let cursor_dir = tmp.path().join(".cursor/skills");
        fs::create_dir_all(&central_dir).unwrap();

        let pool = setup_db(&central_dir, &tmp.path().join("claude")).await;
        pool.write(|d| {
            if let Some(row) = d.table_mut("agents").get_mut("cursor") {
                row["global_skills_dir"] =
                    serde_json::json!(cursor_dir.to_string_lossy().to_string());
            }
            Ok(())
        })
        .await
        .unwrap();
        create_central_skill(&central_dir, "universal-skill");

        let result = install_skill_to_agent_impl(&pool, "universal-skill", "cursor")
            .await
            .unwrap();

        assert_eq!(
            result.symlink_path,
            path_to_string(&central_dir.join("universal-skill"))
        );
        assert!(
            !cursor_dir.join("universal-skill").exists(),
            "universal agents must not receive redundant links for central skills"
        );
        assert!(
            db::get_skill_installations(&pool, "universal-skill")
                .await
                .unwrap()
                .into_iter()
                .all(|installation| installation.agent_id != "cursor"),
            "universal availability must not create removable installation rows"
        );
    }

    #[tokio::test]
    async fn test_install_symlink_is_relative() {
        let tmp = TempDir::new().unwrap();
        let central_dir = tmp.path().join("central");
        let agent_dir = tmp.path().join("claude");
        fs::create_dir_all(&central_dir).unwrap();

        let pool = setup_db(&central_dir, &agent_dir).await;
        create_central_skill(&central_dir, "rel-skill");

        install_skill_to_agent_impl(&pool, "rel-skill", "claude-code")
            .await
            .unwrap();

        let symlink_path = agent_dir.join("rel-skill");
        let link_target = fs::read_link(&symlink_path).unwrap();
        assert!(
            link_target.is_relative(),
            "symlink target should be relative, got {:?}",
            link_target
        );
    }

    #[tokio::test]
    async fn test_install_symlink_resolves_correctly() {
        let tmp = TempDir::new().unwrap();
        let central_dir = tmp.path().join("central");
        let agent_dir = tmp.path().join("claude");
        fs::create_dir_all(&central_dir).unwrap();

        let pool = setup_db(&central_dir, &agent_dir).await;
        create_central_skill(&central_dir, "resolve-skill");

        install_skill_to_agent_impl(&pool, "resolve-skill", "claude-code")
            .await
            .unwrap();

        let symlink_path = agent_dir.join("resolve-skill");
        // Following the symlink should give access to SKILL.md in the central dir.
        let skill_md = symlink_path.join("SKILL.md");
        assert!(
            skill_md.exists(),
            "SKILL.md should be accessible via symlink"
        );
    }

    #[tokio::test]
    async fn test_install_creates_agent_dir_if_missing() {
        let tmp = TempDir::new().unwrap();
        let central_dir = tmp.path().join("central");
        // Do NOT pre-create agent_dir — install should create it.
        let agent_dir = tmp.path().join("new-agent-dir");
        fs::create_dir_all(&central_dir).unwrap();

        let pool = setup_db(&central_dir, &agent_dir).await;
        create_central_skill(&central_dir, "dir-skill");

        let result = install_skill_to_agent_impl(&pool, "dir-skill", "claude-code").await;
        assert!(result.is_ok(), "install should create missing agent dir");
        assert!(agent_dir.exists(), "agent dir should have been created");
    }

    #[tokio::test]
    async fn test_install_updates_db_record() {
        let tmp = TempDir::new().unwrap();
        let central_dir = tmp.path().join("central");
        let agent_dir = tmp.path().join("claude");
        fs::create_dir_all(&central_dir).unwrap();

        let pool = setup_db(&central_dir, &agent_dir).await;
        create_central_skill(&central_dir, "db-skill");

        install_skill_to_agent_impl(&pool, "db-skill", "claude-code")
            .await
            .unwrap();

        let installations = db::get_skill_installations(&pool, "db-skill")
            .await
            .unwrap();
        assert_eq!(installations.len(), 1);
        assert_eq!(installations[0].agent_id, "claude-code");
        assert_eq!(installations[0].link_type, "symlink");
    }

    #[tokio::test]
    async fn test_install_resource_skill_to_universal_agent_creates_platform_link() {
        let tmp = TempDir::new().unwrap();
        let central_dir = tmp.path().join(".agents").join("skills");
        let cursor_dir = tmp.path().join(".cursor").join("skills");
        let resource_dir = tmp.path().join("resource-library");
        fs::create_dir_all(&central_dir).unwrap();
        fs::create_dir_all(&cursor_dir).unwrap();
        fs::create_dir_all(&resource_dir).unwrap();

        let pool = setup_db(&central_dir, &tmp.path().join("claude")).await;
        db::set_skill_resource_library_dir(&pool, &resource_dir.to_string_lossy())
            .await
            .unwrap();
        pool.write(|d| {
            if let Some(row) = d.table_mut("agents").get_mut("cursor") {
                row["global_skills_dir"] =
                    serde_json::json!(cursor_dir.to_string_lossy().into_owned());
            }
            Ok(())
        })
        .await
        .unwrap();

        let source_dir = create_resource_skill(&pool, &resource_dir, "resource-only").await;

        install_resource_skill_to_agent_impl(&pool, "resource-only", "cursor", "symlink")
            .await
            .unwrap();

        let installed_path = cursor_dir.join("resource-only");
        assert!(
            fs::symlink_metadata(&installed_path).is_ok(),
            "resource skill should be explicitly installed into the selected platform"
        );
        assert!(
            !central_dir.join("resource-only").exists(),
            "installing a resource skill to a platform must not copy it into the central library"
        );

        let installations = db::get_skill_installations(&pool, "resource-only")
            .await
            .unwrap();
        assert_eq!(installations.len(), 1);
        assert_eq!(installations[0].agent_id, "cursor");
        assert_eq!(
            installations[0].symlink_target.as_deref(),
            Some(source_dir.to_string_lossy().as_ref())
        );

        let skill = db::get_skill_by_id(&pool, "resource-only")
            .await
            .unwrap()
            .unwrap();
        assert!(
            !skill.is_central,
            "resource library installs must not mark the skill as central"
        );
    }

    #[tokio::test]
    async fn test_install_resource_skill_to_shared_root_adds_to_central_without_self_symlink() {
        let tmp = TempDir::new().unwrap();
        let central_dir = tmp.path().join(".agents").join("skills");
        let resource_dir = tmp.path().join("resource-library");
        fs::create_dir_all(&central_dir).unwrap();
        fs::create_dir_all(&resource_dir).unwrap();

        let pool = setup_db(&central_dir, &tmp.path().join("claude")).await;
        db::set_skill_resource_library_dir(&pool, &resource_dir.to_string_lossy())
            .await
            .unwrap();
        pool.write(|d| {
            for row in d.table_mut("agents").values_mut() {
                if row["id"] != "central" {
                    row["is_enabled"] = false.into();
                }
            }
            Ok(())
        })
        .await
        .unwrap();
        pool.write(|d| {
            if let Some(row) = d.table_mut("agents").get_mut("cursor") {
                row["global_skills_dir"] =
                    serde_json::json!(central_dir.to_string_lossy().into_owned());
            }
            Ok(())
        })
        .await
        .unwrap();

        create_resource_skill(&pool, &resource_dir, "shared-root").await;

        install_resource_skill_to_agent_impl(&pool, "shared-root", "cursor", "symlink")
            .await
            .unwrap();

        let skill = db::get_skill_by_id(&pool, "shared-root")
            .await
            .unwrap()
            .unwrap();
        assert!(
            skill.is_central,
            "installing onto a shared .agents/skills root should add the skill to Central Skills"
        );

        let central_skill_dir = central_dir.join("shared-root");
        assert!(
            central_skill_dir.join("SKILL.md").exists(),
            "the skill should exist in the central library"
        );
        let metadata = fs::symlink_metadata(&central_skill_dir).unwrap();
        assert!(
            metadata.file_type().is_symlink(),
            "shared-root install should link Central Skills to the resource library"
        );

        let installations = db::get_skill_installations(&pool, "shared-root")
            .await
            .unwrap();
        assert!(
            !installations
                .iter()
                .any(|installation| installation.agent_id == "cursor"),
            "shared-root install should not record a platform installation against the shared agent"
        );

        install_resource_skill_to_agent_impl(&pool, "shared-root", "cursor", "symlink")
            .await
            .expect(
                "installing an already-central skill onto the shared root should be idempotent",
            );
        let skill_again = db::get_skill_by_id(&pool, "shared-root")
            .await
            .unwrap()
            .unwrap();
        assert!(skill_again.is_central);
        assert!(
            fs::symlink_metadata(&central_skill_dir)
                .unwrap()
                .file_type()
                .is_symlink(),
            "idempotent shared-root install should keep the Central Skills symlink"
        );
    }

    #[tokio::test]
    async fn test_install_resource_skill_to_shared_project_root_adds_to_central_without_self_symlink(
    ) {
        let tmp = TempDir::new().unwrap();
        let central_dir = tmp.path().join(".agents").join("skills");
        let resource_dir = tmp.path().join("resource-library");
        fs::create_dir_all(&central_dir).unwrap();
        fs::create_dir_all(&resource_dir).unwrap();

        let pool = setup_db(&central_dir, &tmp.path().join("claude")).await;
        db::set_skill_resource_library_dir(&pool, &resource_dir.to_string_lossy())
            .await
            .unwrap();
        pool.write(|d| {
            for row in d.table_mut("agents").values_mut() {
                if row["id"] != "central" {
                    row["is_enabled"] = false.into();
                }
            }
            Ok(())
        })
        .await
        .unwrap();
        let scan_dir = db::add_scan_directory(&pool, &tmp.path().to_string_lossy(), Some("Home"))
            .await
            .unwrap();
        let project_id = project_agent_id(scan_dir.id);

        create_resource_skill(&pool, &resource_dir, "shared-project").await;

        install_resource_skill_to_agent_impl(&pool, "shared-project", &project_id, "symlink")
            .await
            .unwrap();

        let skill = db::get_skill_by_id(&pool, "shared-project")
            .await
            .unwrap()
            .unwrap();
        assert!(
            skill.is_central,
            "installing onto a project .agents/skills root that matches Central Skills should add the skill to Central Skills"
        );

        let central_skill_dir = central_dir.join("shared-project");
        assert!(
            central_skill_dir.join("SKILL.md").exists(),
            "the skill should exist in the central library"
        );
        let metadata = fs::symlink_metadata(&central_skill_dir).unwrap();
        assert!(
            metadata.file_type().is_symlink(),
            "shared project-root install should link Central Skills to the resource library"
        );

        let installations = db::get_skill_installations(&pool, "shared-project")
            .await
            .unwrap();
        assert!(
            !installations
                .iter()
                .any(|installation| installation.agent_id == project_id),
            "shared project-root install should not record a platform installation against the project agent"
        );

        install_resource_skill_to_agent_impl(&pool, "shared-project", &project_id, "symlink")
            .await
            .expect("installing an already-central skill onto the shared project root should be idempotent");
        let skill_again = db::get_skill_by_id(&pool, "shared-project")
            .await
            .unwrap()
            .unwrap();
        assert!(skill_again.is_central);
        assert!(
            fs::symlink_metadata(&central_skill_dir)
                .unwrap()
                .file_type()
                .is_symlink(),
            "idempotent shared project-root install should keep the Central Skills symlink"
        );
    }

    #[tokio::test]
    async fn test_resource_install_uses_resource_source_even_when_central_copy_exists() {
        let tmp = TempDir::new().unwrap();
        let central_dir = tmp.path().join("central");
        let resource_dir = tmp.path().join("resource-library");
        let agent_dir = tmp.path().join("claude").join("skills");
        fs::create_dir_all(&central_dir).unwrap();
        fs::create_dir_all(agent_dir.parent().unwrap()).unwrap();

        let pool = setup_db(&central_dir, &agent_dir).await;
        db::set_skill_resource_library_dir(&pool, &resource_dir.to_string_lossy())
            .await
            .unwrap();
        pool.write(|d| {
            for row in d.table_mut("agents").values_mut() {
                if row["id"] != "central" {
                    row["is_enabled"] = false.into();
                }
            }
            Ok(())
        })
        .await
        .unwrap();
        let resource_source = create_resource_skill(&pool, &resource_dir, "dual-source").await;
        add_resource_skill_to_central_impl(&pool, "dual-source")
            .await
            .unwrap();

        install_resource_skill_to_agent_impl(&pool, "dual-source", "claude-code", "symlink")
            .await
            .unwrap();

        let installation = db::get_skill_installations(&pool, "dual-source")
            .await
            .unwrap()
            .into_iter()
            .find(|installation| installation.agent_id == "claude-code")
            .unwrap();
        assert_eq!(
            installation.symlink_target.as_deref(),
            Some(resource_source.to_string_lossy().as_ref()),
            "a Resource Library install must target the resource source rather than the Central Skills copy"
        );
        assert_eq!(
            fs::canonicalize(agent_dir.join("dual-source")).unwrap(),
            fs::canonicalize(resource_source).unwrap()
        );
    }

    #[tokio::test]
    async fn test_symlink_install_noncentral_skill_does_not_promote_to_central() {
        let tmp = TempDir::new().unwrap();
        let central_dir = tmp.path().join(".agents").join("skills");
        let source_root = tmp.path().join("source-platform").join("skills");
        let target_dir = tmp.path().join("target-platform").join("skills");
        fs::create_dir_all(&central_dir).unwrap();

        let pool = setup_db(&central_dir, &target_dir).await;
        let source_dir = create_noncentral_skill(&pool, &source_root, "platform-only").await;

        install_skill_to_agent_impl(&pool, "platform-only", "claude-code")
            .await
            .unwrap();

        let installed_path = target_dir.join("platform-only");
        assert!(fs::symlink_metadata(&installed_path).unwrap().is_symlink());
        assert!(!central_dir.join("platform-only").exists());

        let installation = db::get_skill_installations(&pool, "platform-only")
            .await
            .unwrap()
            .into_iter()
            .next()
            .unwrap();
        assert_eq!(installation.agent_id, "claude-code");
        assert_eq!(
            installation.symlink_target.as_deref(),
            Some(source_dir.to_string_lossy().as_ref())
        );

        let skill = db::get_skill_by_id(&pool, "platform-only")
            .await
            .unwrap()
            .unwrap();
        assert!(!skill.is_central);
        assert_eq!(skill.canonical_path, None);
    }

    #[tokio::test]
    async fn test_copy_install_noncentral_skill_does_not_promote_to_central() {
        let tmp = TempDir::new().unwrap();
        let central_dir = tmp.path().join(".agents").join("skills");
        let source_root = tmp.path().join("source-platform").join("skills");
        let target_dir = tmp.path().join("target-platform").join("skills");
        fs::create_dir_all(&central_dir).unwrap();

        let pool = setup_db(&central_dir, &target_dir).await;
        create_noncentral_skill(&pool, &source_root, "platform-only-copy").await;

        install_skill_to_agent_copy_impl(&pool, "platform-only-copy", "claude-code")
            .await
            .unwrap();

        let installed_path = target_dir.join("platform-only-copy");
        assert!(installed_path.join("SKILL.md").exists());
        assert!(!fs::symlink_metadata(&installed_path).unwrap().is_symlink());
        assert!(!central_dir.join("platform-only-copy").exists());

        let skill = db::get_skill_by_id(&pool, "platform-only-copy")
            .await
            .unwrap()
            .unwrap();
        assert!(!skill.is_central);
        assert_eq!(skill.canonical_path, None);
    }

    async fn point_agent_at_central(pool: &AppStore, agent_id: &str, central_dir: &Path) {
        pool.write(|d| {
            d.table_mut("agents").get_mut(agent_id).unwrap()["global_skills_dir"] =
                central_dir.to_string_lossy().to_string().into();
            Ok(())
        })
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn test_symlink_install_rejects_noncentral_skill_when_agent_uses_central_dir() {
        let tmp = TempDir::new().unwrap();
        let central_dir = tmp.path().join(".agents").join("skills");
        let source_root = tmp.path().join("resource-library");
        fs::create_dir_all(&central_dir).unwrap();

        let pool = setup_db(&central_dir, &tmp.path().join("claude")).await;
        point_agent_at_central(&pool, "codex", &central_dir).await;
        create_noncentral_skill(&pool, &source_root, "resource-only").await;

        let error = install_skill_to_agent_impl(&pool, "resource-only", "codex")
            .await
            .unwrap_err();

        assert!(error.contains("central skills directory"));
        assert!(!central_dir.join("resource-only").exists());
    }

    #[tokio::test]
    async fn test_copy_install_rejects_noncentral_skill_when_agent_uses_central_dir() {
        let tmp = TempDir::new().unwrap();
        let central_dir = tmp.path().join(".agents").join("skills");
        let source_root = tmp.path().join("resource-library");
        fs::create_dir_all(&central_dir).unwrap();

        let pool = setup_db(&central_dir, &tmp.path().join("claude")).await;
        point_agent_at_central(&pool, "codex", &central_dir).await;
        create_noncentral_skill(&pool, &source_root, "resource-copy").await;

        let error = install_skill_to_agent_copy_impl(&pool, "resource-copy", "codex")
            .await
            .unwrap_err();

        assert!(error.contains("central skills directory"));
        assert!(!central_dir.join("resource-copy").exists());
    }

    #[tokio::test]
    async fn test_reinstall_migrates_legacy_platform_link_out_of_central() {
        let tmp = TempDir::new().unwrap();
        let central_dir = tmp.path().join(".agents").join("skills");
        let resource_dir = tmp.path().join("resource-library");
        let codex_dir = tmp.path().join(".codex").join("skills");
        fs::create_dir_all(&central_dir).unwrap();
        let source_dir = create_central_skill(&resource_dir, "legacy-resource");
        let legacy_link = central_dir.join("legacy-resource");
        create_symlink(&source_dir, &legacy_link).unwrap();

        let pool = setup_db(&central_dir, &tmp.path().join("claude")).await;
        point_agent_at_central(&pool, "codex", &codex_dir).await;
        db::upsert_skill(
            &pool,
            &db::Skill {
                id: "legacy-resource".to_string(),
                name: "legacy-resource".to_string(),
                description: Some("Legacy resource skill".to_string()),
                file_path: legacy_link.join("SKILL.md").to_string_lossy().into_owned(),
                canonical_path: Some(legacy_link.to_string_lossy().into_owned()),
                is_central: true,
                source: Some("resource-library".to_string()),
                content: None,
                scanned_at: chrono::Utc::now().to_rfc3339(),
            },
        )
        .await
        .unwrap();
        crate::storage::fixtures::insert(&pool, "skill_installations", serde_json::json!({"skill_id":"legacy-resource","agent_id":"codex","installed_path":legacy_link.to_string_lossy().into_owned(),"link_type":"symlink","symlink_target":source_dir.to_string_lossy().into_owned(),"is_managed":false,"created_at":chrono::Utc::now().to_rfc3339()})).await.unwrap();

        install_skill_to_agent_impl(&pool, "legacy-resource", "codex")
            .await
            .unwrap();

        assert!(fs::symlink_metadata(&legacy_link).is_err());
        let codex_link = codex_dir.join("legacy-resource");
        assert!(codex_link.join("SKILL.md").exists());
        assert!(fs::symlink_metadata(&codex_link).unwrap().is_symlink());
        let repaired = db::get_skill_by_id(&pool, "legacy-resource")
            .await
            .unwrap()
            .unwrap();
        assert!(!repaired.is_central);
        assert_eq!(
            repaired.canonical_path.as_deref(),
            Some(source_dir.to_string_lossy().as_ref())
        );
        let installation = db::get_skill_installations(&pool, "legacy-resource")
            .await
            .unwrap()
            .into_iter()
            .find(|installation| installation.agent_id == "codex")
            .unwrap();
        assert_eq!(installation.installed_path, codex_link.to_string_lossy());
    }

    #[tokio::test]
    async fn test_install_uses_nested_canonical_path_from_db() {
        let tmp = TempDir::new().unwrap();
        let central_dir = tmp.path().join("central");
        let agent_dir = tmp.path().join("claude");
        let nested_skill_dir = central_dir.join("superpowers").join("using-superpowers");
        fs::create_dir_all(&nested_skill_dir).unwrap();
        fs::write(
            nested_skill_dir.join("SKILL.md"),
            "---\nname: using-superpowers\ndescription: Nested central\n---\n",
        )
        .unwrap();

        let pool = setup_db(&central_dir, &agent_dir).await;
        db::upsert_skill(
            &pool,
            &db::Skill {
                id: "using-superpowers".to_string(),
                name: "using-superpowers".to_string(),
                description: Some("Nested central".to_string()),
                file_path: nested_skill_dir
                    .join("SKILL.md")
                    .to_string_lossy()
                    .into_owned(),
                canonical_path: Some(nested_skill_dir.to_string_lossy().into_owned()),
                is_central: true,
                source: Some("native".to_string()),
                content: None,
                scanned_at: chrono::Utc::now().to_rfc3339(),
            },
        )
        .await
        .unwrap();

        install_skill_to_agent_impl(&pool, "using-superpowers", "claude-code")
            .await
            .unwrap();

        let symlink_path = agent_dir.join("using-superpowers");
        let nested_platform_path = agent_dir.join("superpowers").join("using-superpowers");
        assert!(symlink_path.join("SKILL.md").exists());
        assert!(
            !nested_platform_path.exists(),
            "nested canonical skill must not mirror source grouping into the platform root"
        );
        assert!(
            !central_dir.join("using-superpowers").exists(),
            "nested canonical skill must not be copied to the central root"
        );
        let link_target = fs::read_link(&symlink_path).unwrap();
        assert!(
            link_target
                .components()
                .collect::<Vec<_>>()
                .windows(2)
                .any(|pair| {
                    pair[0].as_os_str() == "superpowers"
                        && pair[1].as_os_str() == "using-superpowers"
                }),
            "symlink should point at nested canonical path, got {:?}",
            link_target
        );
    }

    #[tokio::test]
    async fn test_install_flattens_nested_canonical_relative_path() {
        let tmp = TempDir::new().unwrap();
        let central_dir = tmp.path().join("central");
        let agent_dir = tmp.path().join("claude");
        let nested_skill_dir = central_dir
            .join("anthropics")
            .join("skills")
            .join("brand-guidelines");
        fs::create_dir_all(&nested_skill_dir).unwrap();
        fs::write(
            nested_skill_dir.join("SKILL.md"),
            "---\nname: brand-guidelines\ndescription: Brand rules\n---\n",
        )
        .unwrap();

        let pool = setup_db(&central_dir, &agent_dir).await;
        db::upsert_skill(
            &pool,
            &db::Skill {
                id: "brand-guidelines".to_string(),
                name: "brand-guidelines".to_string(),
                description: Some("Brand rules".to_string()),
                file_path: nested_skill_dir
                    .join("SKILL.md")
                    .to_string_lossy()
                    .into_owned(),
                canonical_path: Some(nested_skill_dir.to_string_lossy().into_owned()),
                is_central: true,
                source: Some("github:anthropics/skills".to_string()),
                content: None,
                scanned_at: chrono::Utc::now().to_rfc3339(),
            },
        )
        .await
        .unwrap();

        install_skill_to_agent_impl(&pool, "brand-guidelines", "claude-code")
            .await
            .unwrap();

        let nested_link = agent_dir
            .join("anthropics")
            .join("skills")
            .join("brand-guidelines");
        let flat_link = agent_dir.join("brand-guidelines");
        assert!(
            flat_link.join("SKILL.md").exists(),
            "platform install should use a flat skill directory"
        );
        assert!(
            !nested_link.exists(),
            "platform install should not mirror central author/repo grouping"
        );
    }

    #[tokio::test]
    async fn test_copy_install_flattens_nested_canonical_relative_path() {
        let tmp = TempDir::new().unwrap();
        let central_dir = tmp.path().join("central");
        let agent_dir = tmp.path().join("claude");
        let nested_skill_dir = central_dir
            .join("anthropics")
            .join("skills")
            .join("copy-guidelines");
        fs::create_dir_all(&nested_skill_dir).unwrap();
        fs::write(
            nested_skill_dir.join("SKILL.md"),
            "---\nname: copy-guidelines\ndescription: Copy rules\n---\n",
        )
        .unwrap();

        let pool = setup_db(&central_dir, &agent_dir).await;
        db::upsert_skill(
            &pool,
            &db::Skill {
                id: "copy-guidelines".to_string(),
                name: "copy-guidelines".to_string(),
                description: Some("Copy rules".to_string()),
                file_path: nested_skill_dir
                    .join("SKILL.md")
                    .to_string_lossy()
                    .into_owned(),
                canonical_path: Some(nested_skill_dir.to_string_lossy().into_owned()),
                is_central: true,
                source: Some("github:anthropics/skills".to_string()),
                content: None,
                scanned_at: chrono::Utc::now().to_rfc3339(),
            },
        )
        .await
        .unwrap();

        install_skill_to_agent_copy_impl(&pool, "copy-guidelines", "claude-code")
            .await
            .unwrap();

        let nested_copy = agent_dir
            .join("anthropics")
            .join("skills")
            .join("copy-guidelines");
        let flat_copy = agent_dir.join("copy-guidelines");
        assert!(
            flat_copy.join("SKILL.md").exists(),
            "copy install should use a flat skill directory"
        );
        assert!(
            !nested_copy.exists(),
            "copy install should not mirror central author/repo grouping"
        );
    }

    #[tokio::test]
    async fn test_flat_symlink_install_replaces_previous_flat_installation() {
        let tmp = TempDir::new().unwrap();
        let central_dir = tmp.path().join("central");
        let agent_dir = tmp.path().join("claude");
        let nested_skill_dir = central_dir
            .join("anthropics")
            .join("skills")
            .join("migrated-link");
        fs::create_dir_all(&nested_skill_dir).unwrap();
        fs::write(
            nested_skill_dir.join("SKILL.md"),
            "---\nname: migrated-link\ndescription: Migrated link\n---\n",
        )
        .unwrap();

        let pool = setup_db(&central_dir, &agent_dir).await;
        db::upsert_skill(
            &pool,
            &db::Skill {
                id: "migrated-link".to_string(),
                name: "migrated-link".to_string(),
                description: Some("Migrated link".to_string()),
                file_path: nested_skill_dir
                    .join("SKILL.md")
                    .to_string_lossy()
                    .into_owned(),
                canonical_path: Some(nested_skill_dir.to_string_lossy().into_owned()),
                is_central: true,
                source: Some("github:anthropics/skills".to_string()),
                content: None,
                scanned_at: chrono::Utc::now().to_rfc3339(),
            },
        )
        .await
        .unwrap();
        fs::create_dir_all(&agent_dir).unwrap();
        let old_flat_path = agent_dir.join("migrated-link");
        create_symlink(&nested_skill_dir, &old_flat_path).unwrap();
        db::upsert_skill_installation(
            &pool,
            &SkillInstallation {
                skill_id: "migrated-link".to_string(),
                agent_id: "claude-code".to_string(),
                installed_path: old_flat_path.to_string_lossy().into_owned(),
                link_type: "symlink".to_string(),
                symlink_target: Some(nested_skill_dir.to_string_lossy().into_owned()),
                created_at: chrono::Utc::now().to_rfc3339(),
            },
        )
        .await
        .unwrap();

        install_skill_to_agent_impl(&pool, "migrated-link", "claude-code")
            .await
            .unwrap();

        assert!(
            old_flat_path.join("SKILL.md").exists(),
            "flat symlink path should still be the active install location"
        );
        assert!(!agent_dir.join("anthropics").exists());
    }

    #[tokio::test]
    async fn test_flat_copy_install_replaces_previous_flat_installation() {
        let tmp = TempDir::new().unwrap();
        let central_dir = tmp.path().join("central");
        let agent_dir = tmp.path().join("claude");
        let nested_skill_dir = central_dir
            .join("anthropics")
            .join("skills")
            .join("migrated-copy");
        fs::create_dir_all(&nested_skill_dir).unwrap();
        fs::write(
            nested_skill_dir.join("SKILL.md"),
            "---\nname: migrated-copy\ndescription: Migrated copy\n---\n",
        )
        .unwrap();

        let pool = setup_db(&central_dir, &agent_dir).await;
        db::upsert_skill(
            &pool,
            &db::Skill {
                id: "migrated-copy".to_string(),
                name: "migrated-copy".to_string(),
                description: Some("Migrated copy".to_string()),
                file_path: nested_skill_dir
                    .join("SKILL.md")
                    .to_string_lossy()
                    .into_owned(),
                canonical_path: Some(nested_skill_dir.to_string_lossy().into_owned()),
                is_central: true,
                source: Some("github:anthropics/skills".to_string()),
                content: None,
                scanned_at: chrono::Utc::now().to_rfc3339(),
            },
        )
        .await
        .unwrap();
        let old_flat_path = agent_dir.join("migrated-copy");
        fs::create_dir_all(&old_flat_path).unwrap();
        fs::write(old_flat_path.join("SKILL.md"), "---\nname: old\n---\n").unwrap();
        db::upsert_skill_installation(
            &pool,
            &SkillInstallation {
                skill_id: "migrated-copy".to_string(),
                agent_id: "claude-code".to_string(),
                installed_path: old_flat_path.to_string_lossy().into_owned(),
                link_type: "copy".to_string(),
                symlink_target: None,
                created_at: chrono::Utc::now().to_rfc3339(),
            },
        )
        .await
        .unwrap();

        install_skill_to_agent_copy_impl(&pool, "migrated-copy", "claude-code")
            .await
            .unwrap();

        assert!(old_flat_path.join("SKILL.md").exists());
        assert!(!agent_dir.join("anthropics").exists());
    }

    #[tokio::test]
    async fn test_install_fails_when_canonical_missing() {
        let tmp = TempDir::new().unwrap();
        let central_dir = tmp.path().join("central");
        let agent_dir = tmp.path().join("claude");
        fs::create_dir_all(&central_dir).unwrap();

        let pool = setup_db(&central_dir, &agent_dir).await;
        // Do NOT create the skill in central_dir.

        let result = install_skill_to_agent_impl(&pool, "nonexistent-skill", "claude-code").await;
        assert!(
            result.is_err(),
            "install should fail if canonical skill missing"
        );
    }

    #[tokio::test]
    async fn test_install_fails_for_unknown_agent() {
        let tmp = TempDir::new().unwrap();
        let central_dir = tmp.path().join("central");
        let agent_dir = tmp.path().join("claude");
        fs::create_dir_all(&central_dir).unwrap();

        let pool = setup_db(&central_dir, &agent_dir).await;
        create_central_skill(&central_dir, "some-skill");

        let result = install_skill_to_agent_impl(&pool, "some-skill", "nonexistent-agent").await;
        assert!(result.is_err(), "install should fail for unknown agent");
    }

    #[tokio::test]
    async fn test_install_to_central_agent_is_rejected() {
        let tmp = TempDir::new().unwrap();
        let central_dir = tmp.path().join("central");
        fs::create_dir_all(&central_dir).unwrap();

        let pool = setup_db(&central_dir, &tmp.path().join("claude")).await;
        create_central_skill(&central_dir, "self-skill");

        let result = install_skill_to_agent_impl(&pool, "self-skill", "central").await;
        assert!(
            result.is_err(),
            "installing to 'central' should be rejected"
        );
    }

    #[tokio::test]
    async fn test_install_replaces_existing_symlink() {
        let tmp = TempDir::new().unwrap();
        let central_dir = tmp.path().join("central");
        let agent_dir = tmp.path().join("claude");
        fs::create_dir_all(&central_dir).unwrap();
        fs::create_dir_all(&agent_dir).unwrap();

        let pool = setup_db(&central_dir, &agent_dir).await;
        create_central_skill(&central_dir, "re-link-skill");

        // Install once.
        install_skill_to_agent_impl(&pool, "re-link-skill", "claude-code")
            .await
            .unwrap();

        // Install again — should replace the existing symlink without error.
        let result = install_skill_to_agent_impl(&pool, "re-link-skill", "claude-code").await;
        assert!(result.is_ok(), "re-install should succeed: {:?}", result);
    }

    #[tokio::test]
    async fn test_install_refuses_to_overwrite_real_dir() {
        let tmp = TempDir::new().unwrap();
        let central_dir = tmp.path().join("central");
        let agent_dir = tmp.path().join("claude");
        fs::create_dir_all(&central_dir).unwrap();
        fs::create_dir_all(&agent_dir).unwrap();

        let pool = setup_db(&central_dir, &agent_dir).await;
        create_central_skill(&central_dir, "real-dir-skill");

        // Create a real (non-symlink) directory at the install location.
        fs::create_dir_all(agent_dir.join("real-dir-skill")).unwrap();

        let result = install_skill_to_agent_impl(&pool, "real-dir-skill", "claude-code").await;
        assert!(
            result.is_err(),
            "install should refuse to overwrite a real directory"
        );
    }

    // ── uninstall_skill_from_agent_impl ───────────────────────────────────────

    #[tokio::test]
    async fn test_uninstall_removes_symlink() {
        let tmp = TempDir::new().unwrap();
        let central_dir = tmp.path().join("central");
        let agent_dir = tmp.path().join("claude");
        fs::create_dir_all(&central_dir).unwrap();

        let pool = setup_db(&central_dir, &agent_dir).await;
        create_central_skill(&central_dir, "uninstall-skill");

        install_skill_to_agent_impl(&pool, "uninstall-skill", "claude-code")
            .await
            .unwrap();

        let symlink_path = agent_dir.join("uninstall-skill");
        assert!(symlink_path.exists() || fs::symlink_metadata(&symlink_path).is_ok());

        uninstall_skill_from_agent_impl(&pool, "uninstall-skill", "claude-code")
            .await
            .unwrap();

        assert!(
            fs::symlink_metadata(&symlink_path).is_err(),
            "symlink should have been removed"
        );
    }

    #[tokio::test]
    async fn test_uninstall_removes_db_record() {
        let tmp = TempDir::new().unwrap();
        let central_dir = tmp.path().join("central");
        let agent_dir = tmp.path().join("claude");
        fs::create_dir_all(&central_dir).unwrap();

        let pool = setup_db(&central_dir, &agent_dir).await;
        create_central_skill(&central_dir, "db-uninstall-skill");

        install_skill_to_agent_impl(&pool, "db-uninstall-skill", "claude-code")
            .await
            .unwrap();

        uninstall_skill_from_agent_impl(&pool, "db-uninstall-skill", "claude-code")
            .await
            .unwrap();

        let installations = db::get_skill_installations(&pool, "db-uninstall-skill")
            .await
            .unwrap();
        assert!(installations.is_empty(), "DB record should be removed");
    }

    #[tokio::test]
    async fn test_uninstall_refuses_real_dir() {
        let tmp = TempDir::new().unwrap();
        let central_dir = tmp.path().join("central");
        let agent_dir = tmp.path().join("claude");
        fs::create_dir_all(&agent_dir).unwrap();
        fs::create_dir_all(&central_dir).unwrap();

        let pool = setup_db(&central_dir, &agent_dir).await;

        // Place a real directory where the symlink would be.
        fs::create_dir_all(agent_dir.join("protected-skill")).unwrap();

        let result = uninstall_skill_from_agent_impl(&pool, "protected-skill", "claude-code").await;
        assert!(
            result.is_err(),
            "uninstall should refuse to delete a real directory"
        );

        // Ensure the directory still exists.
        assert!(
            agent_dir.join("protected-skill").is_dir(),
            "real directory should NOT have been deleted"
        );
    }

    #[tokio::test]
    async fn test_uninstall_nonexistent_path_still_cleans_db() {
        let tmp = TempDir::new().unwrap();
        let central_dir = tmp.path().join("central");
        let agent_dir = tmp.path().join("claude");
        fs::create_dir_all(&central_dir).unwrap();
        fs::create_dir_all(&agent_dir).unwrap();

        let pool = setup_db(&central_dir, &agent_dir).await;

        // Manually insert an installation record without creating the symlink.
        let installation = SkillInstallation {
            skill_id: "ghost-skill".to_string(),
            agent_id: "claude-code".to_string(),
            installed_path: agent_dir.join("ghost-skill").to_string_lossy().into_owned(),
            link_type: "symlink".to_string(),
            symlink_target: None,
            created_at: chrono::Utc::now().to_rfc3339(),
        };
        db::upsert_skill_installation(&pool, &installation)
            .await
            .unwrap();

        let result = uninstall_skill_from_agent_impl(&pool, "ghost-skill", "claude-code").await;
        assert!(result.is_ok(), "uninstall of missing path should succeed");

        let installations = db::get_skill_installations(&pool, "ghost-skill")
            .await
            .unwrap();
        assert!(installations.is_empty(), "DB record should be cleaned up");
    }

    #[tokio::test]
    async fn test_uninstall_universal_availability_without_record_is_noop() {
        let tmp = TempDir::new().unwrap();
        let central_dir = tmp.path().join(".agents/skills");
        fs::create_dir_all(&central_dir).unwrap();
        let pool = setup_db(&central_dir, &tmp.path().join("claude")).await;
        create_central_skill(&central_dir, "universal-skill");

        uninstall_skill_from_agent_impl(&pool, "universal-skill", "cursor")
            .await
            .unwrap();

        assert!(
            central_dir.join("universal-skill/SKILL.md").exists(),
            "uninstalling read-only universal availability must not delete the central skill"
        );
    }

    #[tokio::test]
    async fn test_uninstall_without_managed_record_keeps_external_symlink() {
        let tmp = TempDir::new().unwrap();
        let central_dir = tmp.path().join("central");
        let agent_dir = tmp.path().join("claude");
        let external_dir = tmp.path().join("external-skill");
        fs::create_dir_all(&central_dir).unwrap();
        fs::create_dir_all(&agent_dir).unwrap();
        fs::create_dir_all(&external_dir).unwrap();
        fs::write(
            external_dir.join("SKILL.md"),
            "---\nname: External Skill\n---\n",
        )
        .unwrap();

        let pool = setup_db(&central_dir, &agent_dir).await;
        let external_link = agent_dir.join("external-skill");
        create_symlink(&external_dir, &external_link).unwrap();

        let result = uninstall_skill_from_agent_impl(&pool, "external-skill", "claude-code").await;

        assert!(
            result.is_err(),
            "uninstall without a managed installation row should be rejected"
        );
        assert!(
            fs::symlink_metadata(&external_link)
                .unwrap()
                .file_type()
                .is_symlink(),
            "the external symlink must remain untouched"
        );
        assert_eq!(
            external_link.canonicalize().unwrap(),
            external_dir.canonicalize().unwrap()
        );
    }

    #[tokio::test]
    async fn test_uninstall_uses_recorded_installed_path_for_nested_skill() {
        let tmp = TempDir::new().unwrap();
        let central_dir = tmp.path().join("central");
        let agent_dir = tmp.path().join("hermes");
        let nested_installed = agent_dir.join("apple").join("apple-reminders");
        fs::create_dir_all(&central_dir).unwrap();
        fs::create_dir_all(&nested_installed).unwrap();
        fs::write(
            nested_installed.join("SKILL.md"),
            "---\nname: apple-reminders\ndescription: Nested platform\n---\n",
        )
        .unwrap();

        let pool = setup_db(&central_dir, &agent_dir).await;
        let installation = SkillInstallation {
            skill_id: "apple-reminders".to_string(),
            agent_id: "claude-code".to_string(),
            installed_path: nested_installed.to_string_lossy().into_owned(),
            link_type: "copy".to_string(),
            symlink_target: None,
            created_at: chrono::Utc::now().to_rfc3339(),
        };
        db::upsert_skill_installation(&pool, &installation)
            .await
            .unwrap();

        uninstall_skill_from_agent_impl(&pool, "apple-reminders", "claude-code")
            .await
            .unwrap();

        assert!(
            !nested_installed.exists(),
            "uninstall should remove the actual nested installed path"
        );
        assert!(
            !agent_dir.join("apple").exists(),
            "uninstall should prune empty parent folders left behind by nested installs"
        );
    }

    #[tokio::test]
    async fn test_uninstall_prunes_empty_nested_platform_folders() {
        let tmp = TempDir::new().unwrap();
        let central_dir = tmp.path().join("central");
        let agent_dir = tmp.path().join("claude");
        let nested_skill_dir = central_dir
            .join("mattpocock")
            .join("skills")
            .join("ask-matt");
        fs::create_dir_all(&nested_skill_dir).unwrap();
        fs::write(
            nested_skill_dir.join("SKILL.md"),
            "---\nname: ask-matt\ndescription: Nested central\n---\n",
        )
        .unwrap();

        let pool = setup_db(&central_dir, &agent_dir).await;
        db::upsert_skill(
            &pool,
            &db::Skill {
                id: "ask-matt".to_string(),
                name: "ask-matt".to_string(),
                description: Some("Nested central".to_string()),
                file_path: nested_skill_dir
                    .join("SKILL.md")
                    .to_string_lossy()
                    .into_owned(),
                canonical_path: Some(nested_skill_dir.to_string_lossy().into_owned()),
                is_central: true,
                source: Some("github:mattpocock/skills".to_string()),
                content: None,
                scanned_at: chrono::Utc::now().to_rfc3339(),
            },
        )
        .await
        .unwrap();

        install_skill_to_agent_impl(&pool, "ask-matt", "claude-code")
            .await
            .unwrap();
        assert!(agent_dir.join("ask-matt").join("SKILL.md").exists());
        assert!(!agent_dir.join("mattpocock").exists());

        uninstall_skill_from_agent_impl(&pool, "ask-matt", "claude-code")
            .await
            .unwrap();

        assert!(
            !agent_dir.join("ask-matt").exists(),
            "uninstall should remove the flat platform skill directory"
        );
        assert!(
            agent_dir.exists(),
            "the platform skills root itself must not be deleted"
        );
    }

    #[tokio::test]
    async fn test_prune_mirrored_skips_nested_dir_inside_central_and_does_not_fail() {
        let tmp = TempDir::new().unwrap();
        let central_dir = tmp.path().join("central");
        let nested_inside_central = central_dir.join(".agents").join("skills");
        let bundle = central_dir.join("alchaincyf");
        let skill_dir = bundle.join("darwin-skill");
        fs::create_dir_all(&nested_inside_central).unwrap();
        fs::create_dir_all(&skill_dir).unwrap();
        fs::write(
            skill_dir.join("SKILL.md"),
            "---\nname: darwin-skill\ndescription: Keep\n---\n",
        )
        .unwrap();

        let pool = setup_db(&central_dir, &nested_inside_central).await;
        prune_mirrored_relative_path_from_install_targets(&pool, "alchaincyf")
            .await
            .unwrap();

        assert!(
            skill_dir.join("SKILL.md").exists(),
            "pruning platform leftovers must not delete the Central Skills bundle itself"
        );
    }

    #[tokio::test]
    async fn test_uninstall_keeps_parent_folder_when_sibling_skill_remains() {
        let tmp = TempDir::new().unwrap();
        let central_dir = tmp.path().join("central");
        let agent_dir = tmp.path().join("claude");
        let keep_dir = central_dir
            .join("mattpocock")
            .join("skills")
            .join("keep-me");
        let drop_dir = central_dir
            .join("mattpocock")
            .join("skills")
            .join("drop-me");
        fs::create_dir_all(&keep_dir).unwrap();
        fs::create_dir_all(&drop_dir).unwrap();
        fs::write(
            keep_dir.join("SKILL.md"),
            "---\nname: keep-me\ndescription: Stay\n---\n",
        )
        .unwrap();
        fs::write(
            drop_dir.join("SKILL.md"),
            "---\nname: drop-me\ndescription: Go\n---\n",
        )
        .unwrap();

        let pool = setup_db(&central_dir, &agent_dir).await;
        for (id, dir) in [("keep-me", &keep_dir), ("drop-me", &drop_dir)] {
            db::upsert_skill(
                &pool,
                &db::Skill {
                    id: id.to_string(),
                    name: id.to_string(),
                    description: Some(id.to_string()),
                    file_path: dir.join("SKILL.md").to_string_lossy().into_owned(),
                    canonical_path: Some(dir.to_string_lossy().into_owned()),
                    is_central: true,
                    source: Some("github:mattpocock/skills".to_string()),
                    content: None,
                    scanned_at: chrono::Utc::now().to_rfc3339(),
                },
            )
            .await
            .unwrap();
            install_skill_to_agent_impl(&pool, id, "claude-code")
                .await
                .unwrap();
        }

        uninstall_skill_from_agent_impl(&pool, "drop-me", "claude-code")
            .await
            .unwrap();

        assert!(agent_dir.join("keep-me").join("SKILL.md").exists());
        assert!(!agent_dir.join("drop-me").exists());
        assert!(!agent_dir.join("mattpocock").exists());
    }

    // ── batch install ─────────────────────────────────────────────────────────

    #[tokio::test]
    async fn test_batch_install_multiple_agents() {
        let tmp = TempDir::new().unwrap();
        let central_dir = tmp.path().join("central");
        let claude_dir = tmp.path().join("claude");
        let workbuddy_dir = tmp.path().join("workbuddy");
        fs::create_dir_all(&central_dir).unwrap();

        let pool = setup_db(&central_dir, &claude_dir).await;

        // Override a second non-universal agent's dir too.
        pool.write(|d| {
            if let Some(row) = d.table_mut("agents").get_mut("workbuddy") {
                row["global_skills_dir"] = serde_json::json!(workbuddy_dir.to_str().unwrap());
            }
            Ok(())
        })
        .await
        .unwrap();

        create_central_skill(&central_dir, "batch-skill");

        let result = batch_install_impl(
            &pool,
            "batch-skill",
            &["claude-code".to_string(), "workbuddy".to_string()],
        )
        .await;

        assert_eq!(result.succeeded.len(), 2);
        assert!(result.failed.is_empty());

        assert!(fs::symlink_metadata(claude_dir.join("batch-skill")).is_ok());
        assert!(fs::symlink_metadata(workbuddy_dir.join("batch-skill")).is_ok());
    }

    #[tokio::test]
    async fn test_batch_install_partial_failure() {
        let tmp = TempDir::new().unwrap();
        let central_dir = tmp.path().join("central");
        let claude_dir = tmp.path().join("claude");
        fs::create_dir_all(&central_dir).unwrap();

        let pool = setup_db(&central_dir, &claude_dir).await;
        create_central_skill(&central_dir, "partial-skill");

        let result = batch_install_impl(
            &pool,
            "partial-skill",
            &[
                "claude-code".to_string(),
                "nonexistent-agent".to_string(), // will fail
            ],
        )
        .await;

        assert_eq!(result.succeeded.len(), 1);
        assert_eq!(result.failed.len(), 1);
        assert_eq!(result.failed[0].agent_id, "nonexistent-agent");
    }

    /// Helper that mirrors `batch_install_to_agents` but works with a raw pool
    /// (no Tauri State).
    async fn batch_install_impl(
        pool: &AppStore,
        skill_id: &str,
        agent_ids: &[String],
    ) -> BatchInstallResult {
        let mut succeeded = Vec::new();
        let mut failed = Vec::new();

        for agent_id in agent_ids {
            match install_skill_to_agent_impl(pool, skill_id, agent_id).await {
                Ok(_) => succeeded.push(agent_id.clone()),
                Err(e) => failed.push(FailedInstall {
                    agent_id: agent_id.clone(),
                    error: e,
                }),
            }
        }

        BatchInstallResult { succeeded, failed }
    }

    // ── install_skill_to_agent_copy_impl ──────────────────────────────────────

    #[tokio::test]
    async fn test_copy_install_creates_real_directory() {
        let tmp = TempDir::new().unwrap();
        let central_dir = tmp.path().join("central");
        let agent_dir = tmp.path().join("claude");
        fs::create_dir_all(&central_dir).unwrap();

        let pool = setup_db(&central_dir, &agent_dir).await;
        create_central_skill(&central_dir, "copy-skill");

        let result = install_skill_to_agent_copy_impl(&pool, "copy-skill", "claude-code").await;
        assert!(result.is_ok(), "copy install should succeed: {:?}", result);

        let target = agent_dir.join("copy-skill");
        let meta = fs::symlink_metadata(&target).unwrap();
        // Must be a real directory — NOT a symlink.
        assert!(
            meta.is_dir() && !meta.file_type().is_symlink(),
            "installed path should be a real directory, not a symlink"
        );
    }

    #[tokio::test]
    async fn test_copy_install_files_are_copied() {
        let tmp = TempDir::new().unwrap();
        let central_dir = tmp.path().join("central");
        let agent_dir = tmp.path().join("claude");
        fs::create_dir_all(&central_dir).unwrap();

        let pool = setup_db(&central_dir, &agent_dir).await;

        // Create skill with multiple files to verify all are copied.
        let skill_dir = central_dir.join("multi-file-skill");
        fs::create_dir_all(&skill_dir).unwrap();
        fs::write(
            skill_dir.join("SKILL.md"),
            "---\nname: multi-file-skill\ndescription: Test\n---\n",
        )
        .unwrap();
        fs::write(skill_dir.join("extra.txt"), "extra content").unwrap();

        install_skill_to_agent_copy_impl(&pool, "multi-file-skill", "claude-code")
            .await
            .unwrap();

        let installed_skill_dir = agent_dir.join("multi-file-skill");

        // Verify SKILL.md was copied.
        let skill_md = installed_skill_dir.join("SKILL.md");
        assert!(skill_md.exists(), "SKILL.md should be copied to agent dir");

        // Verify extra file was copied.
        let extra = installed_skill_dir.join("extra.txt");
        assert!(extra.exists(), "extra.txt should be copied to agent dir");
        assert_eq!(
            fs::read_to_string(&extra).unwrap(),
            "extra content",
            "copied file contents should match"
        );

        // Confirm that the installed path is NOT a symlink.
        let meta = fs::symlink_metadata(&installed_skill_dir).unwrap();
        assert!(
            !meta.file_type().is_symlink(),
            "installed directory must NOT be a symlink"
        );
    }

    #[tokio::test]
    async fn test_copy_install_updates_db_with_copy_type() {
        let tmp = TempDir::new().unwrap();
        let central_dir = tmp.path().join("central");
        let agent_dir = tmp.path().join("claude");
        fs::create_dir_all(&central_dir).unwrap();

        let pool = setup_db(&central_dir, &agent_dir).await;
        create_central_skill(&central_dir, "db-copy-skill");

        install_skill_to_agent_copy_impl(&pool, "db-copy-skill", "claude-code")
            .await
            .unwrap();

        let installations = db::get_skill_installations(&pool, "db-copy-skill")
            .await
            .unwrap();
        assert_eq!(installations.len(), 1);
        assert_eq!(installations[0].agent_id, "claude-code");
        assert_eq!(
            installations[0].link_type, "copy",
            "DB should record link_type as 'copy'"
        );
    }

    #[tokio::test]
    async fn test_copy_install_to_central_is_rejected() {
        let tmp = TempDir::new().unwrap();
        let central_dir = tmp.path().join("central");
        fs::create_dir_all(&central_dir).unwrap();

        let pool = setup_db(&central_dir, &tmp.path().join("claude")).await;
        create_central_skill(&central_dir, "self-copy-skill");

        let result = install_skill_to_agent_copy_impl(&pool, "self-copy-skill", "central").await;
        assert!(
            result.is_err(),
            "copy install to 'central' should be rejected"
        );
    }

    #[tokio::test]
    async fn test_add_resource_skill_to_central_flattens_source_group_path() {
        let tmp = TempDir::new().unwrap();
        let central_dir = tmp.path().join("central");
        let agent_dir = tmp.path().join("claude");
        let resource_dir = tmp.path().join("resource");
        let grouped_resource_dir = resource_dir.join("owner").join("repo");
        fs::create_dir_all(&central_dir).unwrap();
        fs::create_dir_all(&agent_dir).unwrap();
        fs::create_dir_all(&grouped_resource_dir).unwrap();

        let pool = setup_db(&central_dir, &agent_dir).await;
        db::set_skill_resource_library_dir(&pool, &resource_dir.to_string_lossy())
            .await
            .unwrap();
        create_resource_skill(&pool, &grouped_resource_dir, "resource-only").await;

        let result = add_resource_skill_to_central_impl(&pool, "resource-only")
            .await
            .unwrap();

        let expected_dir = central_dir.join("resource-only");
        let legacy_group_dir = central_dir.join("owner");
        let resource_skill = grouped_resource_dir.join("resource-only");
        assert_eq!(PathBuf::from(result.central_path), expected_dir);
        assert!(expected_dir.join("SKILL.md").exists());
        assert!(!legacy_group_dir.exists());
        assert!(resource_skill.join("SKILL.md").exists());
        assert!(
            fs::symlink_metadata(&expected_dir)
                .unwrap()
                .file_type()
                .is_symlink(),
            "adding a resource skill to Central Skills should create a symlink, not a copy"
        );

        let skill = db::get_skill_by_id(&pool, "resource-only")
            .await
            .unwrap()
            .unwrap();
        assert!(skill.is_central);
        assert_eq!(
            skill.canonical_path.as_deref(),
            Some(expected_dir.to_string_lossy().as_ref())
        );
    }

    #[tokio::test]
    async fn test_remove_resource_skill_from_central_restores_resource_source() {
        let tmp = TempDir::new().unwrap();
        let central_dir = tmp.path().join("central");
        let agent_dir = tmp.path().join("claude");
        let resource_dir = tmp.path().join("resource");
        let grouped_resource_dir = resource_dir.join("owner").join("repo");
        fs::create_dir_all(&central_dir).unwrap();
        fs::create_dir_all(&agent_dir).unwrap();
        fs::create_dir_all(&grouped_resource_dir).unwrap();

        let pool = setup_db(&central_dir, &agent_dir).await;
        db::set_skill_resource_library_dir(&pool, &resource_dir.to_string_lossy())
            .await
            .unwrap();
        create_resource_skill(&pool, &grouped_resource_dir, "resource-only").await;
        add_resource_skill_to_central_impl(&pool, "resource-only")
            .await
            .unwrap();

        let central_skill = central_dir.join("resource-only");
        let resource_skill = grouped_resource_dir.join("resource-only");
        assert!(fs::symlink_metadata(&central_skill)
            .unwrap()
            .file_type()
            .is_symlink());
        assert!(central_skill.join("SKILL.md").exists());
        assert!(resource_skill.join("SKILL.md").exists());

        let result = remove_resource_skill_from_central_impl(&pool, "resource-only")
            .await
            .unwrap();

        assert_eq!(PathBuf::from(result.resource_path), resource_skill);
        assert!(!central_skill.exists());
        assert!(resource_skill.join("SKILL.md").exists());
        let restored = db::get_skill_by_id(&pool, "resource-only")
            .await
            .unwrap()
            .unwrap();
        assert!(!restored.is_central);
        assert_eq!(
            restored.canonical_path.as_deref(),
            Some(resource_skill.to_string_lossy().as_ref())
        );
        assert!(db::get_skill_installations(&pool, "resource-only")
            .await
            .unwrap()
            .is_empty());
    }

    #[tokio::test]
    async fn test_add_resource_skill_to_central_does_not_mirror_github_group_to_platform() {
        let tmp = TempDir::new().unwrap();
        let central_dir = tmp.path().join("central");
        let resource_dir = tmp.path().join("resource-library");
        let agent_dir = tmp.path().join("claude").join("skills");
        let grouped = resource_dir.join("nested-resource-skill");
        fs::create_dir_all(&central_dir).unwrap();
        fs::create_dir_all(&agent_dir).unwrap();
        fs::create_dir_all(&grouped).unwrap();

        let pool = setup_db(&central_dir, &agent_dir).await;
        db::set_skill_resource_library_dir(&pool, &resource_dir.to_string_lossy())
            .await
            .unwrap();
        pool.write(|d| {
            for (id, row) in d.table_mut("agents") {
                if id != "central" && id != "claude-code" {
                    row["is_enabled"] = false.into();
                }
            }
            Ok(())
        })
        .await
        .unwrap();
        create_resource_skill(&pool, &grouped, "canvas-design").await;

        add_resource_skill_to_central_impl(&pool, "canvas-design")
            .await
            .unwrap();

        assert!(central_dir.join("canvas-design").join("SKILL.md").exists());
        assert!(agent_dir.join("canvas-design").join("SKILL.md").exists());
        assert!(!central_dir.join("nested-resource-skill").exists());
        assert!(!agent_dir.join("nested-resource-skill").exists());
    }

    #[tokio::test]
    async fn test_reconcile_orphaned_central_skills_after_parent_folder_deleted() {
        let tmp = TempDir::new().unwrap();
        let central_dir = tmp.path().join("central");
        let resource_dir = tmp.path().join("resource-library");
        let agent_dir = tmp.path().join("claude").join("skills");
        let grouped = resource_dir.join("mattpocock").join("skills");
        fs::create_dir_all(&central_dir).unwrap();
        fs::create_dir_all(&agent_dir).unwrap();
        fs::create_dir_all(&grouped).unwrap();

        let pool = setup_db(&central_dir, &agent_dir).await;
        db::set_skill_resource_library_dir(&pool, &resource_dir.to_string_lossy())
            .await
            .unwrap();
        pool.write(|d| {
            for (id, row) in d.table_mut("agents") {
                if id != "central" && id != "claude-code" {
                    row["is_enabled"] = false.into();
                }
            }
            Ok(())
        })
        .await
        .unwrap();
        create_resource_skill(&pool, &grouped, "ask-matt").await;
        add_resource_skill_to_central_impl(&pool, "ask-matt")
            .await
            .unwrap();

        let central_skill = central_dir.join("ask-matt");
        let platform_skill = agent_dir.join("ask-matt");
        assert!(central_skill.exists());
        assert!(platform_skill.join("SKILL.md").exists());

        remove_symlink_path(&central_skill).unwrap();

        reconcile_orphaned_central_skills(&pool).await.unwrap();

        let restored = db::get_skill_by_id(&pool, "ask-matt")
            .await
            .unwrap()
            .unwrap();
        assert!(!restored.is_central);
        assert!(!platform_skill.exists());
        assert!(grouped.join("ask-matt").join("SKILL.md").exists());
        assert!(db::get_central_skills(&pool).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn test_reconcile_removes_dangling_central_symlink_and_platform_link() {
        let tmp = TempDir::new().unwrap();
        let central_dir = tmp.path().join("central");
        let agent_dir = tmp.path().join("claude").join("skills");
        let missing_target = tmp
            .path()
            .join("deleted-resource")
            .join("nested-resource-skill");
        fs::create_dir_all(&central_dir).unwrap();
        fs::create_dir_all(&agent_dir).unwrap();

        let pool = setup_db(&central_dir, &agent_dir).await;
        pool.write(|d| {
            for (id, row) in d.table_mut("agents") {
                if id != "central" && id != "claude-code" {
                    row["is_enabled"] = false.into();
                }
            }
            Ok(())
        })
        .await
        .unwrap();

        let central_link = central_dir.join("nested-resource-skill");
        create_symlink(&missing_target, &central_link).unwrap();
        assert!(fs::symlink_metadata(&central_link)
            .unwrap()
            .file_type()
            .is_symlink());
        assert!(!central_link.join("SKILL.md").exists());

        db::upsert_skill(
            &pool,
            &db::Skill {
                id: "nested-resource-skill".to_string(),
                name: "nested-resource-skill".to_string(),
                description: Some("stale central link".to_string()),
                file_path: central_link.join("SKILL.md").to_string_lossy().into_owned(),
                canonical_path: Some(central_link.to_string_lossy().into_owned()),
                is_central: true,
                source: Some("resource-library".to_string()),
                content: None,
                scanned_at: chrono::Utc::now().to_rfc3339(),
            },
        )
        .await
        .unwrap();

        let platform_link = agent_dir.join("nested-resource-skill");
        create_symlink(&central_link, &platform_link).unwrap();
        db::upsert_skill_installation(
            &pool,
            &SkillInstallation {
                skill_id: "nested-resource-skill".to_string(),
                agent_id: "claude-code".to_string(),
                installed_path: platform_link.to_string_lossy().into_owned(),
                link_type: "symlink".to_string(),
                symlink_target: Some(central_link.to_string_lossy().into_owned()),
                created_at: chrono::Utc::now().to_rfc3339(),
            },
        )
        .await
        .unwrap();

        reconcile_orphaned_central_skills(&pool).await.unwrap();

        assert!(fs::symlink_metadata(&central_link).is_err());
        assert!(fs::symlink_metadata(&platform_link).is_err());
        assert!(db::get_skill_by_id(&pool, "nested-resource-skill")
            .await
            .unwrap()
            .is_none());
        assert!(db::get_skill_installations(&pool, "nested-resource-skill")
            .await
            .unwrap()
            .is_empty());
    }

    #[tokio::test]
    async fn test_add_resource_skill_to_central_syncs_to_detected_enabled_platforms() {
        let tmp = TempDir::new().unwrap();
        let central_dir = tmp.path().join("central");
        let resource_dir = tmp.path().join("resource-library");
        let agent_dir = tmp.path().join("claude").join("skills");
        fs::create_dir_all(&central_dir).unwrap();
        fs::create_dir_all(agent_dir.parent().unwrap()).unwrap();

        let pool = setup_db(&central_dir, &agent_dir).await;
        db::set_skill_resource_library_dir(&pool, &resource_dir.to_string_lossy())
            .await
            .unwrap();
        pool.write(|d| {
            for row in d.table_mut("agents").values_mut() {
                if row["id"] != "central" {
                    row["is_enabled"] = false.into();
                }
            }
            Ok(())
        })
        .await
        .unwrap();
        pool.write(|d| {
            d.table_mut("agents").get_mut("claude-code").unwrap()["is_enabled"] = true.into();
            Ok(())
        })
        .await
        .unwrap();
        create_resource_skill(&pool, &resource_dir, "shared-skill").await;

        add_resource_skill_to_central_impl(&pool, "shared-skill")
            .await
            .unwrap();

        let central_skill = central_dir.join("shared-skill");
        let platform_skill = agent_dir.join("shared-skill");
        assert!(central_skill.join("SKILL.md").exists());
        assert!(
            fs::symlink_metadata(&central_skill)
                .unwrap()
                .file_type()
                .is_symlink(),
            "adding a resource skill to Central Skills should create a symlink"
        );
        assert!(
            platform_skill.join("SKILL.md").exists(),
            "adding a resource skill to Central Skills must synchronize it to detected enabled platforms"
        );
        assert!(fs::symlink_metadata(platform_skill).unwrap().is_symlink());
    }

    #[tokio::test]
    async fn test_add_resource_skill_to_central_replaces_legacy_nested_platform_directory() {
        let tmp = TempDir::new().unwrap();
        let central_dir = tmp.path().join("central");
        let resource_dir = tmp.path().join("resource-library");
        let agent_dir = tmp.path().join("claude").join("skills");
        fs::create_dir_all(&central_dir).unwrap();
        fs::create_dir_all(&agent_dir).unwrap();

        let pool = setup_db(&central_dir, &agent_dir).await;
        db::set_skill_resource_library_dir(&pool, &resource_dir.to_string_lossy())
            .await
            .unwrap();
        pool.write(|d| {
            for (id, row) in d.table_mut("agents") {
                if id != "central" && id != "claude-code" {
                    row["is_enabled"] = false.into();
                }
            }
            Ok(())
        })
        .await
        .unwrap();
        create_resource_skill(&pool, &resource_dir, "academy-guide").await;

        let legacy_path = agent_dir
            .join("anthropics")
            .join("skills")
            .join("academy-guide");
        fs::create_dir_all(&legacy_path).unwrap();
        fs::write(
            legacy_path.join("SKILL.md"),
            "---\nname: academy-guide\ndescription: Legacy install\n---\n",
        )
        .unwrap();
        db::upsert_skill_installation(
            &pool,
            &SkillInstallation {
                skill_id: "academy-guide".to_string(),
                agent_id: "claude-code".to_string(),
                installed_path: legacy_path.to_string_lossy().into_owned(),
                link_type: "symlink".to_string(),
                symlink_target: Some(
                    resource_dir
                        .join("academy-guide")
                        .to_string_lossy()
                        .into_owned(),
                ),
                created_at: chrono::Utc::now().to_rfc3339(),
            },
        )
        .await
        .unwrap();

        add_resource_skill_to_central_impl(&pool, "academy-guide")
            .await
            .unwrap();

        let flat_path = agent_dir.join("academy-guide");
        assert!(!legacy_path.exists());
        assert!(flat_path.join("SKILL.md").exists());
        assert!(fs::symlink_metadata(&flat_path)
            .unwrap()
            .file_type()
            .is_symlink());
        assert!(!agent_dir.join("anthropics").exists());
    }

    #[tokio::test]
    async fn test_add_resource_skill_to_central_syncs_to_active_project_directories() {
        let tmp = TempDir::new().unwrap();
        let central_dir = tmp.path().join("central");
        let resource_dir = tmp.path().join("resource-library");
        let agent_dir = tmp.path().join("claude").join("skills");
        let project_dir = tmp.path().join("project");
        fs::create_dir_all(&central_dir).unwrap();
        fs::create_dir_all(&resource_dir).unwrap();
        fs::create_dir_all(&agent_dir).unwrap();
        fs::create_dir_all(&project_dir).unwrap();

        let pool = setup_db(&central_dir, &agent_dir).await;
        pool.write(|d| {
            for row in d.table_mut("agents").values_mut() {
                if row["id"] != "central" {
                    row["is_enabled"] = false.into();
                }
            }
            Ok(())
        })
        .await
        .unwrap();
        db::set_skill_resource_library_dir(&pool, &resource_dir.to_string_lossy())
            .await
            .unwrap();
        let scan_dir =
            db::add_scan_directory(&pool, &project_dir.to_string_lossy(), Some("Project"))
                .await
                .unwrap();
        create_resource_skill(&pool, &resource_dir, "project-shared").await;

        add_resource_skill_to_central_impl(&pool, "project-shared")
            .await
            .unwrap();

        let project_skill = project_dir
            .join(PROJECT_SKILLS_SUBDIR)
            .join("project-shared");
        assert!(
            project_skill.join("SKILL.md").exists(),
            "adding a resource skill to Central Skills must synchronize it to active project directories"
        );

        let installs = db::get_skill_installations(&pool, "project-shared")
            .await
            .unwrap();
        assert!(installs
            .iter()
            .any(|install| install.agent_id == project_agent_id(scan_dir.id)));
    }

    #[tokio::test]
    async fn test_add_resource_skill_to_central_preserves_github_source_metadata() {
        let tmp = TempDir::new().unwrap();
        let central_dir = tmp.path().join("central");
        let agent_dir = tmp.path().join("claude");
        let resource_dir = tmp.path().join("resource");
        fs::create_dir_all(&central_dir).unwrap();
        fs::create_dir_all(&agent_dir).unwrap();
        fs::create_dir_all(&resource_dir).unwrap();

        let pool = setup_db(&central_dir, &agent_dir).await;
        db::set_skill_resource_library_dir(&pool, &resource_dir.to_string_lossy())
            .await
            .unwrap();
        create_resource_skill(&pool, &resource_dir, "nested-resource-skill").await;
        let mut skill = db::get_skill_by_id(&pool, "nested-resource-skill")
            .await
            .unwrap()
            .unwrap();
        skill.source = Some("github:example/skills".to_string());
        db::upsert_skill(&pool, &skill).await.unwrap();
        db::upsert_skill_source(
            &pool,
            &db::SkillSource {
                skill_id: "nested-resource-skill".to_string(),
                source_type: "github".to_string(),
                source_url: Some(
                    "https://raw.githubusercontent.com/example/skills/main/SKILL.md".to_string(),
                ),
                source_author: Some("example".to_string()),
                source_repo: Some("example/skills".to_string()),
                source_path: Some("skills/nested-resource-skill/SKILL.md".to_string()),
                updated_at: chrono::Utc::now().to_rfc3339(),
            },
        )
        .await
        .unwrap();

        add_resource_skill_to_central_impl(&pool, "nested-resource-skill")
            .await
            .unwrap();

        let promoted = db::get_skill_by_id(&pool, "nested-resource-skill")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(promoted.source.as_deref(), Some("github:example/skills"));
        let source = db::get_skill_source(&pool, "nested-resource-skill")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(source.source_type, "github");
        assert_eq!(source.source_repo.as_deref(), Some("example/skills"));
    }

    #[tokio::test]
    async fn test_copy_install_fails_when_canonical_missing() {
        let tmp = TempDir::new().unwrap();
        let central_dir = tmp.path().join("central");
        let agent_dir = tmp.path().join("claude");
        fs::create_dir_all(&central_dir).unwrap();

        let pool = setup_db(&central_dir, &agent_dir).await;
        // Deliberately do NOT create the skill in central_dir.

        let result = install_skill_to_agent_copy_impl(&pool, "missing-skill", "claude-code").await;
        assert!(
            result.is_err(),
            "copy install should fail when canonical skill is missing"
        );
    }

    #[tokio::test]
    async fn test_copy_install_refuses_to_overwrite_real_dir() {
        let tmp = TempDir::new().unwrap();
        let central_dir = tmp.path().join("central");
        let agent_dir = tmp.path().join("claude");
        fs::create_dir_all(&central_dir).unwrap();
        fs::create_dir_all(&agent_dir).unwrap();

        let pool = setup_db(&central_dir, &agent_dir).await;
        create_central_skill(&central_dir, "existing-dir-skill");

        // Create a real directory at the target location.
        fs::create_dir_all(agent_dir.join("existing-dir-skill")).unwrap();

        let result =
            install_skill_to_agent_copy_impl(&pool, "existing-dir-skill", "claude-code").await;
        assert!(
            result.is_err(),
            "copy install should refuse to overwrite an existing real directory"
        );
    }

    // ── uninstall (copy) ──────────────────────────────────────────────────────

    #[tokio::test]
    async fn test_uninstall_removes_copied_directory() {
        let tmp = TempDir::new().unwrap();
        let central_dir = tmp.path().join("central");
        let agent_dir = tmp.path().join("claude");
        fs::create_dir_all(&central_dir).unwrap();

        let pool = setup_db(&central_dir, &agent_dir).await;
        create_central_skill(&central_dir, "uninstall-copy-skill");

        // First, install via copy.
        install_skill_to_agent_copy_impl(&pool, "uninstall-copy-skill", "claude-code")
            .await
            .unwrap();

        let target = agent_dir.join("uninstall-copy-skill");
        assert!(
            target.is_dir(),
            "copied directory should exist before uninstall"
        );

        // Now uninstall.
        uninstall_skill_from_agent_impl(&pool, "uninstall-copy-skill", "claude-code")
            .await
            .unwrap();

        assert!(
            fs::symlink_metadata(&target).is_err(),
            "copied directory should have been removed after uninstall"
        );
    }

    #[tokio::test]
    async fn test_uninstall_copy_removes_db_record() {
        let tmp = TempDir::new().unwrap();
        let central_dir = tmp.path().join("central");
        let agent_dir = tmp.path().join("claude");
        fs::create_dir_all(&central_dir).unwrap();

        let pool = setup_db(&central_dir, &agent_dir).await;
        create_central_skill(&central_dir, "db-copy-uninstall-skill");

        install_skill_to_agent_copy_impl(&pool, "db-copy-uninstall-skill", "claude-code")
            .await
            .unwrap();

        uninstall_skill_from_agent_impl(&pool, "db-copy-uninstall-skill", "claude-code")
            .await
            .unwrap();

        let installations = db::get_skill_installations(&pool, "db-copy-uninstall-skill")
            .await
            .unwrap();
        assert!(
            installations.is_empty(),
            "DB record should be removed after uninstall"
        );
    }

    #[tokio::test]
    async fn test_uninstall_refuses_real_dir_without_copy_record() {
        let tmp = TempDir::new().unwrap();
        let central_dir = tmp.path().join("central");
        let agent_dir = tmp.path().join("claude");
        fs::create_dir_all(&agent_dir).unwrap();
        fs::create_dir_all(&central_dir).unwrap();

        let pool = setup_db(&central_dir, &agent_dir).await;

        // Place a real directory with NO DB record as 'copy' type.
        fs::create_dir_all(agent_dir.join("protected-skill")).unwrap();

        let result = uninstall_skill_from_agent_impl(&pool, "protected-skill", "claude-code").await;
        assert!(
            result.is_err(),
            "uninstall should refuse to delete a real directory without a copy record"
        );

        // Ensure the directory still exists.
        assert!(
            agent_dir.join("protected-skill").is_dir(),
            "real directory should NOT have been deleted"
        );
    }

    #[tokio::test]
    async fn test_batch_install_uses_copy_method() {
        let tmp = TempDir::new().unwrap();
        let central_dir = tmp.path().join("central");
        let agent_dir = tmp.path().join("claude");
        fs::create_dir_all(&central_dir).unwrap();

        let pool = setup_db(&central_dir, &agent_dir).await;
        create_central_skill(&central_dir, "batch-copy-skill");

        let mut succeeded = Vec::new();
        let mut failed = Vec::new();
        for agent_id in &["claude-code".to_string()] {
            match install_skill_to_agent_copy_impl(&pool, "batch-copy-skill", agent_id).await {
                Ok(_) => succeeded.push(agent_id.clone()),
                Err(e) => failed.push(FailedInstall {
                    agent_id: agent_id.clone(),
                    error: e,
                }),
            }
        }

        assert_eq!(succeeded.len(), 1);
        assert!(failed.is_empty());

        // The installed directory must NOT be a symlink.
        let target = agent_dir.join("batch-copy-skill");
        let meta = fs::symlink_metadata(&target).unwrap();
        assert!(
            !meta.file_type().is_symlink(),
            "batch copy install should create a real directory"
        );
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct InstallationRelocation {
    record: SkillInstallation,
    previous_target: Option<PathBuf>,
    source: PathBuf,
}

async fn validate_relocation_path(
    pool: &AppStore,
    record: &SkillInstallation,
) -> Result<PathBuf, String> {
    let target = install_target_for_agent_id(pool, &record.agent_id).await?;
    let path = PathBuf::from(&record.installed_path);
    let relative = path
        .strip_prefix(&target.skills_dir)
        .map_err(|_| "UNSAFE_INSTALL_TARGET")?;
    if relative.as_os_str().is_empty()
        || !relative
            .components()
            .all(|p| matches!(p, Component::Normal(_)))
    {
        return Err("UNSAFE_INSTALL_TARGET".into());
    }
    let mut ancestor = target.skills_dir.clone();
    for part in relative.parent().unwrap_or(Path::new("")).components() {
        ancestor.push(part);
        if std::fs::symlink_metadata(&ancestor).is_ok_and(|m| m.file_type().is_symlink()) {
            return Err("UNSAFE_INSTALL_TARGET".into());
        }
    }
    Ok(path)
}

/// Capture exact link entries before moving a source. Copy installations are untouched.
pub(crate) async fn prepare_installation_relocation_impl(
    pool: &AppStore,
    skill_id: &str,
    old: &Path,
    new: &Path,
) -> Result<Vec<InstallationRelocation>, String> {
    let mut result = Vec::new();
    for record in db::get_skill_installations(pool, skill_id).await? {
        let entry = Path::new(&record.installed_path);
        if old.parent().is_some_and(|p| entry.starts_with(p))
            || new.parent().is_some_and(|p| entry.starts_with(p))
        {
            return Err("UNSAFE_INSTALL_TARGET".into());
        }
        if record.link_type == "copy" {
            continue;
        }
        if record.link_type != "symlink" {
            return Err("UNSAFE_INSTALL_TARGET".into());
        }
        let path = validate_relocation_path(pool, &record).await?;
        let previous_target = match std::fs::symlink_metadata(&path) {
            Ok(meta)
                if meta.file_type().is_symlink()
                    && crate::path_utils::paths_resolve_to_same_entry(&path, old) =>
            {
                Some(std::fs::read_link(&path).map_err(|_| "UNSAFE_INSTALL_TARGET")?)
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
            _ => return Err("UNSAFE_INSTALL_TARGET".into()),
        };
        result.push(InstallationRelocation {
            record,
            previous_target,
            source: new.to_path_buf(),
        });
    }
    Ok(result)
}

/// Idempotent journal replay repairs recorded links at their original installation locations.
pub(crate) async fn apply_installation_relocation_impl(
    pool: &AppStore,
    plan: &InstallationRelocation,
) -> Result<(), String> {
    let root = db::get_skill_resource_library_dir(pool).await?;
    super::repository_mirror::ensure_owned_path(&root, &plan.source)?;
    if !plan.source.join("SKILL.md").is_file() {
        return Err("WRITE_FAILED".into());
    }
    let path = validate_relocation_path(pool, &plan.record).await?;
    let parent = path.parent().ok_or("UNSAFE_INSTALL_TARGET")?;
    let target = symlink_target_path(parent, &plan.source);
    match std::fs::symlink_metadata(&path) {
        Ok(meta) if meta.file_type().is_symlink() => {
            if !crate::path_utils::paths_resolve_to_same_entry(&path, &plan.source) {
                let existing = std::fs::read_link(&path).map_err(|_| "UNSAFE_INSTALL_TARGET")?;
                if Some(&existing) != plan.previous_target.as_ref() && existing != target {
                    return Err("UNSAFE_INSTALL_TARGET".into());
                }
                remove_symlink_path(&path).map_err(|_| "WRITE_FAILED")?;
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        _ => return Err("UNSAFE_INSTALL_TARGET".into()),
    }
    if std::fs::symlink_metadata(&path).is_err() {
        std::fs::create_dir_all(parent).map_err(|_| "WRITE_FAILED")?;
        create_symlink(&target, &path)?;
    }
    if !crate::path_utils::paths_resolve_to_same_entry(&path, &plan.source) {
        return Err("WRITE_FAILED".into());
    }
    let mut record = plan.record.clone();
    record.symlink_target = Some(path_to_string(&plan.source));
    db::upsert_skill_installation(pool, &record).await?;
    pool.write(|d| {
        for row in d.table_mut("agent_skill_observations").values_mut() {
            if row["agent_id"] == record.agent_id
                && row["skill_id"] == record.skill_id
                && row["dir_path"] == record.installed_path
            {
                row["symlink_target"] = serde_json::json!(record.symlink_target);
            }
        }
        Ok(())
    })
    .await
    .map_err(|_| "WRITE_FAILED")?;
    Ok(())
}

/// Strict mirror refreshes only recorded targets; copy edits intentionally follow the source.
pub(crate) async fn refresh_resource_installations_impl(
    pool: &AppStore,
    skill_id: &str,
) -> Result<(), String> {
    let source = resource_source_dir_for_skill(pool, skill_id).await?;
    let expected = super::remote_sources::local_skill_files(&source)?;
    for record in db::get_skill_installations(pool, skill_id).await? {
        let target = install_target_for_agent_id(pool, &record.agent_id).await?;
        let path = PathBuf::from(&record.installed_path);
        let relative = path
            .strip_prefix(&target.skills_dir)
            .map_err(|_| "UNSAFE_INSTALL_TARGET")?;
        if relative.as_os_str().is_empty()
            || !relative
                .components()
                .all(|part| matches!(part, Component::Normal(_)))
        {
            return Err("UNSAFE_INSTALL_TARGET".into());
        }
        let mut ancestor = target.skills_dir.clone();
        for part in relative.parent().unwrap_or(Path::new("")).components() {
            ancestor.push(part);
            if std::fs::symlink_metadata(&ancestor).is_ok_and(|m| m.file_type().is_symlink()) {
                return Err("UNSAFE_INSTALL_TARGET".into());
            }
        }
        if record.link_type == "copy" {
            if std::fs::symlink_metadata(&path).is_ok_and(|m| m.file_type().is_symlink()) {
                return Err("UNSAFE_INSTALL_TARGET".into());
            }
            if path.exists() && super::remote_sources::local_skill_files(&path)? == expected {
                continue;
            }
            let root = db::get_skill_resource_library_dir(pool).await?;
            let mut transaction = super::github_import::ImportDirectoryTransaction::new(&root)?;
            let staged = transaction.root.join("installation");
            copy_dir_all(&source, &staged)?;
            transaction.install(&staged, &path, true)?;
            if super::remote_sources::local_skill_files(&path)? != expected {
                return Err("WRITE_FAILED: installation verification failed".into());
            }
            transaction.commit()?;
        } else if !crate::path_utils::paths_resolve_to_same_entry(&path, &source) {
            // Existing linker validates and repairs recorded links, including central.
            if record.agent_id == "central" {
                add_resource_skill_to_central_impl(pool, skill_id).await?;
            } else {
                install_resource_skill_to_agent_impl(pool, skill_id, &record.agent_id, "symlink")
                    .await?;
            }
            if !crate::path_utils::paths_resolve_to_same_entry(&path, &source) {
                return Err("WRITE_FAILED: installation link verification failed".into());
            }
        }
    }
    Ok(())
}

/// The central scanner may make a resource-backed link the canonical DB row.
/// Before mirroring, restore the primary resource identity and record the hosted link.
/// Standalone central directories and links outside the resource library are untouched.
pub(crate) async fn normalize_resource_backed_central_records_impl(
    pool: &AppStore,
) -> Result<(), String> {
    let root = resolved_path(&db::get_skill_resource_library_dir(pool).await?);
    let Some(central) = db::get_agent_by_id(pool, "central").await? else {
        return Ok(());
    };
    let central_root = resolved_path(Path::new(&central.global_skills_dir));
    for mut skill in db::get_central_skills(pool).await? {
        let Some(path) = skill.canonical_path.as_deref().map(PathBuf::from) else {
            continue;
        };
        if !resolved_symlink_entry(&path).starts_with(&central_root)
            || !std::fs::symlink_metadata(&path).is_ok_and(|m| m.file_type().is_symlink())
        {
            continue;
        }
        let resource = resolved_path(&path);
        if resource == root || !resource.starts_with(&root) {
            continue;
        }
        db::upsert_skill_installation(
            pool,
            &SkillInstallation {
                skill_id: skill.id.clone(),
                agent_id: "central".into(),
                installed_path: path_to_string(&path),
                link_type: "symlink".into(),
                symlink_target: Some(path_to_string(&resource)),
                created_at: chrono::Utc::now().to_rfc3339(),
            },
        )
        .await?;
        skill.canonical_path = Some(path_to_string(&resource));
        skill.file_path = path_to_string(&resource.join("SKILL.md"));
        skill.is_central = false;
        // Generic upsert deliberately preserves central records; this verified
        // normalization is the explicit transition back to the primary library.
        pool.write(|d| {
            if let Some(row) = d.table_mut("skills").get_mut(&skill.id) {
                if row["is_central"] == true {
                    row["canonical_path"] = serde_json::json!(skill.canonical_path);
                    row["file_path"] = serde_json::json!(skill.file_path);
                    row["is_central"] = false.into();
                }
            }
            Ok(())
        })
        .await?;
    }
    Ok(())
}
