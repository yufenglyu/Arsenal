use crate::db::metadata::{change_tag, rename_tag};
pub use crate::db::metadata::{folder_notes, save_note};
pub use crate::storage::models::{FolderNote, MetadataRow};
use crate::AppState;
use tauri::State;

#[tauri::command]
pub async fn get_folder_notes(state: State<'_, AppState>) -> Result<Vec<FolderNote>, String> {
    folder_notes(&state.db).await
}
#[tauri::command]
pub async fn save_folder_note(state: State<'_, AppState>, note: FolderNote) -> Result<(), String> {
    save_note(&state.db, &note).await
}

#[tauri::command]
pub async fn count_skill_tag(state: State<'_, AppState>, tag: String) -> Result<usize, String> {
    crate::db::metadata::count_tag(&state.db, &tag).await
}
#[tauri::command]
pub async fn change_skill_tag(
    state: State<'_, AppState>,
    tag: String,
    skill_ids: Option<Vec<String>>,
    remove: bool,
) -> Result<Vec<MetadataRow>, String> {
    change_tag(&state.db, &tag, skill_ids, remove).await
}

#[tauri::command]
pub async fn rename_skill_tag(
    state: State<'_, AppState>,
    old_tag: String,
    new_tag: String,
    merge: bool,
) -> Result<Vec<MetadataRow>, String> {
    rename_tag(&state.db, &old_tag, &new_tag, merge).await
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn rename_requires_merge_confirmation_and_preserves_notes() {
        let pool = crate::db::AppStore::memory().unwrap();
        for (id, tags) in [
            ("one", vec!["Old", "Keep"]),
            ("two", vec!["Old", "New"]),
            ("three", vec!["Keep"]),
        ] {
            pool.write(|d|{d.table_mut("skill_metadata").insert(id.into(),serde_json::json!({"skill_id":id,"notes":"memo","tags":serde_json::to_string(&tags).unwrap(),"updated_at":""}));Ok(())}).await.unwrap();
        }
        assert!(rename_tag(&pool, "Old", "New", false).await.is_err());
        let rows = rename_tag(&pool, "old", " New ", true).await.unwrap();
        assert_eq!(rows.len(), 2);
        assert!(rows.iter().all(
            |row| row.notes.as_deref() == Some("memo") && !row.tags.iter().any(|t| t == "Old")
        ));
        assert_eq!(
            rows.iter().find(|row| row.id == "two").unwrap().tags,
            vec!["New"]
        );
        assert!(rename_tag(&pool, "New", " ", false).await.is_err());
        let untouched: String = pool
            .read(|d| {
                Ok(d.table("skill_metadata")["three"]["tags"]
                    .as_str()
                    .unwrap()
                    .to_owned())
            })
            .unwrap();
        assert_eq!(untouched, "[\"Keep\"]");
    }

    #[tokio::test]
    async fn tag_changes_preserve_notes_and_unselected_skills() {
        let pool = crate::db::AppStore::memory().unwrap();
        for id in ["a", "b", "c"] {
            pool.write(|d| {
                d.table_mut("skills")
                    .insert(id.into(), serde_json::json!({"id":id}));
                Ok(())
            })
            .await
            .unwrap();
        }
        for (id, tags) in [("a", vec!["A", "B"]), ("b", vec!["B"]), ("c", vec!["A"])] {
            pool.write(|d|{d.table_mut("skill_metadata").insert(id.into(),serde_json::json!({"skill_id":id,"notes":"memo","tags":serde_json::to_string(&tags).unwrap(),"updated_at":""}));Ok(())}).await.unwrap();
        }
        let ids = Some(vec!["a".into(), "b".into()]);
        let added = change_tag(&pool, "A", ids.clone(), false).await.unwrap();
        assert_eq!(added.len(), 1);
        assert_eq!(added[0].id, "b");
        assert_eq!(added[0].notes.as_deref(), Some("memo"));
        assert_eq!(change_tag(&pool, "A", ids, true).await.unwrap().len(), 2);
        let remaining = change_tag(&pool, "A", None, true).await.unwrap();
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].id, "c");
    }
    #[tokio::test]
    async fn folder_notes_roundtrip_and_replace_without_absolute_paths() {
        let pool = crate::db::AppStore::memory().unwrap();
        let mut note = FolderNote {
            id: "stable".into(),
            notes: "memo".into(),
            skill_ids: vec!["member".into()],
        };
        save_note(&pool, &note).await.unwrap();
        note.notes = "edited".into();
        save_note(&pool, &note).await.unwrap();
        let saved = folder_notes(&pool).await.unwrap();
        assert_eq!(saved.len(), 1);
        assert_eq!(saved[0].notes, "edited");
        assert_eq!(saved[0].skill_ids, vec!["member"]);
    }
}
