import i18n from "@/i18n";
import type { TaskKind } from "@/stores/taskQueueStore";

const codes = [
  "TRASH_TOO_LARGE", "TRASH_UNAVAILABLE", "TRASH_NOT_FOUND", "TRASH_INSTALLED", "TRASH_ROOT_CHANGED",
  "TRASH_RESTORE_CONFLICT", "TRASH_RECOVERY_REQUIRED", "IMPORT_SOURCE_MISSING", "IMPORT_SOURCE_AMBIGUOUS", "SOURCE_METADATA_MISSING",
  "REPOSITORY_NOT_IN_LIBRARY", "PREVIEW_EXPIRED", "SNAPSHOT_UNAVAILABLE", "LOCAL_IDENTITY_CHANGED", "LOCAL_CONTENT_CHANGED",
  "TARGET_DIRECTORY_CONFLICT", "SNAPSHOT_LINK_UNAVAILABLE", "UNSAFE_TARGET", "UNSAFE_INSTALL_TARGET",
  "UNSAFE_SNAPSHOT_PATH", "SNAPSHOT_CACHE_FULL", "SNAPSHOT_TOO_LARGE", "WRITE_FAILED",
  "FILE_PERMISSION", "FILE_BUSY", "DISK_FULL", "READ_ONLY", "CONTENT_VERIFY_FAILED", "REPOSITORY_DOC_VERIFY_FAILED",
  "TASK_RESULT_MISSING", "TASK_REASON_MISSING", "COMMIT_UNCERTAIN",
] as const;
const aliases: Record<string, string> = {
  FILE_PERMISSION: "filePermission", FILE_BUSY: "fileBusy", DISK_FULL: "diskFull", READ_ONLY: "readOnly",
  CONTENT_VERIFY_FAILED: "contentVerification", REPOSITORY_DOC_VERIFY_FAILED: "documentVerification",
  TASK_RESULT_MISSING: "resultMissing", TASK_REASON_MISSING: "reasonMissing", COMMIT_UNCERTAIN: "commitUncertain",
};
const operationKeys: Record<string, string> = {
  prepare_github_resource_import: "workflow.taskErrors.prepare", import_github_repo_skills: "workflow.taskErrors.import",
  reimport_github_repo_skills: "workflow.taskErrors.reimport", add_local_resource_skills: "workflow.taskErrors.localImport",
  preview_source_backed_resource_repository_updates: "workflow.flow.checkAction", apply_repository_snapshot: "workflow.flow.syncAction",
  apply_repository_update_item: "workflow.flow.syncAction", update_source_backed_resource_skill: "workflow.flow.syncAction",
  refresh_repository: "workflow.refreshRepositoryInfo", refresh_repository_stars: "workflow.refreshRepositoryInfo",
  explain_skill: "workflow.taskErrors.ai", generate_skill_tags: "workflow.taskErrors.ai", generate_repository_note: "workflow.taskErrors.ai",
};
const legacyMessages: Record<string, string> = {
  "操作失败，请检查配置与目标状态后重试": "reasonMissing",
  "Operation failed. Check settings and the target, then retry.": "reasonMissing",
  "AI 服务授权失败，请检查 AI 配置": "aiAuthorization",
  "GitHub 拒绝访问，请检查仓库权限及 GitHub 访问凭据": "githubAuthorization",
  "网络连接失败，请检查网络后重试": "network", "请求超时，请重试": "timeout",
  "文件或远程资源不存在": "missing", "目标已存在，请重新处理冲突": "conflict",
  "技能仓库目标路径无法安全写入。": "UNSAFE_TARGET", "安装目标已改变，无法安全同步。": "UNSAFE_INSTALL_TARGET",
  "远端包含不支持或冲突的路径，未执行同步。": "UNSAFE_SNAPSHOT_PATH",
  "检查快照存储已达到容量限制。": "SNAPSHOT_CACHE_FULL", "仓库技能内容超过快照容量限制。": "SNAPSHOT_TOO_LARGE",
  "整仓同步未完成；修复目标路径或文件占用后重试同一快照。": "WRITE_FAILED",
};

/** Stable, public diagnostics only. Raw provider bodies, paths, URLs and unknown
 * backend strings are never copied into the returned message or error code. */
export function describeTaskError(error: unknown, kind: TaskKind = "check", command?: string): {code: string; message: string} {
  const text = error instanceof Error ? error.message : typeof error === "string" ? error : error && typeof error === "object" && "message" in error && typeof error.message === "string" ? error.message : "";
  const supplied = error && typeof error === "object" && "code" in error && typeof error.code === "string" ? error.code : /^([A-Z_]+)(?::|$)/.exec(text)?.[1];
  const translated = (key: string, code = key): {code: string; message: string} => ({code, message: i18n.t(`workflow.errors.${key}`)});
  if (legacyMessages[text]) return translated(legacyMessages[text]);
  // Repeated rendering and persisted records from either language retain their
  // classification instead of being reclassified as an unknown provider error.
  for (const language of ["zh", "en"]) {
    const messages = i18n.getResource(language, "translation", "workflow.errors") as Record<string, unknown> | undefined;
    for (const [key, value] of Object.entries(messages ?? {})) {
      if (typeof value === "string" && value === text && !["other", "unclassified"].includes(key)) return translated(key);
    }
    for (const operationKey of new Set([...Object.values(operationKeys), "workflow.taskErrors.sync", "workflow.taskErrors.check"])) {
      if (text === i18n.t("workflow.errors.unclassified", {lng: language, operation: i18n.t(operationKey, {lng: language})})) {
        return {code: "TASK_UNKNOWN", message: i18n.t("workflow.errors.unclassified", {operation: i18n.t(operationKey)})};
      }
    }
  }
  if (supplied && supplied !== "WRITE_FAILED" && codes.some(code => code === supplied)) return translated(aliases[supplied] ?? supplied, supplied);
  if (/^address[A-Za-z]+$/.test(text) && i18n.exists(`workflow.address.errors.${text}`)) return {code: text, message: i18n.t(`workflow.address.errors.${text}`)};
  const message = text.toLowerCase();
  if (message === "repository is no longer in the resource library") return translated("REPOSITORY_NOT_IN_LIBRARY");
  if (/commit_uncertain|commit outcome uncertain/.test(message)) return translated("commitUncertain");
  if (/content verification failed/.test(message)) return translated("contentVerification");
  if (/repository document verification failed/.test(message)) return translated("documentVerification");
  if (/os error (?:28|112)\b|no space left|disk (?:is )?full|磁盘空间不足/.test(message)) return translated("diskFull");
  if (/os error (?:32|33|16)\b|being used by another process|file in use|sharing violation|resource busy|文件.*占用/.test(message)) return translated("fileBusy");
  if (/os error (?:30|19)\b|read.only file system|write.protected|写保护|只读文件系统/.test(message)) return translated("readOnly");
  if (/os error (?:5|13)\b|access is denied|permission denied|拒绝访问|权限不足/.test(message)) return translated("filePermission");
  if (supplied === "WRITE_FAILED") return translated("WRITE_FAILED");
  if (/store is locked|store access.*lock|store lock|database is locked/.test(message)) return translated("storageBusy");
  if (/invalid.*(?:store|manifest|object identifier)|missing manifest|storage history|corrupt|checksum|digest mismatch|recovery required|history missing/.test(message)) return translated("storageCorrupt");
  if (/preview_expired|local_identity_changed/.test(message)) return translated("stale");
  if (/snapshot_unavailable|missing snapshot file/.test(message)) return translated("remoteVersionChanged");
  if (/invalid github repository url|only https:\/\/ github|only github\.com|repository url (?:must|is missing)|enter a github repository/.test(message)) return translated("invalidRepository");
  if (/no importable skills/.test(message)) return translated("noSkills");
  if (/invalid repository import rules/.test(message)) return translated("invalidImportRules");
  if (/missing valid frontmatter|missing.*(?:name|description)|invalid (?:yaml|frontmatter)|skill identifier.*not supported/.test(message)) return translated("invalidSkillMetadata");
  if (/not valid utf.?8|invalid utf.?8/.test(message)) return translated("invalidEncoding");
  if (/file tree is incomplete|repository tree.*truncated/.test(message)) return translated("incompleteRepositoryTree");
  if (/invalid repository file tree|head response did not include a commit|failed to (?:parse|decode).*json/.test(message)) return translated("invalidRemoteResponse");
  if (/star count is unavailable/.test(message)) return translated("repositoryMetadataMissing");
  if (/repository head not found|no commit found|empty repository/.test(message)) return translated("repositoryHeadMissing");
  if (/repository not found|http\s*404\b|404 not found/.test(message)) return translated("repositoryMissing");
  if (/repository.*no longer available|update source not found|has no recorded source|missing source path/.test(message)) return translated("IMPORT_SOURCE_MISSING");
  if (/item category changed/.test(message)) return translated("categoryChanged");
  if (/remote content version changed/.test(message)) return translated("remoteVersionChanged");
  if (/preview changed|update target moved|does not belong to this repository/.test(message)) return translated("stale");
  if (/unsupported path|unsafe.*path|cannot be safely replaced|cannot compare a symlink|conflicting skill directories/.test(message)) return translated("UNSAFE_SNAPSHOT_PATH");
  if (/archive|gzip|tar entry/.test(message) && /invalid|corrupt|unexpected end|unsupported|failed to (?:read|inspect)/.test(message) && !/response body|network/.test(message)) return translated("invalidArchive");
  if (/readme is empty/.test(message)) return translated("readmeEmpty");
  if (/请先在设置中配置 ai api key|ai.*(?:key|provider).*not configured|missing ai (?:key|configuration)/.test(message)) return translated("aiNotConfigured");
  if (/模型达到输出上限|model reached its output limit/.test(message)) return translated("aiOutputLimit");
  if (/ai (?:服务未返回可用正文|返回的标签格式无效|标签应为|应生成)|ai service returned no usable answer|invalid ai response/.test(message)) return translated("invalidAiResponse");
  if (/response body failed|download interrupted|response body incomplete|error decoding response body|network (?:tls )?connection failed/.test(message)) return translated(/timeout|timed out/.test(message) ? "timeout" : "network");
  if (/rate.?limit|too many requests|\b429\b|secondary.*limit/.test(message)) return translated(kind === "ai" ? "aiRateLimit" : "rateLimit");
  if (/timeout|timed out|超时/.test(message)) return translated("timeout");
  if (/401|403|unauthorized|forbidden/.test(message)) return translated(kind === "ai" ? "aiAuthorization" : "githubAuthorization");
  if (/http\s*5\d\d\b|5\d\d (?:internal|bad gateway|service unavailable)|服务暂时不可用/.test(message)) return translated("remoteService");
  if (/network|connect|dns|fetch|请求失败|流读取失败|读取响应失败/.test(message)) return translated("network");
  if (/command .*not found|unknown (?:ipc )?command/.test(message)) return translated("commandUnavailable");
  if (/not found|does not exist|no such file|os error (?:2|3)\b/.test(message)) return translated("missing");
  if (/already exists|conflict|select.*skill|choose.*skill|no valid import operations|renamed.*(?:empty|required)/.test(message)) return translated("conflict");
  // Previously persisted generic messages have no recoverable original cause.
  for (const language of ["zh", "en"]) {
    if (["workflow.errors.other", "workflow.operationFailed"].some(key => i18n.getResource(language, "translation", key) === text)) return translated("reasonMissing", "TASK_REASON_MISSING");
  }
  if (["workflow.noImportableSkills", "workflow.checkPartial"].some(key => i18n.t(key) === text)) return {code: "TASK_SUMMARY", message: text};
  const operation = i18n.t(operationKeys[command ?? ""] ?? `workflow.taskErrors.${kind === "ai" ? "ai" : kind === "import" ? "import" : kind === "update" ? "sync" : "check"}`);
  return {code: "TASK_UNKNOWN", message: i18n.t("workflow.errors.unclassified", {operation})};
}

export function taskErrorMessage(error: unknown, kind: TaskKind = "check", command?: string): string {
  return describeTaskError(error, kind, command).message;
}

export function normalizeTaskResult(result: unknown, kind: TaskKind, command: string, args: Record<string, unknown> = {}): unknown {
  if (["refresh_repository", "refresh_repository_stars"].includes(command) && (typeof result !== "number" || !Number.isFinite(result) || result < 0)) throw {code: "TASK_RESULT_MISSING"};
  if (command !== "preview_source_backed_resource_repository_updates") return result;
  if (!result || typeof result !== "object" || !("repositories" in result) || !Array.isArray(result.repositories)) throw {code: "TASK_RESULT_MISSING"};
  const repositories = result.repositories.map(repo => {
    if (!repo || typeof repo !== "object" || typeof repo.repository !== "string") throw {code: "TASK_RESULT_MISSING"};
    if (!repo || typeof repo !== "object" || !repo.error) return repo;
    const error = describeTaskError(repo.error, kind, command);
    return {...repo, error: error.message, errorCode: error.code};
  });
  for (const repository of Array.isArray(args.repositories) ? args.repositories : []) {
    if (typeof repository !== "string" || repositories.some(repo => repo.repository.toLowerCase() === repository.toLowerCase())) continue;
    const error = describeTaskError({code: "TASK_RESULT_MISSING"}, kind, command);
    repositories.push({repository, snapshotId: null, added: [], modified: [], deleted: [], unchanged: [], error: error.message, errorCode: error.code});
  }
  return {...result, repositories};
}
