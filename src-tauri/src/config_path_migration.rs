//! Complete data-root migration using the normal Store commit and linker APIs.
//! A durable link plan makes removal/recreation resumable after interruption.
use crate::{
    config_store::ConfigStore,
    path_utils,
    storage::store::{Data, Store},
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::BTreeSet,
    fs,
    io::Write,
    path::{Component, Path, PathBuf},
};

#[derive(Default, Serialize, Deserialize)]
pub struct Report {
    pub paths: usize,
    pub links: usize,
    pub alias_removed: bool,
}
#[derive(Clone, Serialize, Deserialize)]
struct LinkChange {
    path: PathBuf,
    previous: PathBuf,
    target: PathBuf,
    staging: PathBuf,
}

struct Mapping {
    old: String,
    new: String,
}
impl Mapping {
    fn new(root: &Path) -> Self {
        Self {
            old: normalized(&root.with_file_name(".skillshub").to_string_lossy()),
            new: path_utils::path_to_string(root),
        }
    }
    fn rewrite(&self, text: &str) -> Option<String> {
        let expanded;
        let input = if text.starts_with("~/") || text.starts_with("~\\") {
            expanded = path_utils::expand_home_path(text);
            normalized(&expanded.to_string_lossy())
        } else {
            normalized(text)
        };
        let prefix = input.get(..self.old.len())?;
        let equal = if cfg!(windows) {
            prefix.eq_ignore_ascii_case(&self.old)
        } else {
            prefix == self.old
        };
        if !equal
            || !(input.len() == self.old.len()
                || input.as_bytes().get(self.old.len()) == Some(&b'/'))
        {
            return None;
        }
        let tail = &input[self.old.len()..];
        Some(format!(
            "{}{}",
            self.new,
            if cfg!(windows) {
                tail.replace('/', "\\")
            } else {
                tail.to_owned()
            }
        ))
    }
    fn path(&self, path: &Path) -> PathBuf {
        self.rewrite(&path.to_string_lossy())
            .map(PathBuf::from)
            .unwrap_or_else(|| path.to_owned())
    }
}
fn normalized(text: &str) -> String {
    text.trim_start_matches("\\\\?\\").replace('\\', "/")
}
fn lexical(path: &Path) -> PathBuf {
    let mut result = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                result.pop();
            }
            other => result.push(other.as_os_str()),
        }
    }
    result
}
fn rewrite_value(value: &mut Value, mapping: &Mapping) -> Result<usize, String> {
    let mut count = 0;
    match value {
        Value::String(text) => {
            if let Some(next) = mapping.rewrite(text) {
                *text = next;
                count += 1;
            } else if (text.starts_with('{') || text.starts_with('['))
                && text.contains(".skillshub")
            {
                if let Ok(mut embedded) = serde_json::from_str::<Value>(text) {
                    let changed = rewrite_value(&mut embedded, mapping)?;
                    if changed > 0 {
                        *text = serde_json::to_string(&embedded)
                            .map_err(|_| "Cannot encode migrated record")?;
                        count += changed;
                    }
                }
            }
        }
        Value::Array(items) => {
            for item in items {
                count += rewrite_value(item, mapping)?;
            }
        }
        Value::Object(object) => {
            let keys: Vec<_> = object.keys().cloned().collect();
            for key in keys {
                count += rewrite_value(object.get_mut(&key).unwrap(), mapping)?;
                if let Some(next) = mapping.rewrite(&key) {
                    if object.contains_key(&next) {
                        return Err("Migrated path conflicts with an existing record key".into());
                    }
                    let value = object.remove(&key).unwrap();
                    object.insert(next, value);
                    count += 1;
                }
            }
        }
        _ => {}
    }
    Ok(count)
}
fn rewrite_data(data: &mut Data, mapping: &Mapping) -> Result<usize, String> {
    let mut count = 0;
    for name in data.table_names() {
        let mut updates = Vec::new();
        for (key, value) in data.table(&name) {
            let mut next = value.clone();
            let changed = rewrite_value(&mut next, mapping)?;
            let next_key = mapping.rewrite(key).unwrap_or_else(|| key.clone());
            if next_key != *key && data.table(&name).contains_key(&next_key) {
                return Err("Migrated path conflicts with an existing row".into());
            }
            if changed > 0 || next_key != *key {
                count += changed + usize::from(next_key != *key);
                updates.push((key.clone(), next_key, next));
            }
        }
        if !updates.is_empty() {
            let table = data.table_mut(&name);
            for (old, key, value) in updates {
                table.remove(&old);
                table.insert(key, value);
            }
        }
    }
    Ok(count)
}

fn durable_json(path: &Path, value: &impl Serialize) -> Result<(), String> {
    let bytes =
        serde_json::to_vec_pretty(value).map_err(|_| "Cannot encode migration checkpoint")?;
    let temp = path.with_extension("pending");
    let mut file = fs::File::create(&temp).map_err(|_| "Cannot create migration checkpoint")?;
    file.write_all(&bytes)
        .and_then(|_| file.sync_all())
        .map_err(|_| "Cannot save migration checkpoint")?;
    drop(file);
    fs::rename(temp, path).map_err(|_| "Cannot publish migration checkpoint".into())
}
fn roots(
    store: &Store,
    config: &crate::config_store::AppConfig,
    mapping: &Mapping,
) -> Result<BTreeSet<PathBuf>, String> {
    let mut roots = BTreeSet::from([store.directory().to_owned()]);
    #[cfg(not(test))]
    roots.insert(path_utils::central_skills_dir());
    for platform in &config.platforms {
        if !platform.global_skills_dir.is_empty() {
            roots.insert(mapping.path(&path_utils::expand_home_path(&platform.global_skills_dir)));
        }
    }
    for project in &config.scan_directories {
        let path = mapping.path(&path_utils::expand_home_path(&project.path));
        roots.insert(path.join(".agents/skills"));
        for platform in &config.platforms {
            if let Some(relative) = &platform.project_skills_dir {
                if !relative.is_empty() && !Path::new(relative).is_absolute() {
                    roots.insert(path.join(relative));
                }
            }
        }
    }
    store.read(|data| {
        for name in ["skill_installations", "agent_skill_observations"] {
            for row in data.table(name).values() {
                for key in ["installed_path", "dir_path"] {
                    if let Some(path) = row
                        .get(key)
                        .and_then(Value::as_str)
                        .filter(|p| !p.is_empty())
                    {
                        let path = mapping.path(&PathBuf::from(path));
                        if let Some(parent) = path.parent() {
                            roots.insert(parent.to_owned());
                        }
                    }
                }
            }
        }
        Ok(())
    })?;
    Ok(roots)
}
fn scan(
    path: &Path,
    mapping: &Mapping,
    visited: &mut BTreeSet<PathBuf>,
    plan: &mut Vec<LinkChange>,
) -> Result<(), String> {
    let path = lexical(path);
    if !visited.insert(path.clone()) {
        return Ok(());
    }
    let metadata = match fs::symlink_metadata(&path) {
        Ok(m) => m,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(_) => return Err("Cannot inspect an installation directory during migration".into()),
    };
    if let Ok(previous) = fs::read_link(&path) {
        let resolved = lexical(&if previous.is_absolute() {
            previous.clone()
        } else {
            path.parent()
                .ok_or("Invalid installation link")?
                .join(&previous)
        });
        if let Some(next) = mapping.rewrite(&resolved.to_string_lossy()) {
            let target = PathBuf::from(next);
            if !target.exists() {
                return Err("An installed link points to missing data; repair it before completing configuration migration".into());
            }
            let staging = path.with_file_name(format!(".arsenal-link-{}", uuid::Uuid::new_v4()));
            plan.push(LinkChange {
                path,
                previous,
                target,
                staging,
            });
        }
        return Ok(()); // Never recurse through links into unrelated directories.
    }
    if metadata.is_dir() {
        for entry in fs::read_dir(&path).map_err(|_| "Cannot scan installation links")? {
            let entry = entry.map_err(|_| "Cannot inspect installation entry")?;
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if [".git", "store", "migrations", "backups"].contains(&name.as_ref())
                || name.starts_with(".arsenal-link-")
            {
                continue;
            }
            scan(&entry.path(), mapping, visited, plan)?;
        }
    }
    Ok(())
}
fn scan_roots(
    roots: &BTreeSet<PathBuf>,
    mapping: &Mapping,
    visited: &mut BTreeSet<PathBuf>,
    plan: &mut Vec<LinkChange>,
) -> Result<(), String> {
    for directory in roots {
        scan(directory, mapping, visited, plan)?;
        // An explicitly configured install root may itself alias another root.
        // Descendant links are still inspected without following their targets.
        if fs::read_link(directory).is_ok() {
            if let Ok(resolved) = fs::canonicalize(directory) {
                scan(&resolved, mapping, visited, plan)?;
            }
        }
    }
    Ok(())
}
fn apply(plan: &[LinkChange]) -> Result<(), String> {
    for change in plan {
        crate::commands::linker::migrate_installation_link(
            &change.path,
            &change.previous,
            &change.target,
            &change.staging,
        )?;
    }
    Ok(())
}

fn retire_legacy_pointers(root: &Path, checkpoint: &Path) -> Result<(), String> {
    let home = path_utils::resolve_home_dir();
    let parent = root.parent().ok_or("Invalid data root")?;
    for (file, replacement, backup) in [
        (
            parent.join("skillshub-config-path"),
            None,
            "portable-pointer.before.txt",
        ),
        (
            home.join(".skillshub-config-path"),
            Some(home.join(".arsenal-config-path")),
            "home-pointer.before.txt",
        ),
    ] {
        let content = match fs::read_to_string(&file) {
            Ok(value) => value,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(_) => return Err("Cannot inspect legacy config pointer".into()),
        };
        let resolved = path_utils::normalize_app_data_dir_with_home(content.trim(), &home)?;
        if !matches!((fs::canonicalize(resolved), fs::canonicalize(root)), (Ok(a), Ok(b)) if a == b)
        {
            continue;
        }
        let snapshot = checkpoint.join(backup);
        if !snapshot.exists() {
            fs::copy(&file, snapshot).map_err(|_| "Cannot back up legacy config pointer")?;
        }
        if let Some(next) = replacement {
            if !next.exists() {
                let mut output = fs::OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(next)
                    .map_err(|_| "Cannot migrate config pointer")?;
                output
                    .write_all(path_utils::path_to_string(root).as_bytes())
                    .and_then(|_| output.sync_all())
                    .map_err(|_| "Cannot save new config pointer")?;
            }
        }
        fs::remove_file(file).map_err(|_| "Cannot retire legacy config pointer")?;
    }
    Ok(())
}

pub async fn finish(store: &Store) -> Result<Report, String> {
    let root = store.directory();
    if root.file_name().and_then(|n| n.to_str()) != Some(".arsenal") {
        return Ok(Report::default());
    }
    let old = root.with_file_name(".skillshub");
    let checkpoint = root.join("migrations/arsenal-paths-v2");
    let done = checkpoint.join("complete.json");
    if done.is_file() && fs::symlink_metadata(&old).is_err() {
        return Ok(Report::default());
    }
    let mapping = Mapping::new(root);
    let config = ConfigStore::new(root);
    fs::create_dir_all(&checkpoint).map_err(|_| "Cannot prepare migration backup")?;
    // Recovery data stays inside .arsenal; never publish configuration contents.
    let backup = checkpoint.join("config.before.json");
    if !backup.exists() && config.path().exists() {
        fs::copy(config.path(), backup).map_err(|_| "Cannot back up configuration")?;
    }
    let data_backup = checkpoint.join("data.before.json");
    if !data_backup.exists() {
        store.read(|data| durable_json(&data_backup, data))?;
    }
    let pending = checkpoint.join("links.json");
    let mut report = Report::default();
    if pending.is_file() {
        let plan: Vec<LinkChange> =
            serde_json::from_slice(&fs::read(&pending).map_err(|_| "Cannot read migration plan")?)
                .map_err(|_| "Invalid migration plan")?;
        apply(&plan)?;
        report.links += plan.len();
    }
    let settings = config.load()?;
    let search_roots = roots(store, &settings, &mapping)?;
    let mut plan = Vec::new();
    let mut visited = BTreeSet::new();
    scan_roots(&search_roots, &mapping, &mut visited, &mut plan)?;
    if !plan.is_empty() {
        durable_json(&pending, &plan)?;
        apply(&plan)?;
        report.links += plan.len();
    }
    report.paths += config.update(|settings| {
        let mut value =
            serde_json::to_value(&*settings).map_err(|_| "Cannot read configuration")?;
        let changed = rewrite_value(&mut value, &mapping)?;
        *settings = serde_json::from_value(value).map_err(|_| "Cannot migrate configuration")?;
        Ok(changed)
    })?;
    report.paths += store.write(|data| rewrite_data(data, &mapping)).await?;
    // Audit the current logical state, not immutable old recovery objects.
    let mut current = store.read(|data| Ok(data.clone()))?;
    if rewrite_data(&mut current, &mapping)? != 0 {
        return Err("Legacy references remain in current storage".into());
    }
    let mut remaining = Vec::new();
    let mut visited = BTreeSet::new();
    scan_roots(&search_roots, &mapping, &mut visited, &mut remaining)?;
    if !remaining.is_empty() {
        return Err("Legacy installation links remain".into());
    }
    retire_legacy_pointers(root, &checkpoint)?;
    if fs::symlink_metadata(&old).is_ok() {
        if !matches!((fs::canonicalize(&old), fs::canonicalize(root)), (Ok(a), Ok(b)) if a == b)
            || fs::read_link(&old).is_err()
        {
            return Err("Refusing to remove an independent legacy directory".into());
        }
        path_utils::remove_symlink_path(&old)
            .map_err(|_| "Cannot remove the verified compatibility link")?;
        report.alias_removed = true;
    }
    // The former directory-renaming gate is no longer needed outside the data root.
    let lock = root.with_file_name(".arsenal-migration.lock");
    if lock.exists() {
        let file = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(&lock)
            .map_err(|_| "Cannot open old migration lock")?;
        file.try_lock()
            .map_err(|_| "Directory migration is still active")?;
        drop(file);
        fs::remove_file(lock).map_err(|_| "Cannot remove old migration lock")?;
    }
    durable_json(&done, &report)?;
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    fn fixture() -> (tempfile::TempDir, Store) {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join(".arsenal");
        let store = Store::open(&root).unwrap();
        ConfigStore::new(&root)
            .update(|config| {
                config.platforms.clear();
                Ok(())
            })
            .unwrap();
        (temp, store)
    }
    fn link(target: &Path, path: &Path) {
        crate::commands::linker::create_symlink(target, path)
            .or_else(|_| crate::config_dir_migration::compatibility_link(target, path))
            .unwrap();
    }
    #[test]
    fn changes_only_the_exact_data_root_prefix() {
        let mapping = Mapping {
            old: "D:/app/.skillshub".into(),
            new: "D:/app/.arsenal".into(),
        };
        let mut value = json!({"file_path":"D:/app/.skillshub/library/test/SKILL.md", "notes":"Mention D:/app/.skillshub in prose", "other":"D:/app/.skillshub-other/file", "external":"D:/other/.skillshub/file"});
        assert_eq!(rewrite_value(&mut value, &mapping).unwrap(), 1);
        assert!(value["file_path"].as_str().unwrap().contains(".arsenal"));
        assert_eq!(value["notes"], "Mention D:/app/.skillshub in prose");
        assert_eq!(value["external"], "D:/other/.skillshub/file");
        assert!(value["other"]
            .as_str()
            .unwrap()
            .contains(".skillshub-other"));
    }
    #[tokio::test]
    async fn rewrites_config_records_and_installed_links_then_removes_alias() {
        let (temp, store) = fixture();
        let root = store.directory();
        let old = temp.path().join(".skillshub");
        let skill = root.join("library/example");
        fs::create_dir_all(&skill).unwrap();
        fs::write(skill.join("SKILL.md"), "unchanged skill").unwrap();
        link(root, &old);
        let installed = temp.path().join("platform");
        fs::create_dir(&installed).unwrap();
        link(&old.join("library/example"), &installed.join("absolute"));
        link(
            Path::new("../.skillshub/library/example"),
            &installed.join("relative"),
        );
        ConfigStore::new(root)
            .update(|config| {
                config.settings.insert(
                    "resource_library_dir".into(),
                    old.join("library").to_string_lossy().into(),
                );
                Ok(())
            })
            .unwrap();
        store.write(|data| {
            data.table_mut("skills").insert("one".into(), json!({"file_path":old.join("library/example/SKILL.md"), "canonical_path":old.join("library/example")}));
            data.table_mut("skill_installations").insert("one".into(), json!({"installed_path":installed.join("absolute"),"symlink_target":old.join("library/example")})); Ok(())
        }).await.unwrap();
        let report = finish(&store).await.unwrap();
        assert_eq!(report.links, 2);
        assert_eq!(report.paths, 4);
        assert!(report.alias_removed);
        assert!(fs::symlink_metadata(&old).is_err());
        for name in ["absolute", "relative"] {
            assert_eq!(
                fs::read(installed.join(name).join("SKILL.md")).unwrap(),
                b"unchanged skill"
            );
            assert!(!fs::read_link(installed.join(name))
                .unwrap()
                .to_string_lossy()
                .contains(".skillshub"));
        }
        assert!(
            ConfigStore::new(root).load().unwrap().settings["resource_library_dir"]
                .contains(".arsenal")
        );
        assert_eq!(finish(&store).await.unwrap().paths, 0);
        let root = root.to_owned();
        drop(store);
        drop(Store::open(&root).unwrap());
    }
    #[tokio::test]
    async fn resumes_missing_installation_link_from_durable_plan() {
        let (temp, store) = fixture();
        let root = store.directory();
        let target = root.join("library/skill");
        fs::create_dir_all(&target).unwrap();
        let checkpoint = root.join("migrations/arsenal-paths-v2");
        fs::create_dir_all(&checkpoint).unwrap();
        let path = temp.path().join("installed");
        durable_json(
            &checkpoint.join("links.json"),
            &vec![LinkChange {
                path: path.clone(),
                previous: temp.path().join(".skillshub/library/skill"),
                target: target.clone(),
                staging: temp.path().join("staging"),
            }],
        )
        .unwrap();
        finish(&store).await.unwrap();
        assert_eq!(
            fs::canonicalize(path).unwrap(),
            fs::canonicalize(target).unwrap()
        );
    }
    #[tokio::test]
    async fn preserves_real_directory_when_recovery_link_was_replaced() {
        let (temp, store) = fixture();
        let root = store.directory();
        let old = root.with_file_name(".skillshub");
        link(root, &old);
        let path = temp.path().join("installed");
        fs::create_dir(&path).unwrap();
        fs::write(path.join("keep"), "user data").unwrap();
        let checkpoint = root.join("migrations/arsenal-paths-v2");
        fs::create_dir_all(&checkpoint).unwrap();
        durable_json(
            &checkpoint.join("links.json"),
            &vec![LinkChange {
                path: path.clone(),
                previous: old.clone(),
                target: root.to_owned(),
                staging: temp.path().join("staging"),
            }],
        )
        .unwrap();
        assert!(finish(&store).await.is_err());
        assert_eq!(fs::read(path.join("keep")).unwrap(), b"user data");
        assert!(old.exists());
        path_utils::remove_symlink_path(&old).unwrap();
    }
}
