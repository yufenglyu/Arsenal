//! Opt-in validation: source data is read only; edits use a disposable store.
use crate::{
    db,
    storage::store::{Data, Store},
};

#[tokio::test]
#[ignore = "requires ARSENAL_VERIFY_JSON_DIR; never edits the supplied store"]
async fn verify_migrated_data_on_disposable_store() {
    let path =
        std::env::var_os("ARSENAL_VERIFY_JSON_DIR").expect("verification directory required");
    let source = Store::open(std::path::Path::new(&path)).expect("source open failed");
    let data: Data = source
        .read(|d| {
            // Roundtrip intentionally discards object references belonging to source.
            serde_json::from_value(serde_json::to_value(d).map_err(|_| "serialize")?)
                .map_err(|_| "deserialize".into())
        })
        .expect("snapshot failed");
    drop(source);
    let directory = tempfile::tempdir().unwrap();
    let store = Store::open(directory.path()).unwrap();
    store
        .write(|d| {
            *d = data;
            Ok(())
        })
        .await
        .unwrap();
    let skills: Vec<db::Skill> = store
        .read(|d| db::rows(d, "skills"))
        .expect("skill decode failed");
    let _: Vec<db::SkillInstallation> = store
        .read(|d| db::rows(d, "skill_installations"))
        .expect("installation decode failed");
    let _: Vec<db::SkillMetadata> = store
        .read(|d| db::rows(d, "skill_metadata"))
        .expect("metadata decode failed");
    for skill in &skills {
        assert!(db::get_skill_by_id(&store, &skill.id)
            .await
            .unwrap()
            .is_some());
        db::get_skill_installations(&store, &skill.id)
            .await
            .unwrap();
    }
    db::get_central_skills(&store).await.unwrap();
    db::get_resource_library_skills(&store).await.unwrap();
    db::get_all_skill_sources(&store).await.unwrap();
    db::get_all_skill_source_syncs(&store).await.unwrap();
    for agent in db::get_all_agents(&store).await.unwrap() {
        db::get_skills_for_agent(&store, &agent.id).await.unwrap();
        db::get_agent_skill_observations(&store, &agent.id)
            .await
            .unwrap();
    }
    for collection in db::get_all_collections(&store).await.unwrap() {
        db::backup::collection_details(&store, &collection.id).unwrap();
    }
    db::backup::memberships(&store).unwrap();
    db::backup::installations(&store).unwrap();
    db::backup::stats(&store).unwrap();
    db::backup::directories(&store).unwrap();
    db::metadata::folder_notes(&store).await.unwrap();
    let notebooks = db::notebooks::list(&store).await.unwrap();
    if let Some(mut note) = notebooks.into_iter().next() {
        note.notebook = "Disposable verification edit".into();
        db::notebooks::put(&store, &note).await.unwrap();
    }
    let id = uuid::Uuid::new_v4().to_string();
    db::upsert_skill_metadata(
        &store,
        &id,
        Some("Disposable verification"),
        &["test".into()],
    )
    .await
    .unwrap();
    let collection = db::create_collection(&store, "Disposable verification", None)
        .await
        .unwrap();
    db::add_skill_to_collection(&store, &collection.id, &id)
        .await
        .unwrap();
    drop(store);
    let store = Store::open(directory.path()).unwrap();
    assert!(db::get_skill_metadata(&store, &id).await.unwrap().is_some());
    assert!(db::backup::memberships(&store)
        .unwrap()
        .iter()
        .any(|r| r.collection_id == collection.id && r.skill_id == id));
    println!("Native read, isolated edit and reopen verification passed");
}
