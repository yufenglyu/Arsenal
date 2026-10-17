import type { SkillWithLinks } from "@/types";

export type SkillListViewMode = "all" | "folders";

export interface SkillFolderGroup<TSkill> {
  name: string;
  relativePath: string;
  path: string;
  skillCount: number;
  linkedAgentIds: string[];
  readOnlyAgentIds: string[];
  linkedAgentCount: number;
  readOnlyAgentCount: number;
  skills: TSkill[];
}

export interface SplitSkillsByTopLevelOptions<TSkill> {
  skills: TSkill[];
  rootPath: string;
  getRootPath?: (skill: TSkill) => string | null | undefined;
  getDirPaths: (skill: TSkill) => string | null | undefined | Array<string | null | undefined>;
  getLinkedAgentIds?: (skill: TSkill) => readonly string[] | null | undefined;
  getReadOnlyAgentIds?: (skill: TSkill) => readonly string[] | null | undefined;
}

export function normalizeFsPath(path: string): string {
  return path.replace(/\\/g, "/").replace(/\/+$/, "");
}

export function dirnameFromSkillFile(filePath: string): string {
  const normalized = normalizeFsPath(filePath);
  if (normalized.toLowerCase().endsWith("/skill.md")) {
    return normalized.slice(0, -"/SKILL.md".length);
  }
  const lastSlash = normalized.lastIndexOf("/");
  return lastSlash >= 0 ? normalized.slice(0, lastSlash) : normalized;
}

export function getRelativePathUnderRoot(path: string, rootPath: string): string | null {
  const normalizedRoot = normalizeFsPath(rootPath);
  const normalizedPath = normalizeFsPath(path);
  if (!normalizedRoot || !normalizedPath) return null;
  if (normalizedPath === normalizedRoot) return "";
  const prefix = `${normalizedRoot}/`;
  if (!normalizedPath.startsWith(prefix)) return null;
  return normalizedPath.slice(prefix.length);
}

function candidatePaths(value: string | null | undefined | Array<string | null | undefined>) {
  return Array.isArray(value) ? value : [value];
}

function uniqueIds(values: Iterable<string>) {
  const seen = new Set<string>();
  const result: string[] = [];
  for (const value of values) {
    if (!value || seen.has(value)) continue;
    seen.add(value);
    result.push(value);
  }
  return result;
}

export function splitSkillsByTopLevel<TSkill>({
  skills,
  rootPath,
  getRootPath,
  getDirPaths,
  getLinkedAgentIds,
  getReadOnlyAgentIds,
}: SplitSkillsByTopLevelOptions<TSkill>) {
  const rootSkills: TSkill[] = [];
  const groups = new Map<string, SkillFolderGroup<TSkill>>();

  for (const skill of skills) {
    let relativePath: string | null = null;
    let matchedRootPath = rootPath;
    for (const path of candidatePaths(getDirPaths(skill))) {
      if (!path) continue;
      const candidateRootPath = getRootPath?.(skill) ?? rootPath;
      relativePath = getRelativePathUnderRoot(path, candidateRootPath);
      if (relativePath !== null) {
        matchedRootPath = candidateRootPath;
        break;
      }
      if (candidateRootPath !== rootPath) {
        relativePath = getRelativePathUnderRoot(path, rootPath);
        matchedRootPath = rootPath;
      }
      if (relativePath !== null) break;
    }

    const parts = relativePath?.split("/").filter(Boolean) ?? [];
    if (parts.length <= 1) {
      rootSkills.push(skill);
      continue;
    }

    const folderName = parts[0];
    const normalizedRoot = normalizeFsPath(matchedRootPath);
    const groupKey = getRootPath ? `${normalizedRoot}/${folderName}` : folderName;
    const group =
      groups.get(groupKey) ??
      {
        name: folderName,
        relativePath: groupKey,
        path: `${normalizedRoot}/${folderName}`,
        skillCount: 0,
        linkedAgentIds: [],
        readOnlyAgentIds: [],
        linkedAgentCount: 0,
        readOnlyAgentCount: 0,
        skills: [],
      };

    group.skills.push(skill);
    group.skillCount = group.skills.length;
    group.linkedAgentIds = uniqueIds(
      group.skills.flatMap((item) => [...(getLinkedAgentIds?.(item) ?? [])])
    );
    group.readOnlyAgentIds = uniqueIds(
      group.skills.flatMap((item) => [...(getReadOnlyAgentIds?.(item) ?? [])])
    );
    group.linkedAgentCount = group.linkedAgentIds.length;
    group.readOnlyAgentCount = group.readOnlyAgentIds.length;
    groups.set(groupKey, group);
  }

  return {
    rootSkills,
    groups: [...groups.values()].sort((a, b) =>
      a.relativePath.localeCompare(b.relativePath, undefined, {
        numeric: true,
        sensitivity: "base",
      })
    ),
  };
}

function sortSkillsByName(skills: SkillWithLinks[]) {
  return [...skills].sort((a, b) =>
    a.name.localeCompare(b.name, undefined, { numeric: true, sensitivity: "base" })
  );
}

function resourceSourceFolderName(skill: SkillWithLinks): string | null {
  const repoParts = skill.source_repo?.split("/").filter(Boolean) ?? [];
  if (repoParts.length >= 2) {
    return `${repoParts[0]}/${repoParts[1]}`;
  }
  return skill.source_author || repoParts[0] || null;
}

function resourceSourceFolderPath(rootPath: string, folderName: string, skill: SkillWithLinks) {
  if (rootPath) {
    return `${normalizeFsPath(rootPath)}/${folderName}`;
  }
  const candidatePath = normalizeFsPath(skill.canonical_path ?? dirnameFromSkillFile(skill.file_path));
  const marker = `/${folderName}/`;
  const markerIndex = candidatePath.toLowerCase().indexOf(marker.toLowerCase());
  if (markerIndex >= 0) {
    return candidatePath.slice(0, markerIndex + marker.length - 1);
  }
  return candidatePath;
}

export function splitResourceLibrarySkillsByFolder(
  skills: SkillWithLinks[],
  rootPath: string
) {
  const rootSkills: SkillWithLinks[] = [];
  const groups = new Map<string, SkillFolderGroup<SkillWithLinks>>();

  for (const skill of skills) {
    const skillDir = dirnameFromSkillFile(skill.file_path);
    const candidatePaths = [
      skill.canonical_path,
      skillDir,
    ];
    let bundleName: string | null = null;
    let bundleKey: string | null = null;
    let bundlePath: string | null = null;

    for (const path of candidatePaths) {
      if (!path) continue;
      const relativePath = getRelativePathUnderRoot(path, rootPath);
      const parts = relativePath?.split("/").filter(Boolean) ?? [];
      if (parts.length >= 3) {
        const bundleParts = parts.slice(0, 2);
        bundleName = bundleParts.join("/");
        bundleKey = bundleName;
        bundlePath = `${normalizeFsPath(rootPath)}/${bundleName}`;
        break;
      }
      if (parts.length === 2) {
        bundleName = parts[0];
        bundleKey = bundleName;
        bundlePath = `${normalizeFsPath(rootPath)}/${bundleName}`;
        break;
      }
    }

    if (!bundleName || !bundleKey || !bundlePath) {
      const sourceFolderName = resourceSourceFolderName(skill);
      if (!sourceFolderName) {
        rootSkills.push(skill);
        continue;
      }
      bundleName = sourceFolderName;
      bundleKey = `source:${sourceFolderName.toLowerCase()}`;
      bundlePath = resourceSourceFolderPath(rootPath, sourceFolderName, skill);
    }

    if (!bundleName || !bundleKey || !bundlePath) {
      rootSkills.push(skill);
      continue;
    }

    const group =
      groups.get(bundleKey) ??
      {
        name: bundleName,
        relativePath: bundleKey,
        path: bundlePath,
        skillCount: 0,
        linkedAgentIds: [],
        readOnlyAgentIds: [],
        linkedAgentCount: 0,
        readOnlyAgentCount: 0,
        skills: [],
      };

    group.skills.push(skill);
    group.skills = sortSkillsByName(group.skills);
    const sourceRepo = group.skills[0]?.source_repo;
    if (sourceRepo && group.skills.every(item => item.source_repo?.toLowerCase() === sourceRepo.toLowerCase())) group.name = sourceRepo;
    group.skillCount = group.skills.length;
    group.linkedAgentIds = uniqueIds(group.skills.flatMap((item) => item.linked_agents));
    group.readOnlyAgentIds = uniqueIds(
      group.skills.flatMap((item) => item.read_only_agents ?? [])
    );
    group.linkedAgentCount = group.linkedAgentIds.length;
    group.readOnlyAgentCount = group.readOnlyAgentIds.length;
    groups.set(bundleKey, group);
  }

  return {
    rootSkills,
    groups: [...groups.values()].sort((a, b) =>
      a.name.localeCompare(b.name, undefined, { numeric: true, sensitivity: "base" })
    ),
  };
}

export function skillListViewModeStorageKey(scope: string): string {
  return `skills-manage.skillListViewMode.${scope}`;
}

export function readStoredSkillListViewMode(scope: string): SkillListViewMode {
  if (typeof window === "undefined") return "all";
  return window.localStorage.getItem(skillListViewModeStorageKey(scope)) === "folders"
    ? "folders"
    : "all";
}

export function writeStoredSkillListViewMode(scope: string, mode: SkillListViewMode) {
  if (typeof window === "undefined") return;
  window.localStorage.setItem(skillListViewModeStorageKey(scope), mode);
}
