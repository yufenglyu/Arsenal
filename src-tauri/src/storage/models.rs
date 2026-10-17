//! Domain DTOs independent of the persistence engine.
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Skill {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub file_path: String,
    pub canonical_path: Option<String>,
    pub is_central: bool,
    pub source: Option<String>,
    pub content: Option<String>,
    pub scanned_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillSource {
    pub skill_id: String,
    pub source_type: String,
    pub source_url: Option<String>,
    pub source_author: Option<String>,
    pub source_repo: Option<String>,
    pub source_path: Option<String>,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillSourceSync {
    pub skill_id: String,
    pub sync_scope: String,
    pub remote_ref: Option<String>,
    pub skill_fingerprint: Option<String>,
    pub last_checked_at: Option<String>,
    pub last_sync_at: Option<String>,
    pub sync_status: String,
    pub sync_error: Option<String>,
    pub remote_deleted: bool,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillMetadata {
    pub skill_id: String,
    pub notes: Option<String>,
    pub tags: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillInstallation {
    pub skill_id: String,
    pub agent_id: String,
    pub installed_path: String,
    pub link_type: String,
    pub symlink_target: Option<String>,
    /// ISO 8601 timestamp of when the skill was first installed.
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentSkillObservation {
    pub row_id: String,
    pub agent_id: String,
    pub skill_id: String,
    pub name: String,
    pub description: Option<String>,
    pub file_path: String,
    pub dir_path: String,
    pub source_kind: String,
    pub source_root: String,
    pub link_type: String,
    pub symlink_target: Option<String>,
    pub is_read_only: bool,
    pub scanned_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Agent {
    pub id: String,
    pub display_name: String,
    pub category: String,
    pub global_skills_dir: String,
    pub project_skills_dir: Option<String>,
    pub is_detected: bool,
    pub is_builtin: bool,
    pub is_enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Collection {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanDirectory {
    pub id: i64,
    pub path: String,
    pub label: Option<String>,
    pub is_active: bool,
    pub is_builtin: bool,
    pub added_at: String,
}

/// A skill enriched with installation and source fields for a platform view.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillForAgent {
    /// Explicit installation record versus Shared Hub availability.
    pub installation_source: String,
    pub id: String,
    /// Stable row identity for list/detail routing.
    pub row_id: String,
    pub name: String,
    pub description: Option<String>,
    /// Absolute path to the `SKILL.md` file.
    pub file_path: String,
    /// Absolute path to the skill directory as installed for this agent
    /// (i.e., `skill_installations.installed_path`).
    pub dir_path: String,
    /// How the skill is linked: "symlink", "copy", or "native".
    pub link_type: String,
    /// Symlink target path, if `link_type` is "symlink".
    pub symlink_target: Option<String>,
    pub is_central: bool,
    /// Logical origin of the managed skill (resource library, GitHub, etc.).
    pub source: Option<String>,
    pub source_kind: Option<String>,
    pub source_root: Option<String>,
    pub is_read_only: bool,
    pub conflict_group: Option<String>,
    pub conflict_count: i64,
    pub source_url: Option<String>,
    pub source_author: Option<String>,
    pub source_repo: Option<String>,
    pub source_path: Option<String>,
    pub created_at: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FolderNote {
    pub id: String,
    pub notes: String,
    pub skill_ids: Vec<String>,
}

#[derive(Serialize)]
pub struct MetadataRow {
    pub id: String,
    pub notes: Option<String>,
    pub tags: Vec<String>,
}
