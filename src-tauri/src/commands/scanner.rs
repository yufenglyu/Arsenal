use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};

use chrono::Utc;
use serde::{Deserialize, Serialize};
use tauri::State;

use crate::commands::linker;
use crate::db::{self, AgentSkillObservation, DbPool, Skill, SkillInstallation};
use crate::path_utils;
use crate::AppState;

// ─── Types ────────────────────────────────────────────────────────────────────

/// Metadata extracted from a SKILL.md frontmatter.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillInfo {
    pub name: String,
    pub description: Option<String>,
}

/// A single skill discovered during a directory scan.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScannedSkill {
    /// Derived from directory name (lowercase, spaces→hyphens).
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    /// Absolute path to the SKILL.md file.
    pub file_path: String,
    /// Absolute path to the skill directory.
    pub dir_path: String,
    /// "symlink", "copy", or "native".
    pub link_type: String,
    /// Symlink target path, if link_type is "symlink".
    pub symlink_target: Option<String>,
    pub is_central: bool,
}

/// Summary returned by `scan_all_skills`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanResult {
    pub total_skills: usize,
    pub agents_scanned: usize,
    pub skills_by_agent: HashMap<String, usize>,
}

#[derive(Debug, Clone, Copy)]
pub struct ScanDirectoryOptions {
    pub nested: bool,
    pub max_depth: usize,
    pub follow_symlinks: bool,
    pub preserve_duplicate_ids: bool,
}

impl ScanDirectoryOptions {
    pub fn nested() -> Self {
        Self {
            nested: true,
            max_depth: 4,
            follow_symlinks: true,
            preserve_duplicate_ids: false,
        }
    }

    pub fn preserving_duplicate_ids(mut self) -> Self {
        self.preserve_duplicate_ids = true;
        self
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AgentSkillSourceKind {
    Compatibility,
}

impl AgentSkillSourceKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::Compatibility => "compatibility",
        }
    }

    fn is_read_only(self) -> bool {
        true
    }
}

#[derive(Debug, Clone)]
struct AgentScanRoot {
    path: PathBuf,
    source_root: Option<PathBuf>,
    source_kind: Option<AgentSkillSourceKind>,
}

// ─── Core Functions ───────────────────────────────────────────────────────────

/// Read a SKILL.md file and extract the YAML frontmatter fields `name` and
/// `description`. Returns `None` if the file is missing, cannot be read, lacks
/// a frontmatter block, or is missing the required `name` field.
pub fn parse_skill_md(path: &Path) -> Option<SkillInfo> {
    let content = std::fs::read_to_string(path).ok()?;
    parse_skill_md_content(&content)
}

pub(super) fn parse_skill_md_content(content: &str) -> Option<SkillInfo> {
    // Frontmatter must begin on the very first line with "---"
    let after_open = content
        .strip_prefix("---\n")
        .or_else(|| content.strip_prefix("---\r\n"))?;

    // Locate the closing "---" delimiter
    let close_pos = after_open.find("\n---")?;
    let frontmatter_str = &after_open[..close_pos];

    // Prefer full YAML parsing, but tolerate third-party SKILL.md files with
    // valid required scalar fields plus non-YAML extension metadata.
    serde_yaml::from_str::<serde_yaml::Value>(frontmatter_str)
        .ok()
        .and_then(|yaml| {
            let name = yaml.get("name")?.as_str()?.to_string();
            let description = yaml
                .get("description")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            Some(SkillInfo { name, description })
        })
        .or_else(|| parse_skill_md_required_fields(frontmatter_str))
}

fn parse_skill_md_required_fields(frontmatter: &str) -> Option<SkillInfo> {
    let name = parse_frontmatter_scalar(frontmatter, "name")?;
    let description = parse_frontmatter_scalar(frontmatter, "description");
    Some(SkillInfo { name, description })
}

fn parse_frontmatter_scalar(frontmatter: &str, key: &str) -> Option<String> {
    let prefix = format!("{key}:");
    for line in frontmatter.lines() {
        let trimmed = line.trim_start();
        if !trimmed.starts_with(&prefix) {
            continue;
        }
        let value = trimmed[prefix.len()..].trim();
        if value.is_empty() || value.starts_with(['|', '>']) {
            return None;
        }
        return Some(value.trim_matches(['"', '\'']).to_string());
    }
    None
}

/// Determine how a skill directory entry was installed at the given path.
///
/// Uses `symlink_metadata` (lstat) so the check is performed on the entry
/// itself rather than its target:
///
/// * `"symlink"` — the entry is a symbolic link.
/// * `"copy"`    — the entry is a regular directory in a platform skills dir.
/// * `"native"`  — the entry is a regular directory in the central skills dir.
///
/// Also returns the symlink target path when the entry is a symlink.
pub fn detect_link_type(path: &Path, is_central_dir: bool) -> (String, Option<String>) {
    match std::fs::symlink_metadata(path) {
        Ok(meta) if meta.file_type().is_symlink() => {
            let target = std::fs::read_link(path)
                .ok()
                .and_then(|p| p.to_str().map(|s| s.to_string()));
            ("symlink".to_string(), target)
        }
        _ => {
            let kind = if is_central_dir { "native" } else { "copy" };
            (kind.to_string(), None)
        }
    }
}

/// Walk `dir` one level deep, looking for immediate subdirectories that contain
/// a `SKILL.md` file. For each such subdirectory, `parse_skill_md` and
/// `detect_link_type` are called to build a `ScannedSkill`.
///
/// Entries that cannot be read or lack valid frontmatter are silently skipped.
pub fn scan_directory(dir: &Path, is_central: bool) -> Vec<ScannedSkill> {
    let mut skills = Vec::new();

    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return skills,
    };

    for entry in entries.flatten() {
        let entry_path = entry.path();

        // Use regular metadata (follows symlinks) to check if this is a dir.
        let meta = match std::fs::metadata(&entry_path) {
            Ok(m) => m,
            Err(_) => continue,
        };
        if !meta.is_dir() {
            continue;
        }

        // Only include entries that contain a SKILL.md file.
        let skill_md_path = entry_path.join("SKILL.md");
        if !skill_md_path.exists() {
            continue;
        }

        // Parse frontmatter; skip entries with invalid/missing frontmatter.
        let info = match parse_skill_md(&skill_md_path) {
            Some(i) => i,
            None => continue,
        };

        // Detect link type using lstat on the skill directory itself.
        let (link_type, symlink_target) = detect_link_type(&entry_path, is_central);

        // Derive a stable ID from the directory name.
        let id = entry_path
            .file_name()
            .and_then(|n| n.to_str())
            .map(|s| s.to_lowercase().replace(' ', "-"))
            .unwrap_or_else(|| "unknown".to_string());

        skills.push(ScannedSkill {
            id,
            name: info.name,
            description: info.description,
            file_path: skill_md_path.to_string_lossy().into_owned(),
            dir_path: entry_path.to_string_lossy().into_owned(),
            link_type,
            symlink_target,
            is_central,
        });
    }

    skills
}

const NESTED_SCAN_SKIP_DIRS: &[&str] = &[
    "node_modules",
    "target",
    ".git",
    "build",
    "dist",
    ".cache",
    "__pycache__",
    ".next",
    ".nuxt",
    ".venv",
    "venv",
    ".tox",
    ".mypy_cache",
    ".pytest_cache",
];

fn should_skip_nested_scan_dir(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| NESTED_SCAN_SKIP_DIRS.contains(&name))
}

fn scanned_skill_from_dir(entry_path: &Path, is_central: bool) -> Option<ScannedSkill> {
    let skill_md_path = entry_path.join("SKILL.md");
    if !skill_md_path.exists() {
        return None;
    }

    let info = parse_skill_md(&skill_md_path)?;
    let (link_type, symlink_target) = detect_link_type(entry_path, is_central);
    let id = entry_path
        .file_name()
        .and_then(|n| n.to_str())
        .map(|s| s.to_lowercase().replace(' ', "-"))
        .unwrap_or_else(|| "unknown".to_string());

    Some(ScannedSkill {
        id,
        name: info.name,
        description: info.description,
        file_path: skill_md_path.to_string_lossy().into_owned(),
        dir_path: entry_path.to_string_lossy().into_owned(),
        link_type,
        symlink_target,
        is_central,
    })
}

fn scan_skill_root_recursive(
    current_dir: &Path,
    is_central: bool,
    options: ScanDirectoryOptions,
    depth: usize,
    visited_dirs: &mut HashSet<PathBuf>,
    out: &mut Vec<(usize, ScannedSkill)>,
) {
    if depth > options.max_depth {
        return;
    }

    let metadata = if options.follow_symlinks {
        std::fs::metadata(current_dir)
    } else {
        std::fs::symlink_metadata(current_dir)
    };
    let Ok(metadata) = metadata else {
        return;
    };
    if !metadata.is_dir() {
        return;
    }

    if let Some(skill) = scanned_skill_from_dir(current_dir, is_central) {
        out.push((depth, skill));
        return;
    }

    if !options.nested || depth >= options.max_depth || should_skip_nested_scan_dir(current_dir) {
        return;
    }

    if options.follow_symlinks {
        if let Ok(canonical) = current_dir.canonicalize() {
            if !visited_dirs.insert(canonical) {
                return;
            }
        }
    }

    let entries = match std::fs::read_dir(current_dir) {
        Ok(entries) => entries,
        Err(_) => return,
    };

    let mut child_paths: Vec<PathBuf> = entries.flatten().map(|entry| entry.path()).collect();
    child_paths.sort();

    for child_path in child_paths {
        scan_skill_root_recursive(
            &child_path,
            is_central,
            options,
            depth + 1,
            visited_dirs,
            out,
        );
    }
}

pub fn scan_skill_root(
    dir: &Path,
    is_central: bool,
    options: ScanDirectoryOptions,
) -> Vec<ScannedSkill> {
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(_) => return Vec::new(),
    };

    let mut visited_dirs = HashSet::new();
    if let Ok(canonical) = dir.canonicalize() {
        visited_dirs.insert(canonical);
    }

    let mut child_paths: Vec<PathBuf> = entries.flatten().map(|entry| entry.path()).collect();
    child_paths.sort();

    let mut candidates = Vec::new();
    for child_path in child_paths {
        scan_skill_root_recursive(
            &child_path,
            is_central,
            options,
            1,
            &mut visited_dirs,
            &mut candidates,
        );
    }

    if options.preserve_duplicate_ids {
        return candidates
            .into_iter()
            .map(|(_, skill)| skill)
            .collect::<Vec<_>>();
    }

    let mut by_id: BTreeMap<String, (usize, String, ScannedSkill)> = BTreeMap::new();
    for (depth, skill) in candidates {
        let sort_path = skill.dir_path.clone();
        match by_id.get(&skill.id) {
            Some((existing_depth, existing_path, _))
                if (*existing_depth, existing_path.as_str()) <= (depth, sort_path.as_str()) => {}
            _ => {
                by_id.insert(skill.id.clone(), (depth, sort_path, skill));
            }
        }
    }

    by_id.into_values().map(|(_, _, skill)| skill).collect()
}

fn agents_skills_compatibility_root(primary_root: &Path) -> Option<PathBuf> {
    primary_root
        .parent()
        .and_then(Path::parent)
        .map(|home_root| home_root.join(".agents/skills"))
}

fn compatibility_scan_root(path: PathBuf) -> AgentScanRoot {
    AgentScanRoot {
        path: path.clone(),
        source_root: Some(path),
        source_kind: Some(AgentSkillSourceKind::Compatibility),
    }
}

fn push_unique_scan_root(roots: &mut Vec<AgentScanRoot>, root: AgentScanRoot) {
    if roots.iter().any(|existing| existing.path == root.path) {
        return;
    }
    roots.push(root);
}

fn scan_roots_for_agent(agent: &crate::db::Agent) -> Vec<AgentScanRoot> {
    let primary_root = PathBuf::from(&agent.global_skills_dir);

    let compatibility_root = agents_skills_compatibility_root(&primary_root);
    if db::agent_supports_universal_agents_skills(&agent.id)
        && compatibility_root
            .as_ref()
            .is_some_and(|root| root == &primary_root)
    {
        return compatibility_root
            .map(compatibility_scan_root)
            .into_iter()
            .collect();
    }

    let mut roots = vec![AgentScanRoot {
        path: primary_root.clone(),
        source_root: None,
        source_kind: None,
    }];

    if agent.id == "factory-droid" || db::agent_supports_universal_agents_skills(&agent.id) {
        if let Some(compatibility_root) = compatibility_root {
            if compatibility_root != primary_root {
                push_unique_scan_root(&mut roots, compatibility_scan_root(compatibility_root));
            }
        }
    }

    roots
}

fn agent_observation_row_id(agent_id: &str, dir_path: &str) -> String {
    format!("{agent_id}::{dir_path}")
}

fn paths_resolve_to_same_entry(left: &Path, right: &Path) -> bool {
    matches!(
        (std::fs::canonicalize(left), std::fs::canonicalize(right)),
        (Ok(left), Ok(right)) if left == right
    )
}

// ─── Tauri Command ────────────────────────────────────────────────────────────

/// Core scanning logic, separated from the Tauri command layer so it can be
/// unit-tested without a running Tauri runtime.
pub async fn scan_all_skills_impl(pool: &DbPool) -> Result<ScanResult, String> {
    let agents = db::get_all_agents(pool).await?;
    let custom_dirs = db::get_scan_directories(pool).await?;

    // Heal Central Skills whose parent folder was deleted without updating DB rows,
    // then re-sync whatever is still actually present under ~/.agents/skills.
    let _ = linker::reconcile_orphaned_central_skills(pool).await;
    let _ = linker::sync_all_central_skills_to_detected_platforms(pool).await;

    let mut total_skills: usize = 0;
    let mut skills_by_agent: HashMap<String, usize> = HashMap::new();

    // Accumulate every skill ID discovered in this scan so we can purge stale
    // rows from the database once all directories have been walked.
    let mut all_found_skill_ids: HashSet<String> = HashSet::new();

    let resource_root = db::get_skill_resource_library_dir(pool).await?;
    for skill in scan_skill_root(
        &resource_root,
        false,
        ScanDirectoryOptions::nested().preserving_duplicate_ids(),
    ) {
        all_found_skill_ids.insert(skill.id);
    }

    let central_root = agents
        .iter()
        .find(|agent| agent.id == "central")
        .map(|agent| PathBuf::from(&agent.global_skills_dir));

    // ── Per-agent scans ───────────────────────────────────────────────────────
    for agent in &agents {
        let is_central = agent.id == "central";
        let scan_roots = scan_roots_for_agent(agent);
        let tracks_observations = scan_roots.iter().any(|root| root.source_kind.is_some());
        let existing_roots: Vec<AgentScanRoot> = scan_roots
            .into_iter()
            .filter(|root| root.path.exists())
            .collect();

        if existing_roots.is_empty() {
            // Mark agent as not detected and record zero count.
            let _ = db::update_agent_detected(pool, &agent.id, false).await;
            skills_by_agent.insert(agent.id.clone(), 0);
            // Reconcile unmanaged rows and managed paths even when no scan root remains.
            let _ = db::delete_stale_skill_installations(pool, &agent.id, &[]).await;
            if tracks_observations {
                let _ = db::delete_stale_agent_skill_observations(pool, &agent.id, &[]).await;
            }
            continue;
        }

        let _ = db::update_agent_detected(pool, &agent.id, true).await;
        let mut scanned = Vec::new();
        let mut found_install_ids = Vec::new();
        let mut found_observation_row_ids = Vec::new();

        for root in &existing_roots {
            let root_path = root
                .source_root
                .as_ref()
                .unwrap_or(&root.path)
                .to_string_lossy()
                .into_owned();
            let root_scanned =
                scan_skill_root(&root.path, is_central, ScanDirectoryOptions::nested());

            for skill in &root_scanned {
                let now = Utc::now().to_rfc3339();

                if let Some(source_kind) = root.source_kind {
                    let observation = AgentSkillObservation {
                        row_id: agent_observation_row_id(&agent.id, &skill.dir_path),
                        agent_id: agent.id.clone(),
                        skill_id: skill.id.clone(),
                        name: skill.name.clone(),
                        description: skill.description.clone(),
                        file_path: skill.file_path.clone(),
                        dir_path: skill.dir_path.clone(),
                        source_kind: source_kind.as_str().to_string(),
                        source_root: root_path.clone(),
                        link_type: skill.link_type.clone(),
                        symlink_target: skill.symlink_target.clone(),
                        is_read_only: source_kind.is_read_only(),
                        scanned_at: now.clone(),
                    };
                    db::upsert_agent_skill_observation(pool, &observation).await?;
                    found_observation_row_ids.push(observation.row_id);
                }

                let should_persist_manageable_state = root
                    .source_kind
                    .is_none_or(|source_kind| !source_kind.is_read_only());
                if should_persist_manageable_state {
                    all_found_skill_ids.insert(skill.id.clone());
                    found_install_ids.push(skill.id.clone());

                    let db_skill = Skill {
                        id: skill.id.clone(),
                        name: skill.name.clone(),
                        description: skill.description.clone(),
                        file_path: skill.file_path.clone(),
                        canonical_path: if is_central {
                            Some(skill.dir_path.clone())
                        } else {
                            None
                        },
                        is_central,
                        source: Some(skill.link_type.clone()),
                        content: None,
                        scanned_at: now.clone(),
                    };
                    db::upsert_skill(pool, &db_skill).await?;

                    // Bug fix: store the skill *directory* path, not the SKILL.md file path.
                    let installation = SkillInstallation {
                        skill_id: skill.id.clone(),
                        agent_id: agent.id.clone(),
                        installed_path: skill.dir_path.clone(),
                        link_type: skill.link_type.clone(),
                        symlink_target: skill.symlink_target.clone(),
                        created_at: now.clone(),
                    };
                    db::upsert_scanned_skill_installation(pool, &installation).await?;
                }
            }

            scanned.extend(root_scanned);
        }

        // Reconcile unmanaged rows against the scan and managed rows against
        // their recorded installation paths.
        db::delete_stale_skill_installations(pool, &agent.id, &found_install_ids).await?;
        if tracks_observations {
            db::delete_stale_agent_skill_observations(pool, &agent.id, &found_observation_row_ids)
                .await?;
        }

        total_skills += scanned.len();
        let managed_count = db::count_managed_skill_installations(pool, &agent.id).await?;
        skills_by_agent.insert(agent.id.clone(), managed_count as usize);

        if !is_central {
            let agent_root = Path::new(&agent.global_skills_dir);
            let shares_central = central_root
                .as_ref()
                .is_some_and(|central| paths_resolve_to_same_entry(agent_root, central));
            if agent_root.exists() && !shares_central {
                path_utils::prune_empty_directories_under(agent_root);
            }
        }
    }

    // ── Project directory targets ─────────────────────────────────────────────
    // User-added directories behave like install targets. A project directory
    // manages skills under `<project>/.agents/skills`, which lets Central Skills
    // synchronize into projects without scanning unrelated project files.
    let mut project_targets_scanned = 0usize;
    for scan_dir in custom_dirs.iter().filter(|d| !d.is_builtin) {
        let target_id = linker::project_agent_id(scan_dir.id);
        if !scan_dir.is_active {
            db::delete_skill_installations_by_agent(pool, &target_id).await?;
            skills_by_agent.insert(target_id, 0);
            continue;
        }

        let Some(target) = linker::project_target_for_scan_directory(scan_dir).await? else {
            db::delete_skill_installations_by_agent(pool, &target_id).await?;
            skills_by_agent.insert(target_id, 0);
            continue;
        };
        project_targets_scanned += 1;

        let mut found_install_ids = Vec::new();
        let scanned = scan_skill_root(&target.skills_dir, false, ScanDirectoryOptions::nested());
        for skill in &scanned {
            all_found_skill_ids.insert(skill.id.clone());
            found_install_ids.push(skill.id.clone());
            let now = Utc::now().to_rfc3339();
            let db_skill = Skill {
                id: skill.id.clone(),
                name: skill.name.clone(),
                description: skill.description.clone(),
                file_path: skill.file_path.clone(),
                canonical_path: None,
                is_central: false,
                source: Some(skill.link_type.clone()),
                content: None,
                scanned_at: now.clone(),
            };
            db::upsert_skill(pool, &db_skill).await?;

            let installation = SkillInstallation {
                skill_id: skill.id.clone(),
                agent_id: target.id.clone(),
                installed_path: skill.dir_path.clone(),
                link_type: skill.link_type.clone(),
                symlink_target: skill.symlink_target.clone(),
                created_at: now,
            };
            db::upsert_scanned_skill_installation(pool, &installation).await?;
        }

        db::delete_stale_skill_installations(pool, &target.id, &found_install_ids).await?;
        let scanned_count = scanned.len();
        total_skills += scanned_count;
        skills_by_agent.insert(target.id, scanned_count);

        let shares_central = central_root
            .as_ref()
            .is_some_and(|central| paths_resolve_to_same_entry(&target.skills_dir, central));
        if target.skills_dir.exists() && !shares_central {
            path_utils::prune_empty_directories_under(&target.skills_dir);
        }
    }

    // ── Global reconciliation ─────────────────────────────────────────────────
    // Remove skills (and their installation records) that were not found in
    // any scanned scope during this run. Managed installations that survived
    // path-based reconciliation remain authoritative even when their scan root
    // is read-only.
    all_found_skill_ids.extend(db::get_managed_skill_ids(pool).await?);
    let found_ids_vec: Vec<String> = all_found_skill_ids.into_iter().collect();
    db::delete_skills_not_in_scope(pool, &found_ids_vec).await?;

    if let Some(central_root) = central_root.as_ref() {
        let central_count = db::get_central_skills(pool).await?.len();
        if central_count > 0 {
            for agent in &agents {
                if agent.id == "central" || !agent.is_enabled {
                    continue;
                }

                let agent_root = Path::new(&agent.global_skills_dir);
                if agent_root.exists() && paths_resolve_to_same_entry(agent_root, central_root) {
                    let count = skills_by_agent.get(&agent.id).copied().unwrap_or_default();
                    skills_by_agent.insert(agent.id.clone(), count.max(central_count));
                }
            }
        }
    }

    Ok(ScanResult {
        total_skills,
        agents_scanned: agents.len() + project_targets_scanned,
        skills_by_agent,
    })
}

/// Tauri command: scan all agent skill directories and persist the results to
/// SQLite. Returns a `ScanResult` with per-agent skill counts.
#[tauri::command]
pub async fn scan_all_skills(state: State<'_, AppState>) -> Result<ScanResult, String> {
    let _guard = super::repository_mirror::MIRROR_LOCK.lock().await;
    super::repository_address::recover(&state.db).await?;
    scan_all_skills_impl(&state.db).await
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[cfg(unix)]
    fn symlink(src: impl AsRef<Path>, dst: impl AsRef<Path>) -> std::io::Result<()> {
        std::os::unix::fs::symlink(src, dst)
    }

    #[cfg(windows)]
    fn symlink(src: impl AsRef<Path>, dst: impl AsRef<Path>) -> std::io::Result<()> {
        std::os::windows::fs::symlink_dir(src, dst)
    }

    // ── Helpers ───────────────────────────────────────────────────────────────

    fn normalize_test_path(path: impl AsRef<str>) -> String {
        path.as_ref().replace('\\', "/")
    }

    /// Write a SKILL.md with the given content in `dir/<skill_name>/SKILL.md`.
    fn create_skill_dir(parent: &Path, dir_name: &str, content: &str) -> std::path::PathBuf {
        let skill_dir = parent.join(dir_name);
        fs::create_dir_all(&skill_dir).unwrap();
        fs::write(skill_dir.join("SKILL.md"), content).unwrap();
        skill_dir
    }

    fn valid_skill_md(name: &str, description: &str) -> String {
        format!(
            "---\nname: {}\ndescription: {}\n---\n\n# {}\n\nContent.\n",
            name, description, name
        )
    }

    fn skill_md_no_description(name: &str) -> String {
        format!("---\nname: {}\n---\n\n# {}\n", name, name)
    }

    // ── parse_skill_md ────────────────────────────────────────────────────────

    #[test]
    fn test_parse_skill_md_valid() {
        let tmp = TempDir::new().unwrap();
        let md_path = tmp.path().join("SKILL.md");
        fs::write(&md_path, valid_skill_md("My Skill", "A great skill")).unwrap();

        let info = parse_skill_md(&md_path).expect("should parse valid SKILL.md");
        assert_eq!(info.name, "My Skill");
        assert_eq!(info.description.as_deref(), Some("A great skill"));
    }

    #[test]
    fn test_parse_skill_md_no_description() {
        let tmp = TempDir::new().unwrap();
        let md_path = tmp.path().join("SKILL.md");
        fs::write(&md_path, skill_md_no_description("Minimal Skill")).unwrap();

        let info = parse_skill_md(&md_path).expect("should parse frontmatter without description");
        assert_eq!(info.name, "Minimal Skill");
        assert!(info.description.is_none());
    }

    #[test]
    fn test_parse_skill_md_tolerates_invalid_extension_metadata() {
        let tmp = TempDir::new().unwrap();
        let md_path = tmp.path().join("SKILL.md");
        fs::write(
            &md_path,
            "---\nname: scholar-deep-research\ndescription: Academic research workflow\ncompatibility: Optional: install extra packages\n---\n\n# Scholar\n",
        )
        .unwrap();

        let info = parse_skill_md(&md_path).expect("should parse required scalar fields");
        assert_eq!(info.name, "scholar-deep-research");
        assert_eq!(
            info.description.as_deref(),
            Some("Academic research workflow")
        );
    }

    #[test]
    fn test_parse_skill_md_missing_name() {
        let tmp = TempDir::new().unwrap();
        let md_path = tmp.path().join("SKILL.md");
        fs::write(
            &md_path,
            "---\ndescription: Has description but no name\n---\n\nContent.",
        )
        .unwrap();

        let result = parse_skill_md(&md_path);
        assert!(result.is_none(), "should return None when name is missing");
    }

    #[test]
    fn test_parse_skill_md_no_frontmatter() {
        let tmp = TempDir::new().unwrap();
        let md_path = tmp.path().join("SKILL.md");
        fs::write(&md_path, "# Just a Markdown file\n\nNo frontmatter here.").unwrap();

        let result = parse_skill_md(&md_path);
        assert!(
            result.is_none(),
            "should return None when frontmatter is absent"
        );
    }

    #[test]
    fn test_parse_skill_md_empty_file() {
        let tmp = TempDir::new().unwrap();
        let md_path = tmp.path().join("SKILL.md");
        fs::write(&md_path, "").unwrap();

        let result = parse_skill_md(&md_path);
        assert!(result.is_none(), "should return None for an empty file");
    }

    #[test]
    fn test_parse_skill_md_file_not_found() {
        let result = parse_skill_md(Path::new("/nonexistent/path/SKILL.md"));
        assert!(result.is_none(), "should return None for a missing file");
    }

    #[test]
    fn test_parse_skill_md_multiline_description() {
        let tmp = TempDir::new().unwrap();
        let md_path = tmp.path().join("SKILL.md");
        // YAML block scalar for multiline strings
        let content =
            "---\nname: Block Skill\ndescription: \"Line one. Line two.\"\n---\n\nBody.\n";
        fs::write(&md_path, content).unwrap();

        let info = parse_skill_md(&md_path).expect("should parse multiline description");
        assert_eq!(info.name, "Block Skill");
        assert!(info.description.is_some());
    }

    // ── detect_link_type ──────────────────────────────────────────────────────

    #[test]
    fn test_detect_link_type_real_dir_platform() {
        let tmp = TempDir::new().unwrap();
        let dir = tmp.path().join("real-skill");
        fs::create_dir_all(&dir).unwrap();

        let (kind, target) = detect_link_type(&dir, false);
        assert_eq!(
            kind, "copy",
            "real dir in platform context should be 'copy'"
        );
        assert!(target.is_none());
    }

    #[test]
    fn test_detect_link_type_real_dir_central() {
        let tmp = TempDir::new().unwrap();
        let dir = tmp.path().join("central-skill");
        fs::create_dir_all(&dir).unwrap();

        let (kind, target) = detect_link_type(&dir, true);
        assert_eq!(
            kind, "native",
            "real dir in central context should be 'native'"
        );
        assert!(target.is_none());
    }

    #[test]
    fn test_detect_link_type_symlink() {
        let tmp = TempDir::new().unwrap();

        // Create a real target directory
        let target_dir = tmp.path().join("target-skill");
        fs::create_dir_all(&target_dir).unwrap();

        // Create a symlink pointing to it
        let link_path = tmp.path().join("linked-skill");
        symlink(&target_dir, &link_path).expect("failed to create symlink");

        let (kind, sym_target) = detect_link_type(&link_path, false);
        assert_eq!(kind, "symlink");
        assert!(
            sym_target.is_some(),
            "symlink target path should be returned"
        );
    }

    #[test]
    fn test_detect_link_type_symlink_is_symlink_regardless_of_is_central() {
        let tmp = TempDir::new().unwrap();
        let target_dir = tmp.path().join("target");
        fs::create_dir_all(&target_dir).unwrap();
        let link_path = tmp.path().join("link");
        symlink(&target_dir, &link_path).unwrap();

        // Even in central context, a symlink is a symlink
        let (kind, _) = detect_link_type(&link_path, true);
        assert_eq!(kind, "symlink");
    }

    // ── scan_directory ────────────────────────────────────────────────────────

    #[test]
    fn test_scan_directory_empty() {
        let tmp = TempDir::new().unwrap();
        let result = scan_directory(tmp.path(), false);
        assert!(result.is_empty(), "empty directory should yield no skills");
    }

    #[test]
    fn test_scan_directory_finds_single_skill() {
        let tmp = TempDir::new().unwrap();
        create_skill_dir(
            tmp.path(),
            "cool-skill",
            &valid_skill_md("Cool Skill", "Does cool things"),
        );

        let skills = scan_directory(tmp.path(), false);
        assert_eq!(skills.len(), 1);
        assert_eq!(skills[0].id, "cool-skill");
        assert_eq!(skills[0].name, "Cool Skill");
        assert_eq!(skills[0].description.as_deref(), Some("Does cool things"));
    }

    #[test]
    fn test_scan_directory_finds_multiple_skills() {
        let tmp = TempDir::new().unwrap();
        create_skill_dir(tmp.path(), "skill-a", &valid_skill_md("Skill A", "Alpha"));
        create_skill_dir(tmp.path(), "skill-b", &valid_skill_md("Skill B", "Beta"));
        create_skill_dir(tmp.path(), "skill-c", &valid_skill_md("Skill C", "Gamma"));

        let mut skills = scan_directory(tmp.path(), false);
        skills.sort_by(|a, b| a.id.cmp(&b.id));
        assert_eq!(skills.len(), 3);
        assert_eq!(skills[0].id, "skill-a");
        assert_eq!(skills[1].id, "skill-b");
        assert_eq!(skills[2].id, "skill-c");
    }

    #[test]
    fn test_scan_directory_skips_dirs_without_skill_md() {
        let tmp = TempDir::new().unwrap();
        create_skill_dir(tmp.path(), "valid-skill", &valid_skill_md("Valid", "OK"));

        // A directory without SKILL.md should be ignored
        fs::create_dir_all(tmp.path().join("no-skill-md")).unwrap();

        let skills = scan_directory(tmp.path(), false);
        assert_eq!(skills.len(), 1);
        assert_eq!(skills[0].id, "valid-skill");
    }

    #[test]
    fn test_scan_directory_skips_invalid_frontmatter() {
        let tmp = TempDir::new().unwrap();
        create_skill_dir(tmp.path(), "valid-skill", &valid_skill_md("Valid", "OK"));
        create_skill_dir(
            tmp.path(),
            "invalid-skill",
            "# No frontmatter here\n\nJust content.",
        );

        let skills = scan_directory(tmp.path(), false);
        assert_eq!(
            skills.len(),
            1,
            "skill with invalid frontmatter should be skipped"
        );
        assert_eq!(skills[0].id, "valid-skill");
    }

    #[test]
    fn test_scan_directory_skips_regular_files() {
        let tmp = TempDir::new().unwrap();
        // A plain file at the top level should be ignored
        fs::write(tmp.path().join("README.md"), "# readme").unwrap();
        create_skill_dir(tmp.path(), "real-skill", &valid_skill_md("Real", "desc"));

        let skills = scan_directory(tmp.path(), false);
        assert_eq!(skills.len(), 1);
    }

    #[test]
    fn test_scan_directory_is_not_recursive() {
        let tmp = TempDir::new().unwrap();
        // Create a nested structure (depth 2); only top-level subdirs should be found
        let deep_dir = tmp.path().join("outer").join("inner");
        fs::create_dir_all(&deep_dir).unwrap();
        fs::write(
            deep_dir.join("SKILL.md"),
            &valid_skill_md("Deep Skill", "too deep"),
        )
        .unwrap();

        let skills = scan_directory(tmp.path(), false);
        assert!(
            skills.is_empty(),
            "scan_directory should not descend more than one level"
        );
    }

    #[test]
    fn test_scan_directory_central_dir_marks_native() {
        let tmp = TempDir::new().unwrap();
        create_skill_dir(
            tmp.path(),
            "central-skill",
            &valid_skill_md("Central", "desc"),
        );

        let skills = scan_directory(tmp.path(), true /* is_central */);
        assert_eq!(skills.len(), 1);
        assert_eq!(skills[0].link_type, "native");
        assert!(skills[0].is_central);
    }

    #[test]
    fn test_scan_directory_detects_symlinked_skill() {
        let tmp = TempDir::new().unwrap();
        let skills_dir = tmp.path().join("agent-skills");
        fs::create_dir_all(&skills_dir).unwrap();

        // Create a real skill in another location (central-like)
        let central_dir = tmp.path().join("central");
        create_skill_dir(
            &central_dir,
            "my-skill",
            &valid_skill_md("My Skill", "desc"),
        );

        // Symlink it into the agent skills dir
        let link = skills_dir.join("my-skill");
        symlink(central_dir.join("my-skill"), &link).unwrap();

        let skills = scan_directory(&skills_dir, false);
        assert_eq!(skills.len(), 1);
        assert_eq!(skills[0].link_type, "symlink");
        assert!(skills[0].symlink_target.is_some());
    }

    #[test]
    fn test_scan_directory_nonexistent_dir_returns_empty() {
        let result = scan_directory(Path::new("/nonexistent/path/skills"), false);
        assert!(result.is_empty());
    }

    // ── scan_skill_root ───────────────────────────────────────────────────────

    #[test]
    fn test_scan_skill_root_finds_nested_category_skills() {
        let tmp = TempDir::new().unwrap();
        create_skill_dir(
            &tmp.path().join("apple"),
            "apple-reminders",
            &valid_skill_md("Apple Reminders", "Nested category skill"),
        );
        create_skill_dir(
            &tmp.path().join("mlops/evaluation"),
            "weights-and-biases",
            &valid_skill_md("Weights and Biases", "Deep nested category skill"),
        );

        let mut skills = scan_skill_root(
            tmp.path(),
            false,
            ScanDirectoryOptions {
                nested: true,
                max_depth: 4,
                follow_symlinks: true,
                preserve_duplicate_ids: false,
            },
        );
        skills.sort_by(|a, b| a.id.cmp(&b.id));

        assert_eq!(skills.len(), 2);
        assert_eq!(skills[0].id, "apple-reminders");
        assert!(normalize_test_path(&skills[0].dir_path).contains("apple/apple-reminders"));
        assert_eq!(skills[1].id, "weights-and-biases");
        assert!(normalize_test_path(&skills[1].dir_path)
            .contains("mlops/evaluation/weights-and-biases"));
    }

    #[test]
    fn test_scan_skill_root_follows_symlinked_bundle_without_looping() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path().join("root");
        let target = tmp.path().join("target-skills");
        fs::create_dir_all(&root).unwrap();
        create_skill_dir(
            &target,
            "using-superpowers",
            &valid_skill_md("Using Superpowers", "Symlinked bundle skill"),
        );
        symlink(&target, root.join("superpowers")).unwrap();
        symlink(&root, target.join("loop-back")).unwrap();

        let skills = scan_skill_root(
            &root,
            true,
            ScanDirectoryOptions {
                nested: true,
                max_depth: 4,
                follow_symlinks: true,
                preserve_duplicate_ids: false,
            },
        );

        assert_eq!(skills.len(), 1);
        assert_eq!(skills[0].id, "using-superpowers");
        assert!(skills[0].is_central);
    }

    #[test]
    fn test_scan_skill_root_prefers_direct_duplicate_over_nested_duplicate() {
        let tmp = TempDir::new().unwrap();
        create_skill_dir(
            tmp.path(),
            "shared-skill",
            &valid_skill_md("Direct Shared", "Direct wins"),
        );
        create_skill_dir(
            &tmp.path().join("bundle"),
            "shared-skill",
            &valid_skill_md("Nested Shared", "Nested duplicate"),
        );

        let skills = scan_skill_root(
            tmp.path(),
            false,
            ScanDirectoryOptions {
                nested: true,
                max_depth: 4,
                follow_symlinks: true,
                preserve_duplicate_ids: false,
            },
        );

        assert_eq!(skills.len(), 1);
        assert_eq!(skills[0].name, "Direct Shared");
        assert!(skills[0].dir_path.ends_with("shared-skill"));
        assert!(!skills[0].dir_path.contains("bundle/shared-skill"));
    }

    #[test]
    fn test_scan_skill_root_can_preserve_duplicate_ids_from_different_paths() {
        let tmp = TempDir::new().unwrap();
        create_skill_dir(
            &tmp.path().join("first/repo"),
            "assetseeker",
            &valid_skill_md("Assetseeker", "First repo"),
        );
        create_skill_dir(
            &tmp.path().join("second/repo"),
            "assetseeker",
            &valid_skill_md("Assetseeker", "Second repo"),
        );

        let skills = scan_skill_root(
            tmp.path(),
            false,
            ScanDirectoryOptions::nested().preserving_duplicate_ids(),
        );

        assert_eq!(skills.len(), 2);
        assert!(skills
            .iter()
            .any(|skill| normalize_test_path(&skill.dir_path).contains("first/repo/assetseeker")));
        assert!(skills
            .iter()
            .any(|skill| normalize_test_path(&skill.dir_path).contains("second/repo/assetseeker")));
    }

    // ── scan_all_skills_impl ──────────────────────────────────────────────────

    async fn setup_test_db() -> DbPool {
        use crate::db;
        use crate::db::DbPool as SqlitePool;
        let pool = SqlitePool::connect(":memory:").await.expect("in-memory DB");
        db::init_database(&pool).await.expect("init");
        pool
    }

    async fn isolate_scan_test(pool: &DbPool) {
        pool.write(|d| {
            d.table_mut("agents").clear();
            Ok(())
        })
        .await
        .unwrap();
        crate::db::update_config(pool, |config| {
            config.scan_directories.clear();
            Ok(())
        })
        .expect("delete seeded scan directories");
    }

    #[tokio::test]
    async fn test_scan_all_skills_impl_empty_dirs() {
        use crate::db::DbPool as SqlitePool;

        // Build a pool with tables but no seeded agents so the test is
        // isolated from whatever the user has installed on their machine.
        let pool = SqlitePool::connect(":memory:").await.expect("in-memory DB");
        db::init_database(&pool).await.expect("init");
        // Remove all seeded agents so the test is isolated from whatever the
        // user has installed on their machine.
        pool.write(|d| {
            d.table_mut("agents").clear();
            Ok(())
        })
        .await
        .unwrap();
        // Also clear the builtin scan directories that init_database seeds,
        // so the custom-scan-dir loop has nothing to scan either.
        crate::db::update_config(&pool, |config| {
            config.scan_directories.clear();
            Ok(())
        })
        .expect("delete scan_directories");

        // Add one agent whose skills dir definitely does not exist.
        let dummy_agent = db::Agent {
            id: "empty-agent".to_string(),
            display_name: "Empty Agent".to_string(),
            category: "platform".to_string(),
            global_skills_dir: "/nonexistent/path/skills".to_string(),
            project_skills_dir: None,
            is_detected: false,
            is_builtin: false,
            is_enabled: true,
        };
        db::insert_custom_agent(&pool, &dummy_agent)
            .await
            .expect("insert dummy agent");

        let result = scan_all_skills_impl(&pool).await;
        assert!(result.is_ok());
        let r = result.unwrap();
        assert_eq!(r.total_skills, 0);
        assert_eq!(r.agents_scanned, 1);
        assert_eq!(r.skills_by_agent.get("empty-agent").copied(), Some(0));
    }

    #[tokio::test]
    async fn test_scan_all_skills_impl_prunes_empty_leftover_dirs() {
        use crate::db;

        let tmp = TempDir::new().unwrap();
        let pool = setup_test_db().await;
        isolate_scan_test(&pool).await;

        let skills_dir = tmp.path().join("skills");
        let leftover = skills_dir.join("old-author").join("skills");
        fs::create_dir_all(&leftover).unwrap();
        create_skill_dir(
            &skills_dir,
            "keep-me",
            &valid_skill_md("Keep Me", "Still installed"),
        );

        let test_agent = db::Agent {
            id: "prune-agent".to_string(),
            display_name: "Prune Agent".to_string(),
            category: "platform".to_string(),
            global_skills_dir: skills_dir.to_string_lossy().into_owned(),
            project_skills_dir: None,
            is_detected: false,
            is_builtin: false,
            is_enabled: true,
        };
        db::insert_custom_agent(&pool, &test_agent).await.unwrap();

        scan_all_skills_impl(&pool).await.unwrap();

        assert!(skills_dir.join("keep-me").join("SKILL.md").exists());
        assert!(
            !skills_dir.join("old-author").exists(),
            "scan should prune historical empty skill folders"
        );
    }

    #[tokio::test]
    async fn test_scan_all_skills_impl_preserves_resource_library_metadata() {
        let tmp = TempDir::new().unwrap();
        let pool = setup_test_db().await;
        isolate_scan_test(&pool).await;

        let resource_root = tmp.path().join("library");
        create_skill_dir(
            &resource_root.join("owner").join("repo"),
            "resource-only",
            &valid_skill_md("Resource Only", "Exists only in the resource library"),
        );
        db::set_skill_resource_library_dir(&pool, &resource_root.to_string_lossy())
            .await
            .unwrap();

        let now = Utc::now().to_rfc3339();
        db::upsert_skill(
            &pool,
            &Skill {
                id: "resource-only".to_string(),
                name: "Resource Only".to_string(),
                description: Some("Exists only in the resource library".to_string()),
                file_path: resource_root
                    .join("owner")
                    .join("repo")
                    .join("resource-only")
                    .join("SKILL.md")
                    .to_string_lossy()
                    .into_owned(),
                canonical_path: Some(
                    resource_root
                        .join("owner")
                        .join("repo")
                        .join("resource-only")
                        .to_string_lossy()
                        .into_owned(),
                ),
                is_central: false,
                source: Some("github:owner/repo".to_string()),
                content: None,
                scanned_at: now,
            },
        )
        .await
        .unwrap();
        db::upsert_skill_metadata(
            &pool,
            "resource-only",
            Some("keep this note"),
            &["important".to_string()],
        )
        .await
        .unwrap();

        scan_all_skills_impl(&pool).await.unwrap();

        let metadata = db::get_skill_metadata(&pool, "resource-only")
            .await
            .unwrap()
            .expect("resource library metadata should survive a full scan");
        assert_eq!(metadata.notes.as_deref(), Some("keep this note"));
        assert_eq!(
            db::parse_skill_metadata_tags(Some(&metadata)),
            vec!["important".to_string()]
        );
    }

    #[tokio::test]
    async fn test_scan_all_skills_impl_syncs_existing_central_skills_to_newly_detected_platforms() {
        let tmp = TempDir::new().unwrap();
        let pool = setup_test_db().await;
        isolate_scan_test(&pool).await;

        let central_dir = tmp.path().join("central");
        let platform_parent = tmp.path().join("detected-agent");
        let platform_dir = platform_parent.join("skills");
        fs::create_dir_all(&central_dir).unwrap();
        fs::create_dir_all(&platform_parent).unwrap();
        let central_skill_dir = create_skill_dir(
            &central_dir,
            "shared-skill",
            &valid_skill_md("Shared Skill", "desc"),
        );

        db::insert_custom_agent(
            &pool,
            &db::Agent {
                id: "central".to_string(),
                display_name: "Central Skills".to_string(),
                category: "central".to_string(),
                global_skills_dir: central_dir.to_string_lossy().into_owned(),
                project_skills_dir: None,
                is_detected: true,
                is_builtin: false,
                is_enabled: true,
            },
        )
        .await
        .unwrap();
        db::insert_custom_agent(
            &pool,
            &db::Agent {
                id: "detected-agent".to_string(),
                display_name: "Detected Agent".to_string(),
                category: "platform".to_string(),
                global_skills_dir: platform_dir.to_string_lossy().into_owned(),
                project_skills_dir: None,
                is_detected: false,
                is_builtin: false,
                is_enabled: true,
            },
        )
        .await
        .unwrap();
        db::upsert_skill(
            &pool,
            &Skill {
                id: "shared-skill".to_string(),
                name: "Shared Skill".to_string(),
                description: Some("desc".to_string()),
                file_path: central_skill_dir
                    .join("SKILL.md")
                    .to_string_lossy()
                    .into_owned(),
                canonical_path: Some(central_skill_dir.to_string_lossy().into_owned()),
                is_central: true,
                source: Some("native".to_string()),
                content: None,
                scanned_at: Utc::now().to_rfc3339(),
            },
        )
        .await
        .unwrap();

        let result = scan_all_skills_impl(&pool).await.unwrap();

        assert!(
            platform_dir.join("shared-skill").join("SKILL.md").exists(),
            "full scan must synchronize existing Central Skills to newly detected platforms"
        );
        assert_eq!(
            result.skills_by_agent.get("detected-agent").copied(),
            Some(1)
        );
    }

    #[tokio::test]
    async fn codex_external_skill_is_not_manageable() {
        use crate::db;

        let tmp = TempDir::new().unwrap();
        let pool = setup_test_db().await;

        pool.write(|d| {
            d.table_mut("agents").clear();
            Ok(())
        })
        .await
        .unwrap();
        crate::db::update_config(&pool, |config| {
            config.scan_directories.clear();
            Ok(())
        })
        .unwrap();

        let codex_agent = db::Agent {
            id: "codex".to_string(),
            display_name: "Codex".to_string(),
            category: "platform".to_string(),
            global_skills_dir: tmp.path().to_string_lossy().into_owned(),
            project_skills_dir: None,
            is_detected: false,
            is_builtin: false,
            is_enabled: true,
        };
        db::insert_custom_agent(&pool, &codex_agent).await.unwrap();

        let skill_dir = create_skill_dir(
            tmp.path(),
            "external-codex-skill",
            &valid_skill_md("External Codex Skill", "Installed outside SkillsHub"),
        );

        let result = scan_all_skills_impl(&pool).await.unwrap();

        assert_eq!(
            db::get_skills_for_agent(&pool, "codex")
                .await
                .unwrap()
                .len(),
            0
        );
        assert_eq!(result.skills_by_agent.get("codex").copied(), Some(0));
        assert!(skill_dir.join("SKILL.md").is_file());
    }

    #[tokio::test]
    async fn rescan_preserves_managed_resource_skill_installed_to_codex() {
        use crate::{commands::linker::install_skill_to_agent_impl, db};

        let tmp = TempDir::new().unwrap();
        let central_dir = tmp.path().join("central");
        let codex_dir = tmp.path().join("codex");
        let resource_dir = tmp.path().join("resource-library");
        fs::create_dir_all(&central_dir).unwrap();
        fs::create_dir_all(&resource_dir).unwrap();

        let pool = setup_test_db().await;
        pool.write(|d| {
            d.table_mut("agents").clear();
            Ok(())
        })
        .await
        .unwrap();
        crate::db::update_config(&pool, |config| {
            config.scan_directories.clear();
            Ok(())
        })
        .unwrap();

        for agent in [
            db::Agent {
                id: "central".to_string(),
                display_name: "Central".to_string(),
                category: "central".to_string(),
                global_skills_dir: central_dir.to_string_lossy().into_owned(),
                project_skills_dir: None,
                is_detected: false,
                is_builtin: false,
                is_enabled: true,
            },
            db::Agent {
                id: "codex".to_string(),
                display_name: "Codex".to_string(),
                category: "platform".to_string(),
                global_skills_dir: codex_dir.to_string_lossy().into_owned(),
                project_skills_dir: None,
                is_detected: false,
                is_builtin: false,
                is_enabled: true,
            },
        ] {
            db::insert_custom_agent(&pool, &agent).await.unwrap();
        }

        let resource_skill_dir = create_skill_dir(
            &resource_dir,
            "resource-only",
            &valid_skill_md("Resource Only", "Resource library skill"),
        );
        db::upsert_skill(
            &pool,
            &db::Skill {
                id: "resource-only".to_string(),
                name: "Resource Only".to_string(),
                description: Some("Resource library skill".to_string()),
                file_path: resource_skill_dir
                    .join("SKILL.md")
                    .to_string_lossy()
                    .into_owned(),
                canonical_path: Some(resource_skill_dir.to_string_lossy().into_owned()),
                is_central: false,
                source: Some("resource-library".to_string()),
                content: None,
                scanned_at: Utc::now().to_rfc3339(),
            },
        )
        .await
        .unwrap();

        install_skill_to_agent_impl(&pool, "resource-only", "codex")
            .await
            .unwrap();

        let result = scan_all_skills_impl(&pool).await.unwrap();
        let codex_skills = db::get_skills_for_agent(&pool, "codex").await.unwrap();

        assert!(
            codex_skills.iter().any(|skill| skill.id == "resource-only"),
            "managed resource skill should remain listed after the rescan"
        );
        assert_eq!(result.skills_by_agent.get("codex").copied(), Some(1));
    }

    #[tokio::test]
    async fn full_scan_preserves_managed_installation_seen_only_as_read_only_compatibility() {
        use crate::{commands::linker::install_skill_to_agent_impl, db};

        let tmp = TempDir::new().unwrap();
        let central_dir = tmp.path().join("central");
        let shared_dir = tmp.path().join(".agents/skills");
        let resource_dir = tmp.path().join("resource-library");
        fs::create_dir_all(&central_dir).unwrap();
        fs::create_dir_all(&shared_dir).unwrap();
        fs::create_dir_all(&resource_dir).unwrap();

        let pool = setup_test_db().await;
        pool.write(|d| {
            d.table_mut("agents").clear();
            Ok(())
        })
        .await
        .unwrap();
        crate::db::update_config(&pool, |config| {
            config.scan_directories.clear();
            Ok(())
        })
        .unwrap();

        for agent in [
            db::Agent {
                id: "central".to_string(),
                display_name: "Central".to_string(),
                category: "central".to_string(),
                global_skills_dir: central_dir.to_string_lossy().into_owned(),
                project_skills_dir: None,
                is_detected: false,
                is_builtin: false,
                is_enabled: true,
            },
            db::Agent {
                id: "antigravity".to_string(),
                display_name: "Antigravity".to_string(),
                category: "platform".to_string(),
                global_skills_dir: shared_dir.to_string_lossy().into_owned(),
                project_skills_dir: None,
                is_detected: false,
                is_builtin: false,
                is_enabled: true,
            },
        ] {
            db::insert_custom_agent(&pool, &agent).await.unwrap();
        }

        let resource_skill_dir = create_skill_dir(
            &resource_dir,
            "resource-only",
            &valid_skill_md("Resource Only", "Resource library skill"),
        );
        db::upsert_skill(
            &pool,
            &db::Skill {
                id: "resource-only".to_string(),
                name: "Resource Only".to_string(),
                description: Some("Resource library skill".to_string()),
                file_path: resource_skill_dir
                    .join("SKILL.md")
                    .to_string_lossy()
                    .into_owned(),
                canonical_path: Some(resource_skill_dir.to_string_lossy().into_owned()),
                is_central: false,
                source: Some("resource-library".to_string()),
                content: None,
                scanned_at: Utc::now().to_rfc3339(),
            },
        )
        .await
        .unwrap();

        install_skill_to_agent_impl(&pool, "resource-only", "antigravity")
            .await
            .unwrap();
        let before = db::get_skill_installations(&pool, "resource-only")
            .await
            .unwrap()
            .pop()
            .expect("managed installation before scan");
        assert!(fs::symlink_metadata(&before.installed_path).is_ok());

        scan_all_skills_impl(&pool).await.unwrap();

        let after = db::get_skill_installations(&pool, "resource-only")
            .await
            .unwrap();
        assert_eq!(after.len(), 1, "the existing managed path must survive");
        assert_eq!(after[0].installed_path, before.installed_path);
    }

    #[tokio::test]
    async fn full_scan_removes_missing_managed_path_despite_compatibility_name_match() {
        use crate::db;

        let tmp = TempDir::new().unwrap();
        let central_dir = tmp.path().join("central");
        let cursor_dir = tmp.path().join(".cursor/skills");
        let shared_dir = tmp.path().join(".agents/skills");
        fs::create_dir_all(&central_dir).unwrap();
        fs::create_dir_all(&cursor_dir).unwrap();
        fs::create_dir_all(&shared_dir).unwrap();
        create_skill_dir(
            &shared_dir,
            "same-name",
            &valid_skill_md("Same Name", "Compatibility observation"),
        );

        let pool = setup_test_db().await;
        pool.write(|d| {
            d.table_mut("agents").clear();
            Ok(())
        })
        .await
        .unwrap();
        crate::db::update_config(&pool, |config| {
            config.scan_directories.clear();
            Ok(())
        })
        .unwrap();
        for agent in [
            db::Agent {
                id: "central".to_string(),
                display_name: "Central".to_string(),
                category: "central".to_string(),
                global_skills_dir: central_dir.to_string_lossy().into_owned(),
                project_skills_dir: None,
                is_detected: false,
                is_builtin: false,
                is_enabled: true,
            },
            db::Agent {
                id: "cursor".to_string(),
                display_name: "Cursor".to_string(),
                category: "platform".to_string(),
                global_skills_dir: cursor_dir.to_string_lossy().into_owned(),
                project_skills_dir: None,
                is_detected: false,
                is_builtin: false,
                is_enabled: true,
            },
        ] {
            db::insert_custom_agent(&pool, &agent).await.unwrap();
        }

        db::upsert_skill(
            &pool,
            &db::Skill {
                id: "same-name".to_string(),
                name: "Same Name".to_string(),
                description: None,
                file_path: tmp
                    .path()
                    .join("resource/same-name/SKILL.md")
                    .to_string_lossy()
                    .into_owned(),
                canonical_path: Some(
                    tmp.path()
                        .join("resource/same-name")
                        .to_string_lossy()
                        .into_owned(),
                ),
                is_central: false,
                source: Some("resource-library".to_string()),
                content: None,
                scanned_at: Utc::now().to_rfc3339(),
            },
        )
        .await
        .unwrap();
        db::upsert_skill_installation(
            &pool,
            &db::SkillInstallation {
                skill_id: "same-name".to_string(),
                agent_id: "cursor".to_string(),
                installed_path: cursor_dir.join("same-name").to_string_lossy().into_owned(),
                link_type: "copy".to_string(),
                symlink_target: None,
                created_at: Utc::now().to_rfc3339(),
            },
        )
        .await
        .unwrap();

        scan_all_skills_impl(&pool).await.unwrap();

        assert!(
            db::get_skill_installations(&pool, "same-name")
                .await
                .unwrap()
                .is_empty(),
            "a compatibility name match must not preserve a missing managed path"
        );
    }

    #[tokio::test]
    async fn test_scan_all_skills_impl_persists_skills() {
        use crate::db;

        let tmp = TempDir::new().unwrap();
        let pool = setup_test_db().await;
        isolate_scan_test(&pool).await;

        // Add a custom agent pointing to our temp directory
        let test_agent = db::Agent {
            id: "test-agent".to_string(),
            display_name: "Test Agent".to_string(),
            category: "platform".to_string(),
            global_skills_dir: tmp.path().to_string_lossy().into_owned(),
            project_skills_dir: None,
            is_detected: false,
            is_builtin: false,
            is_enabled: true,
        };
        db::insert_custom_agent(&pool, &test_agent).await.unwrap();

        // Create skills in the temp directory
        create_skill_dir(
            tmp.path(),
            "alpha-skill",
            &valid_skill_md("Alpha Skill", "First skill"),
        );
        create_skill_dir(
            tmp.path(),
            "beta-skill",
            &valid_skill_md("Beta Skill", "Second skill"),
        );

        let result = scan_all_skills_impl(&pool).await.unwrap();

        assert_eq!(result.total_skills, 2);
        assert_eq!(result.skills_by_agent.get("test-agent").copied(), Some(0));

        // Scanned skills remain in the logical skills table but are not managed
        // platform installations.
        let skills_in_db = db::get_skills_by_agent(&pool, "test-agent").await.unwrap();
        assert!(skills_in_db.is_empty());
        assert!(db::get_skill_by_id(&pool, "alpha-skill")
            .await
            .unwrap()
            .is_some());
        assert!(db::get_skill_by_id(&pool, "beta-skill")
            .await
            .unwrap()
            .is_some());
    }

    #[tokio::test]
    async fn test_scan_all_skills_impl_central_skills_are_marked() {
        use crate::db;

        let tmp = TempDir::new().unwrap();
        let pool = setup_test_db().await;

        // Override the "central" agent's dir with our temp dir by inserting a
        // custom agent with id "central-test".
        let central_agent = db::Agent {
            id: "central-test".to_string(),
            display_name: "Central Test".to_string(),
            category: "central".to_string(),
            global_skills_dir: tmp.path().to_string_lossy().into_owned(),
            project_skills_dir: None,
            is_detected: false,
            is_builtin: false,
            is_enabled: true,
        };
        db::insert_custom_agent(&pool, &central_agent)
            .await
            .unwrap();

        create_skill_dir(
            tmp.path(),
            "canon-skill",
            &valid_skill_md("Canon Skill", "Canonical"),
        );

        scan_all_skills_impl(&pool).await.unwrap();

        // Not is_central because agent id is "central-test", not "central"
        // (the "central" agent points to a non-existent dir in CI)
        let skill = db::get_skill_by_id(&pool, "canon-skill").await.unwrap();
        assert!(skill.is_some());
    }

    #[tokio::test]
    async fn test_scan_all_skills_impl_with_custom_scan_directory() {
        use crate::db;

        let tmp = TempDir::new().unwrap();
        let pool = setup_test_db().await;
        isolate_scan_test(&pool).await;
        let project_dir = tmp.path().join("project");
        let project_skills_dir = project_dir.join(linker::PROJECT_SKILLS_SUBDIR);
        fs::create_dir_all(&project_skills_dir).unwrap();

        // Add a project directory. Skills are managed under .agents/skills.
        let scan_dir =
            db::add_scan_directory(&pool, project_dir.to_str().unwrap(), Some("Test Dir"))
                .await
                .unwrap();

        create_skill_dir(
            &project_skills_dir,
            "custom-dir-skill",
            &valid_skill_md("Custom Dir Skill", "From custom dir"),
        );

        let result = scan_all_skills_impl(&pool).await.unwrap();
        let project_agent_id = linker::project_agent_id(scan_dir.id);
        assert_eq!(result.total_skills, 1);
        assert_eq!(
            result.skills_by_agent.get(&project_agent_id).copied(),
            Some(1),
            "project directories should report managed skill counts under their virtual agent id"
        );

        // Skill should be in the DB
        let skill = db::get_skill_by_id(&pool, "custom-dir-skill")
            .await
            .unwrap();
        assert!(skill.is_some());

        let observed_count = pool
            .read(|d| {
                Ok(d.table("skill_installations")
                    .values()
                    .filter(|r| {
                        r["skill_id"] == "custom-dir-skill"
                            && r["agent_id"] == project_agent_id
                            && !r["is_managed"].as_bool().unwrap_or(false)
                    })
                    .count())
            })
            .unwrap();
        assert_eq!(observed_count, 1);
    }

    #[tokio::test]
    async fn test_scan_all_skills_impl_persists_nested_platform_skills() {
        use crate::db;

        let tmp = TempDir::new().unwrap();
        let pool = setup_test_db().await;
        isolate_scan_test(&pool).await;

        let test_agent = db::Agent {
            id: "nested-agent".to_string(),
            display_name: "Nested Agent".to_string(),
            category: "platform".to_string(),
            global_skills_dir: tmp.path().to_string_lossy().into_owned(),
            project_skills_dir: None,
            is_detected: false,
            is_builtin: false,
            is_enabled: true,
        };
        db::insert_custom_agent(&pool, &test_agent).await.unwrap();

        create_skill_dir(
            &tmp.path().join("apple"),
            "apple-reminders",
            &valid_skill_md("Apple Reminders", "Nested platform skill"),
        );

        let result = scan_all_skills_impl(&pool).await.unwrap();

        assert_eq!(result.total_skills, 1);
        assert_eq!(result.skills_by_agent.get("nested-agent").copied(), Some(0));
        let skill = db::get_skill_by_id(&pool, "apple-reminders")
            .await
            .unwrap()
            .expect("nested skill should remain in the logical skills table");
        assert!(normalize_test_path(&skill.file_path).contains("apple/apple-reminders/SKILL.md"));
    }

    #[tokio::test]
    async fn test_scan_all_skills_impl_returns_per_agent_counts() {
        use crate::db;

        let tmp_a = TempDir::new().unwrap();
        let tmp_b = TempDir::new().unwrap();
        let pool = setup_test_db().await;
        isolate_scan_test(&pool).await;

        let agent_a = db::Agent {
            id: "agent-a".to_string(),
            display_name: "Agent A".to_string(),
            category: "platform".to_string(),
            global_skills_dir: tmp_a.path().to_string_lossy().into_owned(),
            project_skills_dir: None,
            is_detected: false,
            is_builtin: false,
            is_enabled: true,
        };
        let agent_b = db::Agent {
            id: "agent-b".to_string(),
            display_name: "Agent B".to_string(),
            category: "platform".to_string(),
            global_skills_dir: tmp_b.path().to_string_lossy().into_owned(),
            project_skills_dir: None,
            is_detected: false,
            is_builtin: false,
            is_enabled: true,
        };
        db::insert_custom_agent(&pool, &agent_a).await.unwrap();
        db::insert_custom_agent(&pool, &agent_b).await.unwrap();

        create_skill_dir(tmp_a.path(), "skill-x", &valid_skill_md("Skill X", "In A"));
        create_skill_dir(
            tmp_a.path(),
            "skill-y",
            &valid_skill_md("Skill Y", "In A too"),
        );
        create_skill_dir(tmp_b.path(), "skill-z", &valid_skill_md("Skill Z", "In B"));

        let result = scan_all_skills_impl(&pool).await.unwrap();

        assert_eq!(result.total_skills, 3);
        assert_eq!(result.skills_by_agent.get("agent-a").copied(), Some(0));
        assert_eq!(result.skills_by_agent.get("agent-b").copied(), Some(0));
    }

    #[tokio::test]
    async fn test_scan_all_skills_impl_factory_droid_observes_agents_skills_read_only() {
        let tmp = TempDir::new().unwrap();
        let pool = setup_test_db().await;
        // This platform is optional in the trimmed catalog; seed the test fixture explicitly.
        crate::storage::fixtures::insert(&pool, "agents", serde_json::json!({"id":"factory-droid","display_name":"Factory Droid","category":"platform","global_skills_dir":"~/.factory/skills","is_builtin":false,"is_enabled":true})).await.unwrap();

        pool.write(|d| {
            d.table_mut("agents")
                .retain(|id, _| id == "factory-droid" || id == "central");
            Ok(())
        })
        .await
        .unwrap();
        crate::db::update_config(&pool, |config| {
            config.scan_directories.clear();
            Ok(())
        })
        .unwrap();

        let factory_root = tmp.path().join(".factory/skills");
        let shared_root = tmp.path().join(".agents/skills");
        fs::create_dir_all(&factory_root).unwrap();
        fs::create_dir_all(&shared_root).unwrap();

        create_skill_dir(
            &shared_root.join("superpowers"),
            "using-superpowers",
            &valid_skill_md("Using Superpowers", "Shared Factory-compatible skill"),
        );

        pool.write(|d| {
            if let Some(row) = d.table_mut("agents").get_mut("factory-droid") {
                row["global_skills_dir"] =
                    serde_json::json!(factory_root.to_string_lossy().to_string());
            }
            Ok(())
        })
        .await
        .unwrap();
        pool.write(|d| {
            if let Some(row) = d.table_mut("agents").get_mut("central") {
                row["global_skills_dir"] =
                    serde_json::json!(shared_root.to_string_lossy().to_string());
            }
            Ok(())
        })
        .await
        .unwrap();

        let result = scan_all_skills_impl(&pool).await.unwrap();

        assert_eq!(
            result.skills_by_agent.get("factory-droid").copied(),
            Some(0)
        );
        let observations = db::get_agent_skill_observations(&pool, "factory-droid")
            .await
            .unwrap();
        assert_eq!(observations.len(), 1);
        assert_eq!(observations[0].skill_id, "using-superpowers");
        assert_eq!(observations[0].source_kind, "compatibility");
        assert!(observations[0].is_read_only);
        assert!(normalize_test_path(&observations[0].dir_path)
            .contains("superpowers/using-superpowers"));

        let platform_skills = db::get_skills_for_agent(&pool, "factory-droid")
            .await
            .unwrap();
        assert!(
            platform_skills.is_empty(),
            "read-only compatibility observations are not manageable platform skills"
        );

        let factory_installations: Vec<_> = db::get_skill_installations(&pool, "using-superpowers")
            .await
            .unwrap()
            .into_iter()
            .filter(|installation| installation.agent_id == "factory-droid")
            .collect();
        assert!(
            factory_installations.is_empty(),
            "shared .agents skills must not be persisted as removable Factory Droid installs"
        );

        let skill = db::get_skill_by_id(&pool, "using-superpowers")
            .await
            .unwrap()
            .expect("central scan should persist the shared skill");
        assert!(skill.is_central);
        assert_eq!(
            skill.canonical_path.as_deref().map(normalize_test_path),
            Some(normalize_test_path(
                shared_root
                    .join("superpowers/using-superpowers")
                    .to_string_lossy()
            ))
        );
    }

    #[tokio::test]
    async fn test_scan_all_skills_impl_universal_agent_observes_agents_skills_read_only() {
        let tmp = TempDir::new().unwrap();
        let pool = setup_test_db().await;

        pool.write(|d| {
            d.table_mut("agents")
                .retain(|id, _| id == "cursor" || id == "central");
            Ok(())
        })
        .await
        .unwrap();
        crate::db::update_config(&pool, |config| {
            config.scan_directories.clear();
            Ok(())
        })
        .unwrap();

        let cursor_root = tmp.path().join(".cursor/skills");
        let shared_root = tmp.path().join(".agents/skills");
        fs::create_dir_all(&cursor_root).unwrap();
        fs::create_dir_all(&shared_root).unwrap();

        create_skill_dir(
            &shared_root,
            "shared-skill",
            &valid_skill_md("Shared Skill", "Universal compatibility skill"),
        );

        pool.write(|d| {
            if let Some(row) = d.table_mut("agents").get_mut("cursor") {
                row["global_skills_dir"] =
                    serde_json::json!(cursor_root.to_string_lossy().to_string());
            }
            Ok(())
        })
        .await
        .unwrap();
        pool.write(|d| {
            if let Some(row) = d.table_mut("agents").get_mut("central") {
                row["global_skills_dir"] =
                    serde_json::json!(shared_root.to_string_lossy().to_string());
            }
            Ok(())
        })
        .await
        .unwrap();

        let result = scan_all_skills_impl(&pool).await.unwrap();

        assert_eq!(result.skills_by_agent.get("cursor").copied(), Some(0));
        let observations = db::get_agent_skill_observations(&pool, "cursor")
            .await
            .unwrap();
        assert_eq!(observations.len(), 1);
        assert_eq!(observations[0].source_kind, "compatibility");
        assert!(observations[0].is_read_only);

        let cursor_installations: Vec<_> = db::get_skill_installations(&pool, "shared-skill")
            .await
            .unwrap()
            .into_iter()
            .filter(|installation| installation.agent_id == "cursor")
            .collect();
        assert!(
            cursor_installations.is_empty(),
            "shared .agents skills must not be persisted as removable universal-agent installs"
        );
    }

    #[tokio::test]
    async fn test_scan_all_skills_impl_universal_primary_agents_skills_is_read_only() {
        let tmp = TempDir::new().unwrap();
        let pool = setup_test_db().await;

        pool.write(|d| {
            d.table_mut("agents")
                .retain(|id, _| id == "antigravity" || id == "central");
            Ok(())
        })
        .await
        .unwrap();
        crate::db::update_config(&pool, |config| {
            config.scan_directories.clear();
            Ok(())
        })
        .unwrap();

        let shared_root = tmp.path().join(".agents/skills");
        fs::create_dir_all(&shared_root).unwrap();
        create_skill_dir(
            &shared_root,
            "native-universal-skill",
            &valid_skill_md("Native Universal Skill", "Primary root is shared"),
        );

        pool.write(|d| {
            for id in ["antigravity", "central"] {
                d.table_mut("agents").get_mut(id).unwrap()["global_skills_dir"] =
                    shared_root.to_string_lossy().to_string().into();
            }
            Ok(())
        })
        .await
        .unwrap();

        let result = scan_all_skills_impl(&pool).await.unwrap();

        assert_eq!(
            result.skills_by_agent.get("antigravity").copied(),
            Some(1),
            "platforms sharing the Central Skills directory must be counted as populated"
        );

        let observations = db::get_agent_skill_observations(&pool, "antigravity")
            .await
            .unwrap();
        assert_eq!(observations.len(), 1);
        assert_eq!(observations[0].source_kind, "compatibility");
        assert!(observations[0].is_read_only);

        let antigravity_installations: Vec<_> =
            db::get_skill_installations(&pool, "native-universal-skill")
                .await
                .unwrap()
                .into_iter()
                .filter(|installation| installation.agent_id == "antigravity")
                .collect();
        assert!(
            antigravity_installations.is_empty(),
            "universal platforms that use .agents/skills as primary root must remain read-only"
        );
    }

    #[tokio::test]
    async fn test_scan_all_skills_impl_factory_droid_external_duplicates_stay_unmanaged() {
        let tmp = TempDir::new().unwrap();
        let pool = setup_test_db().await;
        // This platform is optional in the trimmed catalog; seed the test fixture explicitly.
        crate::storage::fixtures::insert(&pool, "agents", serde_json::json!({"id":"factory-droid","display_name":"Factory Droid","category":"platform","global_skills_dir":"~/.factory/skills","is_builtin":false,"is_enabled":true})).await.unwrap();

        pool.write(|d| {
            d.table_mut("agents")
                .retain(|id, _| id == "factory-droid" || id == "central");
            Ok(())
        })
        .await
        .unwrap();
        crate::db::update_config(&pool, |config| {
            config.scan_directories.clear();
            Ok(())
        })
        .unwrap();

        let factory_root = tmp.path().join(".factory/skills");
        let shared_root = tmp.path().join(".agents/skills");
        fs::create_dir_all(&factory_root).unwrap();
        fs::create_dir_all(&shared_root).unwrap();

        create_skill_dir(
            &factory_root,
            "shared-skill",
            &valid_skill_md("Factory Skill", "Factory-specific copy"),
        );
        create_skill_dir(
            &shared_root,
            "shared-skill",
            &valid_skill_md("Central Skill", "Shared compatibility copy"),
        );

        pool.write(|d| {
            if let Some(row) = d.table_mut("agents").get_mut("factory-droid") {
                row["global_skills_dir"] =
                    serde_json::json!(factory_root.to_string_lossy().to_string());
            }
            Ok(())
        })
        .await
        .unwrap();
        pool.write(|d| {
            if let Some(row) = d.table_mut("agents").get_mut("central") {
                row["global_skills_dir"] =
                    serde_json::json!(shared_root.to_string_lossy().to_string());
            }
            Ok(())
        })
        .await
        .unwrap();

        scan_all_skills_impl(&pool).await.unwrap();

        let factory_installations = db::get_skill_installations(&pool, "shared-skill")
            .await
            .unwrap()
            .into_iter()
            .filter(|installation| installation.agent_id == "factory-droid")
            .collect::<Vec<_>>();
        assert!(factory_installations.is_empty());

        let platform_skills = db::get_skills_for_agent(&pool, "factory-droid")
            .await
            .unwrap();
        assert!(
            platform_skills.is_empty(),
            "neither an external primary skill nor a compatibility observation is managed"
        );
        let observations = db::get_agent_skill_observations(&pool, "factory-droid")
            .await
            .unwrap();
        assert_eq!(observations.len(), 1);
        assert!(observations[0].is_read_only);
        assert_eq!(
            observations[0].dir_path,
            shared_root.join("shared-skill").to_string_lossy()
        );
    }

    // ── Regression: Bug 1 — installed_path must be the skill directory ────────

    /// installed_path should point to the skill directory, not to the SKILL.md
    /// file inside it.
    #[tokio::test]
    async fn test_installed_path_is_skill_directory_not_skill_md() {
        use crate::db;

        let tmp = TempDir::new().unwrap();
        let pool = setup_test_db().await;

        let test_agent = db::Agent {
            id: "path-agent".to_string(),
            display_name: "Path Agent".to_string(),
            category: "platform".to_string(),
            global_skills_dir: tmp.path().to_string_lossy().into_owned(),
            project_skills_dir: None,
            is_detected: false,
            is_builtin: false,
            is_enabled: true,
        };
        db::insert_custom_agent(&pool, &test_agent).await.unwrap();

        let skill_dir =
            create_skill_dir(tmp.path(), "my-skill", &valid_skill_md("My Skill", "desc"));

        scan_all_skills_impl(&pool).await.unwrap();

        let installation: db::SkillInstallation = pool
            .read(|d| {
                serde_json::from_value(
                    d.table("skill_installations")
                        .values()
                        .find(|r| r["skill_id"] == "my-skill" && r["agent_id"] == "path-agent")
                        .unwrap()
                        .clone(),
                )
                .map_err(|e| e.to_string())
            })
            .unwrap();

        // installed_path must NOT be the SKILL.md file path.
        assert!(
            !installation.installed_path.ends_with("SKILL.md"),
            "installed_path should not point to the SKILL.md file; got: {}",
            installation.installed_path
        );
        // installed_path must equal the skill directory path.
        assert_eq!(
            installation.installed_path,
            skill_dir.to_string_lossy().as_ref(),
            "installed_path should be the skill directory, not the SKILL.md inside it"
        );
    }

    // ── Regression: Bug 2 — rescan removes stale skills from DB ──────────────

    /// After removing a skill from disk and rescanning, the corresponding rows
    /// must no longer appear in skills or skill_installations queries.
    #[tokio::test]
    async fn test_rescan_removes_deleted_skills_from_db() {
        use crate::db;

        let tmp = TempDir::new().unwrap();
        let pool = setup_test_db().await;

        let test_agent = db::Agent {
            id: "stale-agent".to_string(),
            display_name: "Stale Agent".to_string(),
            category: "platform".to_string(),
            global_skills_dir: tmp.path().to_string_lossy().into_owned(),
            project_skills_dir: None,
            is_detected: false,
            is_builtin: false,
            is_enabled: true,
        };
        db::insert_custom_agent(&pool, &test_agent).await.unwrap();

        // Create two skills on disk.
        create_skill_dir(
            tmp.path(),
            "skill-keep",
            &valid_skill_md("Keep Skill", "stays"),
        );
        create_skill_dir(
            tmp.path(),
            "skill-remove",
            &valid_skill_md("Remove Skill", "will be deleted"),
        );

        // First scan — both skills should be persisted.
        scan_all_skills_impl(&pool).await.unwrap();
        assert!(db::get_skill_by_id(&pool, "skill-keep")
            .await
            .unwrap()
            .is_some());
        assert!(db::get_skill_by_id(&pool, "skill-remove")
            .await
            .unwrap()
            .is_some());

        // Remove "skill-remove" from disk.
        fs::remove_dir_all(tmp.path().join("skill-remove")).unwrap();

        // Second scan — "skill-remove" must disappear from the DB.
        scan_all_skills_impl(&pool).await.unwrap();

        assert!(db::get_skill_by_id(&pool, "skill-keep")
            .await
            .unwrap()
            .is_some());

        // The deleted skill must also be gone from the skills table.
        let stale_skill = db::get_skill_by_id(&pool, "skill-remove").await.unwrap();
        assert!(
            stale_skill.is_none(),
            "skill-remove should be removed from the skills table after rescan"
        );

        // No orphaned installation record should remain.
        let stale_inst = db::get_skill_installations(&pool, "skill-remove")
            .await
            .unwrap();
        assert!(
            stale_inst.is_empty(),
            "skill-remove's installation record should be removed after rescan"
        );
    }

    // ── Regression: is_central preserved when codex shares the central dir ───

    /// When the Shared Hub and a platform agent both
    /// point to the same directory, skills from that directory must end up with
    /// `is_central = true` after scanning — regardless of scan order.
    ///
    /// Historically this failed because:
    ///  1. The scan used platform grouping instead of the fixed Shared Hub id,
    ///     so the codex agent always cleared the flag.
    ///  2. Even after fixing the flag, the `INSERT OR REPLACE` would overwrite
    ///     `is_central = true` with `false` when codex was processed last.
    #[tokio::test]
    async fn test_is_central_preserved_when_shared_with_platform_agent() {
        let tmp = TempDir::new().unwrap();
        let pool = setup_test_db().await;

        // Point Shared Hub at the shared temp directory.
        // Use "AA Central Test" as the display_name so it sorts BEFORE "ZZ Codex Test"
        // (ORDER BY display_name ASC) ensuring the central scan runs first.
        pool.write(|d| {
            let row = d.table_mut("agents").get_mut("central").unwrap();
            row["display_name"] = "AA Central Test".into();
            row["global_skills_dir"] = tmp.path().to_string_lossy().into_owned().into();
            Ok(())
        })
        .await
        .unwrap();

        // Point a platform agent at the SAME temp directory,
        // sorted AFTER the central agent so it is processed last (worst case).
        pool.write(|d| {
            let row = d.table_mut("agents").get_mut("codex").unwrap();
            row["display_name"] = "ZZ Codex Test".into();
            row["global_skills_dir"] = tmp.path().to_string_lossy().into_owned().into();
            Ok(())
        })
        .await
        .unwrap();

        // Place one skill in the shared directory.
        create_skill_dir(
            tmp.path(),
            "shared-skill",
            &valid_skill_md("Shared Skill", "desc"),
        );

        // Run the full scan. The platform agent is processed AFTER Shared Hub
        // (due to display_name ordering), which is the failure scenario for the bug.
        scan_all_skills_impl(&pool).await.unwrap();

        // The skill must still be marked as central even though the platform agent
        // scanned the same directory afterwards.
        let skill = db::get_skill_by_id(&pool, "shared-skill")
            .await
            .unwrap()
            .expect("shared-skill must be in the DB");
        assert!(
            skill.is_central,
            "skill should remain is_central=true even when a platform agent \
             scans the same directory after Shared Hub"
        );
    }
}
