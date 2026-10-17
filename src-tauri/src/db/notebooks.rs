//! Native JSON repository notebooks, preserving stable identities.
use crate::commands::notebook_repository::{validate, Repository};
use crate::db::{rows, DbPool};
pub async fn schema(_pool: &DbPool) -> Result<(), String> {
    Ok(())
}
fn list_data(d: &crate::storage::store::Data) -> Result<Vec<Repository>, String> {
    let mut rows = rows::<Repository>(d, "repository_notebooks")?;
    rows.sort_by_key(|r| r.id);
    Ok(rows)
}
pub async fn list(pool: &DbPool) -> Result<Vec<Repository>, String> {
    pool.read(list_data)
}
pub async fn put(pool: &DbPool, row: &Repository) -> Result<(), String> {
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
        crate::db::put(d, "repository_notebooks", row.id.to_string(), row)
    })
    .await
}
pub async fn restore(pool: &DbPool, incoming: &[Repository]) -> Result<(), String> {
    validate(incoming)?;
    pool.write(|d| {
        let mut all = list_data(d)?;
        for row in incoming {
            let mut row = row.clone();
            row.id = all
                .iter()
                .find(|r| r.source_key == row.source_key)
                .map(|r| r.id)
                .unwrap_or_else(|| all.iter().map(|r| r.id).max().unwrap_or(0) + 1);
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
pub async fn next_id(pool: &DbPool) -> Result<i64, String> {
    pool.read(|d| Ok(list_data(d)?.iter().map(|r| r.id).max().unwrap_or(0) + 1))
}
