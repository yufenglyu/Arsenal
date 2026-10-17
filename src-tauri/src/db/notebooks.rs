//! SQLite persistence for repository notebooks.
use crate::db::DbPool;
use crate::commands::notebook_repository::{Repository, validate};
use sqlx::Row;

pub async fn schema(pool: &DbPool) -> Result<(), String> {
    sqlx::query("CREATE TABLE IF NOT EXISTS repository_notebooks (id INTEGER PRIMARY KEY AUTOINCREMENT, source_key TEXT NOT NULL UNIQUE, repository_uid TEXT NOT NULL UNIQUE, data TEXT NOT NULL)")
        .execute(pool).await.map_err(|_| "noteDatabase")?;
    Ok(())
}

pub async fn list(pool: &DbPool) -> Result<Vec<Repository>, String> {
    schema(pool).await?;
    sqlx::query("SELECT data FROM repository_notebooks ORDER BY id").fetch_all(pool).await.map_err(|_| "noteDatabase")?
        .into_iter().map(|r| serde_json::from_str(r.get("data")).map_err(|_| "noteDatabase".into())).collect()
}

pub async fn put(pool: &DbPool, row: &Repository) -> Result<(), String> {
    schema(pool).await?;
    let data = serde_json::to_string(row).map_err(|_| "noteDatabase")?;
    sqlx::query("INSERT INTO repository_notebooks (id, source_key, repository_uid, data) VALUES (?, ?, ?, ?) ON CONFLICT(id) DO UPDATE SET source_key=excluded.source_key, repository_uid=excluded.repository_uid, data=excluded.data")
        .bind(row.id).bind(&row.source_key).bind(&row.repository_uid).bind(data).execute(pool).await.map_err(|_| "noteDatabase")?;
    Ok(())
}

pub async fn restore(pool: &DbPool, rows: &[Repository]) -> Result<(), String> {
    validate(rows)?;
    schema(pool).await?;
    let existing = list(pool).await?;
    let mut merged = existing.clone();
    for row in rows {
        merged.retain(|r| r.source_key != row.source_key);
        merged.push(row.clone());
    }
    validate(&merged)?;
    let mut tx = pool.begin().await.map_err(|_| "noteDatabase")?;
    for row in rows {
        let mut row = row.clone();
        row.id = if let Some(old) = existing.iter().find(|r| r.source_key == row.source_key) { old.id } else {
            let max: i64 = sqlx::query_scalar("SELECT COALESCE(MAX(id), 0) FROM repository_notebooks").fetch_one(&mut *tx).await.map_err(|_| "noteDatabase")?;
            max + 1
        };
        sqlx::query("INSERT INTO repository_notebooks (id, source_key, repository_uid, data) VALUES (?, ?, ?, ?) ON CONFLICT(source_key) DO UPDATE SET repository_uid=excluded.repository_uid, data=excluded.data")
            .bind(row.id).bind(&row.source_key).bind(&row.repository_uid).bind(serde_json::to_string(&row).map_err(|_| "noteDatabase")?)
            .execute(&mut *tx).await.map_err(|_| "noteDatabase")?;
    }
    tx.commit().await.map_err(|_| "noteDatabase".into())
}

pub async fn next_id(pool: &DbPool) -> Result<i64, String> {
    sqlx::query_scalar("SELECT COALESCE(MAX(id), 0) + 1 FROM repository_notebooks")
        .fetch_one(pool).await.map_err(|_| "noteDatabase".into())
}
