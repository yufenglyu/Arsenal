use super::store::{Data, Table};
use super::table::{bucket, SHARDED_TABLE, SHARD_THRESHOLD};
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
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    shards: BTreeMap<String, BTreeMap<u8, String>>,
}
impl Manifest {
    fn validate(&self) -> Result<(), String> {
        if !matches!(self.version, 1 | 2) || (self.version == 1 && !self.shards.is_empty()) {
            return Err("Unsupported JSON store version".into());
        }
        if self
            .shards
            .keys()
            .any(|name| name != SHARDED_TABLE || self.tables.contains_key(name))
        {
            return Err("Invalid sharded table".into());
        }
        Ok(())
    }
    fn object_ids(&self) -> impl Iterator<Item = &String> {
        self.tables
            .values()
            .chain(self.shards.values().flat_map(|shards| shards.values()))
    }
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
    manifest.validate()?;
    // Validate every reference before forming paths. Shared immutable objects are
    // decoded once, then shared with the existing table-level copy-on-write.
    let mut objects = Vec::new();
    let mut total_bytes = 0u64;
    for id in manifest.object_ids().collect::<BTreeSet<_>>() {
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
    let mut loaded = if workers <= 1 {
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
    let mut shard_objects = BTreeMap::new();
    for (name, refs) in manifest.shards {
        let mut table = Table::default();
        table.make_sharded();
        for (slot, id) in &refs {
            if let Some(rows) = loaded.remove(id) {
                shard_objects.insert(id.clone(), Arc::new(Arc::unwrap_or_clone(rows).into_rows()));
            }
            let rows = shard_objects.get(id).ok_or("Missing shard object")?;
            if rows.keys().any(|key| bucket(key) != *slot) {
                return Err("Row stored in incorrect shard".into());
            }
            table.buckets.insert(*slot, rows.clone());
        }
        table.refs = refs;
        data.tables.insert(name, Arc::new(table));
    }
    drop(loaded);
    // v1 stays on disk until the first metadata edit; convert its in-memory
    // representation once, without rewriting data during open.
    if let Some(table) = data.tables.get_mut(SHARDED_TABLE) {
        if table.len() >= SHARD_THRESHOLD {
            Arc::make_mut(table).make_sharded();
        }
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

// Encoding is independent of publication. Keep duplicate-object scheduling on
// the caller so equal-content tables never race on the same pending file.
fn encode_table(rows: &Table) -> Result<(Vec<u8>, String), String> {
    let bytes = serde_json::to_vec(rows).map_err(|e| e.to_string())?;
    let id = hash(&bytes);
    Ok((bytes, id))
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
    let existing_objects = object_files(root)?;
    let (tables, shards) = std::thread::scope(|scope| {
        let mut tables = BTreeMap::new();
        let mut shards = BTreeMap::new();
        let mut scheduled = BTreeSet::new();
        let mut objects = std::collections::VecDeque::new();
        // Only multiple dirty, large flat tables benefit from parallel CPU work.
        // Small saves and sharded metadata retain the existing pipeline. At most
        // two encoders and two object writers are in flight, regardless of size.
        let large: Vec<_> = data
            .tables
            .iter()
            .filter(|(name, rows)| {
                !rows.sharded
                    && rows.len() >= SHARD_THRESHOLD
                    && !data
                        .refs
                        .get(*name)
                        .is_some_and(|id| existing_objects.contains_key(id))
            })
            .collect();
        let parallel = large.len() >= 2;
        let mut large = large.into_iter().filter(|_| parallel);
        let mut encoders = std::collections::VecDeque::new();
        for (name, rows) in large.by_ref().take(2) {
            encoders.push_back((name, scope.spawn(move || encode_table(rows))));
        }
        for (name, rows) in &data.tables {
            if let Some(id) = data
                .refs
                .get(name)
                .filter(|id| existing_objects.contains_key(*id))
            {
                tables.insert(name.clone(), id.clone());
                continue;
            }
            let mut pieces = Vec::new();
            if rows.sharded {
                let mut refs = rows.refs.clone();
                // An in-memory table snapshot may outlive its disk history.
                // Rebuild collected objects instead of publishing stale refs.
                refs.retain(|_, id| existing_objects.contains_key(id));
                for slot in rows.buckets.keys() {
                    if !refs.contains_key(slot) {
                        // Keep only one serialization buffer on the caller.
                        pieces.push(Some(*slot));
                    }
                }
                shards.insert(name.clone(), refs);
            } else {
                pieces.push(None);
            }
            for slot in pieces {
                let (bytes, id) =
                    if slot.is_none() && encoders.front().is_some_and(|(next, _)| *next == name) {
                        let (_, encoder) = encoders.pop_front().unwrap();
                        let encoded = encoder.join().map_err(|_| "Object encoder panicked")??;
                        if let Some((next, rows)) = large.next() {
                            encoders.push_back((next, scope.spawn(move || encode_table(rows))));
                        }
                        encoded
                    } else if let Some(slot) = slot {
                        let bytes = serde_json::to_vec(rows.buckets[&slot].as_ref())
                            .map_err(|e| e.to_string())?;
                        let id = hash(&bytes);
                        (bytes, id)
                    } else {
                        encode_table(rows)?
                    };
                // Equal-content objects are written once, including empty shards.
                if scheduled.insert(id.clone()) {
                    if objects.len() == 2 {
                        let oldest: std::thread::ScopedJoinHandle<'_, Result<(), String>> =
                            objects.pop_front().unwrap();
                        oldest.join().map_err(|_| "Object writer panicked")??;
                    }
                    let object_id = id.clone();
                    objects.push_back(scope.spawn(move || write_object(root, &object_id, &bytes)));
                }
                match slot {
                    Some(slot) => {
                        shards.get_mut(name).unwrap().insert(slot, id);
                    }
                    None => {
                        tables.insert(name.clone(), id);
                    }
                }
            }
        }
        let bytes = serde_json::to_vec_pretty(&Manifest {
            version: if shards.is_empty() { 1 } else { 2 },
            revision,
            tables: tables.clone(),
            shards: shards.clone(),
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
        Ok::<_, String>((tables, shards))
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
    for (name, refs) in shards {
        Arc::make_mut(
            data.tables
                .get_mut(&name)
                .ok_or("Missing committed table")?,
        )
        .refs = refs;
    }
    data.dirty.clear();
    Ok(())
}

fn object_files(root: &Path) -> Result<BTreeMap<String, std::path::PathBuf>, String> {
    // Directory enumeration supplies file types on Windows without a separate
    // metadata call per shard. Still validate every history reference before
    // deleting anything; unknown files and non-regular entries are not deleted.
    let mut object_files = BTreeMap::new();
    for entry in fs::read_dir(root.join("objects")).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path();
        let id = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
        if path.extension().and_then(|s| s.to_str()) == Some("json")
            && id.len() == 64
            && id.bytes().all(|b| b.is_ascii_hexdigit())
            && entry.file_type().map_err(|e| e.to_string())?.is_file()
        {
            object_files.insert(id.to_owned(), path);
        }
    }
    Ok(object_files)
}

// Only named, valid historical manifests are candidates. Unknown files stop cleanup.
pub(super) fn collect(root: &Path, current: u64) -> Result<(), String> {
    let mut histories = Vec::new();
    let object_files = object_files(root)?;
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
        manifest.validate()?;
        if manifest.revision != revision || revision > current {
            return Err("Unrecognized recovery history".into());
        }
        if manifest.object_ids().any(|id| {
            id.len() != 64
                || !id.bytes().all(|b| b.is_ascii_hexdigit())
                || !object_files.contains_key(id)
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
        live.extend(m.object_ids().cloned());
    }
    for (_, path, _) in histories.iter().skip(2) {
        fs::remove_file(path).map_err(|e| e.to_string())?;
    }
    for (id, path) in object_files {
        if !live.contains(&id) {
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
    async fn parallel_large_encoding_deduplicates_and_keeps_atomic_publication() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path()).unwrap();
        store
            .write(|d| {
                d.table_mut("old").insert("key".into(), json!("old"));
                Ok(())
            })
            .await
            .unwrap();
        drop(store);
        let root = dir.path().join("store");
        let (mut data, revision) = load(&root).unwrap();
        let old_manifest = fs::read(root.join("manifest.json")).unwrap();
        // More than two candidates exercises replenishment. The first two have
        // identical content and must never write the same pending path in parallel.
        for name in ["a", "b", "c", "d", "e"] {
            let table = data.table_mut(name);
            for i in 0..SHARD_THRESHOLD {
                table.insert(
                    format!("key-{i}"),
                    json!({"value":i,"group":if name == "b" {"a"} else {name}}),
                );
            }
        }
        data.table_mut("small")
            .insert("key".into(), json!("also atomic"));
        let expected = serde_json::to_value(&data).unwrap();
        let e_id = encode_table(data.table("e")).unwrap().1;
        let blocked = root.join("objects").join(format!("{e_id}.pending"));
        fs::create_dir(&blocked).unwrap();
        assert!(save(&root, &mut data, revision + 1).is_err());
        assert_eq!(fs::read(root.join("manifest.json")).unwrap(), old_manifest);
        assert!(load(&root).unwrap().0.table("a").is_empty());
        fs::remove_dir(blocked).unwrap();
        assert!(save_prepared(&root, &mut data, revision + 1, || Err(
            "before publish".into()
        ))
        .is_err());
        assert_eq!(fs::read(root.join("manifest.json")).unwrap(), old_manifest);
        save(&root, &mut data, revision + 1).unwrap();
        let (loaded, _) = load(&root).unwrap();
        assert_eq!(serde_json::to_value(&loaded).unwrap(), expected);
        assert_eq!(loaded.refs["a"], loaded.refs["b"]);
        assert_eq!(
            fs::read(root.join("manifests").join(format!("{revision}.json"))).unwrap(),
            old_manifest
        );
        // An unchanged large table is reused, not scheduled as a new encoder job.
        data.table_mut("small").insert("key".into(), json!("next"));
        save(&root, &mut data, revision + 2).unwrap();
        assert_eq!(load(&root).unwrap().0.refs["a"], loaded.refs["a"]);
    }

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
        data.table_mut(SHARDED_TABLE).make_sharded();
        for table in ["a", "b", SHARDED_TABLE] {
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
                data.table_mut(SHARDED_TABLE).make_sharded();
                for table in ["a", "b", SHARDED_TABLE] {
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
                assert_eq!(data.table(SHARDED_TABLE)["x"], "committed");
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
    #[tokio::test]
    async fn metadata_edit_writes_only_one_bucket_and_preserves_snapshots() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path()).unwrap();
        store
            .write(|data| {
                data.table_mut(SHARDED_TABLE).make_sharded();
                for i in 0..2048 {
                    data.table_mut(SHARDED_TABLE)
                        .insert(format!("key-{i}"), json!({"notes": "old"}));
                }
                Ok(())
            })
            .await
            .unwrap();
        let root = dir.path().join("store");
        let before: Manifest =
            serde_json::from_slice(&fs::read(root.join("manifest.json")).unwrap()).unwrap();
        assert_eq!(before.version, 2);
        let snapshot = store.read(|data| Ok(data.clone())).unwrap();
        let slot = bucket("key-1");
        for note in ["first", "second", "third"] {
            store
                .write(|data| {
                    data.table_mut(SHARDED_TABLE).get_mut("key-1").unwrap()["notes"] = json!(note);
                    data.table_mut("related")
                        .insert("version".into(), json!(note));
                    Ok(())
                })
                .await
                .unwrap();
        }
        let after: Manifest =
            serde_json::from_slice(&fs::read(root.join("manifest.json")).unwrap()).unwrap();
        for (id, old_ref) in &before.shards[SHARDED_TABLE] {
            if *id == slot {
                assert_ne!(old_ref, &after.shards[SHARDED_TABLE][id]);
            } else {
                assert_eq!(old_ref, &after.shards[SHARDED_TABLE][id]);
            }
        }
        assert_eq!(snapshot.table(SHARDED_TABLE)["key-1"]["notes"], "old");
        let old_id = &before.shards[SHARDED_TABLE][&slot];
        assert!(!root.join("objects").join(format!("{old_id}.json")).exists());
        drop(store);
        let reopened = Store::open(dir.path()).unwrap();
        reopened
            .read(|data| {
                assert_eq!(data.table(SHARDED_TABLE).len(), 2048);
                assert_eq!(data.table(SHARDED_TABLE)["key-1"]["notes"], "third");
                assert_eq!(data.table("related")["version"], "third");
                let exported = serde_json::to_vec(data).unwrap();
                let restored: Data = serde_json::from_slice(&exported).unwrap();
                assert_eq!(serde_json::to_vec(&restored).unwrap(), exported);
                Ok(())
            })
            .unwrap();
        reopened
            .write(|data| {
                *data.table_mut(SHARDED_TABLE) = snapshot.table(SHARDED_TABLE).clone();
                Ok(())
            })
            .await
            .unwrap();
        drop(reopened);
        assert_eq!(
            Store::open(dir.path())
                .unwrap()
                .read(|data| Ok(data.table(SHARDED_TABLE)["key-1"]["notes"].clone()))
                .unwrap(),
            "old"
        );
    }

    #[tokio::test]
    async fn v1_metadata_upgrades_only_when_edited_and_failed_upgrade_is_retryable() {
        let dir = tempfile::tempdir().unwrap();
        drop(Store::open(dir.path()).unwrap());
        let root = dir.path().join("store");
        let mut legacy = Data::default();
        legacy.tables.insert(
            SHARDED_TABLE.into(),
            Arc::new(
                serde_json::from_value(json!({"key": {"notes":"legacy", "tags":"[]"}})).unwrap(),
            ),
        );
        for i in 0..SHARD_THRESHOLD {
            Arc::make_mut(legacy.tables.get_mut(SHARDED_TABLE).unwrap())
                .insert(format!("legacy-{i}"), json!({"notes":"legacy"}));
        }
        save(&root, &mut legacy, 1).unwrap();
        let original = fs::read(root.join("manifest.json")).unwrap();
        let store = Store::open(dir.path()).unwrap();
        assert_eq!(fs::read(root.join("manifest.json")).unwrap(), original);
        store
            .write(|data| {
                data.table_mut("other").insert("x".into(), json!(1));
                Ok(())
            })
            .await
            .unwrap();
        let unchanged: Manifest =
            serde_json::from_slice(&fs::read(root.join("manifest.json")).unwrap()).unwrap();
        assert_eq!(unchanged.version, 1);
        drop(store);
        let (mut data, revision) = load(&root).unwrap();
        data.table_mut(SHARDED_TABLE).get_mut("key").unwrap()["notes"] = json!("new");
        assert!(save_prepared(&root, &mut data, revision + 1, || Err(
            "fail conversion".into()
        ))
        .is_err());
        assert_eq!(
            load(&root).unwrap().0.table(SHARDED_TABLE)["key"]["notes"],
            "legacy"
        );
        save(&root, &mut data, revision + 1).unwrap();
        let upgraded: Manifest =
            serde_json::from_slice(&fs::read(root.join("manifest.json")).unwrap()).unwrap();
        assert_eq!(upgraded.version, 2);
        assert!(!upgraded.tables.contains_key(SHARDED_TABLE));
        assert_eq!(
            load(&root).unwrap().0.table(SHARDED_TABLE)["key"]["notes"],
            "new"
        );
    }

    #[tokio::test]
    async fn corrupt_missing_misrouted_and_conflicting_shards_fail_closed() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path()).unwrap();
        store
            .write(|data| {
                data.table_mut(SHARDED_TABLE).make_sharded();
                data.table_mut(SHARDED_TABLE)
                    .insert("key".into(), json!("note"));
                Ok(())
            })
            .await
            .unwrap();
        drop(store);
        let root = dir.path().join("store");
        let manifest_path = root.join("manifest.json");
        let original = fs::read(&manifest_path).unwrap();
        let mut manifest: Manifest = serde_json::from_slice(&original).unwrap();
        let slot = bucket("key");
        let id = manifest.shards[SHARDED_TABLE][&slot].clone();
        let path = root.join("objects").join(format!("{id}.json"));
        let bytes = fs::read(&path).unwrap();
        fs::write(&path, b"{}").unwrap();
        assert!(load(&root).is_err());
        fs::remove_file(&path).unwrap();
        assert!(load(&root).is_err());
        fs::write(&path, bytes).unwrap();
        manifest
            .shards
            .get_mut(SHARDED_TABLE)
            .unwrap()
            .insert(slot.wrapping_add(1), id.clone());
        fs::write(&manifest_path, serde_json::to_vec(&manifest).unwrap()).unwrap();
        assert!(load(&root).is_err());
        manifest = serde_json::from_slice(&original).unwrap();
        manifest.tables.insert(SHARDED_TABLE.into(), id);
        fs::write(&manifest_path, serde_json::to_vec(&manifest).unwrap()).unwrap();
        assert!(load(&root).is_err());
        manifest = serde_json::from_slice(&original).unwrap();
        manifest.version = 1;
        fs::write(&manifest_path, serde_json::to_vec(&manifest).unwrap()).unwrap();
        assert!(load(&root).is_err());
    }

    #[tokio::test]
    async fn sharded_deletions_and_clear_persist_without_touching_old_snapshot() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path()).unwrap();
        store
            .write(|data| {
                data.table_mut(SHARDED_TABLE).make_sharded();
                for i in 0..512 {
                    data.table_mut(SHARDED_TABLE)
                        .insert(i.to_string(), json!(i));
                }
                Ok(())
            })
            .await
            .unwrap();
        let old = store.read(|data| Ok(data.clone())).unwrap();
        store
            .write(|data| {
                data.table_mut(SHARDED_TABLE)
                    .retain(|_, value| value.as_i64().unwrap() % 2 == 0);
                Ok(())
            })
            .await
            .unwrap();
        drop(store);
        let store = Store::open(dir.path()).unwrap();
        assert_eq!(
            store
                .read(|data| Ok(data.table(SHARDED_TABLE).len()))
                .unwrap(),
            256
        );
        store
            .write(|data| {
                data.table_mut(SHARDED_TABLE).clear();
                Ok(())
            })
            .await
            .unwrap();
        drop(store);
        assert!(Store::open(dir.path())
            .unwrap()
            .read(|data| Ok(data.table(SHARDED_TABLE).is_empty()))
            .unwrap());
        assert_eq!(old.table(SHARDED_TABLE).len(), 512);
    }
    #[tokio::test]
    async fn sharded_failure_is_not_consumed_by_unrelated_commit() {
        let store = Store::memory().unwrap();
        store
            .write(|data| {
                data.table_mut(SHARDED_TABLE).make_sharded();
                data.table_mut(SHARDED_TABLE)
                    .insert("x".into(), json!("old"));
                Ok(())
            })
            .await
            .unwrap();
        store.fail_table_write(SHARDED_TABLE);
        store
            .write(|data| {
                data.table_mut("other").insert("x".into(), json!("old"));
                Ok(())
            })
            .await
            .unwrap();
        assert!(store
            .write(|data| {
                data.table_mut(SHARDED_TABLE)
                    .insert("x".into(), json!("new"));
                data.table_mut("other").insert("x".into(), json!("new"));
                Ok(())
            })
            .await
            .is_err());
        store
            .read(|data| {
                assert_eq!(data.table(SHARDED_TABLE)["x"], "old");
                assert_eq!(data.table("other")["x"], "old");
                Ok(())
            })
            .unwrap();
        let (disk, _) = load(&store.directory().join("store")).unwrap();
        assert_eq!(disk.table(SHARDED_TABLE)["x"], "old");
        assert_eq!(disk.table("other")["x"], "old");
    }
}
