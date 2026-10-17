//! Reproducible, opt-in storage baseline. Uses only generated data in a temp dir.
//! Run: cargo test --lib storage_baseline -- --ignored --nocapture

use crate::db;
use std::time::Instant;

fn p95(samples: &mut [u128]) -> u128 {
    samples.sort_unstable();
    samples[(samples.len() * 95).div_ceil(100) - 1]
}

#[tokio::test]
#[ignore = "synthetic storage benchmark; run explicitly without concurrent builds"]
async fn synthetic_sqlite_baseline() {
    for count in [1_000usize, 10_000, 50_000] {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("baseline.sqlite");
        let pool = db::create_pool(path.to_str().unwrap()).await.unwrap();
        db::init_schema(&pool).await.unwrap();
        let collection = db::create_collection(&pool, "Synthetic collection", None)
            .await.unwrap();
        let seed_start = Instant::now();
        let mut tx = pool.begin().await.unwrap();
        for index in 0..count {
            let skill = db::Skill {
                id: format!("fixture-{index}"), name: format!("Synthetic skill {index}"),
                description: Some("Generated storage baseline fixture".into()),
                file_path: format!("/synthetic/skills/{index}/SKILL.md"),
                canonical_path: Some(format!("/synthetic/skills/{index}")),
                is_central: true, source: Some("local".into()), content: None,
                scanned_at: "2026-01-01T00:00:00Z".into(),
            };
            db::upsert_skill_with_executor(&mut *tx, &skill).await.unwrap();
            sqlx::query("INSERT INTO skill_installations (skill_id,agent_id,installed_path,link_type,is_managed) VALUES (?, 'fixture-platform', ?, 'symlink', 1)")
                .bind(&skill.id).bind(format!("/synthetic/platform/{index}"))
                .execute(&mut *tx).await.unwrap();
            if index % 10 == 0 {
                sqlx::query("INSERT INTO collection_skills (collection_id,skill_id,added_at) VALUES (?, ?, '2026-01-01')")
                    .bind(&collection.id).bind(&skill.id).execute(&mut *tx).await.unwrap();
            }
        }
        tx.commit().await.unwrap();
        let seed_ms = seed_start.elapsed().as_millis();
        let mut lookup = Vec::new();
        let mut platform = Vec::new();
        let mut members = Vec::new();
        let mut edits = Vec::new();
        for index in 0..20 {
            let start = Instant::now();
            assert!(db::get_skill_by_id(&pool, &format!("fixture-{}", count / 2)).await.unwrap().is_some());
            lookup.push(start.elapsed().as_micros());
            let start = Instant::now();
            assert_eq!(db::get_skills_for_agent(&pool, "fixture-platform").await.unwrap().len(), count);
            platform.push(start.elapsed().as_micros());
            let start = Instant::now();
            assert_eq!(db::get_collection_skills(&pool, &collection.id).await.unwrap().len(), count / 10);
            members.push(start.elapsed().as_micros());
            let start = Instant::now();
            db::upsert_skill_metadata(&pool, "fixture-0", Some(&format!("Synthetic note {index}")), &["fixture".into()]).await.unwrap();
            edits.push(start.elapsed().as_micros());
        }
        sqlx::query("PRAGMA wal_checkpoint(TRUNCATE)").execute(&pool).await.unwrap();
        let bytes = std::fs::metadata(&path).unwrap().len();
        pool.close().await;
        let start = Instant::now();
        let reopened = db::create_pool(path.to_str().unwrap()).await.unwrap();
        assert!(db::get_skill_by_id(&reopened, "fixture-0").await.unwrap().is_some());
        let reopen_us = start.elapsed().as_micros();
        reopened.close().await;
        println!("STORAGE_BASELINE count={count} seed_batch_ms={seed_ms} lookup_p95_us={} platform_p95_us={} collection_p95_us={} edit_p95_us={} reopen_first_read_us={reopen_us} db_bytes={bytes}", p95(&mut lookup), p95(&mut platform), p95(&mut members), p95(&mut edits));
    }
}
