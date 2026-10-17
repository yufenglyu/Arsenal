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
        let prepare_us = seed_start.elapsed().as_micros();
        let commit_start = Instant::now();
        tx.commit().await.unwrap();
        let commit_us = commit_start.elapsed().as_micros();
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
        println!("STORAGE_BASELINE count={count} seed_batch_ms={seed_ms} prepare_us={prepare_us} commit_us={commit_us} lookup_p95_us={} platform_p95_us={} collection_p95_us={} edit_p95_us={} reopen_first_read_us={reopen_us} retained_object_bytes={bytes}", p95(&mut lookup), p95(&mut platform), p95(&mut members), p95(&mut edits));
    }
}

// Complement the original sparse-note baseline: every skill has metadata and
// some rows carry large bodies. This exposes whole-table rewrite/COW costs.
#[tokio::test]
#[ignore = "synthetic dense metadata benchmark; run without concurrent builds"]
async fn synthetic_dense_metadata() {
    for count in [1_000usize, 10_000, 50_000] {
        let directory = tempfile::tempdir().unwrap();
        let store = crate::storage::store::Store::open(directory.path()).unwrap();
        store.write(|data| {
            for index in 0..count {
                let id = format!("fixture-{index}");
                let notes = if index % 1000 == 0 { "x".repeat(64 * 1024) } else { "x".repeat(128) };
                data.table_mut("skill_metadata").insert(id.clone(), serde_json::json!({
                    "skill_id": id, "notes": notes, "tags": "[\"fixture\"]", "updated_at": "2026-01-01"
                }));
            }
            Ok(())
        }).await.unwrap();
        let metadata_refs = || {
            let manifest: serde_json::Value = serde_json::from_slice(
                &std::fs::read(directory.path().join("store/manifest.json")).unwrap(),
            )
            .unwrap();
            if let Some(shards) = manifest["shards"]["skill_metadata"].as_object() {
                shards
                    .values()
                    .map(|id| id.as_str().unwrap().to_owned())
                    .collect::<std::collections::BTreeSet<_>>()
            } else {
                [manifest["tables"]["skill_metadata"]
                    .as_str()
                    .unwrap()
                    .to_owned()]
                .into_iter()
                .collect()
            }
        };
        let initial_refs = metadata_refs();
        let mut edits = Vec::new();
        for index in 0..20 {
            let start = Instant::now();
            db::upsert_skill_metadata(
                &store,
                "fixture-1",
                Some(&format!("Synthetic edit {index}")),
                &["fixture".into()],
            )
            .await
            .unwrap();
            edits.push(start.elapsed().as_micros());
        }
        let edit_min_us = *edits.iter().min().unwrap();
        let edit_max_us = *edits.iter().max().unwrap();
        edits.sort_unstable();
        let edit_median_us = edits[edits.len() / 2];
        let new_refs: Vec<_> = metadata_refs().difference(&initial_refs).cloned().collect();
        let edited_object_bytes: u64 = new_refs
            .iter()
            .map(|id| {
                std::fs::metadata(
                    directory
                        .path()
                        .join("store/objects")
                        .join(format!("{id}.json")),
                )
                .unwrap()
                .len()
            })
            .sum();
        let equivalent_table_bytes = store
            .read(|data| {
                Ok(serde_json::to_vec(data.table("skill_metadata"))
                    .unwrap()
                    .len())
            })
            .unwrap();
        assert_eq!(
            new_refs.len(),
            1,
            "single-note edit must replace just one metadata object"
        );
        if count == 50_000 {
            assert!(edited_object_bytes < equivalent_table_bytes as u64 / 100);
        }
        drop(store);
        let mut reopens = Vec::new();
        for _ in 0..5 {
            let start = Instant::now();
            let store = crate::storage::store::Store::open(directory.path()).unwrap();
            store
                .read(|data| {
                    assert_eq!(data.table("skill_metadata").len(), count);
                    assert_eq!(
                        data.table("skill_metadata")["fixture-1"]["notes"],
                        "Synthetic edit 19"
                    );
                    assert_eq!(
                        data.table("skill_metadata")["fixture-0"]["notes"]
                            .as_str()
                            .unwrap()
                            .len(),
                        64 * 1024
                    );
                    Ok(())
                })
                .unwrap();
            reopens.push(start.elapsed().as_micros());
        }
        println!(
            "STORAGE_DENSE count={count} edit_p95_us={} reopen_p95_us={} edited_object_bytes={edited_object_bytes} equivalent_table_bytes={equivalent_table_bytes} edit_min_us={edit_min_us} edit_median_us={edit_median_us} edit_max_us={edit_max_us}",
            p95(&mut edits),
            p95(&mut reopens)
        );
    }
}

#[tokio::test]
#[ignore = "synthetic v1 upgrade benchmark; run without concurrent builds"]
async fn synthetic_v1_dense_upgrade() {
    use serde_json::json;
    use sha2::{Digest, Sha256};
    let directory = tempfile::tempdir().unwrap();
    drop(crate::storage::store::Store::open(directory.path()).unwrap());
    let root = directory.path().join("store");
    let mut rows = std::collections::BTreeMap::new();
    for i in 0..50_000 {
        let id = format!("fixture-{i}");
        let notes = "x".repeat(if i % 1000 == 0 { 64 * 1024 } else { 128 });
        rows.insert(
            id.clone(),
            json!({"skill_id":id,"notes":notes,"tags":"[\"fixture\"]","updated_at":"2026-01-01"}),
        );
    }
    let bytes = serde_json::to_vec(&rows).unwrap();
    let id = format!("{:x}", Sha256::digest(&bytes));
    // Construct exactly the legacy format in a disposable directory, before
    // timing. Never open real application data for performance tests.
    std::fs::write(root.join("objects").join(format!("{id}.json")), bytes).unwrap();
    let manifest =
        serde_json::to_vec(&json!({"version":1,"revision":1,"tables":{"skill_metadata":id}}))
            .unwrap();
    std::fs::write(root.join("manifest.json"), &manifest).unwrap();
    std::fs::write(root.join("manifests/1.json"), &manifest).unwrap();
    drop(rows);
    let start = Instant::now();
    let store = crate::storage::store::Store::open(directory.path()).unwrap();
    let open_us = start.elapsed().as_micros();
    assert_eq!(std::fs::read(root.join("manifest.json")).unwrap(), manifest);
    let start = Instant::now();
    db::upsert_skill_metadata(&store, "fixture-1", Some("converted"), &["fixture".into()])
        .await
        .unwrap();
    let upgrade_us = start.elapsed().as_micros();
    let start = Instant::now();
    db::upsert_skill_metadata(
        &store,
        "fixture-1",
        Some("incremental"),
        &["fixture".into()],
    )
    .await
    .unwrap();
    let next_edit_us = start.elapsed().as_micros();
    drop(store);
    let store = crate::storage::store::Store::open(directory.path()).unwrap();
    assert_eq!(
        db::get_skill_metadata(&store, "fixture-1")
            .await
            .unwrap()
            .unwrap()
            .notes
            .as_deref(),
        Some("incremental")
    );
    assert_eq!(
        store
            .read(|data| Ok(data.table("skill_metadata").len()))
            .unwrap(),
        50_000
    );
    println!("STORAGE_UPGRADE count=50000 open_us={open_us} first_edit_us={upgrade_us} next_edit_us={next_edit_us}");
}

#[tokio::test]
#[ignore = "synthetic selected batch tags; run without concurrent builds"]
async fn synthetic_selected_batch_tags() {
    for count in [1_000usize, 10_000, 50_000] {
        let directory = tempfile::tempdir().unwrap();
        let store = crate::storage::store::Store::open(directory.path()).unwrap();
        store
            .write(|d| {
                for i in 0..count {
                    let id = format!("fixture-{i}");
                    db::put(
                        d,
                        "skills",
                        id.clone(),
                        &db::Skill {
                            id,
                            name: format!("Skill {i}"),
                            description: None,
                            file_path: format!("/synthetic/{i}/SKILL.md"),
                            canonical_path: None,
                            is_central: true,
                            source: None,
                            content: None,
                            scanned_at: "2026-01-01".into(),
                        },
                    )?;
                }
                Ok(())
            })
            .await
            .unwrap();
        let selected: Vec<_> = (0..count)
            .step_by(2)
            .map(|i| format!("fixture-{i}"))
            .collect();
        let start = Instant::now();
        let changed = db::metadata::change_tag(&store, "batch", Some(selected.clone()), false)
            .await
            .unwrap();
        let add_us = start.elapsed().as_micros();
        assert_eq!(changed.len(), count / 2);
        let start = Instant::now();
        let unchanged = db::metadata::change_tag(&store, "BATCH", Some(selected.clone()), false)
            .await
            .unwrap();
        let noop_us = start.elapsed().as_micros();
        assert!(unchanged.is_empty());
        drop(store);
        let store = crate::storage::store::Store::open(directory.path()).unwrap();
        assert_eq!(
            store.read(|d| Ok(d.table("skill_metadata").len())).unwrap(),
            count / 2
        );
        assert!(db::get_skill_metadata(&store, "fixture-1")
            .await
            .unwrap()
            .is_none());
        let start = Instant::now();
        let removed = db::metadata::change_tag(&store, "BATCH", Some(selected), true)
            .await
            .unwrap();
        let remove_us = start.elapsed().as_micros();
        assert_eq!(removed.len(), count / 2);
        assert!(removed.iter().all(|m| m.tags.is_empty()));
        println!(
            "STORAGE_BATCH count={count} add_us={add_us} noop_us={noop_us} remove_us={remove_us}"
        );
    }
}

#[tokio::test]
#[ignore = "small save distribution; run without concurrent builds"]
async fn synthetic_small_save_distribution() {
    use sha2::{Digest, Sha256};
    for count in [1usize, 1_000] {
        let directory = tempfile::tempdir().unwrap();
        let store = crate::storage::store::Store::open(directory.path()).unwrap();
        store
            .write(|d| {
                for i in 0..count {
                    let id = format!("fixture-{i}");
                    d.table_mut("skill_metadata").insert(
                        id.clone(),
                        serde_json::json!({
                    "skill_id":id,"notes":"x".repeat(if i % 1000 == 0 {64 * 1024} else {128}),
                    "tags":"[\"fixture\"]","updated_at":"2026-01-01"}),
                    );
                }
                Ok(())
            })
            .await
            .unwrap();
        let mut cpu = Vec::new();
        let mut saves = Vec::new();
        for i in 0..100 {
            // Independent encoding/hash probe: no filesystem or durability work.
            let start = Instant::now();
            store
                .read(|d| {
                    let bytes = serde_json::to_vec(d.table("skill_metadata")).unwrap();
                    std::hint::black_box(Sha256::digest(&bytes));
                    Ok(())
                })
                .unwrap();
            cpu.push(start.elapsed().as_micros());
            let start = Instant::now();
            db::upsert_skill_metadata(
                &store,
                "fixture-0",
                Some(&format!("{}-{i}", "x".repeat(64 * 1024))),
                &["fixture".into()],
            )
            .await
            .unwrap();
            saves.push(start.elapsed().as_micros());
        }
        saves.sort_unstable();
        cpu.sort_unstable();
        println!("STORAGE_SMALL count={count} save_min_us={} save_median_us={} save_p95_us={} save_max_us={} encode_hash_median_us={} encode_hash_p95_us={}",
            saves[0], saves[50], saves[94], saves[99], cpu[50], cpu[94]);
        drop(store);
        let reopened = crate::storage::store::Store::open(directory.path()).unwrap();
        assert_eq!(
            db::get_skill_metadata(&reopened, "fixture-0")
                .await
                .unwrap()
                .unwrap()
                .notes
                .unwrap(),
            format!("{}-99", "x".repeat(64 * 1024))
        );
    }
}
