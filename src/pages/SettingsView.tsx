import { HintIcon } from "@/components/ui/hint-icon";
import { NotebookSettings } from "@/components/notebook/NotebookSettings";
import { ActionIcon } from "@/components/ui/action-icon";
import { PlatformIcon } from "@/components/platform/PlatformIcon";
import { useAiConnectionStore, type AiConnections } from "@/stores/aiConnectionStore";
import { X } from "lucide-react";
import { AiPromptSettings } from "@/components/settings/AiPromptSettings";
import { useCollectionStore } from "@/stores/collectionStore";
import { KeyboardEvent as ReactKeyboardEvent, useEffect, useMemo, useRef, useState } from "react";
import { Plus, Trash2, Pencil, Loader2, FolderOpen, AppWindow, Info, Database, Globe, Bot, MessageSquareText, ChevronDown, ChevronRight, KeyRound, Download, Upload, RefreshCw, ExternalLink, Save, Keyboard, RotateCcw, Plug } from "lucide-react";
import { invoke } from "@tauri-apps/api/core";
import { open as openDialog, save } from "@tauri-apps/plugin-dialog";
import { toast } from "sonner";
import { useTranslation } from "react-i18next";
import i18n from "@/i18n";

import { Button } from "@/components/ui/button";
import {
  Card,
  CardContent,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import { InlineConfirmAction } from "@/components/ui/inline-confirm-action";
import { Switch } from "@/components/ui/switch";
import { useSettingsStore } from "@/stores/settingsStore";
import { usePlatformStore } from "@/stores/platformStore";
import { useCentralSkillsStore } from "@/stores/centralSkillsStore";
import { useResourceLibraryStore } from "@/stores/resourceLibraryStore";
import { PlatformDialog } from "@/components/settings/PlatformDialog";
import { Input } from "@/components/ui/input";
import { AgentWithStatus, BackupOptions, WebDavBackupFile } from "@/types";
import { AI_PROVIDERS, REGION_LABELS, RegionId } from "@/data/aiProviders";
import { isInstallTargetAgent } from "@/lib/agents";
import { deriveHomeDir, formatPathForDisplay, joinPathForDisplay, normalizePathForInputDisplay } from "@/lib/path";
import { isProjectAgentId } from "@/lib/projectTargets";
import { webDavErrorDetail } from "@/lib/webdavError";
import { defaultLocalBackupFilename, formatBackupTimestamp } from "@/lib/backupTime";
import {
  DEFAULT_SHORTCUTS,
  SHORTCUT_DEFINITIONS,
  formatShortcutCombo,
  shortcutEventToCombo,
  type ShortcutActionId,
} from "@/lib/shortcutKeys";
import { cn } from "@/lib/utils";
import { useShortcutStore } from "@/stores/shortcutStore";

// ─── App constants ────────────────────────────────────────────────────────────

export const APP_VERSION = "1.4.1";
const CONFIG_DIR_FALLBACK = "~/.arsenal";
const COMPLETE_BACKUP_OPTIONS: BackupOptions = {
  includeResourceLibrary: true,
  includeCentralLibrary: false,
  includeAppConfig: true,
  includeInstallations: true,
};

interface DirectoryPathFieldProps {
  id: string;
  value: string;
  placeholder?: string;
  disabled?: boolean;
  browseAriaLabel: string;
  openAriaLabel: string;
  browseTitle: string;
  onChange: (value: string) => void;
  onCommit: (value: string) => Promise<void>;
  onOpen: (value: string) => Promise<void>;
}

function DirectoryPathField({
  id,
  value,
  placeholder,
  disabled,
  browseAriaLabel,
  openAriaLabel,
  browseTitle,
  onChange,
  onCommit,
  onOpen,
}: DirectoryPathFieldProps) {
  const { t } = useTranslation();

  async function handleBrowse() {
    const selected = await openDialog({
      directory: true,
      multiple: false,
      defaultPath: value.trim() || undefined,
      title: browseTitle,
    });
    if (typeof selected !== "string") return;
    const nextPath = normalizePathForInputDisplay(selected);
    onChange(nextPath);
    await onCommit(nextPath);
  }

  return (
    <div className="flex flex-col gap-2 sm:flex-row sm:items-center">
      <Input
        id={id}
        className="min-w-0 flex-1"
        value={value}
        placeholder={placeholder}
        disabled={disabled}
        onChange={(event) => onChange(event.target.value)}
        onKeyDown={(event) => {
          if (event.key !== "Enter") return;
          event.preventDefault();
          const trimmed = value.trim();
          if (!trimmed || disabled) return;
          void onCommit(trimmed);
        }}
      />
      <Button
        type="button"
        variant="outline"
        className="shrink-0 sm:min-w-20"
        onClick={() => void handleBrowse()}
        disabled={disabled}
        aria-label={browseAriaLabel}
      >
        {disabled ? <Loader2 className="size-4 animate-spin" /> : <FolderOpen className="size-4" />}
        <span>{t("common.browse")}</span>
      </Button>
      <Button
        type="button"
        variant="outline"
        className="shrink-0 sm:min-w-20"
        onClick={() => void onOpen(value)}
        disabled={disabled || !value.trim()}
        aria-label={openAriaLabel}
      >
        <ExternalLink className="size-4" />
        <span>{t("settings.openPath")}</span>
      </Button>
    </div>
  );
}

function RefreshItemButton({
  label,
  ariaLabel,
  isLoading,
  disabled,
  onClick,
}: {
  label: string;
  ariaLabel: string;
  isLoading: boolean;
  disabled: boolean;
  onClick: () => void;
}) {
  return (
    <Button
      type="button"
      variant="outline"
      size="sm"
      className="shrink-0"
      onClick={onClick}
      disabled={disabled || isLoading}
      aria-label={ariaLabel}
      aria-busy={isLoading}
      title={ariaLabel}
    >
      {isLoading ? (
        <Loader2 className="size-4 animate-spin" />
      ) : (
        <RefreshCw className="size-4" />
      )}
      <span>{label}</span>
    </Button>
  );
}


// ─── SoftwarePlatformRow ──────────────────────────────────────────────────────

interface SoftwarePlatformRowProps {
  agent: AgentWithStatus;
  onEdit: () => void;
  onRemove: () => void;
  onToggle: (enabled: boolean) => void;
  isRemoving: boolean;
}

function SoftwarePlatformRow({
  agent,
  onEdit,
  onRemove,
  onToggle,
  isRemoving,
}: SoftwarePlatformRowProps) {
  const { t } = useTranslation();
  const showBuiltinDetection = agent.is_builtin;
  const sharesCentral = !!agent.shares_central_skills;
  const dirKindLabel = sharesCentral ? t("settings.sharedDir") : t("settings.independentDir");
  const action = agent.is_enabled ? t("settings.enabled") : t("settings.disabled");
  return (
    <div
      className={cn(
        "flex min-w-0 items-center gap-2 border-b border-border/60 bg-transparent px-1 py-2 transition-colors last:border-b-0 hover:bg-muted/20",
        showBuiltinDetection && agent.is_detected
          ? ""
          : showBuiltinDetection
            ? "opacity-75"
            : ""
      )}
    >
      <PlatformIcon agentId={agent.id} brand displayName={agent.display_name} size={14} className="size-[1em]" />
      <div className="flex-1 min-w-0">
        <div className="flex min-w-0 flex-wrap items-center gap-2">
          <div className="truncate text-sm font-medium">
            {agent.display_name}
          </div>
          <span
            className={cn(
              "shrink-0 rounded border px-1.5 py-0.5 text-xs leading-none",
              sharesCentral
                ? "border-chart-4/40 bg-chart-4/10 text-chart-4"
                : "border-primary/30 bg-primary/10 text-primary"
            )}
            title={dirKindLabel}
          >
            {dirKindLabel}
          </span>
          {showBuiltinDetection && (
            <span
              className={cn(
                "shrink-0 rounded border px-1.5 py-0.5 text-xs leading-none",
                agent.is_detected
                  ? "border-primary/30 bg-primary/10 text-primary"
                  : "border-border/70 bg-background text-muted-foreground"
              )}
            >
              {agent.is_detected
                ? t("settings.platformDirDetected")
                : t("settings.platformDirMissing")}
            </span>
          )}
        </div>
        <div className="mt-0.5 truncate text-xs text-muted-foreground">
          {formatPathForDisplay(agent.global_skills_dir)}
        </div>
      </div>
      <div className="flex shrink-0 items-center gap-1">
        <div className="flex items-center gap-2 pr-1">
          <span className="text-xs text-muted-foreground">
            {action}
          </span>
          <Switch
            checked={agent.is_enabled}
            onCheckedChange={onToggle}
            aria-label={t("settings.enablePlatformLabel", { action, name: agent.display_name })}
          />
        </div>
        <Button
          variant="outline"
          size="sm"
          onClick={onEdit}
          aria-label={t("settings.editPlatformLabel", { name: agent.display_name })}
          className="w-7"
          title={t("settings.editPlatformLabel", { name: agent.display_name })}
        >
          <Pencil className="size-4" />
        </Button>
        <InlineConfirmAction
          onConfirm={onRemove}
          isLoading={isRemoving}
          idleAriaLabel={t("settings.removePlatformLabel", { name: agent.display_name })}
          idleTitle={t("settings.removePlatformLabel", { name: agent.display_name })}
          confirmLabel={t("common.confirmDelete")}
          icon={<Trash2 className="size-4" />}
        />
      </div>
    </div>
  );
}

function SectionStatCounts({
  detectedCount,
  enabledCount,
}: {
  detectedCount: number;
  enabledCount: number;
}) {
  const { t } = useTranslation();

  return (
    <span className="flex shrink-0 items-center gap-2 text-xs font-normal text-muted-foreground">
      <span>{t("settings.platformDetectedCount", { count: detectedCount })}</span>
      <span aria-hidden="true" className="text-muted-foreground/50">
        ·
      </span>
      <span>{t("settings.platformEnabledCount", { count: enabledCount })}</span>
    </span>
  );
}

interface SoftwarePlatformsCardProps {
  softwarePlatforms: AgentWithStatus[];
  platformError: string | null;
  removingAgent: string | null;
  onAddPlatform: () => void;
  onEditPlatform: (agent: AgentWithStatus) => void;
  onRemovePlatform: (agentId: string) => void;
  onTogglePlatform: (agentId: string, enabled: boolean) => void;
  onRefreshLocations: () => void;
  isRefreshingLocations: boolean;
}

function SoftwarePlatformsCard({
  softwarePlatforms,
  platformError,
  removingAgent,
  onAddPlatform,
  onEditPlatform,
  onRemovePlatform,
  onTogglePlatform,
  onRefreshLocations,
  isRefreshingLocations,
}: SoftwarePlatformsCardProps) {
  const { t } = useTranslation();
  const [platformsExpanded, setPlatformsExpanded] = useState(false);

  const platformDetectedCount = softwarePlatforms.filter((agent) => agent.is_detected).length;
  const platformEnabledCount = softwarePlatforms.filter((agent) => agent.is_enabled).length;

  const platformsToggleLabel = platformsExpanded
    ? t("settings.collapseSoftwarePlatforms")
    : t("settings.expandSoftwarePlatforms");


  return (
    <Card>
      <CardHeader>
        <div data-testid="settings-software-platforms-header" className="flex items-center justify-between gap-3">
          <div className="flex min-w-0 flex-wrap items-center gap-2">
            <button type="button" onClick={() => setPlatformsExpanded(value => !value)} aria-expanded={platformsExpanded} aria-label={platformsToggleLabel} className="flex items-center gap-2 text-left">
              <AppWindow className="size-4 shrink-0 text-muted-foreground" />
              <CardTitle className="font-semibold" role="heading" aria-level={2}>{t("settings.softwarePlatforms")}</CardTitle>
              {platformsExpanded ? <ChevronDown className="size-4" /> : <ChevronRight className="size-4" />}
            </button>
            <HintIcon text={t("settings.platformProjectLocationsDesc")} />

          </div>

        </div>
      </CardHeader>
      {platformsExpanded && <CardContent>
        <div className="mb-3 flex flex-wrap items-center justify-between gap-3">
            <SectionStatCounts detectedCount={platformDetectedCount} enabledCount={platformEnabledCount} />
          <div className="flex shrink-0 items-center gap-2">
            <RefreshItemButton label={t("common.refresh")} ariaLabel={t("settings.refreshPlatformsAndProjects")} isLoading={isRefreshingLocations} disabled={isRefreshingLocations} onClick={onRefreshLocations} />
            <Button variant="outline" size="sm" onClick={onAddPlatform} aria-label={t("settings.addPlatformAriaLabel")}><Plus className="size-4" /><span>{t("settings.addPlatform")}</span></Button>
          </div>
        </div>
        <div className="space-y-4">
          <div>
            {platformError && (
              <p className="mt-2 text-xs text-destructive" role="alert">
                {platformError}
              </p>
            )}
            {platformsExpanded ? (
              softwarePlatforms.length === 0 ? (
                <p className="py-4 text-center text-sm text-muted-foreground">
                  {t("settings.noPlatforms")}
                </p>
              ) : (
                <div className="mt-2 grid grid-cols-1 overflow-hidden rounded-lg border border-border/60 px-2 xl:grid-cols-2 xl:gap-x-4">
                  {softwarePlatforms.map((agent) => (
                    <SoftwarePlatformRow
                      key={agent.id}
                      agent={agent}
                      onEdit={() => onEditPlatform(agent)}
                      onRemove={() => onRemovePlatform(agent.id)}
                      onToggle={(enabled) => onTogglePlatform(agent.id, enabled)}
                      isRemoving={removingAgent === agent.id}
                    />
                  ))}
                </div>
              )
            ) : null}
          </div>


        </div>
      </CardContent>}
    </Card>
  );
}

function ShortcutRecorder({
  actionId,
  combo,
  onChange,
  onReset,
}: {
  actionId: ShortcutActionId;
  combo: string;
  onChange: (combo: string) => void;
  onReset: () => void;
}) {
  const { t } = useTranslation();
  const [isRecording, setIsRecording] = useState(false);
  const displayCombo = formatShortcutCombo(combo) || t("settings.shortcutUnset");

  function handleKeyDown(event: ReactKeyboardEvent<HTMLButtonElement>) {
    if (!isRecording) return;
    event.preventDefault();
    event.stopPropagation();

    if (event.key === "Escape") {
      setIsRecording(false);
      return;
    }

    const nextCombo = shortcutEventToCombo(event.nativeEvent);
    if (!nextCombo) return;
    onChange(nextCombo);
    setIsRecording(false);
  }

  return (
    <div className="flex shrink-0 items-center gap-2">
      <kbd
        className="min-w-32 rounded border border-border bg-muted/60 px-2 py-1 text-center text-xs font-medium text-foreground"
        aria-label={displayCombo}
      >
        {displayCombo}
      </kbd>
      <Button
        type="button"
        variant={isRecording ? "default" : "outline"}
        size="sm"

        onClick={() => setIsRecording(true)}
        onKeyDown={handleKeyDown}
        aria-label={t("settings.shortcutRecordLabel", {
          action: t(SHORTCUT_DEFINITIONS.find((definition) => definition.id === actionId)?.labelKey ?? ""),
        })}
      >
        <Keyboard className="size-4" />
        <span>{isRecording ? t("settings.shortcutRecording") : t("settings.shortcutRecord")}</span>
      </Button>
      <Button
        type="button"
        variant="ghost"
        size="icon"

        onClick={onReset}
        aria-label={t("settings.shortcutResetLabel", {
          action: t(SHORTCUT_DEFINITIONS.find((definition) => definition.id === actionId)?.labelKey ?? ""),
        })}
        disabled={combo === DEFAULT_SHORTCUTS[actionId]}
      >
        <RotateCcw className="size-4" />
      </Button>
    </div>
  );
}

function ShortcutsSettingsCard() {
  const { t } = useTranslation();
  const [expanded, setExpanded] = useState(false);
  const shortcuts = useShortcutStore((state) => state.shortcuts);
  const setShortcut = useShortcutStore((state) => state.setShortcut);
  const resetShortcut = useShortcutStore((state) => state.resetShortcut);
  const resetAllShortcuts = useShortcutStore((state) => state.resetAllShortcuts);

  return (
    <Card>
      <CardHeader>
        <div className="flex items-start justify-between gap-3">
          <div className="flex min-w-0 items-center gap-2">
            <Keyboard className="size-4 shrink-0 text-muted-foreground" />
            <div className="flex min-w-0 items-center gap-2">
              <CardTitle className="font-semibold" role="heading" aria-level={2}>
                <button type="button" className="flex items-center gap-2" aria-expanded={expanded} aria-controls="shortcut-settings-content" onClick={() => setExpanded(value => !value)}>
                  {t("settings.shortcutsTitle")}
                  {expanded ? <ChevronDown className="size-4" /> : <ChevronRight className="size-4" />}
                </button>
              </CardTitle>
              <HintIcon text={t("settings.shortcutsDesc")} />
            </div>
          </div>
          {expanded && <Button type="button" variant="outline" size="sm" onClick={resetAllShortcuts}>
            <RotateCcw className="size-4" />
            <span>{t("settings.shortcutResetAll")}</span>
          </Button>}
        </div>
      </CardHeader>
      <CardContent id="shortcut-settings-content" hidden={!expanded}>
        <div className="overflow-hidden rounded-lg border border-border">
          {SHORTCUT_DEFINITIONS.map((definition) => (
            <div
              key={definition.id}
              className="flex flex-col gap-3 border-b border-border/60 px-4 py-3 last:border-0 md:flex-row md:items-center md:justify-between"
            >
              <div className="min-w-0">
                <div className="text-sm font-medium">{t(definition.labelKey)}</div>
                <div className="mt-0.5 text-xs text-muted-foreground">
                  {t(definition.descriptionKey)}
                </div>
              </div>
              <ShortcutRecorder
                actionId={definition.id}
                combo={shortcuts[definition.id]}
                onChange={(combo) => setShortcut(definition.id, combo)}
                onReset={() => resetShortcut(definition.id)}
              />
            </div>
          ))}
        </div>
      </CardContent>
    </Card>
  );
}

// ─── SettingsView ─────────────────────────────────────────────────────────────

export function SettingsView({ onClose }: {onClose?: () => void} = {}) {
  const { t } = useTranslation();

  // ── Store State ────────────────────────────────────────────────────────────

  const scanDirectories = useSettingsStore((s) => s.scanDirectories);
  const loadScanDirectories = useSettingsStore((s) => s.loadScanDirectories);
  const addCustomAgent = useSettingsStore((s) => s.addCustomAgent);
  const updateCustomAgent = useSettingsStore((s) => s.updateCustomAgent);
  const removeCustomAgent = useSettingsStore((s) => s.removeCustomAgent);
  const toggleAgentEnabled = useSettingsStore((s) => s.toggleAgentEnabled);
  const updateCentralSkillsDir = useSettingsStore((s) => s.updateCentralSkillsDir);
  const resourceLibraryDir = useSettingsStore((s) => s.resourceLibraryDir);
  const loadResourceLibraryDir = useSettingsStore((s) => s.loadResourceLibraryDir);
  const updateResourceLibraryDir = useSettingsStore((s) => s.updateResourceLibraryDir);
  const configDir = useSettingsStore((s) => s.configDir);
  const loadConfigDir = useSettingsStore((s) => s.loadConfigDir);
  const updateConfigDir = useSettingsStore((s) => s.updateConfigDir);
  const exportAppBackup = useSettingsStore((s) => s.exportAppBackup);
  const importAppBackup = useSettingsStore((s) => s.importAppBackup);
  const listWebDavBackups = useSettingsStore((s) => s.listWebDavBackups);
  const testWebDavConnection = useSettingsStore((s) => s.testWebDavConnection);
  const uploadWebDavBackup = useSettingsStore((s) => s.uploadWebDavBackup);
  const restoreWebDavBackup = useSettingsStore((s) => s.restoreWebDavBackup);
  const backupImportProgress = useSettingsStore((s) => s.backupImportProgress);
  const deleteWebDavBackup = useSettingsStore((s) => s.deleteWebDavBackup);
  const webDavConfig = useSettingsStore((s) => s.webDavConfig);
  const isSavingWebDavConfig = useSettingsStore((s) => s.isSavingWebDavConfig);
  const loadWebDavConfig = useSettingsStore((s) => s.loadWebDavConfig);
  const saveWebDavConfig = useSettingsStore((s) => s.saveWebDavConfig);
  const githubPat = useSettingsStore((s) => s.githubPat);
  const isLoadingGitHubPat = useSettingsStore((s) => s.isLoadingGitHubPat);
  const isSavingGitHubPat = useSettingsStore((s) => s.isSavingGitHubPat);
  const loadGitHubPat = useSettingsStore((s) => s.loadGitHubPat);
  const saveGitHubPat = useSettingsStore((s) => s.saveGitHubPat);
  const clearGitHubPat = useSettingsStore((s) => s.clearGitHubPat);
  const updateInfo = useSettingsStore((s) => s.updateInfo);
  const isCheckingUpdate = useSettingsStore((s) => s.isCheckingUpdate);
  const checkAppUpdate = useSettingsStore((s) => s.checkAppUpdate);

  const agents = usePlatformStore((s) => s.agents);

  const rescan = usePlatformStore((s) => s.rescan);
  const refreshCounts = usePlatformStore((s) => s.refreshCounts);
  const isRefreshingSkills = usePlatformStore((s) => s.isRefreshing);
  const loadCentralSkills = useCentralSkillsStore((s) => s.loadCentralSkills);
  const loadResourceLibrary = useResourceLibraryStore((s) => s.loadResourceLibrary);

  const softwarePlatforms = agents.filter(
    (agent) => isInstallTargetAgent(agent) && !isProjectAgentId(agent.id)
  );
  const centralAgent = agents.find((a) => a.id === "central");
  const homeDir = useMemo(() => {
    const candidates = [
      agents.find((agent) => agent.id === "central")?.global_skills_dir,
      ...scanDirectories.map((dir) => dir.path),
      ...agents.map((agent) => agent.global_skills_dir),
    ].filter((candidate): candidate is string => Boolean(candidate));

    return candidates
      .map((candidate) => deriveHomeDir(candidate))
      .find((candidate): candidate is string => Boolean(candidate));
  }, [agents, scanDirectories]);
  const configDirDisplay = useMemo(
    () =>
      configDir
        ? formatPathForDisplay(configDir)
        : homeDir
          ? joinPathForDisplay(homeDir, ".arsenal")
          : CONFIG_DIR_FALLBACK,
    [configDir, homeDir]
  );

  // ── Local State ────────────────────────────────────────────────────────────

  // AI Provider state
  const [aiProvider, setAiProvider] = useState("claude");
  const [aiRegion, setAiRegion] = useState<RegionId>("intl");
  const [aiApiKey, setAiApiKey] = useState("");
  const [aiModel, setAiModel] = useState("");
  const [aiCustomUrl, setAiCustomUrl] = useState("");
  const [aiLoaded, setAiLoaded] = useState(false);

  const profiles = useRef<AiConnections>({});
  const loadAiConnection = useAiConnectionStore(state => state.load);
  const saveAiConnection = useAiConnectionStore(state => state.save);
  const [aiSaving, setAiSaving] = useState(false);
  const aiLoadError = t("settings.configLoadFailed");
  useEffect(() => {
    let active = true;
    void loadAiConnection().then(value => {
      if (!active) return;
      profiles.current = value.profiles;
      const config = value.profiles[value.provider];
      setAiProvider(value.provider); setAiRegion(config.region); setAiApiKey(config.apiKey);
      setAiModel(config.model); setAiCustomUrl(config.url); setAiLoaded(true);
    }).catch(() => { if (active) toast.error(aiLoadError); });
    return () => { active = false; };
  }, [loadAiConnection, aiLoadError]);

  function rememberAiDraft() {
    profiles.current[aiProvider] = {region:aiRegion, apiKey:aiApiKey, model:aiModel, url:aiCustomUrl};
  }
  async function handleSaveAiConfig() {
    rememberAiDraft();
    setAiSaving(true);
    try {
      await saveAiConnection(aiProvider, {...profiles.current});
      toast.success(t("settings.aiConfigSaved"));
      return true;
    } catch { toast.error(t("settings.configSaveFailed")); return false; }
    finally { setAiSaving(false); }
  }
  function handleProviderChange(id: string) {
    if (!aiLoaded || aiSaving) return;
    rememberAiDraft();
    const preset = AI_PROVIDERS.find(item => item.id === id);
    const config = profiles.current[id] ?? {region:preset?.regions[0] ?? "intl", apiKey:"", model:preset?.defaultModel ?? "", url:""};
    setAiProvider(id); setAiRegion(config.region); setAiApiKey(config.apiKey);
    setAiModel(config.model); setAiCustomUrl(config.url);
    setAiTestResult(null);
  }
  const currentProvider = AI_PROVIDERS.find((p) => p.id === aiProvider);
  const resolvedUrl = aiProvider === "custom"
    ? aiCustomUrl
    : (currentProvider?.endpoints[aiRegion] ?? "");
  const lang = i18n.language;
  const [aiTesting, setAiTesting] = useState(false);
  const [aiTestResult, setAiTestResult] = useState<{ ok: boolean; msg: string; details?: string } | null>(null);
  const [showAiTestDetails, setShowAiTestDetails] = useState(false);

  const [isPlatformDialogOpen, setIsPlatformDialogOpen] = useState(false);
  const [editingPlatform, setEditingPlatform] = useState<AgentWithStatus | null>(null);
  const [removingAgent, setRemovingAgent] = useState<string | null>(null);
  const [isRefreshingLocations, setIsRefreshingLocations] = useState(false);
  const refreshingLocationsRef = useRef(false);
  const [platformError, setPlatformError] = useState<string | null>(null);
  const [githubPatInput, setGitHubPatInput] = useState("");
  const [githubPatMessage, setGitHubPatMessage] = useState<{ type: "success" | "error"; text: string } | null>(null);
  const [updateMessage, setUpdateMessage] = useState<{ type: "success" | "error"; text: string } | null>(null);
  const [centralPathInput, setCentralPathInput] = useState("");
  const [isSavingCentralPath, setIsSavingCentralPath] = useState(false);
  const [centralPathMessage, setCentralPathMessage] = useState<{ type: "success" | "error"; text: string } | null>(null);
  const [resourcePathInput, setResourcePathInput] = useState("");
  const [isSavingResourcePath, setIsSavingResourcePath] = useState(false);
  const [resourcePathMessage, setResourcePathMessage] = useState<{ type: "success" | "error"; text: string } | null>(null);
  const [configDirInput, setConfigDirInput] = useState("");
  const [isSavingConfigDir, setIsSavingConfigDir] = useState(false);
  const [configDirMessage, setConfigDirMessage] = useState<{ type: "success" | "error"; text: string } | null>(null);
  const [isExportingBackup, setIsExportingBackup] = useState(false);
  const [isImportingBackup, setIsImportingBackup] = useState(false);
  const [webDavBaseUrl, setWebDavBaseUrl] = useState("");
  const [webDavUsername, setWebDavUsername] = useState("");
  const [webDavPassword, setWebDavPassword] = useState("");
  const [webDavRemoteDir, setWebDavRemoteDir] = useState("arsenal");
  const [webDavConfigMessage, setWebDavConfigMessage] = useState<{ type: "success" | "error"; text: string } | null>(null);
  const [webDavFiles, setWebDavFiles] = useState<WebDavBackupFile[]>([]);
  const [selectedWebDavPath, setSelectedWebDavPath] = useState("");
  const [isRefreshingWebDav, setIsRefreshingWebDav] = useState(false);
  const [isTestingWebDav, setIsTestingWebDav] = useState(false);
  const [isUploadingWebDav, setIsUploadingWebDav] = useState(false);
  const [isImportingWebDav, setIsImportingWebDav] = useState(false);
  const [isDeletingWebDav, setIsDeletingWebDav] = useState(false);
  const isBackupBusy = isExportingBackup || isImportingBackup || isRefreshingWebDav || isTestingWebDav || isUploadingWebDav || isImportingWebDav || isDeletingWebDav || isSavingWebDavConfig;

  // ── Load on mount ──────────────────────────────────────────────────────────

  useEffect(() => {
    loadScanDirectories();
    loadGitHubPat();
    loadResourceLibraryDir();
    loadConfigDir();
    loadWebDavConfig();
  }, [loadScanDirectories, loadGitHubPat, loadResourceLibraryDir, loadConfigDir, loadWebDavConfig]);

  useEffect(() => {
    setGitHubPatInput(githubPat);
  }, [githubPat]);

  useEffect(() => {
    if (centralAgent?.global_skills_dir) {
      setCentralPathInput(formatPathForDisplay(centralAgent.global_skills_dir));
    }
  }, [centralAgent?.global_skills_dir]);

  useEffect(() => {
    if (resourceLibraryDir) {
      setResourcePathInput(formatPathForDisplay(resourceLibraryDir));
    }
  }, [resourceLibraryDir]);

  useEffect(() => {
    if (configDirDisplay) {
      setConfigDirInput(configDirDisplay);
    }
  }, [configDirDisplay]);

  useEffect(() => {
    setWebDavBaseUrl(webDavConfig.baseUrl);
    setWebDavUsername(webDavConfig.username ?? "");
    setWebDavPassword(webDavConfig.password ?? "");
    setWebDavRemoteDir(webDavConfig.remoteDir || "arsenal");
  }, [webDavConfig]);

  useEffect(() => {
    setWebDavFiles([]);
    setSelectedWebDavPath("");
  }, [webDavBaseUrl, webDavRemoteDir, webDavUsername, webDavPassword]);

  const isGitHubPatDirty = useMemo(() => githubPatInput.trim() !== githubPat, [githubPatInput, githubPat]);
  const isWebDavConfigDirty = useMemo(
    () =>
      webDavBaseUrl.trim() !== webDavConfig.baseUrl ||
      webDavUsername !== (webDavConfig.username ?? "") ||
      webDavPassword !== (webDavConfig.password ?? "") ||
      webDavRemoteDir.trim() !== (webDavConfig.remoteDir || "arsenal"),
    [webDavBaseUrl, webDavConfig, webDavPassword, webDavRemoteDir, webDavUsername]
  );

  async function handleRefreshPlatformsAndProjects() {
    if (refreshingLocationsRef.current || isRefreshingLocations || isRefreshingSkills) return;
    refreshingLocationsRef.current = true;
    setIsRefreshingLocations(true);
    setPlatformError(null);
    try {
      await Promise.all([refreshCounts(), loadScanDirectories()]);
      const scanError = usePlatformStore.getState()?.error;
      if (scanError) {
        toast.error(t("settings.refreshError", { error: scanError }));
        return;
      }
      toast.success(t("settings.refreshPlatformsAndProjectsSuccess"));
    } catch (err) {
      toast.error(t("settings.refreshError", { error: String(err) }));
    } finally {
      refreshingLocationsRef.current = false;
      setIsRefreshingLocations(false);
    }
  }

  async function handleSaveCentralPath(nextPath = centralPathInput) {
    const trimmed = nextPath.trim();
    if (!trimmed) return;
    setIsSavingCentralPath(true);
    setCentralPathMessage(null);
    try {
      await updateCentralSkillsDir(trimmed);
      await Promise.all([rescan(), loadScanDirectories(), loadCentralSkills()]);
      const message = t("settings.centralPathSaved");
      setCentralPathMessage({ type: "success", text: message });
      toast.success(message);
    } catch (err) {
      const message = String(err);
      setCentralPathMessage({ type: "error", text: message });
      toast.error(message);
    } finally {
      setIsSavingCentralPath(false);
    }
  }

  async function handleSaveResourcePath(nextPath = resourcePathInput) {
    const trimmed = nextPath.trim();
    if (!trimmed) return;
    setIsSavingResourcePath(true);
    setResourcePathMessage(null);
    try {
      await updateResourceLibraryDir(trimmed);
      await loadResourceLibrary();
      const message = t("settings.resourcePathSaved");
      setResourcePathMessage({ type: "success", text: message });
      toast.success(message);
    } catch (err) {
      const message = String(err);
      setResourcePathMessage({ type: "error", text: message });
      toast.error(message);
    } finally {
      setIsSavingResourcePath(false);
    }
  }

  async function handleSaveConfigDir(nextPath = configDirInput) {
    const trimmed = nextPath.trim();
    if (!trimmed) return;
    setIsSavingConfigDir(true);
    setConfigDirMessage(null);
    try {
      await updateConfigDir(trimmed);
      const message = t("settings.configDirSaved");
      setConfigDirMessage({ type: "success", text: message });
      toast.success(message);
    } catch (err) {
      const message = String(err);
      setConfigDirMessage({ type: "error", text: message });
      toast.error(message);
    } finally {
      setIsSavingConfigDir(false);
    }
  }

  async function handleOpenDirectoryPath(path: string) {
    const trimmed = path.trim();
    if (!trimmed) return;
    try {
      await invoke("open_in_file_manager", { path: trimmed });
    } catch (err) {
      toast.error(t("settings.openPathError", { error: String(err) }));
    }
  }

  // ── Custom Platform Handlers ───────────────────────────────────────────────

  function handleOpenAddPlatform() {
    setEditingPlatform(null);
    setPlatformError(null);
    setIsPlatformDialogOpen(true);
  }

  function handleOpenEditPlatform(agent: AgentWithStatus) {
    setEditingPlatform(agent);
    setPlatformError(null);
    setIsPlatformDialogOpen(true);
  }

  async function handleAddPlatform(
    id: string | undefined,
    displayName: string,
    globalSkillsDir: string
  ) {
    setPlatformError(null);
    try {
      const added = await addCustomAgent({
        id,
        display_name: displayName,
        global_skills_dir: globalSkillsDir,
      });
      // Refresh agents + rescan to show new platform in sidebar.
      await rescan();
      toast.success(t("platformDialog.add") + " ✓");
      return added.id;
    } catch (err) {
      setPlatformError(String(err));
      toast.error(String(err));
      throw err;
    }
  }

  async function handleEditPlatform(id: string, displayName: string, globalSkillsDir: string) {
    if (!editingPlatform) return;
    setPlatformError(null);
    try {
      const updated = await updateCustomAgent(editingPlatform.id, {
        id,
        display_name: displayName,
        global_skills_dir: globalSkillsDir,
      });
      // Refresh agents + rescan.
      await rescan();
      toast.success(t("platformDialog.save") + " ✓");
      return updated.id;
    } catch (err) {
      setPlatformError(String(err));
      toast.error(String(err));
      throw err;
    }
  }

  async function handleTogglePlatform(agentId: string, enabled: boolean) {
    setPlatformError(null);
    try {
      await toggleAgentEnabled(agentId, enabled);
      await rescan();
    } catch (err) {
      setPlatformError(String(err));
      toast.error(String(err));
    }
  }

  async function handleRemovePlatform(agentId: string) {
    setRemovingAgent(agentId);
    setPlatformError(null);
    try {
      await removeCustomAgent(agentId);
      // Refresh agents.
      await rescan();
      toast.success(t("common.delete") + " ✓");
    } catch (err) {
      setPlatformError(String(err));
      toast.error(String(err));
    } finally {
      setRemovingAgent(null);
    }
  }

  async function handleSaveGitHubPat() {
    setGitHubPatMessage(null);
    try {
      await saveGitHubPat(githubPatInput);
      setGitHubPatMessage({
        type: "success",
        text: t("settings.githubPatSaved"),
      });
      toast.success(t("settings.githubPatSaved"));
    } catch (err) {
      const text = String(err);
      setGitHubPatMessage({ type: "error", text });
      toast.error(text);
    }
  }

  async function handleClearGitHubPat() {
    setGitHubPatMessage(null);
    try {
      await clearGitHubPat();
      setGitHubPatInput("");
      setGitHubPatMessage({
        type: "success",
        text: t("settings.githubPatCleared"),
      });
      toast.success(t("settings.githubPatCleared"));
    } catch (err) {
      const text = String(err);
      setGitHubPatMessage({ type: "error", text });
      toast.error(text);
    }
  }

  async function handleCheckAppUpdate() {
    setUpdateMessage(null);
    try {
      const info = await checkAppUpdate();
      setUpdateMessage({
        type: "success",
        text: info.isUpdateAvailable
          ? t("settings.updateAvailable", { version: info.latestVersion })
          : t("settings.updateUpToDate"),
      });
    } catch (err) {
      setUpdateMessage({
        type: "error",
        text: t("settings.updateCheckFailed", { error: String(err) }),
      });
    }
  }

  async function handleExportBackup() {
    const destPath = await save({
      defaultPath: defaultLocalBackupFilename(),
      filters: [{ name: "ZIP", extensions: ["zip"] }],
    });
    if (!destPath) {
      return;
    }

    setIsExportingBackup(true);
    try {
      await exportAppBackup(destPath, COMPLETE_BACKUP_OPTIONS);
      toast.success(t("settings.backupExported"));
    } catch (err) {
      toast.error(t("settings.backupExportError", { error: String(err) }));
    } finally {
      setIsExportingBackup(false);
    }
  }

  async function handleImportBackup() {
    setIsImportingBackup(true);
    try {
      const sourcePath = await openDialog({ multiple: false, filters: [{name: t("settings.backupTitle"), extensions: ["zip", "json"]}] });
      if (typeof sourcePath !== "string") return;
      await importAppBackup(sourcePath);
      await Promise.all([
        rescan(),
        loadScanDirectories(),
        loadCentralSkills(),
        loadResourceLibrary(),
        loadGitHubPat(),
      ]);
      toast.success(t("settings.backupImported"));
    } catch (err) {
      toast.error(t("settings.backupImportError", { error: webDavErrorDetail(t, err) }));
    } finally {
      setIsImportingBackup(false);
    }
  }

  function currentWebDavConfig() {
    return {
      baseUrl: webDavBaseUrl.trim(),
      username: webDavUsername,
      password: webDavPassword,
      remoteDir: webDavRemoteDir.trim(),
    };
  }

  async function handleSaveWebDavConfig() {
    if (!webDavBaseUrl.trim() || !webDavRemoteDir.trim()) {
      toast.error(t("settings.webdavMissingConfig"));
      return;
    }
    setWebDavConfigMessage(null);
    try {
      await saveWebDavConfig(currentWebDavConfig());
      const text = t("settings.webdavConfigSaved");
      setWebDavConfigMessage({ type: "success", text });
      toast.success(text);
    } catch (err) {
      const text = t("settings.webdavConfigSaveError", { error: String(err) });
      setWebDavConfigMessage({ type: "error", text });
      toast.error(text);
    }
  }

  async function handleRefreshWebDavBackups() {
    if (!webDavBaseUrl.trim() || !webDavRemoteDir.trim()) {
      toast.error(t("settings.webdavMissingConfig"));
      return;
    }
    setIsRefreshingWebDav(true);
    try {
      const files = await listWebDavBackups(currentWebDavConfig());
      setWebDavFiles(files);
      setSelectedWebDavPath(files[0]?.remotePath ?? "");
      toast.success(t("settings.webdavRefreshed"));
    } catch (err) {
      toast.error(t("settings.webdavRefreshError", { error: webDavErrorDetail(t, err) }));
    } finally {
      setIsRefreshingWebDav(false);
    }
  }

  async function handleTestWebDavConnection() {
    if (!webDavBaseUrl.trim() || !webDavRemoteDir.trim()) {
      toast.error(t("settings.webdavMissingConfig"));
      return;
    }
    setIsTestingWebDav(true);
    try {
      await testWebDavConnection(currentWebDavConfig());
      toast.success(t("settings.webdavTestSucceeded"));
    } catch (err) {
      toast.error(t("settings.webdavTestError", { error: webDavErrorDetail(t, err) }));
    } finally {
      setIsTestingWebDav(false);
    }
  }

  async function handleUploadWebDavBackup() {
    if (!webDavBaseUrl.trim() || !webDavRemoteDir.trim()) {
      toast.error(t("settings.webdavMissingConfig"));
      return;
    }
    setIsUploadingWebDav(true);
    try {
      await uploadWebDavBackup(currentWebDavConfig(), COMPLETE_BACKUP_OPTIONS);
    } catch (err) {
      toast.error(t("settings.webdavUploadError", { error: webDavErrorDetail(t, err) }));
      setIsUploadingWebDav(false);
      return;
    }
    toast.success(t("settings.webdavUploaded"));
    try {
      const files = await listWebDavBackups(currentWebDavConfig());
      setWebDavFiles(files);
      setSelectedWebDavPath(files[0]?.remotePath ?? "");
    } catch (err) {
      toast.error(t("settings.webdavUploadRefreshError", { error: webDavErrorDetail(t, err) }));
    } finally {
      setIsUploadingWebDav(false);
    }
  }

  async function handleDeleteSelectedWebDavBackup() {
    if (!selectedWebDavPath) {
      toast.error(t("settings.webdavMissingSelection"));
      return;
    }
    setIsDeletingWebDav(true);
    try {
      await deleteWebDavBackup(currentWebDavConfig(), selectedWebDavPath);
      const files = await listWebDavBackups(currentWebDavConfig());
      setWebDavFiles(files);
      setSelectedWebDavPath(files[0]?.remotePath ?? "");
      toast.success(t("settings.webdavDeleted"));
    } catch (err) {
      toast.error(t("settings.webdavDeleteError", { error: webDavErrorDetail(t, err) }));
    } finally {
      setIsDeletingWebDav(false);
    }
  }

  async function handleImportSelectedWebDavBackup() {
    if (!selectedWebDavPath) {
      toast.error(t("settings.webdavMissingSelection"));
      return;
    }
    setIsImportingWebDav(true);
    try {
      await restoreWebDavBackup(currentWebDavConfig(), selectedWebDavPath);
      await Promise.all([
        loadResourceLibrary(),
        useCollectionStore.getState().loadCollections(),
      ]);
      toast.success(t("settings.webdavImported"));
    } catch (err) {
      toast.error(t("settings.webdavImportError", { error: webDavErrorDetail(t, err) }));
    } finally {
      setIsImportingWebDav(false);
    }
  }

  // ── Render ─────────────────────────────────────────────────────────────────

  return (
    <div className="flex flex-col h-full">
      {/* Header */}
      <div className="border-b border-border px-4 py-4">
        <div className="flex items-center justify-between"><h1 className="text-sm font-semibold">{t("settings.title")}</h1><Button variant="ghost" size="icon" onClick={onClose} aria-label={t("common.close")}><X className="size-4" /></Button></div>
      </div>

      {/* Content */}
      <div className="flex-1 overflow-auto p-4 space-y-4">

        {/* Section 1: Path settings */}
        <Card>
          <details className="group/settings-section">
          <summary className="cursor-pointer list-none [&::-webkit-details-marker]:hidden">
          <CardHeader>
            <div className="flex items-center gap-2">
              <FolderOpen className="size-4 text-muted-foreground" />
              <div className="flex items-center gap-2">
                <CardTitle role="heading" aria-level={2} className="font-semibold flex items-center gap-2">{t("settings.pathsConfigTitle")}<ChevronRight className="size-4 shrink-0 group-open/settings-section:rotate-90" /></CardTitle>
                <HintIcon text={t("settings.pathsConfigDesc")} />
              </div>
            </div>
          </CardHeader>
          </summary>
          <CardContent className="pt-4">
            <div className="space-y-4">
              <div className="space-y-3">
                <div>
                  <label htmlFor="config-dir" className="mb-1 flex items-center gap-2 text-xs font-medium text-muted-foreground">
                    <span>{t("settings.configDirLabel")}</span>
                    <HintIcon text={`${t("settings.configDirDesc")}\n\n${t("settings.configDirHint")}`} className="size-4" />
                  </label>
                  <DirectoryPathField
                    id="config-dir"
                    value={configDirInput}
                    placeholder={CONFIG_DIR_FALLBACK}
                    disabled={isSavingConfigDir}
                    browseAriaLabel={t("settings.browseConfigDir")}
                    openAriaLabel={t("settings.openConfigDir")}
                    browseTitle={t("settings.browsePathTitle")}
                    onChange={setConfigDirInput}
                    onCommit={handleSaveConfigDir}
                    onOpen={handleOpenDirectoryPath}
                  />
                </div>
                {configDirMessage ? (
                  <p
                    className={configDirMessage.type === "error" ? "text-sm text-destructive" : "text-sm text-emerald-600 dark:text-emerald-400"}
                    role="status"
                  >
                    {configDirMessage.text}
                  </p>
                ) : null}
              </div>

              <div className="space-y-3">
                <div>
                  <label htmlFor="skill-resource-library-dir" className="mb-1 flex items-center gap-2 text-xs font-medium text-muted-foreground">
                    <span>{t("settings.resourcePathLabel")}</span>
                    <HintIcon text={`${t("settings.resourcePathDesc")}\n\n${t("settings.resourcePathHint")}`} className="size-4" />
                  </label>
                  <DirectoryPathField
                    id="skill-resource-library-dir"
                    value={resourcePathInput}
                    placeholder={`${CONFIG_DIR_FALLBACK}/library`}
                    disabled={isSavingResourcePath}
                    browseAriaLabel={t("settings.browseResourcePath")}
                    openAriaLabel={t("settings.openResourcePath")}
                    browseTitle={t("settings.browsePathTitle")}
                    onChange={setResourcePathInput}
                    onCommit={handleSaveResourcePath}
                    onOpen={handleOpenDirectoryPath}
                  />
                </div>
                {resourcePathMessage ? (
                  <p
                    className={resourcePathMessage.type === "error" ? "text-sm text-destructive" : "text-sm text-emerald-600 dark:text-emerald-400"}
                    role="status"
                  >
                    {resourcePathMessage.text}
                  </p>
                ) : null}
              </div>

              <div className="space-y-3">
                <div>
                  <label htmlFor="central-skills-dir" className="mb-1 flex items-center gap-2 text-xs font-medium text-muted-foreground">
                    <span>{t("settings.centralPathLabel")}</span>
                    <HintIcon text={`${t("settings.centralPathDesc")}\n\n${t("settings.centralPathHint")}`} className="size-4" />
                  </label>
                  <DirectoryPathField
                    id="central-skills-dir"
                    value={centralPathInput}
                    placeholder={`${CONFIG_DIR_FALLBACK}/central-skills`}
                    disabled={isSavingCentralPath}
                    browseAriaLabel={t("settings.browseCentralPath")}
                    openAriaLabel={t("settings.openCentralPath")}
                    browseTitle={t("settings.browsePathTitle")}
                    onChange={setCentralPathInput}
                    onCommit={handleSaveCentralPath}
                    onOpen={handleOpenDirectoryPath}
                  />
                </div>
                {centralPathMessage ? (
                  <p
                    className={centralPathMessage.type === "error" ? "text-sm text-destructive" : "text-sm text-emerald-600 dark:text-emerald-400"}
                    role="status"
                  >
                    {centralPathMessage.text}
                  </p>
                ) : null}
              </div>
            </div>
          </CardContent>
          </details>
        </Card>

        <SoftwarePlatformsCard
          softwarePlatforms={softwarePlatforms}
          platformError={platformError}
          removingAgent={removingAgent}
          onAddPlatform={handleOpenAddPlatform}
          onEditPlatform={handleOpenEditPlatform}
          onRemovePlatform={handleRemovePlatform}
          onTogglePlatform={handleTogglePlatform}
          onRefreshLocations={handleRefreshPlatformsAndProjects}
          isRefreshingLocations={isRefreshingLocations || isRefreshingSkills}
        />

        <NotebookSettings />

        <ShortcutsSettingsCard />

        {/* Section 2: Backup and migration */}
        <Card>
          <details className="group/settings-section">
          <summary className="cursor-pointer list-none [&::-webkit-details-marker]:hidden">
          <CardHeader className="flex flex-row items-center justify-between">
            <div className="flex items-center gap-2">
              <Database className="size-4 text-muted-foreground" />
              <div>
                <div className="flex items-center gap-2">
                  <CardTitle className="font-semibold flex items-center gap-2">{t("settings.backupTitle")}<ChevronRight className="size-4 shrink-0 group-open/settings-section:rotate-90" /></CardTitle>
                  <HintIcon text={t("settings.backupDesc")} />
                </div>
              </div>
            </div>
          </CardHeader>
          </summary>
          <CardContent className="pt-4">
            <div className="space-y-4">
              {(isImportingBackup || isImportingWebDav) && backupImportProgress && (
                <p role="status" aria-live="polite" className="text-xs text-muted-foreground">
                  {t(`settings.backupProgress.${backupImportProgress.phase}`)}
                  {backupImportProgress.phase === "downloading" && ` · ${(backupImportProgress.completedBytes / 1048576).toFixed(1)} MiB${backupImportProgress.totalBytes ? ` / ${(backupImportProgress.totalBytes / 1048576).toFixed(1)} MiB` : ""}`}
                </p>
              )}
              <div className="rounded-lg border border-border/70 bg-background/40 p-4">
                <div className="flex flex-col gap-3 md:flex-row md:items-center md:justify-between">
                  <div className="min-w-0">
                    <div className="flex items-center gap-2 text-sm font-medium">
                      <span>{t("settings.backupLocalTitle")}</span>
                      <HintIcon text={`${t("settings.backupLocalDesc")}\n\n${t("settings.backupHint")}`} />
                    </div>
                  </div>
                  <div className="flex shrink-0 flex-wrap items-center justify-end gap-2">
                    <Button variant="outline" onClick={handleExportBackup} disabled={isBackupBusy}>
                      {isExportingBackup ? <Loader2 className="size-4 animate-spin" /> : <Download className="size-4" />}
                      <span>{t("settings.exportBackup")}</span>
                    </Button>
                    <Button
                      variant="outline"
                      onClick={handleImportBackup}
                      disabled={isBackupBusy}
                    >
                      {isImportingBackup ? <Loader2 className="size-4 animate-spin" /> : <Upload className="size-4" />}
                      <span>{t("settings.importBackup")}</span>
                    </Button>

                  </div>
                </div>
              </div>

              <div className="rounded-lg border border-border/70 bg-background/40 p-4">
                <div>
                  <div className="flex items-center gap-2 text-sm font-medium">
                    <span>{t("settings.webdavTitle")}</span>
                    <HintIcon text={`${t("settings.webdavScope")}\n\n${t("settings.webdavPersistHint")}`} />
                  </div>
                </div>

                <div className="mt-4 space-y-3">
                  <div className="text-xs font-medium text-muted-foreground">
                    {t("settings.webdavConnectionTitle")}
                  </div>
                  <div className="grid gap-3 md:grid-cols-2">
                    <div>
                      <label htmlFor="webdav-url" className="mb-1 block text-xs font-medium text-muted-foreground">{t("settings.webdavUrlLabel")}</label>
                      <Input id="webdav-url" value={webDavBaseUrl} onChange={(event) => setWebDavBaseUrl(event.target.value)} placeholder={t("settings.webdavUrlPlaceholder")} />
                    </div>
                    <div>
                      <label htmlFor="webdav-remote-dir" className="mb-1 block text-xs font-medium text-muted-foreground">{t("settings.webdavRemoteDirLabel")}</label>
                      <Input id="webdav-remote-dir" value={webDavRemoteDir} onChange={(event) => setWebDavRemoteDir(event.target.value)} placeholder={t("settings.webdavRemoteDirPlaceholder")} />
                    </div>
                    <div>
                      <label htmlFor="webdav-username" className="mb-1 block text-xs font-medium text-muted-foreground">{t("settings.webdavUsernameLabel")}</label>
                      <Input id="webdav-username" value={webDavUsername} onChange={(event) => setWebDavUsername(event.target.value)} autoComplete="off" />
                    </div>
                    <div>
                      <label htmlFor="webdav-password" className="mb-1 block text-xs font-medium text-muted-foreground">{t("settings.webdavPasswordLabel")}</label>
                      <Input id="webdav-password" type="password" value={webDavPassword} onChange={(event) => setWebDavPassword(event.target.value)} autoComplete="off" />
                    </div>
                  </div>
                  <div className="flex flex-wrap items-center justify-end gap-2">
                    <Button
                      variant="outline"
                      onClick={handleTestWebDavConnection}
                      disabled={isBackupBusy || !webDavBaseUrl.trim() || !webDavRemoteDir.trim()}
                    >
                      {isTestingWebDav ? <Loader2 className="size-4 animate-spin" /> : <RefreshCw className="size-4" />}
                      <span>{t("settings.webdavTestConnection")}</span>
                    </Button>
                    <Button
                      variant="outline"
                      onClick={handleSaveWebDavConfig}
                      disabled={isBackupBusy || !isWebDavConfigDirty || !webDavBaseUrl.trim() || !webDavRemoteDir.trim()}
                    >
                      {isSavingWebDavConfig ? <Loader2 className="size-4 animate-spin" /> : <Save className="size-4" />}
                      <span>{t("settings.webdavSaveConfig")}</span>
                    </Button>
                    {webDavConfigMessage ? (
                      <span
                        className={webDavConfigMessage.type === "error" ? "text-sm text-destructive" : "text-sm text-emerald-600 dark:text-emerald-400"}
                        role="status"
                      >
                        {webDavConfigMessage.text}
                      </span>
                    ) : null}
                  </div>
                </div>

                <div className="mt-5 space-y-3 border-t border-border/60 pt-4">
                  <div className="flex flex-col gap-3 md:flex-row md:items-center md:justify-between">
                    <div className="text-xs font-medium text-muted-foreground">
                      {t("settings.webdavRemoteBackupsTitle")}
                    </div>
                  </div>
                  <div className="rounded-lg border border-border/70">
                    {webDavFiles.length === 0 ? (
                      <p className="px-3 py-3 text-xs text-muted-foreground">{t("settings.webdavNoBackups")}</p>
                    ) : (
                      webDavFiles.map((file) => (
                        <label key={file.remotePath} className="flex items-center gap-2 border-b border-border/50 px-3 py-2 text-sm last:border-0">
                          <input type="radio" name="webdav-backup-file" checked={selectedWebDavPath === file.remotePath} onChange={() => setSelectedWebDavPath(file.remotePath)} />
                          <span className="flex-1 truncate">{file.name}</span>
                          {file.modifiedAt ? (
                            <span className="text-xs text-muted-foreground">
                              {formatBackupTimestamp(file.modifiedAt)}
                            </span>
                          ) : null}
                        </label>
                      ))
                    )}
                  </div>
                  <div className="flex flex-wrap justify-end gap-2">
                    <Button variant="outline" onClick={handleUploadWebDavBackup} disabled={isBackupBusy}>
                      {isUploadingWebDav ? <Loader2 className="size-4 animate-spin" /> : <Upload className="size-4" />}
                      <span>{t("settings.webdavUpload")}</span>
                    </Button>
                    <Button variant="outline" onClick={handleRefreshWebDavBackups} disabled={isBackupBusy}>
                      {isRefreshingWebDav ? <Loader2 className="size-4 animate-spin" /> : <RefreshCw className="size-4" />}
                      <span>{t("settings.webdavRefresh")}</span>
                    </Button>
                    <Button variant="outline" onClick={handleDeleteSelectedWebDavBackup} disabled={isBackupBusy || !selectedWebDavPath}>
                      {isDeletingWebDav ? <Loader2 className="size-4 animate-spin" /> : <Trash2 className="size-4" />}
                      <span>{t("settings.webdavDeleteSelected")}</span>
                    </Button>
                    <Button variant="outline" onClick={handleImportSelectedWebDavBackup} disabled={isBackupBusy || !selectedWebDavPath}>
                      {isImportingWebDav ? <Loader2 className="size-4 animate-spin" /> : <Download className="size-4" />}
                      <span>{t("settings.webdavImportSelected")}</span>
                    </Button>
                  </div>
                </div>
              </div>
            </div>
          </CardContent>
                  </details>
        </Card>

        {/* ── Section 2: GitHub Import Auth ─────────────────────────────── */}
        <Card>
          <details className="group/settings-section">
          <summary className="cursor-pointer list-none [&::-webkit-details-marker]:hidden">
          <CardHeader>
            <div className="flex items-center gap-2">
              <KeyRound className="size-4 text-muted-foreground" />
              <div>
                <div className="flex items-center gap-2">
                  <CardTitle className="font-semibold flex items-center gap-2">{t("settings.githubPatTitle")}<ChevronRight className="size-4 shrink-0 group-open/settings-section:rotate-90" /></CardTitle>
                  <HintIcon
                    text={`${t("settings.githubPatDesc")}\n\n${t("settings.githubPatDirectOnly")}\n\n${t("settings.githubPatRateLimitHint")}`}
                  />
                </div>
              </div>
            </div>
          </CardHeader>
          </summary>
          <CardContent className="pt-4">
            <div className="space-y-4">
              <div>
                <label htmlFor="github-pat" className="mb-1 block text-xs font-medium text-muted-foreground">
                  {t("settings.githubPatLabel")}
                </label>
                <Input
                  id="github-pat"
                  type="password"
                  placeholder="github_pat_..."
                  value={githubPatInput}
                  onChange={(event) => setGitHubPatInput(event.target.value)}
                  disabled={isLoadingGitHubPat || isSavingGitHubPat}
                />
              </div>

              {githubPatMessage ? (
                <p
                  className={githubPatMessage.type === "error" ? "text-sm text-destructive" : "text-sm text-emerald-600 dark:text-emerald-400"}
                  role="status"
                >
                  {githubPatMessage.text}
                </p>
              ) : null}

              <div className="flex flex-wrap items-center justify-end gap-2">
                <Button
                  variant="outline"
                  onClick={handleSaveGitHubPat}
                  disabled={isLoadingGitHubPat || isSavingGitHubPat || !isGitHubPatDirty}
                >
                  {isSavingGitHubPat ? <Loader2 className="size-4 animate-spin" /> : <Save className="size-4" />}
                  <span>{t("common.save")}</span>
                </Button>
                <Button
                  variant="outline"
                  onClick={handleClearGitHubPat}
                  disabled={isLoadingGitHubPat || isSavingGitHubPat || !githubPat}
                >
                  <Trash2 className="size-4" />
                  <span>{t("settings.githubPatClear")}</span>
                </Button>
                {isLoadingGitHubPat ? (
                  <span className="text-xs text-muted-foreground">{t("settings.loading")}</span>
                ) : null}
              </div>
            </div>
          </CardContent>
          </details>
        </Card>

        {/* ── Section 3: AI Provider ─────────────────────────────────────── */}
        <Card>
          <details className="group/settings-section">
          <summary className="cursor-pointer list-none [&::-webkit-details-marker]:hidden">
          <CardHeader className="flex flex-row items-center justify-between">
            <div className="flex items-center gap-2">
              <Bot className="size-4 text-muted-foreground" />
              <div>
                <div className="flex items-center gap-2">
                  <CardTitle className="font-semibold flex items-center gap-2">{t("settings.aiServiceTitle")}<ChevronRight className="size-4 shrink-0 group-open/settings-section:rotate-90" /></CardTitle>
                  <HintIcon
                    text={t("settings.aiServiceDescription")}
                  />
                </div>
              </div>
            </div>
          </CardHeader>
          </summary>
          <CardContent className="space-y-4 pt-3">
            <section aria-labelledby="ai-connection-heading" className="rounded-lg border border-border p-4">
              <h3 id="ai-connection-heading" className="mb-3 flex items-center gap-2 text-sm font-normal"><Plug className="size-[1em] shrink-0" aria-hidden="true"/>{t("settings.aiConnectionTitle")}</h3>
            <div className="space-y-4">
              <div>
                <label className="text-xs font-medium text-muted-foreground mb-2 block">{lang === "zh" ? "提供商" : "Provider"}</label>
                <div className="flex flex-wrap gap-2">
                  {AI_PROVIDERS.map((p) => (
                    <button
                      key={p.id}
                      onClick={() => handleProviderChange(p.id)}
                      className={cn(
                        "rounded-md border px-3 py-1.5 text-xs transition-colors cursor-pointer",
                        aiProvider === p.id
                          ? "border-transparent bg-hover-bg text-white font-medium shadow-sm dark:bg-hover-bg dark:text-white"
                          : "border-border bg-background text-muted-foreground hover:border-primary/40 hover:bg-primary/10 hover:text-primary"
                      )}
                    >
                      {lang === "zh" ? p.name.zh : p.name.en}
                    </button>
                  ))}
                </div>
              </div>
              {currentProvider && currentProvider.regions.length > 1 && (
                <div>
                  <label className="text-xs font-medium text-muted-foreground mb-2 block">{lang === "zh" ? "区域" : "Region"}</label>
                  <div className="flex gap-2">
                    {currentProvider.regions.map((r) => (
                      <button
                        key={r}
                        onClick={() => setAiRegion(r)}
                        className={cn(
                          "rounded-md border px-3 py-1.5 text-xs transition-colors cursor-pointer",
                          aiRegion === r
                            ? "border-transparent bg-hover-bg text-white font-medium shadow-sm dark:bg-hover-bg dark:text-white"
                            : "border-border bg-background text-muted-foreground hover:border-primary/40 hover:bg-primary/10 hover:text-primary"
                        )}
                      >
                        {lang === "zh" ? REGION_LABELS[r].zh : REGION_LABELS[r].en}
                      </button>
                    ))}
                  </div>
                </div>
              )}
              <div className="grid grid-cols-1 gap-4 sm:grid-cols-2">
              <div>
                <label className="text-xs font-medium text-muted-foreground mb-1 block">API Key</label>
                <Input type="password" placeholder="sk-..." value={aiApiKey} onChange={(e) => setAiApiKey(e.target.value)} />
              </div>
              <div>
                <label className="text-xs font-medium text-muted-foreground mb-1 block">{lang === "zh" ? "模型" : "Model"}</label>
                <Input placeholder={lang === "zh" ? "模型名称" : "Model name"} value={aiModel} onChange={(e) => setAiModel(e.target.value)} />
              </div>
              </div>
              {aiProvider === "custom" && (
                <div>
                  <label className="text-xs font-medium text-muted-foreground mb-1 block">API URL</label>
                  <Input placeholder="https://..." value={aiCustomUrl} onChange={(e) => setAiCustomUrl(e.target.value)} />
                </div>
              )}
              <div className="flex flex-col gap-2 sm:flex-row sm:items-center sm:justify-end">

                {resolvedUrl && (
                  <div className="text-xs text-muted-foreground bg-muted/30 rounded-md px-3 py-2 font-mono truncate flex-1 min-w-0">{resolvedUrl}</div>
                )}
                <Button variant="outline" size="sm" disabled={!aiLoaded || aiSaving || aiTesting || !aiApiKey || !resolvedUrl} onClick={async () => {
                  if (!await handleSaveAiConfig()) return;
                  setAiTesting(true); setAiTestResult(null); setShowAiTestDetails(false);
                  try {
                    const result = await invoke<string>("explain_skill", { content: "Test connection. Reply with: OK" });
                    setAiTestResult({ ok: true, msg: result.slice(0, 60) });
                  } catch (err) {
                    const raw = String(err);
                    // Try to extract structured error from JSON-like error strings
                    let msg = raw;
                    let details: string | undefined;
                    const prefix = "API 请求失败: ";
                    if (raw.startsWith(prefix)) {
                      const after = raw.slice(prefix.length);
                      const nlIdx = after.indexOf("\n");
                      if (nlIdx > 0) {
                        msg = after.slice(nlIdx + 1);
                        details = after.slice(0, nlIdx);
                      } else {
                        msg = after;
                      }
                    }
                    setAiTestResult({ ok: false, msg, details });
                  }
                  finally { setAiTesting(false); }
                }} className="shrink-0">
                  {aiTesting ? <Loader2 className="size-4 animate-spin" /> : <Bot className="size-4" />}
                  <span>{lang === "zh" ? "测试连接" : "Test"}</span>
                </Button>
                <Button variant="outline" size="sm" disabled={!aiLoaded || aiSaving || aiTesting} onClick={() => void handleSaveAiConfig()}><Save className="size-4" />{t("settings.aiSaveConfig")}</Button>
              </div>
              {aiTestResult && (
                <div className={`text-xs rounded-md px-3 py-2 space-y-2 ${aiTestResult.ok ? "bg-green-500/10 text-green-700 dark:text-green-400" : "bg-destructive/10 text-destructive"}`}>
                  <p>{aiTestResult.ok ? "✓ " : "✕ "}{aiTestResult.msg}</p>
                  {!aiTestResult.ok && aiTestResult.details && (
                    <div>
                      <button
                        className="flex items-center gap-1 text-muted-foreground hover:text-foreground transition-colors cursor-pointer"
                        onClick={() => setShowAiTestDetails((v) => !v)}
                      >
                        {showAiTestDetails ? <ChevronDown className="size-4" /> : <ChevronRight className="size-4" />}
                        {lang === "zh" ? "查看详情" : "Details"}
                      </button>
                      {showAiTestDetails && (
                        <pre className="mt-1 text-xs leading-4 font-mono text-muted-foreground whitespace-pre-wrap break-all bg-muted/30 rounded-md p-2 max-h-32 overflow-auto">
                          {aiTestResult.details}
                        </pre>
                      )}
                    </div>
                  )}
                  {!aiTestResult.ok && currentProvider && currentProvider.regions.length > 1 && (
                    <p className="text-muted-foreground">
                      {lang === "zh"
                        ? `提示：可在上方切换区域端点后重试（当前：${REGION_LABELS[aiRegion]?.zh ?? aiRegion}）`
                        : `Tip: Try switching the region endpoint above (current: ${REGION_LABELS[aiRegion]?.en ?? aiRegion})`}
                    </p>
                  )}
                </div>
              )}
            </div>
            </section>
            <details aria-labelledby="ai-prompts-heading" className="group/ai-prompts rounded-lg border border-border p-4">
              <summary id="ai-prompts-heading" className="flex cursor-pointer list-none items-center gap-2 text-sm font-normal [&::-webkit-details-marker]:hidden">
                <MessageSquareText className="size-4 shrink-0 text-muted-foreground" />
                <span className="flex items-center gap-2">{t("settings.aiPromptsTitle")}<ChevronRight className="size-4 shrink-0 group-open/ai-prompts:rotate-90" /></span>
                <HintIcon text={t("settings.aiPromptsHelp")} />
              </summary>
              <div className="mt-4"><AiPromptSettings /></div>
            </details>
          </CardContent>
                  </details>
        </Card>

        {/* ── Section 5: About ────────────────────────────────────────────── */}
        <Card>
          <details className="group/settings-section">
          <summary className="cursor-pointer list-none [&::-webkit-details-marker]:hidden">
          <CardHeader>
            <div className="flex items-center gap-2">
              <Info className="size-[1em] shrink-0 text-muted-foreground" />
              <CardTitle role="heading" aria-level={2} className="flex items-center gap-2">
                {t("settings.about")}
                <ChevronRight className="size-[1em] shrink-0 group-open/settings-section:rotate-90" />
              </CardTitle>
            </div>
          </CardHeader>
          </summary>
          <CardContent className="pt-4">
            <div className="space-y-3">
              <div className="flex items-center gap-3">
                <Info className="size-4 text-muted-foreground shrink-0" />
                <div>
                  <div className="text-xs text-muted-foreground">{t("settings.appVersion")}</div>
                  <div className="text-sm font-semibold">{t("app.name")} v{APP_VERSION}</div>
                </div>
              </div>
              <div className="flex items-center gap-3">
                <RefreshCw className="size-4 text-muted-foreground shrink-0" />
                <div className="min-w-0 flex-1">
                  <div className="text-xs text-muted-foreground">{t("settings.checkUpdates")}</div>
                  <div
                    className={cn(
                      "mt-0.5 truncate text-sm",
                      updateMessage?.type === "error" ? "text-destructive" : "text-muted-foreground"
                    )}
                    role={updateMessage ? "status" : undefined}
                  >
                    {updateMessage?.text ?? (
                      updateInfo
                        ? t("settings.latestVersion", { version: updateInfo.latestVersion })
                        : t("settings.updateNotChecked")
                    )}
                  </div>
                </div>
                <div className="flex shrink-0 flex-wrap justify-end gap-2">
                  <Button
                    variant="outline"
                    size="sm"
                    onClick={handleCheckAppUpdate}
                    disabled={isCheckingUpdate}
                  >
                    {isCheckingUpdate ? (
                      <Loader2 className="size-4 animate-spin" />
                    ) : (
                      <RefreshCw className="size-4" />
                    )}
                    <span>
                      {isCheckingUpdate ? t("settings.checkingUpdates") : t("settings.checkUpdates")}
                    </span>
                  </Button>
                  {updateInfo?.isUpdateAvailable && updateInfo.latestUrl ? (
                    <a
                      href={updateInfo.latestUrl}
                      target="_blank"
                      rel="noreferrer"
                      className="inline-flex h-8 items-center gap-2 rounded-md border border-border bg-background px-3 text-sm transition-colors hover:bg-muted"
                    >
                      <ExternalLink className="size-4" />
                      <span>{t("settings.openLatestRelease")}</span>
                    </a>
                  ) : null}
                </div>
              </div>
              {/* ── Language Switcher ──────────────────────────────────────── */}
              <div className="flex items-center gap-3">
                <Globe className="size-4 text-muted-foreground shrink-0" />
                <div className="min-w-0 flex-1">
                  <div className="text-xs text-muted-foreground">{t("settings.language")}</div>
                  <div className="text-sm font-medium">
                    {i18n.language === "zh" ? t("settings.chinese") : t("settings.english")}
                  </div>
                </div>
                <div className="flex shrink-0 justify-end gap-2">
                    <Button
                      variant="outline"
                      size="sm"
                      onClick={() => useSettingsStore.getState().saveLanguage("zh")}
                      aria-pressed={i18n.language === "zh"}
                      className={cn(i18n.language === "zh" && "border-transparent bg-hover-bg text-white shadow-sm hover:bg-hover-bg hover:text-white dark:bg-hover-bg dark:text-white dark:hover:bg-hover-bg dark:hover:text-white")}
                    ><ActionIcon action="language"/>
                      {t("settings.chinese")}
                    </Button>
                    <Button
                      variant="outline"
                      size="sm"
                      onClick={() => useSettingsStore.getState().saveLanguage("en")}
                      aria-pressed={i18n.language === "en"}
                      className={cn(i18n.language === "en" && "border-transparent bg-hover-bg text-white shadow-sm hover:bg-hover-bg hover:text-white dark:bg-hover-bg dark:text-white dark:hover:bg-hover-bg dark:hover:text-white")}
                    ><ActionIcon action="language"/>
                      {t("settings.english")}
                    </Button>
                </div>
              </div>
            </div>
          </CardContent>
          </details>
        </Card>
      </div>

      {/* ── Dialogs ────────────────────────────────────────────────────────── */}
      <PlatformDialog
        open={isPlatformDialogOpen}
        onOpenChange={(open) => {
          setIsPlatformDialogOpen(open);
        }}
        platform={editingPlatform}
        onAdd={handleAddPlatform}
        onEdit={handleEditPlatform}
      />
    </div>
  );
}
