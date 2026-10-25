use super::persistence;
pub use super::table::Table;
use super::table::{SHARDED_TABLE, SHARD_THRESHOLD};
use serde::{Deserialize, Serialize};
#[cfg(test)]
use serde_json::Value;
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, File, OpenOptions},
    path::{Path, PathBuf},
    sync::{Arc, LazyLock, RwLock},
};
use tokio::sync::{Mutex, OwnedMutexGuard};
#[derive(Clone, Default)]
pub struct Data {
    pub(super) tables: BTreeMap<String, Arc<Table>>,
    pub(super) refs: BTreeMap<String, String>,
    pub(super) dirty: BTreeSet<String>,
}
impl Serialize for Data {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        #[derive(Serialize)]
        struct View<'a> {
            tables: BTreeMap<&'a str, &'a Table>,
        }
        View {
            tables: self
                .tables
                .iter()
                .map(|(k, v)| (k.as_str(), v.as_ref()))
                .collect(),
        }
        .serialize(s)
    }
}
impl<'de> Deserialize<'de> for Data {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        struct View {
            tables: BTreeMap<String, Table>,
        }
        Ok(Self {
            tables: View::deserialize(d)?
                .tables
                .into_iter()
                .map(|(k, mut v)| {
                    if k == SHARDED_TABLE && v.len() >= SHARD_THRESHOLD {
                        v.make_sharded();
                    }
                    (k, Arc::new(v))
                })
                .collect(),
            refs: BTreeMap::new(),
            dirty: BTreeSet::new(),
        })
    }
}
impl Data {
    pub fn table_names(&self) -> Vec<String> {
        self.tables.keys().cloned().collect()
    }
    pub fn table(&self, name: &str) -> &Table {
        static EMPTY: LazyLock<Table> = LazyLock::new(Table::default);
        self.tables.get(name).map(Arc::as_ref).unwrap_or(&EMPTY)
    }
    pub fn table_mut(&mut self, name: &str) -> &mut Table {
        self.refs.remove(name);
        self.dirty.insert(name.into());
        let table = Arc::make_mut(self.tables.entry(name.into()).or_default());
        if name == SHARDED_TABLE && !table.sharded && table.len() >= SHARD_THRESHOLD {
            table.make_sharded();
        }
        table
    }
}
struct Inner {
    directory: PathBuf,
    state: RwLock<(Data, u64)>,
    poisoned: std::sync::atomic::AtomicBool,
    writer: Arc<Mutex<()>>,
    #[cfg(test)]
    fail_commit: std::sync::atomic::AtomicBool,
    #[cfg(test)]
    fail_table: std::sync::Mutex<Option<String>>,
    #[cfg(test)]
    fail_after_publish: std::sync::atomic::AtomicBool,
    _lock: File,
    _access_lock: File,
    _temporary: Option<tempfile::TempDir>,
}
#[derive(Clone)]
pub struct Store(Arc<Inner>);
pub struct Transaction {
    store: Store,
    data: Data,
    revision: u64,
    _guard: OwnedMutexGuard<()>,
}
impl Store {
    pub fn open(directory: &Path) -> Result<Self, String> {
        Self::open_inner(directory, None)
    }
    fn open_inner(directory: &Path, temporary: Option<tempfile::TempDir>) -> Result<Self, String> {
        fs::create_dir_all(directory).map_err(|e| e.to_string())?;
        let access_lock = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(directory.join(".store-access.lock"))
            .map_err(|e| e.to_string())?;
        access_lock
            .try_lock()
            .map_err(|_| "Store or offline migration already active".to_string())?;
        if directory.join(".json-migration.lock").exists() {
            return Err("Offline migration marker exists; inspect migration before opening".into());
        }
        let root = directory.join("store");
        fs::create_dir_all(root.join("objects")).map_err(|e| e.to_string())?;
        fs::create_dir_all(root.join("manifests")).map_err(|e| e.to_string())?;
        let lock = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(root.join("store.lock"))
            .map_err(|e| e.to_string())?;
        lock.try_lock()
            .map_err(|_| "JSON store is already open by another owner".to_string())?;
        let state = persistence::load(&root)?;
        Ok(Self(Arc::new(Inner {
            directory: directory.to_owned(),
            state: RwLock::new(state),
            poisoned: std::sync::atomic::AtomicBool::new(false),
            writer: Arc::new(Mutex::new(())),
            #[cfg(test)]
            fail_commit: std::sync::atomic::AtomicBool::new(false),
            #[cfg(test)]
            fail_table: std::sync::Mutex::new(None),
            #[cfg(test)]
            fail_after_publish: std::sync::atomic::AtomicBool::new(false),
            _lock: lock,
            _access_lock: access_lock,
            _temporary: temporary,
        })))
    }
    pub fn memory() -> Result<Self, String> {
        let temp = tempfile::tempdir().map_err(|e| e.to_string())?;
        let path = temp.path().to_owned();
        Self::open_inner(&path, Some(temp))
    }
    #[cfg(test)]
    pub async fn connect(path: &str) -> Result<Self, String> {
        if path == ":memory:" {
            Self::memory()
        } else {
            let path = Path::new(path);
            Self::open(if path.extension().is_some() {
                path.parent().unwrap_or(Path::new("."))
            } else {
                path
            })
        }
    }
    pub fn directory(&self) -> &Path {
        &self.0.directory
    }
    #[cfg(test)]
    pub async fn close(&self) {}
    #[cfg(test)]
    pub fn fail_table_write(&self, table: &str) {
        *self.0.fail_table.lock().unwrap() = Some(table.to_owned());
    }
    #[cfg(test)]
    pub fn fail_next_commit(&self) {
        self.0
            .fail_commit
            .store(true, std::sync::atomic::Ordering::SeqCst);
    }
    pub fn read<R>(&self, f: impl FnOnce(&Data) -> Result<R, String>) -> Result<R, String> {
        if self.0.poisoned.load(std::sync::atomic::Ordering::SeqCst) {
            return Err("Store commit outcome uncertain; reopen required".into());
        }
        let state = self.0.state.read().map_err(|_| "Store lock poisoned")?;
        f(&state.0)
    }
    pub async fn begin(&self) -> Result<Transaction, String> {
        let guard = self.0.writer.clone().lock_owned().await;
        if self.0.poisoned.load(std::sync::atomic::Ordering::SeqCst) {
            return Err("Store commit outcome uncertain; reopen required".into());
        }
        let state = self.0.state.read().map_err(|_| "Store lock poisoned")?;
        Ok(Transaction {
            store: self.clone(),
            data: state.0.clone(),
            revision: state.1,
            _guard: guard,
        })
    }
    pub async fn write<R>(
        &self,
        f: impl FnOnce(&mut Data) -> Result<R, String>,
    ) -> Result<R, String> {
        let mut tx = self.begin().await?;
        let result = f(tx.data_mut())?;
        tx.commit().await?;
        Ok(result)
    }
}
impl Transaction {
    pub fn data(&self) -> &Data {
        &self.data
    }
    pub fn data_mut(&mut self) -> &mut Data {
        &mut self.data
    }
    pub async fn commit(mut self) -> Result<(), String> {
        #[cfg(test)]
        {
            let mut failure = self.store.0.fail_table.lock().unwrap();
            if failure
                .as_ref()
                .is_some_and(|name| self.data.dirty.contains(name))
            {
                failure.take();
                return Err("Injected JSON table write failure".into());
            }
        }

        #[cfg(test)]
        if self
            .store
            .0
            .fail_commit
            .swap(false, std::sync::atomic::Ordering::SeqCst)
        {
            return Err("Injected JSON commit failure".into());
        }
        let revision = self
            .revision
            .checked_add(1)
            .ok_or("Store revision overflow")?;
        let root = self.store.directory().join("store");
        if let Err(error) = persistence::save(&root, &mut self.data, revision) {
            if error.starts_with("COMMIT_UNCERTAIN:") {
                self.store
                    .0
                    .poisoned
                    .store(true, std::sync::atomic::Ordering::SeqCst);
            }
            return Err(error);
        }
        #[cfg(test)]
        if self
            .store
            .0
            .fail_after_publish
            .swap(false, std::sync::atomic::Ordering::SeqCst)
        {
            self.store
                .0
                .poisoned
                .store(true, std::sync::atomic::Ordering::SeqCst);
            return Err("COMMIT_UNCERTAIN: injected post-publication flush failure".into());
        }
        let mut state = self
            .store
            .0
            .state
            .write()
            .map_err(|_| "Store lock poisoned")?;
        *state = (self.data, revision);
        drop(state);
        // Cleanup is best effort after durable publication; it cannot undo a commit.
        let _ = persistence::collect(&root, revision);
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn atomic_restart_and_rollback() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path()).unwrap();
        store
            .write(|d| {
                d.table_mut("a").insert("1".into(), Value::from(1));
                d.table_mut("b").insert("2".into(), Value::from(2));
                Ok(())
            })
            .await
            .unwrap();
        let mut tx = store.begin().await.unwrap();
        tx.data_mut().table_mut("a").clear();
        assert_eq!(store.read(|d| Ok(d.table("a").len())).unwrap(), 1);
        drop(tx);
        drop(store);
        let reopened = Store::open(dir.path()).unwrap();
        assert_eq!(
            reopened
                .read(|d| Ok(d.table("a").len() + d.table("b").len()))
                .unwrap(),
            2
        );
    }
    #[test]
    fn exclusive_lock() {
        let dir = tempfile::tempdir().unwrap();
        let first = Store::open(dir.path()).unwrap();
        assert!(Store::open(dir.path()).is_err());
        drop(first);
        assert!(Store::open(dir.path()).is_ok());
    }
    #[tokio::test]
    async fn corrupt_object_fails_closed() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path()).unwrap();
        store
            .write(|d| {
                d.table_mut("a").insert("a".into(), Value::Null);
                Ok(())
            })
            .await
            .unwrap();
        drop(store);
        let object = fs::read_dir(dir.path().join("store/objects"))
            .unwrap()
            .next()
            .unwrap()
            .unwrap()
            .path();
        fs::write(object, b"{}").unwrap();
        assert!(Store::open(dir.path()).is_err());
    }
    #[tokio::test]
    async fn rejected_write_preserves_state() {
        let store = Store::memory().unwrap();
        let result: Result<(), String> = store
            .write(|d| {
                d.table_mut("a").insert("a".into(), Value::Null);
                Err("rejected".into())
            })
            .await;
        assert!(result.is_err());
        assert_eq!(store.read(|d| Ok(d.table("a").len())).unwrap(), 0);
    }
}

#[cfg(test)]
mod fault_tests {
    use super::*;
    #[tokio::test]
    async fn failed_commit_is_retryable() {
        let store = Store::memory().unwrap();
        store.fail_next_commit();
        assert!(store
            .write(|d| {
                d.table_mut("a").insert("x".into(), Value::Null);
                Ok(())
            })
            .await
            .is_err());
        assert!(store.read(|d| Ok(d.table("a").is_empty())).unwrap());
        store
            .write(|d| {
                d.table_mut("a").insert("x".into(), Value::Null);
                Ok(())
            })
            .await
            .unwrap();
    }
    #[tokio::test]
    async fn concurrent_writes_preserve_all_rows() {
        let store = Store::memory().unwrap();
        let mut tasks = Vec::new();
        for i in 0..16 {
            let store = store.clone();
            tasks.push(tokio::spawn(async move {
                store
                    .write(|d| {
                        d.table_mut("a").insert(i.to_string(), Value::from(i));
                        Ok(())
                    })
                    .await
                    .unwrap()
            }));
        }
        for task in tasks {
            task.await.unwrap();
        }
        assert_eq!(store.read(|d| Ok(d.table("a").len())).unwrap(), 16);
    }
    #[test]
    fn future_version_fails_closed() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir(dir.path().join("store")).unwrap();
        fs::write(
            dir.path().join("store/manifest.json"),
            br#"{"version":999,"revision":1,"tables":{}}"#,
        )
        .unwrap();
        assert!(Store::open(dir.path()).is_err());
    }
    #[tokio::test]
    async fn filesystem_failure_keeps_committed_state() {
        let store = Store::memory().unwrap();
        let manifests = store.directory().join("store/manifests");
        fs::remove_dir(&manifests).unwrap();
        fs::write(&manifests, b"blocked").unwrap();
        assert!(store
            .write(|d| {
                d.table_mut("a").insert("x".into(), Value::Null);
                Ok(())
            })
            .await
            .is_err());
        assert!(store.read(|d| Ok(d.table("a").is_empty())).unwrap());
        fs::remove_file(&manifests).unwrap();
        fs::create_dir(manifests).unwrap();
        store
            .write(|d| {
                d.table_mut("a").insert("x".into(), Value::Null);
                Ok(())
            })
            .await
            .unwrap();
    }
}

#[cfg(test)]
mod retention_tests {
    use super::*;
    #[tokio::test]
    async fn cow_and_retention() {
        let store = Store::memory().unwrap();
        store
            .write(|d| {
                d.table_mut("large")
                    .insert("x".into(), Value::from("unchanged"));
                Ok(())
            })
            .await
            .unwrap();
        let old = store.read(|d| Ok(d.clone())).unwrap();
        for i in 0..5 {
            store
                .write(|d| {
                    assert!(Arc::ptr_eq(
                        d.tables.get("large").unwrap(),
                        old.tables.get("large").unwrap()
                    ));
                    d.table_mut("small").insert("x".into(), Value::from(i));
                    Ok(())
                })
                .await
                .unwrap();
        }
        assert_eq!(
            fs::read_dir(store.directory().join("store/manifests"))
                .unwrap()
                .count(),
            2
        );
        assert_eq!(
            fs::read_dir(store.directory().join("store/objects"))
                .unwrap()
                .count(),
            3
        );
        assert_eq!(old.table("large")["x"], Value::from("unchanged"));
    }
    #[tokio::test]
    async fn missing_manifest_does_not_reset() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path()).unwrap();
        store
            .write(|d| {
                d.table_mut("a").insert("x".into(), Value::Null);
                Ok(())
            })
            .await
            .unwrap();
        drop(store);
        fs::remove_file(dir.path().join("store/manifest.json")).unwrap();
        assert!(Store::open(dir.path()).is_err());
    }
    #[tokio::test]
    async fn unknown_history_prevents_cleanup() {
        let store = Store::memory().unwrap();
        fs::write(
            store.directory().join("store/manifests/unknown.json"),
            b"{}",
        )
        .unwrap();
        for i in 0..4 {
            store
                .write(|d| {
                    d.table_mut("a").insert("x".into(), Value::from(i));
                    Ok(())
                })
                .await
                .unwrap();
        }
        assert_eq!(
            fs::read_dir(store.directory().join("store/objects"))
                .unwrap()
                .count(),
            4
        );
    }
}

#[cfg(test)]
mod uncertain_tests {
    use super::*;
    #[tokio::test]
    async fn ambiguous_publish_requires_reopen() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path()).unwrap();
        store
            .0
            .fail_after_publish
            .store(true, std::sync::atomic::Ordering::SeqCst);
        assert!(store
            .write(|d| {
                d.table_mut("a").insert("x".into(), Value::from(1));
                Ok(())
            })
            .await
            .is_err());
        assert!(store.read(|_| Ok(())).is_err());
        assert!(store.begin().await.is_err());
        drop(store);
        let store = Store::open(dir.path()).unwrap();
        assert_eq!(
            store.read(|d| Ok(d.table("a")["x"].clone())).unwrap(),
            Value::from(1)
        );
    }
}
#[cfg(test)]
mod table_failure_tests {
    use super::*;
    #[tokio::test]
    async fn unrelated_commit_does_not_consume_failure() {
        let store = Store::memory().unwrap();
        store.fail_table_write("target");
        store
            .write(|d| {
                d.table_mut("other").insert("x".into(), Value::Null);
                Ok(())
            })
            .await
            .unwrap();
        assert!(store
            .write(|d| {
                d.table_mut("target").insert("x".into(), Value::Null);
                Ok(())
            })
            .await
            .is_err());
        assert!(store.read(|d| Ok(d.table("target").is_empty())).unwrap());
        store
            .write(|d| {
                d.table_mut("target").insert("x".into(), Value::Null);
                Ok(())
            })
            .await
            .unwrap();
    }
    #[tokio::test]
    #[ignore = "Requires Python standard library for isolated legacy migration fixture"]
    async fn python_migration_is_readable_by_native_store() {
        let temp = tempfile::tempdir().unwrap();
        let script =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../scripts/test_json_migration.py");
        let output = std::process::Command::new("python")
            .arg(script)
            .arg("--export-fixture")
            .arg(temp.path())
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "synthetic migration process failed"
        );
        let store = Store::open(temp.path()).unwrap();
        let probe = std::process::Command::new("python")
            .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("../scripts/test_json_migration.py"))
            .arg("--probe-lock")
            .arg(temp.path())
            .output()
            .unwrap();
        assert_eq!(
            probe.status.code(),
            Some(73),
            "Python migration must respect the Rust store lock"
        );
        let skill = crate::db::get_skill_by_id(&store, "skill")
            .await
            .unwrap()
            .unwrap();
        assert!(skill.is_central);
        assert_eq!(
            crate::db::get_skill_installations(&store, "skill")
                .await
                .unwrap()[0]
                .created_at,
            "first"
        );
        assert!(
            crate::db::get_skill_source_sync(&store, "skill")
                .await
                .unwrap()
                .unwrap()
                .remote_deleted
        );
        assert_eq!(
            crate::db::get_skill_source(&store, "skill")
                .await
                .unwrap()
                .unwrap()
                .source_repo
                .as_deref(),
            Some("fixture/repo")
        );
        assert_eq!(
            crate::db::get_skill_metadata(&store, "skill")
                .await
                .unwrap()
                .unwrap()
                .notes
                .as_deref(),
            Some("note")
        );
        assert_eq!(
            crate::db::get_collection_skills(&store, "collection")
                .await
                .unwrap()
                .len(),
            1
        );
        assert!(
            crate::db::get_agent_by_id(&store, "agent")
                .await
                .unwrap()
                .unwrap()
                .is_enabled
        );
        assert_eq!(
            crate::db::metadata::folder_notes(&store).await.unwrap()[0].skill_ids,
            vec!["skill"]
        );
        assert_eq!(
            crate::db::notebooks::list(&store).await.unwrap()[0].notebook,
            "Text"
        );
        assert_eq!(crate::db::notebooks::next_id(&store).await.unwrap(), 43);
        store
            .read(|d| {
                assert_eq!(
                    d.table("skill_explanations")[&crate::db::key2("skill", "zh")]["explanation"],
                    "cached"
                );
                assert_eq!(
                    d.table("skill_installations")[&crate::db::key2("skill", "agent")]
                        ["is_managed"],
                    true
                );
                assert_eq!(
                    d.table("agent_skill_observations")["observation"]["is_read_only"],
                    true
                );
                assert_eq!(
                    d.table("github_repository_stats")["fixture/repo"]["stars"],
                    3
                );
                assert!(d
                    .table("import_transaction_receipts")
                    .contains_key("receipt"));
                Ok(())
            })
            .unwrap();
    }
}
