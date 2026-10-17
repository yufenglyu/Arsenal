use super::store::Store;
use serde_json::json;
use std::{process::{Command, Stdio}, time::{Duration, Instant}};

#[test]
#[ignore = "subprocess helper; invoked only by acknowledged_commit_survives_process_kill"]
fn committed_child() {
    let Some(root) = std::env::var_os("SKILLSHUB_SYNTHETIC_CRASH_ROOT") else { return };
    let root = std::path::PathBuf::from(root);
    let store = Store::open(&root).unwrap();
    tokio::runtime::Runtime::new().unwrap().block_on(store.write(|data| {
        data.table_mut("skills").insert("fixture".into(), json!({"id":"fixture"}));
        data.table_mut("collection_skills").insert("member".into(), json!({"skill_id":"fixture"}));
        Ok(())
    })).unwrap();
    std::fs::write(root.join("acknowledged"), b"ready").unwrap();
    loop { std::thread::park(); }
}

#[test]
fn acknowledged_commit_survives_process_kill() {
    let directory = tempfile::tempdir().unwrap();
    let mut child = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "storage::durability_tests::committed_child", "--ignored"])
        .env("SKILLSHUB_SYNTHETIC_CRASH_ROOT", directory.path())
        .stdout(Stdio::null()).stderr(Stdio::null()).spawn().unwrap();
    let deadline = Instant::now() + Duration::from_secs(15);
    while !directory.path().join("acknowledged").exists() && Instant::now() < deadline {
        if child.try_wait().unwrap().is_some() { break; }
        std::thread::sleep(Duration::from_millis(10));
    }
    let acknowledged = directory.path().join("acknowledged").exists();
    let _ = child.kill();
    child.wait().unwrap();
    assert!(acknowledged, "synthetic child did not finish its commit");
    let store = Store::open(directory.path()).unwrap();
    store.read(|data| {
        assert_eq!(data.table("skills")["fixture"]["id"], "fixture");
        assert_eq!(data.table("collection_skills")["member"]["skill_id"], "fixture");
        Ok(())
    }).unwrap();
}
