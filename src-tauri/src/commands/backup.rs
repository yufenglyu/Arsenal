use crate::db::backup::{CollectionSkillBackup, RepositoryDirectoryBackup, RepositoryStatsBackup};
use base64::{engine::general_purpose::STANDARD, Engine as _};
use chrono::{DateTime, Local, NaiveDateTime, TimeZone, Utc};
use futures_util::StreamExt;
use percent_encoding::percent_decode_str;
use quick_xml::events::Event;
use reqwest::{Client, Method, Url};
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::collections::HashSet;
use std::io::{Cursor, Read, Write};
use std::path::{Component, Path, PathBuf};
use std::time::Duration;
use tauri::State;
use uuid::Uuid;

use crate::{
    commands::linker::{
        create_symlink, install_skill_to_agent_copy_impl, install_skill_to_agent_impl,
        symlink_target_path,
    },
    commands::skills::SkillWithLinks,
    db::{self, Agent, Collection, DbPool, ScanDirectory, Skill, SkillMetadata, SkillSource},
    path_utils::{path_to_string, remove_symlink_path},
    AppState,
};

const BACKUP_SCHEMA_VERSION: u32 = 1;
const WEBDAV_REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
const WEBDAV_TRANSFER_TIMEOUT: Duration = Duration::from_secs(900);
const WEBDAV_MAX_TRANSFER_BYTES: usize = 1024 * 1024 * 1024;
const WEBDAV_MAX_LIST_BYTES: usize = 8 * 1024 * 1024;
const WEBDAV_MAX_REDIRECTS: usize = 5;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct BackupOptions {
    pub include_resource_library: bool,
    pub include_central_library: bool,
    pub include_app_config: bool,
    pub include_installations: bool,
}

impl Default for BackupOptions {
    fn default() -> Self {
        Self {
            include_resource_library: true,
            include_central_library: false,
            include_app_config: true,
            include_installations: true,
        }
    }
}

fn complete_backup_options(_requested: Option<BackupOptions>) -> BackupOptions {
    BackupOptions {
        include_resource_library: true,
        include_central_library: false,
        include_app_config: true,
        include_installations: true,
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WebDavConfig {
    pub base_url: String,
    pub username: Option<String>,
    pub password: Option<String>,
    pub remote_dir: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct WebDavBackupFile {
    pub name: String,
    pub remote_path: String,
    pub size: Option<u64>,
    pub modified_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SettingBackup {
    key: String,
    value: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SkillInstallationBackup {
    skill_id: String,
    agent_id: String,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        alias = "link_type",
        alias = "linkType"
    )]
    method: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SkillFileBackup {
    relative_path: String,
    content_base64: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SkillBackup {
    skill: Skill,
    source: Option<SkillSource>,
    metadata: Option<SkillMetadata>,
    #[serde(default = "default_skill_backup_storage_kind")]
    storage_kind: String,
    relative_dir: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    files: Vec<SkillFileBackup>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct AgentBackup {
    id: String,
    display_name: String,
    is_detected: bool,
    is_builtin: bool,
    is_enabled: bool,
}

impl From<&Agent> for AgentBackup {
    fn from(agent: &Agent) -> Self {
        Self {
            id: agent.id.clone(),
            display_name: agent.display_name.clone(),
            is_detected: agent.is_detected,
            is_builtin: agent.is_builtin,
            is_enabled: agent.is_enabled,
        }
    }
}

#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize)]
struct ScanDirectoryBackup {
    id: i64,
    label: Option<String>,
    is_active: bool,
    is_builtin: bool,
    added_at: String,
}

impl From<&ScanDirectory> for ScanDirectoryBackup {
    fn from(directory: &ScanDirectory) -> Self {
        Self {
            id: directory.id,
            label: directory.label.clone(),
            is_active: directory.is_active,
            is_builtin: directory.is_builtin,
            added_at: directory.added_at.clone(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct AppBackup {
    #[serde(default)]
    repository_directories: Vec<RepositoryDirectoryBackup>,
    #[serde(default)]
    repository_stats: Vec<RepositoryStatsBackup>,
    #[serde(default)]
    folder_notes: Vec<super::metadata::FolderNote>,
    #[serde(default)]
    repository_notebooks: Vec<super::notebook_repository::Repository>,
    schema_version: u32,
    exported_at: String,
    #[serde(default)]
    included: BackupOptions,
    skills: Vec<SkillBackup>,
    collections: Vec<Collection>,
    collection_skills: Vec<CollectionSkillBackup>,
    settings: Vec<SettingBackup>,
    agents: Vec<AgentBackup>,
    scan_directories: Vec<ScanDirectoryBackup>,
    #[serde(default)]
    skill_installations: Vec<SkillInstallationBackup>,
}

#[tauri::command]
pub async fn export_app_backup(
    state: State<'_, AppState>,
    options: Option<BackupOptions>,
) -> Result<Vec<u8>, String> {
    let _guard = super::repository_mirror::MIRROR_LOCK.lock().await;
    super::repository_address::recover(&state.db).await?;
    export_app_backup_archive_impl(&state.db, complete_backup_options(options)).await
}

#[tauri::command]
pub async fn export_app_backup_to_path(
    state: State<'_, AppState>,
    dest_path: String,
    options: Option<BackupOptions>,
) -> Result<(), String> {
    let _guard = super::repository_mirror::MIRROR_LOCK.lock().await;
    super::repository_address::recover(&state.db).await?;
    let archive =
        export_app_backup_archive_impl(&state.db, complete_backup_options(options)).await?;
    write_backup_archive_to_path(&dest_path, &archive)
}

#[tauri::command]
pub async fn import_app_backup(
    state: State<'_, AppState>,
    backup: Vec<u8>,
    repository_only: Option<bool>,
) -> Result<(), String> {
    let _guard = super::repository_mirror::MIRROR_LOCK.lock().await;
    super::repository_address::recover(&state.db).await?;
    let mut data = read_app_backup_bytes(&backup)?;
    if repository_only.unwrap_or(false) {
        restrict_to_repository(&mut data);
    }
    import_app_backup_data_impl(&state.db, data).await
}

fn repository_backup_options() -> BackupOptions {
    BackupOptions {
        include_resource_library: true,
        include_central_library: false,
        include_app_config: false,
        include_installations: false,
    }
}

fn restrict_to_repository(backup: &mut AppBackup) {
    backup.included = repository_backup_options();
    backup
        .skills
        .retain(|skill| skill.storage_kind == "resource");
    let skill_ids: HashSet<_> = backup
        .skills
        .iter()
        .map(|skill| skill.skill.id.as_str())
        .collect();
    let collection_ids: HashSet<_> = backup
        .collections
        .iter()
        .map(|collection| collection.id.as_str())
        .collect();
    backup.collection_skills.retain(|member| {
        skill_ids.contains(member.skill_id.as_str())
            && collection_ids.contains(member.collection_id.as_str())
    });
    backup.settings.clear();
    backup.agents.clear();
    backup.scan_directories.clear();
    backup.skill_installations.clear();
}

#[tauri::command]
pub async fn list_webdav_backups(config: WebDavConfig) -> Result<Vec<WebDavBackupFile>, String> {
    list_webdav_backups_impl(config).await
}

#[tauri::command]
pub async fn test_webdav_connection(config: WebDavConfig) -> Result<(), String> {
    test_webdav_connection_impl(config).await
}

#[tauri::command]
pub async fn upload_webdav_backup(
    state: State<'_, AppState>,
    config: WebDavConfig,
    options: Option<BackupOptions>,
) -> Result<WebDavBackupFile, String> {
    let _guard = super::repository_mirror::MIRROR_LOCK.lock().await;
    super::repository_address::recover(&state.db).await?;
    let _ = options;
    let archive = export_app_backup_archive_impl(&state.db, repository_backup_options())
        .await
        .map_err(|error| format!("WebDAV backup export failed: {error}"))?;
    upload_webdav_backup_impl(config, archive).await
}

#[tauri::command]
pub async fn download_webdav_backup(
    config: WebDavConfig,
    remote_path: String,
) -> Result<Vec<u8>, String> {
    download_webdav_backup_impl(config, &remote_path).await
}

#[tauri::command]
pub async fn delete_webdav_backup(config: WebDavConfig, remote_path: String) -> Result<(), String> {
    delete_webdav_backup_impl(config, &remote_path).await
}

fn normalize_webdav_base_url(value: &str) -> Result<String, String> {
    let trimmed = value.trim().trim_end_matches('/');
    if trimmed.is_empty() {
        return Err("WebDAV URL cannot be empty".to_string());
    }
    let url = Url::parse(trimmed).map_err(|_| "WebDAV URL is invalid".to_string())?;
    if !matches!(url.scheme(), "http" | "https") {
        return Err("WebDAV URL must use http or https".to_string());
    }
    let authority = trimmed
        .split_once("://")
        .map(|(_, rest)| rest.split('/').next().unwrap_or(rest))
        .unwrap_or_default();
    if !url.username().is_empty()
        || url.password().is_some()
        || authority.contains('@')
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err("WebDAV URL must not include query, fragment, or userinfo".to_string());
    }
    Ok(trimmed.to_string())
}

fn normalize_webdav_remote_path(value: &str) -> Result<String, String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err("WebDAV remote path cannot be empty".to_string());
    }
    if trimmed.starts_with('/') || trimmed.contains('\\') {
        return Err("WebDAV remote path must be relative".to_string());
    }
    if trimmed.contains('?') || trimmed.contains('#') {
        return Err("WebDAV remote path must not contain query or fragment separators".to_string());
    }
    let bytes = trimmed.as_bytes();
    if bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':' {
        return Err("WebDAV remote path must be relative".to_string());
    }
    let parts: Vec<&str> = trimmed.split('/').collect();
    if parts
        .iter()
        .any(|part| part.is_empty() || *part == "." || *part == "..")
    {
        return Err("WebDAV remote path contains unsafe traversal".to_string());
    }
    Ok(parts.join("/"))
}

fn build_webdav_url(config: &WebDavConfig, remote_path: &str) -> Result<String, String> {
    let base = normalize_webdav_base_url(&config.base_url)?;
    let remote_dir = normalize_webdav_remote_path(&config.remote_dir)?;
    let remote_path = normalize_webdav_remote_path(remote_path)?;
    let mut url = Url::parse(&base).map_err(|_| "WebDAV URL is invalid".to_string())?;
    append_webdav_path_segments(&mut url, &remote_dir)?;
    append_webdav_path_segments(&mut url, &remote_path)?;
    Ok(url.to_string())
}

fn build_webdav_directory_url(config: &WebDavConfig) -> Result<String, String> {
    let base = normalize_webdav_base_url(&config.base_url)?;
    let remote_dir = normalize_webdav_remote_path(&config.remote_dir)?;
    let mut url = Url::parse(&base).map_err(|_| "WebDAV URL is invalid".to_string())?;
    append_webdav_path_segments(&mut url, &remote_dir)?;
    ensure_trailing_slash(&mut url);
    Ok(url.to_string())
}

fn append_webdav_path_segments(url: &mut Url, path: &str) -> Result<(), String> {
    let mut segments = url
        .path_segments_mut()
        .map_err(|_| "WebDAV URL cannot accept path segments".to_string())?;
    for segment in path.split('/') {
        segments.push(segment);
    }
    Ok(())
}

fn ensure_trailing_slash(url: &mut Url) {
    let path = url.path().to_string();
    if !path.ends_with('/') {
        url.set_path(&format!("{path}/"));
    }
}

fn apply_webdav_auth(
    builder: reqwest::RequestBuilder,
    config: &WebDavConfig,
) -> reqwest::RequestBuilder {
    match (config.username.as_deref(), config.password.as_deref()) {
        (Some(username), Some(password)) if !username.is_empty() || !password.is_empty() => {
            builder.basic_auth(username.to_string(), Some(password.to_string()))
        }
        (Some(username), None) if !username.is_empty() => {
            builder.basic_auth(username, None::<String>)
        }
        (None, Some(password)) if !password.is_empty() => {
            builder.basic_auth("", Some(password.to_string()))
        }
        _ => builder,
    }
}

fn build_webdav_collection_url(
    config: &WebDavConfig,
    relative_dir: &str,
) -> Result<String, String> {
    let base = normalize_webdav_base_url(&config.base_url)?;
    let relative_dir = normalize_webdav_remote_path(relative_dir)?;
    let mut url = Url::parse(&base).map_err(|_| "WebDAV URL is invalid".to_string())?;
    append_webdav_path_segments(&mut url, &relative_dir)?;
    ensure_trailing_slash(&mut url);
    Ok(url.to_string())
}

fn webdav_status_error(operation: &str, status: reqwest::StatusCode, body: &str) -> String {
    let snippet = body
        .chars()
        .filter(|ch| !ch.is_control() || *ch == '\n' || *ch == '\t')
        .take(180)
        .collect::<String>();
    if snippet.trim().is_empty() {
        format!("{operation} failed with status {status}")
    } else {
        format!(
            "{operation} failed with status {status}: {}",
            snippet.trim()
        )
    }
}

async fn send_webdav_request(
    client: &Client,
    method: Method,
    url: &str,
    config: &WebDavConfig,
    configure: impl Fn(reqwest::RequestBuilder) -> reqwest::RequestBuilder,
    operation: &str,
) -> Result<reqwest::Response, String> {
    let mut current_url = url.to_string();
    for _ in 0..WEBDAV_MAX_REDIRECTS {
        let response = apply_webdav_auth(
            configure(client.request(method.clone(), &current_url)),
            config,
        )
        .send()
        .await
        .map_err(|e| format!("{operation} failed: {}", sanitize_webdav_error(e)))?;
        let status = response.status();
        if status.is_redirection() {
            let location = response
                .headers()
                .get(reqwest::header::LOCATION)
                .and_then(|value| value.to_str().ok())
                .map(str::to_string);
            drop(response);
            let Some(location) = location else {
                return Err(format!("{operation} failed with status {status}"));
            };
            current_url = Url::parse(&current_url)
                .ok()
                .and_then(|base| base.join(&location).ok())
                .map(|joined| joined.to_string())
                .unwrap_or(location);
            continue;
        }
        return Ok(response);
    }
    Err(format!("{operation} failed: too many redirects"))
}

async fn ensure_webdav_remote_dir(client: &Client, config: &WebDavConfig) -> Result<(), String> {
    let remote_dir = normalize_webdav_remote_path(&config.remote_dir)?;
    let mut prefix = String::new();
    for segment in remote_dir.split('/') {
        if !prefix.is_empty() {
            prefix.push('/');
        }
        prefix.push_str(segment);
        mkcol_webdav_collection(client, config, &prefix).await?;
    }
    Ok(())
}

async fn mkcol_webdav_collection(
    client: &Client,
    config: &WebDavConfig,
    relative_dir: &str,
) -> Result<(), String> {
    let url = build_webdav_collection_url(config, relative_dir)?;
    let method = Method::from_bytes(b"MKCOL").map_err(|e| e.to_string())?;
    let response = send_webdav_request(
        client,
        method,
        &url,
        config,
        |builder| builder,
        "WebDAV upload",
    )
    .await?;
    let status = response.status();
    if status.is_success() || matches!(status.as_u16(), 405 | 409) {
        return Ok(());
    }
    if matches!(status.as_u16(), 401 | 403) {
        let body = response.text().await.unwrap_or_default();
        return Err(webdav_status_error("WebDAV upload", status, &body));
    }
    Ok(())
}

fn webdav_success_or_error(
    operation: &str,
    status: reqwest::StatusCode,
    body: &str,
) -> Result<(), String> {
    if status.is_success() {
        Ok(())
    } else {
        Err(webdav_status_error(operation, status, body))
    }
}

fn ensure_webdav_archive_fits(len: usize) -> Result<(), String> {
    if len > WEBDAV_MAX_TRANSFER_BYTES {
        return Err("WebDAV backup exceeds size limit".to_string());
    }
    Ok(())
}

async fn upload_webdav_backup_impl(
    config: WebDavConfig,
    archive: Vec<u8>,
) -> Result<WebDavBackupFile, String> {
    ensure_webdav_archive_fits(archive.len())?;
    let filename = generated_backup_filename();
    let url = build_webdav_url(&config, &filename)?;
    let client = webdav_transfer_client()?;
    ensure_webdav_remote_dir(&client, &config).await?;
    let response = send_webdav_request(
        &client,
        Method::PUT,
        &url,
        &config,
        |builder| {
            builder
                .header(reqwest::header::CONTENT_TYPE, "application/zip")
                .header(reqwest::header::IF_NONE_MATCH, "*")
                .body(archive.clone())
        },
        "WebDAV upload",
    )
    .await?;
    let status = response.status();
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        return Err(webdav_status_error("WebDAV upload", status, &body));
    }
    Ok(WebDavBackupFile {
        name: filename.clone(),
        remote_path: filename,
        size: Some(archive.len() as u64),
        modified_at: Some(Utc::now().to_rfc3339()),
    })
}

async fn download_webdav_backup_impl(
    config: WebDavConfig,
    remote_path: &str,
) -> Result<Vec<u8>, String> {
    let url = build_webdav_url(&config, remote_path)?;
    // Downloading a repository archive needs the same total budget as uploading it.
    // The ordinary 30-second budget is only for small WebDAV control requests.
    let client = webdav_transfer_client()?;
    let response = send_webdav_request(
        &client,
        Method::GET,
        &url,
        &config,
        |builder| builder,
        "WebDAV download",
    )
    .await?;
    let status = response.status();
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        return Err(webdav_status_error("WebDAV download", status, &body));
    }
    read_webdav_bytes(
        response,
        WEBDAV_MAX_TRANSFER_BYTES,
        "WebDAV download failed",
    )
    .await
}

async fn delete_webdav_backup_impl(config: WebDavConfig, remote_path: &str) -> Result<(), String> {
    let url = build_webdav_url(&config, remote_path)?;
    let client = webdav_client()?;
    let response = send_webdav_request(
        &client,
        Method::DELETE,
        &url,
        &config,
        |builder| builder,
        "WebDAV delete",
    )
    .await?;
    let status = response.status();
    let body = response.text().await.unwrap_or_default();
    webdav_success_or_error("WebDAV delete", status, &body)
}

async fn list_webdav_backups_impl(config: WebDavConfig) -> Result<Vec<WebDavBackupFile>, String> {
    let url = build_webdav_directory_url(&config)?;
    let method = Method::from_bytes(b"PROPFIND").map_err(|e| e.to_string())?;
    let client = webdav_client()?;
    let response = send_webdav_request(
        &client,
        method,
        &url,
        &config,
        |builder| builder.header("Depth", "1"),
        "WebDAV list",
    )
    .await?;
    let status = response.status();
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        return Err(webdav_status_error("WebDAV list", status, &body));
    }
    let body = read_webdav_body(response, WEBDAV_MAX_LIST_BYTES, "WebDAV list failed").await?;
    parse_webdav_backup_files(&body)
}

async fn test_webdav_connection_impl(config: WebDavConfig) -> Result<(), String> {
    let url = build_webdav_directory_url(&config)?;
    let method = Method::from_bytes(b"PROPFIND").map_err(|e| e.to_string())?;
    let client = webdav_client()?;
    let response = send_webdav_request(
        &client,
        method,
        &url,
        &config,
        |builder| builder.header("Depth", "0"),
        "WebDAV test",
    )
    .await?;
    let status = response.status();
    if status.as_u16() == 404 {
        let upload_client = webdav_transfer_client()?;
        return ensure_webdav_remote_dir(&upload_client, &config).await;
    }
    let body = response.text().await.unwrap_or_default();
    webdav_success_or_error("WebDAV test", status, &body)
}

fn webdav_client_with_timeout(timeout: Duration) -> Result<Client, String> {
    Client::builder()
        .timeout(timeout)
        .connect_timeout(Duration::from_secs(20))
        .http1_only()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|e| {
            format!(
                "Failed to create WebDAV client: {}",
                sanitize_webdav_error(e)
            )
        })
}

fn webdav_client() -> Result<Client, String> {
    webdav_client_with_timeout(WEBDAV_REQUEST_TIMEOUT)
}

fn webdav_transfer_client() -> Result<Client, String> {
    webdav_client_with_timeout(WEBDAV_TRANSFER_TIMEOUT)
}

async fn read_webdav_body(
    response: reqwest::Response,
    max_bytes: usize,
    operation: &str,
) -> Result<String, String> {
    let body = read_webdav_bytes(response, max_bytes, operation).await?;
    String::from_utf8(body).map_err(|_| format!("{} response is not valid UTF-8", operation))
}

async fn read_webdav_bytes(
    response: reqwest::Response,
    max_bytes: usize,
    operation: &str,
) -> Result<Vec<u8>, String> {
    if response
        .content_length()
        .is_some_and(|length| length > max_bytes as u64)
    {
        return Err(format!("{} response exceeds size limit", operation));
    }

    let mut body = Vec::new();
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| format!("{}: {}", operation, sanitize_webdav_error(e)))?;
        if chunk.len() > max_bytes.saturating_sub(body.len()) {
            return Err(format!("{} response exceeds size limit", operation));
        }
        body.extend_from_slice(&chunk);
    }
    Ok(body)
}

#[derive(Default)]
struct WebDavResponseEntry {
    href: Option<String>,
    content_length: Option<u64>,
    last_modified: Option<String>,
}

fn parse_webdav_backup_files(xml: &str) -> Result<Vec<WebDavBackupFile>, String> {
    let mut reader = quick_xml::Reader::from_str(xml);
    reader.config_mut().trim_text(true);
    let mut buf = Vec::new();
    let mut files = Vec::new();
    let mut current = WebDavResponseEntry::default();
    let mut in_response = false;
    let mut active_tag: Option<String> = None;

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(event)) => {
                let name = String::from_utf8_lossy(event.name().as_ref()).to_string();
                if name.ends_with("response") {
                    in_response = true;
                    current = WebDavResponseEntry::default();
                } else if in_response {
                    active_tag = Some(name);
                }
            }
            Ok(Event::Text(text)) if in_response => {
                let value = text
                    .unescape()
                    .map_err(|e| format!("Invalid WebDAV XML: {}", e))?
                    .to_string();
                match active_tag.as_deref() {
                    Some(tag) if tag.ends_with("href") => current.href = Some(value),
                    Some(tag) if tag.ends_with("getcontentlength") => {
                        current.content_length = value.parse::<u64>().ok();
                    }
                    Some(tag) if tag.ends_with("getlastmodified") => {
                        current.last_modified = Some(value);
                    }
                    _ => {}
                }
            }
            Ok(Event::End(event)) => {
                let name = String::from_utf8_lossy(event.name().as_ref()).to_string();
                if name.ends_with("response") {
                    if let Some(file) = webdav_entry_to_backup_file(&current)? {
                        files.push(file);
                    }
                    in_response = false;
                    active_tag = None;
                } else if in_response {
                    active_tag = None;
                }
            }
            Ok(Event::Eof) => break,
            Err(error) => return Err(format!("Invalid WebDAV XML: {}", error)),
            _ => {}
        }
        buf.clear();
    }

    files.sort_by(|a, b| {
        match (webdav_modified_timestamp(a), webdav_modified_timestamp(b)) {
            (Some(a_date), Some(b_date)) => b_date.cmp(&a_date),
            (Some(_), None) => Ordering::Less,
            (None, Some(_)) => Ordering::Greater,
            (None, None) => Ordering::Equal,
        }
        .then_with(|| a.name.cmp(&b.name))
        .then_with(|| a.remote_path.cmp(&b.remote_path))
    });
    Ok(files)
}

fn webdav_modified_timestamp(file: &WebDavBackupFile) -> Option<DateTime<Utc>> {
    let http_timestamp = parse_webdav_modified_at(file.modified_at.as_deref());
    let filename_timestamp = generated_backup_timestamp_utc(&file.name);
    http_timestamp.into_iter().chain(filename_timestamp).max()
}

fn parse_webdav_modified_at(value: Option<&str>) -> Option<DateTime<Utc>> {
    let value = value?.trim();
    DateTime::parse_from_rfc2822(value)
        .map(|date| date.with_timezone(&Utc))
        .ok()
        .or_else(|| {
            DateTime::parse_from_rfc3339(value)
                .map(|date| date.with_timezone(&Utc))
                .ok()
        })
        .or_else(|| {
            httpdate::parse_http_date(value)
                .ok()
                .map(DateTime::<Utc>::from)
        })
}

fn generated_backup_timestamp(name: &str) -> Option<NaiveDateTime> {
    let timestamp = name.strip_prefix("skillshub-backup-")?;
    let timestamp = timestamp
        .strip_suffix(".zip")
        .or_else(|| timestamp.strip_suffix(".json"))?;
    NaiveDateTime::parse_from_str(timestamp, "%Y%m%d%H%M%S")
        .ok()
        .or_else(|| NaiveDateTime::parse_from_str(timestamp.get(..17)?, "%Y-%m-%d-%H%M%S").ok())
}

fn generated_backup_timestamp_utc(name: &str) -> Option<DateTime<Utc>> {
    let naive = generated_backup_timestamp(name)?;
    Local
        .from_local_datetime(&naive)
        .earliest()
        .or_else(|| Local.from_local_datetime(&naive).latest())
        .map(|dt| dt.with_timezone(&Utc))
}

fn listed_webdav_modified_at(value: Option<&str>) -> Option<String> {
    match parse_webdav_modified_at(value) {
        Some(dt) => Some(dt.to_rfc3339()),
        None => value
            .map(str::trim)
            .filter(|text| !text.is_empty())
            .map(str::to_string),
    }
}

fn generated_backup_filename() -> String {
    format!(
        "skillshub-backup-{}.zip",
        Local::now().format("%Y%m%d%H%M%S")
    )
}

fn write_backup_archive_to_path(dest_path: &str, archive: &[u8]) -> Result<(), String> {
    let trimmed = dest_path.trim();
    if trimmed.is_empty() {
        return Err("Backup destination path is empty".to_string());
    }
    let path = PathBuf::from(trimmed);
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent).map_err(|e| {
                format!(
                    "Failed to create backup directory '{}': {}",
                    parent.display(),
                    e
                )
            })?;
        }
    }
    std::fs::write(&path, archive)
        .map_err(|e| format!("Failed to write backup file '{}': {}", path.display(), e))
}

fn webdav_entry_to_backup_file(
    entry: &WebDavResponseEntry,
) -> Result<Option<WebDavBackupFile>, String> {
    let Some(href) = entry.href.as_deref() else {
        return Ok(None);
    };
    let href = href.trim().trim_end_matches('/');
    let encoded_name = href.rsplit('/').next().unwrap_or(href);
    let name = percent_decode_str(encoded_name)
        .decode_utf8()
        .map_err(|_| "WebDAV href contains invalid percent encoding".to_string())?
        .into_owned();
    if name.is_empty() {
        return Ok(None);
    }
    if !name.ends_with(".zip") && !name.ends_with(".json") {
        return Ok(None);
    }
    if name == "." || name == ".." || name.contains('/') || name.contains('\\') {
        return Err("WebDAV href contains unsafe filename".to_string());
    }
    Ok(Some(WebDavBackupFile {
        name: name.clone(),
        remote_path: normalize_webdav_remote_path(&name)?,
        size: entry.content_length,
        modified_at: listed_webdav_modified_at(entry.last_modified.as_deref()),
    }))
}

fn sanitize_webdav_error(error: reqwest::Error) -> String {
    if error.is_timeout() {
        "request timed out".to_string()
    } else if error.is_connect() {
        "connection failed".to_string()
    } else {
        error.without_url().to_string()
    }
}

async fn export_app_backup_data_impl(
    pool: &DbPool,
    options: BackupOptions,
) -> Result<AppBackup, String> {
    let central_root = central_root(pool).await?;
    let resource_root = db::get_skill_resource_library_dir(pool).await?;
    let mut skill_backups = Vec::new();
    if options.include_central_library {
        let central_skills = db::get_central_skills(pool).await?;
        append_skill_backups(
            pool,
            &mut skill_backups,
            central_skills,
            &central_root,
            "central",
        )
        .await?;
    }
    if options.include_resource_library {
        let resource_skills = crate::commands::skills::get_resource_library_skills_impl(pool)
            .await?
            .into_iter()
            .map(skill_from_resource_listing)
            .collect();
        append_skill_backups(
            pool,
            &mut skill_backups,
            resource_skills,
            &resource_root,
            "resource",
        )
        .await?;
    }
    fill_central_symlink_files_without_resource(&mut skill_backups, &central_root)?;
    let (collections, collection_skills, settings, agents, scan_directories) =
        if options.include_app_config {
            (
                db::get_all_collections(pool).await?,
                db::backup::memberships(pool)?,
                crate::db::load_config(pool)?
                    .settings
                    .into_iter()
                    .map(|(key, value)| SettingBackup { key, value })
                    .filter(|setting| is_exportable_setting_key(&setting.key))
                    .collect(),
                db::get_all_agents(pool)
                    .await?
                    .iter()
                    .map(AgentBackup::from)
                    .collect(),
                db::get_scan_directories(pool)
                    .await?
                    .iter()
                    .map(ScanDirectoryBackup::from)
                    .collect(),
            )
        } else {
            (
                db::get_all_collections(pool).await?,
                db::backup::memberships(pool)?,
                Vec::new(),
                Vec::new(),
                Vec::new(),
            )
        };

    let current_agents = db::get_all_agents(pool).await?;
    let included_skill_ids: HashSet<String> = skill_backups
        .iter()
        .map(|skill| skill.skill.id.clone())
        .collect();
    let restorable_agent_ids: HashSet<String> = current_agents
        .iter()
        .filter(|agent| options.include_app_config || agent.is_builtin)
        .map(|agent| agent.id.clone())
        .collect();
    let skill_installations = if options.include_installations {
        db::backup::installations(pool)?
            .into_iter()
            .filter(|installation| {
                included_skill_ids.contains(&installation.skill_id)
                    && restorable_agent_ids.contains(&installation.agent_id)
            })
            .map(|installation| SkillInstallationBackup {
                skill_id: installation.skill_id,
                agent_id: installation.agent_id,
                method: Some(
                    validated_install_method(Some(&installation.link_type))
                        .as_str()
                        .to_string(),
                ),
            })
            .collect()
    } else {
        Vec::new()
    };

    let repositories: HashSet<String> = skill_backups
        .iter()
        .filter_map(|skill| skill.source.as_ref()?.source_repo.as_deref())
        .filter(|repo| is_portable_github_repo(repo))
        .map(str::to_ascii_lowercase)
        .collect();
    let repository_stats = db::backup::stats(pool)?
        .into_iter()
        .filter(|stats| repositories.contains(&stats.repository.to_ascii_lowercase()))
        .collect();
    let repository_directories = if options.include_resource_library {
        db::backup::directories(pool)?
    } else {
        Vec::new()
    };
    let mut backup = AppBackup {
        repository_directories,
        repository_stats,
        repository_notebooks: if options.include_resource_library {
            super::notebook_repository::list(pool).await?
        } else {
            Vec::new()
        },
        folder_notes: if options.include_resource_library {
            super::metadata::folder_notes(pool).await?
        } else {
            Vec::new()
        },
        schema_version: BACKUP_SCHEMA_VERSION,
        exported_at: Utc::now().to_rfc3339(),
        included: options,
        skills: skill_backups,
        collections,
        collection_skills,
        settings,
        agents,
        scan_directories,
        skill_installations,
    };

    if !options.include_app_config
        && !options.include_installations
        && !options.include_central_library
    {
        restrict_to_repository(&mut backup);
    }
    Ok(backup)
}

pub async fn export_app_backup_impl(
    pool: &DbPool,
    options: BackupOptions,
) -> Result<String, String> {
    let backup = export_app_backup_data_impl(pool, options).await?;
    serde_json::to_string_pretty(&backup).map_err(|e| e.to_string())
}

pub async fn export_app_backup_archive_impl(
    pool: &DbPool,
    options: BackupOptions,
) -> Result<Vec<u8>, String> {
    let mut backup = export_app_backup_data_impl(pool, options).await?;
    let mut archived_files = Vec::new();
    for skill in &mut backup.skills {
        let archive_root = if skill.storage_kind == "resource" {
            "resource-library"
        } else {
            "central-library"
        };
        let relative_dir = normalize_relative_path(&skill.relative_dir)?;
        let relative_dir = path_to_portable_relative(&relative_dir)
            .ok_or_else(|| "Backup contains an empty relative path".to_string())?;
        for file in &skill.files {
            let relative_file = normalize_relative_path(&file.relative_path)?;
            let relative_file = path_to_portable_relative(&relative_file)
                .ok_or_else(|| "Backup contains an empty relative path".to_string())?;
            let content = STANDARD.decode(&file.content_base64).map_err(|e| {
                format!("Invalid base64 content for '{}': {}", file.relative_path, e)
            })?;
            archived_files.push((
                format!("{archive_root}/{relative_dir}/{relative_file}"),
                content,
            ));
        }
        skill.files.clear();
    }

    let manifest = serde_json::to_vec_pretty(&backup).map_err(|e| e.to_string())?;
    let cursor = Cursor::new(Vec::new());
    let mut zip = zip::ZipWriter::new(cursor);
    let options = zip::write::FileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated)
        .unix_permissions(0o644);
    zip.start_file("manifest.json", options)
        .map_err(|e| format!("Failed to create backup archive: {}", e))?;
    zip.write_all(&manifest)
        .map_err(|e| format!("Failed to write backup manifest: {}", e))?;
    archived_files.sort_by(|a, b| a.0.cmp(&b.0));
    for (path, content) in archived_files {
        zip.start_file(path, options)
            .map_err(|e| format!("Failed to create backup archive entry: {}", e))?;
        zip.write_all(&content)
            .map_err(|e| format!("Failed to write backup archive entry: {}", e))?;
    }
    let cursor = zip
        .finish()
        .map_err(|e| format!("Failed to finish backup archive: {}", e))?;
    Ok(cursor.into_inner())
}

#[cfg(test)]
async fn import_app_backup_impl(pool: &DbPool, json: &str) -> Result<(), String> {
    let backup: AppBackup = serde_json::from_str(json).map_err(|e| e.to_string())?;
    import_app_backup_data_impl(pool, backup).await
}

#[cfg(test)]
async fn import_app_backup_bytes_impl(pool: &DbPool, backup: &[u8]) -> Result<(), String> {
    import_app_backup_data_impl(pool, read_app_backup_bytes(backup)?).await
}

fn read_app_backup_bytes(backup: &[u8]) -> Result<AppBackup, String> {
    if backup
        .iter()
        .copied()
        .find(|byte| !byte.is_ascii_whitespace())
        == Some(b'{')
    {
        return serde_json::from_slice(backup).map_err(|e| e.to_string());
    }
    read_app_backup_archive(backup)
}

fn read_app_backup_archive(archive: &[u8]) -> Result<AppBackup, String> {
    let reader = Cursor::new(archive);
    let mut zip = zip::ZipArchive::new(reader)
        .map_err(|e| format!("Backup archive is not a valid ZIP file: {}", e))?;
    let mut manifest_bytes = Vec::new();
    let mut archived_files: Vec<(String, Vec<u8>)> = Vec::new();
    for index in 0..zip.len() {
        let mut file = zip
            .by_index(index)
            .map_err(|e| format!("Failed to read backup archive entry: {}", e))?;
        if file.is_dir() {
            continue;
        }
        let name = file.name().replace('\\', "/");
        normalize_relative_path(&name)?;
        let mut content = Vec::new();
        file.read_to_end(&mut content)
            .map_err(|e| format!("Failed to read backup archive entry '{}': {}", name, e))?;
        if name == "manifest.json" {
            manifest_bytes = content;
        } else {
            archived_files.push((name, content));
        }
    }
    if manifest_bytes.is_empty() {
        return Err("Backup archive is missing manifest.json".to_string());
    }
    let mut backup: AppBackup =
        serde_json::from_slice(&manifest_bytes).map_err(|e| e.to_string())?;
    for skill in &mut backup.skills {
        let archive_root = if skill.storage_kind == "resource" {
            "resource-library"
        } else {
            "central-library"
        };
        let relative_dir = normalize_relative_path(&skill.relative_dir)?;
        let relative_dir = path_to_portable_relative(&relative_dir)
            .ok_or_else(|| "Backup contains an empty relative path".to_string())?;
        let prefix = format!("{archive_root}/{relative_dir}/");
        skill.files = archived_files
            .iter()
            .filter_map(|(name, content)| {
                name.strip_prefix(&prefix)
                    .map(|relative_path| SkillFileBackup {
                        relative_path: relative_path.to_string(),
                        content_base64: STANDARD.encode(content),
                    })
            })
            .collect();
    }
    Ok(backup)
}

async fn import_app_backup_data_impl(pool: &DbPool, backup: AppBackup) -> Result<(), String> {
    if backup.schema_version != BACKUP_SCHEMA_VERSION {
        return Err(format!(
            "Unsupported backup schema version {}",
            backup.schema_version
        ));
    }

    super::notebook_repository::validate(&backup.repository_notebooks)?;

    // Resolve all filesystem roots from the current database before reading any
    // backup-controlled settings or legacy path fields.
    // Restore never writes skill bundles into Central Skills. Keep reading the
    // current central_root for platform installation replay.
    let central_root = central_root(pool).await?;
    let resource_root = db::get_skill_resource_library_dir(pool).await?;
    std::fs::create_dir_all(&resource_root)
        .map_err(|e| format!("Failed to create Skill Resource Library root: {}", e))?;
    validate_restore_plan(&backup.skills)?;
    for stats in &backup.repository_stats {
        if !is_portable_github_repo(&stats.repository)
            || stats.stars < 0
            || DateTime::parse_from_rfc3339(&stats.checked_at).is_err()
        {
            return Err("Backup contains invalid repository statistics".into());
        }
    }
    let mut mapping_keys = HashSet::new();
    let mut mapping_paths = HashSet::new();
    for mapping in &backup.repository_directories {
        let relative = normalize_relative_path(&mapping.directory)?;
        if !is_portable_github_repo(&mapping.repository)
            || relative.components().count() != 2
            || !mapping_keys.insert(mapping.repository.to_lowercase())
            || !mapping_paths.insert(relative_path_key(&relative)?)
        {
            return Err("Backup contains invalid repository directory mappings".into());
        }
        super::repository_mirror::ensure_owned_path(
            &resource_root,
            &resource_root.join(&relative),
        )?;
        let existing =
            db::backup::matching_directories(pool, &mapping.repository, &mapping.directory)?;
        if existing.iter().any(|(repo, dir)| {
            !repo.eq_ignore_ascii_case(&mapping.repository) || dir != &mapping.directory
        }) {
            return Err("Backup repository directory mapping conflicts with local data".into());
        }
    }
    let mappings = backup
        .repository_directories
        .iter()
        .map(|mapping| {
            Ok(RepositoryDirectoryBackup {
                repository: mapping.repository.clone(),
                directory: path_to_portable_relative(&normalize_relative_path(&mapping.directory)?)
                    .ok_or("Invalid repository directory")?,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    db::backup::restore_directories(pool, &mappings).await?;
    super::notebook_repository::restore(pool, &backup.repository_notebooks).await?;
    for note in &backup.folder_notes {
        super::metadata::save_note(pool, note).await?;
    }

    for setting in backup.settings {
        if !is_exportable_setting_key(&setting.key) {
            continue;
        }
        db::set_setting(pool, &setting.key, &setting.value).await?;
    }

    let backup_custom_agent_ids: HashSet<String> = backup
        .agents
        .iter()
        .filter(|agent| !agent.is_builtin)
        .map(|agent| agent.id.clone())
        .collect();
    for agent in backup.agents {
        if !agent.is_builtin {
            update_existing_custom_agent_backup(pool, &agent).await?;
        }
    }

    let skill_backups = backup.skills;
    let resource_skill_ids: HashSet<String> = skill_backups
        .iter()
        .filter(|skill| skill.storage_kind == "resource")
        .map(|skill| skill.skill.id.clone())
        .collect();
    let included_skill_ids: HashSet<String> = skill_backups
        .iter()
        .map(|skill| skill.skill.id.clone())
        .collect();

    for skill in skill_backups
        .iter()
        .filter(|skill| skill.storage_kind == "resource")
    {
        let relative_dir = normalize_relative_path(&skill.relative_dir)?;
        let target_dir = resource_root.join(&relative_dir);
        replace_skill_directory(&target_dir, &skill.files)?;
        upsert_restored_resource_skill(pool, skill, &target_dir).await?;
    }

    restore_central_skills_into_resource_library(
        &resource_root,
        &skill_backups,
        &resource_skill_ids,
    )?;

    for skill in skill_backups
        .iter()
        .filter(|skill| skill.storage_kind != "resource")
    {
        if resource_skill_ids.contains(&skill.skill.id) {
            continue;
        }
        let relative_dir = normalize_relative_path(&skill.relative_dir)?;
        let target_dir = resource_root.join(&relative_dir);
        if !target_dir.join("SKILL.md").is_file() {
            continue;
        }
        upsert_restored_resource_skill(pool, skill, &target_dir).await?;
    }

    for installation in backup.skill_installations {
        if !included_skill_ids.contains(&installation.skill_id) {
            continue;
        }
        if db::get_skill_by_id(pool, &installation.skill_id)
            .await?
            .is_none()
        {
            continue;
        }
        let Some(agent) = db::get_agent_by_id(pool, &installation.agent_id).await? else {
            continue;
        };
        if backup_custom_agent_ids.contains(&installation.agent_id) && agent.is_builtin {
            continue;
        }
        if agent.id == "central" || Path::new(&agent.global_skills_dir) == central_root {
            continue;
        }
        match validated_install_method(installation.method.as_deref()) {
            BackupInstallMethod::Copy => {
                let result = async {
                    prepare_copy_installation_replay(
                        pool,
                        &installation.skill_id,
                        &installation.agent_id,
                        &central_root,
                        &agent.global_skills_dir,
                    )
                    .await?;
                    install_skill_to_agent_copy_impl(
                        pool,
                        &installation.skill_id,
                        &installation.agent_id,
                    )
                    .await
                }
                .await;
                if result.is_err() {
                    continue;
                }
            }
            BackupInstallMethod::Symlink => {
                if install_skill_to_agent_impl(pool, &installation.skill_id, &installation.agent_id)
                    .await
                    .is_err()
                {
                    continue;
                }
            }
        }
    }

    for collection in backup.collections {
        upsert_collection_backup(pool, &collection).await?;
    }

    db::backup::restore_memberships(pool, &backup.collection_skills).await?;
    db::backup::restore_stats(pool, &backup.repository_stats).await?;
    crate::commands::skills::get_resource_library_skills_impl(pool).await?;

    Ok(())
}

fn default_skill_backup_storage_kind() -> String {
    "central".to_string()
}

fn skill_from_resource_listing(skill: SkillWithLinks) -> Skill {
    Skill {
        id: skill.id,
        name: skill.name,
        description: skill.description,
        file_path: skill.file_path,
        canonical_path: skill.canonical_path,
        is_central: skill.is_central,
        source: skill.source,
        content: None,
        scanned_at: skill.scanned_at,
    }
}

fn restore_central_skills_into_resource_library(
    resource_root: &Path,
    skill_backups: &[SkillBackup],
    resource_skill_ids: &HashSet<String>,
) -> Result<(), String> {
    for skill in skill_backups {
        if skill.storage_kind == "resource"
            || resource_skill_ids.contains(&skill.skill.id)
            || skill.files.is_empty()
        {
            continue;
        }
        let relative_dir = normalize_relative_path(&skill.relative_dir)?;
        let target_dir = resource_root.join(relative_dir);
        if target_dir.join("SKILL.md").is_file() {
            continue;
        }
        replace_skill_directory(&target_dir, &skill.files)?;
    }
    Ok(())
}

async fn upsert_restored_resource_skill(
    pool: &DbPool,
    skill: &SkillBackup,
    target_dir: &Path,
) -> Result<(), String> {
    let mut db_skill = skill.skill.clone();
    db_skill.file_path = path_to_string(&target_dir.join("SKILL.md"));
    db_skill.canonical_path = Some(path_to_string(target_dir));
    db_skill.is_central = false;
    db_skill.source = sanitize_skill_origin(db_skill.source);
    db_skill.content = None;
    db::upsert_skill(pool, &db_skill).await?;
    db::backup::clear_source(pool, &db_skill.id).await?;
    if let Some(mut source) = sanitize_skill_source(skill.source.clone()) {
        source.skill_id = db_skill.id.clone();
        db::upsert_skill_source(pool, &source).await?;
    }
    if let Some(metadata) = skill.metadata.clone() {
        let tags = db::parse_skill_metadata_tags(Some(&metadata));
        db::upsert_skill_metadata(pool, &db_skill.id, metadata.notes.as_deref(), &tags).await?;
    }
    Ok(())
}

async fn append_skill_backups(
    pool: &DbPool,
    skill_backups: &mut Vec<SkillBackup>,
    skills: Vec<Skill>,
    root: &Path,
    storage_kind: &str,
) -> Result<(), String> {
    for skill in skills {
        let skill_dir = skill_directory(&skill);
        let metadata = match std::fs::symlink_metadata(&skill_dir) {
            Ok(metadata) => metadata,
            Err(_) => continue,
        };
        if metadata.file_type().is_symlink() {
            if !skill_dir.join("SKILL.md").is_file() {
                continue;
            }
        } else if !metadata.is_dir() {
            continue;
        }
        let relative_dir = relative_to_root(&skill_dir, root).unwrap_or_else(|| skill.id.clone());
        let files = if metadata.file_type().is_symlink() && storage_kind != "resource" {
            Vec::new()
        } else {
            collect_files(&skill_dir)?
        };
        let source = sanitize_skill_source(db::get_skill_source(pool, &skill.id).await?);
        let metadata = db::get_skill_metadata(pool, &skill.id).await?;
        let mut exported_skill = skill;
        exported_skill.file_path.clear();
        exported_skill.canonical_path = None;
        exported_skill.source = sanitize_skill_origin(exported_skill.source);
        skill_backups.push(SkillBackup {
            skill: exported_skill,
            source,
            metadata,
            storage_kind: storage_kind.to_string(),
            relative_dir,
            files,
        });
    }
    Ok(())
}

fn fill_central_symlink_files_without_resource(
    skill_backups: &mut [SkillBackup],
    central_root: &Path,
) -> Result<(), String> {
    let resource_ids: HashSet<String> = skill_backups
        .iter()
        .filter(|skill| skill.storage_kind == "resource")
        .map(|skill| skill.skill.id.clone())
        .collect();
    for skill in skill_backups.iter_mut() {
        if skill.storage_kind == "resource" || !skill.files.is_empty() {
            continue;
        }
        if resource_ids.contains(&skill.skill.id) {
            continue;
        }
        let skill_dir = central_root.join(normalize_relative_path(&skill.relative_dir)?);
        if skill_dir.join("SKILL.md").is_file() {
            skill.files = collect_files(&skill_dir)?;
        }
    }
    Ok(())
}

#[allow(dead_code)]
fn restore_central_skill_as_symlink(central_dir: &Path, resource_dir: &Path) -> Result<(), String> {
    if !resource_dir.join("SKILL.md").is_file() {
        return Err(format!(
            "Cannot restore Central Skills symlink '{}': resource skill '{}' is missing SKILL.md",
            central_dir.display(),
            resource_dir.display()
        ));
    }
    if let Some(parent) = central_dir.parent() {
        std::fs::create_dir_all(parent).map_err(|error| {
            format!(
                "Failed to create Central Skills parent directory '{}': {}",
                parent.display(),
                error
            )
        })?;
    }
    if let Ok(metadata) = std::fs::symlink_metadata(central_dir) {
        if metadata.file_type().is_symlink() {
            remove_symlink_path(central_dir)?;
        } else if metadata.is_dir() {
            std::fs::remove_dir_all(central_dir).map_err(|error| {
                format!(
                    "Failed to remove existing Central Skills directory '{}': {}",
                    central_dir.display(),
                    error
                )
            })?;
        } else {
            std::fs::remove_file(central_dir).map_err(|error| {
                format!(
                    "Failed to remove existing Central Skills path '{}': {}",
                    central_dir.display(),
                    error
                )
            })?;
        }
    }
    let from_dir = central_dir.parent().unwrap_or(central_dir);
    let relative_target = symlink_target_path(from_dir, resource_dir);
    create_symlink(&relative_target, central_dir)
}

async fn central_root(pool: &DbPool) -> Result<PathBuf, String> {
    let central = db::get_agent_by_id(pool, "central")
        .await?
        .ok_or_else(|| "Central agent not found in database".to_string())?;
    Ok(PathBuf::from(central.global_skills_dir))
}

fn skill_directory(skill: &Skill) -> PathBuf {
    skill
        .canonical_path
        .as_deref()
        .map(PathBuf::from)
        .or_else(|| Path::new(&skill.file_path).parent().map(Path::to_path_buf))
        .unwrap_or_else(|| PathBuf::from(&skill.file_path))
}

fn relative_to_root(path: &Path, root: &Path) -> Option<String> {
    path.strip_prefix(root)
        .ok()
        .and_then(path_to_portable_relative)
        .or_else(|| relative_to_root_from_keys(&path.to_string_lossy(), &root.to_string_lossy()))
}

fn relative_to_root_from_keys(path: &str, root: &str) -> Option<String> {
    let path_norm = normalize_backup_path_key(path);
    let root_norm = normalize_backup_path_key(root);
    if path_norm.len() <= root_norm.len() {
        return None;
    }
    let matches_root = if cfg!(windows) {
        path_norm
            .get(..root_norm.len())
            .is_some_and(|prefix| prefix.eq_ignore_ascii_case(&root_norm))
            && path_norm.as_bytes().get(root_norm.len()) == Some(&b'/')
    } else {
        path_norm.starts_with(&root_norm)
            && path_norm.as_bytes().get(root_norm.len()) == Some(&b'/')
    };
    if !matches_root {
        return None;
    }
    let rest = &path_norm[root_norm.len() + 1..];
    if rest.is_empty() || rest.contains(':') {
        return None;
    }
    Some(rest.to_string())
}

fn normalize_backup_path_key(path: &str) -> String {
    path.replace('\\', "/").trim_end_matches('/').to_string()
}

fn collect_files(root: &Path) -> Result<Vec<SkillFileBackup>, String> {
    let mut files = Vec::new();
    collect_files_inner(root, root, &mut files)?;
    files.sort_by(|a, b| a.relative_path.cmp(&b.relative_path));
    Ok(files)
}

fn collect_files_inner(
    root: &Path,
    current: &Path,
    files: &mut Vec<SkillFileBackup>,
) -> Result<(), String> {
    let entries = match std::fs::read_dir(current) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => {
            return Err(format!(
                "Failed to read directory '{}': {}",
                current.display(),
                e
            ))
        }
    };
    for entry in entries {
        let entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path();
        let metadata = match std::fs::symlink_metadata(&path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(e) => return Err(e.to_string()),
        };
        if metadata.file_type().is_symlink() {
            continue;
        }
        if metadata.is_dir() {
            collect_files_inner(root, &path, files)?;
            continue;
        }
        if metadata.is_file() {
            let relative_path = path
                .strip_prefix(root)
                .ok()
                .and_then(path_to_portable_relative)
                .ok_or_else(|| {
                    format!(
                        "File path '{}' cannot be represented as a portable backup path",
                        path.display()
                    )
                })?;
            let bytes = std::fs::read(&path)
                .map_err(|e| format!("Failed to read '{}': {}", path.display(), e))?;
            files.push(SkillFileBackup {
                relative_path,
                content_base64: STANDARD.encode(bytes),
            });
        }
    }
    Ok(())
}

fn normalize_relative_path(value: &str) -> Result<PathBuf, String> {
    let trimmed = value.trim();
    if trimmed.is_empty()
        || trimmed.starts_with('/')
        || trimmed.starts_with('\\')
        || looks_like_windows_drive_path(trimmed)
    {
        return Err("Backup contains an absolute path".to_string());
    }
    let portable = trimmed.replace('\\', "/");
    let mut normalized = PathBuf::new();
    for part in portable.split('/') {
        if part.is_empty() || part == "." || part == ".." || part.contains(':') {
            return Err("Backup contains an unsafe relative path".to_string());
        }
        normalized.push(part);
    }
    if normalized.as_os_str().is_empty() {
        return Err("Backup contains an empty relative path".to_string());
    }
    Ok(normalized)
}

fn looks_like_windows_drive_path(value: &str) -> bool {
    let bytes = value.as_bytes();
    bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':'
}

fn path_to_portable_relative(path: &Path) -> Option<String> {
    let mut parts = Vec::new();
    for component in path.components() {
        let Component::Normal(part) = component else {
            return None;
        };
        parts.push(part.to_string_lossy().to_string());
    }
    if parts.is_empty() {
        None
    } else {
        Some(parts.join("/"))
    }
}

fn validate_skill_backup(skill: &SkillBackup) -> Result<(), String> {
    normalize_relative_path(&skill.relative_dir)?;
    let mut paths: Vec<(PathBuf, String)> = Vec::new();
    let mut has_skill_md = false;
    for file in &skill.files {
        let relative_path = normalize_relative_path(&file.relative_path)?;
        let key = relative_path_key(&relative_path)?;
        let bytes = STANDARD
            .decode(&file.content_base64)
            .map_err(|e| format!("Invalid base64 content for '{}': {}", file.relative_path, e))?;
        if key == "skill.md" {
            let content = std::str::from_utf8(&bytes)
                .map_err(|_| "Backup skill SKILL.md is not valid UTF-8".to_string())?;
            validate_skill_md_content(content)?;
            has_skill_md = true;
        }
        paths.push((relative_path, key));
    }
    if skill.files.is_empty() {
        if skill.storage_kind == "resource" {
            return Err("Backup skill is missing SKILL.md".to_string());
        }
        return Ok(());
    }
    if !has_skill_md {
        return Err("Backup skill is missing SKILL.md".to_string());
    }
    paths.sort_by(|a, b| a.1.cmp(&b.1));
    for index in 0..paths.len() {
        if index > 0 && paths[index].1 == paths[index - 1].1 {
            return Err("Backup contains duplicate file paths".to_string());
        }
        for previous in &paths[..index] {
            if paths[index].1.starts_with(&format!("{}/", previous.1)) {
                return Err("Backup contains conflicting file and directory paths".to_string());
            }
        }
    }
    Ok(())
}

fn validate_skill_md_content(content: &str) -> Result<(), String> {
    super::scanner::parse_skill_md_content(content)
        .filter(|info| !info.name.trim().is_empty())
        .map(|_| ())
        .ok_or_else(|| "Backup skill SKILL.md is missing valid frontmatter or name".to_string())
}

fn validate_restore_plan(skills: &[SkillBackup]) -> Result<(), String> {
    let resource_ids: HashSet<String> = skills
        .iter()
        .filter(|skill| skill.storage_kind == "resource")
        .map(|skill| skill.skill.id.clone())
        .collect();
    let mut targets = Vec::new();
    for skill in skills {
        validate_skill_backup(skill)?;
        if skill.storage_kind != "resource"
            && skill.files.is_empty()
            && !resource_ids.contains(&skill.skill.id)
        {
            return Err(
                "Central skill backup is a symlink placeholder without resource files".to_string(),
            );
        }
        let relative_dir = normalize_relative_path(&skill.relative_dir)?;
        let storage = if skill.storage_kind == "resource" {
            "resource"
        } else {
            "central"
        };
        targets.push((storage.to_string(), relative_path_key(&relative_dir)?));
    }
    targets.sort();
    for index in 1..targets.len() {
        let previous = &targets[index - 1];
        let current = &targets[index];
        if previous.0 == current.0
            && (previous.1 == current.1 || current.1.starts_with(&format!("{}/", previous.1)))
        {
            return Err("Backup contains conflicting skill restore paths".to_string());
        }
    }
    Ok(())
}

fn relative_path_key(path: &Path) -> Result<String, String> {
    path_to_portable_relative(path)
        .map(|path| path.to_ascii_lowercase())
        .ok_or_else(|| "Backup contains an empty relative path".to_string())
}

fn replace_skill_directory(target_dir: &Path, files: &[SkillFileBackup]) -> Result<(), String> {
    let parent = target_dir
        .parent()
        .ok_or_else(|| "Backup target has no parent directory".to_string())?;
    ensure_existing_ancestors_are_directories(parent)?;
    std::fs::create_dir_all(parent)
        .map_err(|e| format!("Failed to create directory '{}': {}", parent.display(), e))?;
    if let Ok(metadata) = std::fs::symlink_metadata(target_dir) {
        if metadata.file_type().is_symlink() || !metadata.is_dir() {
            return Err(format!(
                "Backup target '{}' is not a replaceable skill directory",
                target_dir.display()
            ));
        }
    }
    let name = target_dir
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("skill");
    let unique = Uuid::new_v4().simple().to_string();
    let staging_dir = parent.join(format!(".{name}.restore-{unique}"));
    let old_dir = parent.join(format!(".{name}.old-{unique}"));

    std::fs::create_dir_all(&staging_dir)
        .map_err(|e| format!("Failed to create staging directory: {}", e))?;
    if let Err(error) = write_files(&staging_dir, files) {
        let _ = std::fs::remove_dir_all(&staging_dir);
        return Err(error);
    }

    let had_existing = target_dir.exists();
    if had_existing {
        std::fs::rename(target_dir, &old_dir).map_err(|e| {
            let _ = std::fs::remove_dir_all(&staging_dir);
            format!(
                "Failed to stage existing skill directory '{}': {}",
                target_dir.display(),
                e
            )
        })?;
    }

    if let Err(error) = std::fs::rename(&staging_dir, target_dir) {
        if had_existing {
            let _ = std::fs::rename(&old_dir, target_dir);
        }
        let _ = std::fs::remove_dir_all(&staging_dir);
        return Err(format!(
            "Failed to replace skill directory '{}': {}",
            target_dir.display(),
            error
        ));
    }

    if had_existing {
        let _ = std::fs::remove_dir_all(&old_dir);
    }
    Ok(())
}

fn ensure_existing_ancestors_are_directories(path: &Path) -> Result<(), String> {
    let mut current = Some(path);
    while let Some(candidate) = current {
        if candidate.as_os_str().is_empty() {
            break;
        }
        match std::fs::symlink_metadata(candidate) {
            Ok(metadata) => {
                if metadata.file_type().is_symlink() || !metadata.is_dir() {
                    return Err(format!(
                        "Backup target parent '{}' is not a safe directory",
                        candidate.display()
                    ));
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => {
                return Err(format!(
                    "Failed to inspect backup target parent '{}': {}",
                    candidate.display(),
                    error
                ));
            }
        }
        current = candidate.parent();
    }
    Ok(())
}

fn write_files(root: &Path, files: &[SkillFileBackup]) -> Result<(), String> {
    for file in files {
        let relative_path = normalize_relative_path(&file.relative_path)?;
        let target = root.join(relative_path);
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("Failed to create directory '{}': {}", parent.display(), e))?;
        }
        let bytes = STANDARD
            .decode(&file.content_base64)
            .map_err(|e| format!("Invalid base64 content for '{}': {}", file.relative_path, e))?;
        std::fs::write(&target, bytes)
            .map_err(|e| format!("Failed to write '{}': {}", target.display(), e))?;
    }
    Ok(())
}

async fn prepare_copy_installation_replay(
    pool: &DbPool,
    skill_id: &str,
    agent_id: &str,
    central_root: &Path,
    agent_root: &str,
) -> Result<(), String> {
    let Some(existing) = db::get_skill_installations(pool, skill_id)
        .await?
        .into_iter()
        .find(|installation| installation.agent_id == agent_id)
    else {
        return Ok(());
    };
    if existing.link_type != "copy" {
        return Ok(());
    }
    let Some(skill) = db::get_skill_by_id(pool, skill_id).await? else {
        return Ok(());
    };
    let canonical_path = skill.canonical_path.as_deref().unwrap_or(&skill.file_path);
    let canonical_dir = if canonical_path.ends_with("SKILL.md") {
        Path::new(canonical_path)
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from(canonical_path))
    } else {
        PathBuf::from(canonical_path)
    };
    let relative = canonical_dir
        .strip_prefix(central_root)
        .ok()
        .and_then(path_to_portable_relative)
        .unwrap_or_else(|| skill_id.to_string());
    let expected_path = PathBuf::from(agent_root).join(normalize_relative_path(&relative)?);
    let existing_path = PathBuf::from(&existing.installed_path);
    if existing_path != expected_path || !expected_path.exists() {
        return Ok(());
    }
    if !expected_path.starts_with(agent_root) {
        return Err("Existing copy installation is outside the current agent root".to_string());
    }
    let metadata = std::fs::symlink_metadata(&expected_path).map_err(|e| {
        format!(
            "Failed to inspect existing copy installation '{}': {}",
            expected_path.display(),
            e
        )
    })?;
    if metadata.is_dir() && !metadata.file_type().is_symlink() {
        std::fs::remove_dir_all(&expected_path).map_err(|e| {
            format!(
                "Failed to refresh existing copy installation '{}': {}",
                expected_path.display(),
                e
            )
        })?;
    }
    Ok(())
}

async fn update_existing_custom_agent_backup(
    pool: &DbPool,
    agent: &AgentBackup,
) -> Result<(), String> {
    db::backup::restore_custom_agent(
        pool,
        &agent.id,
        &agent.display_name,
        agent.is_detected,
        agent.is_enabled,
    )
    .await
}

async fn upsert_collection_backup(pool: &DbPool, collection: &Collection) -> Result<(), String> {
    db::backup::restore_collection(pool, collection).await
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BackupInstallMethod {
    Symlink,
    Copy,
}

impl BackupInstallMethod {
    fn as_str(self) -> &'static str {
        match self {
            Self::Symlink => "symlink",
            Self::Copy => "copy",
        }
    }
}

fn validated_install_method(value: Option<&str>) -> BackupInstallMethod {
    match value.map(str::trim) {
        Some("copy") => BackupInstallMethod::Copy,
        Some("symlink") => BackupInstallMethod::Symlink,
        _ => BackupInstallMethod::Symlink,
    }
}

const BACKUP_SETTING_ALLOWLIST: &[&str] = &[
    "language",
    "theme",
    "accent",
    "accent_color",
    "ai_provider",
    "ai_region",
    "ai_model",
];

fn is_exportable_setting_key(key: &str) -> bool {
    BACKUP_SETTING_ALLOWLIST.contains(&key)
}

fn sanitize_skill_source(source: Option<SkillSource>) -> Option<SkillSource> {
    source.map(|mut source| {
        if source
            .source_url
            .as_deref()
            .is_some_and(|url| !is_safe_backup_url(url))
        {
            source.source_url = None;
        }
        if source
            .source_path
            .as_deref()
            .is_some_and(|path| !is_portable_backup_path(path))
        {
            source.source_path = None;
        }
        source
    })
}

fn is_safe_backup_url(value: &str) -> bool {
    let Ok(url) = Url::parse(value) else {
        return false;
    };
    let scheme = url.scheme();
    matches!(scheme, "http" | "https")
        && url.username().is_empty()
        && url.password().is_none()
        && url.query().is_none()
        && url.fragment().is_none()
}

fn sanitize_skill_origin(source: Option<String>) -> Option<String> {
    let source = source?.trim().to_string();
    if source.is_empty() {
        return None;
    }
    if Url::parse(&source).is_ok() {
        return is_safe_backup_url(&source).then_some(source);
    }
    if source.contains("://")
        || source.starts_with('/')
        || source.starts_with('\\')
        || source.contains('\\')
    {
        return None;
    }
    let bytes = source.as_bytes();
    if bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':' {
        return None;
    }
    if let Some(repo) = source.strip_prefix("github:") {
        is_portable_github_repo(repo).then_some(source)
    } else if source.contains(':') || source.contains('/') {
        None
    } else {
        is_simple_source_label(&source).then_some(source)
    }
}

fn is_portable_github_repo(value: &str) -> bool {
    let mut parts = value.split('/');
    let Some(owner) = parts.next() else {
        return false;
    };
    let Some(repo) = parts.next() else {
        return false;
    };
    parts.next().is_none() && is_simple_source_label(owner) && is_simple_source_label(repo)
}

fn is_simple_source_label(value: &str) -> bool {
    !value.is_empty()
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
}

fn is_portable_backup_path(value: &str) -> bool {
    let trimmed = value.trim();
    if trimmed.is_empty()
        || trimmed.starts_with('/')
        || trimmed.starts_with('\\')
        || trimmed.contains('\\')
    {
        return false;
    }
    let bytes = trimmed.as_bytes();
    if bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':' {
        return false;
    }
    trimmed
        .split('/')
        .all(|part| !part.is_empty() && part != "." && part != "..")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::linker::add_resource_skill_to_central_impl;
    use crate::path_utils::remove_symlink_path;
    use std::io::{Cursor, Read};
    use tempfile::tempdir;

    #[test]
    fn public_backup_commands_always_use_complete_options() {
        let requested = BackupOptions {
            include_resource_library: false,
            include_central_library: true,
            include_app_config: false,
            include_installations: false,
        };

        assert_eq!(
            complete_backup_options(Some(requested)),
            BackupOptions {
                include_resource_library: true,
                include_central_library: false,
                include_app_config: true,
                include_installations: true,
            }
        );
    }

    async fn setup_test_db() -> (DbPool, tempfile::TempDir) {
        let dir = tempdir().expect("tempdir");
        let db_path = dir.path().join("db.sqlite");
        let pool = db::create_pool(&db_path.to_string_lossy())
            .await
            .expect("pool");
        db::init_database(&pool).await.expect("init");
        let central = dir.path().join("central");
        std::fs::create_dir_all(&central).expect("central");
        pool.write(|data| {
            data.table_mut("agents").get_mut("central").unwrap()["global_skills_dir"] =
                serde_json::json!(central.to_string_lossy());
            Ok(())
        })
        .await
        .unwrap();
        let resource = dir.path().join("resource-library");
        std::fs::create_dir_all(&resource).expect("resource");
        db::set_skill_resource_library_dir(&pool, &resource.to_string_lossy())
            .await
            .expect("resource path");
        (pool, dir)
    }

    #[tokio::test]
    async fn repository_backup_preserves_migrated_directories_and_rejects_unsafe_paths() {
        let (pool, _dir) = setup_test_db().await;
        db::backup::restore_directories(
            &pool,
            &[RepositoryDirectoryBackup {
                repository: "new/repo".into(),
                directory: "old/repo".into(),
            }],
        )
        .await
        .unwrap();
        let backup = export_app_backup_data_impl(&pool, repository_backup_options())
            .await
            .unwrap();
        assert_eq!(backup.repository_directories.len(), 1);
        let (restored, _other) = setup_test_db().await;
        import_app_backup_data_impl(&restored, backup.clone())
            .await
            .unwrap();
        let root = db::get_skill_resource_library_dir(&restored).await.unwrap();
        assert_eq!(
            super::super::repository_address::directory(&restored, &root, "new", "new/repo")
                .await
                .unwrap(),
            root.join("old/repo")
        );
        let mut invalid = backup;
        invalid.repository_directories[0].directory = "../escape".into();
        let (fresh, _fresh_dir) = setup_test_db().await;
        assert!(import_app_backup_data_impl(&fresh, invalid).await.is_err());
        let count = fresh
            .read(|data| Ok(data.table("repository_local_directories").len()))
            .unwrap();
        assert_eq!(count, 0);
    }

    #[tokio::test]
    async fn repository_backup_roundtrip_preserves_folder_notes() {
        let (pool, _dir) = setup_test_db().await;
        super::super::metadata::save_note(
            &pool,
            &super::super::metadata::FolderNote {
                id: "folder".into(),
                notes: "memo".into(),
                skill_ids: vec!["member".into()],
            },
        )
        .await
        .unwrap();
        let backup = export_app_backup_data_impl(&pool, repository_backup_options())
            .await
            .unwrap();
        assert_eq!(backup.folder_notes.len(), 1);
        let (restored, _other) = setup_test_db().await;
        import_app_backup_data_impl(&restored, backup)
            .await
            .unwrap();
        let notes = super::super::metadata::folder_notes(&restored)
            .await
            .unwrap();
        assert_eq!(notes[0].notes, "memo");
        assert_eq!(notes[0].skill_ids, vec!["member"]);
    }

    #[tokio::test]
    async fn repository_backup_preserves_notebook_identities_without_accessing_external_files() {
        use super::super::notebook_repository as notebooks;
        let (pool, _dir) = setup_test_db().await;
        let mut row = notebooks::Repository::new(42, "fixture/project".into());
        row.notebook = "historical internal body".into();
        let note_id = uuid::Uuid::new_v4().to_string();
        row.obsidian = Some(super::super::notebook::Binding {
            binding_version: 2,
            note_id: Some(note_id.clone()),
            vault_key: "default".into(),
            relative_path: "Missing Vault/renamed.md".into(),
            obsidian_uri: format!("obsidian://skillshub-note?id={note_id}"),
        });
        notebooks::put(&pool, &row).await.unwrap();
        let backup = export_app_backup_data_impl(&pool, repository_backup_options())
            .await
            .unwrap();
        assert_eq!(backup.repository_notebooks.len(), 1);
        let (restored, _other) = setup_test_db().await;
        import_app_backup_data_impl(&restored, backup)
            .await
            .unwrap();
        let result = notebooks::list(&restored).await.unwrap().remove(0);
        assert_eq!(result.repository_uid, row.repository_uid);
        assert_eq!(result.notebook, row.notebook);
        assert_eq!(result.obsidian, row.obsidian);
        // Restoring the same backup never duplicates the stable record.
        notebooks::restore(&restored, &[result]).await.unwrap();
        assert_eq!(notebooks::list(&restored).await.unwrap().len(), 1);
    }

    async fn configure_agent_root(pool: &DbPool, agent_id: &str, root: &Path) {
        std::fs::create_dir_all(root).expect("agent root");
        pool.write(|data| {
            data.table_mut("agents").get_mut(agent_id).unwrap()["global_skills_dir"] =
                serde_json::json!(path_to_string(root));
            Ok(())
        })
        .await
        .unwrap();
    }

    fn zip_entry_bytes(archive: &[u8], name: &str) -> Vec<u8> {
        let reader = Cursor::new(archive);
        let mut zip = zip::ZipArchive::new(reader).expect("zip archive");
        let mut file = zip.by_name(name).expect("zip entry");
        let mut bytes = Vec::new();
        file.read_to_end(&mut bytes).expect("zip bytes");
        bytes
    }

    fn zip_has_entry(archive: &[u8], name: &str) -> bool {
        let reader = Cursor::new(archive);
        let zip = zip::ZipArchive::new(reader).expect("zip archive");
        let found = zip.file_names().any(|entry| entry == name);
        found
    }

    fn zip_entry_text(archive: &[u8], name: &str) -> String {
        String::from_utf8(zip_entry_bytes(archive, name)).expect("utf8 zip entry")
    }

    #[tokio::test]
    async fn repository_sync_restores_windows_paths_and_collections_without_local_config() {
        let (source, source_dir) = setup_test_db().await;
        let source_root = source_dir.path().join("source-library");
        db::set_skill_resource_library_dir(&source, &source_root.to_string_lossy())
            .await
            .unwrap();
        let bundle = source_root.join("owner/repo/demo");
        std::fs::create_dir_all(&bundle).unwrap();
        // Third-party metadata accepted by the scanner must also survive restore.
        std::fs::write(
            bundle.join("SKILL.md"),
            "---\r\nname: Demo\r\nmetadata: [invalid extension\r\n---\r\nBody",
        )
        .unwrap();
        std::fs::write(bundle.join("asset.txt"), "portable asset").unwrap();
        let skills = crate::commands::skills::get_resource_library_skills_impl(&source)
            .await
            .unwrap();
        let collection = db::create_collection(&source, "Demo collection", None)
            .await
            .unwrap();
        db::backup::restore_memberships(
            &source,
            &[CollectionSkillBackup {
                collection_id: collection.id.clone(),
                skill_id: skills[0].id.clone(),
                added_at: "2026-01-01T00:00:00Z".into(),
            }],
        )
        .await
        .unwrap();
        db::set_setting(&source, "language", "zh").await.unwrap();
        let archive = export_app_backup_archive_impl(&source, repository_backup_options())
            .await
            .unwrap();
        let portable = read_app_backup_bytes(&archive).unwrap();
        assert_eq!(portable.collections.len(), 1);
        assert_eq!(portable.collection_skills.len(), 1);
        assert!(portable.settings.is_empty());
        assert!(portable.agents.is_empty());
        assert!(portable.scan_directories.is_empty());
        assert!(portable.skill_installations.is_empty());

        // Old full backups are narrowed on WebDAV import, including legacy Windows separators.
        let archive = export_app_backup_archive_impl(&source, BackupOptions::default())
            .await
            .unwrap();
        let mut legacy = read_app_backup_bytes(&archive).unwrap();
        for skill in &mut legacy.skills {
            skill.relative_dir = skill.relative_dir.replace('/', "\\");
            for file in &mut skill.files {
                file.relative_path = file.relative_path.replace('/', "\\");
            }
        }
        restrict_to_repository(&mut legacy);
        let (target, target_dir) = setup_test_db().await;
        let target_root = target_dir.path().join("target-library");
        db::set_skill_resource_library_dir(&target, &target_root.to_string_lossy())
            .await
            .unwrap();
        db::set_setting(&target, "language", "en").await.unwrap();
        import_app_backup_data_impl(&target, legacy).await.unwrap();
        assert_eq!(
            std::fs::read_to_string(target_root.join("owner/repo/demo/asset.txt")).unwrap(),
            "portable asset"
        );
        assert_eq!(
            db::get_setting(&target, "language")
                .await
                .unwrap()
                .as_deref(),
            Some("en")
        );
        assert_eq!(db::get_all_collections(&target).await.unwrap().len(), 1);
        let members = (target
            .read(|data| Ok(data.table("collection_skills").len()))
            .unwrap(),);
        assert_eq!(members.0, 1);
    }

    #[tokio::test]
    async fn repository_symlink_backup_contains_files() {
        let (pool, dir) = setup_test_db().await;
        let root = dir.path().join("library");
        let actual = dir.path().join("actual");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::create_dir_all(&actual).unwrap();
        std::fs::write(actual.join("SKILL.md"), "---\nname: Demo\n---\n").unwrap();
        create_symlink(&actual, &root.join("demo")).unwrap();
        db::set_skill_resource_library_dir(&pool, &root.to_string_lossy())
            .await
            .unwrap();
        let archive = export_app_backup_archive_impl(&pool, repository_backup_options())
            .await
            .unwrap();
        let backup = read_app_backup_bytes(&archive).unwrap();
        assert_eq!(backup.skills.len(), 1);
        validate_restore_plan(&backup.skills).unwrap();
        assert!(backup.skills[0]
            .files
            .iter()
            .any(|file| file.relative_path == "SKILL.md"));
    }

    #[tokio::test]
    async fn archive_backup_stores_resource_files_outside_manifest_json() {
        let (pool, dir) = setup_test_db().await;
        let resource_root = dir.path().join("resource-library");
        db::set_skill_resource_library_dir(&pool, &resource_root.to_string_lossy())
            .await
            .expect("resource dir");
        let skill_dir = resource_root.join("manual-pack").join("demo-skill");
        std::fs::create_dir_all(&skill_dir).expect("skill dir");
        std::fs::write(
            skill_dir.join("SKILL.md"),
            "---\nname: Demo Skill\n---\n\nUse this skill.",
        )
        .expect("skill");
        std::fs::write(skill_dir.join("asset.txt"), "asset body").expect("asset");
        db::upsert_skill(
            &pool,
            &Skill {
                id: "demo-skill".to_string(),
                name: "Demo Skill".to_string(),
                description: None,
                file_path: path_to_string(&skill_dir.join("SKILL.md")),
                canonical_path: Some(path_to_string(&skill_dir)),
                is_central: false,
                source: Some("manual".to_string()),
                content: None,
                scanned_at: "2026-01-01T00:00:00Z".to_string(),
            },
        )
        .await
        .expect("skill");

        let archive = export_app_backup_archive_impl(
            &pool,
            BackupOptions {
                include_resource_library: true,
                include_central_library: false,
                include_app_config: false,
                include_installations: false,
            },
        )
        .await
        .expect("archive export");

        let manifest = zip_entry_text(&archive, "manifest.json");
        assert!(manifest.contains("\"schema_version\""));
        assert!(manifest.contains("\"relative_dir\": \"manual-pack/demo-skill\""));
        assert!(!manifest.contains("contentBase64"));
        assert!(!manifest.contains(&STANDARD.encode("asset body")));
        assert_eq!(
            zip_entry_text(&archive, "resource-library/manual-pack/demo-skill/SKILL.md"),
            "---\nname: Demo Skill\n---\n\nUse this skill."
        );
        assert_eq!(
            zip_entry_text(
                &archive,
                "resource-library/manual-pack/demo-skill/asset.txt"
            ),
            "asset body"
        );
    }

    #[tokio::test]
    async fn archive_export_skips_skills_whose_directory_is_missing() {
        let (pool, dir) = setup_test_db().await;
        let resource_root = dir.path().join("resource-library");
        db::set_skill_resource_library_dir(&pool, &resource_root.to_string_lossy())
            .await
            .expect("resource dir");
        let missing_dir = resource_root.join("gone-skill");
        db::upsert_skill(
            &pool,
            &Skill {
                id: "gone-skill".to_string(),
                name: "Gone Skill".to_string(),
                description: None,
                file_path: path_to_string(&missing_dir.join("SKILL.md")),
                canonical_path: Some(path_to_string(&missing_dir)),
                is_central: false,
                source: Some("manual".to_string()),
                content: None,
                scanned_at: "2026-01-01T00:00:00Z".to_string(),
            },
        )
        .await
        .expect("skill");

        let archive = export_app_backup_archive_impl(
            &pool,
            BackupOptions {
                include_resource_library: true,
                include_central_library: false,
                include_app_config: false,
                include_installations: false,
            },
        )
        .await
        .expect("archive export");

        let manifest = zip_entry_text(&archive, "manifest.json");
        assert!(!manifest.contains("gone-skill"));
    }

    #[tokio::test]
    async fn archive_export_includes_resource_skills_present_on_disk_but_missing_from_db() {
        let (pool, dir) = setup_test_db().await;
        let resource_root = dir.path().join("resource-library");
        db::set_skill_resource_library_dir(&pool, &resource_root.to_string_lossy())
            .await
            .expect("resource dir");
        let skill_dir = resource_root
            .join("author")
            .join("repo")
            .join("disk-only-skill");
        std::fs::create_dir_all(&skill_dir).expect("skill dir");
        std::fs::write(
            skill_dir.join("SKILL.md"),
            "---\nname: Disk Only Skill\ndescription: Indexed during backup export\n---\n",
        )
        .expect("skill");

        let archive = export_app_backup_archive_impl(
            &pool,
            BackupOptions {
                include_resource_library: true,
                include_central_library: false,
                include_app_config: false,
                include_installations: false,
            },
        )
        .await
        .expect("archive export");

        let manifest = zip_entry_text(&archive, "manifest.json");
        assert!(manifest.contains("\"relative_dir\": \"author/repo/disk-only-skill\""));
        assert_eq!(
            zip_entry_text(
                &archive,
                "resource-library/author/repo/disk-only-skill/SKILL.md"
            ),
            "---\nname: Disk Only Skill\ndescription: Indexed during backup export\n---\n"
        );
    }

    #[tokio::test]
    async fn archive_import_restores_resource_library_files_and_metadata() {
        let (pool, dir) = setup_test_db().await;
        let resource_root = dir.path().join("resource-library");
        db::set_skill_resource_library_dir(&pool, &resource_root.to_string_lossy())
            .await
            .expect("resource dir");
        let skill_dir = resource_root.join("manual-pack").join("roundtrip-skill");
        std::fs::create_dir_all(&skill_dir).expect("skill dir");
        std::fs::write(
            skill_dir.join("SKILL.md"),
            "---\nname: Roundtrip Skill\n---\n\nUse this skill.",
        )
        .expect("skill");
        std::fs::write(skill_dir.join("asset.txt"), "asset body").expect("asset");
        db::upsert_skill(
            &pool,
            &Skill {
                id: "roundtrip-skill".to_string(),
                name: "Roundtrip Skill".to_string(),
                description: None,
                file_path: path_to_string(&skill_dir.join("SKILL.md")),
                canonical_path: Some(path_to_string(&skill_dir)),
                is_central: false,
                source: Some("manual".to_string()),
                content: None,
                scanned_at: "2026-01-01T00:00:00Z".to_string(),
            },
        )
        .await
        .expect("skill");
        db::upsert_skill_metadata(
            &pool,
            "roundtrip-skill",
            Some("Local note"),
            &["manual".to_string()],
        )
        .await
        .expect("metadata");

        let archive = export_app_backup_archive_impl(&pool, BackupOptions::default())
            .await
            .expect("archive export");
        std::fs::remove_dir_all(&skill_dir).expect("remove files");
        db::delete_skill(&pool, "roundtrip-skill")
            .await
            .expect("delete skill");

        import_app_backup_bytes_impl(&pool, &archive)
            .await
            .expect("archive import");

        assert_eq!(
            std::fs::read_to_string(skill_dir.join("asset.txt")).expect("asset"),
            "asset body"
        );
        let skill = db::get_skill_by_id(&pool, "roundtrip-skill")
            .await
            .expect("skill lookup")
            .expect("skill");
        assert!(!skill.is_central);
        let expected_skill_dir = path_to_string(&skill_dir);
        assert_eq!(
            skill.canonical_path.as_deref(),
            Some(expected_skill_dir.as_str())
        );
        let metadata = db::get_skill_metadata(&pool, "roundtrip-skill")
            .await
            .expect("metadata lookup")
            .expect("metadata");
        assert_eq!(metadata.notes.as_deref(), Some("Local note"));
    }

    #[tokio::test]
    async fn archive_import_rejects_unsafe_entry_paths_before_writing() {
        let (pool, dir) = setup_test_db().await;
        let resource_root = dir.path().join("resource-library");
        db::set_skill_resource_library_dir(&pool, &resource_root.to_string_lossy())
            .await
            .expect("resource dir");
        let manifest = serde_json::json!({
            "schema_version": BACKUP_SCHEMA_VERSION,
            "exported_at": "2026-01-01T00:00:00Z",
            "included": BackupOptions::default(),
            "skills": [],
            "collections": [],
            "collection_skills": [],
            "settings": [],
            "agents": [],
            "scan_directories": [],
            "skill_installations": []
        });
        let cursor = Cursor::new(Vec::new());
        let mut zip = zip::ZipWriter::new(cursor);
        let options =
            zip::write::FileOptions::default().compression_method(zip::CompressionMethod::Deflated);
        zip.start_file("manifest.json", options)
            .expect("manifest entry");
        zip.write_all(manifest.to_string().as_bytes())
            .expect("manifest");
        zip.start_file("../outside.txt", options)
            .expect("unsafe entry");
        zip.write_all(b"outside").expect("unsafe content");
        let archive = zip.finish().expect("zip").into_inner();

        let error = import_app_backup_bytes_impl(&pool, &archive)
            .await
            .expect_err("unsafe archive accepted");
        assert!(
            error.contains("absolute path") || error.contains("unsafe relative path"),
            "unexpected error: {error}"
        );
        assert!(!dir.path().join("outside.txt").exists());
    }

    #[tokio::test]
    async fn backup_roundtrip_preserves_skill_installations() {
        let (source_pool, source_dir) = setup_test_db().await;
        let source_agent_root = source_dir.path().join("source-windsurf");
        configure_agent_root(&source_pool, "windsurf", &source_agent_root).await;
        let resource_root = source_dir.path().join("resource-library");
        db::set_skill_resource_library_dir(&source_pool, &resource_root.to_string_lossy())
            .await
            .expect("resource dir");
        let skill_dir = resource_root.join("installed-demo");
        std::fs::create_dir_all(&skill_dir).expect("skill dir");
        std::fs::write(
            skill_dir.join("SKILL.md"),
            "---\nname: Installed Demo\n---\n",
        )
        .expect("skill");

        db::upsert_skill(
            &source_pool,
            &Skill {
                id: "installed-demo".to_string(),
                name: "Installed Demo".to_string(),
                description: None,
                file_path: path_to_string(&skill_dir.join("SKILL.md")),
                canonical_path: Some(path_to_string(&skill_dir)),
                is_central: false,
                source: Some("https://example.com/raw/SKILL.md?token=secret".to_string()),
                content: None,
                scanned_at: "2026-01-01T00:00:00Z".to_string(),
            },
        )
        .await
        .expect("skill");
        crate::commands::linker::install_skill_to_agent_copy_impl(
            &source_pool,
            "installed-demo",
            "windsurf",
        )
        .await
        .expect("installation");

        let json = export_app_backup_impl(&source_pool, BackupOptions::default())
            .await
            .expect("export");
        assert!(!json.contains("installed_path"));
        assert!(!json.contains("symlink_target"));
        assert!(json.contains("\"method\": \"copy\""));

        let (target_pool, target_dir) = setup_test_db().await;
        let target_agent_root = target_dir.path().join("target-windsurf");
        configure_agent_root(&target_pool, "windsurf", &target_agent_root).await;
        let target_resource = target_dir.path().join("resource-library");
        db::set_skill_resource_library_dir(&target_pool, &target_resource.to_string_lossy())
            .await
            .expect("target resource dir");
        import_app_backup_impl(&target_pool, &json)
            .await
            .expect("import");

        let installations = db::get_skill_installations(&target_pool, "installed-demo")
            .await
            .expect("installations");
        assert_eq!(installations.len(), 1);
        assert_eq!(installations[0].agent_id, "windsurf");
        assert_eq!(installations[0].link_type, "copy");
        assert_eq!(
            installations[0].installed_path,
            path_to_string(&target_agent_root.join("installed-demo"))
        );
        assert!(target_agent_root.join("installed-demo").is_dir());
        assert_eq!(installations[0].symlink_target.as_deref(), None);

        std::fs::write(
            target_agent_root.join("installed-demo").join("stale.txt"),
            "stale copy content",
        )
        .expect("stale copy marker");
        import_app_backup_impl(&target_pool, &json)
            .await
            .expect("repeat import refreshes copy");
        assert!(target_agent_root.join("installed-demo").is_dir());
        assert!(!target_agent_root
            .join("installed-demo")
            .join("stale.txt")
            .exists());
    }

    #[tokio::test]
    async fn backup_excludes_unmanaged_skill_installations() {
        let (pool, _dir) = setup_test_db().await;
        let central = central_root(&pool).await.expect("central");
        let skill_dir = central.join("external-demo");
        std::fs::create_dir_all(&skill_dir).expect("skill dir");
        std::fs::write(
            skill_dir.join("SKILL.md"),
            "---\nname: External Demo\n---\n",
        )
        .expect("skill");
        db::upsert_skill(
            &pool,
            &Skill {
                id: "external-demo".to_string(),
                name: "External Demo".to_string(),
                description: None,
                file_path: path_to_string(&skill_dir.join("SKILL.md")),
                canonical_path: Some(path_to_string(&skill_dir)),
                is_central: true,
                source: None,
                content: None,
                scanned_at: "2026-01-01T00:00:00Z".to_string(),
            },
        )
        .await
        .expect("skill");
        db::upsert_scanned_skill_installation(
            &pool,
            &db::SkillInstallation {
                skill_id: "external-demo".into(),
                agent_id: "cursor".into(),
                installed_path: path_to_string(&skill_dir),
                link_type: "native".into(),
                symlink_target: None,
                created_at: "2026-01-01T00:00:00Z".into(),
            },
        )
        .await
        .expect("unmanaged relation");

        let json = export_app_backup_impl(&pool, BackupOptions::default())
            .await
            .expect("export");
        let manifest: serde_json::Value = serde_json::from_str(&json).expect("manifest");

        assert_eq!(manifest["skill_installations"], serde_json::json!([]));
    }

    #[tokio::test]
    async fn legacy_installation_paths_are_ignored_during_import() {
        let (source_pool, source_dir) = setup_test_db().await;
        let resource_root = source_dir.path().join("resource-library");
        db::set_skill_resource_library_dir(&source_pool, &resource_root.to_string_lossy())
            .await
            .expect("resource dir");
        let skill_dir = resource_root.join("legacy-demo");
        std::fs::create_dir_all(&skill_dir).expect("skill dir");
        std::fs::write(skill_dir.join("SKILL.md"), "---\nname: Legacy Demo\n---\n").expect("skill");
        db::upsert_skill(
            &source_pool,
            &Skill {
                id: "legacy-demo".to_string(),
                name: "Legacy Demo".to_string(),
                description: None,
                file_path: path_to_string(&skill_dir.join("SKILL.md")),
                canonical_path: Some(path_to_string(&skill_dir)),
                is_central: false,
                source: None,
                content: None,
                scanned_at: "2026-01-01T00:00:00Z".to_string(),
            },
        )
        .await
        .expect("skill");
        let json = export_app_backup_impl(
            &source_pool,
            BackupOptions {
                include_installations: false,
                ..BackupOptions::default()
            },
        )
        .await
        .expect("export");
        let mut value: serde_json::Value = serde_json::from_str(&json).expect("backup");
        value["skill_installations"] = serde_json::json!([{
            "skill_id": "legacy-demo",
            "agent_id": "windsurf",
            "installed_path": "D:\\\\outside\\\\dangerous",
            "link_type": "copy",
            "symlink_target": "D:\\\\outside\\\\canonical",
            "created_at": "2026-01-02T00:00:00Z"
        }]);

        let (target_pool, target_dir) = setup_test_db().await;
        let target_agent_root = target_dir.path().join("target-windsurf");
        configure_agent_root(&target_pool, "windsurf", &target_agent_root).await;
        let target_resource = target_dir.path().join("resource-library");
        db::set_skill_resource_library_dir(&target_pool, &target_resource.to_string_lossy())
            .await
            .expect("target resource dir");
        import_app_backup_impl(&target_pool, &value.to_string())
            .await
            .expect("import");

        let installation = db::get_skill_installations(&target_pool, "legacy-demo")
            .await
            .expect("installations")
            .pop()
            .expect("installation");
        assert_eq!(installation.link_type, "copy");
        assert_eq!(
            installation.installed_path,
            path_to_string(&target_agent_root.join("legacy-demo"))
        );
        assert_ne!(installation.installed_path, "D:\\outside\\dangerous");
        assert_eq!(installation.symlink_target, None);
        assert!(target_agent_root.join("legacy-demo").is_dir());
    }

    #[test]
    fn installation_method_accepts_only_safe_values() {
        assert_eq!(
            validated_install_method(Some("symlink")),
            BackupInstallMethod::Symlink
        );
        assert_eq!(
            validated_install_method(Some("copy")),
            BackupInstallMethod::Copy
        );
        assert_eq!(
            validated_install_method(Some("copycat")),
            BackupInstallMethod::Symlink
        );
        assert_eq!(validated_install_method(None), BackupInstallMethod::Symlink);
    }

    #[tokio::test]
    async fn backup_roundtrip_preserves_grouped_skill_files_and_source() {
        let (pool, dir) = setup_test_db().await;
        let resource_root = dir.path().join("resource-library");
        db::set_skill_resource_library_dir(&pool, &resource_root.to_string_lossy())
            .await
            .expect("resource dir");
        let skill_dir = resource_root.join("openai").join("skills").join("demo");
        std::fs::create_dir_all(&skill_dir).expect("skill dir");
        std::fs::write(skill_dir.join("SKILL.md"), "---\nname: Demo\n---\n").expect("skill");
        std::fs::write(skill_dir.join("asset.bin"), [0, 1, 2, 3]).expect("asset");

        db::upsert_skill(
            &pool,
            &Skill {
                id: "demo".to_string(),
                name: "Demo".to_string(),
                description: None,
                file_path: path_to_string(&skill_dir.join("SKILL.md")),
                canonical_path: Some(path_to_string(&skill_dir)),
                is_central: false,
                source: Some("github:openai/skills".to_string()),
                content: None,
                scanned_at: "2026-01-01T00:00:00Z".to_string(),
            },
        )
        .await
        .expect("skill");
        db::upsert_skill_source(
            &pool,
            &SkillSource {
                skill_id: "demo".to_string(),
                source_type: "github".to_string(),
                source_url: Some(
                    "https://raw.githubusercontent.com/openai/skills/main/demo/SKILL.md"
                        .to_string(),
                ),
                source_author: Some("openai".to_string()),
                source_repo: Some("openai/skills".to_string()),
                source_path: Some("demo".to_string()),
                updated_at: "2026-01-01T00:00:00Z".to_string(),
            },
        )
        .await
        .expect("source");
        db::upsert_skill_metadata(
            &pool,
            "demo",
            Some("Use for imported repository demos."),
            &["repo".to_string(), "demo".to_string()],
        )
        .await
        .expect("metadata");

        db::save_github_stars(&pool, "openai/skills", 1234)
            .await
            .unwrap();
        db::save_github_stars(&pool, "unrelated/repository", 999)
            .await
            .unwrap();
        let json = export_app_backup_impl(&pool, BackupOptions::default())
            .await
            .expect("export");
        let mut serialized: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(serialized["repository_stats"].as_array().unwrap().len(), 1);
        let checked_at = serialized["repository_stats"][0]["checked_at"]
            .as_str()
            .unwrap()
            .to_string();
        serialized
            .as_object_mut()
            .unwrap()
            .remove("repository_stats");
        assert!(serde_json::from_value::<AppBackup>(serialized)
            .unwrap()
            .repository_stats
            .is_empty());
        std::fs::remove_dir_all(&skill_dir).expect("remove original files");
        db::delete_skill(&pool, "demo").await.expect("delete db");

        pool.write(|data| {
            data.table_mut("github_repository_stats").clear();
            Ok(())
        })
        .await
        .unwrap();
        import_app_backup_impl(&pool, &json).await.expect("import");

        assert_eq!(
            db::get_github_stars(&pool, "openai/skills").await.unwrap(),
            Some(1234)
        );
        let restored_time = pool
            .read(|data| {
                Ok(
                    data.table("github_repository_stats")["openai/skills"]["checked_at"]
                        .as_str()
                        .unwrap()
                        .to_string(),
                )
            })
            .unwrap();
        assert_eq!(restored_time, checked_at);
        db::save_github_stars(&pool, "openai/skills", 0)
            .await
            .unwrap();
        let archive = export_app_backup_archive_impl(&pool, repository_backup_options())
            .await
            .unwrap();
        pool.write(|data| {
            data.table_mut("github_repository_stats").clear();
            Ok(())
        })
        .await
        .unwrap();
        import_app_backup_bytes_impl(&pool, &archive).await.unwrap();
        assert_eq!(
            db::get_github_stars(&pool, "openai/skills").await.unwrap(),
            Some(0)
        );
        assert!(skill_dir.join("SKILL.md").exists());
        assert_eq!(
            std::fs::read(skill_dir.join("asset.bin")).unwrap(),
            vec![0, 1, 2, 3]
        );
        let source = db::get_skill_source(&pool, "demo")
            .await
            .expect("source")
            .expect("source row");
        assert_eq!(source.source_author.as_deref(), Some("openai"));
        assert_eq!(source.source_repo.as_deref(), Some("openai/skills"));
        let metadata = db::get_skill_metadata(&pool, "demo")
            .await
            .expect("metadata")
            .expect("metadata row");
        assert_eq!(
            metadata.notes.as_deref(),
            Some("Use for imported repository demos.")
        );
        assert_eq!(
            db::parse_skill_metadata_tags(Some(&metadata)),
            vec!["repo".to_string(), "demo".to_string()]
        );
    }

    #[tokio::test]
    async fn backup_roundtrip_preserves_resource_library_skills() {
        let (pool, dir) = setup_test_db().await;
        let resource_root = dir.path().join("resource-library");
        db::set_skill_resource_library_dir(&pool, &resource_root.to_string_lossy())
            .await
            .expect("resource dir");
        let skill_dir = resource_root
            .join("example")
            .join("skills")
            .join("resource-demo");
        std::fs::create_dir_all(&skill_dir).expect("skill dir");
        std::fs::write(
            skill_dir.join("SKILL.md"),
            "---\nname: Resource Demo\n---\n",
        )
        .expect("skill");

        db::upsert_skill(
            &pool,
            &Skill {
                id: "resource-demo".to_string(),
                name: "Resource Demo".to_string(),
                description: None,
                file_path: path_to_string(&skill_dir.join("SKILL.md")),
                canonical_path: Some(path_to_string(&skill_dir)),
                is_central: false,
                source: Some("github:example/skills".to_string()),
                content: None,
                scanned_at: "2026-01-01T00:00:00Z".to_string(),
            },
        )
        .await
        .expect("skill");

        let json = export_app_backup_impl(&pool, BackupOptions::default())
            .await
            .expect("export");
        assert!(json.contains("\"storage_kind\": \"resource\""));
        std::fs::remove_dir_all(&skill_dir).expect("remove original files");
        db::delete_skill(&pool, "resource-demo")
            .await
            .expect("delete db");

        import_app_backup_impl(&pool, &json).await.expect("import");

        assert!(skill_dir.join("SKILL.md").exists());
        let restored = db::get_skill_by_id(&pool, "resource-demo")
            .await
            .expect("restored query")
            .expect("restored skill");
        assert_eq!(
            restored.canonical_path.as_deref(),
            Some(path_to_string(&skill_dir).as_str())
        );
        assert!(
            !restored.is_central,
            "resource library backups must restore as resource skills"
        );
    }

    #[tokio::test]
    async fn backup_roundtrip_keeps_promoted_skills_in_resource_library() {
        let (pool, dir) = setup_test_db().await;
        let resource_root = dir.path().join("resource-library");
        let central_root = dir.path().join("central");
        db::set_skill_resource_library_dir(&pool, &resource_root.to_string_lossy())
            .await
            .expect("resource dir");
        pool.write(|data| {
            for (id, row) in data.table_mut("agents") {
                if id != "central" {
                    row["is_enabled"] = serde_json::json!(false);
                }
            }
            Ok(())
        })
        .await
        .expect("disable platforms");

        let resource_skill_dir = resource_root
            .join("owner")
            .join("repo")
            .join("promoted-skill");
        std::fs::create_dir_all(&resource_skill_dir).expect("skill dir");
        std::fs::write(
            resource_skill_dir.join("SKILL.md"),
            "---\nname: Promoted Skill\n---\n\nKeep this in the resource library.\n",
        )
        .expect("skill");
        db::upsert_skill(
            &pool,
            &Skill {
                id: "promoted-skill".to_string(),
                name: "Promoted Skill".to_string(),
                description: None,
                file_path: path_to_string(&resource_skill_dir.join("SKILL.md")),
                canonical_path: Some(path_to_string(&resource_skill_dir)),
                is_central: false,
                source: Some("github:owner/repo".to_string()),
                content: None,
                scanned_at: "2026-01-01T00:00:00Z".to_string(),
            },
        )
        .await
        .expect("skill");
        add_resource_skill_to_central_impl(&pool, "promoted-skill")
            .await
            .expect("promote");

        let archive = export_app_backup_archive_impl(&pool, BackupOptions::default())
            .await
            .expect("archive export");
        assert_eq!(
            zip_entry_text(
                &archive,
                "resource-library/owner/repo/promoted-skill/SKILL.md"
            ),
            "---\nname: Promoted Skill\n---\n\nKeep this in the resource library.\n"
        );
        assert!(
            !zip_has_entry(
                &archive,
                "central-library/owner/repo/promoted-skill/SKILL.md"
            ),
            "Central Skills symlink entries should not duplicate resource files in the archive"
        );

        remove_symlink_path(&central_root.join("promoted-skill")).expect("remove central files");
        std::fs::remove_dir_all(&resource_skill_dir).expect("remove resource files");
        db::delete_skill(&pool, "promoted-skill")
            .await
            .expect("delete db");

        import_app_backup_bytes_impl(&pool, &archive)
            .await
            .expect("archive import");

        assert!(resource_skill_dir.join("SKILL.md").exists());
        let restored_central = central_root.join("promoted-skill");
        assert!(
            !restored_central.exists(),
            "complete restore must not recreate Central Skills paths"
        );
        let restored = db::get_skill_by_id(&pool, "promoted-skill")
            .await
            .expect("restored query")
            .expect("restored skill");
        assert!(
            !restored.is_central,
            "restored resource skills must not be marked central"
        );
        let resource_skills = crate::commands::skills::get_resource_library_skills_impl(&pool)
            .await
            .expect("resource listing");
        assert!(
            resource_skills
                .iter()
                .any(|skill| skill.id == "promoted-skill"),
            "promoted skills remain visible in the resource library after restore"
        );
    }

    #[tokio::test]
    async fn complete_backup_omits_central_only_skills() {
        let (pool, dir) = setup_test_db().await;
        let resource_root = dir.path().join("resource-library");
        let central_root = dir.path().join("central");
        db::set_skill_resource_library_dir(&pool, &resource_root.to_string_lossy())
            .await
            .expect("resource dir");
        let central_skill_dir = central_root.join("owner").join("repo").join("central-only");
        std::fs::create_dir_all(&central_skill_dir).expect("central skill dir");
        std::fs::write(
            central_skill_dir.join("SKILL.md"),
            "---\nname: Central Only\n---\n\nSource this from the resource library.\n",
        )
        .expect("skill");
        db::upsert_skill(
            &pool,
            &Skill {
                id: "central-only".to_string(),
                name: "Central Only".to_string(),
                description: None,
                file_path: path_to_string(&central_skill_dir.join("SKILL.md")),
                canonical_path: Some(path_to_string(&central_skill_dir)),
                is_central: true,
                source: Some("github:owner/repo".to_string()),
                content: None,
                scanned_at: "2026-01-01T00:00:00Z".to_string(),
            },
        )
        .await
        .expect("skill");

        let archive = export_app_backup_archive_impl(&pool, BackupOptions::default())
            .await
            .expect("archive export");
        assert!(
            !zip_has_entry(&archive, "central-library/owner/repo/central-only/SKILL.md"),
            "complete backup must not pack Central Skills as a separate archive target"
        );
        let json = export_app_backup_impl(&pool, BackupOptions::default())
            .await
            .expect("json export");
        let backup: AppBackup = serde_json::from_str(&json).expect("backup json");
        assert!(
            !backup
                .skills
                .iter()
                .any(|skill| skill.skill.id == "central-only" || skill.storage_kind == "central"),
            "complete backup must omit central-only skills"
        );
    }

    #[tokio::test]
    async fn legacy_central_backup_restores_into_resource_library_only() {
        let (pool, dir) = setup_test_db().await;
        let resource_root = dir.path().join("resource-library");
        let central_root = dir.path().join("central");
        db::set_skill_resource_library_dir(&pool, &resource_root.to_string_lossy())
            .await
            .expect("resource dir");
        let backup = serde_json::json!({
            "schema_version": BACKUP_SCHEMA_VERSION,
            "exported_at": "2026-01-01T00:00:00Z",
            "included": {"includeResourceLibrary": true, "includeCentralLibrary": true, "includeAppConfig": true, "includeInstallations": true},
            "skills": [{
                "skill": {
                    "id": "central-only",
                    "name": "Central Only",
                    "description": null,
                    "file_path": "",
                    "canonical_path": null,
                    "is_central": true,
                    "source": "github:owner/repo",
                    "content": null,
                    "scanned_at": "2026-01-01T00:00:00Z"
                },
                "storage_kind": "central",
                "relative_dir": "owner/repo/central-only",
                "files": [{"relative_path": "SKILL.md", "content_base64": "LS0tCm5hbWU6IENlbnRyYWwgT25seQotLS0KClNvdXJjZSB0aGlzIGZyb20gdGhlIHJlc291cmNlIGxpYnJhcnkuCg=="}]
            }],
            "collections": [],
            "collection_skills": [],
            "settings": [],
            "agents": [],
            "scan_directories": [],
            "skill_installations": []
        });

        import_app_backup_impl(&pool, &backup.to_string())
            .await
            .expect("import legacy central backup");

        let resource_skill_dir = resource_root
            .join("owner")
            .join("repo")
            .join("central-only");
        assert!(
            resource_skill_dir.join("SKILL.md").exists(),
            "legacy central skills are restored into the resource library"
        );
        assert!(
            !central_root
                .join("owner")
                .join("repo")
                .join("central-only")
                .exists(),
            "legacy central restore must not recreate Central Skills"
        );
        let resource_skills = crate::commands::skills::get_resource_library_skills_impl(&pool)
            .await
            .expect("resource listing");
        assert!(resource_skills
            .iter()
            .any(|skill| skill.id == "central-only"));
        let restored = db::get_skill_by_id(&pool, "central-only")
            .await
            .expect("restored query")
            .expect("restored skill");
        assert!(!restored.is_central);
        assert_eq!(
            restored.canonical_path.as_deref(),
            Some(path_to_string(&resource_skill_dir).as_str())
        );
    }

    #[tokio::test]
    async fn backup_excludes_sensitive_settings_and_import_ignores_them() {
        let (pool, _dir) = setup_test_db().await;
        db::set_setting(&pool, "language", "zh")
            .await
            .expect("language");
        db::set_setting(&pool, "skill_resource_library_dir", "D:\\backup-library")
            .await
            .expect("resource library path");
        db::set_setting(&pool, "github_pat", "should-not-export")
            .await
            .expect("pat");
        db::set_setting(&pool, "ai_api_key", "should-not-export")
            .await
            .expect("api key");
        db::set_setting(&pool, "account_password", "should-not-export")
            .await
            .expect("password");
        db::set_setting(&pool, "service_secret", "should-not-export")
            .await
            .expect("secret");
        db::set_setting(&pool, "webdav_url", "https://user:pass@example.com/dav")
            .await
            .expect("userinfo URL");

        let json = export_app_backup_impl(&pool, BackupOptions::default())
            .await
            .expect("export");
        assert!(json.contains("\"language\""));
        assert!(!json.contains("skill_resource_library_dir"));
        assert!(!json.contains("https://user:pass@example.com/dav"));

        let mut backup: AppBackup = serde_json::from_str(&json).expect("backup");
        backup.settings.push(SettingBackup {
            key: "github_pat".to_string(),
            value: "should-not-import".to_string(),
        });
        let import_json = serde_json::to_string(&backup).expect("json");
        db::set_setting(&pool, "github_pat", "existing")
            .await
            .expect("existing");

        import_app_backup_impl(&pool, &import_json)
            .await
            .expect("import");

        assert_eq!(
            db::get_setting(&pool, "language").await.expect("language"),
            Some("zh".to_string())
        );
        assert_eq!(
            db::get_setting(&pool, "github_pat").await.expect("pat"),
            Some("existing".to_string())
        );
    }

    #[tokio::test]
    async fn backup_excludes_credential_urls_and_sync_errors() {
        let (pool, _dir) = setup_test_db().await;
        let central = central_root(&pool).await.expect("central");
        let skill_dir = central.join("url-safety-demo");
        std::fs::create_dir_all(&skill_dir).expect("skill dir");
        std::fs::write(skill_dir.join("SKILL.md"), "---\nname: URL Safety\n---\n").expect("skill");
        db::upsert_skill(
            &pool,
            &Skill {
                id: "url-safety-demo".to_string(),
                name: "URL Safety".to_string(),
                description: None,
                file_path: path_to_string(&skill_dir.join("SKILL.md")),
                canonical_path: Some(path_to_string(&skill_dir)),
                is_central: true,
                source: None,
                content: None,
                scanned_at: "2026-01-01T00:00:00Z".to_string(),
            },
        )
        .await
        .expect("skill");
        db::upsert_skill_source(
            &pool,
            &SkillSource {
                skill_id: "url-safety-demo".to_string(),
                source_type: "github".to_string(),
                source_url: Some("https://token:secret@example.com/skill.md".to_string()),
                source_author: Some("example".to_string()),
                source_repo: Some("example/repo".to_string()),
                source_path: Some(r"C:\private\skill".to_string()),
                updated_at: "2026-01-01T00:00:00Z".to_string(),
            },
        )
        .await
        .expect("source");

        let json = export_app_backup_impl(&pool, BackupOptions::default())
            .await
            .expect("export");

        assert!(!json.contains("https://token:secret@example.com"));
        assert!(!json.contains("token=secret"));
        assert!(!json.contains("C:\\\\private"));
    }

    #[tokio::test]
    async fn backup_and_import_drop_local_skill_origin_paths() {
        let (pool, _dir) = setup_test_db().await;
        let central = central_root(&pool).await.expect("central");
        let skill_dir = central.join("local-origin-demo");
        std::fs::create_dir_all(&skill_dir).expect("skill dir");
        std::fs::write(skill_dir.join("SKILL.md"), "---\nname: Local Origin\n---\n")
            .expect("skill");
        db::upsert_skill(
            &pool,
            &Skill {
                id: "local-origin-demo".to_string(),
                name: "Local Origin".to_string(),
                description: None,
                file_path: path_to_string(&skill_dir.join("SKILL.md")),
                canonical_path: Some(path_to_string(&skill_dir)),
                is_central: true,
                source: Some(r"C:\Users\secret\skill".to_string()),
                content: None,
                scanned_at: "2026-01-01T00:00:00Z".to_string(),
            },
        )
        .await
        .expect("skill");

        let json = export_app_backup_impl(&pool, BackupOptions::default())
            .await
            .expect("export");
        assert!(!json.contains(r"C:\Users\secret"));

        let backup = serde_json::json!({
            "schema_version": BACKUP_SCHEMA_VERSION,
            "exported_at": "2026-01-01T00:00:00Z",
            "included": {"includeResourceLibrary": true, "includeCentralLibrary": true, "includeAppConfig": true, "includeInstallations": true},
            "skills": [{
                "skill": {
                    "id": "import-local-origin",
                    "name": "Import Local Origin",
                    "description": null,
                    "file_path": "",
                    "canonical_path": null,
                    "is_central": true,
                    "source": "/Users/secret/skill",
                    "content": null,
                    "scanned_at": "2026-01-01T00:00:00Z"
                },
                "storage_kind": "central",
                "relative_dir": "import-local-origin",
                "files": [{"relative_path": "SKILL.md", "content_base64": "LS0tCm5hbWU6IERlbW8KLS0tCg=="}]
            }],
            "collections": [],
            "collection_skills": [],
            "settings": [],
            "agents": [],
            "scan_directories": [],
            "skill_installations": []
        });
        import_app_backup_impl(&pool, &backup.to_string())
            .await
            .expect("import");
        let restored = db::get_skill_by_id(&pool, "import-local-origin")
            .await
            .expect("skill")
            .expect("skill row");
        assert!(!restored.is_central);
        assert_ne!(restored.source.as_deref(), Some("/Users/secret/skill"));
        assert_eq!(restored.source.as_deref(), Some("resource-library"));
    }

    #[tokio::test]
    async fn import_clears_stale_skill_source_when_backup_omits_source() {
        let (pool, _dir) = setup_test_db().await;
        let central = central_root(&pool).await.expect("central");
        let skill_dir = central.join("stale-source-demo");
        std::fs::create_dir_all(&skill_dir).expect("skill dir");
        std::fs::write(skill_dir.join("SKILL.md"), "---\nname: Stale Source\n---\n")
            .expect("skill");
        db::upsert_skill(
            &pool,
            &Skill {
                id: "stale-source-demo".to_string(),
                name: "Stale Source".to_string(),
                description: None,
                file_path: path_to_string(&skill_dir.join("SKILL.md")),
                canonical_path: Some(path_to_string(&skill_dir)),
                is_central: true,
                source: Some("github:old/repo".to_string()),
                content: None,
                scanned_at: "2026-01-01T00:00:00Z".to_string(),
            },
        )
        .await
        .expect("skill");
        db::upsert_skill_source(
            &pool,
            &SkillSource {
                skill_id: "stale-source-demo".to_string(),
                source_type: "github".to_string(),
                source_url: Some("https://example.com/old/SKILL.md".to_string()),
                source_author: Some("old".to_string()),
                source_repo: Some("old/repo".to_string()),
                source_path: Some("old/SKILL.md".to_string()),
                updated_at: "2026-01-01T00:00:00Z".to_string(),
            },
        )
        .await
        .expect("source");
        let backup = serde_json::json!({
            "schema_version": BACKUP_SCHEMA_VERSION,
            "exported_at": "2026-01-01T00:00:00Z",
            "included": {"includeResourceLibrary": true, "includeCentralLibrary": true, "includeAppConfig": true, "includeInstallations": true},
            "skills": [{
                "skill": {
                    "id": "stale-source-demo",
                    "name": "Stale Source",
                    "description": null,
                    "file_path": "",
                    "canonical_path": null,
                    "is_central": true,
                    "source": null,
                    "content": null,
                    "scanned_at": "2026-01-01T00:00:00Z"
                },
                "source": null,
                "metadata": null,
                "storage_kind": "central",
                "relative_dir": "stale-source-demo",
                "files": [{"relative_path": "SKILL.md", "content_base64": "LS0tCm5hbWU6IFN0YWxlIFNvdXJjZQotLS0K"}]
            }],
            "collections": [],
            "collection_skills": [],
            "settings": [],
            "agents": [],
            "scan_directories": [],
            "skill_installations": []
        });

        import_app_backup_impl(&pool, &backup.to_string())
            .await
            .expect("import");

        assert!(db::get_skill_source(&pool, "stale-source-demo")
            .await
            .expect("source lookup")
            .is_none());
    }

    #[tokio::test]
    async fn backup_options_resource_only_excludes_central_and_app_config() {
        let (pool, dir) = setup_test_db().await;
        db::set_setting(&pool, "language", "zh")
            .await
            .expect("setting");

        let central = central_root(&pool).await.expect("central");
        let central_skill_dir = central.join("central-demo");
        std::fs::create_dir_all(&central_skill_dir).expect("central skill dir");
        std::fs::write(
            central_skill_dir.join("SKILL.md"),
            "---\nname: Central Demo\n---\n",
        )
        .expect("central skill");
        db::upsert_skill(
            &pool,
            &Skill {
                id: "central-demo".to_string(),
                name: "Central Demo".to_string(),
                description: None,
                file_path: path_to_string(&central_skill_dir.join("SKILL.md")),
                canonical_path: Some(path_to_string(&central_skill_dir)),
                is_central: true,
                source: None,
                content: None,
                scanned_at: "2026-01-01T00:00:00Z".to_string(),
            },
        )
        .await
        .expect("central db skill");

        let resource_root = dir.path().join("resource-library");
        db::set_skill_resource_library_dir(&pool, &resource_root.to_string_lossy())
            .await
            .expect("resource dir");
        let resource_skill_dir = resource_root.join("resource-demo");
        std::fs::create_dir_all(&resource_skill_dir).expect("resource skill dir");
        std::fs::write(
            resource_skill_dir.join("SKILL.md"),
            "---\nname: Resource Demo\n---\n",
        )
        .expect("resource skill");
        db::upsert_skill(
            &pool,
            &Skill {
                id: "resource-demo".to_string(),
                name: "Resource Demo".to_string(),
                description: None,
                file_path: path_to_string(&resource_skill_dir.join("SKILL.md")),
                canonical_path: Some(path_to_string(&resource_skill_dir)),
                is_central: false,
                source: None,
                content: None,
                scanned_at: "2026-01-01T00:00:00Z".to_string(),
            },
        )
        .await
        .expect("resource db skill");

        let json = export_app_backup_impl(
            &pool,
            BackupOptions {
                include_resource_library: true,
                include_central_library: false,
                include_app_config: false,
                include_installations: false,
            },
        )
        .await
        .expect("export");
        let backup: AppBackup = serde_json::from_str(&json).expect("backup json");

        assert!(backup
            .skills
            .iter()
            .any(|skill| skill.skill.id == "resource-demo"));
        assert!(!backup
            .skills
            .iter()
            .any(|skill| skill.skill.id == "central-demo"));
        assert!(backup.settings.is_empty());
        assert!(backup.agents.is_empty());
        assert!(backup.collections.is_empty());
        assert!(backup.collection_skills.is_empty());
    }

    #[tokio::test]
    async fn backup_options_central_only_excludes_resource_skills() {
        let (pool, dir) = setup_test_db().await;
        let central = central_root(&pool).await.expect("central");
        let central_skill_dir = central.join("central-only-demo");
        std::fs::create_dir_all(&central_skill_dir).expect("central skill dir");
        std::fs::write(
            central_skill_dir.join("SKILL.md"),
            "---\nname: Central Only Demo\n---\n",
        )
        .expect("central skill");
        db::upsert_skill(
            &pool,
            &Skill {
                id: "central-only-demo".to_string(),
                name: "Central Only Demo".to_string(),
                description: None,
                file_path: path_to_string(&central_skill_dir.join("SKILL.md")),
                canonical_path: Some(path_to_string(&central_skill_dir)),
                is_central: true,
                source: None,
                content: None,
                scanned_at: "2026-01-01T00:00:00Z".to_string(),
            },
        )
        .await
        .expect("central db skill");

        let resource_root = dir.path().join("resource-library");
        db::set_skill_resource_library_dir(&pool, &resource_root.to_string_lossy())
            .await
            .expect("resource dir");
        let resource_skill_dir = resource_root.join("resource-only-demo");
        std::fs::create_dir_all(&resource_skill_dir).expect("resource skill dir");
        std::fs::write(
            resource_skill_dir.join("SKILL.md"),
            "---\nname: Resource Only Demo\n---\n",
        )
        .expect("resource skill");
        db::upsert_skill(
            &pool,
            &Skill {
                id: "resource-only-demo".to_string(),
                name: "Resource Only Demo".to_string(),
                description: None,
                file_path: path_to_string(&resource_skill_dir.join("SKILL.md")),
                canonical_path: Some(path_to_string(&resource_skill_dir)),
                is_central: false,
                source: None,
                content: None,
                scanned_at: "2026-01-01T00:00:00Z".to_string(),
            },
        )
        .await
        .expect("resource db skill");

        let json = export_app_backup_impl(
            &pool,
            BackupOptions {
                include_resource_library: false,
                include_central_library: true,
                include_app_config: false,
                include_installations: false,
            },
        )
        .await
        .expect("export");
        let backup: AppBackup = serde_json::from_str(&json).expect("backup json");

        assert!(backup
            .skills
            .iter()
            .any(|skill| skill.skill.id == "central-only-demo"));
        assert!(!backup
            .skills
            .iter()
            .any(|skill| skill.skill.id == "resource-only-demo"));
    }

    #[test]
    fn webdav_normalize_remote_path_rejects_traversal() {
        let result = normalize_webdav_remote_path("../secret.json");
        assert!(result.is_err());
    }

    #[test]
    fn webdav_normalize_remote_path_rejects_absolute_and_unc_like_values() {
        for value in [
            "/backups/skillshub-backup.json",
            r"\server\share\backup.json",
            r"C:\backups\skillshub-backup.json",
            "C:/backups/skillshub-backup.json",
            r"safe\..\secret.json",
        ] {
            assert!(
                normalize_webdav_remote_path(value).is_err(),
                "absolute-looking path accepted: {value}"
            );
        }
    }

    #[test]
    fn generated_backup_filenames_use_compact_timestamp_and_zip_suffix() {
        let first = generated_backup_filename();
        assert_eq!(first.len(), "skillshub-backup-".len() + 14 + ".zip".len());
        assert!(first.starts_with("skillshub-backup-"));
        assert!(first.ends_with(".zip"));
        assert!(generated_backup_timestamp(&first).is_some());
    }

    #[test]
    fn generated_backup_filename_uses_local_clock() {
        let name = generated_backup_filename();
        let parsed = generated_backup_timestamp(&name).expect("filename timestamp");
        let now = Local::now().naive_local();
        let diff = (now - parsed).num_seconds().abs();
        assert!(
            diff <= 2,
            "filename {name} is {diff}s away from local clock {now}"
        );
    }

    #[test]
    fn write_backup_archive_to_path_creates_the_zip() {
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join("out").join("backup.zip");
        write_backup_archive_to_path(&path.to_string_lossy(), b"PK\x03\x04").expect("write backup");
        assert_eq!(std::fs::read(&path).expect("read backup"), b"PK\x03\x04");
    }

    #[test]
    fn write_backup_archive_to_path_rejects_empty_path() {
        let error = write_backup_archive_to_path("   ", b"PK\x03\x04").expect_err("empty path");
        assert!(error.contains("empty"));
    }

    #[test]
    fn webdav_normalize_remote_path_accepts_nested_zip_backup() {
        let result =
            normalize_webdav_remote_path("backups/skillshub-backup.zip").expect("normalized path");
        assert_eq!(result, "backups/skillshub-backup.zip");
    }

    #[test]
    fn generated_backup_timestamp_accepts_legacy_json_suffix() {
        let timestamp = generated_backup_timestamp("skillshub-backup-2026-07-15-120000-old.json");
        assert!(timestamp.is_some());
    }

    #[test]
    fn webdav_normalize_base_url_rejects_non_http_urls() {
        let result = normalize_webdav_base_url("file:///tmp/backups");
        assert!(result.is_err());
    }

    #[test]
    fn webdav_directory_url_uses_trailing_slash() {
        let config = WebDavConfig {
            base_url: "https://example.com/dav".to_string(),
            username: None,
            password: None,
            remote_dir: "skillshub".to_string(),
        };
        let url = build_webdav_directory_url(&config).expect("directory url");
        assert_eq!(url, "https://example.com/dav/skillshub/");
    }

    #[test]
    fn webdav_normalize_base_url_trims_trailing_slash() {
        let result = normalize_webdav_base_url("https://example.com/dav/").expect("normalized url");
        assert_eq!(result, "https://example.com/dav");
    }

    #[test]
    fn webdav_normalize_base_url_rejects_query_fragment_and_userinfo() {
        for value in [
            "https://example.com/dav?scope=backups",
            "https://example.com/dav#backups",
            "https://user@example.com/dav",
        ] {
            let error = normalize_webdav_base_url(value).expect_err("unsafe URL accepted");
            assert!(!error.contains("example.com"));
        }
    }

    #[test]
    fn webdav_build_url_encodes_each_path_segment() {
        let config = WebDavConfig {
            base_url: "https://example.com/dav".to_string(),
            username: None,
            password: None,
            remote_dir: "nested folder".to_string(),
        };

        let url =
            build_webdav_url(&config, "backup name%2e%2e%3Fx%23frag.json").expect("WebDAV URL");
        assert!(url.contains("/dav/nested%20folder/"));
        assert!(url.contains("backup%20name%252e%252e%253Fx%2523frag.json"));
        let parsed = Url::parse(&url).expect("encoded URL");
        assert_eq!(parsed.query(), None);
        assert_eq!(parsed.fragment(), None);
        assert_eq!(
            parsed
                .path_segments()
                .expect("path segments")
                .collect::<Vec<_>>(),
            vec![
                "dav",
                "nested%20folder",
                "backup%20name%252e%252e%253Fx%2523frag.json"
            ]
        );
    }

    #[test]
    fn webdav_build_url_keeps_encoded_delimiters_inside_remote_dir() {
        let config = WebDavConfig {
            base_url: "https://example.com/dav".to_string(),
            username: None,
            password: None,
            remote_dir: "safe".to_string(),
        };

        let url = build_webdav_url(&config, "%2e%2e/%2f/backup.json").expect("WebDAV URL");
        assert_eq!(
            url,
            "https://example.com/dav/safe/%252e%252e/%252f/backup.json"
        );
    }

    #[test]
    fn webdav_parse_namespaced_propfind_xml() {
        let xml = r#"
            <d:multistatus xmlns:d="DAV:">
              <d:response>
                <d:href>/dav/backups/</d:href>
                <d:propstat><d:prop><d:resourcetype><d:collection/></d:resourcetype></d:prop></d:propstat>
              </d:response>
              <d:response>
                <d:href>/dav/backups/skillshub-backup.json</d:href>
                <d:propstat>
                  <d:prop>
                    <d:getcontentlength>42</d:getcontentlength>
                    <d:getlastmodified>Wed, 15 Jul 2026 08:00:00 GMT</d:getlastmodified>
                  </d:prop>
                  <d:status>HTTP/1.1 200 OK</d:status>
                </d:propstat>
              </d:response>
              <d:response><d:href>/dav/backups/readme.txt</d:href></d:response>
            </d:multistatus>
        "#;

        let files = parse_webdav_backup_files(xml).expect("PROPFIND XML");
        assert_eq!(
            files,
            vec![WebDavBackupFile {
                name: "skillshub-backup.json".to_string(),
                remote_path: "skillshub-backup.json".to_string(),
                size: Some(42),
                modified_at: Some("2026-07-15T08:00:00+00:00".to_string()),
            }]
        );
    }

    #[test]
    fn webdav_parse_decodes_one_href_filename_segment() {
        let xml = r#"
            <d:multistatus xmlns:d="DAV:">
              <d:response>
                <d:href>/dav/backups/skillshub-backup%20copy.json</d:href>
              </d:response>
            </d:multistatus>
        "#;

        let files = parse_webdav_backup_files(xml).expect("PROPFIND XML");

        assert_eq!(files[0].name, "skillshub-backup copy.json");
        assert_eq!(files[0].remote_path, "skillshub-backup copy.json");
        let url = build_webdav_url(
            &WebDavConfig {
                base_url: "https://example.com/dav".to_string(),
                username: None,
                password: None,
                remote_dir: "backups".to_string(),
            },
            &files[0].remote_path,
        )
        .expect("WebDAV URL");
        assert!(url.ends_with("/dav/backups/skillshub-backup%20copy.json"));
    }

    #[test]
    fn webdav_parse_rejects_decoded_href_separators() {
        let xml = r#"
            <d:multistatus xmlns:d="DAV:">
              <d:response>
                <d:href>/dav/backups/skillshub-backup%2Fnested.json</d:href>
              </d:response>
            </d:multistatus>
        "#;

        assert!(parse_webdav_backup_files(xml).is_err());
    }

    #[test]
    fn webdav_parse_sorts_valid_http_dates_newest_first() {
        let xml = r#"
            <d:multistatus xmlns:d="DAV:">
              <d:response>
                <d:href>/dav/backups/skillshub-backup-2026-07-15-100000.json</d:href>
                <d:propstat><d:prop><d:getlastmodified>Wed, 15 Jul 2026 10:00:00 GMT</d:getlastmodified></d:prop></d:propstat>
              </d:response>
              <d:response>
                <d:href>/dav/backups/skillshub-backup-2026-07-16-090000.json</d:href>
                <d:propstat><d:prop><d:getlastmodified>Thu, 16 Jul 2026 09:00:00 GMT</d:getlastmodified></d:prop></d:propstat>
              </d:response>
            </d:multistatus>
        "#;

        let files = parse_webdav_backup_files(xml).expect("PROPFIND XML");

        assert_eq!(
            files
                .iter()
                .map(|file| file.name.as_str())
                .collect::<Vec<_>>(),
            vec![
                "skillshub-backup-2026-07-16-090000.json",
                "skillshub-backup-2026-07-15-100000.json",
            ]
        );
    }

    #[test]
    fn webdav_parse_sorts_missing_or_invalid_dates_by_filename_timestamp() {
        let xml = r#"
            <d:multistatus xmlns:d="DAV:">
              <d:response>
                <d:href>/dav/backups/skillshub-backup-2026-01-01-010000.json</d:href>
              </d:response>
              <d:response>
                <d:href>/dav/backups/skillshub-backup-2026-07-01-010000.json</d:href>
                <d:propstat><d:prop><d:getlastmodified>not a date</d:getlastmodified></d:prop></d:propstat>
              </d:response>
              <d:response>
                <d:href>/dav/backups/skillshub-backup-2026-06-01-010000.json</d:href>
              </d:response>
              <d:response>
                <d:href>/dav/backups/skillshub-backup-2026-08-01-010000.json</d:href>
                <d:propstat><d:prop><d:getlastmodified>Thu, 01 Jan 2026 00:00:00 GMT</d:getlastmodified></d:prop></d:propstat>
              </d:response>
            </d:multistatus>
        "#;

        let files = parse_webdav_backup_files(xml).expect("PROPFIND XML");

        assert_eq!(
            files
                .iter()
                .map(|file| file.name.as_str())
                .collect::<Vec<_>>(),
            vec![
                "skillshub-backup-2026-08-01-010000.json",
                "skillshub-backup-2026-07-01-010000.json",
                "skillshub-backup-2026-06-01-010000.json",
                "skillshub-backup-2026-01-01-010000.json",
            ]
        );
    }

    #[test]
    fn webdav_parse_sorts_by_newest_available_http_or_filename_timestamp() {
        let xml = r#"
            <d:multistatus xmlns:d="DAV:">
              <d:response>
                <d:href>/dav/backups/skillshub-backup-2026-07-17-090000.json</d:href>
                <d:propstat><d:prop><d:getlastmodified>Wed, 15 Jul 2026 08:00:00 GMT</d:getlastmodified></d:prop></d:propstat>
              </d:response>
              <d:response>
                <d:href>/dav/backups/skillshub-backup-2026-07-15-100000.json</d:href>
                <d:propstat><d:prop><d:getlastmodified>Thu, 16 Jul 2026 09:00:00 GMT</d:getlastmodified></d:prop></d:propstat>
              </d:response>
            </d:multistatus>
        "#;

        let files = parse_webdav_backup_files(xml).expect("PROPFIND XML");
        assert_eq!(
            files.first().map(|file| file.name.as_str()),
            Some("skillshub-backup-2026-07-17-090000.json")
        );
    }

    #[tokio::test]
    async fn export_omits_local_filesystem_paths() {
        let (pool, dir) = setup_test_db().await;
        let private_root = dir.path().join("private-export-root");
        let central = central_root(&pool).await.expect("central");
        let skill_dir = central.join("portable-demo");
        std::fs::create_dir_all(&skill_dir).expect("skill dir");
        std::fs::write(skill_dir.join("SKILL.md"), "---\nname: Portable\n---\n").expect("skill");
        db::upsert_skill(
            &pool,
            &Skill {
                id: "portable-demo".to_string(),
                name: "Portable".to_string(),
                description: None,
                file_path: path_to_string(&private_root.join("file-path").join("SKILL.md")),
                canonical_path: Some(path_to_string(&skill_dir)),
                is_central: true,
                source: None,
                content: None,
                scanned_at: "2026-01-01T00:00:00Z".to_string(),
            },
        )
        .await
        .expect("skill");
        db::set_setting(
            &pool,
            "skill_resource_library_dir",
            &path_to_string(&private_root),
        )
        .await
        .expect("resource setting");

        let json = export_app_backup_impl(&pool, BackupOptions::default())
            .await
            .expect("export");

        assert!(!json.contains(&path_to_string(&private_root)));
        assert!(!json.contains("central_root"));
        assert!(!json.contains("global_skills_dir"));
    }

    #[tokio::test]
    async fn import_uses_current_resource_root_and_ignores_backup_root() {
        let (pool, dir) = setup_test_db().await;
        let current_root = dir.path().join("current-resource-root");
        let malicious_root = dir.path().join("malicious-resource-root");
        db::set_skill_resource_library_dir(&pool, &current_root.to_string_lossy())
            .await
            .expect("current resource root");
        let backup = serde_json::json!({
            "schema_version": BACKUP_SCHEMA_VERSION,
            "exported_at": "2026-01-01T00:00:00Z",
            "central_root": malicious_root.join("central").to_string_lossy(),
            "included": {"includeResourceLibrary": true, "includeCentralLibrary": true, "includeAppConfig": true, "includeInstallations": true},
            "collections": [],
            "collection_skills": [],
            "settings": [{"key": "skill_resource_library_dir", "value": malicious_root.to_string_lossy()}],
            "agents": [],
            "scan_directories": [],
            "skills": [{
                "skill": {
                    "id": "resource-root-demo",
                    "name": "Resource Root Demo",
                    "description": null,
                    "file_path": malicious_root.join("file").to_string_lossy(),
                    "canonical_path": malicious_root.to_string_lossy(),
                    "is_central": false,
                    "source": null,
                    "content": null,
                    "scanned_at": "2026-01-01T00:00:00Z"
                },
                "storage_kind": "resource",
                "relative_dir": "resource-root-demo",
                "files": [{"relative_path": "SKILL.md", "content_base64": "LS0tCm5hbWU6IERlbW8KLS0tCg=="}]
            }]
        });

        import_app_backup_impl(&pool, &backup.to_string())
            .await
            .expect("import");

        assert!(current_root.join("resource-root-demo/SKILL.md").exists());
        assert!(!malicious_root.exists());
        assert_eq!(
            db::get_skill_resource_library_dir(&pool)
                .await
                .expect("resource root"),
            current_root
        );
    }

    #[tokio::test]
    async fn import_does_not_create_custom_agents_or_use_backup_paths() {
        let (pool, dir) = setup_test_db().await;
        let malicious_root = dir.path().join("malicious-agent-root");
        let backup = serde_json::json!({
            "schema_version": BACKUP_SCHEMA_VERSION,
            "exported_at": "2026-01-01T00:00:00Z",
            "included": {"includeResourceLibrary": true, "includeCentralLibrary": true, "includeAppConfig": true, "includeInstallations": true},
            "skills": [],
            "collections": [],
            "collection_skills": [],
            "settings": [],
            "agents": [{
                "id": "backup-custom-agent",
                "display_name": "Backup Agent",
                "global_skills_dir": malicious_root.to_string_lossy(),
                "project_skills_dir": malicious_root.to_string_lossy(),
                "is_detected": true,
                "is_builtin": false,
                "is_enabled": true
            }],
            "scan_directories": [],
            "skill_installations": [{"skill_id": "missing-skill", "agent_id": "backup-custom-agent", "method": "copy"}]
        });

        import_app_backup_impl(&pool, &backup.to_string())
            .await
            .expect("missing custom agent is skipped");

        assert!(db::get_agent_by_id(&pool, "backup-custom-agent")
            .await
            .expect("agent lookup")
            .is_none());
        assert!(!malicious_root.exists());
    }

    #[tokio::test]
    async fn import_skips_installations_with_missing_dependencies() {
        let (pool, _dir) = setup_test_db().await;
        let backup = serde_json::json!({
            "schema_version": BACKUP_SCHEMA_VERSION,
            "exported_at": "2026-01-01T00:00:00Z",
            "included": {"includeResourceLibrary": true, "includeCentralLibrary": true, "includeAppConfig": true, "includeInstallations": true},
            "skills": [],
            "collections": [],
            "collection_skills": [],
            "settings": [],
            "agents": [],
            "scan_directories": [],
            "skill_installations": [
                {"skill_id": "missing-skill", "agent_id": "claude-code", "method": "copy"},
                {"skill_id": "missing-skill", "agent_id": "missing-agent", "method": "copy"}
            ]
        });

        import_app_backup_impl(&pool, &backup.to_string())
            .await
            .expect("missing installation dependencies are skipped");
        assert!(db::get_skill_installations(&pool, "missing-skill")
            .await
            .expect("installations")
            .is_empty());
    }

    #[tokio::test]
    async fn import_skips_installations_that_fail_to_replay() {
        let (pool, dir) = setup_test_db().await;
        let blocked_agent_root = dir.path().join("blocked-agent-root");
        std::fs::write(&blocked_agent_root, "not a directory").expect("blocked root");
        db::insert_custom_agent(
            &pool,
            &Agent {
                id: "blocked-agent".to_string(),
                display_name: "Blocked Agent".to_string(),
                category: "platform".to_string(),
                global_skills_dir: path_to_string(&blocked_agent_root),
                project_skills_dir: None,
                is_detected: true,
                is_builtin: false,
                is_enabled: true,
            },
        )
        .await
        .expect("agent");
        let backup = serde_json::json!({
            "schema_version": BACKUP_SCHEMA_VERSION,
            "exported_at": "2026-01-01T00:00:00Z",
            "included": {"includeResourceLibrary": true, "includeCentralLibrary": true, "includeAppConfig": true, "includeInstallations": true},
            "skills": [{
                "skill": {
                    "id": "skip-install-failure",
                    "name": "Skip Install Failure",
                    "description": null,
                    "file_path": "",
                    "canonical_path": null,
                    "is_central": true,
                    "source": null,
                    "content": null,
                    "scanned_at": "2026-01-01T00:00:00Z"
                },
                "storage_kind": "central",
                "relative_dir": "skip-install-failure",
                "files": [{"relative_path": "SKILL.md", "content_base64": "LS0tCm5hbWU6IFNraXAgSW5zdGFsbCBGYWlsdXJlCi0tLQo="}]
            }],
            "collections": [],
            "collection_skills": [],
            "settings": [],
            "agents": [],
            "scan_directories": [],
            "skill_installations": [{"skill_id": "skip-install-failure", "agent_id": "blocked-agent", "method": "copy"}]
        });

        import_app_backup_impl(&pool, &backup.to_string())
            .await
            .expect("installation replay failure is skipped");
        assert!(db::get_skill_by_id(&pool, "skip-install-failure")
            .await
            .expect("skill")
            .is_some());
        assert!(db::get_skill_installations(&pool, "skip-install-failure")
            .await
            .expect("installations")
            .is_empty());
    }

    #[tokio::test]
    async fn import_validates_files_before_replacing_existing_skill_directory() {
        let (pool, _dir) = setup_test_db().await;
        let central = central_root(&pool).await.expect("central");
        let skill_dir = central.join("invalid-content-demo");
        std::fs::create_dir_all(&skill_dir).expect("skill dir");
        std::fs::write(skill_dir.join("SKILL.md"), "---\nname: Existing\n---\n")
            .expect("existing skill");
        db::upsert_skill(
            &pool,
            &Skill {
                id: "invalid-content-demo".to_string(),
                name: "Existing".to_string(),
                description: None,
                file_path: path_to_string(&skill_dir.join("SKILL.md")),
                canonical_path: Some(path_to_string(&skill_dir)),
                is_central: true,
                source: None,
                content: None,
                scanned_at: "2026-01-01T00:00:00Z".to_string(),
            },
        )
        .await
        .expect("skill");
        let backup = serde_json::json!({
            "schema_version": BACKUP_SCHEMA_VERSION,
            "exported_at": "2026-01-01T00:00:00Z",
            "included": {"includeResourceLibrary": true, "includeCentralLibrary": true, "includeAppConfig": true, "includeInstallations": true},
            "skills": [{
                "skill": {
                    "id": "invalid-content-demo",
                    "name": "Invalid Content Demo",
                    "description": null,
                    "file_path": "",
                    "canonical_path": null,
                    "is_central": true,
                    "source": null,
                    "content": null,
                    "scanned_at": "2026-01-01T00:00:00Z"
                },
                "storage_kind": "central",
                "relative_dir": "invalid-content-demo",
                "files": [{"relative_path": "SKILL.md", "content_base64": "not base64"}]
            }],
            "collections": [],
            "collection_skills": [],
            "settings": [],
            "agents": [],
            "scan_directories": [],
            "skill_installations": []
        });

        let error = import_app_backup_impl(&pool, &backup.to_string())
            .await
            .expect_err("invalid backup accepted");
        assert!(error.contains("Invalid base64 content"));
        assert_eq!(
            std::fs::read_to_string(skill_dir.join("SKILL.md")).expect("existing skill content"),
            "---\nname: Existing\n---\n"
        );
    }

    #[tokio::test]
    async fn import_rejects_invalid_skill_md_frontmatter_before_replacing_existing_skill_directory()
    {
        let (pool, _dir) = setup_test_db().await;
        let central = central_root(&pool).await.expect("central");
        let skill_dir = central.join("invalid-frontmatter-demo");
        std::fs::create_dir_all(&skill_dir).expect("skill dir");
        std::fs::write(skill_dir.join("SKILL.md"), "---\nname: Existing\n---\n")
            .expect("existing skill");
        let backup = serde_json::json!({
            "schema_version": BACKUP_SCHEMA_VERSION,
            "exported_at": "2026-01-01T00:00:00Z",
            "included": {"includeResourceLibrary": true, "includeCentralLibrary": true, "includeAppConfig": true, "includeInstallations": true},
            "skills": [{
                "skill": {
                    "id": "invalid-frontmatter-demo",
                    "name": "Invalid Frontmatter Demo",
                    "description": null,
                    "file_path": "",
                    "canonical_path": null,
                    "is_central": true,
                    "source": null,
                    "content": null,
                    "scanned_at": "2026-01-01T00:00:00Z"
                },
                "storage_kind": "central",
                "relative_dir": "invalid-frontmatter-demo",
                "files": [{"relative_path": "SKILL.md", "content_base64": "IyBub3Bl"}]
            }],
            "collections": [],
            "collection_skills": [],
            "settings": [],
            "agents": [],
            "scan_directories": [],
            "skill_installations": []
        });

        let error = import_app_backup_impl(&pool, &backup.to_string())
            .await
            .expect_err("invalid SKILL.md accepted");
        assert!(error.contains("valid frontmatter"));
        assert_eq!(
            std::fs::read_to_string(skill_dir.join("SKILL.md")).expect("existing skill content"),
            "---\nname: Existing\n---\n"
        );
    }

    #[tokio::test]
    async fn import_rejects_existing_non_directory_target_without_replacing_it() {
        let (pool, dir) = setup_test_db().await;
        let resource_root = dir.path().join("resource-library");
        db::set_skill_resource_library_dir(&pool, &resource_root.to_string_lossy())
            .await
            .expect("resource dir");
        let target_file = resource_root.join("file-collision-demo");
        std::fs::write(&target_file, "keep me").expect("target file");
        let backup = serde_json::json!({
            "schema_version": BACKUP_SCHEMA_VERSION,
            "exported_at": "2026-01-01T00:00:00Z",
            "included": {"includeResourceLibrary": true, "includeCentralLibrary": true, "includeAppConfig": true, "includeInstallations": true},
            "skills": [{
                "skill": {
                    "id": "file-collision-demo",
                    "name": "File Collision Demo",
                    "description": null,
                    "file_path": "",
                    "canonical_path": null,
                    "is_central": true,
                    "source": null,
                    "content": null,
                    "scanned_at": "2026-01-01T00:00:00Z"
                },
                "storage_kind": "central",
                "relative_dir": "file-collision-demo",
                "files": [{"relative_path": "SKILL.md", "content_base64": "LS0tCm5hbWU6IEZpbGUgQ29sbGlzaW9uIERlbW8KLS0tCg=="}]
            }],
            "collections": [],
            "collection_skills": [],
            "settings": [],
            "agents": [],
            "scan_directories": [],
            "skill_installations": []
        });

        let error = import_app_backup_impl(&pool, &backup.to_string())
            .await
            .expect_err("existing file target accepted");
        assert!(error.contains("not a replaceable skill directory"));
        assert_eq!(
            std::fs::read_to_string(&target_file).expect("target file"),
            "keep me"
        );
    }

    #[tokio::test]
    async fn import_rejects_conflicting_file_tree_before_replacing_existing_skill_directory() {
        let (pool, _dir) = setup_test_db().await;
        let central = central_root(&pool).await.expect("central");
        let skill_dir = central.join("conflict-demo");
        std::fs::create_dir_all(&skill_dir).expect("skill dir");
        std::fs::write(skill_dir.join("SKILL.md"), "---\nname: Existing\n---\n")
            .expect("existing skill");
        let backup = serde_json::json!({
            "schema_version": BACKUP_SCHEMA_VERSION,
            "exported_at": "2026-01-01T00:00:00Z",
            "included": {"includeResourceLibrary": true, "includeCentralLibrary": true, "includeAppConfig": true, "includeInstallations": true},
            "skills": [{
                "skill": {
                    "id": "conflict-demo",
                    "name": "Conflict Demo",
                    "description": null,
                    "file_path": "",
                    "canonical_path": null,
                    "is_central": true,
                    "source": null,
                    "content": null,
                    "scanned_at": "2026-01-01T00:00:00Z"
                },
                "storage_kind": "central",
                "relative_dir": "conflict-demo",
                "files": [
                    {"relative_path": "SKILL.md", "content_base64": "LS0tCm5hbWU6IENvbmZsaWN0IERlbW8KLS0tCg=="},
                    {"relative_path": "node", "content_base64": "bm9kZQ=="},
                    {"relative_path": "node/child", "content_base64": "Y2hpbGQ="}
                ]
            }],
            "collections": [],
            "collection_skills": [],
            "settings": [],
            "agents": [],
            "scan_directories": [],
            "skill_installations": []
        });

        let error = import_app_backup_impl(&pool, &backup.to_string())
            .await
            .expect_err("conflicting backup accepted");
        assert!(error.contains("conflicting file and directory paths"));
        assert_eq!(
            std::fs::read_to_string(skill_dir.join("SKILL.md")).expect("existing skill content"),
            "---\nname: Existing\n---\n"
        );
    }

    #[test]
    fn normalize_relative_path_rejects_nested_windows_drive_segments() {
        assert!(normalize_relative_path("safe/C:/escape").is_err());
    }

    #[test]
    fn relative_to_root_from_keys_accepts_mixed_windows_separators() {
        assert_eq!(
            relative_to_root_from_keys(r"D:\Skills\library\owner\repo\demo", "D:/Skills/library")
                .as_deref(),
            Some("owner/repo/demo")
        );
        assert_eq!(
            relative_to_root_from_keys(r"D:\Skills\library-extra\demo", r"D:\Skills\library"),
            None
        );
    }

    #[tokio::test]
    async fn import_rejects_missing_skill_md_before_replacing_existing_skill_directory() {
        let (pool, _dir) = setup_test_db().await;
        let central = central_root(&pool).await.expect("central");
        let skill_dir = central.join("missing-skill-md-demo");
        std::fs::create_dir_all(&skill_dir).expect("skill dir");
        std::fs::write(skill_dir.join("SKILL.md"), "---\nname: Existing\n---\n")
            .expect("existing skill");
        let backup = serde_json::json!({
            "schema_version": BACKUP_SCHEMA_VERSION,
            "exported_at": "2026-01-01T00:00:00Z",
            "included": {"includeResourceLibrary": true, "includeCentralLibrary": true, "includeAppConfig": true, "includeInstallations": true},
            "skills": [{
                "skill": {
                    "id": "missing-skill-md-demo",
                    "name": "Missing Skill MD Demo",
                    "description": null,
                    "file_path": "",
                    "canonical_path": null,
                    "is_central": true,
                    "source": null,
                    "content": null,
                    "scanned_at": "2026-01-01T00:00:00Z"
                },
                "storage_kind": "central",
                "relative_dir": "missing-skill-md-demo",
                "files": [{"relative_path": "README.md", "content_base64": "cmVhZG1l"}]
            }],
            "collections": [],
            "collection_skills": [],
            "settings": [],
            "agents": [],
            "scan_directories": [],
            "skill_installations": []
        });

        let error = import_app_backup_impl(&pool, &backup.to_string())
            .await
            .expect_err("missing SKILL.md accepted");
        assert!(error.contains("missing SKILL.md"));
        assert_eq!(
            std::fs::read_to_string(skill_dir.join("SKILL.md")).expect("existing skill content"),
            "---\nname: Existing\n---\n"
        );
    }

    #[tokio::test]
    async fn import_rejects_global_skill_restore_path_collisions() {
        let (pool, _dir) = setup_test_db().await;
        let backup = serde_json::json!({
            "schema_version": BACKUP_SCHEMA_VERSION,
            "exported_at": "2026-01-01T00:00:00Z",
            "included": {"includeResourceLibrary": true, "includeCentralLibrary": true, "includeAppConfig": true, "includeInstallations": true},
            "skills": [
                {
                    "skill": {
                        "id": "demo-one",
                        "name": "Demo One",
                        "description": null,
                        "file_path": "",
                        "canonical_path": null,
                        "is_central": true,
                        "source": null,
                        "content": null,
                        "scanned_at": "2026-01-01T00:00:00Z"
                    },
                    "storage_kind": "central",
                    "relative_dir": "CaseSkill",
                    "files": [{"relative_path": "SKILL.md", "content_base64": "LS0tCm5hbWU6IERlbW8KLS0tCg=="}]
                },
                {
                    "skill": {
                        "id": "demo-two",
                        "name": "Demo Two",
                        "description": null,
                        "file_path": "",
                        "canonical_path": null,
                        "is_central": true,
                        "source": null,
                        "content": null,
                        "scanned_at": "2026-01-01T00:00:00Z"
                    },
                    "storage_kind": "central",
                    "relative_dir": "caseskill",
                    "files": [{"relative_path": "SKILL.md", "content_base64": "LS0tCm5hbWU6IERlbW8KLS0tCg=="}]
                }
            ],
            "collections": [],
            "collection_skills": [],
            "settings": [],
            "agents": [],
            "scan_directories": [],
            "skill_installations": []
        });

        let error = import_app_backup_impl(&pool, &backup.to_string())
            .await
            .expect_err("colliding restore paths accepted");
        assert!(error.contains("conflicting skill restore paths"));
    }

    #[test]
    fn webdav_upload_rejects_backups_larger_than_transfer_limit() {
        assert!(ensure_webdav_archive_fits(WEBDAV_MAX_TRANSFER_BYTES).is_ok());
        let error = ensure_webdav_archive_fits(WEBDAV_MAX_TRANSFER_BYTES + 1)
            .expect_err("oversized upload accepted");
        assert!(error.contains("size limit"));
    }

    #[tokio::test]
    async fn webdav_download_allows_body_transfer_beyond_control_request_timeout() {
        use std::net::TcpListener;
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let address = listener.local_addr().expect("address");
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept");
            stream
                .set_read_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            let mut request = Vec::new();
            let mut byte = [0u8; 1];
            while !request.ends_with(b"\r\n\r\n") {
                stream.read_exact(&mut byte).expect("read request");
                request.push(byte[0]);
            }
            stream
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 4\r\nConnection: close\r\n\r\nPK")
                .unwrap();
            stream.flush().unwrap();
            // Exercise the real production download path. Previously this request
            // failed while reading the body at the ordinary 30-second deadline.
            std::thread::sleep(WEBDAV_REQUEST_TIMEOUT + Duration::from_secs(1));
            let _ = stream.write_all(b"OK");
        });
        let config = WebDavConfig {
            base_url: format!("http://{address}"),
            username: None,
            password: None,
            remote_dir: "backups".to_string(),
        };
        let result = download_webdav_backup_impl(config, "backup.zip").await;
        assert_eq!(result.expect("slow download should finish"), b"PKOK");
        server.join().expect("server");
    }

    #[tokio::test]
    async fn webdav_upload_creates_missing_collection_before_put() {
        use std::io::{Read, Write};
        use std::net::TcpListener;
        use std::sync::{
            atomic::{AtomicUsize, Ordering},
            Arc, Mutex,
        };

        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let address = listener.local_addr().expect("addr");
        let requests = Arc::new(Mutex::new(Vec::<String>::new()));
        let accepted = Arc::new(AtomicUsize::new(0));
        let requests_clone = Arc::clone(&requests);
        let accepted_clone = Arc::clone(&accepted);

        listener.set_nonblocking(false).expect("blocking listener");
        let server = std::thread::spawn(move || {
            while accepted_clone.load(Ordering::SeqCst) < 2 {
                let (mut stream, _) = listener.accept().expect("accept");
                let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
                let mut data = Vec::new();
                let mut buffer = [0_u8; 4096];
                loop {
                    match stream.read(&mut buffer) {
                        Ok(0) => break,
                        Ok(bytes_read) => {
                            data.extend_from_slice(&buffer[..bytes_read]);
                            let header_end = data
                                .windows(4)
                                .position(|window| window == b"\r\n\r\n")
                                .map(|index| index + 4);
                            if let Some(header_end) = header_end {
                                let headers = String::from_utf8_lossy(&data[..header_end]);
                                let content_length = headers
                                    .lines()
                                    .find_map(|line| {
                                        line.split_once(':').and_then(|(name, value)| {
                                            if name.eq_ignore_ascii_case("content-length") {
                                                value.trim().parse::<usize>().ok()
                                            } else {
                                                None
                                            }
                                        })
                                    })
                                    .unwrap_or(0);
                                if data.len() >= header_end + content_length {
                                    break;
                                }
                            }
                        }
                        Err(_) => break,
                    }
                }
                let request_text = String::from_utf8_lossy(&data).to_string();
                requests_clone
                    .lock()
                    .expect("lock")
                    .push(request_text.clone());
                accepted_clone.fetch_add(1, Ordering::SeqCst);
                let status = if request_text.starts_with("MKCOL ") {
                    "HTTP/1.1 201 Created\r\nContent-Length: 0\r\n\r\n"
                } else {
                    "HTTP/1.1 201 Created\r\nContent-Length: 0\r\n\r\n"
                };
                let _ = stream.write_all(status.as_bytes());
            }
        });

        let config = WebDavConfig {
            base_url: format!("http://{address}/dav"),
            username: Some("user".to_string()),
            password: Some("token".to_string()),
            remote_dir: "skillshub".to_string(),
        };
        let uploaded = upload_webdav_backup_impl(config, b"zip-bytes".to_vec())
            .await
            .expect("upload");
        server.join().expect("server join");

        let captured = requests.lock().expect("captured");
        assert!(
            captured.iter().any(|request| request.starts_with("MKCOL ")),
            "missing MKCOL before PUT: {captured:?}"
        );
        assert!(
            captured.iter().any(|request| request.starts_with("PUT ")),
            "missing PUT after MKCOL: {captured:?}"
        );
        let put = captured
            .iter()
            .find(|request| request.starts_with("PUT "))
            .unwrap();
        assert!(put.to_ascii_lowercase().contains("if-none-match: *"));
        assert!(uploaded.name.ends_with(".zip"));
    }

    #[tokio::test]
    async fn webdav_delete_rejects_unsafe_remote_path_before_network_request() {
        let config = WebDavConfig {
            base_url: "https://example.com/dav".to_string(),
            username: None,
            password: None,
            remote_dir: "skillshub".to_string(),
        };

        let error = delete_webdav_backup_impl(config, "../escape.zip")
            .await
            .expect_err("unsafe delete path accepted");

        assert!(error.contains("unsafe traversal"));
    }

    #[tokio::test]
    async fn webdav_test_connection_validates_config_before_network_request() {
        let config = WebDavConfig {
            base_url: "".to_string(),
            username: None,
            password: None,
            remote_dir: "skillshub".to_string(),
        };

        let error = test_webdav_connection_impl(config)
            .await
            .expect_err("empty WebDAV URL accepted");

        assert!(error.contains("URL cannot be empty"));
    }

    #[tokio::test]
    async fn import_accepts_legacy_backslash_relative_paths_as_portable_segments() {
        let (pool, dir) = setup_test_db().await;
        let resource_root = dir.path().join("resource-library");
        db::set_skill_resource_library_dir(&pool, &resource_root.to_string_lossy())
            .await
            .expect("resource dir");
        let central = central_root(&pool).await.expect("central");
        let backup = serde_json::json!({
            "schema_version": BACKUP_SCHEMA_VERSION,
            "exported_at": "2026-01-01T00:00:00Z",
            "included": {"includeResourceLibrary": true, "includeCentralLibrary": true, "includeAppConfig": true, "includeInstallations": true},
            "skills": [{
                "skill": {
                    "id": "legacy-path-demo",
                    "name": "Legacy Path Demo",
                    "description": null,
                    "file_path": "",
                    "canonical_path": null,
                    "is_central": true,
                    "source": null,
                    "content": null,
                    "scanned_at": "2026-01-01T00:00:00Z"
                },
                "storage_kind": "central",
                "relative_dir": "nested\\legacy-path-demo",
                "files": [
                    {"relative_path": "SKILL.md", "content_base64": "LS0tCm5hbWU6IExlZ2FjeSBQYXRoIERlbW8KLS0tCg=="},
                    {"relative_path": "docs\\guide.md", "content_base64": "Z3VpZGU="}
                ]
            }],
            "collections": [],
            "collection_skills": [],
            "settings": [],
            "agents": [],
            "scan_directories": [],
            "skill_installations": []
        });

        import_app_backup_impl(&pool, &backup.to_string())
            .await
            .expect("import");
        assert_eq!(
            std::fs::read_to_string(resource_root.join("nested/legacy-path-demo/docs/guide.md"))
                .expect("restored file"),
            "guide"
        );
        assert!(
            !central.join("nested/legacy-path-demo").exists(),
            "legacy central path restore must not write Central Skills"
        );
    }
}
