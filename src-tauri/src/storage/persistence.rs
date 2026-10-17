use super::store::{Data, Table};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, File},
    io::Write,
    path::Path,
    sync::Arc,
};
#[derive(Serialize, Deserialize)]
struct Manifest {
    version: u32,
    revision: u64,
    tables: BTreeMap<String, String>,
}
fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn durable(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let mut file = File::create(path).map_err(|e| e.to_string())?;
    file.write_all(bytes)
        .and_then(|_| file.sync_all())
        .map_err(|e| e.to_string())
}
#[cfg(windows)]
fn replace(source: &Path, target: &Path) -> Result<(), String> {
    use std::os::windows::ffi::OsStrExt;
    #[link(name = "kernel32")]
    extern "system" {
        fn MoveFileExW(source: *const u16, target: *const u16, flags: u32) -> i32;
    }
    let source: Vec<u16> = source.as_os_str().encode_wide().chain(Some(0)).collect();
    let target: Vec<u16> = target.as_os_str().encode_wide().chain(Some(0)).collect();
    if unsafe { MoveFileExW(source.as_ptr(), target.as_ptr(), 1 | 8) } == 0 {
        Err(std::io::Error::last_os_error().to_string())
    } else {
        Ok(())
    }
}
#[cfg(not(windows))]
fn replace(source: &Path, target: &Path) -> Result<(), String> {
    fs::rename(source, target).map_err(|e| e.to_string())
}
pub(super) fn load(root: &Path) -> Result<(Data, u64), String> {
    let path = root.join("manifest.json");
    if !path.exists() {
        for directory in [root.join("objects"), root.join("manifests")] {
            if fs::read_dir(directory)
                .map_err(|e| e.to_string())?
                .next()
                .is_some()
            {
                return Err(
                    "Missing manifest with existing storage history; recovery required".into(),
                );
            }
        }
        return Ok((Data::default(), 0));
    }
    let manifest: Manifest = serde_json::from_slice(&fs::read(path).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    if manifest.version != 1 {
        return Err("Unsupported JSON store version".into());
    }
    // Validate every reference before forming paths. Shared immutable objects are
    // decoded once, then shared with the existing table-level copy-on-write.
    let mut objects = Vec::new();
    let mut total_bytes = 0u64;
    for id in manifest.tables.values().collect::<BTreeSet<_>>() {
        if id.len() != 64 || !id.bytes().all(|c| c.is_ascii_hexdigit()) {
            return Err("Invalid object identifier".into());
        }
        let size = fs::metadata(root.join("objects").join(format!("{id}.json")))
            .map_err(|e| e.to_string())?
            .len();
        total_bytes = total_bytes.saturating_add(size);
        objects.push((id.clone(), size));
    }
    let workers = if total_bytes >= 1024 * 1024 {
        std::thread::available_parallelism()
            .map_or(1, usize::from)
            .min(4)
            .min(objects.len())
    } else {
        1
    };
    let loaded = if workers <= 1 {
        load_objects(root, &objects)?
    } else {
        // Balance by bytes, not table count: skills and installations dominate.
        objects.sort_by_key(|(_, size)| std::cmp::Reverse(*size));
        let mut batches = vec![(Vec::new(), 0u64); workers];
        for object in objects {
            let batch = batches.iter_mut().min_by_key(|(_, size)| *size).unwrap();
            batch.1 = batch.1.saturating_add(object.1);
            batch.0.push(object);
        }
        std::thread::scope(|scope| -> Result<BTreeMap<String, Arc<Table>>, String> {
            let mut handles = Vec::new();
            for (batch, _) in &batches {
                handles.push(scope.spawn(|| load_objects(root, batch)));
            }
            // Join every worker before returning any error; no partially loaded
            // state is visible and no worker outlives the store's exclusive lock.
            let results: Vec<_> = handles.into_iter().map(|handle| handle.join()).collect();
            let mut loaded = BTreeMap::new();
            for result in results {
                loaded.extend(result.map_err(|_| "Object loader panicked")??);
            }
            Ok(loaded)
        })?
    };
    let mut data = Data::default();
    for (name, id) in manifest.tables {
        let rows = loaded.get(&id).ok_or("Missing loaded object")?.clone();
        data.refs.insert(name.clone(), id);
        data.tables.insert(name, rows);
    }
    Ok((data, manifest.revision))
}
fn load_objects(
    root: &Path,
    objects: &[(String, u64)],
) -> Result<BTreeMap<String, Arc<Table>>, String> {
    let mut loaded = BTreeMap::new();
    for (id, _) in objects {
        let bytes =
            fs::read(root.join("objects").join(format!("{id}.json"))).map_err(|e| e.to_string())?;
        if hash(&bytes) != *id {
            return Err("JSON object checksum mismatch".into());
        }
        let rows = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
        loaded.insert(id.clone(), Arc::new(rows));
    }
    Ok(loaded)
}

fn write_object(root: &Path, id: &str, bytes: &[u8]) -> Result<(), String> {
    let path = root.join("objects").join(format!("{id}.json"));
    if path.exists() {
        if fs::read(&path).map_err(|e| e.to_string())? != bytes {
            return Err("Existing JSON object is corrupt".into());
        }
    } else {
        let pending = root.join("objects").join(format!("{id}.pending"));
        durable(&pending, bytes)?;
        replace(&pending, &path)?;
    }
    Ok(())
}

pub(super) fn save(root: &Path, data: &mut Data, revision: u64) -> Result<(), String> {
    save_prepared(root, data, revision, || Ok(()))
}

fn save_prepared(
    root: &Path,
    data: &mut Data,
    revision: u64,
    before_publish: impl FnOnce() -> Result<(), String>,
) -> Result<(), String> {
    let staging = root.join("manifest.pending");
    let tables = std::thread::scope(|scope| -> Result<BTreeMap<String, String>, String> {
        let mut tables = BTreeMap::new();
        let mut scheduled = BTreeSet::new();
        let mut objects = std::collections::VecDeque::new();
        for (name, rows) in &data.tables {
            if let Some(id) = data.refs.get(name) {
                tables.insert(name.clone(), id.clone());
                continue;
            }
            let bytes = serde_json::to_vec(rows.as_ref()).map_err(|e| e.to_string())?;
            let id = hash(&bytes);
            // Identical tables share a content-addressed file. Only one worker
            // may write that file; cap queued writers at two (plus the
            // buffer currently being serialized on the calling thread).
            if scheduled.insert(id.clone()) {
                if objects.len() == 2 {
                    let oldest: std::thread::ScopedJoinHandle<'_, Result<(), String>> =
                        objects.pop_front().unwrap();
                    oldest.join().map_err(|_| "Object writer panicked")??;
                }
                let object_id = id.clone();
                objects.push_back(scope.spawn(move || write_object(root, &object_id, &bytes)));
            }
            tables.insert(name.clone(), id);
        }
        let bytes = serde_json::to_vec_pretty(&Manifest {
            version: 1,
            revision,
            tables: tables.clone(),
        })
        .map_err(|e| e.to_string())?;
        let history = root.join("manifests").join(format!("{revision}.json"));
        // History and staging remain independent copies. Every object and both
        // manifests must finish sync_all before the sole publication point.
        let manifest_results = std::thread::scope(|scope| {
            let pending = scope.spawn(|| durable(&staging, &bytes));
            let history_result = durable(&history, &bytes);
            (history_result, pending.join())
        });
        let object_results: Vec<_> = objects.into_iter().map(|handle| handle.join()).collect();
        for result in object_results {
            result.map_err(|_| "Object writer panicked")??;
        }
        manifest_results.0?;
        manifest_results
            .1
            .map_err(|_| "Manifest writer panicked")??;
        Ok(tables)
    })?;
    #[cfg(unix)]
    for directory in [root.join("objects"), root.join("manifests")] {
        File::open(directory)
            .and_then(|f| f.sync_all())
            .map_err(|e| e.to_string())?;
    }
    before_publish()?;
    replace(&staging, &root.join("manifest.json"))?;
    #[cfg(unix)]
    {
        File::open(root)
            .and_then(|f| f.sync_all())
            .map_err(|e| format!("COMMIT_UNCERTAIN: {e}"))?;
    }
    data.refs = tables;
    Ok(())
}

// Only named, valid historical manifests are candidates. Unknown files stop cleanup.
pub(super) fn collect(root: &Path, current: u64) -> Result<(), String> {
    let mut histories = Vec::new();
    let mut checked_objects = BTreeSet::new();
    for entry in fs::read_dir(root.join("manifests")).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path();
        let revision = path
            .file_stem()
            .and_then(|s| s.to_str())
            .and_then(|s| s.parse::<u64>().ok())
            .ok_or("Unknown history file")?;
        if path.extension().and_then(|s| s.to_str()) != Some("json") {
            return Err("Unknown history file".into());
        }
        let manifest: Manifest =
            serde_json::from_slice(&fs::read(&path).map_err(|e| e.to_string())?)
                .map_err(|e| e.to_string())?;
        if manifest.version != 1 || manifest.revision != revision || revision > current {
            return Err("Unrecognized recovery history".into());
        }
        if manifest.tables.values().any(|id| {
            id.len() != 64
                || !id.bytes().all(|b| b.is_ascii_hexdigit())
                || (checked_objects.insert(id.clone())
                    && !root.join("objects").join(format!("{id}.json")).is_file())
        }) {
            return Err("Invalid recovery object reference".into());
        }
        histories.push((revision, path, manifest));
    }
    if !histories.iter().any(|h| h.0 == current) {
        return Err("Current history missing".into());
    }
    histories.sort_by_key(|h| std::cmp::Reverse(h.0));
    let mut live = std::collections::BTreeSet::new();
    for (_, _, m) in histories.iter().take(2) {
        live.extend(m.tables.values().cloned());
    }
    for (_, path, _) in histories.iter().skip(2) {
        fs::remove_file(path).map_err(|e| e.to_string())?;
    }
    for entry in fs::read_dir(root.join("objects")).map_err(|e| e.to_string())? {
        let path = entry.map_err(|e| e.to_string())?.path();
        let id = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
        if path.extension().and_then(|s| s.to_str()) == Some("json")
            && id.len() == 64
            && id.bytes().all(|b| b.is_ascii_hexdigit())
            && !live.contains(id)
        {
            fs::remove_file(&path).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::store::Store;
    use serde_json::json;

    #[tokio::test]
    async fn prepared_commit_is_invisible_until_publish_and_retryable() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path()).unwrap();
        store
            .write(|data| {
                data.table_mut("a").insert("x".into(), json!("old"));
                data.table_mut("b").insert("x".into(), json!("old"));
                Ok(())
            })
            .await
            .unwrap();
        drop(store);
        let root = dir.path().join("store");
        let (mut data, revision) = load(&root).unwrap();
        let published = fs::read(root.join("manifest.json")).unwrap();
        for name in ["a", "b", "c", "d", "e"] {
            data.table_mut(name).insert("x".into(), json!("new"));
        }
        assert!(save_prepared(&root, &mut data, revision + 1, || Err(
            "stop before publication".into()
        ))
        .is_err());
        assert_eq!(fs::read(root.join("manifest.json")).unwrap(), published);
        let (old, old_revision) = load(&root).unwrap();
        assert_eq!(old_revision, revision);
        assert_eq!(old.table("a")["x"], "old");
        assert!(old.table("c").is_empty());
        save(&root, &mut data, revision + 1).unwrap();
        let (new, _) = load(&root).unwrap();
        for name in ["a", "b", "c", "d", "e"] {
            assert_eq!(new.table(name)["x"], "new");
        }
        // A later publication cannot change the independent historical copy.
        assert_eq!(
            fs::read(root.join("manifests").join(format!("{revision}.json"))).unwrap(),
            published
        );
    }

    #[tokio::test]
    async fn object_write_failure_never_publishes_prepared_manifests() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path()).unwrap();
        store
            .write(|data| {
                data.table_mut("a").insert("x".into(), json!("old"));
                Ok(())
            })
            .await
            .unwrap();
        drop(store);
        let root = dir.path().join("store");
        let (mut data, revision) = load(&root).unwrap();
        data.table_mut("a").insert("x".into(), json!("new"));
        let id = hash(&serde_json::to_vec(data.table("a")).unwrap());
        let blocked = root.join("objects").join(format!("{id}.pending"));
        fs::create_dir(&blocked).unwrap();
        assert!(save(&root, &mut data, revision + 1).is_err());
        assert_eq!(load(&root).unwrap().0.table("a")["x"], "old");
        fs::remove_dir(blocked).unwrap();
        save(&root, &mut data, revision + 1).unwrap();
        assert_eq!(load(&root).unwrap().0.table("a")["x"], "new");
    }

    #[tokio::test]
    async fn shared_large_objects_remain_cow_and_corruption_fails_closed() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path()).unwrap();
        store
            .write(|data| {
                for name in ["a", "b", "c", "d", "e"] {
                    // Distinct large objects force bounded parallel loading.
                    data.table_mut(name)
                        .insert("body".into(), json!(name.repeat(300_000)));
                }
                let same = data.table("a").clone();
                *data.table_mut("shared") = same;
                Ok(())
            })
            .await
            .unwrap();
        drop(store);
        let root = dir.path().join("store");
        let (mut data, _) = load(&root).unwrap();
        assert!(Arc::ptr_eq(&data.tables["a"], &data.tables["shared"]));
        data.table_mut("shared").clear();
        assert_eq!(data.table("a")["body"].as_str().unwrap().len(), 300_000);
        let corrupt = root
            .join("objects")
            .join(format!("{}.json", data.refs["e"]));
        fs::write(&corrupt, b"{}").unwrap();
        assert!(Store::open(dir.path()).is_err());
    }

    #[tokio::test]
    async fn invalid_reference_and_missing_object_fail_before_loading() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path()).unwrap();
        store
            .write(|data| {
                data.table_mut("a").insert("x".into(), json!(1));
                Ok(())
            })
            .await
            .unwrap();
        drop(store);
        let root = dir.path().join("store");
        let path = root.join("manifest.json");
        let mut manifest: Manifest = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        let id = manifest.tables["a"].clone();
        fs::remove_file(root.join("objects").join(format!("{id}.json"))).unwrap();
        assert!(load(&root).is_err());
        manifest.tables.insert("a".into(), "../outside".into());
        fs::write(path, serde_json::to_vec(&manifest).unwrap()).unwrap();
        assert!(load(&root)
            .err()
            .unwrap()
            .contains("Invalid object identifier"));
    }
    #[test]
    #[ignore = "subprocess helper for prepared_commit_survives_process_kill"]
    fn prepared_child() {
        let Some(path) = std::env::var_os("SKILLSHUB_SYNTHETIC_PREPARE_ROOT") else {
            return;
        };
        let directory = std::path::PathBuf::from(path);
        let _store = Store::open(&directory).unwrap();
        let root = directory.join("store");
        let (mut data, revision) = load(&root).unwrap();
        for table in ["a", "b"] {
            data.table_mut(table)
                .insert("x".into(), json!("unpublished"));
        }
        save_prepared(&root, &mut data, revision + 1, || {
            fs::write(directory.join("prepared"), b"ready").unwrap();
            loop {
                std::thread::park();
            }
        })
        .unwrap();
    }

    #[tokio::test]
    async fn prepared_commit_survives_process_kill() {
        use std::process::{Command, Stdio};
        use std::time::{Duration, Instant};
        let directory = tempfile::tempdir().unwrap();
        let store = Store::open(directory.path()).unwrap();
        store
            .write(|data| {
                for table in ["a", "b"] {
                    data.table_mut(table).insert("x".into(), json!("committed"));
                }
                Ok(())
            })
            .await
            .unwrap();
        drop(store);
        let mut child = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "storage::persistence::tests::prepared_child",
                "--ignored",
            ])
            .env("SKILLSHUB_SYNTHETIC_PREPARE_ROOT", directory.path())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(15);
        while !directory.path().join("prepared").exists() && Instant::now() < deadline {
            if child.try_wait().unwrap().is_some() {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        let prepared = directory.path().join("prepared").exists();
        let _ = child.kill();
        child.wait().unwrap();
        assert!(
            prepared,
            "synthetic child did not finish preparing the commit"
        );
        let store = Store::open(directory.path()).unwrap();
        store
            .read(|data| {
                assert_eq!(data.table("a")["x"], "committed");
                assert_eq!(data.table("b")["x"], "committed");
                Ok(())
            })
            .unwrap();
        store
            .write(|data| {
                data.table_mut("a").insert("x".into(), json!("retry"));
                Ok(())
            })
            .await
            .unwrap();
        drop(store);
        assert_eq!(
            Store::open(directory.path())
                .unwrap()
                .read(|data| Ok(data.table("a")["x"].clone()))
                .unwrap(),
            "retry"
        );
    }
}
