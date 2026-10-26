//! File-backed restore: archive payloads never enter the WebView or a whole-archive buffer.
use super::*;
use std::{collections::BTreeMap, fs::File, io::BufReader, sync::Arc, time::Instant};
use tokio::io::AsyncWriteExt;

pub(super) const MAX_MANIFEST_BYTES: u64 = 128 * 1024 * 1024;
const MAX_EXPANDED_BYTES: u64 = 16 * 1024 * 1024 * 1024;
const MAX_ENTRIES: usize = 100_000;

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupImportProgress {
    phase: &'static str,
    completed_bytes: u64,
    total_bytes: Option<u64>,
}
pub(super) type Progress = Arc<dyn Fn(BackupImportProgress) + Send + Sync>;
fn report(progress: &Progress, phase: &'static str, completed: u64, total: Option<u64>) {
    progress(BackupImportProgress {
        phase,
        completed_bytes: completed,
        total_bytes: total,
    });
}

pub(super) async fn download(
    pool: &AppStore,
    config: WebDavConfig,
    remote_path: &str,
    progress: &Progress,
) -> Result<tempfile::NamedTempFile, String> {
    let url = build_webdav_url(&config, remote_path)?;
    let response = send_webdav_request(
        &webdav_transfer_client()?,
        Method::GET,
        &url,
        &config,
        |builder| builder,
        "WebDAV download",
    )
    .await?;
    if !response.status().is_success() {
        // Keep error responses bounded as well; do not return credentials or server bodies.
        return Err(webdav_status_error(
            "WebDAV download",
            response.status(),
            "",
        ));
    }
    let total = response.content_length();
    if total.is_some_and(|size| size > WEBDAV_MAX_TRANSFER_BYTES as u64) {
        return Err("WebDAV backup exceeds size limit".into());
    }
    let temporary = tempfile::Builder::new()
        .prefix(".backup-download-")
        .tempfile_in(pool.directory())
        .map_err(|e| format!("Failed to create backup download file: {e}"))?;
    let mut output = tokio::fs::File::from_std(temporary.reopen().map_err(|e| e.to_string())?);
    let mut stream = response.bytes_stream();
    let mut received = 0u64;
    let mut last = Instant::now();
    report(progress, "downloading", 0, total);
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|error| {
            if error.is_timeout() {
                "WebDAV download failed: request timed out"
            } else {
                "WebDAV backup download interrupted; no restore was started"
            }
        })?;
        received = received
            .checked_add(chunk.len() as u64)
            .ok_or("WebDAV backup exceeds size limit")?;
        if received > WEBDAV_MAX_TRANSFER_BYTES as u64 {
            return Err("WebDAV backup exceeds size limit".into());
        }
        output
            .write_all(&chunk)
            .await
            .map_err(|e| format!("Failed to write backup download file: {e}"))?;
        if last.elapsed() >= Duration::from_millis(250) {
            report(progress, "downloading", received, total);
            last = Instant::now();
        }
    }
    output
        .flush()
        .await
        .map_err(|e| format!("Failed to finish backup download: {e}"))?;
    if total.is_some_and(|expected| expected != received) {
        return Err("WebDAV backup download incomplete; no restore was started".into());
    }
    report(progress, "downloading", received, total);
    Ok(temporary)
}

pub(super) async fn restore_path(
    pool: &AppStore,
    path: PathBuf,
    repository_only: bool,
    progress: Progress,
) -> Result<(), String> {
    let pool = pool.clone();
    let runtime = tokio::runtime::Handle::current();
    // ZIP/JSON parsing, CRC checks and filesystem restores must not monopolize the async executor.
    tokio::task::spawn_blocking(move || {
        report(&progress, "validating", 0, None);
        let staging = tempfile::Builder::new()
            .prefix(".backup-restore-")
            .tempdir_in(pool.directory())
            .map_err(|e| format!("Failed to create backup staging directory: {e}"))?;
        let mut backup = prepare(&path, staging.path(), &progress)?;
        if repository_only {
            restrict_to_repository(&mut backup);
        }
        // Validate every skill before the existing restore implementation writes any metadata.
        validate_restore_plan(&backup.skills)?;
        report(&progress, "restoring", 0, None);
        runtime.block_on(import_app_backup_data_impl(&pool, backup))?;
        report(&progress, "completed", 0, None);
        Ok(())
    })
    .await
    .map_err(|_| {
        "Backup restore worker failed; check available disk space and memory".to_string()
    })?
}

pub(super) fn prepare(
    path: &Path,
    staging: &Path,
    progress: &Progress,
) -> Result<AppBackup, String> {
    let mut file = File::open(path).map_err(|e| format!("Failed to open backup file: {e}"))?;
    let length = file.metadata().map_err(|e| e.to_string())?.len();
    if length > WEBDAV_MAX_TRANSFER_BYTES as u64 {
        return Err("Backup exceeds size limit (1 GiB)".into());
    }
    let mut prefix = [0u8; 4];
    let count = file.read(&mut prefix).map_err(|e| e.to_string())?;
    std::io::Seek::rewind(&mut file).map_err(|e| e.to_string())?;
    if count < 4 || &prefix[..2] != b"PK" {
        if length > MAX_MANIFEST_BYTES {
            return Err("JSON backup exceeds size limit (128 MiB); use a ZIP backup".into());
        }
        return serde_json::from_reader(BufReader::new(file))
            .map_err(|_| "Backup is not a valid ZIP or JSON document".into());
    }
    let mut archive =
        zip::ZipArchive::new(file).map_err(|_| "Backup archive is not a valid ZIP file")?;
    if archive.len() > MAX_ENTRIES {
        return Err("Backup archive exceeds file count limit".into());
    }
    let mut manifest = None;
    let mut files = BTreeMap::new();
    let mut names = HashSet::new();
    let mut expanded = 0u64;
    let mut last = Instant::now();
    for index in 0..archive.len() {
        let entry = archive
            .by_index(index)
            .map_err(|_| "Failed to read backup archive entry")?;
        let name = entry.name().replace('\\', "/");
        let relative = normalize_relative_path(name.trim_end_matches('/'))?;
        let name = path_to_portable_relative(&relative).ok_or("Invalid backup entry path")?;
        if !names.insert(name.to_lowercase()) {
            return Err("Backup contains duplicate archive paths".into());
        }
        if entry
            .unix_mode()
            .is_some_and(|mode| mode & 0o170000 == 0o120000)
        {
            return Err("Backup contains unsupported symbolic links".into());
        }
        if entry.is_dir() {
            continue;
        }
        expanded = expanded
            .checked_add(entry.size())
            .ok_or("Backup expanded size exceeds limit")?;
        if expanded > MAX_EXPANDED_BYTES {
            return Err("Backup expanded size exceeds limit (16 GiB)".into());
        }
        if name == "manifest.json" {
            if entry.size() > MAX_MANIFEST_BYTES {
                return Err("Backup manifest exceeds size limit (128 MiB)".into());
            }
            let mut bytes = Vec::new();
            entry
                .take(MAX_MANIFEST_BYTES + 1)
                .read_to_end(&mut bytes)
                .map_err(|_| "Backup manifest is corrupt or incomplete")?;
            if bytes.len() as u64 > MAX_MANIFEST_BYTES {
                return Err("Backup manifest exceeds size limit".into());
            }
            manifest = Some(
                serde_json::from_slice::<AppBackup>(&bytes)
                    .map_err(|_| "Backup manifest is not valid JSON")?,
            );
        } else {
            // Flat private names avoid extracting any archive-controlled filesystem path.
            let target = staging.join(index.to_string());
            let mut output =
                File::create(&target).map_err(|e| format!("Failed to stage backup file: {e}"))?;
            let expected = entry.size();
            let copied =
                std::io::copy(&mut entry.take(expected + 1), &mut output).map_err(|error| {
                    if error.raw_os_error().is_some() {
                        format!("Failed to stage backup file: {error}")
                    } else {
                        "Backup archive entry is corrupt or incomplete".into()
                    }
                })?;
            if copied != expected {
                return Err("Backup archive entry has an invalid size".into());
            }
            files.insert(name, target);
        }
        if last.elapsed() >= Duration::from_millis(250) {
            report(
                progress,
                "validating",
                index as u64 + 1,
                Some(archive.len() as u64),
            );
            last = Instant::now();
        }
    }
    let mut backup = manifest.ok_or("Backup archive is missing manifest.json")?;
    for skill in &mut backup.skills {
        let root = if skill.storage_kind == "resource" {
            "resource-library"
        } else {
            "central-library"
        };
        let relative = path_to_portable_relative(&normalize_relative_path(&skill.relative_dir)?)
            .ok_or("Backup contains an empty relative path")?;
        let prefix = format!("{root}/{relative}/");
        skill.files = files
            .range(prefix.clone()..)
            .take_while(|(name, _)| name.starts_with(&prefix))
            .map(|(name, path)| SkillFileBackup {
                relative_path: name[prefix.len()..].to_owned(),
                content_base64: String::new(),
                staged_path: Some(path.clone()),
            })
            .collect();
    }
    Ok(backup)
}
