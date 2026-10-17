use crate::AppState;
use base64::{engine::general_purpose::STANDARD, Engine};
use std::{
    collections::BTreeMap,
    fs,
    path::{Component, Path},
};
use tauri::State;
const BUNDLED: &[(&str, &[u8])] = &[
    (
        "antigravity.svg",
        include_bytes!("../resources/default-platforms/icons/antigravity.svg"),
    ),
    (
        "claude-code.svg",
        include_bytes!("../resources/default-platforms/icons/claude-code.svg"),
    ),
    (
        "cline.svg",
        include_bytes!("../resources/default-platforms/icons/cline.svg"),
    ),
    (
        "codearts-agent.png",
        include_bytes!("../resources/default-platforms/icons/codearts-agent.png"),
    ),
    (
        "codebuddy.svg",
        include_bytes!("../resources/default-platforms/icons/codebuddy.svg"),
    ),
    (
        "codex.svg",
        include_bytes!("../resources/default-platforms/icons/codex.svg"),
    ),
    (
        "copilot.svg",
        include_bytes!("../resources/default-platforms/icons/copilot.svg"),
    ),
    (
        "cursor.svg",
        include_bytes!("../resources/default-platforms/icons/cursor.svg"),
    ),
    (
        "gemini-cli.svg",
        include_bytes!("../resources/default-platforms/icons/gemini-cli.svg"),
    ),
    (
        "hermes.svg",
        include_bytes!("../resources/default-platforms/icons/hermes.svg"),
    ),
    (
        "kimi-code-cli.svg",
        include_bytes!("../resources/default-platforms/icons/kimi-code-cli.svg"),
    ),
    (
        "kiro.svg",
        include_bytes!("../resources/default-platforms/icons/kiro.svg"),
    ),
    (
        "LICENSE-LobeHub.txt",
        include_bytes!("../resources/default-platforms/icons/LICENSE-LobeHub.txt"),
    ),
    (
        "openclaw.svg",
        include_bytes!("../resources/default-platforms/icons/openclaw.svg"),
    ),
    (
        "opencode.svg",
        include_bytes!("../resources/default-platforms/icons/opencode.svg"),
    ),
    (
        "pi.svg",
        include_bytes!("../resources/default-platforms/icons/pi.svg"),
    ),
    (
        "qoder.svg",
        include_bytes!("../resources/default-platforms/icons/qoder.svg"),
    ),
    (
        "qwen.svg",
        include_bytes!("../resources/default-platforms/icons/qwen.svg"),
    ),
    (
        "SOURCES.md",
        include_bytes!("../resources/default-platforms/icons/SOURCES.md"),
    ),
    (
        "trae-cn.svg",
        include_bytes!("../resources/default-platforms/icons/trae-cn.svg"),
    ),
    (
        "trae.svg",
        include_bytes!("../resources/default-platforms/icons/trae.svg"),
    ),
    (
        "warp.png",
        include_bytes!("../resources/default-platforms/icons/warp.png"),
    ),
    (
        "windsurf.svg",
        include_bytes!("../resources/default-platforms/icons/windsurf.svg"),
    ),
    (
        "workbuddy.svg",
        include_bytes!("../resources/default-platforms/icons/workbuddy.svg"),
    ),
];
pub fn seed(root: &Path) -> Result<(), String> {
    fs::create_dir_all(root.join("icons")).map_err(|_| "Cannot create icons directory")?;
    for (name, data) in BUNDLED {
        let path = root.join("icons").join(name);
        if !path.exists() {
            fs::write(path, data).map_err(|_| "Cannot seed platform icon")?;
        }
    }
    let config_path = root.join("config.json");
    let config = crate::config_store::load_path(&config_path)?;
    let defaults = crate::platforms::default_platform_definitions()?;
    let mut updates = BTreeMap::new();
    for platform in &config.platforms {
        if platform
            .icon
            .as_deref()
            .is_some_and(|path| path.starts_with("icons/"))
        {
            continue;
        }
        // Preserve icons from the previous implicit-by-ID directory layout.
        let legacy = ["svg", "png", "webp"]
            .iter()
            .map(|ext| {
                root.join("platform/icons")
                    .join(format!("{}.{}", platform.id, ext))
            })
            .find(|p| p.is_file());
        let icon = if let Some(path) = legacy {
            let name = format!(
                "icons/custom-{}.{}",
                uuid::Uuid::new_v4(),
                path.extension().unwrap().to_string_lossy()
            );
            fs::copy(path, root.join(&name)).map_err(|_| "Cannot preserve platform icon")?;
            Some(name)
        } else {
            defaults
                .iter()
                .find(|p| p.id == platform.id)
                .and_then(|p| p.icon.clone())
        };
        if icon.is_some() {
            updates.insert(platform.id.clone(), icon);
        }
    }
    if !updates.is_empty() {
        crate::config_store::update_path(&config_path, |config| {
            for p in &mut config.platforms {
                if let Some(icon) = updates.get(&p.id) {
                    p.icon = icon.clone();
                }
            }
            Ok(())
        })?;
    }
    Ok(())
}
fn read_icons(root: &Path) -> Result<BTreeMap<String, String>, String> {
    seed(root)?;
    let config = crate::config_store::load_path(&root.join("config.json"))?;
    let mut result = BTreeMap::new();
    for platform in config.platforms {
        let Some(icon) = platform.icon else {
            continue;
        };
        let path = Path::new(&icon);
        if !icon.starts_with("icons/")
            || path
                .components()
                .any(|c| !matches!(c, Component::Normal(_)))
        {
            continue;
        }
        let Ok(data) = fs::read(root.join(path)) else {
            continue;
        };
        let mime = match path.extension().and_then(|s| s.to_str()) {
            Some("svg") => "image/svg+xml",
            Some("png") => "image/png",
            Some("webp") => "image/webp",
            _ => continue,
        };
        if data.len() <= 2 * 1024 * 1024 {
            result.insert(
                platform.id,
                format!("data:{};base64,{}", mime, STANDARD.encode(data)),
            );
        }
    }
    Ok(result)
}
#[tauri::command]
pub async fn get_platform_icons(
    state: State<'_, AppState>,
) -> Result<BTreeMap<String, String>, String> {
    let path = crate::db::config_path(&state.db)?;
    read_icons(path.parent().ok_or("Invalid config directory")?)
}
fn save_icon(root: &Path, agent_id: &str, data_url: Option<&str>) -> Result<(), String> {
    let config_path = root.join("config.json");
    let config = crate::config_store::load_path(&config_path)?;
    if !config.platforms.iter().any(|p| p.id == agent_id) {
        return Err("Platform not found".into());
    }
    let icon = if let Some(url) = data_url {
        let (header, body) = url.split_once(",").ok_or("Invalid image")?;
        let ext = match header {
            "data:image/png;base64" => "png",
            "data:image/webp;base64" => "webp",
            "data:image/svg+xml;base64" => "svg",
            _ => return Err("Use PNG, SVG or WebP".into()),
        };
        if body.len() > 3 * 1024 * 1024 {
            return Err("Icon exceeds 2 MB".into());
        }
        let bytes = STANDARD.decode(body).map_err(|_| "Invalid image")?;
        if bytes.len() > 2 * 1024 * 1024 {
            return Err("Icon exceeds 2 MB".into());
        }
        let valid = match ext {
            "png" => bytes.starts_with(b"\x89PNG\r\n\x1a\n"),
            "webp" => bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(b"WEBP"),
            _ => std::str::from_utf8(&bytes).is_ok_and(|s| {
                s.contains("<svg")
                    && !s.to_lowercase().contains("<script")
                    && !s.to_lowercase().contains("<foreignobject")
            }),
        };
        if !valid {
            return Err("Invalid image content".into());
        }
        fs::create_dir_all(root.join("icons")).map_err(|_| "Cannot create icons directory")?;
        let name = format!("icons/custom-{}.{}", uuid::Uuid::new_v4(), ext);
        fs::write(root.join(&name), bytes).map_err(|_| "Cannot save icon")?;
        Some(name)
    } else {
        crate::platforms::default_platform_definitions()?
            .iter()
            .find(|p| p.id == agent_id)
            .and_then(|p| p.icon.clone())
    };
    crate::config_store::update_path(&config_path, |config| {
        let platform = config
            .platforms
            .iter_mut()
            .find(|p| p.id == agent_id)
            .ok_or("Platform not found")?;
        platform.icon = icon;
        Ok(())
    })
}
#[tauri::command]
pub async fn set_platform_icon(
    state: State<'_, AppState>,
    agent_id: String,
    data_url: Option<String>,
) -> Result<(), String> {
    let path = crate::db::config_path(&state.db)?;
    save_icon(
        path.parent().ok_or("Invalid config directory")?,
        &agent_id,
        data_url.as_deref(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bundled_icons_and_custom_icons_survive_reseeding() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        crate::config_store::update_path(&root.join("config.json"), |_| Ok(())).unwrap();
        seed(root).unwrap();
        let icons = read_icons(root).unwrap();
        assert_eq!(icons.len(), 22);
        let data = "data:image/svg+xml;base64,".to_owned()
            + &STANDARD.encode(b"<svg xmlns='http://www.w3.org/2000/svg'/>");
        save_icon(root, "cursor", Some(&data)).unwrap();
        seed(root).unwrap();
        assert_eq!(read_icons(root).unwrap()["cursor"], data);
        let config = crate::config_store::load_path(&root.join("config.json")).unwrap();
        assert!(config
            .platforms
            .iter()
            .find(|p| p.id == "cursor")
            .unwrap()
            .icon
            .as_ref()
            .unwrap()
            .starts_with("icons/custom-"));
        save_icon(root, "cursor", None).unwrap();
        assert_eq!(read_icons(root).unwrap()["cursor"], icons["cursor"]);
    }
    #[test]
    fn migrates_legacy_custom_icon_without_deleting_original() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        crate::config_store::update_path(&root.join("config.json"), |c| {
            c.platforms
                .iter_mut()
                .find(|p| p.id == "cursor")
                .unwrap()
                .icon = None;
            Ok(())
        })
        .unwrap();
        fs::create_dir_all(root.join("platform/icons")).unwrap();
        fs::write(root.join("platform/icons/cursor.svg"), b"<svg/>").unwrap();
        seed(root).unwrap();
        assert!(root.join("platform/icons/cursor.svg").exists());
        assert_eq!(
            read_icons(root).unwrap()["cursor"],
            format!("data:image/svg+xml;base64,{}", STANDARD.encode(b"<svg/>"))
        );
    }
    #[test]
    fn rejects_invalid_upload_without_changing_config() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        crate::config_store::update_path(&root.join("config.json"), |_| Ok(())).unwrap();
        let before = fs::read(root.join("config.json")).unwrap();
        assert!(save_icon(root, "cursor", Some("data:image/png;base64,bm90IGltYWdl")).is_err());
        assert_eq!(fs::read(root.join("config.json")).unwrap(), before);
    }
}
