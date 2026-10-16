import { Button } from "@/components/ui/button";
import { ActionIcon } from "@/components/ui/action-icon";
import { browserStats } from "@/lib/browserStats";
import { formatRepositoryName } from "@/lib/repositoryName";
import { useRepositoryNameStore } from "@/stores/repositoryNameStore";
import { useBrowserStatusStore } from "@/stores/browserStatusStore";
import { shouldIgnoreShortcutTarget } from "@/lib/shortcutKeys";
import { BatchSkillActionDialog, type BatchAction, type BatchEntry, type BatchOperations } from "./BatchSkillActionDialog";
import { createPortal } from "react-dom";
import { ActionMenuContext } from "@/components/ui/action-menu-context";
import { isProjectAgentId } from "@/lib/projectTargets";
import {
  BookOpen,
  Check,
  ArrowLeftRight,
  ArrowUpDown,
  ArrowUp,
  ArrowDown,
  ChevronRight,
  ChevronDown,
  FileText,
  Pencil,
  Plus,
  LocateFixed,
  FolderOpen,
  Link2,
  Loader2,
  PackageMinus,
  PackagePlus,
  RefreshCw,
  Trash2,
} from "lucide-react";
import {
  Fragment,
  useContext,
  useLayoutEffect,
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
  type MouseEvent as ReactMouseEvent,
  type MouseEventHandler,
  type PointerEvent as ReactPointerEvent,
  type CSSProperties,
  type ReactNode,
} from "react";
import { useTranslation } from "react-i18next";

import { InlineConfirmAction } from "@/components/ui/inline-confirm-action";
import type { SkillTableKind } from "@/hooks/useSkillTableColumns";
import { FIXED_SKILL_COLUMNS } from "@/hooks/useSkillTableColumns";
import { optionsForSkillTable } from "@/components/skill/skillColumnOptions";
import {
  getSkillSourceLineKeys,
  isExceptionalSkillOrigin,
} from "@/lib/skillSourceDisplay";
import {
  nextSkillSortDirection,
  type SkillSortDirection,
  type SkillSortField,
} from "@/lib/skillSort";
import { cn } from "@/lib/utils";
import type { AgentWithStatus, PlatformSourceKind } from "@/types";
import type { UnifiedSkillCardProps } from "@/components/skill/UnifiedSkillCard";
import {
  buildMembershipInstallSummary,
  type InstallSummaryMember,
  formatInstallSummaryTooltip,
} from "@/lib/installSummary";
import {
  InstallTargetsActionIcon,
  SharedHubActionIcon,
} from "@/components/skill/SkillActionIcons";

type InstallationSource = "independent" | "shared";

export interface FolderTableItem {
  onNotebook?: () => void;
  hasNotebook?: boolean;
  sourceRepo?: string | null;
  batchOperations?: BatchOperations;
  expandable?: boolean;
  onSelect?: () => void;
  onEdit?: () => void;
  onAddSkills?: () => void;
  metadata?: ReactNode;
  notesEditor?: ReactNode;
  skillListLoading?: boolean;
  skillKeys?: string[];
  children?: SkillTableItem[];
  expanded?: boolean;
  onToggle?: () => void;
  tags?: string[];
  githubStars?: number | null;
  onSearch?: () => void;
  onLocate?: () => void;
  highlighted?: boolean;
  scrollToRow?: boolean;
  centralUninstall?: boolean;
  installationSources?: InstallationSource[];
  installSummaryMembers?: InstallSummaryMember[];
  key: string;
  name: string;
  path: string;
  skillCount: number;
  linkedAgentCount?: number;
  readOnlyAgentCount?: number;
  installAgents?: AgentWithStatus[];
  installLinkedAgentIds?: string[];
  installReadOnlyAgentIds?: string[];
  previewNames?: string[];
  createdAt?: string | null;
  updatedAt?: string | null;
  onOpen: () => void;
  onAddToCentral?: () => void;
  addToCentralLabel?: string;
  isAddingToCentral?: boolean;
  onRemoveFromCentral?: () => void;
  removeFromCentralLabel?: string;
  isRemovingFromCentral?: boolean;
  onUpdate?: () => void;
  updateLabel?: string;
  isUpdating?: boolean;
  onInstall?: () => void;
  installLabel?: string;
  isInstalling?: boolean;
  onUninstall?: () => void;
  uninstallLabel?: string;
  isUninstalling?: boolean;
  onDelete?: () => void;
  deleteLabel?: string;
  isDeleting?: boolean;
  deleteRequiresConfirmation?: boolean;
}

export interface SkillTableItem extends UnifiedSkillCardProps {
  batchOperations?: BatchOperations;
  detailRequest?: { skillId: string; agentId?: string; rowId?: string };
  onSearch?: () => void;
  onLocate?: () => void;
  highlighted?: boolean;
  scrollToRow?: boolean;
  centralUninstall?: boolean;
  uninstallRequiresConfirmation?: boolean;
  installationSources?: InstallationSource[];
  installSummaryMembers?: InstallSummaryMember[];
  installLinkedCount?: number;
  installReadOnlyCount?: number;
  installAgents?: AgentWithStatus[];
  installLinkedAgentIds?: string[];
  installReadOnlyAgentIds?: string[];
  onRemoveFromCentral?: () => void;
  removeFromCentralLabel?: string;
  installToLabel?: string;
  removeLabel?: string;
}

export interface SkillBrowserTableProps {
  statusPath?: string;
  statusCollections?: boolean;
  onSelectionChange?: (rowKeys: string[]) => void;
  compactList?: boolean;
  tree?: boolean;
  showInstallationSource?: boolean;
  showGithubStars?: boolean;
  kind: SkillTableKind;
  visibleColumns: Set<string>;
  skills?: SkillTableItem[];
  folders?: FolderTableItem[];
  sortField?: SkillSortField;
  sortDirection?: SkillSortDirection;
  onSortChange?: (field: SkillSortField, direction: SkillSortDirection) => void;
  onToggleColumn?: (key: string) => void;
  onResetColumns?: () => void;
  nameHeaderAction?: ReactNode;
  stickyHeaderTop?: string;
  className?: string;
}

const FIXED_COLUMNS = new Set<string>(FIXED_SKILL_COLUMNS);
const DEFAULT_COLUMN_WIDTHS: Record<string, number> = {
  index: 40,
  name: 384,
  source: 180,
  repository: 220,
  createdAt: 140,
  updatedAt: 140,
  installSummary: 240,
  tags: 180,
  notes: 256,
  skillCount: 100,
  githubStars: 110,
  notebook: 68,
  actions: 180,
};
const MIN_COLUMN_WIDTH = 80;
const MIN_INDEX_COLUMN_WIDTH = 32;
const MAX_AUTO_COLUMN_WIDTH = 640;
const COLUMN_WIDTH_EVENT = "skills-manage:skill-table-widths";
function formatDate(value?: string | null) {
  if (!value) return "-";
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return value.slice(0, 10);
  return date.toISOString().slice(0, 10);
}

function TagsCell({ tags }: { tags: string[] }) {
  const uniqueTags = [...new Set(tags.map(tag => tag.trim()).filter(Boolean))];
  return <div className="flex flex-wrap gap-1">
    {uniqueTags.map(tag => <span key={tag} className="max-w-full break-words rounded-md bg-muted/60 px-1.5 py-0.5 text-xs text-muted-foreground">#{tag}</span>)}
  </div>;
}

function sourceLabel(skill: UnifiedSkillCardProps) {
  return skill.sourceRepo ?? skill.sourceAuthor ?? skill.publisher ?? "-";
}

export function InstallSummaryCell({
  agents,
  members,
}: {
  members: InstallSummaryMember[];
  agents: readonly AgentWithStatus[];
}) {
  const { t } = useTranslation();
  const summary = buildMembershipInstallSummary(members, agents);
  const tooltip = formatInstallSummaryTooltip(t, summary);
  const directTotal = summary.directPlatforms.length + summary.directProjects.length;
  const directLabel = t("skillBrowser.installSummaryDirect", {
    total: directTotal,
    platforms: summary.directPlatforms.length,
    projects: summary.directProjects.length,
  });
  const sharedLabel = t("skillBrowser.installSummaryShared", {
    total: summary.shared.length,
    platforms: summary.shared.filter(target => !isProjectAgentId(target.id)).length,
    projects: summary.shared.filter(target => isProjectAgentId(target.id)).length,
  });

  return (
    <div
      className="flex flex-col gap-0.5 text-sm leading-5 text-muted-foreground outline-none focus-visible:ring-2 focus-visible:ring-ring"
      title={tooltip}
      tabIndex={0}
      aria-label={tooltip}
    >
      {members.some(skill => skill.is_central) && <span>{sharedLabel}</span>}
      {directTotal > 0 && <span>{directLabel}</span>}
      {!members.some(skill => skill.is_central) && directTotal === 0 && <span>{t("skillBrowser.notInstalled")}</span>}
    </div>
  );
}

function skillInstallSummaryProps(skill: SkillTableItem) {
  return {
    members: skill.installSummaryMembers ?? [{ is_central: skill.isCentral ?? false, linked_agents: skill.installLinkedAgentIds ?? skill.platformIcons?.linkedAgents, read_only_agents: skill.installReadOnlyAgentIds ?? skill.platformIcons?.readOnlyAgents }],
    agents: skill.installAgents ?? skill.platformIcons?.agents ?? [],
  };
}

function folderInstallSummaryProps(folder: FolderTableItem) {
  return {
    members: folder.installSummaryMembers ?? [{ is_central: false, linked_agents: folder.installLinkedAgentIds }],
    agents: folder.installAgents ?? [],
  };
}

function ActionButton({
  label,
  onClick,
  disabled,
  children,
  destructive = false,
}: {
  label: string;
  onClick: MouseEventHandler<HTMLButtonElement>;
  disabled?: boolean;
  children: ReactNode;
  destructive?: boolean;
}) {
  const menu = useContext(ActionMenuContext);
  return (
    <button
      role={menu ? "menuitem" : undefined}
      type="button"
      className={cn(
        "inline-flex size-8 items-center justify-center rounded-md text-muted-foreground transition-colors",
        destructive
          ? "hover:bg-destructive/10 hover:text-destructive"
          : "hover:bg-primary/10 hover:text-primary",
        menu && "w-full justify-start gap-2 px-2.5",
        disabled && "pointer-events-none opacity-50"
      )}
      title={label}
      aria-label={label}
      disabled={disabled}
      onClick={event => { onClick(event); menu?.close(); }}
    >
      {children}
      {menu && <span>{label}</span>}
    </button>
  );
}

function SkillActions({ skill }: { skill: SkillTableItem }) {
  const { t } = useTranslation();
  const busy = !!skill.isLoading;
  const actions = [];
  if (skill.onLocate) actions.push(<ActionButton key="locate" label={t("skillBrowser.locateInRepository")} onClick={skill.onLocate}><LocateFixed className="size-4" /></ActionButton>);

  if (skill.onInstallToCentral && !skill.isCentral) {
    actions.push(
      <ActionButton
        key="central"
        label={skill.installToCentralLabel ?? t("resource.addToCentralAction")}
        disabled={busy}
        onClick={skill.onInstallToCentral}
      >
        {busy ? <Loader2 className="size-4 animate-spin" /> : <SharedHubActionIcon installed={false} />}
      </ActionButton>
    );
  }

  if (skill.onRemoveFromCentral && skill.isCentral) {
    actions.push(
      <InlineConfirmAction
        key="central"
        onConfirm={skill.onRemoveFromCentral}
        isLoading={busy}
        idleTitle={skill.removeFromCentralLabel ?? t("resource.removeFromCentralAction")}
        idleAriaLabel={skill.removeFromCentralLabel ?? t("resource.removeFromCentralAction")}
        confirmLabel={t("common.confirmDelete")}
        icon={<SharedHubActionIcon installed />}
      />
    );
  }

  if (skill.onUninstallFromPlatform) {
    actions.push(
      skill.uninstallRequiresConfirmation === false ? (
        <ActionButton key="install-to" label={skill.uninstallFromLabel ?? t("resource.uninstallFromTargetsAction")}
          disabled={busy} onClick={skill.onUninstallFromPlatform}>
          <PackageMinus className="size-4" />
        </ActionButton>
      ) : <InlineConfirmAction
        key="install-to"
        onConfirm={skill.onUninstallFromPlatform}
        isLoading={busy}
        idleTitle={skill.uninstallFromLabel ?? t("resource.uninstallFromTargetsAction")}
        idleAriaLabel={skill.uninstallFromLabel ?? t("resource.uninstallFromTargetsAction")}
        confirmLabel={t("common.confirmDelete")}
        icon={<PackageMinus className="size-4" />}
      />
    );
  } else if (skill.onInstallTo) {
    actions.push(
      <ActionButton
        key="install-to"
        label={skill.installToLabel ?? t("resource.installToTargetsAction")}
        disabled={busy}
        onClick={skill.onInstallTo}
      >
        <InstallTargetsActionIcon />
      </ActionButton>
    );
  }

  if (skill.onUpdateFromSource || skill.updateFromSourceLabel) {
    actions.push(
      <ActionButton
        key="update"
        label={skill.updateFromSourceLabel ?? t("resource.updateAction")}
        disabled={busy || !skill.onUpdateFromSource}
        onClick={() => skill.onUpdateFromSource?.()}
      >
        {busy ? <Loader2 className="size-4 animate-spin" /> : <RefreshCw className="size-4" />}
      </ActionButton>
    );
  }


  if (skill.onDeleteFromCentral) {
    actions.push(
      skill.deleteFromCentralRequiresDialog ? (
        <ActionButton
          key="delete"
          label={skill.deleteFromCentralLabel ?? t("resource.deleteAction")}
          disabled={busy}
          destructive
          onClick={skill.onDeleteFromCentral}
        >
          {busy ? <Loader2 className="size-4 animate-spin" /> : (skill.centralUninstall ? <PackageMinus className="size-4" /> : <Trash2 className="size-4" />)}
        </ActionButton>
      ) : (
        <InlineConfirmAction
          key="delete"
          onConfirm={skill.onDeleteFromCentral}
          isLoading={busy}
          idleTitle={skill.deleteFromCentralLabel ?? t("resource.deleteAction")}
          idleAriaLabel={skill.deleteFromCentralLabel ?? t("resource.deleteAction")}
          confirmLabel={t(skill.centralUninstall ? "common.confirmUninstall" : "common.confirmDelete")}
          icon={(skill.centralUninstall ? <PackageMinus className="size-4" /> : <Trash2 className="size-4" />)}
        />
      )
    );
  }

  if (skill.onInstall && skill.isInstalled) {
    actions.push(
      <span
        key="installed"
        className="inline-flex size-7 items-center justify-center rounded-md text-primary"
        title={t("common.installed")}
      >
        <Check className="size-4" />
      </span>
    );
  }

  if (skill.onRemove) {
    actions.push(
      <InlineConfirmAction
        key="remove"
        onConfirm={skill.onRemove}
        isLoading={busy}
        idleTitle={skill.removeLabel ?? t("resource.deleteAction")}
        idleAriaLabel={skill.removeLabel ?? t("resource.deleteAction")}
        confirmLabel={t(skill.centralUninstall ? "common.confirmUninstall" : "common.confirmDelete")}
        icon={(skill.centralUninstall ? <PackageMinus className="size-4" /> : <Trash2 className="size-4" />)}
      />
    );
  }

  return <div className="flex flex-wrap justify-start gap-1">{actions}</div>;
}

function FolderActions({ folder }: {folder: FolderTableItem}) {
  const { t } = useTranslation();
  return (                          <div className="flex flex-wrap justify-start gap-1">
                            {folder.onEdit && <ActionButton label={t("collection.edit")} onClick={folder.onEdit}><Pencil className="size-4" /></ActionButton>}
                            {folder.onAddSkills && <ActionButton label={t("collection.addSkill")} onClick={folder.onAddSkills}><Plus className="size-4" /></ActionButton>}
                            {folder.onLocate && <ActionButton label={t("skillBrowser.locateInRepository")} onClick={folder.onLocate}><LocateFixed className="size-4" /></ActionButton>}
                            {folder.onAddToCentral ? (
                              <ActionButton
                                label={folder.addToCentralLabel ?? t("resource.addToCentralAction")}
                                disabled={folder.isAddingToCentral}
                                onClick={folder.onAddToCentral}
                              >
                                {folder.isAddingToCentral ? (
                                  <Loader2 className="size-4 animate-spin" />
                                ) : (
                                  <SharedHubActionIcon installed={false} />
                                )}
                              </ActionButton>
                            ) : null}
                            {folder.onRemoveFromCentral ? (
                              <InlineConfirmAction
                                onConfirm={folder.onRemoveFromCentral}
                                isLoading={folder.isRemovingFromCentral}
                                idleTitle={
                                  folder.removeFromCentralLabel ??
                                  t("resource.removeFromCentralAction")
                                }
                                idleAriaLabel={
                                  folder.removeFromCentralLabel ??
                                  t("resource.removeFromCentralAction")
                                }
                                confirmLabel={t("common.confirmDelete")}
                                icon={<SharedHubActionIcon installed />}
                              />
                            ) : null}
                            {folder.onUninstall ? (
                              <ActionButton
                                label={folder.uninstallLabel ?? t("resource.uninstallFromTargetsAction")}
                                disabled={folder.isUninstalling}
                                onClick={folder.onUninstall}
                              >
                                {folder.isUninstalling ? (
                                  <Loader2 className="size-4 animate-spin" />
                                ) : (
                                  <PackageMinus className="size-4" />
                                )}
                              </ActionButton>
                            ) : null}
                            {!folder.onUninstall && folder.onInstall ? (
                              <ActionButton
                                label={folder.installLabel ?? t("resource.installToTargetsAction")}
                                disabled={folder.isInstalling}
                                onClick={folder.onInstall}
                              >
                                {folder.isInstalling ? (
                                  <Loader2 className="size-4 animate-spin" />
                                ) : (
                                  <InstallTargetsActionIcon />
                                )}
                              </ActionButton>
                            ) : null}
                            {folder.onUpdate || folder.updateLabel ? (
                              <ActionButton
                                label={folder.updateLabel ?? t("resource.updateAction")}
                                disabled={folder.isUpdating || !folder.onUpdate}
                                onClick={() => folder.onUpdate?.()}
                              >
                                {folder.isUpdating ? (
                                  <Loader2 className="size-4 animate-spin" />
                                ) : (
                                  <RefreshCw className="size-4" />
                                )}
                              </ActionButton>
                            ) : null}
                            {folder.onDelete ? (
                              folder.deleteRequiresConfirmation ? (
                                <InlineConfirmAction
                                  onConfirm={folder.onDelete}
                                  isLoading={folder.isDeleting}
                                  idleTitle={folder.deleteLabel ?? t("resource.deleteAction")}
                                  idleAriaLabel={folder.deleteLabel ?? t("resource.deleteAction")}
                                  confirmLabel={t(folder.centralUninstall ? "common.confirmUninstall" : "common.confirmDelete")}
                                  icon={folder.centralUninstall ? <PackageMinus className="size-4" /> : <Trash2 className="size-4" />}
                                />
                              ) : (
                                <ActionButton
                                  label={folder.deleteLabel ?? t("resource.deleteAction")}
                                  disabled={folder.isDeleting}
                                  destructive
                                  onClick={folder.onDelete}
                                >
                                  {folder.centralUninstall ? <PackageMinus className="size-4" /> : <Trash2 className="size-4" />}
                                </ActionButton>
                              )
                            ) : null}
                          </div>);
}

function SourceIndicator({
  sourceType,
  sourceLocation = "standalone",
}: {
  sourceType: "symlink" | "copy" | "native";
  sourceLocation?: "central" | "resource-library" | "standalone";
}) {
  const { t } = useTranslation();
  const isSymlink = sourceType === "symlink";
  const { label, hint } = getSkillSourceLineKeys(sourceType, sourceLocation);

  return (
    <div
      className={cn("inline-flex items-center gap-1 text-xs font-normal", isSymlink ? "text-primary/80" : "text-muted-foreground")}
      title={t(hint)}
    >
      {isSymlink ? <Link2 className="size-4 shrink-0" /> : <FolderOpen className="size-4 shrink-0" />}
      <span>{t(label)}</span>
    </div>
  );
}

function SourceOriginBadge({ originKind }: { originKind: PlatformSourceKind }) {
  const { t } = useTranslation();
  const label =
    originKind === "shared-central"
      ? t("platform.originSharedCentral")
      : t("platform.originCompatibility");
  const hint =
    originKind === "shared-central"
      ? t("platform.originSharedCentralHint")
      : t("platform.originCompatibilityHint");
  return (
    <span
      title={hint}
      className={cn(
        "inline-flex w-fit items-center rounded-full bg-violet-500/10 px-2 py-0.5 text-xs font-normal text-violet-700 ring-1 ring-violet-500/20 dark:text-violet-300"
      )}
    >
      {label}
    </span>
  );
}

function NotesCell({ notes }: { notes?: string | null }) {
  if (!notes?.trim()) return <span className="text-muted-foreground">-</span>;
  return <span className="line-clamp-2">{notes.trim()}</span>;
}

function minColumnWidth(column: string) {
  return column === "index" ? MIN_INDEX_COLUMN_WIDTH : MIN_COLUMN_WIDTH;
}

function cellHorizontalPadding(column: string) {
  return column === "index" ? 16 : 24;
}

function clampAutoFitWidth(column: string, measured: number) {
  const capped =
    column === "notes" || column === "tags"
      ? Math.min(measured, DEFAULT_COLUMN_WIDTHS[column] ?? 256)
      : Math.min(measured, MAX_AUTO_COLUMN_WIDTH);
  return Math.max(minColumnWidth(column), capped);
}

function fitLabelsForCell(cell: HTMLTableCellElement): string[] {
  const truncated = Array.from(cell.querySelectorAll(".truncate"))
    .map((element) => element.textContent?.trim() ?? "")
    .filter(Boolean);
  if (truncated.length > 0) return truncated;

  const labels: string[] = [];
  for (const child of Array.from(cell.childNodes)) {
    if (child.nodeType === Node.TEXT_NODE) {
      const text = child.textContent?.trim();
      if (text) labels.push(text);
      continue;
    }
    if (!(child instanceof HTMLElement)) continue;
    if (child.getAttribute("role") === "separator") continue;
    const text = (child.innerText ?? child.textContent ?? "").trim();
    if (!text) continue;
    for (const line of text.split(/\n+/)) {
      const label = line.trim();
      if (label) labels.push(label);
    }
  }
  return labels;
}

function estimateGlyphWidth(text: string) {
  let width = 0;
  for (const character of text) {
    width += character.charCodeAt(0) > 255 ? 13 : 7;
  }
  return width;
}

function measureTextWidth(text: string, sample: HTMLElement) {
  const canvas = document.createElement("canvas");
  const context = canvas.getContext("2d");
  if (context) {
    const style = window.getComputedStyle(sample);
    const font = style.font && style.font !== "" ? style.font : `${style.fontSize} ${style.fontFamily}`;
    if (font.trim()) {
      context.font = font;
      const measured = context.measureText(text).width;
      if (measured > 0) return measured;
    }
  }
  return estimateGlyphWidth(text);
}

function measureCellFitWidth(cell: HTMLTableCellElement, column: string) {
  const labels = fitLabelsForCell(cell);
  if (labels.length === 0) return minColumnWidth(column);

  let contentWidth = 0;
  for (const label of labels) {
    contentWidth = Math.max(contentWidth, measureTextWidth(label, cell));
  }
  const extras =
    (cell.querySelector("input[type='checkbox']") ? 24 : 0) +
    (cell.querySelector("[aria-hidden='true']") ? 14 : 0);
  return Math.ceil(contentWidth + cellHorizontalPadding(column) + extras);
}

function columnWidthStorageKey(kind: SkillTableKind) {
  return `skills-manage.skillTableColumnWidths.${kind}`;
}

function readColumnWidths(kind: SkillTableKind) {
  if (typeof window === "undefined") return {};
  const raw = window.localStorage.getItem(columnWidthStorageKey(kind));
  if (!raw) return {};
  try {
    const parsed = JSON.parse(raw);
    if (!parsed || typeof parsed !== "object" || Array.isArray(parsed)) return {};
    return Object.fromEntries(
      Object.entries(parsed)
        .map(([key, value]): [string, number] => [key, Number(value)])
        .filter((entry): entry is [string, number] => {
          const [key, value] = entry;
          return Number.isFinite(value) && value >= minColumnWidth(key);
        })
    );
  } catch {
    return {};
  }
}

function isSortableColumn(column: string, kind: SkillTableKind): column is SkillSortField {
  if (kind === "folder") return column === "skillCount" || column === "githubStars" || column === "name" || column === "createdAt" || column === "updatedAt";
  return column === "name" || column === "source" || column === "repository" || column === "createdAt" || column === "updatedAt";
}

function sortHeaderLabel(
  t: ReturnType<typeof useTranslation>["t"],
  label: string,
  direction: SkillSortDirection
) {
  return t("skillBrowser.sortedHeaderLabel", {
    label,
    direction: t(
      direction === "asc" ? "skillBrowser.sortDirectionAsc" : "skillBrowser.sortDirectionDesc"
    ),
  });
}

export function SkillBrowserTable({
  statusPath,
  statusCollections = false,
  onSelectionChange,
  tree = false,
  compactList = false,
  kind,
  visibleColumns,
  skills: originalSkills = [],
  folders: originalFolders = [],
  sortField,
  sortDirection = "asc",
  onSortChange,
  onToggleColumn,
  onResetColumns,
  nameHeaderAction,
  stickyHeaderTop = "0px",
  showInstallationSource = false,
  showGithubStars = false,
  className,
}: SkillBrowserTableProps) {
  const { t } = useTranslation();
  const { repositoryFirst, toggle: toggleRepositoryName } = useRepositoryNameStore();
  const folders = useMemo(() => {
    const result = originalFolders.map(folder => {
      if (!repositoryFirst || folder.sourceRepo?.toLowerCase() !== folder.name.toLowerCase()) return folder;
      return { ...folder, name: formatRepositoryName(folder.sourceRepo, repositoryFirst) };
    });
    if (sortField === "name") {
      result.sort((a, b) => a.name.localeCompare(b.name, undefined, { numeric: true, sensitivity: "base" }) * (sortDirection === "asc" ? 1 : -1));
    }
    return result;
  }, [originalFolders, repositoryFirst, sortField, sortDirection]);
  const skills = useMemo(() => {
    if (sortField !== "repository") return originalSkills;
    const compare = (a: string, b: string) => a.localeCompare(b, undefined, {numeric: true, sensitivity: "base"});
    return [...originalSkills].sort((a, b) => {
      const first = formatRepositoryName(a.sourceRepo || t("browser.localSkills"), repositoryFirst);
      const second = formatRepositoryName(b.sourceRepo || t("browser.localSkills"), repositoryFirst);
      return (compare(first, second) || compare(a.name, b.name)) * (sortDirection === "asc" ? 1 : -1);
    });
  }, [originalSkills, sortField, sortDirection, repositoryFirst, t]);
  const canSwitchRepositoryName = kind === "skill" ? skills.some(skill => !!skill.sourceRepo)
    : originalFolders.some(folder => folder.sourceRepo?.toLowerCase() === folder.name.toLowerCase());
  const repositoryNameColumn = kind === "skill" ? "repository" : "name";
  const switchNameLabel = t(repositoryFirst ? "skillBrowser.showOwnerFirst" : "skillBrowser.showRepositoryFirst");
  const columnLabel = (column: string) => t(`skillBrowser.columns.${column === "installSummary" && showInstallationSource ? "installationSource" : column}`);
  const renderInstallationSources = (sources: InstallationSource[] = []) => (
    <span className="text-inherit text-muted-foreground">
      {sources.map(source => t(`skillBrowser.installationSource.${source}`)).join("、") || "—"}
    </span>
  );
  const [selectedRows,setSelectedRows] = useState<Set<string>>(new Set());
  const anchor = useRef<string | null>(null);
  const [batchRequest,setBatchRequest] = useState<{action:BatchAction;entries:BatchEntry[]} | null>(null);
  const visibleRows: {key:string;skill?:SkillTableItem;folder?:FolderTableItem}[] = kind === "skill" ? skills.map(skill=>({key:`skill:${skill.rowKey ?? skill.name}`,skill})) : folders.flatMap(folder=>[
    {key:`folder:${folder.key}`,folder},
    ...(tree && folder.expanded ? (folder.children ?? []).map(skill=>({key:`skill:${skill.rowKey ?? skill.name}`,skill})) : []),
  ] as {key:string;skill?:SkillTableItem;folder?:FolderTableItem}[]);
  const visibleKeys = visibleRows.map(row=>row.key);
  const rowNodes = useRef(new Map<string, HTMLTableRowElement>());
  const scrollTargetKey = visibleRows.filter(row => {
    const item = row.skill ?? row.folder;
    return item && (item.scrollToRow ?? (!tree && item.highlighted));
  }).at(-1)?.key;
  useLayoutEffect(() => {
    if (scrollTargetKey) rowNodes.current.get(scrollTargetKey)?.scrollIntoView?.({block:"center"});
  }, [scrollTargetKey]);
  const effectiveSelection = new Set([...selectedRows].filter(key=>visibleKeys.includes(key)));
  const selectionPayload = JSON.stringify([...effectiveSelection].sort());
  const previousSelectionPayload = useRef("[]");
  useEffect(() => {
    if (previousSelectionPayload.current !== selectionPayload) {
      previousSelectionPayload.current = selectionPayload;
      onSelectionChange?.(JSON.parse(selectionPayload) as string[]);
    }
  }, [selectionPayload, onSelectionChange]);
  const statusPayload = statusPath === undefined ? null : JSON.stringify({path:statusPath,collections:statusCollections,...browserStats(visibleRows,effectiveSelection)});
  useEffect(() => {
    if (statusPayload) useBrowserStatusStore.getState().setStats(JSON.parse(statusPayload));
  }, [statusPayload]);
  function preventModifiedTextSelection(event: ReactMouseEvent) {
    if (event.button === 0 && (event.shiftKey || event.ctrlKey || event.metaKey)
      && !(event.target as HTMLElement).closest("input,textarea,[contenteditable=true]")) {
      event.preventDefault();
    }
  }
  function selectRow(event:ReactMouseEvent, key:string) {
    if((event.target as HTMLElement).closest("input"))return;
    if (event.shiftKey || event.ctrlKey || event.metaKey
      || (event.target as HTMLElement).closest("button[data-row-activate]")
      || !(event.target as HTMLElement).closest("button,a,textarea,[contenteditable=true]")) {
      (event.currentTarget as HTMLElement).focus({preventScroll:true});
    }
    const additive=event.ctrlKey || event.metaKey;
    if(event.shiftKey && anchor.current && visibleKeys.includes(anchor.current)) {
      const start=visibleKeys.indexOf(anchor.current), end=visibleKeys.indexOf(key);
      const range=visibleKeys.slice(Math.min(start,end),Math.max(start,end)+1);
      setSelectedRows(new Set([...(additive ? effectiveSelection : []),...range]));
    } else if(additive) {
      const next=new Set(effectiveSelection);if(next.has(key))next.delete(key);else next.add(key);
      setSelectedRows(next);anchor.current=key;
    } else {setSelectedRows(new Set([key]));anchor.current=key;}
    if(additive || event.shiftKey){event.preventDefault();event.stopPropagation();}
  }
  const batchEntries = [...new Map(visibleRows.filter(row=>effectiveSelection.has(row.key)).flatMap(row=>{
    const items = row.skill ? [row.skill] : row.folder?.batchOperations ? [] : row.folder?.children ?? [];
    if(row.folder?.batchOperations)return [{key:row.key,name:row.folder.name,operations:row.folder.batchOperations,agents:row.folder.installAgents}];
    return items.map(skill=>({key:`skill:${skill.rowKey ?? skill.name}`,name:skill.name,operations:skill.batchOperations,agents:skill.installAgents}));
  }).map(entry=>[entry.key,entry])).values()];
  const selectionClass = (key:string, highlighted?:boolean) => effectiveSelection.size ? effectiveSelection.has(key) : highlighted;
  const [rowMenu, setRowMenu] = useState<{kind:"skill"|"folder"; key:string; x:number; y:number} | null>(null);
  const menuReturnFocus = useRef<HTMLElement | null>(null);
  const menuRef = useRef<HTMLDivElement>(null);
  const menuSkill = rowMenu?.kind === "skill" ? [...skills, ...folders.flatMap(folder => folder.children ?? [])].find(skill => (skill.rowKey ?? skill.name) === rowMenu.key) : undefined;
  const menuFolder = rowMenu?.kind === "folder" ? folders.find(folder => folder.key === rowMenu.key) : undefined;
  useLayoutEffect(() => {
    if (!rowMenu || !menuRef.current) return;
    const menu = menuRef.current;
    const bounds = menu.getBoundingClientRect();
    menu.style.left = `${Math.max(4, Math.min(rowMenu.x, window.innerWidth - bounds.width - 4))}px`;
    menu.style.top = `${Math.max(4, Math.min(rowMenu.y, window.innerHeight - bounds.height - 4))}px`;
    menu.querySelector<HTMLButtonElement>("button:not(:disabled)")?.focus();
  }, [rowMenu]);
  useEffect(() => {
    if (!rowMenu) return;
    const outside = (event: PointerEvent) => { if (!menuRef.current?.contains(event.target as Node)) setRowMenu(null); };
    const dismiss = () => setRowMenu(null);
    document.addEventListener("pointerdown", outside);
    window.addEventListener("resize", dismiss);
    return () => { document.removeEventListener("pointerdown", outside); window.removeEventListener("resize", dismiss); };
  }, [rowMenu]);
  function openRowMenu(event: ReactMouseEvent, kind: "skill"|"folder", key: string) {
    event.preventDefault(); event.stopPropagation(); setColumnMenu(null);
    const rowKey=`${kind}:${key}`;if(!effectiveSelection.has(rowKey)){setSelectedRows(new Set([rowKey]));anchor.current=rowKey;}
    menuReturnFocus.current = event.currentTarget as HTMLElement;
    setRowMenu({kind,key,x:event.clientX,y:event.clientY});
  }
  const [columnMenu, setColumnMenu] = useState<{
    x: number;
    y: number;
  } | null>(null);
  const orderStorageKey = `skills-manage.skillTableOrder.${tree ? "tree" : kind}.v1`;
  const readOrder = () => {
    try {
      const saved: unknown = JSON.parse(localStorage.getItem(orderStorageKey) ?? "[]");
      return Array.isArray(saved) ? [...new Set(saved.filter((value): value is string => typeof value === "string"))] : [];
    } catch { return []; }
  };
  const [columnOrder, setColumnOrder] = useState<string[]>(readOrder);
  const draggedColumn = useRef<string | null>(null);
  const blockColumnDrag = useRef(false);
  const [dropTarget, setDropTarget] = useState<{column:string; after:boolean} | null>(null);
  useEffect(() => {
    const update = () => setColumnOrder(readOrder());
    update();
    window.addEventListener("skillshub-column-order", update);
    window.addEventListener("storage", update);
    return () => { window.removeEventListener("skillshub-column-order", update); window.removeEventListener("storage", update); };
    // The storage key identifies the table configuration.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [orderStorageKey]);
  function saveOrder(next: string[]) {
    setColumnOrder(next);
    try { localStorage.setItem(orderStorageKey, JSON.stringify(next)); } catch { /* Keep current order in memory. */ }
    window.dispatchEvent(new Event("skillshub-column-order"));
  }
  const [columnWidths, setColumnWidths] = useState<Record<string, number>>(() =>
    readColumnWidths(kind)
  );
  const dragState = useRef<{
    column: string;
    startX: number;
    startWidth: number;
    fittedWidths?: Record<string, number>;
  } | null>(null);
  const tableRef = useRef<HTMLTableElement | null>(null);
  const columns = kind === "skill"
    ? ["index", "name", "repository", "createdAt", "updatedAt", "installSummary"]
    : ["index", "name", "skillCount", ...(folders.some(folder => folder.onNotebook) ? ["notebook"] : []), ...(showGithubStars ? ["githubStars"] : []), "createdAt", "updatedAt", "installSummary"];
  const orderedColumns = [...columnOrder.filter(column => columns.includes(column)), ...columns.filter(column => !columnOrder.includes(column))];
  const activeColumns = orderedColumns.filter(
    column => column === "index" || (!tree && column === "githubStars") || visibleColumns.has(column)
  );
  const columnOptions = useMemo(() => optionsForSkillTable(kind).filter(option => (option.key !== "githubStars" || showGithubStars) && (option.key !== "notebook" || folders.some(folder => folder.onNotebook))), [kind, showGithubStars, folders]);
  const resolvedColumnWidths = useMemo(
    () =>
      Object.fromEntries(
        activeColumns.map((column) => [
          column,
          columnWidths[column] ?? (tree ? ({ index: 64, name: 280, createdAt: 112, updatedAt: 112, skillCount: 76, githubStars: 86 } as Record<string,number>)[column] : undefined) ?? DEFAULT_COLUMN_WIDTHS[column] ?? 140,
        ])
      ),
    [activeColumns, columnWidths, tree]
  );
  const tableWidth = activeColumns.reduce(
    (total, column) => total + resolvedColumnWidths[column],
    0
  );

  useEffect(() => {
    setColumnWidths(readColumnWidths(kind));
  }, [kind]);

  useEffect(() => {
    function closeMenu() {
      setColumnMenu(null);
    }
    window.addEventListener("click", closeMenu);
    window.addEventListener("keydown", closeMenu);
    return () => {
      window.removeEventListener("click", closeMenu);
      window.removeEventListener("keydown", closeMenu);
    };
  }, []);

  const persistColumnWidths = useCallback(
    (next: Record<string, number>) => {
      try {
        window.localStorage.setItem(columnWidthStorageKey(kind), JSON.stringify(next));
      } catch {
        // Keep in-memory column widths if localStorage is unavailable.
      }
      window.dispatchEvent(new CustomEvent(COLUMN_WIDTH_EVENT, { detail: { kind, widths: next } }));
    },
    [kind]
  );

  function currentFittedWidths() {
    const available = tableRef.current?.getBoundingClientRect().width || tableWidth;
    return Object.fromEntries(activeColumns.map(column => [column, resolvedColumnWidths[column] / tableWidth * available]));
  }

  function resizeWithinTable(column: string, desired: number, widths: Record<string, number>) {
    const index = activeColumns.indexOf(column);
    const neighbor = activeColumns[index + 1] ?? activeColumns[index - 1];
    if (!neighbor) return widths;
    const total = widths[column] + widths[neighbor];
    const minimum = Math.min(40, total / 2);
    const nextWidth = Math.max(minimum, Math.min(total - minimum, desired));
    return {...widths, [column]: nextWidth, [neighbor]: total - nextWidth};
  }

  function handleResizePointerDown(
    event: ReactPointerEvent<HTMLSpanElement>,
    column: string
  ) {
    event.preventDefault();
    event.stopPropagation();
    const fittedWidths = tree ? currentFittedWidths() : undefined;
    dragState.current = {
      fittedWidths,
      column,
      startX: event.clientX,
      startWidth: fittedWidths?.[column] ?? columnWidths[column] ?? (tree ? ({ index: 64, name: 280, createdAt: 112, updatedAt: 112, skillCount: 76, githubStars: 86 } as Record<string,number>)[column] : undefined) ?? DEFAULT_COLUMN_WIDTHS[column] ?? 140,
    };

    function handleMove(moveEvent: PointerEvent) {
      const state = dragState.current;
      if (!state) return;
      const width = Math.max(
        minColumnWidth(state.column),
        state.startWidth + moveEvent.clientX - state.startX
      );
      setColumnWidths((previous) => {
        const next = state.fittedWidths ? {...previous, ...resizeWithinTable(state.column, state.startWidth + moveEvent.clientX - state.startX, state.fittedWidths)} : { ...previous, [state.column]: width };
        persistColumnWidths(next);
        return next;
      });
    }

    function handleUp() {
      dragState.current = null;
      document.removeEventListener("pointermove", handleMove);
      document.removeEventListener("pointerup", handleUp);
    }

    document.addEventListener("pointermove", handleMove);
    document.addEventListener("pointerup", handleUp);
  }

  function handleAutoSizeColumn(
    event: ReactMouseEvent<HTMLSpanElement>,
    column: string
  ) {
    event.preventDefault();
    event.stopPropagation();
    const columnIndex = activeColumns.indexOf(column);
    if (columnIndex < 0 || !tableRef.current) return;

    const measuredWidth = Array.from(tableRef.current.rows).reduce((maximum, row) => {
      const cell = row.cells.item(columnIndex);
      if (!cell) return maximum;
      return Math.max(maximum, measureCellFitWidth(cell, column));
    }, minColumnWidth(column));
    const width = clampAutoFitWidth(column, measuredWidth);
    setColumnWidths((previous) => {
      const next = tree ? {...previous, ...resizeWithinTable(column, width, currentFittedWidths())} : { ...previous, [column]: width };
      persistColumnWidths(next);
      return next;
    });
  }

  function handleHeaderContextMenu(event: ReactMouseEvent<HTMLTableCellElement>) {
    if (!onToggleColumn && !onResetColumns) return;
    event.preventDefault();
    setColumnMenu({ x: event.clientX, y: event.clientY });
  }

  function renderHeaderContent(column: string) {
    const label = columnLabel(column);
    const sortable = onSortChange && isSortableColumn(column, kind);
    const selected = sortable && sortField === column;
    if (!sortable) return label;
    const SortIcon = selected ? (sortDirection === "asc" ? ArrowUp : ArrowDown) : ArrowUpDown;

    return (
      <button
        type="button"
        className={cn(
          "inline-flex max-w-full items-center gap-1 rounded-md text-left leading-5 transition-colors hover:text-primary",
          selected && "font-normal text-foreground"
        )}
        aria-label={selected ? sortHeaderLabel(t, label, sortDirection) : label}
        onClick={() =>
          onSortChange(
            column,
            nextSkillSortDirection(sortField ?? "name", sortDirection, column)
          )
        }
      >
        <span className="truncate">{label}</span>
        <SortIcon aria-hidden="true" className={cn("size-4 shrink-0", selected ? "text-primary" : "text-muted-foreground/60")} />
      </button>
    );
  }

  const renderSkillRow = (skill: SkillTableItem, skillIndex: number, nested = false, parentIndex = 0, lastChild = false) => (
                  <tr draggable onDragStart={event => {
                    const key = `skill:${skill.rowKey ?? skill.name}`;
                    const rows = effectiveSelection.has(key) ? visibleRows.filter(row => effectiveSelection.has(row.key)) : [{skill}];
                    const ids = rows.flatMap(row => row.skill && !row.skill.isReadOnly ? [row.skill.detailRequest?.skillId ?? row.skill.rowKey ?? row.skill.name] : []);
                    if (!ids.length) {event.preventDefault();return;}
                    // Dragging does not dispatch the click that normally transfers title focus to the row.
                    event.currentTarget.focus({preventScroll:true});
                    window.getSelection()?.removeAllRanges();
                    event.dataTransfer.setData("application/skillshub-skills",JSON.stringify(ids));
                    event.dataTransfer.effectAllowed="copyMove";
                  }} onMouseDownCapture={preventModifiedTextSelection} aria-selected={effectiveSelection.has(`skill:${skill.rowKey ?? skill.name}`)} onClickCapture={event=>selectRow(event,`skill:${skill.rowKey ?? skill.name}`)} tabIndex={0} onContextMenu={event => openRowMenu(event, "skill", skill.rowKey ?? skill.name)} onKeyDown={event => { if(event.key === "ContextMenu" || (event.shiftKey && event.key === "F10")) { event.preventDefault(); if(!effectiveSelection.has(`skill:${skill.rowKey ?? skill.name}`))setSelectedRows(new Set([`skill:${skill.rowKey ?? skill.name}`])); menuReturnFocus.current = event.currentTarget; const rect=event.currentTarget.getBoundingClientRect(); setRowMenu({kind:"skill",key:skill.rowKey ?? skill.name,x:rect.left+40,y:rect.top+20}); } }} onClick={(event) => { if (tree && !(event.target as HTMLElement).closest("button,input,a")) skill.onDetail?.(event as unknown as ReactMouseEvent<HTMLButtonElement>); }} key={skill.rowKey ?? skill.name} ref={node => { const key = `skill:${skill.rowKey ?? skill.name}`; if (node) rowNodes.current.set(key, node); else rowNodes.current.delete(key); }} className={cn("align-middle select-none", !selectionClass(`skill:${skill.rowKey ?? skill.name}`,skill.highlighted) && "hover:bg-muted/25", selectionClass(`skill:${skill.rowKey ?? skill.name}`,skill.highlighted) && "bg-primary/10 ring-1 ring-inset ring-primary/40")}>
                    {activeColumns.map((column) => {
                      if (column === "index") {
                        return (
                          <td
                            key={column}
                            className="px-2 py-2 tabular-nums text-muted-foreground"
                          >
                            {nested ? `${parentIndex + 1}.${skillIndex + 1}` : skillIndex + 1}
                          </td>
                        );
                      }
                      if (column === "name") {
                        return (
                          <td key={column} className={cn("relative px-3 py-2", nested && "pl-14")}>
                            <div className="flex min-w-0 items-center gap-2">
                              {nested && <span aria-hidden="true" className={cn("pointer-events-none absolute left-7 top-0 w-5 border-l border-border", lastChild ? "h-5" : "h-full")}><span className="absolute top-5 left-0 w-4 border-t border-border" /></span>}
                              {tree && <FileText className="size-[1em] shrink-0 text-muted-foreground" />}
                              {skill.checkbox && !skill.isReadOnly ? (
                                <input
                                  type="checkbox"
                                  checked={skill.checkbox.checked}
                                  onChange={skill.checkbox.onChange}
                                  aria-label={skill.checkbox.ariaLabel}
                                  className="mt-0.5 size-4 accent-primary"
                                />
                              ) : null}
                              <div className="min-w-0">
                                {skill.onDetail ? (
                                  <button
                                    data-row-activate
                                    ref={skill.detailButtonRef}
                                    type="button"
                                    className="block max-w-full truncate text-left font-normal text-foreground"
                                    aria-label={t("central.viewDetailsLabel", {
                                      name: skill.name,
                                    })}
                                    onClick={skill.onDetail}
                                  >
                                    {skill.name}
                                  </button>
                                ) : (
                                  <div className="truncate font-normal text-foreground">{skill.name}</div>
                                )}
                              </div>
                            </div>
                          </td>
                        );
                      }
                      if (column === "repository") {
                        return <td key={column} className="px-3 py-2 text-muted-foreground" title={skill.sourceRepo ?? undefined}>{skill.sourceRepo ? formatRepositoryName(skill.sourceRepo, repositoryFirst) : t("browser.localSkills")}</td>;
                      }
                      if (column === "source") {
                        return (
                          <td key={column} className="px-3 py-2 text-muted-foreground">
                            <div className="flex flex-col gap-1">
                              <span>{sourceLabel(skill)}</span>
                              {skill.sourceType ? (
                                <SourceIndicator
                                  sourceType={skill.sourceType}
                                  sourceLocation={skill.sourceLocation}
                                />
                              ) : null}
                              {isExceptionalSkillOrigin(skill.originKind) ? (
                                <SourceOriginBadge originKind={skill.originKind} />
                              ) : null}
                            </div>
                          </td>
                        );
                      }
                      if (column === "createdAt") {
                        return <td key={column} className="px-3 py-2 text-muted-foreground">{formatDate(skill.createdAt)}</td>;
                      }
                      if (column === "updatedAt") {
                        return <td key={column} className="px-3 py-2 text-muted-foreground">{formatDate(skill.updatedAt)}</td>;
                      }
                      if (column === "installSummary") {
                        const summaryProps = skillInstallSummaryProps(skill);
                        return (
                          <td key={column} className="px-3 py-2">
                            {showInstallationSource ? renderInstallationSources(skill.installationSources) : <InstallSummaryCell {...summaryProps} />}
                          </td>
                        );
                      }
                      if (column === "tags") {
                        return <td key={column} className="px-3 py-2"><TagsCell tags={(skill.tags ?? []).map(tag => tag.label)} /></td>;
                      }
                      if (column === "notes") {
                        return <td key={column} className="px-3 py-2 text-muted-foreground"><NotesCell notes={skill.notes} /></td>;
                      }
                      if (column === "skillCount" || column === "githubStars" || column === "notebook") return <td key={column} />;
                      return <td key={column} className="px-3 py-2"><SkillActions skill={skill} /></td>;
                    })}
                  </tr>
  );

  return (
    <div className={cn("relative overflow-visible rounded-xl border border-border bg-card shadow-sm", className)}>
      <div className="overflow-x-visible">
        <table
          ref={tableRef}
          onKeyDown={event => {
            if (event.nativeEvent.isComposing || shouldIgnoreShortcutTarget(event.target)
              || event.altKey || event.ctrlKey || event.metaKey || event.shiftKey
              || !["ArrowUp", "ArrowDown", "ArrowLeft", "Home", "End"].includes(event.key)) return;
            const current = (event.target as HTMLElement).closest("tr");
            const rows = Array.from(event.currentTarget.querySelectorAll<HTMLTableRowElement>("tbody > tr"));
            const index = rows.indexOf(current as HTMLTableRowElement);
            if (index < 0) return;
            if (event.key === "ArrowLeft") {
              if (!tree || kind !== "folder" || !visibleRows[index]?.skill) return;
              let parentIndex = index - 1;
              while (parentIndex >= 0 && !visibleRows[parentIndex].folder) parentIndex--;
              const parent = visibleRows[parentIndex]?.folder;
              const parentRow = rows[parentIndex];
              if (!parent?.onToggle || !parentRow) return;
              event.preventDefault(); event.stopPropagation();
              const action = parentRow.querySelector<HTMLButtonElement>("button[data-row-activate]");
              (action ?? parentRow).click();
              if (parent.expanded) parent.onToggle();
              parentRow.focus({preventScroll:true});
              parentRow.scrollIntoView?.({block:"nearest", inline:"nearest"});
              return;
            }
            event.preventDefault(); event.stopPropagation();
            let nextIndex = index + (event.key === "ArrowDown" ? 1 : -1);
            if (event.key === "Home" || event.key === "End") {
              if (visibleRows[index]?.folder) {
                const folderIndices = visibleRows.flatMap((row, rowIndex) => row.folder ? [rowIndex] : []);
                nextIndex = event.key === "Home" ? folderIndices[0] : folderIndices[folderIndices.length - 1];
              } else {
                let first = index;
                let last = index;
                while (first > 0 && visibleRows[first - 1]?.skill) first--;
                while (visibleRows[last + 1]?.skill) last++;
                nextIndex = event.key === "Home" ? first : last;
              }
            }
            const next = rows[nextIndex];
            if (!next) return;
            const action = next.querySelector<HTMLButtonElement>("button[data-row-activate]");
            (action ?? next).click();
            next.focus({preventScroll:true});
            next.scrollIntoView?.({block:"nearest", inline:"nearest"});
          }}
          className={cn("table-fixed min-w-full text-left text-sm font-normal", compactList && "[&_*]:font-normal", tree && "[&_td]:overflow-hidden [&_td]:text-ellipsis [&_td]:whitespace-nowrap [&_th]:overflow-hidden")}
          style={{ width: tree || compactList ? "100%" : `${tableWidth}px`, "--skill-table-sticky-top": stickyHeaderTop } as CSSProperties}
        >
          <colgroup>
            {activeColumns.map((column) => (
              <col
                key={column}
                style={{ width: compactList ? (column === "index" ? "44px" : undefined) : tree ? `${resolvedColumnWidths[column] / tableWidth * 100}%` : `${resolvedColumnWidths[column]}px` }}
              />
            ))}
          </colgroup>
          <thead className={cn("sticky top-[var(--skill-table-sticky-top)] z-20 bg-background text-sm font-normal text-muted-foreground", compactList ? "shadow-[0_1px_0_0_var(--border)]" : "shadow-[0_2px_0_0_var(--border)]")}>
            <tr>
              {activeColumns.map((column) => (
                <th
                  key={column}
                  scope="col"
                  draggable
                  onPointerDownCapture={event => { blockColumnDrag.current = !!(event.target as HTMLElement).closest('[role="separator"]'); }}
                  onDragStart={event => {
                    if (blockColumnDrag.current || dragState.current) { event.preventDefault(); return; }
                    draggedColumn.current = column;
                    event.dataTransfer.effectAllowed = "move";
                    event.dataTransfer.setData("text/plain", column);
                    setColumnMenu(null); setRowMenu(null);
                  }}
                  onDragOver={event => {
                    if (!draggedColumn.current) return;
                    event.preventDefault(); event.dataTransfer.dropEffect = "move";
                    const bounds = event.currentTarget.getBoundingClientRect();
                    setDropTarget({column, after:event.clientX > bounds.left + bounds.width / 2});
                  }}
                  onDrop={event => {
                    event.preventDefault();
                    const source = draggedColumn.current;
                    if (source && source !== column) {
                      const bounds = event.currentTarget.getBoundingClientRect();
                      const after = event.clientX > bounds.left + bounds.width / 2;
                      const next = orderedColumns.filter(value => value !== source);
                      next.splice(next.indexOf(column) + (after ? 1 : 0), 0, source);
                      saveOrder(next);
                    }
                    draggedColumn.current = null; setDropTarget(null);
                  }}
                  onDragEnd={() => { draggedColumn.current = null; setDropTarget(null); }}
                  aria-label={columnLabel(column)}
                  onContextMenu={handleHeaderContextMenu}
                  className={cn(
                    "relative bg-background py-2 font-normal",
                    dropTarget?.column === column && (dropTarget.after ? "shadow-[inset_-2px_0_var(--primary)]" : "shadow-[inset_2px_0_var(--primary)]"),
                    column === "index" ? "px-2" : "px-3"
                  )}
                >
                  <div className="flex min-w-0 items-center gap-2 overflow-hidden whitespace-nowrap">
                    {renderHeaderContent(column)}
                    {(column === "name" || column === repositoryNameColumn) && <div className="ml-auto flex shrink-0 items-center gap-2 pr-1">
                      {column === repositoryNameColumn && canSwitchRepositoryName && <button type="button" title={switchNameLabel} aria-label={switchNameLabel} aria-pressed={repositoryFirst} onClick={toggleRepositoryName} className="inline-flex size-6 shrink-0 items-center justify-center rounded-md text-muted-foreground hover:bg-accent hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"><ArrowLeftRight className="size-4 shrink-0" aria-hidden="true" /></button>}
                      {column === "name" && nameHeaderAction}
                    </div>}
                  </div>
                  {column !== "actions" && !compactList && (!tree || column !== activeColumns.at(-1)) ? (
                    <span
                      role="separator"
                      aria-label={t("skillBrowser.resizeColumnLabel", {
                        column: columnLabel(column),
                      })}
                      tabIndex={0}
                      onPointerDown={(event) => handleResizePointerDown(event, column)}
                      onDoubleClick={(event) => handleAutoSizeColumn(event, column)}
                      className="group/resize absolute inset-y-0 right-0 w-2 cursor-col-resize focus-visible:outline-none"
                    >
                      <span className="absolute inset-y-1 left-1/2 w-px -translate-x-1/2 bg-border transition-all group-hover/resize:w-0.5 group-hover/resize:bg-primary/60 group-focus-visible/resize:w-0.5 group-focus-visible/resize:bg-primary/60" />
                    </span>
                  ) : null}
                </th>
              ))}
            </tr>
          </thead>
          <tbody className="divide-y divide-border">
            {kind === "skill"
              ? skills.map((skill, index) => renderSkillRow(skill, index))
              : folders.map((folder, folderIndex) => (
                  <Fragment key={folder.key}>
                  <tr onMouseDownCapture={preventModifiedTextSelection} aria-selected={effectiveSelection.has(`folder:${folder.key}`)} onClickCapture={event=>selectRow(event,`folder:${folder.key}`)} tabIndex={0} onContextMenu={event => openRowMenu(event, "folder", folder.key)} onKeyDown={event => {
                    if (tree && folder.onToggle && !event.nativeEvent.isComposing && !shouldIgnoreShortcutTarget(event.target)
                      && !event.altKey && !event.ctrlKey && !event.metaKey && !event.shiftKey
                      && (event.key === "ArrowLeft" || event.key === "ArrowRight")) {
                      event.preventDefault(); event.stopPropagation();
                      event.currentTarget.focus({preventScroll:true});
                      if ((event.key === "ArrowRight") !== Boolean(folder.expanded)) folder.onToggle();
                      return;
                    }
                    if(event.key === "ContextMenu" || (event.shiftKey && event.key === "F10")) { event.preventDefault(); if(!effectiveSelection.has(`folder:${folder.key}`))setSelectedRows(new Set([`folder:${folder.key}`])); menuReturnFocus.current = event.currentTarget; const rect=event.currentTarget.getBoundingClientRect(); setRowMenu({kind:"folder",key:folder.key,x:rect.left+40,y:rect.top+20}); } }} onClick={(event) => { if (tree && !(event.target as HTMLElement).closest("button,input,a")) folder.onOpen(); }} key={folder.key} ref={node => { const key = `folder:${folder.key}`; if (node) rowNodes.current.set(key, node); else rowNodes.current.delete(key); }} className={cn("align-middle select-none", !selectionClass(`folder:${folder.key}`,folder.highlighted) && "hover:bg-muted/25", tree && "font-normal", selectionClass(`folder:${folder.key}`,folder.highlighted) && "bg-primary/10 ring-1 ring-inset ring-primary/40")}>
                    {activeColumns.map((column) => {
                      if (column === "index") {
                        return (
                          <td
                            key={column}
                            className="px-2 py-2 tabular-nums text-muted-foreground"
                          >
                            {folderIndex + 1}
                          </td>
                        );
                      }
                      if (column === "name") {
                        return (
                          <td key={column} className="overflow-hidden px-3 py-2">
                            <div className="flex min-w-0 items-center gap-2">
                            {tree && folder.onToggle && <button type="button" aria-expanded={folder.expanded} aria-label={t(folder.expanded ? "browser.collapseFolder" : "browser.expandFolder", { name: folder.name })} onClick={folder.onToggle} className="shrink-0 text-muted-foreground">{folder.expanded ? <ChevronDown className="size-4" /> : <ChevronRight className="size-4" />}</button>}
                            {tree && <FolderOpen className="size-4 shrink-0 text-muted-foreground" />}
                            <button
                              type="button"
                              className="block min-w-0 truncate text-left font-normal text-foreground"
                              data-row-activate
                              onClick={folder.onOpen}
                            >
                              {folder.name}
                            </button>
                            </div>
                          </td>
                        );
                      }
                      if (column === "notebook") return <td key={column} className="px-3 py-2">{folder.hasNotebook && folder.onNotebook && <Button variant="ghost" size="icon" title={t("g.notebook")} aria-label={t("g.notebook")} onClick={e => { e.stopPropagation(); folder.onNotebook?.(); }}><BookOpen/></Button>}</td>;
                      if (column === "source" || column === "notes") return <td key={column} />;
                      if (column === "githubStars") {
                        return <td key={column} className="px-3 py-2 tabular-nums text-muted-foreground">{folder.githubStars == null ? "" : folder.githubStars.toLocaleString()}</td>;
                      }
                      if (column === "tags") {
                        return <td key={column} className="px-3 py-2"><TagsCell tags={folder.tags ?? []} /></td>;
                      }
                      if (column === "skillCount") {
                        return (
                          <td key={column} className="relative overflow-hidden px-3 py-2 text-muted-foreground">
                            {folder.skillCount}
                          </td>
                        );
                      }
                      if (column === "installSummary") {
                        const summaryProps = folderInstallSummaryProps(folder);
                        return (
                          <td key={column} className="px-3 py-2">
                            {showInstallationSource ? renderInstallationSources(folder.installationSources) : <InstallSummaryCell {...summaryProps} />}
                          </td>
                        );
                      }
                      if (column === "createdAt") {
                        return <td key={column} className="px-3 py-2 text-muted-foreground">{formatDate(folder.createdAt)}</td>;
                      }
                      if (column === "updatedAt") {
                        return <td key={column} className="px-3 py-2 text-muted-foreground">{formatDate(folder.updatedAt)}</td>;
                      }
                      return (
                        <td key={column} className="px-3 py-2">
                          <FolderActions folder={folder} />
                        </td>
                      );
                    })}
                  </tr>
                  {tree && folder.expanded && folder.children?.map((skill, index) => renderSkillRow(skill, index, true, folderIndex, index === folder.children!.length - 1))}
                  </Fragment>
                ))}
          </tbody>
        </table>
      </div>
      {rowMenu && (menuSkill || menuFolder) && createPortal(
        <ActionMenuContext.Provider value={{close: () => setRowMenu(null)}}>
          <div key={`${rowMenu.kind}:${rowMenu.key}`} ref={menuRef} role="menu" aria-label={menuSkill?.name ?? menuFolder?.name} className="fixed z-50 min-w-48 max-w-[calc(100vw-8px)] max-h-[calc(100vh-8px)] overflow-y-auto rounded-lg border border-border bg-popover p-1 text-sm text-popover-foreground shadow-lg [&>div]:flex-col [&>div>div]:w-full" style={{left:rowMenu.x,top:rowMenu.y}}
            onContextMenu={event => event.preventDefault()} onClick={event => event.stopPropagation()}
            onKeyDown={event => {
              if(event.key === "Escape") { event.preventDefault(); event.stopPropagation(); setRowMenu(null); menuReturnFocus.current?.focus({preventScroll:true}); }
              if(event.key === "Tab") setRowMenu(null);
              if(event.key === "ArrowDown" || event.key === "ArrowUp") {
                event.preventDefault();
                const buttons = [...event.currentTarget.querySelectorAll<HTMLButtonElement>("button:not(:disabled)")];
                const index = buttons.indexOf(document.activeElement as HTMLButtonElement);
                buttons[(index + (event.key === "ArrowDown" ? 1 : -1) + buttons.length) % buttons.length]?.focus();
              }
            }}>
            {effectiveSelection.size > 1 ? <div>
              <p className="px-2.5 py-1 text-xs text-muted-foreground">{t("browser.selectedRows",{count:effectiveSelection.size})}</p>
              {(["install","update","uninstall","delete"] as BatchAction[]).filter(action=>batchEntries.some(entry=>entry.operations?.[action])).map(action=><ActionButton key={action} label={t(`browser.batch.${action}`)} destructive={action==="delete"||action==="uninstall"} onClick={()=>setBatchRequest({action,entries:batchEntries})}>{action==="install"?<PackagePlus className="size-4"/>:action==="update"?<RefreshCw className="size-4"/>:action==="uninstall"?<PackageMinus className="size-4"/>:<Trash2 className="size-4"/>}</ActionButton>)}
            </div> : menuSkill ? <SkillActions skill={menuSkill}/> : menuFolder ? <FolderActions folder={menuFolder}/> : null}
          </div>
        </ActionMenuContext.Provider>, document.body)}
      {batchRequest && <BatchSkillActionDialog {...batchRequest} onClose={()=>setBatchRequest(null)}/> }
      {columnMenu ? (
        <div
          role="menu"
          aria-label={t("skillBrowser.columnSettings")}
          className="fixed z-50 w-56 rounded-xl border border-border bg-popover p-2 text-popover-foreground shadow-lg"
          style={{ left: columnMenu.x, top: columnMenu.y }}
          onClick={(event) => event.stopPropagation()}
        >
          <div className="mb-1 px-2 text-xs font-normal text-muted-foreground">
            {t("skillBrowser.visibleColumns")}
          </div>
          <div className="space-y-1">
            {columnOptions.map((option) => {
              const fixed = option.fixed || FIXED_COLUMNS.has(option.key);
              return (
                <label
                  key={option.key}
                  className={cn(
                    "flex items-center gap-2 rounded-lg px-2 py-1.5 text-xs",
                    fixed ? "text-muted-foreground" : "cursor-pointer hover:bg-muted"
                  )}
                >
                  <input
                    type="checkbox"
                    aria-label={columnLabel(option.key)}
                    checked={visibleColumns.has(option.key)}
                    disabled={fixed}
                    onChange={() => onToggleColumn?.(option.key)}
                    className="size-3.5 accent-primary"
                  />
                  <span>{columnLabel(option.key)}</span>
                  {fixed ? (
                    <span className="ml-auto text-[0.68rem] text-muted-foreground">
                      {t("skillBrowser.fixedColumn")}
                    </span>
                  ) : null}
                </label>
              );
            })}
          </div>
          {onResetColumns ? (
            <div className="mt-2 border-t border-border pt-2">
              <button
                type="button"
                className="flex w-full items-center gap-2 rounded-lg px-2 py-1.5 text-left text-xs text-muted-foreground transition-colors hover:bg-muted hover:text-foreground"
                onClick={() => { saveOrder([]); onResetColumns(); }}
              >
                <ActionIcon action="reset"/>{t("skillBrowser.resetColumns")}
              </button>
            </div>
          ) : null}
        </div>
      ) : null}
    </div>
  );
}
