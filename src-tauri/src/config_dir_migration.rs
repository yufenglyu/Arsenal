//! Rename the data root without invalidating stored paths or installed links.
use std::fs::{self, OpenOptions};
use std::path::Path;

#[cfg(windows)]
pub(crate) fn compatibility_link(target: &Path, link: &Path) -> Result<(), String> {
    use std::os::windows::process::CommandExt;
    let status = std::process::Command::new("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-Command", "$ErrorActionPreference = 'Stop'; New-Item -ItemType Junction -Path $env:ARSENAL_LEGACY_DIR -Target $env:ARSENAL_DATA_DIR | Out-Null"])
        .env("ARSENAL_LEGACY_DIR", link)
        .env("ARSENAL_DATA_DIR", std::path::absolute(target).map_err(|e| e.to_string())?)
        .creation_flags(0x08000000)
        .output()
        .map_err(|_| "Could not create the legacy configuration link".to_string())?;
    if status.status.success() {
        Ok(())
    } else {
        Err("Could not create the legacy configuration link".into())
    }
}

#[cfg(unix)]
pub(crate) fn compatibility_link(target: &Path, link: &Path) -> Result<(), String> {
    std::os::unix::fs::symlink(target.file_name().ok_or("Invalid configuration directory")?, link)
        .map_err(|_| "Could not create the legacy configuration link".into())
}

pub fn migrate(target: &Path) -> Result<(), String> {
    if target.file_name().and_then(|name| name.to_str()) != Some(".arsenal") {
        return Ok(());
    }
    let parent = target.parent().ok_or("Invalid configuration directory")?;
    let old = parent.join(".skillshub");
    let pending = target.join(".arsenal-migration.pending");
    if fs::symlink_metadata(&old).is_err() && !pending.is_file() {
        return Ok(());
    }
    let migration_lock = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(parent.join(".arsenal-migration.lock"))
        .map_err(|e| e.to_string())?;
    migration_lock
        .try_lock()
        .map_err(|_| "Configuration migration is already running".to_string())?;
    if fs::symlink_metadata(&old).is_err() && pending.is_file() {
        compatibility_link(target, &old)?;
        fs::remove_file(&pending).map_err(|e| e.to_string())?;
        return Ok(());
    }
    if matches!((fs::canonicalize(&old), fs::canonicalize(target)), (Ok(old_path), Ok(new_path)) if old_path == new_path)
    {
        if pending.is_file() {
            fs::remove_file(pending).map_err(|e| e.to_string())?;
        }
        return Ok(());
    }
    if target.exists() {
        return Err("Both .arsenal and .skillshub exist with separate data. Resolve the directory conflict before starting Arsenal; neither directory was changed.".into());
    }
    if !old.is_dir()
        || fs::symlink_metadata(&old)
            .map_err(|e| e.to_string())?
            .file_type()
            .is_symlink()
    {
        return Err(
            "Legacy .skillshub is not a regular directory; migrate its target explicitly.".into(),
        );
    }
    // All application writers hold this gate; do not move a live data directory.
    let gate = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(old.join(".store-access.lock"))
        .map_err(|e| e.to_string())?;
    gate.try_lock().map_err(|_| {
        "Configuration is in use. Close the older application before migrating to .arsenal."
            .to_string()
    })?;
    fs::write(
        old.join(".arsenal-migration.pending"),
        b"legacy-root-rename-v1",
    )
    .map_err(|e| e.to_string())?;
    // Windows cannot rename a directory containing an open lock file. An older
    // writer's open gate also prevents the rename, so a race fails safely.
    #[cfg(windows)]
    drop(gate);
    fs::rename(&old, target)
        .map_err(|e| format!("Could not migrate configuration directory: {e}"))?;
    if let Err(error) = compatibility_link(target, &old) {
        fs::rename(target, &old).map_err(|_| "Migration could not create the compatibility link or restore the old directory. Data remains in .arsenal; repair the legacy link before restarting.".to_string())?;
        return Err(error);
    }
    fs::remove_file(pending).map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrates_without_changing_data_and_preserves_old_paths() {
        let temp = tempfile::tempdir().unwrap();
        let old = temp.path().join(".skillshub");
        let new = temp.path().join(".arsenal");
        fs::create_dir_all(old.join("library/example")).unwrap();
        fs::write(old.join("library/example/SKILL.md"), "original content").unwrap();
        fs::write(old.join("config.json"), "original configuration").unwrap();
        migrate(&new).unwrap();
        assert_eq!(
            fs::read(new.join("config.json")).unwrap(),
            b"original configuration"
        );
        assert_eq!(
            fs::read(old.join("library/example/SKILL.md")).unwrap(),
            b"original content"
        );
        assert_eq!(
            fs::canonicalize(&old).unwrap(),
            fs::canonicalize(&new).unwrap()
        );
        migrate(&new).unwrap();
        // Remove only the compatibility link before TempDir recursively cleans fixtures.
        #[cfg(windows)]
        fs::remove_dir(old).unwrap();
        #[cfg(unix)]
        fs::remove_file(old).unwrap();
    }

    #[test]
    fn refuses_conflicts_and_active_writers() {
        let temp = tempfile::tempdir().unwrap();
        let old = temp.path().join(".skillshub");
        let new = temp.path().join(".arsenal");
        fs::create_dir(&old).unwrap();
        let gate = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(old.join(".store-access.lock"))
            .unwrap();
        gate.try_lock().unwrap();
        assert!(migrate(&new).unwrap_err().contains("in use"));
        assert!(!new.exists());
        drop(gate);
        fs::create_dir(&new).unwrap();
        assert!(migrate(&new).unwrap_err().contains("separate data"));
        assert!(old.is_dir() && new.is_dir());
    }

    #[test]
    fn resumes_after_directory_rename_and_reopens_json_store() {
        let temp = tempfile::tempdir().unwrap();
        let old = temp.path().join(".skillshub");
        let new = temp.path().join(".arsenal");
        drop(crate::storage::store::Store::open(&old).unwrap());
        fs::write(
            old.join(".arsenal-migration.pending"),
            "legacy-root-rename-v1",
        )
        .unwrap();
        fs::rename(&old, &new).unwrap();
        migrate(&new).unwrap();
        assert!(!new.join(".arsenal-migration.pending").exists());
        drop(crate::storage::store::Store::open(&new).unwrap());
        assert_eq!(
            fs::canonicalize(&old).unwrap(),
            fs::canonicalize(&new).unwrap()
        );
        #[cfg(windows)]
        fs::remove_dir(old).unwrap();
        #[cfg(unix)]
        fs::remove_file(old).unwrap();
    }
}
