use super::{AppStore, Collection};
use serde::{Deserialize, Serialize};
use serde_json::json;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CollectionSkillBackup {
    pub collection_id: String,
    pub skill_id: String,
    pub added_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepositoryStatsBackup {
    pub repository: String,
    pub stars: i64,
    pub checked_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepositoryDirectoryBackup {
    pub repository: String,
    pub directory: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SkillInstallationMethodRow {
    pub skill_id: String,
    pub agent_id: String,
    pub link_type: String,
}

pub fn memberships(pool: &AppStore) -> Result<Vec<CollectionSkillBackup>, String> {
    pool.read(|data| {
        let mut rows = super::rows::<CollectionSkillBackup>(data, "collection_skills")?;
        rows.sort_by(|a, b| (&a.collection_id, &a.added_at).cmp(&(&b.collection_id, &b.added_at)));
        Ok(rows)
    })
}
pub fn installations(pool: &AppStore) -> Result<Vec<SkillInstallationMethodRow>, String> {
    pool.read(|data| {
        let mut rows: Vec<SkillInstallationMethodRow> = data
            .table("skill_installations")
            .values()
            .filter(|v| v["is_managed"].as_bool() == Some(true))
            .map(|v| serde_json::from_value(v.clone()).map_err(|e| e.to_string()))
            .collect::<Result<_, _>>()?;
        rows.sort_by(|a, b| (&a.skill_id, &a.agent_id).cmp(&(&b.skill_id, &b.agent_id)));
        Ok(rows)
    })
}
pub fn stats(pool: &AppStore) -> Result<Vec<RepositoryStatsBackup>, String> {
    pool.read(|data| {
        let mut rows = super::rows::<RepositoryStatsBackup>(data, "github_repository_stats")?;
        rows.sort_by(|a, b| {
            a.repository
                .to_ascii_lowercase()
                .cmp(&b.repository.to_ascii_lowercase())
        });
        Ok(rows)
    })
}
pub fn directories(pool: &AppStore) -> Result<Vec<RepositoryDirectoryBackup>, String> {
    pool.read(|data| {
        let mut rows =
            super::rows::<RepositoryDirectoryBackup>(data, "repository_local_directories")?;
        rows.sort_by(|a, b| {
            a.repository
                .to_ascii_lowercase()
                .cmp(&b.repository.to_ascii_lowercase())
        });
        Ok(rows)
    })
}
pub fn matching_directories(
    pool: &AppStore,
    repo: &str,
    dir: &str,
) -> Result<Vec<(String, String)>, String> {
    Ok(directories(pool)?
        .into_iter()
        .filter(|r| {
            r.repository.eq_ignore_ascii_case(repo) || r.directory.eq_ignore_ascii_case(dir)
        })
        .map(|r| (r.repository, r.directory))
        .collect())
}
pub async fn restore_directories(
    pool: &AppStore,
    rows: &[RepositoryDirectoryBackup],
) -> Result<(), String> {
    pool.write(|data| {
        for row in rows {
            let key = row.repository.to_ascii_lowercase();
            // Repeat conflict checks under the writer lock.
            for value in data.table("repository_local_directories").values() {
                let r = value["repository"]
                    .as_str()
                    .ok_or("Invalid repository mapping")?;
                let d = value["directory"]
                    .as_str()
                    .ok_or("Invalid repository mapping")?;
                if (r.eq_ignore_ascii_case(&row.repository)
                    || d.eq_ignore_ascii_case(&row.directory))
                    && (!r.eq_ignore_ascii_case(&row.repository) || d != row.directory)
                {
                    return Err(
                        "Backup repository directory mapping conflicts with local data".into(),
                    );
                }
            }
            data.table_mut("repository_local_directories")
                .entry(key.clone())
                .or_insert(serde_json::to_value(row).map_err(|e| e.to_string())?);
            data.table_mut("repository_mirrors")
                .entry(key)
                .or_insert(json!({"repository":row.repository,"last_snapshot_id":null}));
        }
        Ok(())
    })
    .await
}
pub async fn restore_memberships(
    pool: &AppStore,
    rows: &[CollectionSkillBackup],
) -> Result<(), String> {
    pool.write(|data| {
        for row in rows {
            data.table_mut("collection_skills")
                .entry(super::key2(&row.collection_id, &row.skill_id))
                .or_insert(serde_json::to_value(row).map_err(|e| e.to_string())?);
        }
        Ok(())
    })
    .await
}
fn timestamp(value: &str) -> Option<chrono::DateTime<chrono::Utc>> {
    chrono::DateTime::parse_from_rfc3339(value)
        .ok()
        .map(|v| v.with_timezone(&chrono::Utc))
        .or_else(|| {
            [
                "%Y-%m-%d %H:%M:%S%.f",
                "%Y-%m-%dT%H:%M:%S%.f",
                "%Y-%m-%d %H:%M",
                "%Y-%m-%dT%H:%M",
            ]
            .iter()
            .find_map(|format| chrono::NaiveDateTime::parse_from_str(value, format).ok())
            .map(|v| v.and_utc())
        })
        .or_else(|| {
            chrono::NaiveDate::parse_from_str(value, "%Y-%m-%d")
                .ok()?
                .and_hms_opt(0, 0, 0)
                .map(|v| v.and_utc())
        })
}
pub async fn restore_stats(pool: &AppStore, rows: &[RepositoryStatsBackup]) -> Result<(), String> {
    pool.write(|data| {
        for row in rows {
            if row.stars < 0 {
                return Err("Invalid repository stars".into());
            }
            let key = row.repository.to_ascii_lowercase();
            let existing = data.table("github_repository_stats").get(&key);
            let replace = match existing {
                None => true,
                Some(old) => timestamp(&row.checked_at)
                    .zip(old["checked_at"].as_str().and_then(timestamp))
                    .is_some_and(|(new, old)| new >= old),
            };
            if replace {
                let mut row = row.clone();
                if let Some(name) = existing.and_then(|r| r["repository"].as_str()) {
                    row.repository = name.to_owned();
                }
                super::put(data, "github_repository_stats", key, &row)?;
            }
        }
        Ok(())
    })
    .await
}
pub async fn clear_source(pool: &AppStore, id: &str) -> Result<(), String> {
    pool.write(|data| {
        data.table_mut("skill_source_syncs").remove(id);
        data.table_mut("skill_sources").remove(id);
        Ok(())
    })
    .await
}
pub async fn restore_custom_agent(
    pool: &AppStore,
    id: &str,
    name: &str,
    detected: bool,
    enabled: bool,
) -> Result<(), String> {
    pool.write(|data| {
        if let Some(row) = data.table_mut("agents").get_mut(id) {
            if row["is_builtin"].as_bool() == Some(false) {
                row["display_name"] = json!(name);
                row["is_detected"] = json!(detected);
                row["is_enabled"] = json!(enabled);
            }
        }
        Ok(())
    })
    .await
}
pub async fn restore_collection(pool: &AppStore, collection: &Collection) -> Result<(), String> {
    pool.write(|data| {
        let created = data
            .table("collections")
            .get(&collection.id)
            .and_then(|r| r.get("created_at"))
            .cloned();
        let mut row = serde_json::to_value(collection).map_err(|e| e.to_string())?;
        if let Some(created) = created {
            row["created_at"] = created;
        }
        data.table_mut("collections")
            .insert(collection.id.clone(), row);
        Ok(())
    })
    .await
}

pub fn collection_details(
    pool: &AppStore,
    id: &str,
) -> Result<Vec<(super::Skill, Option<super::SkillSource>)>, String> {
    pool.read(|data| {
        let mut members = super::rows::<CollectionSkillBackup>(data, "collection_skills")?;
        members.retain(|row| row.collection_id == id);
        members.sort_by(|a, b| a.added_at.cmp(&b.added_at));
        members
            .into_iter()
            .filter_map(|member| {
                data.table("skills")
                    .get(&member.skill_id)
                    .map(|skill| (member, skill))
            })
            .map(|(member, skill)| {
                let skill = serde_json::from_value(skill.clone()).map_err(|e| e.to_string())?;
                let source = data
                    .table("skill_sources")
                    .get(&member.skill_id)
                    .map(|v| serde_json::from_value(v.clone()).map_err(|e| e.to_string()))
                    .transpose()?;
                Ok((skill, source))
            })
            .collect()
    })
}

#[cfg(test)]
mod equivalence_tests {
    use super::*;
    #[tokio::test]
    async fn stats_keep_original_spelling_and_ascii_nocase_order() {
        let pool = AppStore::memory().unwrap();
        super::super::save_github_stars(&pool, "a/repo", 1)
            .await
            .unwrap();
        super::super::save_github_stars(&pool, "B/repo", 2)
            .await
            .unwrap();
        super::super::save_github_stars(&pool, "A/REPO", 3)
            .await
            .unwrap();
        let result = stats(&pool).unwrap();
        assert_eq!(result[0].repository, "a/repo");
        assert_eq!(result[0].stars, 3);
        assert_eq!(result[1].repository, "B/repo");
    }
    #[tokio::test]
    async fn restore_accepts_sqlite_date_forms_without_overwriting_newer_stats() {
        let pool = AppStore::memory().unwrap();
        for (stamp, stars) in [
            ("2026-01-01", 1),
            ("2026-01-02 00:00:00.500", 2),
            ("2026-01-02T00:00:00.499Z", 3),
            ("invalid", 4),
        ] {
            restore_stats(
                &pool,
                &[RepositoryStatsBackup {
                    repository: if stars == 1 {
                        "Mixed/Repo"
                    } else {
                        "mixed/repo"
                    }
                    .into(),
                    stars,
                    checked_at: stamp.into(),
                }],
            )
            .await
            .unwrap();
        }
        let result = stats(&pool).unwrap();
        assert_eq!(result[0].stars, 2);
        assert_eq!(result[0].repository, "Mixed/Repo");
    }
}
