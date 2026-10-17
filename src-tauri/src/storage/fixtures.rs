//! Synthetic regression fixtures. Never used by the application.
use super::store::Store;
use serde_json::{json, Value};

pub fn key(table: &str, row: &Value) -> String {
    let text = |field: &str| row[field].as_str().unwrap_or_default();
    match table {
        "skill_installations" => crate::db::key2(text("skill_id"), text("agent_id")),
        "collection_skills" => crate::db::key2(text("collection_id"), text("skill_id")),
        "skill_explanations" => crate::db::key2(text("skill_id"), text("lang")),
        "skill_sources" | "skill_source_syncs" | "skill_metadata" => text("skill_id").into(),
        "agent_skill_observations" => text("row_id").into(),
        "repository_mirror_recovery" => text("snapshot_id").into(),
        "repository_notebooks" => row["id"].to_string(),
        "repository_mirrors" | "repository_local_directories" | "github_repository_stats" => {
            text("repository").to_ascii_lowercase()
        }
        _ => text("id").into(),
    }
}

pub async fn insert(pool: &Store, table: &str, row: Value) -> Result<(), String> {
    let now = chrono::Utc::now().to_rfc3339();
    let mut defaults = match table {
        "skills" => {
            json!({"description":null,"canonical_path":null,"is_central":false,"source":null,"content":null})
        }
        "skill_installations" => json!({"symlink_target":null,"is_managed":false,"created_at":now}),
        "skill_sources" => {
            json!({"source_url":null,"source_author":null,"source_repo":null,"source_path":null})
        }
        "skill_source_syncs" => {
            json!({"sync_scope":"skill","remote_ref":null,"skill_fingerprint":null,"last_checked_at":null,"last_sync_at":null,"sync_status":"never","sync_error":null,"remote_deleted":false})
        }
        "skill_metadata" => json!({"notes":null,"tags":"[]","updated_at":now}),
        "agents" => {
            json!({"project_skills_dir":null,"is_detected":false,"is_builtin":true,"is_enabled":true})
        }
        "collections" => json!({"description":null}),
        "skill_explanations" => json!({"lang":"zh","model":null,"created_at":now,"updated_at":now}),
        "repository_mirror_snapshots" => json!({"state":"checked"}),
        "repository_mirrors" => json!({"last_snapshot_id":null}),
        "repository_address_moves" => json!({"committed":false}),
        _ => json!({}),
    };
    defaults
        .as_object_mut()
        .unwrap()
        .extend(row.as_object().ok_or("Invalid fixture")?.clone());
    pool.write(|data| {
        data.table_mut(table)
            .insert(key(table, &defaults), defaults);
        Ok(())
    })
    .await
}
pub fn count(pool: &Store, table: &str, field: Option<(&str, &str)>) -> i64 {
    pool.read(|data| {
        Ok(data
            .table(table)
            .values()
            .filter(|row| field.is_none_or(|(key, value)| row[key].as_str() == Some(value)))
            .count() as i64)
    })
    .unwrap()
}
pub async fn set_agent_path(pool: &Store, id: &str, path: &str) {
    pool.write(|data| {
        if let Some(row) = data.table_mut("agents").get_mut(id) {
            row["global_skills_dir"] = json!(path);
        }
        Ok(())
    })
    .await
    .unwrap();
}
pub async fn disable_except(pool: &Store, ids: &[&str]) {
    pool.write(|data| {
        for (id, row) in data.table_mut("agents") {
            if !ids.contains(&id.as_str()) {
                row["is_enabled"] = json!(false);
            }
        }
        Ok(())
    })
    .await
    .unwrap();
}
pub async fn clear(pool: &Store, table: &str) {
    pool.write(|data| {
        data.table_mut(table).clear();
        Ok(())
    })
    .await
    .unwrap();
}

/// Exercise v2 through the real store without requiring huge business fixtures.
pub async fn shard_metadata(store: &Store) {
    store.write(|data| { data.table_mut("skill_metadata").make_sharded(); Ok(()) }).await.unwrap();
}
