use super::github_import::{
    self as github, GitHubRepoRef, GitHubRepoSnapshot, RemoteSkillCandidate,
};
use crate::db::AppStore;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, HashMap, HashSet};

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct ImportRules {
    // Logical name -> explicitly retained source directories. Empty means excluded.
    pub groups: BTreeMap<String, Vec<String>>,
}

fn name(candidate: &RemoteSkillCandidate) -> String {
    candidate.skill_name.trim().to_lowercase()
}

fn preference(candidate: &RemoteSkillCandidate) -> (u8, usize, &str) {
    let path = candidate.source_path.as_str();
    let rank = if path.starts_with("skills/") {
        0
    } else if path.starts_with(".agents/skills/") {
        1
    } else {
        2
    };
    (rank, path.matches('/').count(), path)
}

// Compare the complete payload that will actually be installed, including scripts and assets.
// A shared name or identical SKILL.md alone is not sufficient evidence of duplication.
pub(crate) fn deduplicate(
    snapshot: &GitHubRepoSnapshot,
    mut candidates: Vec<RemoteSkillCandidate>,
) -> Result<Vec<RemoteSkillCandidate>, String> {
    candidates.sort_by(|a, b| preference(a).cmp(&preference(b)));
    let mut seen: HashMap<(String, String), usize> = HashMap::new();
    let mut retained: Vec<RemoteSkillCandidate> = Vec::new();
    for candidate in candidates {
        let mut hash = Sha256::new();
        for (path, bytes) in github::mirror_skill_files(snapshot, &candidate.source_path)? {
            hash.update((path.len() as u64).to_le_bytes());
            hash.update(path.as_bytes());
            hash.update((bytes.len() as u64).to_le_bytes());
            hash.update(bytes);
        }
        let key = (name(&candidate), format!("{:x}", hash.finalize()));
        if let Some(&index) = seen.get(&key) {
            retained[index].duplicate_paths.push(candidate.source_path);
        } else {
            seen.insert(key, retained.len());
            retained.push(candidate);
        }
    }
    Ok(retained)
}

pub(crate) fn candidates(
    repo: &GitHubRepoRef,
    snapshot: &GitHubRepoSnapshot,
) -> Result<Vec<RemoteSkillCandidate>, String> {
    deduplicate(
        snapshot,
        github::build_repo_skill_candidates_from_snapshot(repo, snapshot)?,
    )
}

pub(crate) fn selected_rules(
    all: &[RemoteSkillCandidate],
    selected: &HashSet<String>,
) -> ImportRules {
    let mut rules = ImportRules::default();
    for candidate in all {
        let paths = rules.groups.entry(name(candidate)).or_default();
        if selected.contains(&candidate.source_path) {
            paths.push(candidate.source_path.clone());
        }
    }
    rules
}

pub(crate) async fn load(pool: &AppStore, repository: &str) -> Result<Option<ImportRules>, String> {
    pool.read(|data| {
        data.table("repository_mirrors")
            .get(&repository.to_lowercase())
            .and_then(|row| row.get("import_rules"))
            .filter(|value| !value.is_null())
            .map(|value| {
                serde_json::from_value(value.clone())
                    .map_err(|_| "Invalid repository import rules".into())
            })
            .transpose()
    })
}

pub(crate) fn update_candidates(
    repo: &GitHubRepoRef,
    snapshot: &GitHubRepoSnapshot,
    rules: Option<&ImportRules>,
) -> Result<Vec<RemoteSkillCandidate>, String> {
    let all = github::build_repo_skill_candidates_from_snapshot(repo, snapshot)?;
    let Some(rules) = rules else {
        return Ok(all);
    }; // Existing repositories retain their current behavior.
    let mut retained = Vec::new();
    let mut new_names = Vec::new();
    for candidate in all {
        match rules.groups.get(&name(&candidate)) {
            Some(paths) if paths.contains(&candidate.source_path) => retained.push(candidate),
            Some(_) => {}
            None => new_names.push(candidate),
        }
    }
    // A removed chosen source requires a new import choice, never a silent switch to a variant.
    for paths in rules.groups.values().filter(|paths| !paths.is_empty()) {
        if paths.iter().any(|path| {
            !retained
                .iter()
                .any(|candidate| &candidate.source_path == path)
        }) {
            return Err("IMPORT_SOURCE_MISSING".into());
        }
    }
    retained.extend(deduplicate(snapshot, new_names)?);
    Ok(retained)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> (GitHubRepoRef, GitHubRepoSnapshot) {
        let repo = GitHubRepoRef {
            owner: "example".into(),
            repo: "tools".into(),
            branch: "a".repeat(40),
            normalized_url: String::new(),
            stars: None,
        };
        let mut snapshot = GitHubRepoSnapshot::default();
        for root in ["skills/shared", "plugins/demo/skills/shared"] {
            snapshot.files.insert(
                format!("{root}/SKILL.md"),
                b"---\nname: shared\ndescription: demo\n---\nDemo".to_vec(),
            );
            snapshot
                .files
                .insert(format!("{root}/scripts/run.py"), b"print('demo')".to_vec());
        }
        (repo, snapshot)
    }
    #[test]
    fn exact_copies_collapse_but_different_resources_remain_variants() {
        let (repo, mut snapshot) = fixture();
        let kept = candidates(&repo, &snapshot).unwrap();
        assert_eq!(kept.len(), 1);
        assert_eq!(kept[0].source_path, "skills/shared/SKILL.md");
        assert_eq!(
            kept[0].duplicate_paths,
            vec!["plugins/demo/skills/shared/SKILL.md"]
        );
        snapshot.files.insert(
            "plugins/demo/skills/shared/scripts/run.py".into(),
            b"different".to_vec(),
        );
        assert_eq!(candidates(&repo, &snapshot).unwrap().len(), 2);
    }
    #[test]
    fn saved_choice_excludes_copies_even_after_they_diverge() {
        let (repo, mut snapshot) = fixture();
        let imports = candidates(&repo, &snapshot).unwrap();
        let rules = selected_rules(&imports, &HashSet::from(["skills/shared/SKILL.md".into()]));
        snapshot.files.insert(
            "plugins/demo/skills/shared/scripts/run.py".into(),
            b"changed distribution".to_vec(),
        );
        let update = update_candidates(&repo, &snapshot, Some(&rules)).unwrap();
        assert_eq!(update.len(), 1);
        assert_eq!(update[0].source_path, "skills/shared/SKILL.md");
        snapshot
            .files
            .retain(|path, _| !path.starts_with("skills/shared/"));
        assert_eq!(
            update_candidates(&repo, &snapshot, Some(&rules)).unwrap_err(),
            "IMPORT_SOURCE_MISSING"
        );
    }
    #[test]
    fn unconfigured_repositories_are_not_deduplicated_or_migrated() {
        let (repo, snapshot) = fixture();
        assert_eq!(update_candidates(&repo, &snapshot, None).unwrap().len(), 2);
    }
}
