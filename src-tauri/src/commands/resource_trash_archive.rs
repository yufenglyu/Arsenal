//! Binary sidecars keep repository payloads out of JSON tables and their size limits.
use super::{Directory, Payload};
use crate::db::{AppStore, SkillInstallation};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::BTreeMap,
    fs::File,
    io::{Read, Write},
    path::PathBuf,
};
use zip::{write::FileOptions, ZipArchive, ZipWriter};

#[derive(Serialize, Deserialize)]
struct ArchivedDirectory {
    path: PathBuf,
    files: BTreeMap<String, u64>,
}
#[derive(Serialize, Deserialize)]
struct Manifest {
    version: u32,
    root: PathBuf,
    directories: Vec<ArchivedDirectory>,
    tables: BTreeMap<String, BTreeMap<String, Value>>,
    installations: Vec<SkillInstallation>,
    copies: Vec<ArchivedDirectory>,
}
fn directory_metadata(directories: &[Directory]) -> Vec<ArchivedDirectory> {
    directories
        .iter()
        .map(|directory| ArchivedDirectory {
            path: directory.path.clone(),
            files: directory
                .files
                .iter()
                .map(|(path, bytes)| (path.clone(), bytes.len() as u64))
                .collect(),
        })
        .collect()
}
pub(super) fn path(pool: &AppStore, id: &str) -> Result<PathBuf, String> {
    uuid::Uuid::parse_str(id).map_err(|_| "TRASH_UNAVAILABLE")?;
    let directory = pool.directory().join("trash");
    if std::fs::symlink_metadata(&directory)
        .is_ok_and(|metadata| metadata.file_type().is_symlink() || !metadata.is_dir())
    {
        return Err("UNSAFE_TARGET".into());
    }
    let path = directory.join(format!("{id}.zip"));
    if std::fs::symlink_metadata(&path)
        .is_ok_and(|metadata| metadata.file_type().is_symlink() || !metadata.is_file())
    {
        return Err("UNSAFE_TARGET".into());
    }
    Ok(path)
}
pub(super) fn write(pool: &AppStore, id: &str, payload: &Payload) -> Result<(), String> {
    let path = path(pool, id)?;
    let parent = path.parent().ok_or("TRASH_UNAVAILABLE")?;
    std::fs::create_dir_all(parent).map_err(|_| "WRITE_FAILED")?;
    let mut temporary = tempfile::Builder::new()
        .prefix(&format!(".arsenal-trash-{id}-"))
        .tempfile_in(parent)
        .map_err(|_| "WRITE_FAILED")?;
    let manifest = Manifest {
        version: 2,
        root: payload.root.clone(),
        directories: directory_metadata(&payload.directories),
        tables: payload.tables.clone(),
        installations: payload.installations.clone(),
        copies: directory_metadata(&payload.copies),
    };
    {
        let options = FileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated)
            .compression_level(Some(1))
            .large_file(true);
        let mut archive = ZipWriter::new(temporary.as_file_mut());
        archive
            .start_file("manifest.json", options)
            .map_err(|_| "WRITE_FAILED")?;
        serde_json::to_writer(&mut archive, &manifest).map_err(|_| "WRITE_FAILED")?;
        for (prefix, directories) in [
            ("directories", &payload.directories),
            ("copies", &payload.copies),
        ] {
            for (index, directory) in directories.iter().enumerate() {
                for (name, bytes) in &directory.files {
                    if name.ends_with('/') {
                        continue;
                    }
                    archive
                        .start_file(format!("{prefix}/{index}/{name}"), options)
                        .map_err(|_| "WRITE_FAILED")?;
                    archive.write_all(bytes).map_err(|_| "WRITE_FAILED")?;
                }
            }
        }
        archive
            .finish()
            .map_err(|_| "WRITE_FAILED")?
            .sync_all()
            .map_err(|_| "WRITE_FAILED")?;
    }
    temporary
        .persist_noclobber(&path)
        .map_err(|_| "WRITE_FAILED")?;
    Ok(())
}
fn read_directories(
    archive: &mut ZipArchive<File>,
    prefix: &str,
    directories: Vec<ArchivedDirectory>,
    include_files: bool,
) -> Result<Vec<Directory>, String> {
    directories
        .into_iter()
        .enumerate()
        .map(|(index, directory)| {
            let mut files = BTreeMap::new();
            for (name, expected) in directory.files {
                let mut bytes = Vec::new();
                if include_files && !name.ends_with('/') {
                    let mut file = archive
                        .by_name(&format!("{prefix}/{index}/{name}"))
                        .map_err(|_| "TRASH_UNAVAILABLE")?;
                    if file.size() != expected {
                        return Err("TRASH_UNAVAILABLE".into());
                    }
                    bytes
                        .try_reserve_exact(
                            usize::try_from(expected).map_err(|_| "TRASH_UNAVAILABLE")?,
                        )
                        .map_err(|_| "TRASH_UNAVAILABLE")?;
                    file.read_to_end(&mut bytes)
                        .map_err(|_| "TRASH_UNAVAILABLE")?;
                    if bytes.len() as u64 != expected {
                        return Err("TRASH_UNAVAILABLE".into());
                    }
                } else if name.ends_with('/') && expected != 0 {
                    return Err("TRASH_UNAVAILABLE".into());
                }
                files.insert(name, bytes);
            }
            Ok(Directory {
                path: directory.path,
                files,
            })
        })
        .collect()
}
pub(super) fn read(pool: &AppStore, id: &str, include_files: bool) -> Result<Payload, String> {
    let file = File::open(path(pool, id)?).map_err(|_| "TRASH_UNAVAILABLE")?;
    let mut archive = ZipArchive::new(file).map_err(|_| "TRASH_UNAVAILABLE")?;
    let manifest: Manifest = serde_json::from_reader(
        archive
            .by_name("manifest.json")
            .map_err(|_| "TRASH_UNAVAILABLE")?,
    )
    .map_err(|_| "TRASH_UNAVAILABLE")?;
    if manifest.version != 2 {
        return Err("TRASH_UNAVAILABLE".into());
    }
    let directories = read_directories(
        &mut archive,
        "directories",
        manifest.directories,
        include_files,
    )?;
    let copies = read_directories(&mut archive, "copies", manifest.copies, include_files)?;
    Ok(Payload {
        root: manifest.root,
        directories,
        tables: manifest.tables,
        installations: manifest.installations,
        copies,
    })
}
pub(super) fn remove(pool: &AppStore, id: &str) {
    if let Ok(path) = path(pool, id) {
        let _ = std::fs::remove_file(path);
    }
}

/// Called only at startup or while MIRROR_LOCK excludes active archive writers.
pub(super) fn cleanup_unreferenced(pool: &AppStore) {
    let live = match pool.read(|data| {
        Ok(data
            .table("resource_trash")
            .keys()
            .cloned()
            .collect::<std::collections::HashSet<_>>())
    }) {
        Ok(live) => live,
        Err(_) => return,
    };
    let sample = match path(pool, &uuid::Uuid::nil().to_string()) {
        Ok(path) => path,
        Err(_) => return,
    };
    let Some(parent) = sample.parent() else {
        return;
    };
    let Ok(entries) = std::fs::read_dir(parent) else {
        return;
    };
    for entry in entries.flatten() {
        if !entry.file_type().is_ok_and(|kind| kind.is_file()) {
            continue;
        }
        let name = entry.file_name();
        let name = name.to_string_lossy();
        let abandoned = name
            .strip_suffix(".zip")
            .is_some_and(|id| uuid::Uuid::parse_str(id).is_ok() && !live.contains(id));
        let unfinished = name.strip_prefix(".arsenal-trash-").is_some_and(|tail| {
            tail.len() > 37
                && tail.as_bytes()[36] == b'-'
                && uuid::Uuid::parse_str(&tail[..36]).is_ok()
        });
        if abandoned || unfinished {
            let _ = std::fs::remove_file(entry.path());
        }
    }
}
