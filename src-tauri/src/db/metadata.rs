//! SQLite adapter for metadata operations; command handlers have no SQL.
use crate::db;
use crate::storage::models::{FolderNote, MetadataRow};
use sqlx::Row;

async fn ensure_schema(pool: &db::DbPool) -> Result<(), String> {
    sqlx::query("CREATE TABLE IF NOT EXISTS folder_notes (id TEXT PRIMARY KEY, notes TEXT NOT NULL, skill_ids TEXT NOT NULL)")
        .execute(pool).await.map_err(|e|e.to_string())?;
    Ok(())
}

pub async fn folder_notes(pool: &db::DbPool) -> Result<Vec<FolderNote>, String> {
    ensure_schema(pool).await?;
    sqlx::query("SELECT * FROM folder_notes")
        .fetch_all(pool)
        .await
        .map_err(|e| e.to_string())?
        .into_iter()
        .map(|r| {
            Ok(FolderNote {
                id: r.get("id"),
                notes: r.get("notes"),
                skill_ids: serde_json::from_str(r.get("skill_ids")).map_err(|e| e.to_string())?,
            })
        })
        .collect()
}

pub async fn save_note(pool: &db::DbPool, note: &FolderNote) -> Result<(), String> {
    ensure_schema(pool).await?;
    sqlx::query("INSERT INTO folder_notes VALUES (?, ?, ?) ON CONFLICT(id) DO UPDATE SET notes=excluded.notes, skill_ids=excluded.skill_ids")
        .bind(&note.id).bind(&note.notes).bind(serde_json::to_string(&note.skill_ids).map_err(|e|e.to_string())?)
        .execute(pool).await.map_err(|e|e.to_string())?;
    Ok(())
}

pub async fn change_tag(
    pool: &db::DbPool,
    tag: &str,
    skill_ids: Option<Vec<String>>,
    remove: bool,
) -> Result<Vec<MetadataRow>, String> {
    let tag = tag.trim();
    if tag.is_empty() || tag.chars().count() > 100 {
        return Err("Invalid tag".into());
    }
    let mut tx = pool.begin().await.map_err(|e| e.to_string())?;
    let rows = sqlx::query(
        "SELECT s.id, m.notes, m.tags FROM skills s LEFT JOIN skill_metadata m ON s.id=m.skill_id",
    )
    .fetch_all(&mut *tx)
    .await
    .map_err(|e| e.to_string())?;
    let mut changed = Vec::new();
    for row in rows {
        let id: String = row.get("id");
        if skill_ids.as_ref().is_some_and(|ids| !ids.contains(&id)) {
            continue;
        }
        let mut tags: Vec<String> = row
            .get::<Option<String>, _>("tags")
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default();
        let has = tags.iter().any(|t| t.to_lowercase() == tag.to_lowercase());
        if has != remove {
            continue;
        }
        if remove {
            tags.retain(|t| t.to_lowercase() != tag.to_lowercase());
        } else {
            tags.push(tag.into());
        }
        let notes: Option<String> = row.get("notes");
        sqlx::query("INSERT INTO skill_metadata (skill_id,notes,tags,updated_at) VALUES (?,?,?,?) ON CONFLICT(skill_id) DO UPDATE SET tags=excluded.tags,updated_at=excluded.updated_at")
            .bind(&id).bind(&notes).bind(serde_json::to_string(&tags).map_err(|e|e.to_string())?).bind(chrono::Utc::now().to_rfc3339())
            .execute(&mut *tx).await.map_err(|e|e.to_string())?;
        changed.push(MetadataRow { id, notes, tags });
    }
    tx.commit().await.map_err(|e| e.to_string())?;
    Ok(changed)
}

pub async fn rename_tag(
    pool: &db::DbPool,
    old: &str,
    new: &str,
    merge: bool,
) -> Result<Vec<MetadataRow>, String> {
    let new = new.trim();
    if new.is_empty() || new.chars().count() > 100 {
        return Err("Invalid tag".into());
    }
    let mut tx = pool.begin().await.map_err(|e| e.to_string())?;
    let rows = sqlx::query("SELECT skill_id, notes, tags FROM skill_metadata")
        .fetch_all(&mut *tx)
        .await
        .map_err(|e| e.to_string())?;
    let parsed = rows
        .into_iter()
        .map(|r| {
            Ok(MetadataRow {
                id: r.get("skill_id"),
                notes: r.get("notes"),
                tags: serde_json::from_str::<Vec<String>>(r.get("tags"))
                    .map_err(|e| e.to_string())?,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    if old.to_lowercase() != new.to_lowercase()
        && !merge
        && parsed.iter().any(|r| {
            r.tags
                .iter()
                .any(|t| t.to_lowercase() == new.to_lowercase())
        })
    {
        return Err("Tag already exists".into());
    }
    let mut changed = Vec::new();
    for mut row in parsed {
        if !row
            .tags
            .iter()
            .any(|t| t.to_lowercase() == old.to_lowercase())
        {
            continue;
        }
        let mut seen = std::collections::HashSet::new();
        row.tags = row
            .tags
            .into_iter()
            .map(|t| {
                if t.to_lowercase() == old.to_lowercase() || t.to_lowercase() == new.to_lowercase()
                {
                    new.to_string()
                } else {
                    t
                }
            })
            .filter(|t| seen.insert(t.to_lowercase()))
            .collect();
        sqlx::query(
            "UPDATE skill_metadata SET tags=?, updated_at=datetime('now') WHERE skill_id=?",
        )
        .bind(serde_json::to_string(&row.tags).map_err(|e| e.to_string())?)
        .bind(&row.id)
        .execute(&mut *tx)
        .await
        .map_err(|e| e.to_string())?;
        changed.push(row);
    }
    tx.commit().await.map_err(|e| e.to_string())?;
    Ok(changed)
}

pub async fn count_tag(pool: &db::DbPool, tag: &str) -> Result<usize, String> {
    let rows = sqlx::query("SELECT tags FROM skill_metadata")
        .fetch_all(pool)
        .await
        .map_err(|e| e.to_string())?;
    Ok(rows
        .iter()
        .filter(|r| {
            serde_json::from_str::<Vec<String>>(r.get("tags"))
                .unwrap_or_default()
                .iter()
                .any(|t| t.to_lowercase() == tag.to_lowercase())
        })
        .count())
}
