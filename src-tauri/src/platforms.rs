use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

use crate::db::Agent;
#[cfg(not(test))]
use crate::path_utils::app_data_dir;
use crate::path_utils::{expand_home_path, path_to_string};

const DEFAULTS_JSON: &str = include_str!("../resources/default-platforms/defaults.json");

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlatformDefinition {
    pub id: String,
    pub display_name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    pub global_skills_dir: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project_skills_dir: Option<String>,
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    #[serde(default)]
    pub supports_universal_agents_skills: bool,
}

fn default_enabled() -> bool {
    true
}

#[cfg(not(test))]
pub fn platform_dir() -> Option<PathBuf> {
    Some(app_data_dir().join("platform"))
}

#[cfg(test)]
pub fn platform_dir() -> Option<PathBuf> {
    std::env::var_os("SKILLSHUB_PLATFORM_DIR")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
}

fn catalog_path(dir: &Path) -> PathBuf {
    dir.parent().unwrap_or(Path::new(".")).join("config.json")
}

pub fn default_platform_definitions() -> Result<Vec<PlatformDefinition>, String> {
    serde_json::from_str(DEFAULTS_JSON)
        .map_err(|e| format!("Invalid default platform catalog: {}", e))
}

fn default_platform_ids() -> HashSet<String> {
    default_platform_definitions()
        .unwrap_or_default()
        .into_iter()
        .map(|platform| platform.id)
        .collect()
}

fn is_project_agent_id(agent_id: &str) -> bool {
    agent_id.starts_with("project:")
}

fn seed_platform_dir(dir: &Path) -> Result<(), String> {
    // Missing platform configuration uses bundled defaults; an explicit empty list stays empty.
    if !catalog_path(dir).exists() {
        crate::config_store::update_path(&catalog_path(dir), |_| Ok(()))?;
    }
    load_from_dir(dir)?;
    crate::platform_icons::seed(dir.parent().unwrap_or(Path::new(".")))?;
    match fs::remove_file(dir.join("platform.json")) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(_) => Err("Failed to remove obsolete platform/platform.json".into()),
    }
}

fn load_from_dir(dir: &Path) -> Result<Vec<PlatformDefinition>, String> {
    let config = crate::config_store::load_path(&catalog_path(dir))?;
    let mut seen = HashSet::new();
    for definition in &config.platforms {
        if definition.id.trim().is_empty() || !seen.insert(&definition.id) {
            return Err("Platform IDs in config.json must be nonempty and unique".into());
        }
    }
    Ok(config.platforms)
}

pub fn load_platform_definitions() -> Result<Vec<PlatformDefinition>, String> {
    match platform_dir() {
        Some(dir) => {
            seed_platform_dir(&dir)?;
            load_from_dir(&dir)
        }
        None => default_platform_definitions(),
    }
}

fn portable_global_skills_dir(raw: &str) -> String {
    path_to_string(&expand_home_path(raw))
}

pub fn agents_from_definitions(definitions: &[PlatformDefinition]) -> Vec<Agent> {
    let builtin_ids = default_platform_ids();
    let mut agents: Vec<Agent> = definitions
        .iter()
        .filter(|definition| !is_project_agent_id(&definition.id))
        .map(|definition| Agent {
            id: definition.id.clone(),
            display_name: definition.display_name.clone(),
            category: if definition.id == "central" {
                "central".to_string()
            } else {
                "platform".to_string()
            },
            global_skills_dir: portable_global_skills_dir(&definition.global_skills_dir),
            project_skills_dir: definition.project_skills_dir.clone(),
            is_detected: false,
            is_builtin: definition.id == "central" || builtin_ids.contains(&definition.id),
            is_enabled: definition.enabled,
        })
        .collect();
    // Shared Hub is an internal installation target, not an optional software platform.
    if !agents.iter().any(|agent| agent.id == "central") {
        agents.push(Agent {
            id: "central".into(),
            display_name: "Central Skills".into(),
            category: "central".into(),
            global_skills_dir: portable_global_skills_dir("~/.agents/skills"),
            project_skills_dir: None,
            is_detected: false,
            is_builtin: true,
            is_enabled: true,
        });
    }
    agents
}

pub fn builtin_agents() -> Vec<Agent> {
    load_platform_definitions()
        .map(|definitions| agents_from_definitions(&definitions))
        .unwrap_or_else(|_| Vec::new())
}

pub fn universal_agents_skills_agent_ids() -> Vec<String> {
    load_platform_definitions()
        .unwrap_or_default()
        .into_iter()
        .filter(|platform| platform.supports_universal_agents_skills)
        .map(|platform| platform.id)
        .collect()
}

pub fn agent_supports_universal_agents_skills(agent_id: &str) -> bool {
    universal_agents_skills_agent_ids()
        .iter()
        .any(|id| id == agent_id)
}

fn upsert_definition(platforms: &mut Vec<PlatformDefinition>, agent: &Agent) {
    let supports_universal = platforms
        .iter()
        .find(|platform| platform.id == agent.id)
        .map(|platform| platform.supports_universal_agents_skills)
        .unwrap_or_else(|| {
            default_platform_definitions()
                .unwrap_or_default()
                .iter()
                .any(|platform| {
                    platform.id == agent.id && platform.supports_universal_agents_skills
                })
        });
    if let Some(existing) = platforms
        .iter_mut()
        .find(|platform| platform.id == agent.id)
    {
        existing.display_name = agent.display_name.clone();
        existing.global_skills_dir = agent.global_skills_dir.clone();
        existing.project_skills_dir = agent.project_skills_dir.clone();
        existing.enabled = agent.is_enabled;
        existing.supports_universal_agents_skills = supports_universal;
        return;
    }
    platforms.push(PlatformDefinition {
        id: agent.id.clone(),
        icon: None,
        display_name: agent.display_name.clone(),
        global_skills_dir: agent.global_skills_dir.clone(),
        project_skills_dir: agent.project_skills_dir.clone(),
        enabled: agent.is_enabled,
        supports_universal_agents_skills: supports_universal,
    });
}

pub fn persist_platform_edit(agent: &Agent) -> Result<(), String> {
    if is_project_agent_id(&agent.id) {
        return Ok(());
    }
    let Some(dir) = platform_dir() else {
        return Ok(());
    };
    persist_platform_edit_in(&dir, agent)
}

fn persist_platform_edit_in(dir: &Path, agent: &Agent) -> Result<(), String> {
    seed_platform_dir(dir)?;
    crate::config_store::update_path(&catalog_path(dir), |config| {
        upsert_definition(&mut config.platforms, agent);
        Ok(())
    })
}

pub fn persist_platform_delete(agent_id: &str) -> Result<(), String> {
    if is_project_agent_id(agent_id) {
        return Ok(());
    }
    let Some(dir) = platform_dir() else {
        return Ok(());
    };
    persist_platform_delete_in(&dir, agent_id)
}

fn persist_platform_delete_in(dir: &Path, agent_id: &str) -> Result<(), String> {
    if !catalog_path(dir).exists() {
        return Ok(());
    }
    crate::config_store::update_path(&catalog_path(dir), |config| {
        config.platforms.retain(|platform| platform.id != agent_id);
        Ok(())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn default_catalog_includes_claude_code_and_runtime_adds_central() {
        let platforms = default_platform_definitions().unwrap();
        assert!(platforms
            .iter()
            .any(|platform| platform.id == "claude-code"));
        assert!(agents_from_definitions(&platforms)
            .iter()
            .any(|agent| agent.id == "central"));
        assert!(agents_from_definitions(&[])
            .iter()
            .any(|agent| agent.id == "central"));
        assert!(
            platforms
                .iter()
                .find(|platform| platform.id == "codex")
                .unwrap()
                .supports_universal_agents_skills
        );
    }

    #[test]
    fn seed_writes_catalog_then_loads() {
        let dir = tempdir().unwrap();
        seed_platform_dir(&dir.path().join("platform")).unwrap();
        assert!(catalog_path(&dir.path().join("platform")).exists());
        let loaded = load_from_dir(&dir.path().join("platform")).unwrap();
        assert_eq!(loaded.len(), default_platform_definitions().unwrap().len());
        assert!(loaded.iter().all(|platform| platform.enabled));
    }

    #[test]
    fn existing_catalog_is_not_overwritten_on_seed() {
        let dir = tempdir().unwrap();
        fs::create_dir_all(dir.path()).unwrap();
        let custom = PlatformDefinition {
            id: "only-custom".to_string(),
            icon: None,
            display_name: "Only Custom".to_string(),
            global_skills_dir: "~/.only-custom/skills".to_string(),
            project_skills_dir: None,
            enabled: true,
            supports_universal_agents_skills: false,
        };
        crate::config_store::update_path(&dir.path().join("config.json"), |config| {
            config.platforms = vec![custom];
            Ok(())
        })
        .unwrap();
        seed_platform_dir(&dir.path().join("platform")).unwrap();
        let loaded = load_from_dir(&dir.path().join("platform")).unwrap();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].id, "only-custom");
    }

    #[test]
    fn obsolete_catalog_is_removed_without_importing_it() {
        let dir = tempdir().unwrap();
        let platform_dir = dir.path().join("platform");
        fs::create_dir_all(&platform_dir).unwrap();
        fs::write(platform_dir.join("platform.json"), "{obsolete}").unwrap();
        seed_platform_dir(&platform_dir).unwrap();
        assert!(!platform_dir.join("platform.json").exists());
        assert!(!platform_dir.join("icons").exists());
        assert_eq!(
            load_from_dir(&platform_dir).unwrap().len(),
            default_platform_definitions().unwrap().len()
        );
    }

    #[test]
    fn platform_edits_preserve_other_configuration_and_an_empty_list() {
        let dir = tempdir().unwrap();
        let platform_dir = dir.path().join("platform");
        let path = dir.path().join("config.json");
        crate::config_store::update_path(&path, |config| {
            config.settings.insert("language".into(), "en".into());
            config.platforms.clear();
            Ok(())
        })
        .unwrap();
        seed_platform_dir(&platform_dir).unwrap();
        let config = crate::config_store::load_path(&path).unwrap();
        assert!(config.platforms.is_empty());
        assert_eq!(config.settings["language"], "en");
        assert!(!platform_dir.join("platform.json").exists());
    }

    #[test]
    fn platform_crud_uses_config_and_preserves_settings() {
        let dir = tempdir().unwrap();
        let platform_dir = dir.path().join("platform");
        seed_platform_dir(&platform_dir).unwrap();
        crate::config_store::update_path(&catalog_path(&platform_dir), |config| {
            config.settings.insert("language".into(), "en".into());
            Ok(())
        })
        .unwrap();
        let mut agent =
            agents_from_definitions(&default_platform_definitions().unwrap())[0].clone();
        agent.id = "custom-example".into();
        agent.display_name = "Example".into();
        persist_platform_edit_in(&platform_dir, &agent).unwrap();
        agent.is_enabled = false;
        agent.display_name = "Renamed".into();
        persist_platform_edit_in(&platform_dir, &agent).unwrap();
        let platforms = load_from_dir(&platform_dir).unwrap();
        let saved = platforms.iter().find(|entry| entry.id == agent.id).unwrap();
        assert_eq!(saved.display_name, "Renamed");
        assert!(!saved.enabled);
        persist_platform_delete_in(&platform_dir, &agent.id).unwrap();
        let config = crate::config_store::load_path(&catalog_path(&platform_dir)).unwrap();
        assert!(!config.platforms.iter().any(|entry| entry.id == agent.id));
        assert_eq!(config.settings["language"], "en");
        assert!(!platform_dir.join("platform.json").exists());
    }
}

pub fn persist_platform_rename(old_id: &str, agent: &Agent) -> Result<(), String> {
    let Some(dir) = platform_dir() else {
        return Ok(());
    };
    crate::config_store::update_path(&catalog_path(&dir), |config| {
        if let Some(platform) = config.platforms.iter_mut().find(|p| p.id == old_id) {
            platform.id = agent.id.clone();
        }
        upsert_definition(&mut config.platforms, agent);
        Ok(())
    })
}
