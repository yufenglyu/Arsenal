//! Repository address persistence. Filesystem changes are coordinated by the caller.
use crate::commands::notebook_repository::Repository;
use crate::db::AppStore;
use std::path::PathBuf;

pub struct AddressCommit<'a> {
    pub old: &'a str,
    pub new: &'a str,
    pub relative: &'a str,
    pub move_id: &'a str,
    pub updates: &'a [(String, PathBuf, PathBuf)],
    pub notebooks: Vec<Repository>,
}

pub async fn commit_address(pool: &AppStore, change: AddressCommit<'_>) -> Result<(), String> {
    let AddressCommit {
        old,
        new,
        relative,
        move_id,
        updates,
        notebooks,
    } = change;
    let old_key = format!("github:{}", old.to_lowercase());
    let new_key = format!("github:{}", new.to_lowercase());
    let mut tx = pool.begin().await.map_err(|_| "addressDatabase")?;
    let data = tx.data_mut();
    for (id, file, canonical) in updates {
        if let Some(row) = data.table_mut("skills").get_mut(id) {
            row["canonical_path"] = canonical.to_string_lossy().as_ref().into();
            row["file_path"] = file.to_string_lossy().as_ref().into();
        }
    }
    let source_ids: Vec<String> = data
        .table("skill_sources")
        .values()
        .filter(|r| {
            r["source_repo"]
                .as_str()
                .is_some_and(|v| v.eq_ignore_ascii_case(old))
        })
        .filter_map(|r| r["skill_id"].as_str().map(str::to_owned))
        .collect();
    for id in source_ids {
        if let Some(row) = data.table_mut("skills").get_mut(&id) {
            row["source"] = format!("github:{new}").into();
        }
        if let Some(row) = data.table_mut("skill_sources").get_mut(&id) {
            row["source_repo"] = new.into();
            row["source_author"] = serde_json::json!(new.split('/').next());
            row["source_url"] = serde_json::Value::Null;
            row["updated_at"] = chrono::Utc::now().to_rfc3339().into();
        }
    }
    for table in [
        "repository_mirrors",
        "repository_local_directories",
        "github_repository_stats",
    ] {
        data.table_mut(table).retain(|_, r| {
            !r["repository"]
                .as_str()
                .is_some_and(|v| v.eq_ignore_ascii_case(old))
        });
    }
    data.table_mut("repository_mirrors").insert(
        new.to_ascii_lowercase(),
        serde_json::json!({"repository":new.to_ascii_lowercase()}),
    );
    data.table_mut("repository_local_directories").insert(
        new.to_ascii_lowercase(),
        serde_json::json!({"repository":new.to_ascii_lowercase(),"directory":relative}),
    );
    for row in data.table_mut("repository_mirror_snapshots").values_mut() {
        if row["repository"]
            .as_str()
            .is_some_and(|v| v.eq_ignore_ascii_case(old))
        {
            row["state"] = "invalidated".into();
        }
    }
    for mut row in notebooks
        .into_iter()
        .filter(|r| r.source_key.eq_ignore_ascii_case(&old_key))
    {
        row.source_key = new_key.clone();
        row.full_name = new.into();
        row.url = format!("https://github.com/{new}");
        row.stars = None;
        data.table_mut("repository_notebooks").insert(
            row.id.to_string(),
            serde_json::to_value(&row).map_err(|_| "addressDatabase")?,
        );
    }
    if let Some(row) = data.table_mut("repository_address_moves").get_mut(move_id) {
        row["committed"] = true.into();
    }
    tx.commit().await.map_err(|_| "addressDatabase".into())
}
