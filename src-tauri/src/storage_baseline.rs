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
async fn synthetic_json_baseline() {
    for count in [1_000usize, 10_000, 50_000] {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().to_path_buf();
        let pool = db::open_test_store(path.to_str().unwrap()).await.unwrap();
        db::init_schema(&pool).await.unwrap();
        let collection = db::create_collection(&pool, "Synthetic collection", None)
            .await
            .unwrap();
        let seed_start = Instant::now();
        let mut tx = pool.begin().await.unwrap();
        for index in 0..count {
            let skill = db::Skill {
                id: format!("fixture-{index}"),
                name: format!("Synthetic skill {index}"),
                description: Some("Generated storage baseline fixture".into()),
                file_path: format!("/synthetic/skills/{index}/SKILL.md"),
                canonical_path: Some(format!("/synthetic/skills/{index}")),
                is_central: true,
                source: Some("local".into()),
                content: None,
                scanned_at: "2026-01-01T00:00:00Z".into(),
            };
            db::put(tx.data_mut(), "skills", skill.id.clone(), &skill).unwrap();
            tx.data_mut().table_mut("skill_installations").insert(db::key2(&skill.id,"fixture-platform"), serde_json::json!({"skill_id":skill.id,"agent_id":"fixture-platform","installed_path":format!("/synthetic/platform/{index}"),"link_type":"symlink","is_managed":true,"created_at":"2026-01-01T00:00:00Z","symlink_target":null}));
            if index % 10 == 0 {
                tx.data_mut().table_mut("collection_skills").insert(db::key2(&collection.id,&skill.id),serde_json::json!({"collection_id":collection.id,"skill_id":skill.id,"added_at":"2026-01-01"}));
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
            assert!(
                db::get_skill_by_id(&pool, &format!("fixture-{}", count / 2))
                    .await
                    .unwrap()
                    .is_some()
            );
            lookup.push(start.elapsed().as_micros());
            let start = Instant::now();
            assert_eq!(
                db::get_skills_for_agent(&pool, "fixture-platform")
                    .await
                    .unwrap()
                    .len(),
                count
            );
            platform.push(start.elapsed().as_micros());
            let start = Instant::now();
            assert_eq!(
                db::get_collection_skills(&pool, &collection.id)
                    .await
                    .unwrap()
                    .len(),
                count / 10
            );
            members.push(start.elapsed().as_micros());
            let start = Instant::now();
            db::upsert_skill_metadata(
                &pool,
                "fixture-0",
                Some(&format!("Synthetic note {index}")),
                &["fixture".into()],
            )
            .await
            .unwrap();
            edits.push(start.elapsed().as_micros());
        }

        let bytes: u64 = std::fs::read_dir(path.join("store/objects"))
            .unwrap()
            .map(|e| e.unwrap().metadata().unwrap().len())
            .sum();
        drop(pool);
        let start = Instant::now();
        let reopened = db::open_test_store(path.to_str().unwrap()).await.unwrap();
        assert!(db::get_skill_by_id(&reopened, "fixture-0")
            .await
            .unwrap()
            .is_some());
        let reopen_us = start.elapsed().as_micros();
        drop(reopened);
        println!("STORAGE_BASELINE count={count} seed_batch_ms={seed_ms} lookup_p95_us={} platform_p95_us={} collection_p95_us={} edit_p95_us={} reopen_first_read_us={reopen_us} retained_object_bytes={bytes}", p95(&mut lookup), p95(&mut platform), p95(&mut members), p95(&mut edits));
    }
}
