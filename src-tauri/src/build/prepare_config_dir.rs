use std::{
    env, fs,
    path::{Path, PathBuf},
    process,
};

// The executable may replace only this repository's generated packaging output.
// Other destinations are accepted only when absent, preventing accidental cleanup
// of a user's configuration directory.
fn validate_destination(dest: &Path, generated: &Path) -> Result<PathBuf, String> {
    let parent = dest
        .parent()
        .ok_or("Destination requires a parent directory")?
        .canonicalize()
        .map_err(|e| e.to_string())?;
    let name = dest
        .file_name()
        .ok_or("Destination requires a directory name")?;
    let resolved = parent.join(name);
    if let Ok(meta) = fs::symlink_metadata(&resolved) {
        if meta.file_type().is_symlink() || !meta.is_dir() {
            return Err("Refusing to replace a linked or non-directory destination".into());
        }
        let expected_parent = generated
            .parent()
            .ok_or("Invalid packaging path")?
            .canonicalize()
            .map_err(|e| e.to_string())?;
        if resolved != expected_parent.join(generated.file_name().ok_or("Invalid packaging path")?)
        {
            return Err(
                "Existing destination is not the repository generated packaging directory".into(),
            );
        }
    }
    Ok(resolved)
}

async fn prepare(dest: &Path, generated: &Path) -> Result<(), String> {
    let dest = validate_destination(dest, generated)?;
    let parent = dest.parent().ok_or("Invalid destination")?;
    let stage = tempfile::Builder::new()
        .prefix(".packaged-config-")
        .tempdir_in(parent)
        .map_err(|e| e.to_string())?;
    let fresh = stage.path().join("fresh");
    skills_manage_lib::db::prepare_config_dir(&fresh).await?;
    let previous = stage.path().join("previous");
    let had_previous = dest.exists();
    if had_previous {
        fs::rename(&dest, &previous).map_err(|e| e.to_string())?;
    }
    if let Err(error) = fs::rename(&fresh, &dest) {
        if had_previous && fs::rename(&previous, &dest).is_err() {
            // Preserve the original tree for manual recovery if replacement
            // and rollback both fail; TempDir must not remove it.
            let _ = stage.keep();
            return Err(
                "Packaging publication and rollback failed; recovery directory retained".into(),
            );
        }
        return Err(error.to_string());
    }
    Ok(())
}
fn main() {
    let dest = env::args().nth(1).unwrap_or_else(|| {
        eprintln!("usage: prepare-config-dir <config-dir>");
        process::exit(2);
    });
    let generated = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources/packaged-config");
    if let Err(error) = tauri::async_runtime::block_on(prepare(Path::new(&dest), &generated)) {
        eprintln!("Failed to prepare config directory: {error}");
        process::exit(1);
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn existing_user_directory_is_rejected_and_preserved() {
        let root = tempfile::tempdir().unwrap();
        let user = root.path().join("user");
        fs::create_dir(&user).unwrap();
        fs::write(user.join("keep"), "preserve").unwrap();
        assert!(validate_destination(&user, &root.path().join("packaged-config")).is_err());
        assert_eq!(fs::read_to_string(user.join("keep")).unwrap(), "preserve");
    }
    #[tokio::test]
    async fn generated_output_is_fresh_and_contains_no_legacy_or_history() {
        let root = tempfile::tempdir().unwrap();
        let dest = root.path().join("packaged-config");
        fs::create_dir(&dest).unwrap();
        fs::write(dest.join("db.sqlite"), "old").unwrap();
        fs::create_dir(dest.join("store")).unwrap();
        fs::write(dest.join("store/old-history"), "old").unwrap();
        prepare(&dest, &dest).await.unwrap();
        assert!(!dest.join("db.sqlite").exists());
        assert!(!dest.join("store/old-history").exists());
        assert!(dest.join("store/manifest.json").is_file());
        assert!(dest.join("config.json").is_file());
        prepare(&dest, &dest).await.unwrap();
        assert!(dest.join("store/manifest.json").is_file());
    }
}
