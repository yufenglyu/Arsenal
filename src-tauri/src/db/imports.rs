//! Import metadata commits; file staging remains owned by the import workflow.
use super::{DbPool, Skill, SkillSource};

pub struct ImportCommit(crate::storage::store::Transaction);

impl ImportCommit {
    pub async fn begin(pool: &DbPool) -> Result<Self, String> {
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

pub async fn has_receipt(pool: &DbPool, id: &str) -> Result<bool, String> {
    pool.read(|data| Ok(data.table("import_transaction_receipts").contains_key(id)))
}

pub async fn installed_paths(pool: &DbPool) -> Result<Vec<String>, String> {
    pool.read(|data| {
        Ok(data
            .table("skill_installations")
            .values()
            .filter_map(|row| row["installed_path"].as_str().map(str::to_owned))
            .collect())
    })
}

pub async fn canonical_paths(pool: &DbPool) -> Result<Vec<(String, String)>, String> {
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
