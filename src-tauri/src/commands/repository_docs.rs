//! Repository-root README variants shared by imports, updates and preview.
use serde::Serialize;
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

pub(crate) fn language(name: &str) -> Option<&'static str> {
    if name.contains(['/', '\\']) {
        return None;
    }
    let lower = name.to_ascii_lowercase();
    let stem = lower
        .strip_suffix(".md")
        .or_else(|| lower.strip_suffix(".markdown"))?;
    let suffix = stem.strip_prefix("readme")?;
    if suffix.is_empty() {
        return Some("default");
    }
    if !suffix.starts_with(['.', '_', '-']) {
        return None;
    }
    let suffix = suffix[1..].replace(['.', '_'], "-");
    match suffix.as_str() {
        "zh" | "cn" | "zh-cn" | "zh-tw" | "zh-hans" | "zh-hant" | "chinese" => Some("zh"),
        "en" | "en-us" | "en-gb" | "english" => Some("en"),
        _ => None,
    }
}

pub(crate) fn canonical_name(name: &str) -> String {
    if name.eq_ignore_ascii_case("README.md") {
        "README.md".into()
    } else {
        name.into()
    }
}

pub(crate) fn paths(directory: &Path) -> Result<Vec<PathBuf>, String> {
    if !directory.exists() {
        return Ok(Vec::new());
    }
    let mut paths = Vec::new();
    for entry in std::fs::read_dir(directory)
        .map_err(|e| format!("Failed to inspect repository documents: {e}"))?
    {
        let entry = entry.map_err(|e| e.to_string())?;
        if language(&entry.file_name().to_string_lossy()).is_some()
            && entry.file_type().map_err(|e| e.to_string())?.is_file()
        {
            paths.push(entry.path());
        }
    }
    paths.sort();
    Ok(paths)
}

pub(crate) fn files(directory: &Path) -> Result<BTreeMap<String, Vec<u8>>, String> {
    paths(directory)?
        .into_iter()
        .map(|path| {
            let name = canonical_name(&path.file_name().unwrap().to_string_lossy());
            std::fs::read(path)
                .map(|bytes| (name, bytes))
                .map_err(|e| format!("Failed to read repository document: {e}"))
        })
        .collect()
}

// Keep the same repository grouping as the shared frontend browser.
pub(crate) fn group_directory(root: &Path, skill_directory: &Path) -> Option<PathBuf> {
    let relative = skill_directory.strip_prefix(root).ok()?;
    let parts = relative.components().collect::<Vec<_>>();
    if parts.len() < 2 {
        return None;
    }
    let count = if parts.len() >= 3 { 2 } else { 1 };
    Some(
        parts[..count]
            .iter()
            .fold(root.to_path_buf(), |path, part| path.join(part.as_os_str())),
    )
}

#[derive(Serialize)]
pub struct RepositoryDocument {
    pub name: String,
    pub language: &'static str,
    pub content: String,
}

pub(crate) fn read(directory: &Path) -> Result<Vec<RepositoryDocument>, String> {
    let mut documents = files(directory)?
        .into_iter()
        .map(|(name, bytes)| {
            let language = language(&name).unwrap();
            String::from_utf8(bytes)
                .map(|content| RepositoryDocument {
                    name,
                    language,
                    content,
                })
                .map_err(|_| "Repository README is not valid UTF-8".to_string())
        })
        .collect::<Result<Vec<_>, _>>()?;
    documents.sort_by_key(|doc| {
        (
            match doc.language {
                "default" => 0,
                "zh" => 1,
                _ => 2,
            },
            !doc.name.eq_ignore_ascii_case("README.md"),
            doc.name.clone(),
        )
    });
    Ok(documents)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn recognizes_default_and_chinese_english_variants_only_at_root() {
        for name in [
            "README.md",
            "readme.MD",
            "README.zh.md",
            "README_CN.md",
            "README.zh-CN.md",
            "README_zh_CN.md",
            "README.en.md",
            "README_EN.md",
        ] {
            assert!(language(name).is_some(), "{name}");
        }
        for name in [
            "docs/README.md",
            "README.fr.md",
            "READMEevil.md",
            "README.zh.md/other",
        ] {
            assert!(language(name).is_none(), "{name}");
        }
    }
}
