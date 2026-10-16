import { ActionIcon } from "@/components/ui/action-icon";
import {
  Check,
  Link2,
  FolderOpen,
  Folder,
  Globe,
  Plus,
  ChevronRight,
  X,
  Loader2,
  Lock,
  Trash2,
  Calendar,
  GitBranch,
  RefreshCw,
} from "lucide-react";
import type { MouseEventHandler, Ref } from "react";
import { useTranslation } from "react-i18next";
import { Checkbox } from "@/components/ui/checkbox";
import { InlineConfirmAction } from "@/components/ui/inline-confirm-action";
import { PlatformIcon } from "@/components/platform/PlatformIcon";
import type { AgentWithStatus, PlatformSourceKind } from "@/types";
import { cn } from "@/lib/utils";
import { isInstallTargetAgent } from "@/lib/agents";
import {
  getSkillSourceLineKeys,
  isExceptionalSkillOrigin,
} from "@/lib/skillSourceDisplay";
import {
  InstallTargetsActionIcon,
  SharedHubActionIcon,
} from "@/components/skill/SkillActionIcons";

const FEATURED_AGENT_IDS = [
  "openclaw",
  "qclaw",
  "cursor",
  "trae",
  "claude-code",
  "windsurf",
  "codex",
  "qwen",
];

// ─── Platform Toggle Icon (internal) ──────────────────────────────────────────

function PlatformToggleIcon({
  agent,
  skillName,
  isLinked,
  isReadOnly,
  isToggling,
  onToggle,
}: {
  agent: AgentWithStatus;
  skillName: string;
  isLinked: boolean;
  isReadOnly: boolean;
  isToggling: boolean;
  onToggle: () => void;
}) {
  const { t } = useTranslation();
  return (
    <button
      type="button"
      className={cn(
        "inline-flex h-6 w-6 items-center justify-center rounded-md transition-colors cursor-pointer",
        isLinked
          ? "text-primary hover:bg-primary/10"
          : "text-muted-foreground/40 hover:bg-muted/60 hover:text-muted-foreground",
        isReadOnly && "cursor-default hover:bg-transparent",
        isToggling && "animate-pulse pointer-events-none"
      )}
      title={agent.display_name}
      aria-label={t("central.toggleInstallLabel", { platform: agent.display_name, skill: skillName })}
      aria-pressed={isLinked}
      disabled={isToggling || isReadOnly}
      onClick={onToggle}
    >
      <PlatformIcon
        agentId={agent.id}
        className={cn(
          "size-4 shrink-0 transition-all",
          isLinked ? "opacity-100 grayscale-0" : "opacity-40 grayscale"
        )}
        size={14}
      />
    </button>
  );
}

// ─── Types ────────────────────────────────────────────────────────────────────

export interface UnifiedSkillCardProps {
  /** Core data — always required. */
  rowKey?: string;
  name: string;
  description?: string;
  className?: string;

  /** Click the card itself (platform variant navigates to detail). */
  onClick?: () => void;

  // ── optional multi-select and source badges ──
  checkbox?: { checked: boolean; onChange: () => void; ariaLabel?: string };
  isCentral?: boolean;
  platformBadge?: { id: string; name: string };
  projectBadge?: string;

  // ── central variant ──
  platformIcons?: {
    agents: AgentWithStatus[];
    linkedAgents: string[];
    readOnlyAgents?: string[];
    skillId: string;
    onToggle: (skillId: string, agentId: string) => void;
    onManage?: () => void;
    togglingAgentId: string | null;
  };

  // ── platform variant ──
  sourceType?: "symlink" | "copy" | "native";
  sourceLocation?: "central" | "resource-library" | "standalone";
  originKind?: PlatformSourceKind | null;
  isReadOnly?: boolean;

  // ── metadata badges ──
  isInstalled?: boolean;
  tags?: { key: string; label: string }[];
  publisher?: string;
  sourceAuthor?: string | null;
  sourceRepo?: string | null;
  sourceUrl?: string | null;
  createdAt?: string | null;
  updatedAt?: string | null;
  notes?: string | null;

  // ── actions (pass only the ones relevant to the context) ──
  onDetail?: MouseEventHandler<HTMLButtonElement>;
  onInstallTo?: () => void;
  onInstallToCentral?: () => void;
  installToCentralLabel?: string;
  onInstallToPlatform?: () => void;
  onUninstallFromPlatform?: () => void;
  uninstallFromLabel?: string;
  onDeleteFromCentral?: () => void;
  deleteFromCentralLabel?: string;
  deleteFromCentralRequiresDialog?: boolean;
  onUpdateFromSource?: () => void;
  updateFromSourceLabel?: string;
  onInstall?: () => void;
  onRemove?: () => void;
  isLoading?: boolean;
  detailButtonRef?: Ref<HTMLButtonElement>;
}

// ─── UnifiedSkillCard ─────────────────────────────────────────────────────────

export function UnifiedSkillCard(props: UnifiedSkillCardProps) {
  const { t } = useTranslation();
  const {
    name,
    description,
    className,
    onClick,
    checkbox,
    isCentral,
    platformBadge,
    projectBadge,
    platformIcons,
    sourceType,
    sourceLocation,
    originKind,
    isReadOnly,
    isInstalled,
    tags,
    publisher,
    sourceAuthor,
    sourceRepo,
    sourceUrl,
    createdAt,
    updatedAt,
    onDetail,
    onInstallTo,
    onInstallToCentral,
    installToCentralLabel,
    onInstallToPlatform,
    onUninstallFromPlatform,
    uninstallFromLabel,
    onDeleteFromCentral,
    deleteFromCentralLabel,
    deleteFromCentralRequiresDialog,
    onUpdateFromSource,
    updateFromSourceLabel,
    onInstall,
    onRemove,
    isLoading,
    detailButtonRef,
  } = props;

  // Determine variant features
  const hasCheckbox = !!checkbox;
  const hasPlatformIcons = !!platformIcons;
  const hasActions = !!(
    onDetail ||
    onInstallTo ||
    onInstallToCentral ||
    onInstallToPlatform ||
    onUninstallFromPlatform ||
    onDeleteFromCentral ||
    onUpdateFromSource ||
    onInstall ||
    onRemove
  );

  // Show a featured subset of software platforms on the card; the rest open via Manage.
  const targetPlatformAgents = platformIcons?.agents.filter(isInstallTargetAgent) ?? [];
  const linkedAgentIds = new Set(platformIcons?.linkedAgents ?? []);
  const readOnlyAgentIds = new Set(platformIcons?.readOnlyAgents ?? []);
  const featuredAgents = FEATURED_AGENT_IDS
    .map((agentId) => targetPlatformAgents.find((agent) => agent.id === agentId))
    .filter((agent): agent is AgentWithStatus => !!agent);
  const featuredAgentIds = new Set(featuredAgents.map((agent) => agent.id));
  const hiddenPlatformCount = targetPlatformAgents.filter((agent) => !featuredAgentIds.has(agent.id)).length;
  const sourceLabel = sourceRepo ?? sourceAuthor;

  // ── Platform variant: clickable card style ──
  if (onClick && !hasActions && !hasCheckbox && !hasPlatformIcons) {
    return (
      <button
        role="button"
        onClick={onClick}
        className={cn(
          "w-full h-full text-left focus:outline-none focus-visible:ring-2 focus-visible:ring-ring rounded-xl",
          className
        )}
        aria-label={t("platform.searchSkillLabel", { name })}
      >
        <div className="h-full flex flex-col rounded-xl bg-card ring-1 ring-border shadow-sm p-3 gap-3 transition-all hover:ring-primary/25 hover:bg-accent/30 cursor-pointer">
          <div className="flex flex-1 items-start justify-between gap-3">
            <div className="min-w-0 flex-1 space-y-1">
              <div className="font-medium text-sm text-foreground truncate">{name}</div>
              {description && (
                <p className="text-xs text-muted-foreground line-clamp-2 leading-relaxed">{description}</p>
              )}
              {sourceType && <SourceIndicator sourceType={sourceType} sourceLocation={sourceLocation} />}
            </div>
            <ChevronRight className="size-4 text-muted-foreground shrink-0 mt-0.5" />
          </div>
        </div>
      </button>
    );
  }

  // ── Default card style (central, resource, collection) ──
  return (
    <div
      className={cn(
        "rounded-xl bg-card ring-1 ring-border shadow-sm p-3 flex flex-col transition-colors",
        checkbox?.checked && "ring-primary/40 bg-primary/5",
        isLoading && "opacity-50",
        className
      )}
    >
      <div className="flex items-start gap-2.5">
        {/* Optional checkbox */}
        {hasCheckbox && (
          <div className="pt-0.5">
            <Checkbox
              checked={checkbox.checked}
              onCheckedChange={checkbox.onChange}
              aria-label={checkbox.ariaLabel ?? t("common.selectSkill")}
            />
          </div>
        )}

        {/* Main content */}
        <div className="flex-1 min-w-0 space-y-1.5">
          {/* Row 1: Name + icon actions */}
          <div className="flex items-center justify-between gap-2">
            {/* Skill name — clickable if onDetail provided */}
            {onDetail ? (
              <button
                ref={detailButtonRef}
                className="font-medium text-sm text-foreground truncate hover:text-primary hover:underline text-left min-w-0 flex-1"
                onClick={onDetail}
                aria-label={t("central.viewDetailsLabel", { name })}
              >
                {name}
              </button>
            ) : (
              <h3 className="text-sm font-semibold truncate min-w-0 flex-1">{name}</h3>
            )}

            {/* Icon action buttons */}
            {hasActions && (
              <div className="flex items-center gap-0.5 shrink-0">
                {/* Install to Shared Hub */}
                {onInstallToCentral && !isCentral && (
                  <button
                    onClick={onInstallToCentral}
                    disabled={isLoading}
                    title={installToCentralLabel ?? t("skillCard.installToCentral")}
                    aria-label={installToCentralLabel ?? t("skillCard.installToCentral")}
                    className="inline-flex h-8 w-8 items-center justify-center rounded-md transition-colors text-muted-foreground hover:bg-primary/10 hover:text-primary disabled:opacity-50 disabled:cursor-default"
                  >
                    {isLoading ? <Loader2 className="size-4 animate-spin" /> : <SharedHubActionIcon installed={false} />}
                  </button>
                )}

                {/* Install To... (central / platform / collection) */}
                {onInstallTo && (
                  <button
                    onClick={onInstallTo}
                    disabled={isLoading}
                    title={t("central.installTo")}
                    aria-label={t("central.installLabel", { name })}
                    className="inline-flex h-8 w-8 items-center justify-center rounded-md transition-colors text-muted-foreground hover:bg-primary/10 hover:text-primary disabled:opacity-50 disabled:cursor-default"
                  >
                    <InstallTargetsActionIcon />
                  </button>
                )}

                {/* Install to Platform */}
                {onInstallToPlatform && (
                  <button
                    onClick={onInstallToPlatform}
                    disabled={isLoading}
                    title={t("skillCard.installToPlatform")}
                    aria-label={t("skillCard.installToPlatform")}
                    className="inline-flex h-8 w-8 items-center justify-center rounded-md transition-colors text-muted-foreground hover:bg-primary/10 hover:text-primary disabled:opacity-50 disabled:cursor-default"
                  >
                    {isLoading ? <Loader2 className="size-4 animate-spin" /> : <Plus className="size-4" />}
                  </button>
                )}

                {onUninstallFromPlatform && (
                  <InlineConfirmAction
                    onConfirm={onUninstallFromPlatform}
                    isLoading={isLoading}
                    idleTitle={uninstallFromLabel ?? t("common.uninstall")}
                    idleAriaLabel={uninstallFromLabel ?? t("common.uninstall")}
                    confirmLabel={t("common.confirmDelete")}
                    icon={<X className="size-4" />}
                  />
                )}

                {onDeleteFromCentral &&
                  (deleteFromCentralRequiresDialog ? (
                    <button
                      onClick={onDeleteFromCentral}
                      disabled={isLoading}
                      title={deleteFromCentralLabel ?? t("common.delete")}
                      aria-label={deleteFromCentralLabel ?? t("common.delete")}
                      className="inline-flex h-8 w-8 items-center justify-center rounded-md transition-colors text-muted-foreground hover:text-destructive hover:bg-destructive/10 disabled:opacity-50 disabled:cursor-default"
                    >
                      {isLoading ? <Loader2 className="size-4 animate-spin" /> : <Trash2 className="size-4" />}
                    </button>
                  ) : (
                    <InlineConfirmAction
                      onConfirm={onDeleteFromCentral}
                      isLoading={isLoading}
                      idleTitle={deleteFromCentralLabel ?? t("common.delete")}
                      idleAriaLabel={deleteFromCentralLabel ?? t("common.delete")}
                      confirmLabel={t("common.confirmDelete")}
                      icon={<Trash2 className="size-4" />}
                    />
                  ))}

                {onUpdateFromSource && (
                  <button
                    onClick={onUpdateFromSource}
                    disabled={isLoading}
                    title={updateFromSourceLabel ?? t("central.updateFromSource")}
                    aria-label={updateFromSourceLabel ?? t("central.updateFromSource")}
                    className="inline-flex h-8 w-8 items-center justify-center rounded-md transition-colors text-muted-foreground hover:bg-primary/10 hover:text-primary disabled:opacity-50 disabled:cursor-default"
                  >
                    {isLoading ? <Loader2 className="size-4 animate-spin" /> : <RefreshCw className="size-4" />}
                  </button>
                )}

                {/* Installed indicator (disabled Check icon) */}
                {onInstall && isInstalled && (
                  <button
                    disabled
                    title={t("common.installed")}
                    aria-label={t("common.installed")}
                    className="inline-flex h-8 w-8 items-center justify-center rounded-md text-primary cursor-default"
                  >
                    <Check className="size-4" />
                  </button>
                )}

                {/* Remove (collection) */}
                {onRemove && (
                  <InlineConfirmAction
                    onConfirm={onRemove}
                    isLoading={isLoading}
                    idleTitle={t("collection.removeSkillLabel", { name })}
                    idleAriaLabel={t("collection.removeSkillLabel", { name })}
                    confirmLabel={t("common.confirmDelete")}
                    icon={<X className="size-4" />}
                  />
                )}
              </div>
            )}
          </div>

          {/* Row 2: Description — full width, not compressed by actions */}
          {description && (
            <p className="text-xs text-muted-foreground line-clamp-2 leading-relaxed">{description}</p>
          )}

          {/* Row 3: Info badges */}
          <div className="flex flex-wrap items-center gap-1.5 empty:hidden">
            {isExceptionalSkillOrigin(originKind) && (
              <SourceOriginBadge originKind={originKind} />
            )}
            {isReadOnly && <ReadOnlyBadge />}

            {/* Source indicator (platform) */}
            {sourceType && <SourceIndicator sourceType={sourceType} sourceLocation={sourceLocation} />}

            {/* "Already in Shared Hub" badge */}
            {isCentral && (
              <span className="inline-flex items-center gap-1 text-xs text-muted-foreground bg-muted/50 px-1.5 py-0.5 rounded">
                <Globe className="size-4" />
                {t("skillCard.alreadyCentral")}
              </span>
            )}

            {/* Platform badge */}
            {platformBadge && (
              <span className="inline-flex items-center gap-1 text-xs text-muted-foreground">
                <PlatformIcon agentId={platformBadge.id} className="size-3" />
                {platformBadge.name}
              </span>
            )}

            {/* Project badge */}
            {projectBadge && (
              <span className="inline-flex items-center gap-1 text-xs text-muted-foreground">
                <Folder className="size-4" />
                {projectBadge}
              </span>
            )}

            {/* Source label */}
            {publisher && (
              <span className="text-xs text-muted-foreground truncate">{publisher}</span>
            )}

            {sourceLabel && (
              <span
                className="inline-flex max-w-full items-center gap-1 truncate rounded bg-muted/50 px-1.5 py-0.5 text-xs text-muted-foreground"
                title={sourceUrl ?? sourceLabel}
              >
                <GitBranch className="size-4 shrink-0" />
                <span className="truncate">{sourceLabel}</span>
              </span>
            )}

            <DateBadge
              label={t("detail.createdAt", { defaultValue: "Created" })}
              value={createdAt}
            />
            <DateBadge
              label={t("detail.updatedAt", { defaultValue: "Updated" })}
              value={updatedAt}
            />

            {/* Tags */}
            {tags && tags.length > 0 && (
              <div className="flex items-center gap-1">
                {tags.slice(0, 2).map((tag) => (
                  <span key={tag.key} className="text-xs bg-muted/60 text-muted-foreground px-1.5 py-0.5 rounded">
                    {tag.label}
                  </span>
                ))}
              </div>
            )}
          </div>

          {/* Row 3: Platform toggles (central) */}
          {hasPlatformIcons && targetPlatformAgents.length > 0 && (
            <div className="mt-auto space-y-1 pt-1">
              <div className="flex items-center gap-1.5">
                <span className="w-14 shrink-0 text-xs font-medium uppercase tracking-wider text-muted-foreground/70">
                  {t("sidebar.softwarePlatforms")}
                </span>
                <div className="flex min-w-0 flex-1 items-center gap-0.5 overflow-hidden">
                  {featuredAgents.map((agent) => {
                    const isReadOnlyAgent = readOnlyAgentIds.has(agent.id);
                    return (
                      <PlatformToggleIcon
                        key={agent.id}
                        agent={agent}
                        skillName={name}
                        isLinked={linkedAgentIds.has(agent.id) || isReadOnlyAgent}
                        isReadOnly={isReadOnlyAgent}
                        isToggling={platformIcons.togglingAgentId === agent.id}
                        onToggle={() => platformIcons.onToggle(platformIcons.skillId, agent.id)}
                      />
                    );
                  })}
                  {hiddenPlatformCount > 0 && (
                    <span className="ml-0.5 rounded-md bg-muted/60 px-1.5 py-0.5 text-xs font-medium text-muted-foreground">
                      +{hiddenPlatformCount}
                    </span>
                  )}
                </div>
                {platformIcons.onManage && (
                  <button
                    type="button"
                    onClick={platformIcons.onManage}
                    className="inline-flex shrink-0 items-center gap-1 rounded-md px-2 py-1 text-xs font-medium text-primary transition-colors hover:bg-primary/10 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
                    aria-label={t("central.managePlatformsLabel", { skill: name })}
                  >
                    <ActionIcon action="edit"/>{t("central.managePlatforms")}
                  </button>
                )}
              </div>
            </div>
          )}
        </div>
      </div>
    </div>
  );
}

function formatDateLabel(value: string | null): string | null {
  if (!value) return null;
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return value.slice(0, 10);
  return date.toISOString().slice(0, 10);
}

function DateBadge({ label, value }: { label: string; value?: string | null }) {
  const dateLabel = formatDateLabel(value ?? null);
  if (!dateLabel) return null;

  return (
    <span
      className="inline-flex items-center gap-1 rounded bg-muted/50 px-1.5 py-0.5 text-xs text-muted-foreground"
      title={`${label}: ${value ?? dateLabel}`}
    >
      <Calendar className="size-4 shrink-0" />
      <span className="text-muted-foreground/80">{label}</span>
      <span>{dateLabel}</span>
    </span>
  );
}

// ─── Source Indicator (internal) ──────────────────────────────────────────────

function SourceIndicator({
  sourceType,
  sourceLocation = "standalone",
}: {
  sourceType: string;
  sourceLocation?: "central" | "resource-library" | "standalone";
}) {
  const { t } = useTranslation();
  const isSymlink = sourceType === "symlink";
  const { label, hint } = getSkillSourceLineKeys(sourceType, sourceLocation);

  return (
    <div
      className={cn(
        "inline-flex items-center gap-1 text-xs font-medium",
        isSymlink ? "text-primary/80" : "text-muted-foreground"
      )}
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
        "inline-flex items-center rounded-full bg-violet-500/10 px-2 py-0.5 text-xs font-medium text-violet-700 ring-1 ring-violet-500/20 dark:text-violet-300"
      )}
    >
      {label}
    </span>
  );
}

function ReadOnlyBadge() {
  const { t, i18n } = useTranslation();
  const label = t("platform.readOnly", {
    defaultValue: i18n.language.startsWith("zh") ? "只读" : "Read-only",
  });
  const description = t("platform.readOnlyHint", {
    defaultValue: i18n.language.startsWith("zh")
      ? "来自共享中心兼容目录的只读可见项，不是当前平台的可删除安装。"
      : "Visible from a Shared Hub compatibility directory; this is not a removable install in the current platform.",
  });

  return (
    <span
      className="inline-flex items-center gap-1 rounded-full bg-muted px-2 py-0.5 text-xs font-medium text-muted-foreground ring-1 ring-border/70"
      title={description}
      aria-label={`${label}: ${description}`}
    >
      <Lock className="size-4 shrink-0" />
      {label}
    </span>
  );
}
