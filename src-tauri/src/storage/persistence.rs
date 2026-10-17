use super::store::Data;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs::{self, File},
    io::Write,
    path::Path,
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
    let mut data = Data::default();
    for (name, id) in manifest.tables {
        if id.len() != 64 || !id.bytes().all(|c| c.is_ascii_hexdigit()) {
            return Err("Invalid object identifier".into());
        }
        let bytes =
            fs::read(root.join("objects").join(format!("{id}.json"))).map_err(|e| e.to_string())?;
        if hash(&bytes) != id {
            return Err("JSON object checksum mismatch".into());
        }
        data.refs.insert(name.clone(), id);
        data.tables.insert(
            name,
            std::sync::Arc::new(serde_json::from_slice(&bytes).map_err(|e| e.to_string())?),
        );
    }
    Ok((data, manifest.revision))
}
pub(super) fn save(root: &Path, data: &mut Data, revision: u64) -> Result<(), String> {
    let mut tables = BTreeMap::new();
    for (name, rows) in &data.tables {
        if let Some(id) = data.refs.get(name) {
            tables.insert(name.clone(), id.clone());
            continue;
        }
        let bytes = serde_json::to_vec(rows.as_ref()).map_err(|e| e.to_string())?;
        let id = hash(&bytes);
        let path = root.join("objects").join(format!("{id}.json"));
        if path.exists() {
            if fs::read(&path).map_err(|e| e.to_string())? != bytes {
                return Err("Existing JSON object is corrupt".into());
            }
        } else {
            let pending = root.join("objects").join(format!("{id}.pending"));
            durable(&pending, &bytes)?;
            replace(&pending, &path)?;
        }
        tables.insert(name.clone(), id);
    }
    let bytes = serde_json::to_vec_pretty(&Manifest {
        version: 1,
        revision,
        tables: tables.clone(),
    })
    .map_err(|e| e.to_string())?;
    durable(
        &root.join("manifests").join(format!("{revision}.json")),
        &bytes,
    )?;
    #[cfg(unix)]
    for directory in [root.join("objects"), root.join("manifests")] {
        File::open(directory)
            .and_then(|f| f.sync_all())
            .map_err(|e| e.to_string())?;
    }
    let staging = root.join("manifest.pending");
    durable(&staging, &bytes)?;
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
                || !root.join("objects").join(format!("{id}.json")).is_file()
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
