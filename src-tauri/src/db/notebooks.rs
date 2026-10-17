//! Native JSON repository notebooks, preserving stable identities.
use crate::commands::notebook_repository::{validate, Repository};
use crate::db::{rows, AppStore};
pub async fn schema(_pool: &AppStore) -> Result<(), String> {
    Ok(())
}
fn list_data(d: &crate::storage::store::Data) -> Result<Vec<Repository>, String> {
    let mut rows = rows::<Repository>(d, "repository_notebooks")?;
    rows.sort_by_key(|r| r.id);
    Ok(rows)
}
pub async fn list(pool: &AppStore) -> Result<Vec<Repository>, String> {
    pool.read(list_data)
}
pub async fn put(pool: &AppStore, row: &Repository) -> Result<(), String> {
    pool.write(|d| {
        let mut all = list_data(d)?;
        all.retain(|r| r.id != row.id);
        // Match the old persistence constraints; binding validation belongs to callers.
        if all
            .iter()
            .any(|r| r.source_key == row.source_key || r.repository_uid == row.repository_uid)
        {
            return Err("noteDatabase".into());
        }
        advance_counter(d, row.id.checked_add(1).ok_or("noteIdExhausted")?)?;
        crate::db::put(d, "repository_notebooks", row.id.to_string(), row)
    })
    .await
}
pub async fn restore(pool: &AppStore, incoming: &[Repository]) -> Result<(), String> {
    validate(incoming)?;
    pool.write(|d| {
        let mut all = list_data(d)?;
        for row in incoming {
            let mut row = row.clone();
            row.id = if let Some(existing) = all.iter().find(|r| r.source_key == row.source_key) {
                existing.id
            } else { allocate(d)? };
            all.retain(|r| r.source_key != row.source_key);
            all.push(row);
        }
        validate(&all)?;
        for row in all {
            crate::db::put(d, "repository_notebooks", row.id.to_string(), &row)?;
        }
        Ok(())
    })
    .await
}
fn next_value(d: &crate::storage::store::Data) -> Result<i64, String> {
    let floor = list_data(d)?.iter().map(|r| r.id).max().unwrap_or(0).checked_add(1).ok_or("noteIdExhausted")?;
    let saved = d.table("store_counters").get("repository_notebooks")
        .map(|r| r["next_id"].as_i64().ok_or("noteDatabase")).transpose()?.unwrap_or(1);
    Ok(saved.max(floor).max(1))
}
fn advance_counter(d: &mut crate::storage::store::Data, minimum: i64) -> Result<(), String> {
    let next = next_value(d)?.max(minimum);
    d.table_mut("store_counters").insert("repository_notebooks".into(), serde_json::json!({"next_id":next}));
    Ok(())
}
fn allocate(d: &mut crate::storage::store::Data) -> Result<i64, String> {
    let id = next_value(d)?;
    let next = id.checked_add(1).ok_or("noteIdExhausted")?;
    d.table_mut("store_counters").insert("repository_notebooks".into(), serde_json::json!({"next_id":next}));
    Ok(id)
}
pub async fn next_id(pool: &AppStore) -> Result<i64, String> {
    pool.write(allocate).await
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn deleted_highest_notebook_id_is_not_reused() {
        let pool = AppStore::memory().unwrap();
        put(&pool, &Repository::new(42, "fixture/repo".into())).await.unwrap();
        pool.write(|d| { d.table_mut("repository_notebooks").clear(); Ok(()) }).await.unwrap();
        let (a,b) = tokio::join!(next_id(&pool),next_id(&pool));
        let mut ids = [a.unwrap(),b.unwrap()]; ids.sort();
        assert_eq!(ids,[43,44]);
        restore(&pool,&[Repository::new(1,"fixture/restored".into())]).await.unwrap();
        assert_eq!(list(&pool).await.unwrap()[0].id,45);
        assert_eq!(next_id(&pool).await.unwrap(),46);
    }
    #[tokio::test]
    async fn exhausted_counter_fails_without_wrapping() {
        let pool = AppStore::memory().unwrap();
        pool.write(|d| { d.table_mut("store_counters").insert("repository_notebooks".into(),serde_json::json!({"next_id":i64::MAX})); Ok(()) }).await.unwrap();
        assert!(next_id(&pool).await.is_err());
    }
}
