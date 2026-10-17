//! Explanation cache persistence, separate from provider and display logic.
use crate::db::DbPool;

pub async fn delete(pool: &DbPool, skill_id: &str, lang: &str) -> Result<(), String> {
    sqlx::query("DELETE FROM skill_explanations WHERE skill_id = ? AND lang = ?")
        .bind(skill_id)
        .bind(lang)
        .execute(pool)
        .await
        .map_err(|e| e.to_string())?;
    Ok(())
}

pub async fn load(pool: &DbPool, skill_id: &str, lang: &str) -> Result<Option<String>, String> {
    sqlx::query_scalar("SELECT explanation FROM skill_explanations WHERE skill_id = ? AND lang = ?")
        .bind(skill_id).bind(lang).fetch_optional(pool).await.map_err(|e| e.to_string())
}

pub async fn save(pool: &DbPool, skill_id: &str, lang: &str, model: &str, explanation: &str) -> Result<(), String> {
    let now = chrono::Utc::now().to_rfc3339();
    sqlx::query(
        "INSERT OR REPLACE INTO skill_explanations (skill_id, explanation, lang, model, created_at, updated_at)
         VALUES (?, ?, ?, ?,
            COALESCE((SELECT created_at FROM skill_explanations WHERE skill_id = ? AND lang = ?), ?),
            ?)",
    )
    .bind(skill_id)
    .bind(explanation)
    .bind(lang)
    .bind(model)
    .bind(skill_id)
    .bind(lang)
    .bind(&now)
    .bind(&now)
    .execute(pool)
    .await
    .map_err(|e| format!("缓存解释失败: {}", e))?;

    Ok(())
}

pub async fn clear(pool: &DbPool) -> Result<(), String> {
    sqlx::query("DELETE FROM skill_explanations").execute(pool).await.map_err(|e| e.to_string())?;
    Ok(())
}
