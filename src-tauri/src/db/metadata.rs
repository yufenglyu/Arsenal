//! Atomic metadata operations on the JSON store.
use crate::db::{self, AppStore, SkillMetadata};
use crate::storage::models::{FolderNote, MetadataRow};
pub async fn folder_notes(pool: &AppStore) -> Result<Vec<FolderNote>, String> {
    pool.read(|d| db::rows(d, "folder_notes"))
}
pub async fn save_note(pool: &AppStore, note: &FolderNote) -> Result<(), String> {
    pool.write(|d| db::put(d, "folder_notes", note.id.clone(), note))
        .await
}
pub async fn change_tag(
    pool: &AppStore,
    tag: &str,
    skill_ids: Option<Vec<String>>,
    remove: bool,
) -> Result<Vec<MetadataRow>, String> {
    let tag = tag.trim();
    if tag.is_empty() || tag.chars().count() > 100 {
        return Err("Invalid tag".into());
    }
    let selected = skill_ids.map(|ids| ids.into_iter().collect::<std::collections::HashSet<_>>());
    let normalized_tag = tag.to_lowercase();
    pool.write(|d| {
        let ids: Vec<String> = d.table("skills").keys().cloned().collect();
        let mut changed = Vec::new();
        for id in ids {
            if selected.as_ref().is_some_and(|ids| !ids.contains(&id)) {
                continue;
            }
            let old = d
                .table("skill_metadata")
                .get(&id)
                .map(<SkillMetadata as serde::Deserialize>::deserialize)
                .transpose()
                .map_err(|e| e.to_string())?;
            let mut tags: Vec<String> = old
                .as_ref()
                .and_then(|m| serde_json::from_str(&m.tags).ok())
                .unwrap_or_default();
            let has = tags.iter().any(|t| t.to_lowercase() == normalized_tag);
            if has != remove {
                continue;
            }
            if remove {
                tags.retain(|t| t.to_lowercase() != normalized_tag);
            } else {
                tags.push(tag.into());
            }
            let notes = old.and_then(|m| m.notes);
            let row = SkillMetadata {
                skill_id: id.clone(),
                notes: notes.clone(),
                tags: serde_json::to_string(&tags).map_err(|e| e.to_string())?,
                updated_at: chrono::Utc::now().to_rfc3339(),
            };
            db::put(d, "skill_metadata", id.clone(), &row)?;
            changed.push(MetadataRow { id, notes, tags });
        }
        Ok(changed)
    })
    .await
}
pub async fn rename_tag(
    pool: &AppStore,
    old: &str,
    new: &str,
    merge: bool,
) -> Result<Vec<MetadataRow>, String> {
    let new = new.trim();
    if new.is_empty() || new.chars().count() > 100 {
        return Err("Invalid tag".into());
    }
    pool.write(|d| {
        let all = db::rows::<SkillMetadata>(d, "skill_metadata")?;
        let parsed = all
            .into_iter()
            .map(|m| {
                let tags =
                    serde_json::from_str::<Vec<String>>(&m.tags).map_err(|e| e.to_string())?;
                Ok((m, tags))
            })
            .collect::<Result<Vec<_>, String>>()?;
        if old.to_lowercase() != new.to_lowercase()
            && !merge
            && parsed
                .iter()
                .any(|(_, tags)| tags.iter().any(|t| t.to_lowercase() == new.to_lowercase()))
        {
            return Err("Tag already exists".into());
        }
        let mut changed = Vec::new();
        for (mut m, tags) in parsed {
            if !tags.iter().any(|t| t.to_lowercase() == old.to_lowercase()) {
                continue;
            }
            let mut seen = std::collections::HashSet::new();
            let tags: Vec<_> = tags
                .into_iter()
                .map(|t| {
                    if t.to_lowercase() == old.to_lowercase()
                        || t.to_lowercase() == new.to_lowercase()
                    {
                        new.to_owned()
                    } else {
                        t
                    }
                })
                .filter(|t| seen.insert(t.to_lowercase()))
                .collect();
            m.tags = serde_json::to_string(&tags).map_err(|e| e.to_string())?;
            m.updated_at = chrono::Utc::now().to_rfc3339();
            db::put(d, "skill_metadata", m.skill_id.clone(), &m)?;
            changed.push(MetadataRow {
                id: m.skill_id,
                notes: m.notes,
                tags,
            });
        }
        Ok(changed)
    })
    .await
}
pub async fn count_tag(pool: &AppStore, tag: &str) -> Result<usize, String> {
    pool.read(|d| {
        Ok(db::rows::<SkillMetadata>(d, "skill_metadata")?
            .iter()
            .filter(|m| {
                serde_json::from_str::<Vec<String>>(&m.tags)
                    .unwrap_or_default()
                    .iter()
                    .any(|t| t.to_lowercase() == tag.to_lowercase())
            })
            .count())
    })
}
