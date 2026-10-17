// ─── Agent Types ─────────────────────────────────────────────────────────────

export interface AgentWithStatus {
  id: string;
  display_name: string;
  global_skills_dir: string;
  project_skills_dir?: string;
  is_detected: boolean;
  is_builtin: boolean;
  is_enabled: boolean;
  shares_central_skills?: boolean;
}

export interface CustomAgentConfig {
  id?: string;
  display_name: string;
  global_skills_dir: string;
}

export interface UpdateCustomAgentConfig {
  id?: string;
  display_name: string;
  global_skills_dir: string;
}

export interface BackupOptions {
  includeResourceLibrary: boolean;
  includeCentralLibrary: boolean;
  includeAppConfig: boolean;
  includeInstallations: boolean;
}

export interface WebDavConfig {
  baseUrl: string;
  username?: string | null;
  password?: string | null;
  remoteDir: string;
}

export interface WebDavBackupFile {
  name: string;
  remotePath: string;
  size?: number | null;
  modifiedAt?: string | null;
}

export interface AppUpdateInfo {
  currentVersion: string;
  latestVersion: string;
  latestUrl: string;
  isUpdateAvailable: boolean;
  releaseName?: string | null;
  publishedAt?: string | null;
}

// ─── Scan Types ───────────────────────────────────────────────────────────────

export interface ScanResult {
  total_skills: number;
  agents_scanned: number;
  skills_by_agent: Record<string, number>;
}

export type PlatformSourceKind = "compatibility" | "shared-central";

export interface ScannedSkill {
  installation_source?: "independent" | "shared";
  id: string;
  row_id?: string;
  name: string;
  description?: string;
  file_path: string;
  dir_path: string;
  link_type: string;
  symlink_target?: string;
  is_central: boolean;
  source?: string | null;
  source_kind?: PlatformSourceKind | null;
  source_root?: string | null;
  is_read_only?: boolean;
  conflict_group?: string | null;
  conflict_count?: number;
  source_url?: string | null;
  source_author?: string | null;
  source_repo?: string | null;
  source_path?: string | null;
  created_at?: string | null;
  updated_at?: string | null;
}

// ─── Skill Types ──────────────────────────────────────────────────────────────

export interface Skill {
  id: string;
  name: string;
  description?: string;
  file_path: string;
  canonical_path?: string;
  is_central: boolean;
  source?: string;
  source_url?: string | null;
  source_author?: string | null;
  source_repo?: string | null;
  source_path?: string | null;
  notes?: string | null;
  tags?: string[];
  content?: string;
  scanned_at: string;
  created_at?: string;
  updated_at?: string;
}

export interface SkillInstallation {
  skill_id: string;
  agent_id: string;
  installed_path: string;
  link_type: string;
  symlink_target?: string;
  /** ISO 8601 timestamp of when the skill was first installed. */
  installed_at?: string;
}

export interface SkillDetail extends Omit<Skill, "content"> {
  row_id?: string;
  dir_path?: string;
  source_kind?: PlatformSourceKind | null;
  source_root?: string | null;
  is_read_only?: boolean;
  conflict_group?: string | null;
  conflict_count?: number;
  /** Agent IDs that can see this central skill through a read-only compatibility root. */
  read_only_agents?: string[];
  installations: SkillInstallation[];
  /** Collections this skill currently belongs to. */
  collections?: Collection[];
}

export interface SkillDirectoryNode {
  name: string;
  path: string;
  relative_path: string;
  is_dir: boolean;
  children: SkillDirectoryNode[];
}

export interface SkillDetailRequest {
  skillId: string;
  agentId?: string;
  rowId?: string;
}

export interface SkillSourceMetadataUpdate {
  sourceType: string;
  sourceUrl?: string | null;
  sourceAuthor?: string | null;
  sourceRepo?: string | null;
  sourcePath?: string | null;
}

export interface SkillSourceMetadataResponse {
  skill_id: string;
  source?: string | null;
  source_type: string;
  source_url?: string | null;
  source_author?: string | null;
  source_repo?: string | null;
  source_path?: string | null;
  updated_at: string;
}

export interface CreateManualResourceSkillInput {
  skillId: string;
  name: string;
  description?: string | null;
  body?: string | null;
  sourceUrl?: string | null;
  sourceAuthor?: string | null;
  sourceRepo?: string | null;
  sourcePath?: string | null;
}

export type LocalResourceImportKind = "single_skill" | "collection";

export interface AddLocalResourceSkillsInput {
  sourceDir: string;
  overwrite: boolean;
}

export interface AddLocalResourceSkillsResult {
  sourceDir: string;
  importKind: LocalResourceImportKind;
  collectionName?: string | null;
  addedSkills: SkillWithLinks[];
  skippedSkills: string[];
}

export interface GitHubSnapshotImportInput {
  input: string;
  skill?: string | null;
  overwrite: boolean;
}

export interface SkillWithLinks {
  github_stars?: number | null;
  shared_agents?: string[];
  id: string;
  name: string;
  description?: string;
  file_path: string;
  canonical_path?: string;
  is_central: boolean;
  source?: string;
  source_url?: string | null;
  source_author?: string | null;
  source_repo?: string | null;
  source_path?: string | null;
  notes?: string | null;
  tags?: string[];
  scanned_at: string;
  created_at?: string;
  updated_at?: string;
  /** Agent IDs that currently have this skill installed (symlink or copy). */
  linked_agents: string[];
  /** Agent IDs that can see this skill through a read-only compatibility root. */
  read_only_agents?: string[];
}

export interface BatchInstallResult {
  succeeded: string[];
  failed: Array<{ agent_id: string; error: string }>;
}

export interface DeleteCentralSkillOptions {
  cascadeUninstall: boolean;
}

export interface DeleteCentralSkillResult {
  skillId: string;
  removedCanonicalPath: string;
  uninstalledAgents: string[];
  skippedReadOnlyAgents: string[];
}

export interface DeleteResourceSkillOptions {
  cascadeUninstall: boolean;
}

export interface DeleteResourceSkillResult {
  skillId: string;
  removedCanonicalPath: string;
  uninstalledAgents: string[];
  skippedReadOnlyAgents: string[];
}

export interface CentralSkillBundle {
  name: string;
  relativePath: string;
  path: string;
  isSymlink: boolean;
  skillCount: number;
  linkedAgentCount: number;
  readOnlyAgentCount: number;
}

export interface CentralSkillBundleDeletePreview {
  bundle: CentralSkillBundle;
  skills: SkillWithLinks[];
  affectedAgents: string[];
  skippedReadOnlyAgents: string[];
}

export interface CentralSkillBundleDetail {
  bundle: CentralSkillBundle;
  skills: SkillWithLinks[];
}

export interface DeleteCentralSkillBundleOptions {
  cascadeUninstall: boolean;
}

export interface DeleteCentralSkillBundleResult {
  relativePath: string;
  removedBundlePath: string;
  removedKind: "directory" | "symlink" | string;
  removedSkillIds: string[];
  uninstalledAgents: string[];
  skippedReadOnlyAgents: string[];
}

// ─── Collection Types ─────────────────────────────────────────────────────────

export interface Collection {
  id: string;
  name: string;
  description?: string;
  created_at: string;
  updated_at: string;
}

export interface CollectionWithSkills extends Collection {
  skill_ids: string[];
}

export interface CollectionDetail extends Collection {
  /** Full skill objects that are members of this collection. */
  skills: Skill[];
}

export interface CollectionBatchInstallResult {
  succeeded: string[];
  failed: Array<{ agent_id: string; error: string }>;
}

// ─── Settings Types ───────────────────────────────────────────────────────────

export interface ScanDirectory {
  id: number;
  path: string;
  label?: string;
  is_active: boolean;
  is_builtin: boolean;
  added_at: string;
}

export interface ScanRoot {
  path: string;
  label: string;
  exists: boolean;
  enabled: boolean;
}

// ─── GitHub Import Types ─────────────────────────────────────────────────────

export interface GitHubRepoRef {
  owner: string;
  repo: string;
  branch: string;
  normalizedUrl: string;
}

export interface GitHubSkillConflict {
  existingSkillId: string;
  existingName: string;
  existingCanonicalPath?: string | null;
  proposedSkillId: string;
  proposedName: string;
}

export interface GitHubSkillPreview {
  sourcePath: string;
  skillId: string;
  skillName: string;
  description?: string | null;
  rootDirectory: string;
  skillDirectoryName: string;
  downloadUrl: string;
  conflict?: GitHubSkillConflict | null;
}

export interface GitHubRepoPreview {
  repo: GitHubRepoRef;
  skills: GitHubSkillPreview[];
}

export type DuplicateResolution = "overwrite" | "skip" | "rename";

export interface GitHubSkillImportSelection {
  sourcePath: string;
  resolution: DuplicateResolution;
  renamedSkillId?: string | null;
}

export interface ImportedGitHubSkillSummary {
  sourcePath: string;
  originalSkillId: string;
  importedSkillId: string;
  skillName: string;
  targetDirectory: string;
  resolution: DuplicateResolution;
}

export interface GitHubRepoImportResult {
  repo: GitHubRepoRef;
  importedSkills: ImportedGitHubSkillSummary[];
  skippedSkills: string[];
}

export type GitHubImportProgressPhase = "preparing" | "writing" | "finalizing";

export interface GitHubImportProgressPayload {
  operationId?: string | null;
  phase: GitHubImportProgressPhase;
  currentSkill?: string | null;
  currentPath?: string | null;
  completedFiles: number;
  totalFiles: number;
  completedBytes: number;
  totalBytes: number;
}

export interface SkillSourceUpdateProgress {
  current: number;
  total: number;
  name: string;
  skillId: string;
}

export type SkillSourceUpdateStatus = "updated" | "unchanged" | "deleted" | "failed" | "skipped";

export interface SkillSourceUpdateItem {
  skillId: string;
  name: string;
  status: SkillSourceUpdateStatus;
  error?: string | null;
  remoteDeleted?: boolean;
}

export interface SkillSourceUpdateReport {
  items: SkillSourceUpdateItem[];
}

export interface RepositorySyncPreviewItem {
  sourcePath?: string | null;
  version?: string;
  files?: {path:string;status:"added"|"modified"|"deleted"}[];
  skillId: string;
  name: string;
}

export interface RepositorySyncPreview {
  snapshotId?: string | null;
  documentChanges?: {path:string;status:"added"|"modified"|"deleted"}[];
  repository: string;
  currentRef?: string | null;
  remoteRef?: string | null;
  added: RepositorySyncPreviewItem[];
  modified: RepositorySyncPreviewItem[];
  deleted: RepositorySyncPreviewItem[];
  unchanged: RepositorySyncPreviewItem[];
  updated?: RepositorySyncPreviewItem[];
  error?: string | null;
}

export interface RepositoryCoverage {
  skillId: string; name: string; repository?: string | null;
  status: "completed" | "failed" | "pending" | "excluded";
  reason?: string | null;
}
export interface RepositorySyncPreviewReport {
  coverage?: RepositoryCoverage[];
  repositories: RepositorySyncPreview[];
}

export interface RepositorySyncApplyOptions {
  includeAdded: boolean;
  removeDeleted: boolean;
  repositories?: string[] | null;
}
