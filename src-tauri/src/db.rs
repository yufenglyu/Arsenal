pub mod backup;
pub mod collection_tree;
pub mod explanations;
pub mod imports;
pub mod metadata;
pub mod notebooks;
pub mod repository;
use crate::path_utils::{app_data_dir, expand_home_path, path_to_string};
use crate::storage::store::Data;
use chrono::Utc;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::{json, Value};
use std::{collections::HashSet, path::Path};
use uuid::Uuid;
pub type AppStore = crate::storage::store::Store;
pub use crate::db_config::{
    config_path, initialize as initialize_config, load as load_config, update as update_config,
};
pub use crate::platforms::{agent_supports_universal_agents_skills, builtin_agents};
pub use crate::storage::models::*;
pub const SKILL_RESOURCE_LIBRARY_DIR_SETTING_KEY: &str = "skill_resource_library_dir";
pub fn key2(a: &str, b: &str) -> String {
    serde_json::to_string(&(a, b)).expect("string pair")
}
pub fn rows<T: DeserializeOwned>(d: &Data, t: &str) -> Result<Vec<T>, String> {
    d.table(t)
        .values()
        .map(|v| T::deserialize(v).map_err(|e| e.to_string()))
        .collect()
}
pub fn put<T: Serialize>(d: &mut Data, t: &str, key: String, row: &T) -> Result<(), String> {
    d.table_mut(t)
        .insert(key, serde_json::to_value(row).map_err(|e| e.to_string())?);
    Ok(())
}
fn one<T: DeserializeOwned>(d: &Data, t: &str, k: &str) -> Result<Option<T>, String> {
    d.table(t)
        .get(k)
        .map(T::deserialize)
        .transpose()
        .map_err(|e| e.to_string())
}
fn field<'a>(v: &'a Value, k: &str) -> &'a str {
    v[k].as_str().unwrap_or("")
}
fn managed(v: &Value) -> bool {
    v["is_managed"].as_bool().unwrap_or(false)
}
#[cfg(test)]
pub async fn open_test_store(db_path: &str) -> Result<AppStore, String> {
    AppStore::connect(db_path).await
}

pub async fn init_schema(pool: &AppStore) -> Result<(), String> {
    initialize_config(pool)?;
    pool.write(|d| {
        let skills = d.table("skills").clone();
        for r in d.table_mut("skill_installations").values_mut() {
            if r.get("created_at").is_none() || r["created_at"].is_null() {
                r["created_at"] = json!(Utc::now().to_rfc3339());
            }
            if r.get("is_managed").is_none() {
                let target = r["symlink_target"].as_str();
                let canonical = skills
                    .get(field(r, "skill_id"))
                    .and_then(|s| s["canonical_path"].as_str());
                r["is_managed"] = json!(
                    field(r, "link_type") == "symlink" && target.is_some() && target == canonical
                );
            }
        }
        Ok(())
    })
    .await
}

pub async fn init_database(pool: &AppStore) -> Result<(), String> {
    init_schema(pool).await?;
    let next = pool.read(|d| {
        Ok(["skill_installations", "agent_skill_observations"]
            .iter()
            .flat_map(|t| d.table(t).values())
            .filter_map(|r| {
                field(r, "agent_id")
                    .strip_prefix("project:")?
                    .parse::<i64>()
                    .ok()?
                    .checked_add(1)
            })
            .max()
            .unwrap_or(1))
    })?;
    update_config(pool, |c| {
        c.next_scan_directory_id = c.next_scan_directory_id.max(next);
        Ok(())
    })?;
    seed_builtin_agents(pool).await?;
    seed_builtin_scan_directories(pool).await
}

pub async fn prepare_config_dir(config_dir: &Path) -> Result<(), String> {
    std::fs::create_dir_all(config_dir).map_err(|e| e.to_string())?;
    let pool = AppStore::open(config_dir)?;
    initialize_config(&pool)?;
    crate::platform_icons::seed(config_dir)?;
    init_schema(&pool).await
}

async fn seed_builtin_agents(pool: &AppStore) -> Result<(), String> {
    let definitions = crate::platforms::load_platform_definitions()?;
    let agents = crate::platforms::agents_from_definitions(&definitions);
    let sync_existing = crate::platforms::platform_dir().is_some();
    seed_builtin_agents_from(pool, &agents, sync_existing).await
}

async fn seed_builtin_agents_from(
    pool: &AppStore,
    agents: &[Agent],
    sync_existing: bool,
) -> Result<(), String> {
    pool.write(|d| {
        for a in agents {
            let old = one::<Agent>(d, "agents", &a.id)?;
            if old.is_none() || sync_existing {
                let mut a = a.clone();
                a.is_detected = old.map(|r| r.is_detected).unwrap_or(false);
                put(d, "agents", a.id.clone(), &a)?;
            }
        }
        if sync_existing {
            d.table_mut("agents")
                .retain(|id, _| id.starts_with("project:") || agents.iter().any(|a| a.id == *id));
        }
        Ok(())
    })
    .await
}

async fn seed_builtin_scan_directories(pool: &AppStore) -> Result<(), String> {
    let agents = get_all_agents(pool).await?;
    crate::db::update_config(pool, |config| {
        let paths: HashSet<_> = agents
            .iter()
            .filter(|agent| agent.is_builtin)
            .map(|agent| agent.global_skills_dir.as_str())
            .collect();
        config
            .scan_directories
            .retain(|dir| !dir.is_builtin || paths.contains(dir.path.as_str()));
        for agent in agents.iter().filter(|agent| agent.is_builtin) {
            if !config
                .scan_directories
                .iter()
                .any(|dir| dir.path == agent.global_skills_dir)
            {
                let id = next_scan_directory_id(config);
                config.scan_directories.push(ScanDirectory {
                    id,
                    path: agent.global_skills_dir.clone(),
                    label: Some(agent.display_name.clone()),
                    is_active: true,
                    is_builtin: true,
                    added_at: Utc::now().to_rfc3339(),
                });
            }
        }
        Ok(())
    })
}

pub async fn upsert_skill(pool: &AppStore, skill: &Skill) -> Result<(), String> {
    pool.write(|d| upsert_skill_in(d, skill)).await
}

pub(crate) async fn upsert_skill_with_executor(d: &mut Data, skill: &Skill) -> Result<(), String> {
    upsert_skill_in(d, skill)
}
pub fn upsert_skill_in(d: &mut Data, skill: &Skill) -> Result<(), String> {
    let mut next = skill.clone();
    if let Some(old) = one::<Skill>(d, "skills", &skill.id)? {
        if old.is_central && !skill.is_central {
            next = old;
            next.scanned_at = skill.scanned_at.clone();
        } else if next.canonical_path.is_none() {
            next.canonical_path = old.canonical_path;
        }
    }
    put(d, "skills", skill.id.clone(), &next)
}

pub async fn repair_legacy_centralized_resource_skill(
    pool: &AppStore,
    skill: &Skill,
    legacy_central_path: &str,
) -> Result<(), String> {
    pool.write(|d| {
        if d.table("skills").contains_key(&skill.id) {
            let mut s = skill.clone();
            s.is_central = false;
            put(d, "skills", s.id.clone(), &s)?;
        }
        d.table_mut("skill_installations").retain(|_, r| {
            !(field(r, "skill_id") == skill.id
                && field(r, "installed_path") == legacy_central_path
                && field(r, "link_type") == "symlink")
        });
        Ok(())
    })
    .await
}

pub async fn get_skills_by_agent(pool: &AppStore, agent_id: &str) -> Result<Vec<Skill>, String> {
    pool.read(|d| {
        d.table("skill_installations")
            .values()
            .filter(|r| field(r, "agent_id") == agent_id && managed(r))
            .filter_map(|r| d.table("skills").get(field(r, "skill_id")))
            .map(|r| Skill::deserialize(r).map_err(|e| e.to_string()))
            .collect()
    })
}

pub async fn get_skills_for_agent(
    pool: &AppStore,
    agent_id: &str,
) -> Result<Vec<SkillForAgent>, String> {
    // Validate the same fields as the owned DTOs, but borrow strings that are
    // not returned (notably the potentially large Markdown content).
    #[derive(Deserialize)]
    struct SkillView<'a> {
        id: &'a str,
        name: &'a str,
        description: Option<&'a str>,
        file_path: &'a str,
        #[serde(rename = "canonical_path")]
        _canonical_path: Option<&'a str>,
        is_central: bool,
        source: Option<&'a str>,
        #[serde(rename = "content")]
        _content: Option<&'a str>,
        #[serde(rename = "scanned_at")]
        _scanned_at: &'a str,
    }
    #[derive(Deserialize)]
    struct InstallationView<'a> {
        #[serde(rename = "skill_id")]
        _skill_id: &'a str,
        #[serde(rename = "agent_id")]
        _agent_id: &'a str,
        installed_path: &'a str,
        link_type: &'a str,
        symlink_target: Option<&'a str>,
        created_at: &'a str,
    }
    pool.read(|d| {
        let skills = d.table("skills");
        let sources = d.table("skill_sources");
        let mut out = Vec::new();
        for i in d
            .table("skill_installations")
            .values()
            .filter(|r| field(r, "agent_id") == agent_id && managed(r))
        {
            let id = field(i, "skill_id");
            if let Some(raw) = skills.get(id) {
                let skill: SkillView = serde::Deserialize::deserialize(raw)
                    .map_err(|e: serde_json::Error| e.to_string())?;
                let install: InstallationView = serde::Deserialize::deserialize(i)
                    .map_err(|e: serde_json::Error| e.to_string())?;
                let source = sources.get(id);
                let source_field =
                    |key: &str| source.and_then(|s| s[key].as_str()).map(str::to_owned);
                out.push(SkillForAgent {
                    installation_source: "independent".into(),
                    row_id: skill.id.to_owned(),
                    id: skill.id.to_owned(),
                    name: skill.name.to_owned(),
                    description: skill.description.map(str::to_owned),
                    file_path: skill.file_path.to_owned(),
                    dir_path: install.installed_path.to_owned(),
                    link_type: install.link_type.to_owned(),
                    symlink_target: install.symlink_target.map(str::to_owned),
                    is_central: skill.is_central,
                    source: skill.source.map(str::to_owned),
                    source_kind: None,
                    source_root: None,
                    is_read_only: false,
                    conflict_group: None,
                    conflict_count: 0,
                    source_url: source_field("source_url"),
                    source_author: source_field("source_author"),
                    source_repo: source_field("source_repo"),
                    source_path: source_field("source_path"),
                    created_at: Some(install.created_at.to_owned()),
                });
            }
        }
        Ok(out)
    })
}

pub async fn upsert_agent_skill_observation(
    pool: &AppStore,
    observation: &AgentSkillObservation,
) -> Result<(), String> {
    pool.write(|d| {
        put(
            d,
            "agent_skill_observations",
            observation.row_id.clone(),
            observation,
        )
    })
    .await
}

pub async fn get_agent_skill_observations(
    pool: &AppStore,
    agent_id: &str,
) -> Result<Vec<AgentSkillObservation>, String> {
    pool.read(|d| {
        let mut v: Vec<_> = rows::<AgentSkillObservation>(d, "agent_skill_observations")?
            .into_iter()
            .filter(|r| r.agent_id == agent_id)
            .collect();
        v.sort_by(|a, b| a.name.cmp(&b.name).then(a.dir_path.cmp(&b.dir_path)));
        Ok(v)
    })
}

pub async fn get_central_skills(pool: &AppStore) -> Result<Vec<Skill>, String> {
    pool.read(|d| {
        Ok(rows::<Skill>(d, "skills")?
            .into_iter()
            .filter(|s| s.is_central)
            .collect())
    })
}

pub fn default_skill_resource_library_dir() -> std::path::PathBuf {
    app_data_dir().join("library")
}

pub async fn get_skill_resource_library_dir(pool: &AppStore) -> Result<std::path::PathBuf, String> {
    Ok(get_setting(pool, SKILL_RESOURCE_LIBRARY_DIR_SETTING_KEY)
        .await?
        .map(|path| expand_home_path(&path))
        .unwrap_or_else(default_skill_resource_library_dir))
}

pub async fn set_skill_resource_library_dir(
    pool: &AppStore,
    path: &str,
) -> Result<std::path::PathBuf, String> {
    let expanded = expand_home_path(path.trim());
    if expanded.as_os_str().is_empty() {
        return Err("Skill resource library path cannot be empty".to_string());
    }
    std::fs::create_dir_all(&expanded)
        .map_err(|e| format!("Failed to create Skill Resource Library directory: {}", e))?;
    set_setting(
        pool,
        SKILL_RESOURCE_LIBRARY_DIR_SETTING_KEY,
        &path_to_string(&expanded),
    )
    .await?;
    Ok(expanded)
}

pub async fn get_resource_library_skills(pool: &AppStore) -> Result<Vec<Skill>, String> {
    pool.read(|d| {
        let mut v: Vec<_> = rows::<Skill>(d, "skills")?
            .into_iter()
            .filter(|s| !s.is_central && s.canonical_path.is_some())
            .collect();
        v.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(v)
    })
}

pub async fn get_skill_by_id(pool: &AppStore, skill_id: &str) -> Result<Option<Skill>, String> {
    pool.read(|d| one::<Skill>(d, "skills", skill_id))
}

pub async fn upsert_skill_source(pool: &AppStore, source: &SkillSource) -> Result<(), String> {
    pool.write(|d| upsert_skill_source_in(d, source)).await
}

pub(crate) async fn upsert_skill_source_with_executor(
    d: &mut Data,
    source: &SkillSource,
) -> Result<(), String> {
    upsert_skill_source_in(d, source)
}
pub fn upsert_skill_source_in(d: &mut Data, source: &SkillSource) -> Result<(), String> {
    put(d, "skill_sources", source.skill_id.clone(), source)
}

pub async fn get_skill_source(
    pool: &AppStore,
    skill_id: &str,
) -> Result<Option<SkillSource>, String> {
    pool.read(|d| one::<SkillSource>(d, "skill_sources", skill_id))
}

pub async fn get_all_skill_sources(pool: &AppStore) -> Result<Vec<SkillSource>, String> {
    pool.read(|d| rows(d, "skill_sources"))
}

pub async fn upsert_skill_source_sync(pool: &AppStore, sync: &SkillSourceSync) -> Result<(), String> {
    pool.write(|d| put(d, "skill_source_syncs", sync.skill_id.clone(), sync))
        .await
}

pub async fn record_skill_check_times(
    pool: &AppStore,
    skill_ids: &[String],
    checked_at: &str,
) -> Result<(), String> {
    pool.write(|d|{for id in skill_ids{let r=d.table_mut("skill_source_syncs").entry(id.clone()).or_insert_with(||json!({"skill_id":id,"sync_scope":"skill","remote_ref":null,"skill_fingerprint":null,"last_checked_at":null,"last_sync_at":null,"sync_status":"never","sync_error":null,"remote_deleted":false,"updated_at":checked_at}));r["last_checked_at"]=json!(checked_at);r["updated_at"]=json!(checked_at);}Ok(())}).await
}

pub async fn get_skill_source_sync(
    pool: &AppStore,
    skill_id: &str,
) -> Result<Option<SkillSourceSync>, String> {
    pool.read(|d| one::<SkillSourceSync>(d, "skill_source_syncs", skill_id))
}

pub async fn get_all_skill_source_syncs(pool: &AppStore) -> Result<Vec<SkillSourceSync>, String> {
    pool.read(|d| rows(d, "skill_source_syncs"))
}

pub async fn get_skill_metadata(
    pool: &AppStore,
    skill_id: &str,
) -> Result<Option<SkillMetadata>, String> {
    pool.read(|d| one::<SkillMetadata>(d, "skill_metadata", skill_id))
}

pub async fn upsert_skill_metadata(
    pool: &AppStore,
    skill_id: &str,
    notes: Option<&str>,
    tags: &[String],
) -> Result<SkillMetadata, String> {
    let row = SkillMetadata {
        skill_id: skill_id.into(),
        notes: notes.map(str::to_owned),
        tags: serde_json::to_string(tags).map_err(|e| e.to_string())?,
        updated_at: Utc::now().to_rfc3339(),
    };
    pool.write(|d| put(d, "skill_metadata", skill_id.into(), &row))
        .await?;
    Ok(row)
}

pub fn parse_skill_metadata_tags(metadata: Option<&SkillMetadata>) -> Vec<String> {
    metadata
        .and_then(|metadata| serde_json::from_str::<Vec<String>>(&metadata.tags).ok())
        .unwrap_or_default()
}

pub async fn delete_skill(pool: &AppStore, skill_id: &str) -> Result<(), String> {
    pool.write(|d| {
        for t in [
            "skill_installations",
            "skill_source_syncs",
            "skill_sources",
            "skill_metadata",
        ] {
            d.table_mut(t)
                .retain(|_, r| field(r, "skill_id") != skill_id);
        }
        d.table_mut("skills").remove(skill_id);
        Ok(())
    })
    .await
}

pub async fn delete_central_skill_records(
    pool: &AppStore,
    skill_id: &str,
    _skill_name: &str,
) -> Result<(), String> {
    pool.write(|d| {
        for t in [
            "skill_installations",
            "skill_source_syncs",
            "skill_sources",
            "skill_metadata",
            "collection_skills",
            "skill_explanations",
        ] {
            d.table_mut(t)
                .retain(|_, r| field(r, "skill_id") != skill_id);
        }
        d.table_mut("skills").remove(skill_id);
        Ok(())
    })
    .await
}

pub async fn delete_skill_owned_records(
    pool: &AppStore,
    skill_id: &str,
    _skill_name: &str,
) -> Result<(), String> {
    pool.write(|d| {
        for t in [
            "skill_installations",
            "skill_source_syncs",
            "skill_sources",
            "skill_metadata",
            "collection_skills",
            "skill_explanations",
        ] {
            d.table_mut(t)
                .retain(|_, r| field(r, "skill_id") != skill_id);
        }
        d.table_mut("skills").remove(skill_id);
        Ok(())
    })
    .await
}

pub async fn upsert_skill_installation(
    pool: &AppStore,
    installation: &SkillInstallation,
) -> Result<(), String> {
    pool.write(|d| {
        let key = key2(&installation.skill_id, &installation.agent_id);
        let old = d.table("skill_installations").get(&key).cloned();

        let mut v = serde_json::to_value(installation).map_err(|e| e.to_string())?;
        v["is_managed"] = json!(true);
        if let Some(old) = old {
            v["created_at"] = old["created_at"].clone();
        }
        d.table_mut("skill_installations").insert(key, v);
        Ok(())
    })
    .await
}

pub async fn upsert_scanned_skill_installation(
    pool: &AppStore,
    installation: &SkillInstallation,
) -> Result<(), String> {
    pool.write(|d| upsert_scanned_installation_in(d, installation))
        .await
}
fn upsert_scanned_installation_in(
    d: &mut Data,
    installation: &SkillInstallation,
) -> Result<(), String> {
    let key = key2(&installation.skill_id, &installation.agent_id);
    let old = d.table("skill_installations").get(&key);
    if old.is_some_and(managed) {
        return Ok(());
    }
    let mut value = serde_json::to_value(installation).map_err(|e| e.to_string())?;
    value["is_managed"] = json!(false);
    if let Some(old) = old {
        value["created_at"] = old["created_at"].clone();
    }
    d.table_mut("skill_installations").insert(key, value);
    Ok(())
}
/// Persist a completed scan scope together, keeping all user-owned fields intact.
pub async fn commit_scan_batch(
    pool: &AppStore,
    skills: &[Skill],
    installations: &[SkillInstallation],
    observations: &[AgentSkillObservation],
) -> Result<(), String> {
    pool.write(|d| {
        for s in skills {
            upsert_skill_in(d, s)?;
        }
        for i in installations {
            upsert_scanned_installation_in(d, i)?;
        }
        for o in observations {
            put(d, "agent_skill_observations", o.row_id.clone(), o)?;
        }
        Ok(())
    })
    .await
}

pub async fn delete_skill_installation(
    pool: &AppStore,
    skill_id: &str,
    agent_id: &str,
) -> Result<(), String> {
    pool.write(|d| {
        d.table_mut("skill_installations")
            .remove(&key2(skill_id, agent_id));
        Ok(())
    })
    .await
}

pub async fn delete_skill_installations_by_agent(
    pool: &AppStore,
    agent_id: &str,
) -> Result<(), String> {
    pool.write(|d| {
        d.table_mut("skill_installations")
            .retain(|_, r| field(r, "agent_id") != agent_id);
        Ok(())
    })
    .await
}

pub async fn delete_stale_skill_installations(
    pool: &AppStore,
    agent_id: &str,
    found_skill_ids: &[String],
) -> Result<(), String> {
    pool.write(|d|{let mut stale=Vec::new();for(k,r)in d.table("skill_installations"){if field(r,"agent_id")!=agent_id{continue;}let path=field(r,"installed_path");let id=field(r,"skill_id");let legacy=d.table("skills").get(id).is_some_and(|s|s["is_central"]==true&&field(s,"canonical_path")==path)&&field(r,"link_type")=="symlink"&&r["symlink_target"].as_str().is_some_and(|target|std::fs::symlink_metadata(path).map(|m|m.file_type().is_symlink()).unwrap_or(false)&&matches!((std::fs::canonicalize(path),std::fs::canonicalize(target)),(Ok(a),Ok(b)) if a==b));let remove=if managed(r){matches!(std::fs::symlink_metadata(path),Err(e) if e.kind()==std::io::ErrorKind::NotFound)}else{!found_skill_ids.iter().any(|s|s==id)&&!legacy};if remove{stale.push(k.clone());}}for k in stale{d.table_mut("skill_installations").remove(&k);}Ok(())}).await
}

pub async fn delete_stale_agent_skill_observations(
    pool: &AppStore,
    agent_id: &str,
    found_row_ids: &[String],
) -> Result<(), String> {
    pool.write(|d| {
        d.table_mut("agent_skill_observations")
            .retain(|id, r| field(r, "agent_id") != agent_id || found_row_ids.contains(id));
        Ok(())
    })
    .await
}

pub async fn delete_skills_not_in_scope(
    pool: &AppStore,
    found_skill_ids: &[String],
) -> Result<(), String> {
    pool.write(|d| {
        for t in [
            "skill_installations",
            "skill_source_syncs",
            "skill_sources",
            "skill_metadata",
        ] {
            d.table_mut(t)
                .retain(|_, r| found_skill_ids.iter().any(|id| id == field(r, "skill_id")));
        }
        d.table_mut("skills")
            .retain(|id, _| found_skill_ids.contains(id));
        Ok(())
    })
    .await
}

pub async fn get_skill_installations(
    pool: &AppStore,
    skill_id: &str,
) -> Result<Vec<SkillInstallation>, String> {
    pool.read(|d| {
        d.table("skill_installations")
            .values()
            .filter(|r| field(r, "skill_id") == skill_id && managed(r))
            .cloned()
            .map(|v| serde_json::from_value(v).map_err(|e| e.to_string()))
            .collect()
    })
}

pub(crate) async fn get_skill_installations_for_legacy_repair(
    pool: &AppStore,
    skill_id: &str,
) -> Result<Vec<SkillInstallation>, String> {
    pool.read(|d| {
        d.table("skill_installations")
            .values()
            .filter(|r| field(r, "skill_id") == skill_id)
            .cloned()
            .map(|v| serde_json::from_value(v).map_err(|e| e.to_string()))
            .collect()
    })
}

pub async fn count_managed_skill_installations(
    pool: &AppStore,
    agent_id: &str,
) -> Result<i64, String> {
    pool.read(|d| {
        Ok(d.table("skill_installations")
            .values()
            .filter(|r| field(r, "agent_id") == agent_id && managed(r))
            .count() as i64)
    })
}

pub async fn get_managed_skill_ids(pool: &AppStore) -> Result<Vec<String>, String> {
    pool.read(|d| {
        Ok(d.table("skill_installations")
            .values()
            .filter(|r| managed(r))
            .map(|r| field(r, "skill_id").to_owned())
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .collect())
    })
}

pub async fn get_read_only_observed_agent_ids_for_skill(
    pool: &AppStore,
    skill_id: &str,
) -> Result<Vec<String>, String> {
    pool.read(|d| {
        Ok(
            rows::<AgentSkillObservation>(d, "agent_skill_observations")?
                .into_iter()
                .filter(|r| {
                    r.skill_id == skill_id && r.is_read_only && r.source_kind == "compatibility"
                })
                .map(|r| r.agent_id)
                .collect::<std::collections::BTreeSet<_>>()
                .into_iter()
                .collect(),
        )
    })
}

pub async fn get_all_agents(pool: &AppStore) -> Result<Vec<Agent>, String> {
    pool.read(|d| {
        let mut v = rows::<Agent>(d, "agents")?;
        v.sort_by(|a, b| {
            b.is_builtin
                .cmp(&a.is_builtin)
                .then(a.display_name.cmp(&b.display_name))
        });
        Ok(v)
    })
}

pub async fn get_agent_by_id(pool: &AppStore, agent_id: &str) -> Result<Option<Agent>, String> {
    pool.read(|d| one::<Agent>(d, "agents", agent_id))
}

pub async fn update_agent_detected(
    pool: &AppStore,
    agent_id: &str,
    is_detected: bool,
) -> Result<(), String> {
    pool.write(|d| {
        if let Some(r) = d.table_mut("agents").get_mut(agent_id) {
            r["is_detected"] = json!(is_detected);
        }
        Ok(())
    })
    .await
}

pub async fn insert_custom_agent(pool: &AppStore, agent: &Agent) -> Result<(), String> {
    pool.write(|d| {
        if d.table("agents").contains_key(&agent.id) {
            return Err("Agent already exists".into());
        }
        let mut a = agent.clone();
        a.is_builtin = false;
        a.is_enabled = true;
        put(d, "agents", a.id.clone(), &a)
    })
    .await?;
    crate::platforms::persist_platform_edit(agent)?;
    Ok(())
}

pub async fn delete_custom_agent(pool: &AppStore, agent_id: &str) -> Result<(), String> {
    let agent = get_agent_by_id(pool, agent_id)
        .await?
        .ok_or_else(|| format!("Agent '{}' not found", agent_id))?;
    pool.write(|d| {
        d.table_mut("agents").remove(agent_id);
        Ok(())
    })
    .await?;
    crate::platforms::persist_platform_delete(&agent.id)?;
    Ok(())
}

pub async fn update_custom_agent(
    pool: &AppStore,
    agent_id: &str,
    next_agent_id: &str,
    display_name: &str,
    global_skills_dir: &str,
) -> Result<Agent, String> {
    if next_agent_id.trim().is_empty() {
        return Err("Agent ID cannot be empty".into());
    }
    let updated = pool
        .write(|d| {
            let mut a = one::<Agent>(d, "agents", agent_id)?
                .ok_or_else(|| format!("Agent '{}' not found", agent_id))?;
            if next_agent_id != agent_id && d.table("agents").contains_key(next_agent_id) {
                return Err(format!("Agent ID '{}' already exists", next_agent_id));
            }
            a.id = next_agent_id.into();
            a.display_name = display_name.into();
            a.global_skills_dir = global_skills_dir.into();
            d.table_mut("agents").remove(agent_id);
            put(d, "agents", a.id.clone(), &a)?;
            if next_agent_id != agent_id {
                let updates: Vec<_> = d
                    .table("skill_installations")
                    .iter()
                    .filter(|(_, r)| field(r, "agent_id") == agent_id)
                    .map(|(k, r)| (k.clone(), r.clone()))
                    .collect();
                for (k, mut r) in updates {
                    d.table_mut("skill_installations").remove(&k);
                    r["agent_id"] = json!(next_agent_id);
                    d.table_mut("skill_installations")
                        .insert(key2(field(&r, "skill_id"), next_agent_id), r);
                }
                for r in d.table_mut("agent_skill_observations").values_mut() {
                    if field(r, "agent_id") == agent_id {
                        r["agent_id"] = json!(next_agent_id);
                    }
                }
            }
            Ok(a)
        })
        .await?;
    if next_agent_id != agent_id {
        crate::platforms::persist_platform_rename(agent_id, &updated)?;
    } else {
        crate::platforms::persist_platform_edit(&updated)?;
    }
    Ok(updated)
}

pub async fn set_agent_enabled(
    pool: &AppStore,
    agent_id: &str,
    enabled: bool,
) -> Result<Agent, String> {
    if agent_id == "central" || agent_id.starts_with("project:") {
        return Err("This target cannot be enabled or disabled here".into());
    }
    let updated = pool
        .write(|d| {
            let mut a = one::<Agent>(d, "agents", agent_id)?
                .ok_or_else(|| format!("Agent '{}' not found", agent_id))?;
            a.is_enabled = enabled;
            put(d, "agents", a.id.clone(), &a)?;
            Ok(a)
        })
        .await?;
    crate::platforms::persist_platform_edit(&updated)?;
    Ok(updated)
}

pub async fn update_central_agent_skills_dir(
    pool: &AppStore,
    global_skills_dir: &str,
) -> Result<Agent, String> {
    let updated = pool
        .write(|d| {
            let mut a = one::<Agent>(d, "agents", "central")?.ok_or("Central agent not found")?;
            a.global_skills_dir = global_skills_dir.into();
            put(d, "agents", a.id.clone(), &a)?;
            Ok(a)
        })
        .await?;
    crate::platforms::persist_platform_edit(&updated)?;
    Ok(updated)
}

pub async fn create_collection(
    pool: &AppStore,
    name: &str,
    description: Option<&str>,
) -> Result<Collection, String> {
    let now = Utc::now().to_rfc3339();
    let row = Collection {
        id: Uuid::new_v4().to_string(),
        parent_id: None,
        sort_order: Utc::now().timestamp_millis(),
        name: name.into(),
        description: description.map(str::to_owned),
        created_at: now.clone(),
        updated_at: now,
    };
    pool.write(|d| put(d, "collections", row.id.clone(), &row))
        .await?;
    Ok(row)
}

pub async fn get_all_collections(pool: &AppStore) -> Result<Vec<Collection>, String> {
    pool.read(|d| {
        let mut v = rows::<Collection>(d, "collections")?;
        v.sort_by(|a, b| a.sort_order.cmp(&b.sort_order).then(a.created_at.cmp(&b.created_at)));
        Ok(v)
    })
}

pub async fn get_collection_by_id(
    pool: &AppStore,
    collection_id: &str,
) -> Result<Option<Collection>, String> {
    pool.read(|d| one::<Collection>(d, "collections", collection_id))
}

pub async fn update_collection(
    pool: &AppStore,
    collection_id: &str,
    name: &str,
    description: Option<&str>,
) -> Result<(), String> {
    pool.write(|d| {
        if let Some(r) = d.table_mut("collections").get_mut(collection_id) {
            r["name"] = json!(name);
            r["description"] = json!(description);
            r["updated_at"] = json!(Utc::now().to_rfc3339());
        }
        Ok(())
    })
    .await
}

pub async fn delete_collection(pool: &AppStore, collection_id: &str) -> Result<(), String> {
    collection_tree::change(pool, collection_tree::CollectionChange {action:"delete".into(),id:Some(collection_id.into()),name:None,parent_id:None,before_id:None,skill_ids:None,repositories:None,source_collection_id:None,include_descendants:None}).await.map(|_|())
}

pub async fn add_skill_to_collection(
    pool: &AppStore,
    collection_id: &str,
    skill_id: &str,
) -> Result<(), String> {
    pool.write(|d|{d.table_mut("collection_skills").entry(key2(collection_id,skill_id)).or_insert_with(||json!({"collection_id":collection_id,"skill_id":skill_id,"added_at":Utc::now().to_rfc3339()}));Ok(())}).await
}

pub async fn remove_skill_from_collection(
    pool: &AppStore,
    collection_id: &str,
    skill_id: &str,
) -> Result<(), String> {
    pool.write(|d| {
        d.table_mut("collection_skills")
            .remove(&key2(collection_id, skill_id));
        Ok(())
    })
    .await
}

pub async fn get_collection_skills(
    pool: &AppStore,
    collection_id: &str,
) -> Result<Vec<Skill>, String> {
    pool.read(|d| {
        let mut members: Vec<_> = d
            .table("collection_skills")
            .values()
            .filter(|r| field(r, "collection_id") == collection_id)
            .collect();
        members.sort_by_key(|r| field(r, "added_at"));
        members
            .into_iter()
            .filter_map(|r| d.table("skills").get(field(r, "skill_id")))
            .map(|r| Skill::deserialize(r).map_err(|e| e.to_string()))
            .collect()
    })
}

pub async fn get_skill_collections(
    pool: &AppStore,
    skill_id: &str,
) -> Result<Vec<Collection>, String> {
    pool.read(|d| {
        let mut members: Vec<_> = d
            .table("collection_skills")
            .values()
            .filter(|r| field(r, "skill_id") == skill_id)
            .collect();
        members.sort_by_key(|r| field(r, "added_at"));
        members
            .into_iter()
            .filter_map(|r| d.table("collections").get(field(r, "collection_id")))
            .map(|r| Collection::deserialize(r).map_err(|e| e.to_string()))
            .collect()
    })
}

pub async fn get_scan_directories(pool: &AppStore) -> Result<Vec<ScanDirectory>, String> {
    let mut dirs = crate::db::load_config(pool)?.scan_directories;
    dirs.sort_by(|a, b| {
        b.is_builtin
            .cmp(&a.is_builtin)
            .then(a.added_at.cmp(&b.added_at))
    });
    Ok(dirs)
}

pub async fn get_scan_directory_by_id(
    pool: &AppStore,
    id: i64,
) -> Result<Option<ScanDirectory>, String> {
    Ok(get_scan_directories(pool)
        .await?
        .into_iter()
        .find(|dir| dir.id == id))
}

fn next_scan_directory_id(config: &mut crate::config_store::AppConfig) -> i64 {
    let id = config.next_scan_directory_id.max(1).max(
        config
            .scan_directories
            .iter()
            .map(|dir| dir.id + 1)
            .max()
            .unwrap_or(1),
    );
    config.next_scan_directory_id = id + 1;
    id
}

pub async fn add_scan_directory(
    pool: &AppStore,
    path: &str,
    label: Option<&str>,
) -> Result<ScanDirectory, String> {
    crate::db::update_config(pool, |config| {
        if config.scan_directories.iter().any(|dir| dir.path == path) {
            return Err("Scan directory already exists".into());
        }
        let dir = ScanDirectory {
            id: next_scan_directory_id(config),
            path: path.into(),
            label: label.map(str::to_string),
            is_active: true,
            is_builtin: false,
            added_at: Utc::now().to_rfc3339(),
        };
        config.scan_directories.push(dir.clone());
        Ok(dir)
    })
}

pub async fn update_scan_directory(
    pool: &AppStore,
    path: &str,
    new_path: &str,
    label: Option<&str>,
) -> Result<ScanDirectory, String> {
    if new_path.trim().is_empty() {
        return Err("Scan directory path cannot be empty".into());
    }
    crate::db::update_config(pool, |config| {
        if config
            .scan_directories
            .iter()
            .any(|dir| dir.path == new_path && dir.path != path)
        {
            return Err("Scan directory already exists".into());
        }
        let dir = config
            .scan_directories
            .iter_mut()
            .find(|dir| dir.path == path)
            .ok_or("Scan directory not found")?;
        if dir.is_builtin {
            return Err("Cannot update built-in scan directory".into());
        }
        dir.path = new_path.into();
        dir.label = label.map(str::to_string);
        Ok(dir.clone())
    })
}

pub async fn remove_scan_directory(pool: &AppStore, path: &str) -> Result<(), String> {
    let dir = get_scan_directories(pool)
        .await?
        .into_iter()
        .find(|dir| dir.path == path)
        .ok_or("Scan directory not found")?;
    if dir.is_builtin {
        return Err("Cannot remove built-in scan directory".into());
    }
    crate::db::update_config(pool, |config| {
        config.scan_directories.retain(|entry| entry.id != dir.id);
        Ok(())
    })?;
    delete_skill_installations_by_agent(pool, &format!("project:{}", dir.id)).await
}

pub async fn toggle_scan_directory(
    pool: &AppStore,
    path: &str,
    is_active: bool,
) -> Result<(), String> {
    crate::db::update_config(pool, |config| {
        let dir = config
            .scan_directories
            .iter_mut()
            .find(|dir| dir.path == path)
            .ok_or("Scan directory not found")?;
        dir.is_active = is_active;
        Ok(())
    })
}

pub async fn upsert_builtin_scan_directory(
    pool: &AppStore,
    path: &str,
    label: &str,
) -> Result<(), String> {
    crate::db::update_config(pool, |config| {
        if let Some(dir) = config
            .scan_directories
            .iter_mut()
            .find(|dir| dir.path == path)
        {
            dir.label = Some(label.into());
            dir.is_builtin = true;
            dir.is_active = true;
        } else {
            let id = next_scan_directory_id(config);
            config.scan_directories.push(ScanDirectory {
                id,
                path: path.into(),
                label: Some(label.into()),
                is_active: true,
                is_builtin: true,
                added_at: Utc::now().to_rfc3339(),
            });
        }
        Ok(())
    })
}

pub async fn get_setting(pool: &AppStore, key: &str) -> Result<Option<String>, String> {
    Ok(crate::db::load_config(pool)?.settings.get(key).cloned())
}

pub async fn set_setting(pool: &AppStore, key: &str, value: &str) -> Result<(), String> {
    crate::db::update_config(pool, |config| {
        config.settings.insert(key.into(), value.into());
        Ok(())
    })
}

pub async fn save_github_stars(pool: &AppStore, repository: &str, stars: i64) -> Result<(), String> {
    pool.write(|d| save_github_stars_in(d, repository, stars))
        .await
}

pub(crate) async fn save_github_stars_with_executor(
    d: &mut Data,
    repository: &str,
    stars: i64,
) -> Result<(), String> {
    save_github_stars_in(d, repository, stars)
}
pub fn save_github_stars_in(d: &mut Data, repository: &str, stars: i64) -> Result<(), String> {
    if stars < 0 {
        return Err("Stars must be nonnegative".into());
    }
    let key = repository.to_ascii_lowercase();
    let spelling = d
        .table("github_repository_stats")
        .get(&key)
        .and_then(|r| r["repository"].as_str())
        .unwrap_or(repository)
        .to_owned();
    put(
        d,
        "github_repository_stats",
        key,
        &json!({"repository":spelling,"stars":stars,"checked_at":Utc::now().to_rfc3339()}),
    )
}

pub async fn get_github_stars(pool: &AppStore, repository: &str) -> Result<Option<i64>, String> {
    pool.read(|d| {
        Ok(d.table("github_repository_stats")
            .get(&repository.to_ascii_lowercase())
            .and_then(|r| r["stars"].as_i64()))
    })
}

pub async fn collection_member_ids(
    pool: &AppStore,
    collection_id: &str,
) -> Result<Vec<String>, String> {
    pool.read(|d| {
        Ok(d.table("collection_skills")
            .values()
            .filter(|r| field(r, "collection_id") == collection_id)
            .map(|r| field(r, "skill_id").to_owned())
            .collect())
    })
}

pub async fn managed_central_paths(pool: &AppStore) -> Result<Vec<(String, String)>, String> {
    pool.read(|d| {
        Ok(d.table("skill_installations")
            .values()
            .filter(|r| field(r, "agent_id") == "central" && managed(r))
            .map(|r| {
                (
                    field(r, "skill_id").to_owned(),
                    field(r, "installed_path").to_owned(),
                )
            })
            .collect())
    })
}

pub async fn has_pending_address_moves(pool: &AppStore) -> Result<bool, String> {
    pool.read(|d| Ok(!d.table("repository_address_moves").is_empty()))
}
#[cfg(test)]
#[path = "db/tests.rs"]
mod tests_file;
