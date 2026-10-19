use std::collections::HashSet;
use std::ffi::OsString;
use std::path::{Path, PathBuf};

fn resolve_home_dir_from_env_vars(
    home: Option<OsString>,
    userprofile: Option<OsString>,
    homedrive: Option<OsString>,
    homepath: Option<OsString>,
) -> PathBuf {
    if let Some(home) = home.filter(|value| !value.is_empty()) {
        return PathBuf::from(home);
    }

    if let Some(userprofile) = userprofile.filter(|value| !value.is_empty()) {
        return PathBuf::from(userprofile);
    }

    if let (Some(homedrive), Some(homepath)) = (homedrive, homepath) {
        if !homedrive.is_empty() && !homepath.is_empty() {
            let combined = format!(
                "{}{}",
                homedrive.to_string_lossy(),
                homepath.to_string_lossy()
            );
            return PathBuf::from(combined);
        }
    }

    std::env::temp_dir()
}

pub fn resolve_home_dir() -> PathBuf {
    resolve_home_dir_from_env_vars(
        std::env::var_os("HOME"),
        std::env::var_os("USERPROFILE"),
        std::env::var_os("HOMEDRIVE"),
        std::env::var_os("HOMEPATH"),
    )
}

pub fn default_app_data_dir() -> PathBuf {
    resolve_home_dir().join(".arsenal")
}

fn appimage_dir_from_env() -> Option<PathBuf> {
    let value = std::env::var_os("APPIMAGE")?;
    if value.is_empty() {
        return None;
    }
    PathBuf::from(value).parent().map(Path::to_path_buf)
}

pub fn app_data_dir() -> PathBuf {
    let exe_path = std::env::current_exe().ok();
    let exe_dir = exe_path.as_deref().and_then(|path| path.parent());
    let appimage_dir = appimage_dir_from_env();
    resolve_app_data_dir_from_extra(exe_dir, appimage_dir.as_deref(), &resolve_home_dir())
}

fn read_dir_pointer(file: &Path, home_dir: &Path) -> Option<PathBuf> {
    let contents = std::fs::read_to_string(file).ok()?;
    let trimmed = contents.trim();
    if trimmed.is_empty() {
        return None;
    }
    normalize_app_data_dir_with_home(trimmed, home_dir).ok()
}

fn macos_app_bundle_parent(exe_dir: &Path) -> Option<PathBuf> {
    if exe_dir.file_name()?.to_str()? != "MacOS" {
        return None;
    }
    let contents = exe_dir.parent()?;
    if contents.file_name()?.to_str()? != "Contents" {
        return None;
    }
    let app_bundle = contents.parent()?;
    if app_bundle.extension().and_then(|ext| ext.to_str()) != Some("app") {
        return None;
    }
    app_bundle.parent().map(Path::to_path_buf)
}

fn portable_anchor_dirs(exe_dir: Option<&Path>, extra_dir: Option<&Path>) -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Some(exe_dir) = exe_dir {
        dirs.push(exe_dir.to_path_buf());
        if let Some(parent) = macos_app_bundle_parent(exe_dir) {
            if !dirs.iter().any(|dir| dir == &parent) {
                dirs.push(parent);
            }
        }
    }
    if let Some(extra_dir) = extra_dir {
        if !dirs.iter().any(|dir| dir == extra_dir) {
            dirs.push(extra_dir.to_path_buf());
        }
    }
    dirs
}

fn existing_portable_config_dir(
    exe_dir: Option<&Path>,
    extra_dir: Option<&Path>,
) -> Option<PathBuf> {
    for dir in portable_anchor_dirs(exe_dir, extra_dir) {
        let portable = dir.join(".arsenal");
        if portable.is_dir() {
            return Some(portable);
        }
    }
    None
}

pub fn resolve_app_data_dir_from(exe_dir: Option<&Path>, home_dir: &Path) -> PathBuf {
    resolve_app_data_dir_from_extra(exe_dir, None, home_dir)
}

fn resolve_app_data_dir_from_extra(
    exe_dir: Option<&Path>,
    extra_dir: Option<&Path>,
    home_dir: &Path,
) -> PathBuf {
    if let Some(exe_dir) = exe_dir {
        if let Some(path) = read_dir_pointer(&exe_dir.join("arsenal-config-path"), home_dir) {
            return path;
        }
    }
    if let Some(portable) = existing_portable_config_dir(exe_dir, extra_dir) {
        return portable;
    }
    if let Some(path) = read_dir_pointer(&home_dir.join(".arsenal-config-path"), home_dir) {
        return path;
    }
    home_dir.join(".arsenal")
}

pub fn normalize_app_data_dir_with_home(path: &str, home_dir: &Path) -> Result<PathBuf, String> {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return Err("Config directory cannot be empty".to_string());
    }
    let expanded = expand_home_path_with_home(trimmed, home_dir);
    if expanded.file_name().and_then(|name| name.to_str()) == Some(".arsenal") {
        Ok(expanded)
    } else {
        Ok(expanded.join(".arsenal"))
    }
}

pub fn normalize_app_data_dir(path: &str) -> Result<PathBuf, String> {
    normalize_app_data_dir_with_home(path, &resolve_home_dir())
}

fn auto_app_data_dir_without_pointer(
    exe_dir: Option<&Path>,
    extra_dir: Option<&Path>,
    home_dir: &Path,
) -> PathBuf {
    existing_portable_config_dir(exe_dir, extra_dir).unwrap_or_else(|| home_dir.join(".arsenal"))
}

fn remove_pointer_file(file: &Path) -> Result<(), String> {
    if file.exists() {
        std::fs::remove_file(file)
            .map_err(|e| format!("Failed to clear config path override: {}", e))?;
    }
    Ok(())
}

pub fn write_app_data_dir_override_with(
    path: &Path,
    exe_dir: Option<&Path>,
    home_dir: &Path,
) -> Result<(), String> {
    write_app_data_dir_override_with_extra(path, exe_dir, None, home_dir)
}

fn write_app_data_dir_override_with_extra(
    path: &Path,
    exe_dir: Option<&Path>,
    extra_dir: Option<&Path>,
    home_dir: &Path,
) -> Result<(), String> {
    let auto_path = auto_app_data_dir_without_pointer(exe_dir, extra_dir, home_dir);
    let home_pointer = home_dir.join(".arsenal-config-path");
    let exe_pointer = exe_dir.map(|dir| dir.join("arsenal-config-path"));
    if path == auto_path {
        if let Some(pointer) = &exe_pointer {
            remove_pointer_file(pointer)?;
        }
        return remove_pointer_file(&home_pointer);
    }

    let serialized = path_to_string(path);
    std::fs::write(&home_pointer, &serialized)
        .map_err(|e| format!("Failed to save config path: {}", e))?;
    if let Some(pointer) = exe_pointer {
        let _ = std::fs::write(pointer, &serialized);
    }
    Ok(())
}

pub fn write_app_data_dir_override(path: &Path) -> Result<(), String> {
    let exe_path = std::env::current_exe().ok();
    let exe_dir = exe_path.as_deref().and_then(|path| path.parent());
    let appimage_dir = appimage_dir_from_env();
    write_app_data_dir_override_with_extra(
        path,
        exe_dir,
        appimage_dir.as_deref(),
        &resolve_home_dir(),
    )
}

pub fn copy_app_data_dir(from: &Path, to: &Path) -> Result<(), String> {
    if from == to {
        return Ok(());
    }
    if to.starts_with(from) {
        return Err("Config directory cannot be inside the current config directory".to_string());
    }
    // Share the app/migrator gate before creating destination store files.
    std::fs::create_dir_all(to).map_err(|e| e.to_string())?;
    let access_lock = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(to.join(".store-access.lock"))
        .map_err(|e| e.to_string())?;
    access_lock
        .try_lock()
        .map_err(|_| "Destination data directory is already open".to_string())?;
    let store = to.join("store");
    std::fs::create_dir_all(&store).map_err(|e| e.to_string())?;
    let lock = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(store.join("store.lock"))
        .map_err(|e| e.to_string())?;
    lock.try_lock()
        .map_err(|_| "Destination data directory is already open".to_string())?;
    if store.join("manifest.json").exists() {
        return Err("Destination already contains JSON storage".into());
    }
    copy_app_data_dir_recursive(from, to)?;
    let manifest = from.join("store/manifest.json");
    if manifest.exists() {
        use std::io::Write;
        let pending = store.join("copy-manifest.pending");
        let mut file = std::fs::File::create(&pending).map_err(|e| e.to_string())?;
        file.write_all(&std::fs::read(manifest).map_err(|e| e.to_string())?)
            .and_then(|_| file.sync_all())
            .map_err(|e| e.to_string())?;
        drop(file);
        std::fs::rename(pending, store.join("manifest.json")).map_err(|e| e.to_string())?;
    }
    Ok(())
}

fn copy_app_data_dir_recursive(src: &Path, dst: &Path) -> Result<(), String> {
    std::fs::create_dir_all(dst).map_err(|e| {
        format!(
            "Failed to create config directory '{}': {}",
            dst.display(),
            e
        )
    })?;
    for entry in std::fs::read_dir(src)
        .map_err(|e| format!("Failed to read config directory '{}': {}", src.display(), e))?
    {
        let entry = entry.map_err(|e| format!("Failed to read directory entry: {}", e))?;
        let name = entry.file_name();
        if name == ".store-access.lock"
            || name == ".json-migration.lock"
            || name == "database-path"
            || name == "db.sqlite"
            || name == "db.sqlite-wal"
            || name == "db.sqlite-shm"
            || (src.file_name().is_some_and(|n| n == "store")
                && (name == "store.lock" || name == "manifest.json"))
            || name.to_string_lossy().ends_with(".pending")
        {
            continue;
        }
        let src_path = entry.path();
        let dst_path = dst.join(&name);
        let file_type = entry
            .file_type()
            .map_err(|e| format!("Failed to determine file type: {}", e))?;
        if file_type.is_dir() {
            copy_app_data_dir_recursive(&src_path, &dst_path)?;
        } else {
            std::fs::copy(&src_path, &dst_path).map_err(|e| {
                format!(
                    "Failed to copy '{}' -> '{}': {}",
                    src_path.display(),
                    dst_path.display(),
                    e
                )
            })?;
        }
    }
    Ok(())
}

pub fn central_skills_dir() -> PathBuf {
    resolve_home_dir().join(".agents").join("skills")
}

fn expand_home_path_with_home(path: &str, home_dir: &Path) -> PathBuf {
    let trimmed = path.trim();
    if trimmed == "~" {
        return home_dir.to_path_buf();
    }

    if let Some(rest) = trimmed
        .strip_prefix("~/")
        .or_else(|| trimmed.strip_prefix("~\\"))
    {
        return home_dir.join(rest);
    }

    PathBuf::from(trimmed)
}

pub fn expand_home_path(path: &str) -> PathBuf {
    expand_home_path_with_home(path, &resolve_home_dir())
}

pub fn path_to_string(path: &Path) -> String {
    let display = path.to_string_lossy().into_owned();
    #[cfg(windows)]
    {
        display.replace('/', "\\")
    }
    #[cfg(not(windows))]
    {
        display
    }
}

#[cfg(windows)]
mod windows_symlink {
    use std::io;
    use std::os::windows::ffi::OsStrExt;
    use std::path::Path;
    use std::ptr;

    const INVALID_FILE_ATTRIBUTES: u32 = u32::MAX;
    const FILE_ATTRIBUTE_READONLY: u32 = 0x0000_0001;
    const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0000_0400;
    const FILE_SHARE_READ: u32 = 0x0000_0001;
    const FILE_SHARE_WRITE: u32 = 0x0000_0002;
    const FILE_SHARE_DELETE: u32 = 0x0000_0004;
    const OPEN_EXISTING: u32 = 3;
    const FILE_FLAG_BACKUP_SEMANTICS: u32 = 0x0200_0000;
    const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;
    const DELETE: u32 = 0x0001_0000;
    const INVALID_HANDLE_VALUE: isize = -1;
    const FILE_DISPOSITION_INFO_CLASS: i32 = 4;

    #[repr(C)]
    struct FileDispositionInfo {
        delete_file: u8,
    }

    #[link(name = "kernel32")]
    extern "system" {
        fn GetFileAttributesW(lp_file_name: *const u16) -> u32;
        fn SetFileAttributesW(lp_file_name: *const u16, dw_file_attributes: u32) -> i32;
        fn CreateFileW(
            lp_file_name: *const u16,
            dw_desired_access: u32,
            dw_share_mode: u32,
            lp_security_attributes: *mut core::ffi::c_void,
            dw_creation_disposition: u32,
            dw_flags_and_attributes: u32,
            h_template_file: *mut core::ffi::c_void,
        ) -> isize;
        fn SetFileInformationByHandle(
            h_file: isize,
            file_information_class: i32,
            lp_file_information: *const core::ffi::c_void,
            dw_buffer_size: u32,
        ) -> i32;
        fn CloseHandle(h_object: isize) -> i32;
    }

    fn to_wide(path: &Path) -> Vec<u16> {
        path.as_os_str().encode_wide().chain([0]).collect()
    }

    pub fn set_readonly(path: &Path, readonly: bool) -> io::Result<()> {
        let wide = to_wide(path);
        unsafe {
            let attrs = GetFileAttributesW(wide.as_ptr());
            if attrs == INVALID_FILE_ATTRIBUTES {
                return Err(io::Error::last_os_error());
            }
            let next = if readonly {
                attrs | FILE_ATTRIBUTE_READONLY
            } else {
                attrs & !FILE_ATTRIBUTE_READONLY
            };
            if next == attrs {
                return Ok(());
            }
            if SetFileAttributesW(wide.as_ptr(), next) == 0 {
                return Err(io::Error::last_os_error());
            }
        }
        Ok(())
    }

    pub fn clear_readonly(path: &Path) {
        let _ = set_readonly(path, false);
    }

    #[cfg(test)]
    pub fn set_readonly_for_test(path: &Path, readonly: bool) -> io::Result<()> {
        set_readonly(path, readonly)
    }

    pub fn is_reparse_point(path: &Path) -> bool {
        let wide = to_wide(path);
        unsafe {
            let attrs = GetFileAttributesW(wide.as_ptr());
            attrs != INVALID_FILE_ATTRIBUTES && attrs & FILE_ATTRIBUTE_REPARSE_POINT != 0
        }
    }

    pub fn delete_reparse_point(path: &Path) -> io::Result<()> {
        let wide = to_wide(path);
        unsafe {
            let handle = CreateFileW(
                wide.as_ptr(),
                DELETE,
                FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
                ptr::null_mut(),
                OPEN_EXISTING,
                FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT,
                ptr::null_mut(),
            );
            if handle == 0 || handle == INVALID_HANDLE_VALUE {
                return Err(io::Error::last_os_error());
            }
            let info = FileDispositionInfo { delete_file: 1 };
            let ok = SetFileInformationByHandle(
                handle,
                FILE_DISPOSITION_INFO_CLASS,
                &info as *const FileDispositionInfo as *const core::ffi::c_void,
                std::mem::size_of::<FileDispositionInfo>() as u32,
            );
            CloseHandle(handle);
            if ok == 0 {
                return Err(io::Error::last_os_error());
            }
        }
        Ok(())
    }
}

#[cfg(windows)]
pub fn remove_symlink_path(path: &Path) -> Result<(), String> {
    windows_symlink::clear_readonly(path);
    match std::fs::remove_dir(path) {
        Ok(()) => Ok(()),
        Err(dir_error) => match std::fs::remove_file(path) {
            Ok(()) => Ok(()),
            Err(file_error) => {
                if windows_symlink::is_reparse_point(path) {
                    windows_symlink::delete_reparse_point(path).map_err(|reparse_error| {
                        format!(
                            "directory symlink removal failed: {}; file symlink removal failed: {}; reparse point deletion failed: {}",
                            dir_error, file_error, reparse_error
                        )
                    })
                } else {
                    Err(format!(
                        "directory symlink removal failed: {}; file symlink removal failed: {}",
                        dir_error, file_error
                    ))
                }
            }
        },
    }
}

#[cfg(not(windows))]
pub fn remove_symlink_path(path: &Path) -> Result<(), String> {
    std::fs::remove_file(path).map_err(|e| e.to_string())
}

fn path_without_verbatim_prefix(path: &Path) -> PathBuf {
    #[cfg(windows)]
    {
        let display = path.to_string_lossy();
        if let Some(stripped) = display.strip_prefix(r"\\?\UNC\") {
            return PathBuf::from(format!(r"\\{stripped}"));
        }
        if let Some(stripped) = display.strip_prefix(r"\\?\") {
            return PathBuf::from(stripped);
        }
    }
    path.to_path_buf()
}

/// Argument suitable for Finder / Explorer / xdg-open.
///
/// Windows Explorer treats each `/segment` as a command-line switch, so a path
/// like `C:/Users/me/.agents/skills` is parsed as `/Users`, `/me`, `/.agents`
/// and `/skills`, then Explorer falls back to the Documents folder. It also
/// does not understand `\\?\` verbatim prefixes from `canonicalize`.
pub fn path_for_file_manager(path: &Path) -> String {
    let display = path_to_string(&path_without_verbatim_prefix(path));
    #[cfg(windows)]
    {
        display.replace('/', "\\")
    }
    #[cfg(not(windows))]
    {
        display
    }
}

/// Expand `~`, verify the path exists, then normalize it for the file manager.
pub fn resolve_path_for_file_manager(path: &str) -> Result<String, String> {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return Err("Path is empty".to_string());
    }
    let expanded = expand_home_path(trimmed);
    if !expanded.exists() {
        return Err(format!("Path does not exist: {}", path));
    }
    let resolved = std::fs::canonicalize(&expanded).unwrap_or(expanded);
    Ok(path_for_file_manager(&resolved))
}

fn normalize_path_for_comparison(path: &Path) -> String {
    let value = path_without_verbatim_prefix(path)
        .to_string_lossy()
        .replace('\\', "/");
    #[cfg(windows)]
    let value = value.to_lowercase();
    value.trim_end_matches('/').to_string()
}

/// True when `left` and `right` resolve to the same filesystem entry.
///
/// Prefer canonicalize so Windows `\\?\` prefixes, trailing separators, and
/// equivalent spellings of the same directory compare as one entry. If either
/// path cannot be canonicalized (for example it does not exist yet), fall back
/// to a normalized lexical comparison after stripping verbatim prefixes.
pub fn paths_resolve_to_same_entry(left: &Path, right: &Path) -> bool {
    match (std::fs::canonicalize(left), std::fs::canonicalize(right)) {
        (Ok(left), Ok(right)) => {
            path_without_verbatim_prefix(&left) == path_without_verbatim_prefix(&right)
        }
        _ => normalize_path_for_comparison(left) == normalize_path_for_comparison(right),
    }
}

/// Remove empty real directories walking from `removed`'s parent up to, but
/// not including, `stop_at`. Symlinks and non-empty folders are left intact.
pub fn prune_empty_parent_dirs(removed: &Path, stop_at: &Path) -> Result<(), String> {
    let stop_at = path_without_verbatim_prefix(stop_at);
    let mut current = match removed.parent() {
        Some(parent) => parent.to_path_buf(),
        None => return Ok(()),
    };

    loop {
        let current_normalized = path_without_verbatim_prefix(&current);
        if current_normalized == stop_at || !current_normalized.starts_with(&stop_at) {
            break;
        }

        let Ok(meta) = std::fs::symlink_metadata(&current) else {
            current = match current.parent() {
                Some(parent) => parent.to_path_buf(),
                None => break,
            };
            continue;
        };
        if meta.file_type().is_symlink() || !meta.is_dir() {
            break;
        }

        let is_empty = match std::fs::read_dir(&current) {
            Ok(mut entries) => entries.next().is_none(),
            Err(_) => break,
        };
        if !is_empty {
            break;
        }

        std::fs::remove_dir(&current).map_err(|error| {
            format!(
                "Failed to remove leftover empty directory '{}': {}",
                current.display(),
                error
            )
        })?;

        current = match current.parent() {
            Some(parent) => parent.to_path_buf(),
            None => break,
        };
    }

    Ok(())
}

/// Recursively remove empty real directories under `root`, leaving `root` itself.
pub fn prune_empty_directories_under(root: &Path) {
    let _ = prune_empty_directories_under_inner(root, true);
}

fn prune_empty_directories_under_inner(path: &Path, is_root: bool) -> bool {
    let Ok(meta) = std::fs::symlink_metadata(path) else {
        return true;
    };
    if meta.file_type().is_symlink() || !meta.is_dir() {
        return false;
    }

    let Ok(entries) = std::fs::read_dir(path) else {
        return false;
    };
    let children: Vec<_> = entries.flatten().map(|entry| entry.path()).collect();
    for child in children {
        prune_empty_directories_under_inner(&child, false);
    }

    if is_root {
        return false;
    }

    match std::fs::read_dir(path) {
        Ok(remaining) => {
            if remaining.count() == 0 {
                std::fs::remove_dir(path).is_ok()
            } else {
                false
            }
        }
        Err(_) => false,
    }
}

fn contains_skill_markdown(path: &Path, visited: &mut HashSet<PathBuf>) -> bool {
    if path.join("SKILL.md").is_file() {
        return true;
    }

    let Ok(meta) = std::fs::symlink_metadata(path) else {
        return false;
    };
    let is_dir = if meta.file_type().is_symlink() {
        std::fs::metadata(path)
            .map(|resolved| resolved.is_dir())
            .unwrap_or(false)
    } else {
        meta.is_dir()
    };
    if !is_dir {
        return false;
    }

    if let Ok(canonical) = std::fs::canonicalize(path) {
        if !visited.insert(canonical) {
            return false;
        }
    }

    let Ok(entries) = std::fs::read_dir(path) else {
        return false;
    };
    entries
        .flatten()
        .any(|entry| contains_skill_markdown(&entry.path(), visited))
}

fn remove_leftover_skill_group(path: &Path) -> Result<(), String> {
    let meta = match std::fs::symlink_metadata(path) {
        Ok(meta) => meta,
        Err(_) => return Ok(()),
    };
    if meta.file_type().is_symlink() {
        return remove_symlink_path(path);
    }
    if meta.is_dir() {
        let children: Vec<_> = std::fs::read_dir(path)
            .map_err(|error| {
                format!(
                    "Failed to read leftover skill group '{}': {}",
                    path.display(),
                    error
                )
            })?
            .flatten()
            .map(|entry| entry.path())
            .collect();
        for child in children {
            remove_leftover_skill_group(&child)?;
        }
        return std::fs::remove_dir(path).map_err(|error| {
            format!(
                "Failed to remove leftover skill group '{}': {}",
                path.display(),
                error
            )
        });
    }

    std::fs::remove_file(path).map_err(|error| {
        format!(
            "Failed to remove leftover skill file '{}': {}",
            path.display(),
            error
        )
    })
}

/// Delete a mirrored skill group if it no longer contains any `SKILL.md`.
/// Empty leftover directories and dangling symlinks are removed; live skills stay.
pub fn remove_path_if_no_skill_markdown(path: &Path) -> Result<(), String> {
    if std::fs::symlink_metadata(path).is_err() {
        return Ok(());
    }
    if contains_skill_markdown(path, &mut HashSet::new()) {
        prune_empty_directories_under(path);
        return Ok(());
    }
    remove_leftover_skill_group(path)
}

pub fn sanitize_path_segment(value: &str) -> String {
    let segment = value
        .trim()
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.') {
                ch
            } else {
                '-'
            }
        })
        .collect::<String>()
        .trim_matches(['-', '.', ' '])
        .to_string();

    if segment.is_empty() {
        "unknown".to_string()
    } else {
        segment
    }
}

pub fn source_grouped_skill_dir(
    central_root: &Path,
    source_author: Option<&str>,
    source_repo: Option<&str>,
    fallback_group: Option<&str>,
    skill_id: &str,
) -> PathBuf {
    let (author, repo) = match source_repo.and_then(|repo| repo.split_once('/')) {
        Some((author, repo)) => (author, repo),
        None => (
            source_author.unwrap_or("remote-source"),
            source_repo.or(fallback_group).unwrap_or("imported-skills"),
        ),
    };

    central_root
        .join(sanitize_path_segment(author))
        .join(sanitize_path_segment(repo))
        .join(sanitize_path_segment(skill_id))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths_resolve_to_same_entry_matches_canonical_and_raw() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path();
        let canonical = std::fs::canonicalize(path).expect("canonicalize");
        assert!(paths_resolve_to_same_entry(path, &canonical));
        assert!(paths_resolve_to_same_entry(&path.join("."), path,));
    }

    #[test]
    fn paths_resolve_to_same_entry_rejects_distinct_dirs() {
        let left = tempfile::tempdir().expect("left");
        let right = tempfile::tempdir().expect("right");
        assert!(!paths_resolve_to_same_entry(left.path(), right.path()));
    }

    #[test]
    fn paths_resolve_to_same_entry_normalizes_missing_windows_prefixes() {
        let left = Path::new(r"\\?\C:\Users\alice\.agents\skills");
        let right = Path::new(r"C:\Users\alice\.agents\skills\");
        assert_eq!(
            normalize_path_for_comparison(left),
            normalize_path_for_comparison(right)
        );
        assert!(paths_resolve_to_same_entry(left, right));
    }

    #[test]
    fn resolve_home_dir_prefers_home() {
        let resolved = resolve_home_dir_from_env_vars(
            Some(OsString::from("/tmp/home")),
            Some(OsString::from("/tmp/profile")),
            Some(OsString::from("C:")),
            Some(OsString::from("\\Users\\fallback")),
        );
        assert_eq!(resolved, PathBuf::from("/tmp/home"));
    }

    #[test]
    fn resolve_home_dir_falls_back_to_userprofile() {
        let resolved = resolve_home_dir_from_env_vars(
            None,
            Some(OsString::from("C:\\Users\\alice")),
            None,
            None,
        );
        assert_eq!(resolved, PathBuf::from("C:\\Users\\alice"));
    }

    #[test]
    fn resolve_home_dir_falls_back_to_home_drive_and_path() {
        let resolved = resolve_home_dir_from_env_vars(
            None,
            None,
            Some(OsString::from("C:")),
            Some(OsString::from("\\Users\\bob")),
        );
        assert_eq!(resolved, PathBuf::from("C:\\Users\\bob"));
    }

    #[test]
    fn expand_home_path_expands_unix_style_tilde() {
        let expanded = expand_home_path_with_home("~/.claude/skills", Path::new("/tmp/home"));
        assert_eq!(expanded, PathBuf::from("/tmp/home/.claude/skills"));
    }

    #[test]
    fn expand_home_path_expands_windows_style_tilde() {
        let expanded =
            expand_home_path_with_home("~\\.claude\\skills", Path::new("C:\\Users\\alice"));
        assert_eq!(expanded, PathBuf::from("C:\\Users\\alice/.claude\\skills"));
    }

    #[test]
    fn expand_home_path_leaves_absolute_paths_unchanged() {
        let expanded =
            expand_home_path_with_home("/opt/skills/custom", Path::new("/tmp/ignored-home"));
        assert_eq!(expanded, PathBuf::from("/opt/skills/custom"));
    }

    #[test]
    fn path_for_file_manager_uses_native_windows_separators() {
        let native = path_for_file_manager(Path::new(r"C:/Users/alice/.agents/skills"));
        #[cfg(windows)]
        {
            assert_eq!(native, r"C:\Users\alice\.agents\skills");
            assert!(!native.contains('/'));
        }
        #[cfg(not(windows))]
        {
            assert_eq!(native, "C:/Users/alice/.agents/skills");
        }
    }

    #[test]
    fn path_to_string_uses_native_windows_separators() {
        let native = path_to_string(Path::new(r"C:\Users\alice\.agents/skills/demo"));
        #[cfg(windows)]
        {
            assert_eq!(native, r"C:\Users\alice\.agents\skills\demo");
            assert!(!native.contains('/'));
        }
        #[cfg(not(windows))]
        {
            assert_eq!(native, r"C:\Users\alice\.agents/skills/demo");
        }
    }

    #[test]
    fn path_for_file_manager_strips_windows_verbatim_prefix() {
        let native = path_for_file_manager(Path::new(r"\\?\C:\Users\alice\.agents\skills"));
        #[cfg(windows)]
        {
            assert_eq!(native, r"C:\Users\alice\.agents\skills");
        }
        #[cfg(not(windows))]
        {
            assert_eq!(native, r"\\?\C:\Users\alice\.agents\skills");
        }
    }

    #[test]
    fn resolve_path_for_file_manager_normalizes_dot_folders() {
        let dir = tempfile::tempdir().expect("tempdir");
        let hidden = dir.path().join(".agents").join("skills");
        std::fs::create_dir_all(&hidden).expect("create hidden skills dir");

        let forward_slash = hidden.to_string_lossy().replace('\\', "/");
        let resolved = resolve_path_for_file_manager(&forward_slash).expect("resolve");

        #[cfg(windows)]
        {
            assert!(
                !resolved.contains('/'),
                "explorer.exe treats /segment as switches and opens Documents: {resolved}"
            );
            assert!(
                !resolved.starts_with(r"\\?\"),
                "explorer.exe ignores verbatim prefixes: {resolved}"
            );
        }
        assert!(Path::new(&resolved).is_dir());
        assert!(paths_resolve_to_same_entry(Path::new(&resolved), &hidden));
    }

    #[test]
    fn resolve_path_for_file_manager_rejects_missing_and_empty() {
        let missing = resolve_path_for_file_manager("/nonexistent/path/that/does/not/exist");
        assert!(missing.is_err());
        assert!(resolve_path_for_file_manager("   ").is_err());
    }

    #[test]
    fn source_grouped_skill_dir_uses_author_repo_and_skill_id() {
        let path = source_grouped_skill_dir(
            Path::new("/central"),
            Some("openai"),
            Some("openai/skills"),
            None,
            "brand-guidelines",
        );
        assert_eq!(
            path,
            PathBuf::from("/central")
                .join("openai")
                .join("skills")
                .join("brand-guidelines")
        );
    }

    #[test]
    fn normalize_app_data_dir_appends_arsenal_folder() {
        let path =
            normalize_app_data_dir_with_home("~/data", Path::new("/tmp/home")).expect("path");
        assert_eq!(path, PathBuf::from("/tmp/home/data/.arsenal"));
    }

    #[test]
    fn normalize_app_data_dir_keeps_arsenal_folder() {
        let path = normalize_app_data_dir_with_home("~/custom/.arsenal", Path::new("/tmp/home"))
            .expect("path");
        assert_eq!(path, PathBuf::from("/tmp/home/custom/.arsenal"));
    }

    #[test]
    fn resolve_app_data_dir_uses_portable_folder_next_to_exe() {
        let dir = tempfile::tempdir().expect("tempdir");
        let exe_dir = dir.path().join("app");
        let portable = exe_dir.join(".arsenal");
        std::fs::create_dir_all(&portable).expect("portable");
        let resolved = resolve_app_data_dir_from(Some(&exe_dir), Path::new("/tmp/home"));
        assert_eq!(resolved, portable);
    }

    #[test]
    fn resolve_app_data_dir_uses_arsenal_next_to_macos_app() {
        let dir = tempfile::tempdir().expect("tempdir");
        let stage = dir.path().join("portable");
        let exe_dir = stage.join("SkillsHub.app").join("Contents").join("MacOS");
        let portable = stage.join(".arsenal");
        std::fs::create_dir_all(&exe_dir).expect("macos exe dir");
        std::fs::create_dir_all(&portable).expect("portable");
        let resolved = resolve_app_data_dir_from(Some(&exe_dir), Path::new("/tmp/home"));
        assert_eq!(resolved, portable);
    }

    #[test]
    fn resolve_app_data_dir_prefers_exe_dir_over_macos_app_sibling() {
        let dir = tempfile::tempdir().expect("tempdir");
        let stage = dir.path().join("portable");
        let exe_dir = stage.join("SkillsHub.app").join("Contents").join("MacOS");
        let nested = exe_dir.join(".arsenal");
        let sibling = stage.join(".arsenal");
        std::fs::create_dir_all(&nested).expect("nested portable");
        std::fs::create_dir_all(&sibling).expect("sibling portable");
        let resolved = resolve_app_data_dir_from(Some(&exe_dir), Path::new("/tmp/home"));
        assert_eq!(resolved, nested);
    }

    #[test]
    fn resolve_app_data_dir_uses_appimage_parent_folder() {
        let dir = tempfile::tempdir().expect("tempdir");
        let exe_dir = dir.path().join("mount").join("usr").join("bin");
        let appimage_dir = dir.path().join("downloads");
        let portable = appimage_dir.join(".arsenal");
        std::fs::create_dir_all(&exe_dir).expect("appimage mount");
        std::fs::create_dir_all(&portable).expect("portable");
        let resolved = resolve_app_data_dir_from_extra(
            Some(&exe_dir),
            Some(&appimage_dir),
            Path::new("/tmp/home"),
        );
        assert_eq!(resolved, portable);
    }

    #[test]
    fn resolve_app_data_dir_uses_exe_pointer_before_portable_folder() {
        let dir = tempfile::tempdir().expect("tempdir");
        let exe_dir = dir.path().join("app");
        let portable = exe_dir.join(".arsenal");
        let custom = dir.path().join("custom").join(".arsenal");
        std::fs::create_dir_all(&portable).expect("portable");
        std::fs::write(
            exe_dir.join("arsenal-config-path"),
            custom.to_str().unwrap(),
        )
        .expect("pointer");
        let resolved = resolve_app_data_dir_from(Some(&exe_dir), Path::new("/tmp/home"));
        assert_eq!(resolved, custom);
    }

    #[test]
    fn legacy_pointer_and_portable_directory_are_ignored() {
        let temp = tempfile::tempdir().unwrap();
        let home = temp.path().join("home");
        let exe = temp.path().join("app");
        std::fs::create_dir_all(exe.join(".skillshub")).unwrap();
        std::fs::create_dir_all(&home).unwrap();
        assert_eq!(resolve_app_data_dir_from(Some(&exe), &home), home.join(".arsenal"));
        let custom = temp.path().join("custom/.skillshub");
        std::fs::write(exe.join("skillshub-config-path"), custom.to_str().unwrap()).unwrap();
        assert_eq!(resolve_app_data_dir_from(Some(&exe), &home), home.join(".arsenal"));
        std::fs::write(home.join(".skillshub-config-path"), custom.to_str().unwrap()).unwrap();
        assert_eq!(resolve_app_data_dir_from(None, &home), home.join(".arsenal"));
    }

    #[test]
    fn resolve_app_data_dir_uses_home_pointer() {
        let dir = tempfile::tempdir().expect("tempdir");
        let home = dir.path().join("home");
        std::fs::create_dir_all(&home).expect("home");
        std::fs::write(home.join(".arsenal-config-path"), "D:/data/.arsenal").expect("pointer");
        let resolved = resolve_app_data_dir_from(None, &home);
        assert_eq!(resolved, PathBuf::from("D:/data/.arsenal"));
    }

    #[test]
    fn copy_json_store_does_not_copy_writer_lock_or_old_database() {
        let temp = tempfile::tempdir().unwrap();
        let from = temp.path().join("from");
        let to = temp.path().join("to");
        std::fs::create_dir_all(from.join("store/objects")).unwrap();
        std::fs::write(from.join("store/manifest.json"), "manifest fixture").unwrap();
        std::fs::write(from.join("store/objects/fixture.json"), "{}").unwrap();
        std::fs::write(from.join("store/store.lock"), "not copied").unwrap();
        std::fs::write(from.join("db.sqlite"), "old archive").unwrap();
        copy_app_data_dir(&from, &to).unwrap();
        assert_eq!(
            std::fs::read_to_string(to.join("store/manifest.json")).unwrap(),
            "manifest fixture"
        );
        assert_eq!(std::fs::read(to.join("store/store.lock")).unwrap(), b"");
        assert!(!to.join("db.sqlite").exists());
        assert!(copy_app_data_dir(&from, &to).is_err());
    }

    #[test]
    fn copy_app_data_dir_copies_config_files_and_skips_legacy_database_path() {
        let dir = tempfile::tempdir().expect("tempdir");
        let from = dir.path().join("from").join(".arsenal");
        let to = dir.path().join("to").join(".arsenal");
        std::fs::create_dir_all(from.join("library")).expect("from dir");
        std::fs::write(from.join("db.sqlite"), "main-db").expect("db");
        std::fs::write(from.join("database-path"), "stale").expect("legacy");
        std::fs::write(from.join("library").join("skill.md"), "skill").expect("library");
        copy_app_data_dir(&from, &to).expect("copy");
        assert!(!to.join("db.sqlite").exists());
        assert_eq!(
            std::fs::read_to_string(to.join("library").join("skill.md")).unwrap(),
            "skill"
        );
        assert!(!to.join("database-path").exists());
    }

    #[cfg(windows)]
    #[test]
    fn remove_symlink_path_removes_windows_directory_symlink_without_target() {
        let dir = tempfile::tempdir().expect("tempdir");
        let target = dir.path().join("target");
        let link = dir.path().join("link");
        std::fs::create_dir_all(&target).expect("target");
        std::fs::write(target.join("SKILL.md"), "---\nname: linked\n---\n").expect("skill");
        std::os::windows::fs::symlink_dir(&target, &link).expect("symlink");

        remove_symlink_path(&link).expect("remove symlink");

        assert!(std::fs::symlink_metadata(&link).is_err());
        assert!(target.join("SKILL.md").exists());
    }

    #[cfg(windows)]
    #[test]
    fn remove_symlink_path_removes_readonly_windows_directory_symlink() {
        let dir = tempfile::tempdir().expect("tempdir");
        let target = dir.path().join("target");
        let link = dir.path().join("link");
        std::fs::create_dir_all(&target).expect("target");
        std::fs::write(target.join("SKILL.md"), "---\nname: linked\n---\n").expect("skill");
        std::os::windows::fs::symlink_dir(&target, &link).expect("symlink");
        windows_symlink::set_readonly_for_test(&link, true).expect("mark readonly");

        remove_symlink_path(&link).expect("remove readonly symlink");

        assert!(std::fs::symlink_metadata(&link).is_err());
        assert!(
            target.join("SKILL.md").exists(),
            "removing the symlink must not delete the skill files it points to"
        );
    }

    #[test]
    fn prune_empty_parent_dirs_removes_nested_empty_parents() {
        let dir = tempfile::tempdir().expect("tempdir");
        let stop_at = dir.path().join("skills");
        let skill = stop_at.join("author").join("repo").join("skill");
        std::fs::create_dir_all(&skill).expect("nested");
        std::fs::remove_dir(&skill).expect("remove skill");

        prune_empty_parent_dirs(&skill, &stop_at).expect("prune");

        assert!(!stop_at.join("author").exists());
        assert!(stop_at.exists());
    }

    #[test]
    fn prune_empty_parent_dirs_keeps_non_empty_parent() {
        let dir = tempfile::tempdir().expect("tempdir");
        let stop_at = dir.path().join("skills");
        let sibling = stop_at.join("author").join("keep");
        let removed = stop_at.join("author").join("gone");
        std::fs::create_dir_all(&sibling).expect("sibling");
        std::fs::create_dir_all(&removed).expect("removed");
        std::fs::write(sibling.join("SKILL.md"), "---\nname: keep\n---\n").expect("skill");
        std::fs::remove_dir(&removed).expect("remove gone");

        prune_empty_parent_dirs(&removed, &stop_at).expect("prune");

        assert!(sibling.join("SKILL.md").exists());
        assert!(stop_at.join("author").exists());
        assert!(!removed.exists());
    }

    #[test]
    fn remove_path_if_no_skill_markdown_deletes_empty_group() {
        let dir = tempfile::tempdir().expect("tempdir");
        let group = dir.path().join("author");
        std::fs::create_dir_all(group.join("skills")).expect("empty group");

        remove_path_if_no_skill_markdown(&group).expect("remove");

        assert!(!group.exists());
    }

    #[test]
    fn remove_path_if_no_skill_markdown_keeps_live_skill_and_prunes_empties() {
        let dir = tempfile::tempdir().expect("tempdir");
        let group = dir.path().join("author");
        let skill = group.join("skills").join("keep");
        std::fs::create_dir_all(&skill).expect("skill");
        std::fs::write(skill.join("SKILL.md"), "---\nname: keep\n---\n").expect("md");
        std::fs::create_dir_all(group.join("empty-leftover")).expect("empty");

        remove_path_if_no_skill_markdown(&group).expect("prune empties only");

        assert!(skill.join("SKILL.md").exists());
        assert!(!group.join("empty-leftover").exists());
    }
}
