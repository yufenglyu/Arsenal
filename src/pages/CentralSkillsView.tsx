import { ActionIcon } from "@/components/ui/action-icon";
import { SearchScopes } from "@/components/skill/SearchScopes";
import { useSearchScopes, matchesSearch } from "@/lib/skillFilters";
import { TagFilters } from "@/components/skill/TagFilters";
import { matchesTags } from "@/lib/skillFilters";
import { SkillBrowserHeader } from "@/components/skill/SkillBrowserHeader";
import { SidebarTagFilter } from "@/components/layout/SidebarTagFilter";
import { SkillBrowserWorkspace } from "@/components/skill/SkillBrowserWorkspace";
import { repositoryLocationUrl } from "@/lib/skillNavigation";
import {
AlertTriangle,
RotateCw
} from "lucide-react";
import { useDeferredValue,useEffect,useMemo,useRef,useState } from "react";
import { useTranslation } from "react-i18next";
import { useNavigate } from "react-router-dom";
import { toast } from "sonner";

import { InstallDialog } from "@/components/central/InstallDialog";
import { PlatformInstallDrawer } from "@/components/central/PlatformInstallDrawer";
import { OpenableDirectoryPath } from "@/components/common/OpenableDirectoryPath";
import { type FolderTableItem } from "@/components/skill/SkillBrowserTable";
import { SkillDetailDrawer } from "@/components/skill/SkillDetailDrawer";
import { Button } from "@/components/ui/button";
import {
Dialog,
DialogContent,
DialogDescription,
DialogFooter,
DialogHeader,
DialogTitle,
} from "@/components/ui/dialog";
import { SearchInput } from "@/components/ui/search-input";
import { formatPathForDisplay } from "@/lib/path";
import { normalizeSearchQuery } from "@/lib/search";
import {
splitResourceLibrarySkillsByFolder,
type SkillFolderGroup,
} from "@/lib/skillFolders";
import {
sortBySkillBrowserOrder,
sortFoldersBySkillBrowserOrder,
type SkillSortDirection,
type SkillSortField,
} from "@/lib/skillSort";
import { isTauriRuntime } from "@/lib/tauri";
import { useCentralSkillsStore } from "@/stores/centralSkillsStore";
import { usePlatformStore } from "@/stores/platformStore";
import { AgentWithStatus,CentralSkillBundle,SkillWithLinks } from "@/types";

const BROWSER_FIXTURE_AGENTS: AgentWithStatus[] = [
  {
    id: "claude-code",
    display_name: "Claude Code",
    global_skills_dir: "/Users/browser/.claude/skills/",
    is_detected: true,
    is_builtin: true,
    is_enabled: true,
  },
  {
    id: "cursor",
    display_name: "Cursor",
    global_skills_dir: "/Users/browser/.cursor/skills/",
    is_detected: true,
    is_builtin: true,
    is_enabled: true,
  },
  {
    id: "central",
    display_name: "Shared Hub",
    global_skills_dir: "/Users/browser/.agents/skills/",
    is_detected: true,
    is_builtin: true,
    is_enabled: true,
  },
];

const BROWSER_FIXTURE_SKILLS: SkillWithLinks[] = [
  {
    id: "fixture-central-skill",
    name: "fixture-central-skill",
    description: "Browser validation fixture for Central and drawer entry flows.",
    file_path: "~/.agents/skills/fixture-central-skill/SKILL.md",
    canonical_path: "~/.agents/skills/fixture-central-skill",
    is_central: true,
    source: "browser-fixture",
    scanned_at: "2026-04-17T00:00:00.000Z",
    created_at: "2026-04-17T00:00:00.000Z",
    updated_at: "2026-04-17T00:00:00.000Z",
    linked_agents: ["claude-code"],
    read_only_agents: [],
  },
];

const EMPTY_SKILLS: SkillWithLinks[] = [];
const EMPTY_BUNDLES: CentralSkillBundle[] = [];
const EMPTY_AGENTS: AgentWithStatus[] = [];
const noopLoadCentralSkills = async () => {};
const noopLoadCentralBundles = async () => {};
const noopRefreshCounts = async () => {};
const noopTogglePlatformLink = async (_skillId: string, _agentId: string) => {};
const noopDeleteCentralSkill = async (
  _skillId: string,
  _options: { cascadeUninstall: boolean }
) => ({
  skillId: _skillId,
  removedCanonicalPath: "",
  uninstalledAgents: [],
  skippedReadOnlyAgents: [],
});
const noopPreviewDeleteCentralBundle = async (relativePath: string) => ({
  bundle: {
    name: relativePath,
    relativePath,
    path: "",
    isSymlink: false,
    skillCount: 0,
    linkedAgentCount: 0,
    readOnlyAgentCount: 0,
  },
  skills: [],
  affectedAgents: [],
  skippedReadOnlyAgents: [],
});
const noopDeleteCentralBundle = async (relativePath: string) => ({
  relativePath,
  removedBundlePath: "",
  removedKind: "directory",
  removedSkillIds: [],
  uninstalledAgents: [],
  skippedReadOnlyAgents: [],
});
const noopClearBundleDeletePreview = () => {};
const noopInstallSkill = async () => ({
  succeeded: [],
  failed: [],
});

function latestSkillUpdatedAt(skills: SkillWithLinks[]) {
  return skills.reduce<string | null>((latest, skill) => {
    const value = skill.updated_at ?? skill.scanned_at ?? null;
    if (!value) return latest;
    if (!latest) return value;
    return Date.parse(value) > Date.parse(latest) ? value : latest;
  }, null);
}

function earliestSkillCreatedAt(skills: SkillWithLinks[]) {
  return skills.reduce<string | null>((earliest, skill) => {
    const value = skill.created_at ?? skill.scanned_at ?? null;
    if (!value) return earliest;
    if (!earliest) return value;
    return Date.parse(value) < Date.parse(earliest) ? value : earliest;
  }, null);
}

// ─── Empty State ──────────────────────────────────────────────────────────────



// ─── CentralSkillsView ────────────────────────────────────────────────────────

export function CentralSkillsView() {
  const navigate = useNavigate();
  const { t } = useTranslation();
  const rawSkills = useCentralSkillsStore((state) => state.skills);
  const rawBundles = useCentralSkillsStore((state) => state.bundles);
  const rawAgents = useCentralSkillsStore((state) => state.agents);
  const rawIsLoading = useCentralSkillsStore((state) => state.isLoading);
  const rawLoadCentralSkills = useCentralSkillsStore(
    (state) => state.loadCentralSkills
  );
  const shouldUseBrowserFixtures =
    !isTauriRuntime() &&
    rawSkills === undefined &&
    rawAgents === undefined &&
    rawLoadCentralSkills === undefined;
  const skills = shouldUseBrowserFixtures
    ? BROWSER_FIXTURE_SKILLS
    : (rawSkills ?? EMPTY_SKILLS);
  const bundles = rawBundles ?? EMPTY_BUNDLES;
  const agents = shouldUseBrowserFixtures
    ? BROWSER_FIXTURE_AGENTS
    : (rawAgents ?? EMPTY_AGENTS);
  const centralAgentDir =
    agents.find((agent) => agent.id === "central")?.global_skills_dir ?? "";
  const centralSkillsRoot = centralAgentDir || t("central.path");
  const isLoading = shouldUseBrowserFixtures ? false : rawIsLoading ?? false;
  const loadCentralSkills = rawLoadCentralSkills ?? noopLoadCentralSkills;
  const loadCentralBundles =
    useCentralSkillsStore((state) => state.loadCentralBundles) ??
    noopLoadCentralBundles;
  const installSkill =
    useCentralSkillsStore((state) => state.installSkill) ?? noopInstallSkill;
  const togglePlatformLink =
    useCentralSkillsStore((state) => state.togglePlatformLink) ??
    noopTogglePlatformLink;
  const deleteCentralSkill =
    useCentralSkillsStore((state) => state.deleteCentralSkill) ??
    noopDeleteCentralSkill;
  const previewDeleteCentralBundle =
    useCentralSkillsStore((state) => state.previewDeleteCentralBundle) ??
    noopPreviewDeleteCentralBundle;
  const deleteCentralBundle =
    useCentralSkillsStore((state) => state.deleteCentralBundle) ??
    noopDeleteCentralBundle;
  const clearBundleDeletePreview =
    useCentralSkillsStore((state) => state.clearBundleDeletePreview) ??
    noopClearBundleDeletePreview;
  const bundleDeletePreview = useCentralSkillsStore(
    (state) => state.bundleDeletePreview
  );
  const togglingAgentId = useCentralSkillsStore((state) => state.togglingAgentId);
  const deletingSkillId = useCentralSkillsStore((state) => state.deletingSkillId);
  const deletingBundlePath = useCentralSkillsStore((state) => state.deletingBundlePath);

  // Keep the platform sidebar counts in sync after install.
  const refreshCounts =
    usePlatformStore((state) => state.refreshCounts) ?? noopRefreshCounts;




  const [sortField, setSortField] = useState<SkillSortField>("name");
  const [sortDirection, setSortDirection] = useState<SkillSortDirection>("asc");
  const [searchQuery, setSearchQuery] = useState("");
  const [searchScopes, setSearchScopes] = useSearchScopes();
  const [selectedTags, setSelectedTags] = useState<string[]>([]);
  const [installTargetSkill, setInstallTargetSkill] =
    useState<SkillWithLinks | null>(null);
  const [deleteTargetSkill, setDeleteTargetSkill] =
    useState<SkillWithLinks | null>(null);
  const [deleteTargetBundle, setDeleteTargetBundle] =
    useState<CentralSkillBundle | null>(null);
  const [deletingFolderGroupPath, setDeletingFolderGroupPath] =
    useState<string | null>(null);
  const [activeFolderKey, setActiveFolderKey] = useState<string | null>(null);
  const [isDialogOpen, setIsDialogOpen] = useState(false);
  const [drawerSkillId, setDrawerSkillId] = useState<string | null>(null);
  const [isDrawerOpen, setIsDrawerOpen] = useState(false);
  const [platformDrawerSkillId, setPlatformDrawerSkillId] = useState<string | null>(null);
  const [isPlatformDrawerOpen, setIsPlatformDrawerOpen] = useState(false);
  const contentRef = useRef<HTMLDivElement | null>(null);
  const detailButtonRefs = useRef<Record<string, HTMLButtonElement | null>>({});
  const deferredSearchQuery = useDeferredValue(searchQuery);
  const effectiveSearchQuery =
    skills.length > 80 ? deferredSearchQuery : searchQuery;
  const normalizedSearchQuery = useMemo(
    () => normalizeSearchQuery(effectiveSearchQuery),
    [effectiveSearchQuery]
  );
  const centralFolderSplit = useMemo(
    () => splitResourceLibrarySkillsByFolder(skills, centralAgentDir),
    [centralAgentDir, skills]
  );
  const centralFolderGroupsByPath = useMemo(
    () =>
      new Map(
        [
          ...centralFolderSplit.groups,
          ...bundles
            .filter(
              (bundle) =>
                !centralFolderSplit.groups.some(
                  (group) =>
                    group.relativePath === bundle.relativePath ||
                    group.path === bundle.path
                )
            )
            .map((bundle) => ({
              name: bundle.name,
              relativePath: bundle.relativePath,
              path: bundle.path,
              skillCount: bundle.skillCount,
              linkedAgentIds: [],
              readOnlyAgentIds: [],
              linkedAgentCount: bundle.linkedAgentCount,
              readOnlyAgentCount: bundle.readOnlyAgentCount,
              skills: [],
            })),
        ].map((group) => [group.relativePath, group])
      ),
    [bundles, centralFolderSplit.groups]
  );



  const availableTags = useMemo(() => {
    const tags = new Map<string, string>();
    for (const skill of skills) {
      for (const tag of skill.tags ?? []) {
        const normalized = tag.toLowerCase();
        if (!tags.has(normalized)) {
          tags.set(normalized, tag);
        }
      }
    }
    return Array.from(tags.entries())
      .map(([key, label]) => ({ key, label }))
      .sort((a, b) => a.label.localeCompare(b.label, undefined, { sensitivity: "base" }));
  }, [skills]);
  const isSearchActive = normalizedSearchQuery.length > 0;

  // Load central skills on mount.
  useEffect(() => {
    loadCentralSkills();
  }, [loadCentralSkills]);

  useEffect(() => {
    loadCentralBundles();
  }, [loadCentralBundles]);

  // Filter skills by search query.
  const filteredSkills = useMemo(() => skills.filter(skill => {
    const group = [...centralFolderGroupsByPath.values()].find(group=>group.skills.some(item=>item.id===skill.id));
    return matchesTags(skill.tags,selectedTags) && matchesSearch(skill,normalizedSearchQuery,searchScopes,group?.name);
  }),[skills,centralFolderGroupsByPath,selectedTags,normalizedSearchQuery,searchScopes]);
  const filteredFolderGroups = useMemo(() => [...centralFolderGroupsByPath.values()].filter(group=>(!normalizedSearchQuery&&!selectedTags.length)||group.skills.some(skill=>filteredSkills.some(match=>match.id===skill.id))),[centralFolderGroupsByPath,filteredSkills,normalizedSearchQuery,selectedTags.length]);

  const sortedSkills = useMemo(() => {
    return sortBySkillBrowserOrder(filteredSkills, sortField, sortDirection);
  }, [filteredSkills, sortDirection, sortField]);

  const sortedFolderGroups = useMemo(
    () => sortFoldersBySkillBrowserOrder(filteredFolderGroups, sortField, sortDirection),
    [filteredFolderGroups, sortDirection, sortField]
  );



  useEffect(() => {
    if (!isSearchActive || !contentRef.current) return;
    contentRef.current.scrollTop = 0;
  }, [isSearchActive, normalizedSearchQuery]);

  function agentDisplayNames(agentIds: string[]): string[] {
    const namesById = new Map(agents.map((agent) => [agent.id, agent.display_name]));
    return Array.from(new Set(agentIds)).map((agentId) => namesById.get(agentId) ?? agentId);
  }

  function linkedAgentNames(skill: SkillWithLinks): string[] {
    return agentDisplayNames([...skill.linked_agents, ...(skill.read_only_agents ?? [])]);
  }

  function findBundleForFolderGroup(
    group: SkillFolderGroup<SkillWithLinks>
  ): CentralSkillBundle | null {
    return (
      bundles.find((bundle) => bundle.relativePath === group.relativePath) ??
      bundles.find((bundle) => bundle.path === group.path) ??
      null
    );
  }

  function setDetailButtonRef(skillId: string, node: HTMLButtonElement | null) {
    detailButtonRefs.current[skillId] = node;
  }

  function handleOpenDrawer(skillId: string) {
    setDrawerSkillId(skillId);
    setIsDrawerOpen(true);
  }

  function handleOpenPlatformDrawer(skillId: string) {
    setPlatformDrawerSkillId(skillId);
    setIsPlatformDrawerOpen(true);
  }

  async function handleTogglePlatform(skillId: string, agentId: string) {
    try {
      await togglePlatformLink(skillId, agentId);
      await refreshCounts();
    } catch (err) {
      toast.error(t("central.installError", { error: String(err) }));
    }
  }

  async function handleInstall(skillId: string, agentIds: string[], method: string) {
    try {
      const result = await installSkill(skillId, agentIds, method);
      // Refresh sidebar counts after install.
      await refreshCounts();
      if (result.failed.length > 0) {
        const failedNames = result.failed.map((f) => f.agent_id).join(", ");
        toast.error(t("central.installPartialFail", { platforms: failedNames }));
      }
    } catch (err) {
      toast.error(t("central.installError", { error: String(err) }));
    }
  }

  async function handleDeleteCentralSkill(skill: SkillWithLinks, cascadeUninstall: boolean) {
    try {
      await deleteCentralSkill(skill.id, { cascadeUninstall });
      await refreshCounts();
      toast.success(t("central.deleteSuccess", { name: skill.name }));
      setDeleteTargetSkill(null);
    } catch (err) {
      toast.error(t("central.deleteError", { error: String(err) }));
    }
  }

  function handleDeleteClick(skill: SkillWithLinks) {
    if (skill.linked_agents.length > 0 || (skill.read_only_agents?.length ?? 0) > 0) {
      setDeleteTargetSkill(skill);
      return;
    }

    void handleDeleteCentralSkill(skill, false);
  }

  async function handleDeleteBundleClick(bundle: CentralSkillBundle) {
    try {
      await previewDeleteCentralBundle(bundle.relativePath);
      setDeleteTargetBundle(bundle);
    } catch (err) {
      toast.error(t("central.deleteBundlePreviewError", { error: String(err) }));
    }
  }

  function handleDeleteFolderGroupClick(group: SkillFolderGroup<SkillWithLinks>) {
    const bundle = findBundleForFolderGroup(group);
    if (bundle) {
      void handleDeleteBundleClick(bundle);
      return;
    }
    void handleDeleteCentralFolderGroup(group);
  }

  async function handleDeleteCentralBundle(bundle: CentralSkillBundle) {
    try {
      await deleteCentralBundle(bundle.relativePath, { cascadeUninstall: true });
      await refreshCounts();
      toast.success(t("central.deleteBundleSuccess", { name: bundle.name }));
      setDeleteTargetBundle(null);
      clearBundleDeletePreview();
      if (activeFolderKey === bundle.relativePath) {
        setActiveFolderKey(null);
      }
    } catch (err) {
      toast.error(t("central.deleteBundleError", { error: String(err) }));
    }
  }

  async function handleDeleteCentralFolderGroup(
    group: SkillFolderGroup<SkillWithLinks>
  ) {
    if (group.skills.length === 0) {
      return;
    }

    setDeletingFolderGroupPath(group.relativePath);
    try {
      for (const skill of group.skills) {
        await deleteCentralSkill(skill.id, { cascadeUninstall: true });
      }
      await refreshCounts();
      await Promise.all([loadCentralSkills(), loadCentralBundles()]);
      toast.success(t("central.deleteFolderGroupSuccess", { name: group.name }));
      if (activeFolderKey === group.relativePath) {
        setActiveFolderKey(null);
      }
    } catch (err) {
      toast.error(t("central.deleteError", { error: String(err) }));
    } finally {
      setDeletingFolderGroupPath(null);
    }
  }

  async function handleRefresh() {
    try {
      // Re-scan the filesystem first so new/removed skills are picked up,
      // then reload central skills from the (now-updated) database.
      await refreshCounts();
      await Promise.all([loadCentralSkills(), loadCentralBundles()]);
    } catch (err) {
      toast.error(t("central.refreshError", { error: String(err) }));
    }
  }

  const platformDrawerSkill = useMemo(
    () => skills.find((skill) => skill.id === platformDrawerSkillId) ?? null,
    [platformDrawerSkillId, skills]
  );

  return (
    <div className="flex flex-col h-full">
      {/* Header */}


      {/* Search bar */}


      {/* Content */}
      <SidebarTagFilter hasSelection={selectedTags.length > 0} onClear={() => setSelectedTags([])}><TagFilters tags={availableTags} selected={selectedTags} onChange={setSelectedTags}/></SidebarTagFilter>
      <SkillBrowserWorkspace onRefresh={handleRefresh} toolbar={viewControl => <SkillBrowserHeader viewControl={viewControl} title={<div>
          <div className="flex items-center gap-2">
            <h1 className="text-sm font-semibold">{t("central.title")}</h1>
            <Button
              variant="ghost"
              size="icon"
              onClick={handleRefresh}
              disabled={isLoading}
              aria-label={t("central.refresh")}
            >
              <RotateCw className={`size-4 ${isLoading ? "animate-spin" : ""}`} />
            </Button>
          </div>
          <OpenableDirectoryPath iconOnly
            path={centralAgentDir}
            displayPath={centralSkillsRoot}
          />
        </div>} search={<div className="flex items-center gap-2"><SearchInput
            placeholder={t("central.searchPlaceholder")}
            value={searchQuery}
            onValueChange={setSearchQuery}
            aria-label={t("central.searchPlaceholder")}
            containerClassName="min-w-0 flex-1"
          trailing={<SearchScopes value={searchScopes} onChange={setSearchScopes}/>} /></div>} />} loading={isLoading} folders={sortedFolderGroups.map((group): FolderTableItem => {
                    const groupSkills = group.skills;
                    return {
                      key: group.relativePath,
                      onLocate: () => navigate(repositoryLocationUrl(group.skills[0], true)),
                      name: group.name,
                      sourceRepo: group.skills.find(skill => skill.source_repo?.toLowerCase() === group.name.toLowerCase())?.source_repo,
                      path: group.path,
                      skillCount: group.skillCount,
 skillKeys: groupSkills.map(skill => skill.id),
                      tags: [...new Set(group.skills.flatMap(skill => skill.tags ?? []))],
                      installAgents: agents,
                      installSummaryMembers: groupSkills,
                      installLinkedAgentIds: group.linkedAgentIds,
                      installReadOnlyAgentIds: group.readOnlyAgentIds,
                      previewNames: groupSkills.map((skill) => skill.name),
                      createdAt: earliestSkillCreatedAt(groupSkills),
                      updatedAt: latestSkillUpdatedAt(groupSkills),
                      onOpen: () => setActiveFolderKey(group.relativePath),
                      centralUninstall: true,
                      onDelete: () => handleDeleteFolderGroupClick(group),
                      deleteLabel: t("central.uninstallAction"),
                      deleteRequiresConfirmation: !findBundleForFolderGroup(group),
                      isDeleting:
                        deletingBundlePath === group.relativePath ||
                        deletingFolderGroupPath === group.relativePath,
                    };
                  })} storageKey="CentralSkillsView"  searchActive={Boolean(normalizedSearchQuery || selectedTags.length)}
                  sortField={sortField}
                  sortDirection={sortDirection}
                  onSortChange={(field, direction) => {
                    setSortField(field);
                    setSortDirection(direction);
                  }}
                  skills={sortedSkills.map((skill) => ({
                    rowKey: skill.id,
                    batchOperations: {
                      install: async (targets: string[]) => { const result=await installSkill(skill.id,targets,"auto"); await refreshCounts(); return result; },
                      uninstall: async () => { await deleteCentralSkill(skill.id,{cascadeUninstall:true}); await refreshCounts(); },
                    },
 detailRequest: { skillId: skill.id },
                    onLocate: () => navigate(repositoryLocationUrl(skill, false)),
                    name: skill.name,
                    description: skill.description,
                    notes: skill.notes,
                    publisher: skill.source_repo ?? skill.source_author ?? undefined,
                    sourceAuthor: skill.source_author,
                    sourceRepo: skill.source_repo,
                    sourceUrl: skill.source_url,
                    createdAt: skill.created_at,
                    updatedAt: skill.updated_at,
                    tags: (skill.tags ?? []).map((tag) => ({ key: tag, label: tag })),
                    onDetail: () => handleOpenDrawer(skill.id),
                    centralUninstall: true,
                    onDeleteFromCentral: () => handleDeleteClick(skill),
                    deleteFromCentralLabel: t("central.uninstallAction"),
                    deleteFromCentralRequiresDialog:
                      skill.linked_agents.length > 0 || (skill.read_only_agents?.length ?? 0) > 0,
                    isLoading: deletingSkillId === skill.id,
                    detailButtonRef: (node) => setDetailButtonRef(skill.id, node),
                    installAgents: agents,
                    isCentral: skill.is_central,
                    installSummaryMembers: [skill],
                            installLinkedAgentIds: skill.linked_agents,
                    installReadOnlyAgentIds: skill.read_only_agents ?? [],
                    platformIcons: {
                      agents,
                      linkedAgents: skill.linked_agents,
                      readOnlyAgents: skill.read_only_agents ?? [],
                      skillId: skill.id,
                      onToggle: handleTogglePlatform,
                      onManage: () => handleOpenPlatformDrawer(skill.id),
                      togglingAgentId,
                    },
                  }))}
                />

      {/* Install Dialog */}
      <InstallDialog
        open={isDialogOpen}
        onOpenChange={setIsDialogOpen}
        skill={installTargetSkill}
        agents={agents}
        onInstall={handleInstall}
      />

      <SkillDetailDrawer
        open={isDrawerOpen}
        skillId={drawerSkillId}
        onOpenChange={(open) => {
          setIsDrawerOpen(open);
          if (!open) {
            setDrawerSkillId(null);
          }
        }}
        returnFocusRef={
          drawerSkillId
            ? {
                current: detailButtonRefs.current[drawerSkillId] ?? null,
              }
            : undefined
        }
      />

      <PlatformInstallDrawer
        open={isPlatformDrawerOpen}
        skill={platformDrawerSkill}
        agents={agents}
        togglingAgentId={togglingAgentId}
        onOpenChange={(open) => {
          setIsPlatformDrawerOpen(open);
          if (!open) {
            setPlatformDrawerSkillId(null);
          }
        }}
        onToggle={handleTogglePlatform}
        onOpenInstallDialog={() => {
          if (platformDrawerSkill) {
            setInstallTargetSkill(platformDrawerSkill);
            setIsPlatformDrawerOpen(false);
            setPlatformDrawerSkillId(null);
            setIsDialogOpen(true);
          }
        }}
      />

      <Dialog
        open={!!deleteTargetSkill}
        onOpenChange={(open) => {
          if (!open) setDeleteTargetSkill(null);
        }}
      >
        <DialogContent className="sm:max-w-md">
          <DialogHeader>
            <DialogTitle>
              {t("central.deleteConfirmTitle", { name: deleteTargetSkill?.name ?? "" })}
            </DialogTitle>
            <DialogDescription>
              {deleteTargetSkill
                ? t("central.deleteLinkedWarning", {
                    platforms: linkedAgentNames(deleteTargetSkill).join(", "),
                  })
                : ""}
            </DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <Button
              variant="outline"
              onClick={() => setDeleteTargetSkill(null)}
              disabled={!!deleteTargetSkill && deletingSkillId === deleteTargetSkill.id}
            ><ActionIcon action="cancel"/>
              {t("common.cancel")}
            </Button>
            <Button
              variant="destructive"
              onClick={() => {
                if (deleteTargetSkill) {
                  void handleDeleteCentralSkill(deleteTargetSkill, true);
                }
              }}
              disabled={!!deleteTargetSkill && deletingSkillId === deleteTargetSkill.id}
            ><ActionIcon action="delete"/>
              {t("central.deleteCascadeLabel")}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      <Dialog
        open={!!deleteTargetBundle}
        onOpenChange={(open) => {
          if (!open) {
            setDeleteTargetBundle(null);
            clearBundleDeletePreview();
          }
        }}
      >
        <DialogContent className="sm:max-w-lg">
          <DialogHeader>
            <DialogTitle>
              {t("central.deleteBundleConfirmTitle", {
                name: deleteTargetBundle?.name ?? "",
              })}
            </DialogTitle>
            <DialogDescription>
              {bundleDeletePreview?.bundle.isSymlink
                ? t("central.deleteBundleSymlinkWarning", {
                    path: formatPathForDisplay(
                      bundleDeletePreview.bundle.path || deleteTargetBundle?.path || ""
                    ),
                  })
                : t("central.deleteBundleDirectoryWarning", {
                    path: formatPathForDisplay(
                      bundleDeletePreview?.bundle.path || deleteTargetBundle?.path || ""
                    ),
                  })}
            </DialogDescription>
          </DialogHeader>

          <div className="space-y-3 text-sm">
            <div className="rounded-xl border border-destructive/30 bg-destructive/5 p-3">
              <div className="flex items-center gap-2 font-medium text-destructive">
                <AlertTriangle className="size-4" />
                {t("central.deleteBundleDangerTitle")}
              </div>
              <p className="mt-1 text-muted-foreground">
                {t("central.deleteBundleDangerDescription", {
                  count:
                    bundleDeletePreview?.bundle.skillCount ??
                    deleteTargetBundle?.skillCount ??
                    0,
                })}
              </p>
            </div>

            {bundleDeletePreview && (
              <div className="space-y-2">
                <div>
                  <div className="text-xs font-medium text-muted-foreground">
                    {t("central.deleteBundleSkillsLabel")}
                  </div>
                  <div className="mt-1 flex flex-wrap gap-1.5">
                    {bundleDeletePreview.skills.map((skill) => (
                      <span
                        key={skill.id}
                        className="rounded-full bg-muted px-2 py-0.5 text-xs"
                      >
                        {skill.name}
                      </span>
                    ))}
                  </div>
                </div>

                {bundleDeletePreview.affectedAgents.length > 0 && (
                  <div>
                    <div className="text-xs font-medium text-muted-foreground">
                      {t("central.deleteBundleAgentsLabel")}
                    </div>
                    <div className="mt-1 text-muted-foreground">
                      {agentDisplayNames(bundleDeletePreview.affectedAgents).join(", ")}
                    </div>
                  </div>
                )}
              </div>
            )}
          </div>

          <DialogFooter>
            <Button
              variant="outline"
              onClick={() => {
                setDeleteTargetBundle(null);
                clearBundleDeletePreview();
              }}
              disabled={
                !!deleteTargetBundle &&
                deletingBundlePath === deleteTargetBundle.relativePath
              }
            ><ActionIcon action="cancel"/>
              {t("common.cancel")}
            </Button>
            <Button
              variant="destructive"
              onClick={() => {
                if (deleteTargetBundle) {
                  void handleDeleteCentralBundle(deleteTargetBundle);
                }
              }}
              disabled={
                !bundleDeletePreview ||
                (!!deleteTargetBundle &&
                  deletingBundlePath === deleteTargetBundle.relativePath)
              }
            ><ActionIcon action="delete"/>
              {t("central.deleteBundleCascadeLabel")}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  );
}
