//! Import metadata commits; file staging remains owned by the import workflow.
use super::{AppStore, Skill, SkillSource};

pub struct ImportCommit(crate::storage::store::Transaction);

impl ImportCommit {
    pub async fn begin(pool: &AppStore) -> Result<Self, String> {
        pool.begin().await.map(Self).map_err(|e| e.to_string())
    }

    pub async fn skill(&mut self, skill: &Skill) -> Result<(), String> {
        super::upsert_skill_with_executor(self.0.data_mut(), skill).await
    }

    pub async fn source(&mut self, source: &SkillSource) -> Result<(), String> {
        super::upsert_skill_source_with_executor(self.0.data_mut(), source).await
    }

    pub async fn stars(&mut self, repository: &str, stars: i64) -> Result<(), String> {
        super::save_github_stars_with_executor(self.0.data_mut(), repository, stars).await
    }

    pub async fn receipt(&mut self, id: &str) -> Result<(), String> {
        self.0
            .data_mut()
            .table_mut("import_transaction_receipts")
            .insert(id.to_owned(), serde_json::json!({"id": id}));
        Ok(())
    }

    pub async fn commit(self) -> Result<(), String> {
        self.0.commit().await.map_err(|e| e.to_string())
    }
}

pub async fn has_receipt(pool: &AppStore, id: &str) -> Result<bool, String> {
    pool.read(|data| Ok(data.table("import_transaction_receipts").contains_key(id)))
}

pub async fn installed_paths(pool: &AppStore) -> Result<Vec<String>, String> {
    pool.read(|data| {
        Ok(data
            .table("skill_installations")
            .values()
            .filter_map(|row| row["installed_path"].as_str().map(str::to_owned))
            .collect())
    })
}

pub async fn canonical_paths(pool: &AppStore) -> Result<Vec<(String, String)>, String> {
    pool.read(|data| {
        Ok(data
            .table("skills")
            .values()
            .filter_map(|row| {
                Some((
                    row["id"].as_str()?.to_owned(),
                    row["canonical_path"].as_str()?.to_owned(),
                ))
            })
            .collect())
    })
}

#[cfg(test)]
mod durability_tests {
    use super::*;
    #[tokio::test]
    async fn import_receipt_and_metadata_publish_together_across_reopen() {
        let root = tempfile::tempdir().unwrap();
        let store = AppStore::open(root.path()).unwrap();
        let skill = Skill {
            id: "synthetic".into(),
            name: "Synthetic".into(),
            description: None,
            file_path: "/synthetic/SKILL.md".into(),
            canonical_path: Some("/synthetic".into()),
            is_central: false,
            source: Some("github".into()),
            content: None,
            scanned_at: "2026-01-01".into(),
        };
        let source = SkillSource {
            skill_id: skill.id.clone(),
            source_type: "github".into(),
            source_url: None,
            source_author: Some("fixture".into()),
            source_repo: Some("repo".into()),
            source_path: None,
            updated_at: "2026-01-01".into(),
        };
        let mut tx = ImportCommit::begin(&store).await.unwrap();
        tx.skill(&skill).await.unwrap();
        tx.source(&source).await.unwrap();
        tx.receipt("receipt").await.unwrap();
        store.fail_table_write("skill_sources");
        assert!(tx.commit().await.is_err());
        drop(store);
        let store = AppStore::open(root.path()).unwrap();
        assert!(!has_receipt(&store, "receipt").await.unwrap());
        assert!(crate::db::get_skill_by_id(&store, "synthetic")
            .await
            .unwrap()
            .is_none());
        let mut tx = ImportCommit::begin(&store).await.unwrap();
        tx.skill(&skill).await.unwrap();
        tx.source(&source).await.unwrap();
        tx.receipt("receipt").await.unwrap();
        tx.commit().await.unwrap();
        drop(store);
        let store = AppStore::open(root.path()).unwrap();
        assert!(has_receipt(&store, "receipt").await.unwrap());
        assert_eq!(
            crate::db::get_skill_by_id(&store, "synthetic")
                .await
                .unwrap()
                .unwrap()
                .name,
            "Synthetic"
        );
        assert_eq!(
            store
                .read(|d| Ok(d.table("skill_sources")["synthetic"]["source_repo"].clone()))
                .unwrap(),
            "repo"
        );
    }
}
