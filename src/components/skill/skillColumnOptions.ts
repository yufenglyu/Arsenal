import type { SkillTableKind } from "@/hooks/useSkillTableColumns";

export interface SkillColumnOption {
  key: string;
  labelKey: string;
  fixed?: boolean;
}

export const SKILL_COLUMN_OPTIONS: SkillColumnOption[] = [
  { key: "repository", labelKey: "skillBrowser.columns.repository" },
  { key: "index", labelKey: "skillBrowser.columns.index", fixed: true },
  { key: "name", labelKey: "skillBrowser.columns.name", fixed: true },
  { key: "createdAt", labelKey: "skillBrowser.columns.createdAt" },
  { key: "updatedAt", labelKey: "skillBrowser.columns.updatedAt" },
  { key: "installSummary", labelKey: "skillBrowser.columns.installSummary" },
];

export const FOLDER_COLUMN_OPTIONS: SkillColumnOption[] = [
  { key: "index", labelKey: "skillBrowser.columns.index", fixed: true },
  { key: "name", labelKey: "skillBrowser.columns.name", fixed: true },
  { key: "notebook", labelKey: "skillBrowser.columns.notebook" },
  { key: "skillCount", labelKey: "skillBrowser.columns.skillCount" },
  { key: "githubStars", labelKey: "skillBrowser.columns.githubStars" },
  { key: "installSummary", labelKey: "skillBrowser.columns.installSummary" },
  { key: "createdAt", labelKey: "skillBrowser.columns.createdAt" },
  { key: "updatedAt", labelKey: "skillBrowser.columns.updatedAt" },
];

export function optionsForSkillTable(kind: SkillTableKind) {
  return kind === "skill" ? SKILL_COLUMN_OPTIONS : FOLDER_COLUMN_OPTIONS;
}
