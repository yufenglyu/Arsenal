use crate::AppState;
use super::notebook_repository as db;
pub(super) static WRITE: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
use base64::Engine;
use notify::Watcher;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{HashMap, HashSet},
    io::{Read, Write},
    path::{Component, Path, PathBuf},
    sync::{Mutex, OnceLock},
    time::{Duration, Instant},
};
use tauri::{Emitter, Manager, State};
use tauri_plugin_opener::OpenerExt;

const MAX_NOTE: u64 = 4 * 1024 * 1024;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Binding {
    pub binding_version: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note_id: Option<String>,
    pub vault_key: String,
    pub relative_path: String,
    pub obsidian_uri: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NotebookFile {
    pub body: String,
    pub revision: String,
    pub properties: String,
    pub relative_path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note_id: Option<String>,
}

#[derive(Default)]
pub struct NotebookRuntime {
    pub navigation: Mutex<Vec<String>>,
    pub watcher: Mutex<Option<notify::RecommendedWatcher>>,
}

fn revision(text: &str) -> String {
    format!("{:x}", Sha256::digest(text.as_bytes()))
}

pub fn valid_relative(value: &str) -> bool {
    !value.is_empty()
        && !value.contains('\\')
        && !value.contains(':')
        && Path::new(value)
            .components()
            .all(|c| matches!(c, Component::Normal(_)))
        && value.split('/').all(|s| {
            !s.is_empty()
                && !s.starts_with('.')
                && !s.ends_with(['.', ' '])
                && !s.chars().any(|c| c.is_control() || "<>\"|?*#^".contains(c))
                && ![
                    "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6",
                    "COM7", "COM8", "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7",
                    "LPT8", "LPT9",
                ]
                .contains(
                    &s.split('.')
                        .next()
                        .unwrap_or("")
                        .to_ascii_uppercase()
                        .as_str(),
                )
        })
}

// Settings accept either separator and an optional vault-root prefix.
pub fn normalize_note_folder(value: &str) -> Result<String, String> {
    let normalized = value.trim().replace('\\', "/")
        .split('/').filter(|part| !part.is_empty()).collect::<Vec<_>>().join("/");
    let normalized = if normalized.is_empty() { "SkillsHub".into() } else { normalized };
    if !valid_relative(&normalized) {
        return Err("notePath".into());
    }
    Ok(normalized)
}

pub fn validate_binding(binding: &Binding) -> bool {
    let Ok(uri) = reqwest::Url::parse(&binding.obsidian_uri) else {
        return false;
    };
    let identity_valid = match binding.binding_version {
        1 => binding.note_id.is_none() && uri.host_str() == Some("open"),
        2 => binding.note_id.as_ref().is_some_and(|id| {
            uuid::Uuid::parse_str(id).is_ok()
                && uri.host_str() == Some("skillshub-note")
                && uri.query_pairs().filter(|(key, _)| key == "id").count() == 1
                && uri
                    .query_pairs()
                    .any(|(key, value)| key == "id" && value == id.as_str())
                && uri.query_pairs().all(|(key, _)| key == "id")
        }),
        _ => false,
    };
    identity_valid
        && binding.vault_key == "default"
        && valid_relative(&binding.relative_path)
        && Path::new(&binding.relative_path)
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("md"))
        && uri.scheme() == "obsidian"
        && uri.username().is_empty()
        && uri.password().is_none()
        && uri.port().is_none()
        && uri.fragment().is_none()
        && uri.path().is_empty()
}

fn stable_note_uri(note_id: &str) -> String {
    format!("obsidian://skillshub-note?id={note_id}")
}

fn note_id(text: &str) -> Result<Option<String>, String> {
    let data = yaml(parts(text)?.1)?;
    match data.get("skillshub_note_id") {
        None => Ok(None),
        Some(value) => value
            .as_str()
            .and_then(|id| uuid::Uuid::parse_str(id).ok())
            .map(|id| Some(id.to_string()))
            .ok_or_else(|| "noteIdentity".into()),
    }
}

// Only managed identity fields are added during migration; body and user properties stay lossless.
fn with_note_identity(text: &str, id: &str) -> Result<String, String> {
    let (_, front, body) = parts(text)?;
    let newline = if text.contains("\r\n") { "\r\n" } else { "\n" };
    let normalized = front.replace("\r\n", "\n");
    let doc: yaml_edit::YamlFile = normalized.parse().map_err(|_| "noteYaml")?;
    doc.ensure_document();
    doc.set("skillshub_note_id", id);
    doc.set("skillshub_note_uri", stable_note_uri(id));
    doc.set("skillshub_schema", 2);
    let bom = if text.starts_with('\u{feff}') {
        "\u{feff}"
    } else {
        ""
    };
    Ok(format!(
        "{bom}---{newline}{}{newline}---{newline}{body}",
        doc.to_string()
            .trim_end_matches('\n')
            .replace('\n', newline)
    ))
}

fn vault(pool: &crate::db::DbPool) -> Result<PathBuf, String> {
    let config = crate::db::load_config(pool)?.settings;
    let root = config
        .get("obsidian_vault_path")
        .filter(|p| !p.trim().is_empty())
        .ok_or("obsidianSetup")?;
    let root = std::fs::canonicalize(root).map_err(|_| "obsidianUnavailable")?;
    if !root.is_dir() || !root.join(".obsidian").is_dir() {
        return Err("obsidianSetup".into());
    }
    Ok(root)
}

// Resolve every existing component, including the final file, inside the chosen vault.
fn safe_path(root: &Path, relative: &str, create_parents: bool) -> Result<PathBuf, String> {
    if !valid_relative(relative) {
        return Err("notePath".into());
    }
    let parts: Vec<_> = Path::new(relative).components().collect();
    let mut path = root.to_owned();
    for (index, part) in parts.iter().enumerate() {
        path.push(part.as_os_str());
        if path.exists() || std::fs::symlink_metadata(&path).is_ok() {
            let resolved = std::fs::canonicalize(&path).map_err(|_| "notePath")?;
            if !resolved.starts_with(root) {
                return Err("notePath".into());
            }
            // Refuse links even within the vault to keep replacement semantics predictable.
            if std::fs::symlink_metadata(&path)
                .map_err(|_| "notePath")?
                .file_type()
                .is_symlink()
            {
                return Err("notePath".into());
            }
            path = resolved;
        } else if index + 1 < parts.len() {
            if !create_parents {
                return Err("noteMissing".into());
            }
            std::fs::create_dir(&path).map_err(|_| "noteWrite")?;
            path = std::fs::canonicalize(&path).map_err(|_| "notePath")?;
            if !path.starts_with(root) {
                return Err("notePath".into());
            }
        }
    }
    Ok(path)
}

fn relative_input(root: &Path, input: &str) -> Result<String, String> {
    if input.contains(['#', '^']) {
        return Err("noteUriPath".into());
    }
    let input = Path::new(input);
    let relative = if input.is_absolute() {
        dunce::simplified(input)
            .strip_prefix(dunce::simplified(root))
            .map_err(|_| "notePath")?
    } else {
        input
    };
    let relative = relative.to_string_lossy().replace('\\', "/");
    if !valid_relative(&relative)
        || !Path::new(&relative)
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("md"))
    {
        return Err("notePath".into());
    }
    Ok(relative)
}

fn read_file(path: &Path) -> Result<String, String> {
    let mut file = std::fs::File::open(path).map_err(|_| "noteMissing")?;
    if file.metadata().map_err(|_| "noteRead")?.len() > MAX_NOTE {
        return Err("tooLarge".into());
    }
    let mut text = String::new();
    Read::by_ref(&mut file)
        .take(MAX_NOTE + 1)
        .read_to_string(&mut text)
        .map_err(|_| "noteRead")?;
    if text.len() as u64 > MAX_NOTE {
        return Err("tooLarge".into());
    }
    Ok(text)
}

// Byte offsets preserve the existing frontmatter and its original line endings on body saves.
fn parts(text: &str) -> Result<(&str, &str, &str), String> {
    let bom = if text.starts_with('\u{feff}') { 3 } else { 0 };
    let remainder = &text[bom..];
    let opening = if remainder.starts_with("---\r\n") {
        5
    } else if remainder.starts_with("---\n") {
        4
    } else {
        return Ok((&text[..bom], "", remainder));
    };
    let mut offset = bom + opening;
    for line in text[offset..].split_inclusive('\n') {
        if line.trim_end_matches(['\r', '\n']) == "---" {
            return Ok((
                &text[..offset + line.len()],
                &text[bom + opening..offset],
                &text[offset + line.len()..],
            ));
        }
        offset += line.len();
    }
    Err("noteYaml".into())
}

fn yaml(text: &str) -> Result<serde_yaml::Mapping, String> {
    if text.trim().is_empty() {
        return Ok(serde_yaml::Mapping::new());
    }
    let value: serde_yaml::Value = serde_yaml::from_str(text).map_err(|_| "noteYaml")?;
    value.as_mapping().cloned().ok_or_else(|| "noteYaml".into())
}

fn check_identity(text: &str, uid: &str, required: bool) -> Result<(), String> {
    let (_, front, _) = parts(text)?;
    let data = yaml(front)?;
    match data.get(serde_yaml::Value::String("skillshub_repository_id".into())) {
        Some(value) if value.as_str() == Some(uid) => Ok(()),
        None if !required => Ok(()),
        _ => Err("noteIdentity".into()),
    }
}

fn snapshot(text: &str, relative: &str) -> Result<NotebookFile, String> {
    let (_, front, body) = parts(text)?;
    let data = yaml(front)?;
    let properties = if data.is_empty() {
        String::new()
    } else {
        serde_yaml::to_string(&data).map_err(|_| "noteYaml")?
    };
    Ok(NotebookFile {
        body: body.into(),
        revision: revision(text),
        properties,
        relative_path: relative.into(),
        note_id: note_id(text)?,
    })
}

fn write_checked(path: &Path, expected: Option<&str>, text: &str) -> Result<(), String> {
    if text.len() as u64 > MAX_NOTE {
        return Err("tooLarge".into());
    }
    let check = || -> Result<(), String> {
        match expected {
            Some(hash) if revision(&read_file(path)?) == hash => Ok(()),
            None if !path.exists() => Ok(()),
            _ => Err("noteConflict".into()),
        }
    };
    check()?;
    let mut temp = tempfile::NamedTempFile::new_in(path.parent().ok_or("notePath")?)
        .map_err(|_| "noteWrite")?;
    if path.exists() {
        temp.as_file()
            .set_permissions(
                std::fs::metadata(path)
                    .map_err(|_| "noteWrite")?
                    .permissions(),
            )
            .map_err(|_| "noteWrite")?;
    }
    temp.write_all(text.as_bytes()).map_err(|_| "noteWrite")?;
    temp.as_file().sync_all().map_err(|_| "noteWrite")?;
    check()?;
    if expected.is_some() {
        temp.persist(path).map_err(|_| "noteWrite")?;
    } else {
        temp.persist_noclobber(path).map_err(|_| "noteConflict")?;
    }
    if read_file(path)? != text {
        return Err("noteConflict".into());
    }
    Ok(())
}

fn default_template() -> String {
    serde_json::from_str::<serde_json::Value>(include_str!(
        "../../../src/data/obsidianTemplate.json"
    ))
    .expect("bundled template is valid")["template"]
        .as_str()
        .expect("bundled template exists")
        .into()
}

fn render_template(
    template: &str,
    variables: &serde_json::Value,
) -> Result<serde_yaml::Mapping, String> {
    if template.len() > 64 * 1024 {
        return Err("noteTemplate".into());
    }
    let mut rendered = String::new();
    let mut remainder = template;
    while let Some(start) = remainder.find("{{") {
        rendered.push_str(&remainder[..start]);
        let tail = &remainder[start + 2..];
        let end = tail.find("}}").ok_or("noteTemplate")?;
        let value = variables.get(tail[..end].trim()).ok_or("noteTemplate")?;
        rendered.push_str(&serde_json::to_string(value).map_err(|_| "noteTemplate")?);
        remainder = &tail[end + 2..];
    }
    rendered.push_str(remainder);
    let (prefix, front, body) = parts(&rendered).map_err(|_| "noteTemplate")?;
    if prefix.is_empty() || !body.trim().is_empty() {
        return Err("noteTemplate".into());
    }
    let mapping = yaml(front).map_err(|_| "noteTemplate")?;
    if mapping.keys().any(|key| key.as_str().is_none()) {
        return Err("noteTemplate".into());
    }
    Ok(mapping)
}

pub fn validate_template(template: &str) -> Result<(), String> {
    render_template(template, &serde_json::json!({"noteId":"00000000-0000-4000-8000-000000000002", "obsidianUri":"obsidian://skillshub-note?id=00000000-0000-4000-8000-000000000002", "repositoryUid":"fixture-id", "skillshubUri":"skillshub://repository/fixture-id?tab=notebook", "fullName":"example/project", "url":"https://github.com/example/project", "description":"Example: a quoted \"value\"\nSecond line", "stars":42, "tags":["example"], "categories":["Tools/Example"]})).map(|_| ())
}

fn configured_template(pool: &crate::db::DbPool) -> Result<String, String> {
    Ok(crate::db::load_config(pool)?.settings
        .get("obsidian_property_template")
        .filter(|s| !s.trim().is_empty())
        .cloned()
        .unwrap_or_else(default_template))
}

#[cfg(test)]
fn properties(
    text: &str,
    repository: &db::Repository,
    categories: &[db::Category],
) -> Result<String, String> {
    properties_with_template(text, repository, categories, &default_template())
}

fn properties_with_template(
    text: &str,
    repository: &db::Repository,
    categories: &[db::Category],
    template: &str,
) -> Result<String, String> {
    check_identity(text, &repository.repository_uid, false)?;
    let (_, front, body) = parts(text)?;
    let newline = if text.contains("\r\n") { "\r\n" } else { "\n" };
    let front = front.replace("\r\n", "\n");
    let document: yaml_edit::YamlFile = front.parse().map_err(|_| "noteYaml")?;
    document.ensure_document();
    let identity = note_id(text)?.unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
    let variables = serde_json::json!({
        "noteId":identity, "obsidianUri":stable_note_uri(&identity),
        "repositoryUid":repository.repository_uid,
        "skillshubUri":format!("skillshub://repository/{}?tab=notebook", repository.repository_uid),
        "fullName":repository.full_name, "url":repository.url,
        "description":repository.description, "tags":repository.tags, "stars": repository.stars,
        "categories":categories.iter().filter(|c| c.repository_ids.contains(&repository.id)).map(|c| {
            let mut names = vec![c.name.clone()]; let mut parent = c.parent_id.as_deref(); let mut seen = HashSet::new();
            while let Some(id) = parent { if !seen.insert(id) { break; }
            if let Some(p) = categories.iter().find(|p| p.id == id) { names.push(p.name.clone()); parent = p.parent_id.as_deref(); } else { break; } }
            names.reverse(); names.join("/")
        }).collect::<Vec<_>>()
    });
    let mut data = render_template(template, &variables)?;
    // Stable identity and the return link cannot be disabled by a custom template.
    for (key, value) in [
        ("skillshub_schema", serde_yaml::Value::Number(2.into())),
        (
            "skillshub_note_id",
            serde_yaml::Value::String(identity.clone()),
        ),
        (
            "skillshub_note_uri",
            serde_yaml::Value::String(stable_note_uri(&identity)),
        ),
        (
            "skillshub_repository_id",
            serde_yaml::Value::String(repository.repository_uid.clone()),
        ),
        (
            "skillshub_uri",
            serde_yaml::Value::String(
                variables["skillshubUri"]
                    .as_str()
                    .ok_or("noteTemplate")?
                    .into(),
            ),
        ),
    ] {
        data.insert(serde_yaml::Value::String(key.into()), value);
    }
    if repository.stars.is_none() {
        data.remove(serde_yaml::Value::String("skillshub_stars".into()));
    }
    // Serialize as YAML Front Matter, retaining scalar/list types rather than JSON formatting.
    let values: yaml_edit::YamlFile = serde_yaml::to_string(&data)
        .map_err(|_| "noteYaml")?
        .parse()
        .map_err(|_| "noteYaml")?;
    let mapping = values
        .document()
        .and_then(|d| d.as_mapping())
        .ok_or("noteYaml")?;
    for (key, value) in mapping.iter() {
        document.set(key, value);
    }
    if repository.stars.is_none() {
        if let Some(mapping) = document.document().and_then(|d| d.as_mapping()) {
            mapping.remove("skillshub_stars");
        }
    }
    let edited = document.to_string();
    let edited = edited.trim_end_matches('\n');
    let bom = if text.starts_with('\u{feff}') {
        "\u{feff}"
    } else {
        ""
    };
    let result = format!(
        "{bom}---{newline}{}{newline}---{newline}{body}",
        edited.replace('\n', newline)
    );
    check_identity(&result, &repository.repository_uid, true)?;
    // The lossless editor must retain all user property values and comments.
    let old = yaml(&front)?;
    let new = yaml(parts(&result)?.1)?;
    for (key, value) in old {
        if !key.as_str().is_some_and(|key| {
            data.contains_key(serde_yaml::Value::String(key.into())) || key == "skillshub_stars"
        }) && new.get(&key) != Some(&value)
        {
            return Err("noteYaml".into());
        }
    }
    Ok(result)
}

#[cfg(test)]
fn binding_for_path(root: &Path, relative: &str) -> Result<Binding, String> {
    let mut uri = reqwest::Url::parse("obsidian://open").map_err(|_| "operation")?;
    uri.query_pairs_mut()
        .append_pair(
            "vault",
            &root.file_name().ok_or("obsidianSetup")?.to_string_lossy(),
        )
        .append_pair("file", relative);
    Ok(Binding {
        binding_version: 1,
        note_id: None,
        vault_key: "default".into(),
        relative_path: relative.into(),
        obsidian_uri: uri.to_string().replace('+', "%20"),
    })
}

fn binding_for_note(relative: &str, id: &str) -> Binding {
    Binding {
        binding_version: 2,
        note_id: Some(id.into()),
        vault_key: "default".into(),
        relative_path: relative.into(),
        obsidian_uri: stable_note_uri(id),
    }
}

#[derive(Default)]
struct VaultIndex {
    notes: HashMap<String, Vec<String>>,
    repositories: HashMap<String, Vec<String>>,
}
struct CachedIndex {
    built: Instant,
    index: VaultIndex,
}
static NOTE_INDEX: OnceLock<Mutex<HashMap<PathBuf, CachedIndex>>> = OnceLock::new();
fn indexes() -> &'static Mutex<HashMap<PathBuf, CachedIndex>> {
    NOTE_INDEX.get_or_init(Mutex::default)
}
fn invalidate_index(root: &Path) {
    if let Ok(mut cache) = indexes().lock() {
        cache.remove(root);
    }
}

fn build_index(root: &Path) -> Result<VaultIndex, String> {
    let mut index = VaultIndex::default();
    let mut directories = vec![root.to_owned()];
    let mut visited = 0;
    while let Some(directory) = directories.pop() {
        for entry in std::fs::read_dir(directory).map_err(|_| "noteRead")? {
            visited += 1;
            if visited > 20000 {
                return Err("noteLocate".into());
            }
            let entry = entry.map_err(|_| "noteRead")?;
            if entry.file_name().to_string_lossy().starts_with('.') {
                continue;
            }
            let path = entry.path();
            let relative = path
                .strip_prefix(root)
                .map_err(|_| "notePath")?
                .to_string_lossy()
                .replace('\\', "/");
            if !valid_relative(&relative) {
                continue;
            }
            let Ok(safe) = safe_path(root, &relative, false) else {
                continue;
            };
            let kind = entry.file_type().map_err(|_| "noteRead")?;
            if kind.is_dir() {
                directories.push(safe);
            } else if kind.is_file()
                && path
                    .extension()
                    .is_some_and(|e| e.eq_ignore_ascii_case("md"))
            {
                let Ok(text) = read_file(&safe) else {
                    continue;
                };
                let Ok((_, front, _)) = parts(&text) else {
                    continue;
                };
                let Ok(data) = yaml(front) else {
                    continue;
                };
                if let Some(id) = data.get("skillshub_repository_id").and_then(|v| v.as_str()) {
                    index
                        .repositories
                        .entry(id.to_owned())
                        .or_default()
                        .push(relative.clone());
                }
                if let Ok(Some(id)) = note_id(&text) {
                    index.notes.entry(id).or_default().push(relative);
                }
            }
        }
    }
    Ok(index)
}

fn indexed_path(root: &Path, uid: &str, note: Option<&str>, force: bool) -> Result<String, String> {
    let mut cache = indexes().lock().map_err(|_| "operation")?;
    if force
        || cache
            .get(root)
            .is_none_or(|cached| cached.built.elapsed() >= Duration::from_secs(30))
    {
        let index = build_index(root)?;
        cache.insert(
            root.to_owned(),
            CachedIndex {
                built: Instant::now(),
                index,
            },
        );
    }
    let index = &cache.get(root).ok_or("noteRead")?.index;
    let matches = if let Some(id) = note {
        index.notes.get(id)
    } else {
        index.repositories.get(uid)
    };
    match matches.map(Vec::as_slice) {
        Some([only]) => Ok(only.clone()),
        Some(_) => Err("noteLocate".into()),
        None => Err("noteMissing".into()),
    }
}

#[cfg(test)]
fn find_moved_note(root: &Path, uid: &str) -> Result<String, String> {
    indexed_path(root, uid, None, true)
}

// Callers hold AppState.write. Cached paths never authorize reading a different identity.
async fn resolve_in_vault(
    state: &AppState,
    repository: &mut db::Repository,
    root: &Path,
) -> Result<(PathBuf, Binding), String> {
    let previous = repository.obsidian.as_ref().ok_or("noteUnlinked")?.clone();
    if !validate_binding(&previous) {
        return Err("notePath".into());
    }
    let mut relative = match indexed_path(
        root,
        &repository.repository_uid,
        previous.note_id.as_deref(),
        false,
    ) {
        Ok(path) => path,
        Err(error) if error == "noteMissing" => indexed_path(
            root,
            &repository.repository_uid,
            previous.note_id.as_deref(),
            true,
        )?,
        Err(error) => return Err(error),
    };
    let mut path = safe_path(root, &relative, false)?;
    let mut text = read_file(&path);
    if text.is_err()
        || text.as_ref().is_ok_and(|text| {
            previous.note_id.is_some() && note_id(text).ok().flatten() != previous.note_id
        })
    {
        relative = indexed_path(
            root,
            &repository.repository_uid,
            previous.note_id.as_deref(),
            true,
        )?;
        path = safe_path(root, &relative, false)?;
        text = read_file(&path);
    }
    let text = text?;
    check_identity(&text, &repository.repository_uid, true)?;
    let id = match previous.note_id {
        Some(id) if note_id(&text)?.as_deref() == Some(&id) => id,
        Some(_) => return Err("noteIdentity".into()),
        None => {
            let id = note_id(&text)?.unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
            // An existing note ID must also be unique before a legacy association adopts it.
            if note_id(&text)?.is_some() {
                indexed_path(root, &repository.repository_uid, Some(&id), true)?;
            }
            let migrated = with_note_identity(&text, &id)?;
            if migrated != text {
                write_checked(&path, Some(&revision(&text)), &migrated)?;
                invalidate_index(root);
            }
            id
        }
    };
    let binding = binding_for_note(&relative, &id);
    if repository.obsidian.as_ref() != Some(&binding) {
        repository.obsidian = Some(binding.clone());
        db::put(&state.db, repository).await?;
    }
    Ok((path, binding))
}

async fn bound_path(
    state: &AppState,
    repository: &mut db::Repository,
) -> Result<(PathBuf, PathBuf, Binding), String> {
    let root = vault(&state.db)?;
    let (path, binding) = resolve_in_vault(state, repository, &root).await?;
    Ok((root, path, binding))
}

#[tauri::command]
pub async fn preview_notebook_properties(
    state: State<'_, AppState>,
    id: i64,
) -> Result<String, String> {
    let repository = db::get(&state.db, id).await?;
    let categories = db::categories(&state.db).await?;
    properties_with_template("", &repository, &categories, &configured_template(&state.db)?)
}

#[tauri::command]
pub async fn read_notebook(state: State<'_, AppState>, id: i64) -> Result<NotebookFile, String> {
    let _guard = WRITE.lock().await;
    let mut repository = db::get(&state.db, id).await?;
    let (_, path, binding) = bound_path(&state, &mut repository).await?;
    let text = read_file(&path)?;
    check_identity(&text, &repository.repository_uid, true)?;
    snapshot(&text, &binding.relative_path)
}

#[tauri::command]
pub fn inspect_notebook(state: State<'_, AppState>, path: String) -> Result<NotebookFile, String> {
    let root = vault(&state.db)?;
    let relative = relative_input(&root, &path)?;
    snapshot(&read_file(&safe_path(&root, &relative, false)?)?, &relative)
}

#[tauri::command]
pub async fn bind_notebook(
    state: State<'_, AppState>,
    id: i64,
    path: String,
    create: bool,
    expected_revision: Option<String>,
    initial_body: Option<String>,
) -> Result<NotebookFile, String> {
    let _guard = WRITE.lock().await;
    let mut repository = db::get(&state.db, id).await?;
    let root = vault(&state.db)?;
    let relative = relative_input(&root, &path)?;
    for other in db::list(&state.db).await? {
        if other.id != id
            && other.obsidian.is_some_and(|b| {
                b.note_id.is_none() && b.relative_path.eq_ignore_ascii_case(&relative)
            })
        {
            return Err("noteIdentity".into());
        }
    }
    let path = safe_path(&root, &relative, create)?;
    let text = if create {
        initial_body.unwrap_or_else(|| repository.notebook.clone())
    } else {
        if expected_revision.is_none() {
            return Err("noteConflict".into());
        }
        read_file(&path)?
    };
    if let Some(id) = note_id(&text)? {
        let indexed = indexed_path(&root, &repository.repository_uid, Some(&id), true)?;
        if indexed != relative {
            return Err("noteIdentity".into());
        }
    }
    let categories = db::categories(&state.db).await?;
    let text = properties_with_template(&text, &repository, &categories, &configured_template(&state.db)?)?;
    write_checked(
        &path,
        if create {
            None
        } else {
            expected_revision.as_deref()
        },
        &text,
    )?;
    repository.obsidian = Some(binding_for_note(
        &relative,
        &note_id(&text)?.ok_or("noteIdentity")?,
    ));
    invalidate_index(&root);
    db::put(&state.db, &repository).await?;
    snapshot(&text, &relative)
}

#[tauri::command]
pub async fn save_notebook(
    state: State<'_, AppState>,
    id: i64,
    body: String,
    expected_revision: String,
    update_properties: bool,
) -> Result<NotebookFile, String> {
    let _guard = WRITE.lock().await;
    let mut repository = db::get(&state.db, id).await?;
    let (_, path, binding) = bound_path(&state, &mut repository).await?;
    let current = read_file(&path)?;
    check_identity(&current, &repository.repository_uid, true)?;
    if revision(&current) != expected_revision {
        return Err("noteConflict".into());
    }
    let next = if update_properties {
        let categories = db::categories(&state.db).await?;
        properties_with_template(&current, &repository, &categories, &configured_template(&state.db)?)?
    } else {
        let prefix = parts(&current)?.0;
        let separator = if prefix.ends_with("---") { "\n" } else { "" };
        format!("{prefix}{separator}{body}")
    };
    write_checked(&path, Some(&expected_revision), &next)?;
    invalidate_index(&vault(&state.db)?);
    snapshot(&next, &binding.relative_path)
}

#[tauri::command]
pub async fn rename_notebook(
    state: State<'_, AppState>,
    id: i64,
    name: String,
    expected_revision: String,
) -> Result<NotebookFile, String> {
    let _guard = WRITE.lock().await;
    let mut repository = db::get(&state.db, id).await?;
    let (root, source, binding) = bound_path(&state, &mut repository).await?;
    rename_file_binding(
        &state,
        &mut repository,
        &root,
        &source,
        &binding,
        &name,
        &expected_revision,
    )
    .await
}

async fn rename_file_binding(
    state: &AppState,
    repository: &mut db::Repository,
    root: &Path,
    source: &Path,
    binding: &Binding,
    name: &str,
    expected_revision: &str,
) -> Result<NotebookFile, String> {
    let text = read_file(source)?;
    check_identity(&text, &repository.repository_uid, true)?;
    if revision(&text) != expected_revision {
        return Err("noteConflict".into());
    }
    let relative = renamed_relative(&binding.relative_path, name)?;
    if relative == binding.relative_path {
        return snapshot(&text, &relative);
    }
    for other in db::list(&state.db).await? {
        if other.id != repository.id
            && other.obsidian.is_some_and(|b| {
                b.note_id.is_none() && b.relative_path.eq_ignore_ascii_case(&relative)
            })
        {
            return Err("noteIdentity".into());
        }
    }
    let target = safe_path(root, &relative, false)?;
    // Stage a new file without overwriting any existing target, on all supported filesystems.
    write_checked(&target, None, &text)?;
    if revision(&read_file(source)?) != expected_revision {
        return Err("noteConflict".into()); // Keep both files so concurrent external edits are never lost.
    }
    repository.obsidian = Some(match binding.note_id.as_deref() {
        Some(id) => binding_for_note(&relative, id),
        None => return Err("noteIdentity".into()),
    });
    invalidate_index(root);
    if let Err(error) = db::put(&state.db, repository).await {
        // The original remains authoritative if database persistence fails.
        if read_file(&target).is_ok_and(|current| current == text) {
            let _ = std::fs::remove_file(&target);
        }
        return Err(error);
    }
    // If Obsidian has already changed the old file, retain it instead of deleting its new content.
    if read_file(source).is_ok_and(|current| revision(&current) == expected_revision) {
        std::fs::remove_file(source).map_err(|_| "noteWrite")?;
    }
    snapshot(&read_file(&target)?, &relative)
}

fn renamed_relative(relative: &str, name: &str) -> Result<String, String> {
    let name = name.trim();
    if name.is_empty() || name.contains(['/', '\\']) {
        return Err("notePath".into());
    }
    let filename = if name.to_ascii_lowercase().ends_with(".md") {
        name.to_owned()
    } else {
        format!("{name}.md")
    };
    let parent = Path::new(relative).parent().ok_or("notePath")?;
    let next = parent.join(filename).to_string_lossy().replace('\\', "/");
    if !valid_relative(&next) {
        return Err("notePath".into());
    }
    Ok(next)
}

#[tauri::command]
pub async fn unlink_notebook(
    state: State<'_, AppState>,
    id: i64,
    keep_body: bool,
    expected_revision: Option<String>,
) -> Result<(), String> {
    let _guard = WRITE.lock().await;
    let mut repository = db::get(&state.db, id).await?;
    if keep_body {
        let (_, path, _) = bound_path(&state, &mut repository).await?;
        let text = read_file(&path)?;
        check_identity(&text, &repository.repository_uid, true)?;
        if expected_revision.as_deref() != Some(&revision(&text)) {
            return Err("noteConflict".into());
        }
        repository.notebook = parts(&text)?.2.into();
    }
    repository.obsidian = None;
    db::put(&state.db, &repository).await
}

#[tauri::command]
pub async fn clear_legacy_notebook(
    state: State<'_, AppState>,
    id: i64,
    expected_legacy: String,
) -> Result<(), String> {
    let _guard = WRITE.lock().await;
    let mut repository = db::get(&state.db, id).await?;
    let (_, path, _) = bound_path(&state, &mut repository).await?;
    check_identity(&read_file(&path)?, &repository.repository_uid, true)?;
    if repository.notebook != expected_legacy {
        return Err("noteConflict".into());
    }
    repository.notebook.clear();
    db::put(&state.db, &repository).await
}

#[tauri::command]
pub async fn open_obsidian(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    id: i64,
) -> Result<(), String> {
    let _guard = WRITE.lock().await;
    let mut repository = db::get(&state.db, id).await?;
    let (_, path, _) = bound_path(&state, &mut repository).await?;
    check_identity(&read_file(&path)?, &repository.repository_uid, true)?;
    let mut uri = reqwest::Url::parse("obsidian://open").map_err(|_| "operation")?;
    // An absolute path avoids ambiguity when two vaults have the same name.
    uri.query_pairs_mut()
        .append_pair("path", &dunce::simplified(&path).to_string_lossy());
    app.opener()
        .open_url(uri.to_string().replace('+', "%20"), None::<&str>)
        .map_err(|_| "obsidianOpen".into())
}

fn asset_path(root: &Path, binding: &Binding, link: &str) -> Result<PathBuf, String> {
    let base = reqwest::Url::from_file_path(dunce::simplified(root).join(&binding.relative_path))
        .map_err(|_| "notePath")?;
    let target = base.join(link).map_err(|_| "notePath")?;
    if target.scheme() != "file" {
        return Err("notePath".into());
    }
    let path = target.to_file_path().map_err(|_| "notePath")?;
    let path = std::fs::canonicalize(path).map_err(|_| "noteMissing")?;
    let relative = path.strip_prefix(root).map_err(|_| "notePath")?;
    if relative
        .components()
        .any(|c| c.as_os_str().to_string_lossy().starts_with('.'))
    {
        return Err("notePath".into());
    }
    Ok(path)
}

#[tauri::command]
pub async fn notebook_asset(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    id: i64,
    link: String,
    open: bool,
) -> Result<String, String> {
    let _guard = WRITE.lock().await;
    let mut repository = db::get(&state.db, id).await?;
    let (root, _, binding) = bound_path(&state, &mut repository).await?;
    let path = asset_path(&root, &binding, &link)?;
    if open {
        let ext = path
            .extension()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        if ![
            "md", "txt", "pdf", "png", "jpg", "jpeg", "gif", "webp", "docx", "xlsx", "pptx",
        ]
        .contains(&ext.as_str())
        {
            return Err("notePath".into());
        }
        app.opener()
            .open_path(dunce::simplified(&path).to_string_lossy(), None::<&str>)
            .map_err(|_| "operation")?;
        return Ok(String::new());
    }
    let mime = match path
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_ascii_lowercase()
        .as_str()
    {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        _ => return Err("notePath".into()),
    };
    let file = std::fs::File::open(path).map_err(|_| "noteRead")?;
    let mut bytes = Vec::new();
    file.take(10 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "noteRead")?;
    if bytes.len() > 10 * 1024 * 1024 {
        return Err("tooLarge".into());
    }
    Ok(format!(
        "data:{mime};base64,{}",
        base64::engine::general_purpose::STANDARD.encode(bytes)
    ))
}

#[tauri::command]
pub fn watch_notebook(app: tauri::AppHandle, state: State<'_, AppState>, enabled: bool) -> Result<(), String> {
    let runtime = app.state::<NotebookRuntime>();
    let mut slot = runtime.watcher.lock().map_err(|_| "operation")?;
    *slot = None;
    if enabled {
        let root = vault(&state.db)?;
        let handle = app.clone();
        let watched_root = root.clone();
        let mut watcher =
            notify::recommended_watcher(move |event: Result<notify::Event, notify::Error>| {
                if event.is_ok() {
                    invalidate_index(&watched_root);
                    let _ = handle.emit("obsidian-changed", ());
                }
            })
            .map_err(|_| "noteRead")?;
        watcher
            .watch(&root, notify::RecursiveMode::Recursive)
            .map_err(|_| "noteRead")?;
        *slot = Some(watcher);
    }
    Ok(())
}

pub fn navigation_uid(uri: &str) -> Option<String> {
    let uri = reqwest::Url::parse(uri).ok()?;
    if uri.scheme() != "skillshub"
        || uri.host_str() != Some("repository")
        || !uri.username().is_empty()
        || uri.password().is_some()
        || uri.port().is_some()
        || uri.fragment().is_some()
    {
        return None;
    }
    let uid = uri.path().strip_prefix('/')?;
    let uid = uuid::Uuid::parse_str(uid).ok()?;
    if uri
        .query_pairs()
        .any(|(key, value)| key != "tab" || value != "notebook")
    {
        return None;
    }
    Some(uid.to_string())
}

pub fn queue_navigation(app: &tauri::AppHandle, urls: impl IntoIterator<Item = String>) {
    if let Ok(mut queue) = app.state::<NotebookRuntime>().navigation.lock() {
        for uri in urls {
            if let Some(uid) = navigation_uid(&uri) {
                queue.push(uid);
            }
        }
        queue.truncate(16);
    }
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
    }
    let _ = app.emit("notebook-navigation", ());
}

#[tauri::command]
pub fn take_notebook_navigation(app: tauri::AppHandle) -> Vec<String> {
    app.state::<NotebookRuntime>()
        .navigation
        .lock()
        .map(|mut q| std::mem::take(&mut *q))
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn moved_notes_are_found_by_identity_and_duplicates_are_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let root = std::fs::canonicalize(dir.path()).unwrap();
        let uid = "00000000-0000-4000-8000-000000000001";
        let text = format!("---\nskillshub_repository_id: {uid}\n---\nbody");
        let path = safe_path(&root, "notes/Renamed 中文.md", true).unwrap();
        std::fs::write(&path, &text).unwrap();
        assert_eq!(
            find_moved_note(&root, uid).unwrap(),
            "notes/Renamed 中文.md"
        );
        let binding = binding_for_path(&root, "notes/Renamed 中文.md").unwrap();
        let url = reqwest::Url::parse(&binding.obsidian_uri).unwrap();
        assert!(url
            .query_pairs()
            .any(|(key, value)| key == "file" && value == "notes/Renamed 中文.md"));
        std::fs::write(root.join("duplicate.md"), &text).unwrap();
        assert_eq!(find_moved_note(&root, uid).unwrap_err(), "noteLocate");
        std::fs::remove_file(root.join("duplicate.md")).unwrap();
        std::fs::remove_file(path).unwrap();
        assert_eq!(find_moved_note(&root, uid).unwrap_err(), "noteMissing");
    }

    #[tokio::test]
    async fn rename_preserves_file_identity_updates_database_and_refuses_overwrite() {
        let dir = tempfile::tempdir().unwrap();
        let root = std::fs::canonicalize(dir.path()).unwrap();
        let state = AppState {
            db: db::create_pool(&root.join("test.sqlite")).await.unwrap(),
        };
        let mut repository = db::Repository::new(1, "example/project".into());
        let binding = binding_for_note(
            "notes/project@example.md",
            "00000000-0000-4000-8000-000000000002",
        );
        repository.obsidian = Some(binding.clone());
        db::put(&state.db, &repository).await.unwrap();
        let source = safe_path(&root, &binding.relative_path, true).unwrap();
        let text = format!("---\n# retain comment\nskillshub_repository_id: {}\nskillshub_uri: skillshub://repository/{}?tab=notebook\ncustom: [one, two]\n---\n# Body\n",repository.repository_uid,repository.repository_uid);
        let text = with_note_identity(&text, binding.note_id.as_deref().unwrap()).unwrap();
        std::fs::write(&source, &text).unwrap();
        std::fs::write(root.join("notes/occupied.md"), "keep other file").unwrap();
        assert_eq!(
            rename_file_binding(
                &state,
                &mut repository,
                &root,
                &source,
                &binding,
                "occupied",
                &revision(&text)
            )
            .await
            .unwrap_err(),
            "noteConflict"
        );
        assert_eq!(
            read_file(&root.join("notes/occupied.md")).unwrap(),
            "keep other file"
        );
        assert_eq!(
            rename_file_binding(
                &state,
                &mut repository,
                &root,
                &source,
                &binding,
                "Renamed",
                "stale"
            )
            .await
            .unwrap_err(),
            "noteConflict"
        );
        let result = rename_file_binding(
            &state,
            &mut repository,
            &root,
            &source,
            &binding,
            "Renamed 中文",
            &revision(&text),
        )
        .await
        .unwrap();
        assert_eq!(result.relative_path, "notes/Renamed 中文.md");
        assert!(!source.exists());
        assert_eq!(read_file(&root.join(&result.relative_path)).unwrap(), text);
        let saved = db::get(&state.db, 1).await.unwrap();
        assert_eq!(saved.repository_uid, repository.repository_uid);
        let saved_binding = saved.obsidian.unwrap();
        assert_eq!(saved_binding.relative_path, result.relative_path);
        assert!(reqwest::Url::parse(&saved_binding.obsidian_uri)
            .unwrap()
            .query_pairs()
            .any(|(key, value)| key == "id" && value == binding.note_id.as_deref().unwrap()));
        for name in ["../escape", "nested/name", "", "CON", "file#heading"] {
            assert!(renamed_relative("notes/original.md", name).is_err());
        }
    }

    #[test]
    fn property_display_uses_yaml_without_changing_file_bytes() {
        let original = "---\n\"skillshub_tags\": [\"one\", \"two\"]\n\"skillshub_stars\": 42\n---\nbody";
        let file = snapshot(original, "note.md").unwrap();
        assert!(file.properties.contains("skillshub_tags:\n- one\n- two"));
        assert!(file.properties.contains("skillshub_stars: 42"));
        assert_eq!(file.body, "body");
        assert_eq!(file.revision, revision(original));
    }

    #[tokio::test]
    async fn legacy_migrates_losslessly_and_note_id_survives_vault_move_and_stale_path_collision() {
        let dir = tempfile::tempdir().unwrap();
        let root = std::fs::canonicalize(dir.path()).unwrap();
        let state = AppState {
            db: db::create_pool(&root.join("test.sqlite")).await.unwrap(),
        };
        let mut repo = db::Repository::new(101, "fixture/identity".into());
        repo.obsidian = Some(binding_for_path(&root, "old.md").unwrap());
        let original=format!("\u{feff}---\r\n# keep comment\r\nskillshub_repository_id: {}\r\ncustom: 'keep me'\r\n---\r\n# Body\r\n",repo.repository_uid);
        std::fs::write(root.join("old.md"), &original).unwrap();
        db::put(&state.db, &repo).await.unwrap();
        let (path, binding) = resolve_in_vault(&state, &mut repo, &root).await.unwrap();
        assert_eq!(binding.binding_version, 2);
        assert!(validate_binding(&binding));
        let id = binding.note_id.clone().unwrap();
        let stable = binding.obsidian_uri.clone();
        let migrated = read_file(&path).unwrap();
        assert_eq!(parts(&migrated).unwrap().2, parts(&original).unwrap().2);
        assert!(migrated.contains("# keep comment\r\n"));
        assert!(migrated.contains("custom: 'keep me'"));
        assert!(migrated.starts_with('\u{feff}'));
        assert_eq!(note_id(&migrated).unwrap(), Some(id.clone()));
        let other = tempfile::tempdir().unwrap();
        let other_root = std::fs::canonicalize(other.path()).unwrap();
        std::fs::create_dir(other_root.join("Moved")).unwrap();
        std::fs::write(other_root.join("Moved/renamed.md"), &migrated).unwrap();
        // A different file at the cached old path must never be adopted.
        std::fs::write(other_root.join("old.md"), "unrelated body").unwrap();
        let (_, relocated) = resolve_in_vault(&state, &mut repo, &other_root)
            .await
            .unwrap();
        assert_eq!(relocated.relative_path, "Moved/renamed.md");
        assert_eq!(relocated.obsidian_uri, stable);
        assert_eq!(relocated.note_id, Some(id.clone()));
        assert_eq!(
            read_file(&other_root.join("old.md")).unwrap(),
            "unrelated body"
        );
        // Copied note identities are ambiguous even when the cached target still exists.
        std::fs::write(other_root.join("duplicate.md"), &migrated).unwrap();
        invalidate_index(&other_root);
        assert_eq!(
            resolve_in_vault(&state, &mut repo, &other_root)
                .await
                .unwrap_err(),
            "noteLocate"
        );
        std::fs::remove_file(other_root.join("duplicate.md")).unwrap();
        invalidate_index(&other_root);
        std::fs::write(
            other_root.join("Moved/renamed.md"),
            migrated.replace(&id, "00000000-0000-4000-8000-000000000099"),
        )
        .unwrap();
        invalidate_index(&other_root);
        assert_eq!(
            resolve_in_vault(&state, &mut repo, &other_root)
                .await
                .unwrap_err(),
            "noteMissing"
        );
    }

    #[test]
    fn metadata_preserves_body_user_properties_comments_and_types() {
        let mut repo = db::Repository::new(1, "example/project".into());
        repo.stars = Some(42);
        repo.tags = vec!["中文".into(), "x: y".into()];
        let original = "\u{feff}---\r\n# user comment\r\ncustom: 'keep me'\r\nlist: [a, b]\r\n---\r\n# Body\r\nunchanged\r\n";
        let next = properties(original, &repo, &[]).unwrap();
        assert_eq!(parts(&next).unwrap().2, parts(original).unwrap().2);
        assert!(next.contains("# user comment"));
        assert!(next.contains("custom: 'keep me'"));
        let values = yaml(parts(&next).unwrap().1).unwrap();
        assert_eq!(values["skillshub_stars"].as_i64(), Some(42));
        assert_eq!(values["skillshub_tags"][0].as_str(), Some("中文"));
        assert_eq!(
            values["skillshub_uri"].as_str(),
            Some(format!("skillshub://repository/{}?tab=notebook", repo.repository_uid).as_str())
        );
        repo.stars = None;
        let next = properties(&next, &repo, &[]).unwrap();
        assert!(!yaml(parts(&next).unwrap().1)
            .unwrap()
            .contains_key("skillshub_stars"));
    }

    #[test]
    fn rejects_invalid_yaml_duplicate_keys_and_other_identity() {
        let repo = db::Repository::new(1, "example/project".into());
        for text in [
            "---\na: 1\na: 2\n---\nbody",
            "---\na: [\n---\nbody",
            "---\nskillshub_repository_id: other\n---\nbody",
        ] {
            assert!(properties(text, &repo, &[]).is_err());
        }
    }

    #[test]
    fn writes_atomically_and_rejects_stale_revision_and_existing_create() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("note.md");
        write_checked(&path, None, "first").unwrap();
        assert_eq!(
            write_checked(&path, None, "overwrite").unwrap_err(),
            "noteConflict"
        );
        let old = revision("first");
        std::fs::write(&path, "external").unwrap();
        assert_eq!(
            write_checked(&path, Some(&old), "draft").unwrap_err(),
            "noteConflict"
        );
        assert_eq!(read_file(&path).unwrap(), "external");
        write_checked(&path, Some(&revision("external")), "saved").unwrap();
        assert_eq!(read_file(&path).unwrap(), "saved");
    }

    #[test]
    fn validates_paths_and_navigation() {
        let dir = tempfile::tempdir().unwrap();
        let root = std::fs::canonicalize(dir.path()).unwrap();
        for path in [
            "../outside.md",
            "a/../b.md",
            ".obsidian/config.md",
            "/absolute.md",
            "a//b.md",
            "x:stream.md",
        ] {
            assert!(!valid_relative(path), "{path}");
        }
        let path = safe_path(&root, "notes/中文.md", true).unwrap();
        std::fs::write(&path, "ok").unwrap();
        assert_eq!(
            relative_input(&root, &dunce::simplified(&path).to_string_lossy()).unwrap(),
            "notes/中文.md"
        );
        let uid = "00000000-0000-4000-8000-000000000001";
        assert_eq!(
            navigation_uid(&format!("skillshub://repository/{uid}?tab=notebook")),
            Some(uid.into())
        );
        for uri in [
            "skillshub://repository/1",
            "skillshub://repository/../../file",
            "https://repository/00000000-0000-4000-8000-000000000001",
            "skillshub://repository/00000000-0000-4000-8000-000000000001?tab=other",
        ] {
            assert!(navigation_uid(uri).is_none());
        }
    }
}
#[test]
fn yaml_templates_keep_types_escape_values_and_protect_identity() {
    let mut repo = db::Repository::new(1, "example/project".into());
    repo.description = "Quotes: \"hello\"\ncustom: injected".into();
    repo.tags = vec!["a:b".into(), "space tag".into()];
    repo.stars = Some(42);
    let template = "---\nname: {{fullName}}\ndescription: {{description}}\ntags: {{tags}}\nstars: {{stars}}\nstatus: draft\nskillshub_repository_id: overridden\nskillshub_uri: https://example.com\n---\n";
    let original = "---\n# user comment\ncustom: keep\nstatus: old\n---\n# Body\n";
    let result = properties_with_template(original, &repo, &[], template).unwrap();
    let (_, front, body) = parts(&result).unwrap();
    let data = yaml(front).unwrap();
    assert_eq!(body, "# Body\n");
    assert!(front.contains("# user comment"));
    assert_eq!(
        data["description"].as_str(),
        Some(repo.description.as_str())
    );
    assert_eq!(data["tags"].as_sequence().unwrap().len(), 2);
    assert_eq!(data["stars"].as_u64(), Some(42));
    assert_eq!(data["custom"].as_str(), Some("keep"));
    assert_eq!(data["status"].as_str(), Some("draft"));
    assert_eq!(
        data["skillshub_repository_id"].as_str(),
        Some(repo.repository_uid.as_str())
    );
    assert_eq!(
        data["skillshub_uri"].as_str(),
        Some(format!("skillshub://repository/{}?tab=notebook", repo.repository_uid).as_str())
    );
    assert!(front.contains("name: example/project"));
    for invalid in [
        "name: {{fullName}}",
        "---\nname: {{unknown}}\n---",
        "---\nname: {{fullName}}\nname: duplicate\n---",
        "---\nname: \"{{fullName}}\"\n---",
        "---\nname: value\n---\nBody",
        "---\n- list\n---",
    ] {
        assert!(
            validate_template(invalid).is_err(),
            "unexpected valid template"
        );
    }
    assert!(validate_template(&default_template()).is_ok());
}

#[cfg(test)]
mod folder_tests {
    use super::*;
    #[test]
    fn normalizes_vault_relative_directories_and_rejects_escape() {
        for input in [r"\附件\SkillsHub", "附件/SkillsHub", "/附件/SkillsHub/", r"附件\SkillsHub\"] {
            assert_eq!(normalize_note_folder(input).unwrap(), "附件/SkillsHub");
        }
        assert_eq!(normalize_note_folder("").unwrap(), "SkillsHub");
        for input in [r"..\outside", "a/../outside", "./SkillsHub", r"C:\outside", "CON", ".obsidian"] {
            assert!(normalize_note_folder(input).is_err());
        }
        let root = tempfile::tempdir().unwrap();
        let folder = normalize_note_folder(r"\附件\SkillsHub").unwrap();
        let canonical_root = std::fs::canonicalize(root.path()).unwrap();
        let path = safe_path(&canonical_root, &format!("{folder}/project@example.md"), true).unwrap();
        write_checked(&path, None, "# Fixture").unwrap();
        assert_eq!(read_file(&root.path().join("附件/SkillsHub/project@example.md")).unwrap(), "# Fixture");
    }
}
