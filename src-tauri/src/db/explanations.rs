//! Explanation cache persistence, separate from provider and display logic.
use crate::db::{key2, AppStore};
pub async fn delete(pool: &AppStore, skill_id: &str, lang: &str) -> Result<(), String> {
    pool.write(|d| {
        d.table_mut("skill_explanations")
            .remove(&key2(skill_id, lang));
        Ok(())
    })
    .await
}
pub async fn load(pool: &AppStore, skill_id: &str, lang: &str) -> Result<Option<String>, String> {
    pool.read(|d| {
        Ok(d.table("skill_explanations")
            .get(&key2(skill_id, lang))
            .and_then(|r| r["explanation"].as_str())
            .map(str::to_owned))
    })
}
pub async fn save(
    pool: &AppStore,
    skill_id: &str,
    lang: &str,
    model: &str,
    explanation: &str,
) -> Result<(), String> {
    pool.write(|d|{let now=chrono::Utc::now().to_rfc3339();let key=key2(skill_id,lang);let created=d.table("skill_explanations").get(&key).and_then(|r|r["created_at"].as_str()).unwrap_or(&now).to_owned();d.table_mut("skill_explanations").insert(key,serde_json::json!({"skill_id":skill_id,"lang":lang,"model":model,"explanation":explanation,"created_at":created,"updated_at":now}));Ok(())}).await
}
pub async fn clear(pool: &AppStore) -> Result<(), String> {
    pool.write(|d| {
        d.table_mut("skill_explanations").clear();
        Ok(())
    })
    .await
}
