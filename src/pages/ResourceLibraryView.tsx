import {continueGitHubImportInBackground} from "@/stores/importPreparationStore";
import {RepositoryAddressButton} from "@/components/skill/RepositoryAddressButton";
import {useCollectionStore} from "@/stores/collectionStore";
import {collectionMembers,collectionScope,UNFILED_COLLECTION,SKILLS_DRAG,COLLECTION_SOURCE_DRAG} from "@/lib/collectionGroups";
import { ActionIcon } from "@/components/ui/action-icon";
import { RepositoryCheckConfirm } from "@/components/skill/RepositoryCheckConfirm";
import { useTaskQueueStore } from "@/stores/taskQueueStore";
import { AddSkillsDialog } from "@/components/skill/AddSkillsDialog";
import { useMetadataStore, findFolderNote } from "@/stores/metadataStore";
import { FolderNotesEditor } from "@/components/skill/FolderNotesEditor";
import { SearchScopes } from "@/components/skill/SearchScopes";
import { useSearchScopes, matchesSearch } from "@/lib/skillFilters";
import { TagFilters } from "@/components/skill/TagFilters";
import { matchesTags } from "@/lib/skillFilters";
import { SkillBrowserHeader } from "@/components/skill/SkillBrowserHeader";
import { SidebarTagFilter } from "@/components/layout/SidebarTagFilter";
import { SkillBrowserWorkspace } from "@/components/skill/SkillBrowserWorkspace";
import { openSkillSearch } from "@/lib/skillNavigation";
import { useRepositorySyncStore } from "@/stores/repositorySyncStore";
import {
Loader2,
RotateCw,
PackagePlus,
RefreshCw,
Trash2
} from "lucide-react";
import { useDeferredValue,useEffect,useMemo,useRef,useState } from "react";
import { useTranslation } from "react-i18next";
import { useSearchParams } from "react-router-dom";
import { toast } from "sonner";

import { InstallDialog } from "@/components/central/InstallDialog";
import { InstallTargetList } from "@/components/central/InstallTargetList";
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
import { useSkillListViewMode } from "@/hooks/useSkillListViewMode";
import { isInstallTargetAgent } from "@/lib/agents";
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
import { cn } from "@/lib/utils";
import { useCentralSkillsStore } from "@/stores/centralSkillsStore";
import { usePlatformStore } from "@/stores/platformStore";
import { useResourceLibraryStore } from "@/stores/resourceLibraryStore";
import { useSkillStore } from "@/stores/skillStore";
import type {
CentralSkillBundleDeletePreview,
SkillWithLinks,
} from "@/types";



function githubRepoFromSourceLabel(source?: string | null): string | null {
  const prefix = "github:";
  if (!source?.startsWith(prefix)) return null;
  const repo = source.slice(prefix.length).trim();
  return repo.includes("/") ? repo : null;
}

function resourceSkillSourceRepo(skill: SkillWithLinks): string | null {
  return skill.source_repo ?? githubRepoFromSourceLabel(skill.source) ?? null;
}

function isSourceBackedSkill(skill: SkillWithLinks) {
  return !!(skill.source_url || (resourceSkillSourceRepo(skill) && skill.source_path));
}

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

export function ResourceLibraryView() {
  const [locationParams, setLocationParams] = useSearchParams();
  const [addressRepository,setAddressRepository] = useState<string | null>(null);
  const [locationReady, setLocationReady] = useState(false);
  const [locatedSkillId, setLocatedSkillId] = useState<string | null>(null);
  const [locatedFolderKey, setLocatedFolderKey] = useState<string | null>(null);
  const { t } = useTranslation();
  const skills = useResourceLibraryStore((state) => state.skills);
  const agents = useResourceLibraryStore((state) => state.agents);
  const resourceLibraryDir = useResourceLibraryStore((state) => state.resourceLibraryDir);
  const isLoading = useResourceLibraryStore((state) => state.isLoading);
  const isUpdatingSources = useResourceLibraryStore((state) => state.isUpdatingSources);
  const togglingAgentId = useResourceLibraryStore((state) => state.togglingAgentId);
  const deletingSkillId = useResourceLibraryStore((state) => state.deletingSkillId);
  const loadResourceLibrary = useResourceLibraryStore((state) => state.loadResourceLibrary);
  const installSkill = useResourceLibraryStore((state) => state.installSkill);
  const addToCentral = useResourceLibraryStore((state) => state.addToCentral);
  const removeFromCentral = useResourceLibraryStore((state) => state.removeFromCentral);
  const togglePlatformLink = useResourceLibraryStore((state) => state.togglePlatformLink);
  const previewDeleteResourceBundle = useResourceLibraryStore(
    (state) => state.previewDeleteResourceBundle
  );
  const deleteResourceBundle = useResourceLibraryStore((state) => state.deleteResourceBundle);
  const deleteResourceSkill = useResourceLibraryStore((state) => state.deleteResourceSkill);
  const checkForUpdates = useRepositorySyncStore((state) => state.checkForUpdates);

  const refreshCounts = usePlatformStore((state) => state.refreshCounts);
  const loadCentralSkills = useCentralSkillsStore((state) => state.loadCentralSkills);
  const getSkillsByAgent = useSkillStore((state) => state.getSkillsByAgent);
  const uninstallSkillFromAgent = useSkillStore((state) => state.uninstallSkillFromAgent);

  const [viewMode, setViewMode] = useSkillListViewMode("resource-library");


  const [sortField, setSortField] = useState<SkillSortField>("name");
  const [sortDirection, setSortDirection] = useState<SkillSortDirection>("asc");
  const [searchQuery, setSearchQuery] = useState("");
  const folderNotes = useMetadataStore(state=>state.folders);
  useEffect(()=>{void useMetadataStore.getState().loadFolders();},[]);
  const [searchScopes, setSearchScopes] = useSearchScopes();
  const [selectedTags, setSelectedTags] = useState<string[]>([]);
  const [activeFolderKey, setActiveFolderKey] = useState<string | null>(null);
  const [installTargetSkill, setInstallTargetSkill] = useState<SkillWithLinks | null>(null);
  const [deleteTargetSkill, setDeleteTargetSkill] = useState<SkillWithLinks | null>(null);
  const [isInstallDialogOpen, setIsInstallDialogOpen] = useState(false);
  const [drawerSkillId, setDrawerSkillId] = useState<string | null>(null);
  const [isDrawerOpen, setIsDrawerOpen] = useState(false);
  const [updatingSkillId, setUpdatingSkillId] = useState<string | null>(null);
  const [isCheckConfirmOpen, setIsCheckConfirmOpen] = useState(false);
  const [isLocalAddOpen, setIsLocalAddOpen] = useState(false);
  const [folderDeletePreview, setFolderDeletePreview] =
    useState<CentralSkillBundleDeletePreview | null>(null);
  const [isDeletingFolder, setIsDeletingFolder] = useState(false);
  const [folderInstallTargetIds, setFolderInstallTargetIds] = useState<Set<string>>(new Set());
  const [folderUninstallTargetIds, setFolderUninstallTargetIds] = useState<Set<string>>(new Set());
  const [folderInstallMethod, setFolderInstallMethod] = useState<"auto" | "symlink" | "copy">("auto");
  const [folderActionGroupKey, setFolderActionGroupKey] = useState<string | null>(null);
  const [folderActionMode, setFolderActionMode] = useState<"install" | "uninstall" | null>(null);
  const [pendingFolderAction, setPendingFolderAction] = useState<
    "central" | "install" | "uninstall" | "update" | null
  >(null);
  const [pendingFolderActionKey, setPendingFolderActionKey] = useState<string | null>(null);
  const isRepositorySyncPreviewLoading = useRepositorySyncStore(s=>s.isChecking);
  const pendingRepositorySync = false;

  const detailButtonRefs = useRef<Record<string, HTMLButtonElement | null>>({});
  const deferredSearchQuery = useDeferredValue(searchQuery);
  const effectiveSearchQuery = skills.length > 80 ? deferredSearchQuery : searchQuery;
  const normalizedSearchQuery = useMemo(
    () => normalizeSearchQuery(effectiveSearchQuery),
    [effectiveSearchQuery]
  );

  useEffect(() => {
    let active = true;
    const ready = () => { if (active) setLocationReady(true); };
    Promise.resolve(loadResourceLibrary()).then(ready, ready);
    return () => { active = false; };
  }, [loadResourceLibrary]);

  const groupState=useCollectionStore(s=>s);
  useEffect(()=>()=>useCollectionStore.setState({selectedSkillIds:[]}),[]);
  const {collections:groupIndex,currentDetail:groupDetail,loadGroupDetails}=groupState;
  const groupError=t('groups.operationFailed');
  useEffect(()=>{void loadGroupDetails().catch(()=>toast.error(groupError));},[groupIndex,groupDetail,loadGroupDetails,groupError]);
  const visibleSkills = useMemo(() => {
    const active=groupState.activeCollectionId;
    if(!active)return skills;
    const assigned=collectionMembers(groupState.groupDetails,new Set(groupState.collections.map(item=>item.id)));
    if(active===UNFILED_COLLECTION)return skills.filter(skill=>!assigned.has(skill.id));
    const members=collectionMembers(groupState.groupDetails,collectionScope(groupState.collections,active,groupState.includeDescendants));
    return skills.filter(skill=>members.has(skill.id));
  },[skills,groupState.activeCollectionId,groupState.groupDetails,groupState.collections,groupState.includeDescendants]);
  const folderSplit = useMemo(
    () => splitResourceLibrarySkillsByFolder(skills, resourceLibraryDir),
    [resourceLibraryDir, skills]
  );

  const scopedFolders = useMemo(()=>splitResourceLibrarySkillsByFolder(visibleSkills,resourceLibraryDir).groups,[visibleSkills,resourceLibraryDir]);
  const folderGroupsByPath = useMemo(()=>new Map(scopedFolders.map(group=>[group.relativePath,group])),[scopedFolders]);
  useEffect(() => {
    const id = locationParams.get("locate");
    if (!id || isLoading || !locationReady) return;
    const target = skills.find(skill => skill.id === id) ?? skills.find(skill =>
      locationParams.has("repo") && skill.source_repo?.toLowerCase() === locationParams.get("repo")?.toLowerCase()
      && skill.source_path === locationParams.get("path"));
    if (!target) {
      if (!resourceLibraryDir) return;
      toast.error(t("skillBrowser.repositorySkillMissing"));
    } else {
      const group = folderSplit.groups.find(group => group.skills.some(skill => skill.id === target.id));
      const folder = locationParams.get("view") === "folders" && group;
      setViewMode(folder ? "folders" : "all");
      setActiveFolderKey(null);
      setSearchQuery("");
      setSelectedTags(current=>current.length?[]:current);
      setLocatedSkillId(folder ? null : target.id);
      setLocatedFolderKey(folder ? group.relativePath : null);
    }
    setLocationParams({}, { replace: true });
  }, [locationParams, locationReady, isLoading, skills, resourceLibraryDir, folderSplit.groups, setViewMode, setLocationParams, t]);

  function handleSearchSkill(skill: Parameters<typeof openSkillSearch>[0]) {
    void openSkillSearch(skill).catch(() => toast.error(t("skillBrowser.openSearchError")));
  }



  useEffect(() => {
    if (viewMode === "all") {
      setActiveFolderKey(null);
      return;
    }
    if (activeFolderKey && !folderGroupsByPath.has(activeFolderKey)) {
      setActiveFolderKey(null);
    }
  }, [activeFolderKey, folderGroupsByPath, viewMode]);

  useEffect(() => {
    setFolderInstallTargetIds(new Set());
    setFolderUninstallTargetIds(new Set());
    setFolderInstallMethod("auto");
    setPendingFolderAction(null);
  }, [activeFolderKey]);

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

  const filteredSkills = useMemo(() => {
    return visibleSkills.filter((skill) => {
      if (!matchesTags(skill.tags, selectedTags)) {
        return false;
      }
      if (!normalizedSearchQuery) return true;
      const group = folderSplit.groups.find(group=>group.skills.some(item=>item.id===skill.id));
      const note = group ? findFolderNote(folderNotes,group.skills.map(item=>item.id))?.notes : "";
      return matchesSearch(skill,normalizedSearchQuery,searchScopes,group?.name,note);
    });
  }, [normalizedSearchQuery, selectedTags, visibleSkills, folderSplit.groups, folderNotes, searchScopes]);
  const sortedSkills = useMemo(() => sortBySkillBrowserOrder(filteredSkills, sortField, sortDirection),[filteredSkills,sortField,sortDirection]);
  const filteredFolders = useMemo(() => sortFoldersBySkillBrowserOrder(scopedFolders.filter(group=>group.skills.some(skill=>filteredSkills.some(match=>match.id===skill.id))),sortField,sortDirection),[scopedFolders,filteredSkills,sortField,sortDirection]);
  // Partial search/group results must retain their selected members, never widen to a whole source.
  const completeDragRepositories = useMemo(() => {
    const visible = new Set(filteredSkills.map(skill => skill.id));
    const complete = new Map<string, boolean>();
    for (const skill of skills) if (skill.source_repo) {
      const repo = skill.source_repo.toLowerCase();
      complete.set(repo, (complete.get(repo) ?? true) && visible.has(skill.id));
    }
    return complete;
  }, [skills, filteredSkills]);
  const canDetachGroup=!!groupState.activeCollectionId&&groupState.activeCollectionId!==UNFILED_COLLECTION;
  async function detachSkills(ids:string[]){if(!canDetachGroup)return;try{await groupState.changeGroup({action:'detach',id:groupState.activeCollectionId!,skillIds:ids});}catch{toast.error(t('groups.operationFailed'));}}

  const availableInstallAgents = useMemo(
    () => agents.filter(isInstallTargetAgent),
    [agents]
  );

  async function refreshSyncedInstallTargets(agentIds?: string[]) {
    const targetIds = agentIds ?? availableInstallAgents.map((agent) => agent.id);
    await Promise.all([
      refreshCounts(),
      ...targetIds.map((agentId) => getSkillsByAgent(agentId)),
    ]);
  }

  const folderActionGroup = folderActionGroupKey
    ? folderGroupsByPath.get(folderActionGroupKey) ?? null
    : null;
  const folderActionLinkedAgentIds = useMemo(() => {
    const ids = new Set<string>();
    for (const skill of folderActionGroup?.skills ?? []) {
      for (const agentId of skill.linked_agents) {
        ids.add(agentId);
      }
    }
    return ids;
  }, [folderActionGroup?.skills]);
  const folderActionUninstallAgents = useMemo(
    () => availableInstallAgents.filter((agent) => folderActionLinkedAgentIds.has(agent.id)),
    [folderActionLinkedAgentIds, availableInstallAgents]
  );
  async function handleRefresh() {
    await loadResourceLibrary();
  }

  function handleOpenDrawer(skillId: string) {
    setDrawerSkillId(skillId);
    setIsDrawerOpen(true);
  }

  function setDetailButtonRef(skillId: string, node: HTMLButtonElement | null) {
    if (node) {
      detailButtonRefs.current[skillId] = node;
    } else {
      delete detailButtonRefs.current[skillId];
    }
  }

  function handleInstallClick(skill: SkillWithLinks) {
    setInstallTargetSkill(skill);
    setIsInstallDialogOpen(true);
  }

  async function handleInstall(skillId: string, agentIds: string[], method: string) {
    const result = await installSkill(skillId, agentIds, method);
    if (result.failed.length > 0) {
      toast.error(
        t("central.installPartialFail", {
          platforms: result.failed.map((item) => item.agent_id).join(", "),
        })
      );
    }
    await Promise.all([
      refreshCounts(),
      ...agentIds.map((agentId) => getSkillsByAgent(agentId)),
    ]);
    return result;
  }

  async function handleTogglePlatform(skillId: string, agentId: string) {
    try {
      await togglePlatformLink(skillId, agentId);
      await Promise.all([refreshCounts(), getSkillsByAgent(agentId)]);
    } catch (err) {
      toast.error(t("central.installError", { error: String(err) }));
    }
  }

  async function handleUpdateSources() {
    if (pendingRepositorySync || isUpdatingSources) return;
    setIsCheckConfirmOpen(true);
  }

  async function handleUpdateSingleSource(skill: SkillWithLinks) {
    const repository=resourceSkillSourceRepo(skill);
    if(repository){await checkForUpdates([repository]);return;}
    useTaskQueueStore.getState().enqueue({key:`update:${skill.id}`,kind:"update",label:skill.name,locks:[`skill:${skill.id}`],steps:[{command:"update_source_backed_resource_skill",args:{skillId:skill.id},label:skill.name}]});
  }

  async function handleAddToCentral(skill: SkillWithLinks) {
    setUpdatingSkillId(skill.id);
    try {
      await addToCentral(skill.id);
      await Promise.all([loadCentralSkills(), refreshSyncedInstallTargets()]);
      toast.success(t("resource.addToCentralSuccess", { name: skill.name }));
    } catch (err) {
      toast.error(t("resource.addToCentralError", { name: skill.name, error: String(err) }));
    } finally {
      setUpdatingSkillId(null);
    }
  }

  async function handleRemoveFromCentral(skill: SkillWithLinks) {
    setUpdatingSkillId(skill.id);
    try {
      const affectedAgentIds = [
        ...skill.linked_agents,
        ...(skill.read_only_agents ?? []),
      ];
      await removeFromCentral(skill.id);
      await Promise.all([
        loadCentralSkills(),
        refreshCounts(),
        ...affectedAgentIds.map((agentId) => getSkillsByAgent(agentId)),
      ]);
      toast.success(t("resource.removeFromCentralSuccess", { name: skill.name }));
    } catch (err) {
      toast.error(t("resource.removeFromCentralError", { error: String(err) }));
    } finally {
      setUpdatingSkillId(null);
    }
  }

  async function handleUninstallFromAllTargets(skill: SkillWithLinks) {
    setUpdatingSkillId(skill.id);
    try {
      for (const agentId of skill.linked_agents) {
        await uninstallSkillFromAgent(skill.id, agentId);
      }
      await Promise.all([
        loadResourceLibrary(),
        refreshCounts(),
        ...skill.linked_agents.map((agentId) => getSkillsByAgent(agentId)),
      ]);
    } catch (err) {
      toast.error(t("detail.uninstallError", { error: String(err) }));
    } finally {
      setUpdatingSkillId(null);
    }
  }

  function closeFolderActionDialog() {
    setFolderActionMode(null);
    setFolderActionGroupKey(null);
    setFolderInstallTargetIds(new Set());
    setFolderUninstallTargetIds(new Set());
    setFolderInstallMethod("auto");
  }

  function handleFolderInstallTargetChange(agentId: string, checked: boolean) {
    setFolderInstallTargetIds((prev) => {
      const next = new Set(prev);
      if (checked) {
        next.add(agentId);
      } else {
        next.delete(agentId);
      }
      return next;
    });
  }

  function handleFolderUninstallTargetChange(agentId: string, checked: boolean) {
    setFolderUninstallTargetIds((prev) => {
      const next = new Set(prev);
      if (checked) {
        next.add(agentId);
      } else {
        next.delete(agentId);
      }
      return next;
    });
  }

  function handleOpenInstallFolder(group: SkillFolderGroup<SkillWithLinks>) {
    setFolderActionGroupKey(group.relativePath);
    setFolderActionMode("install");
    setFolderInstallTargetIds(new Set());
    setFolderInstallMethod("symlink");
  }

  function handleOpenUninstallFolder(group: SkillFolderGroup<SkillWithLinks>) {
    setFolderActionGroupKey(group.relativePath);
    setFolderActionMode("uninstall");
    setFolderUninstallTargetIds(new Set());
  }

  async function handleAddFolderToCentral(group: SkillFolderGroup<SkillWithLinks>) {
    if (pendingFolderAction) return;
    setPendingFolderAction("central");
    setPendingFolderActionKey(group.relativePath);
    try {
      for (const skill of group.skills) {
        if (!skill.is_central) {
          await addToCentral(skill.id);
        }
      }
      await Promise.all([
        loadResourceLibrary(),
        loadCentralSkills(),
        refreshSyncedInstallTargets(),
      ]);
      toast.success(t("skillFolder.addFolderToCentralSuccess", { count: group.skills.length }));
    } catch (err) {
      toast.error(t("resource.addToCentralError", { name: group.name, error: String(err) }));
    } finally {
      setPendingFolderAction(null);
      setPendingFolderActionKey(null);
    }
  }

  async function handleRemoveFolderFromCentral(group: SkillFolderGroup<SkillWithLinks>) {
    if (pendingFolderAction) return;
    setPendingFolderAction("central");
    setPendingFolderActionKey(group.relativePath);
    try {
      for (const skill of group.skills) {
        if (skill.is_central) {
          await removeFromCentral(skill.id);
        }
      }
      await Promise.all([
        loadResourceLibrary(),
        loadCentralSkills(),
        refreshSyncedInstallTargets(),
      ]);
    } catch (err) {
      toast.error(t("resource.removeFromCentralError", { error: String(err) }));
    } finally {
      setPendingFolderAction(null);
      setPendingFolderActionKey(null);
    }
  }

  async function handleUpdateFolderSources(group: SkillFolderGroup<SkillWithLinks>) {
    if (pendingFolderAction || isRepositorySyncPreviewLoading || pendingRepositorySync) return;
    const repositories = Array.from(
      new Set(
        group.skills
          .map(resourceSkillSourceRepo)
          .filter((repo): repo is string => !!repo && repo.includes("/"))
      )
    );
    if (repositories.length === 0) {
      toast.error(t("resource.updateSourcesError", { error: t("resource.noSourceBackedSkills") }));
      return;
    }
    await checkForUpdates(repositories);
  }

  async function handleInstallFolderToTarget() {
    const isFolderCentral =
      folderActionGroup?.skills.every((skill) => skill.is_central) ?? false;
    const targetIds = Array.from(folderInstallTargetIds).filter((agentId) => {
      const agent = availableInstallAgents.find((candidate) => candidate.id === agentId);
      if (!agent) return false;
      return !(agent.shares_central_skills && isFolderCentral);
    });
    if (!folderActionGroup || targetIds.length === 0 || pendingFolderAction) return;
    setPendingFolderAction("install");
    setPendingFolderActionKey(folderActionGroup.relativePath);
    try {
      let failedCount = 0;
      for (const skill of folderActionGroup.skills) {
        const result = await installSkill(skill.id, targetIds, folderInstallMethod);
        failedCount += result.failed.length;
      }
      await Promise.all([
        loadResourceLibrary(),
        refreshCounts(),
        ...targetIds.map((agentId) => getSkillsByAgent(agentId)),
      ]);
      if (failedCount > 0) {
        toast.error(t("skillFolder.installFolderPartialFail", { count: failedCount }));
        return;
      }
      toast.success(t("skillFolder.installFolderSuccess", { count: folderActionGroup.skills.length }));
      closeFolderActionDialog();
    } catch (err) {
      toast.error(t("central.installError", { error: String(err) }));
    } finally {
      setPendingFolderAction(null);
      setPendingFolderActionKey(null);
    }
  }

  async function handleUninstallFolderFromTarget() {
    const targetIds = Array.from(folderUninstallTargetIds);
    if (!folderActionGroup || targetIds.length === 0 || pendingFolderAction) return;
    setPendingFolderAction("uninstall");
    setPendingFolderActionKey(folderActionGroup.relativePath);
    try {
      let uninstallCount = 0;
      for (const agentId of targetIds) {
        const removableSkills = folderActionGroup.skills.filter((skill) =>
          skill.linked_agents.includes(agentId)
        );
        uninstallCount += removableSkills.length;
        for (const skill of removableSkills) {
          await uninstallSkillFromAgent(skill.id, agentId);
        }
      }
      await Promise.all([
        loadResourceLibrary(),
        refreshCounts(),
        ...targetIds.map((agentId) => getSkillsByAgent(agentId)),
      ]);
      toast.success(t("skillFolder.uninstallFolderSuccess", { count: uninstallCount }));
      closeFolderActionDialog();
    } catch (err) {
      toast.error(t("detail.uninstallError", { error: String(err) }));
    } finally {
      setPendingFolderAction(null);
      setPendingFolderActionKey(null);
    }
  }

  function linkedAgentNames(skill: SkillWithLinks) {
    const affectedIds = new Set([
      ...skill.linked_agents,
      ...(skill.read_only_agents ?? []),
    ]);
    return agents
      .filter((agent) => affectedIds.has(agent.id))
      .map((agent) => agent.display_name);
  }

  async function handleDeleteResourceSkill(skill: SkillWithLinks, cascadeUninstall: boolean) {
    try {
      await deleteResourceSkill(skill.id, { cascadeUninstall });
      await Promise.all([
        refreshCounts(),
        loadCentralSkills(),
        ...skill.linked_agents.map((agentId) => getSkillsByAgent(agentId)),
      ]);
      toast.success(t("resource.deleteSuccess", { name: skill.name }));
      setDeleteTargetSkill(null);
    } catch (err) {
      toast.error(t("resource.deleteError", { error: String(err) }));
    }
  }

  function handleDeleteClick(skill: SkillWithLinks) {
    if (skill.linked_agents.length > 0 || (skill.read_only_agents?.length ?? 0) > 0) {
      setDeleteTargetSkill(skill);
      return;
    }

    void handleDeleteResourceSkill(skill, false);
  }

  async function handleDeleteFolderClick(group: SkillFolderGroup<SkillWithLinks>) {
    try {
      const preview = await previewDeleteResourceBundle(group.relativePath);
      setFolderDeletePreview(preview);
    } catch (err) {
      toast.error(t("resource.deleteFolderError", { error: String(err) }));
    }
  }

  async function handleConfirmDeleteFolder() {
    if (!folderDeletePreview) return;
    const cascadeUninstall = folderDeletePreview.affectedAgents.length > 0;
    setIsDeletingFolder(true);
    try {
      await deleteResourceBundle(folderDeletePreview.bundle.relativePath, { cascadeUninstall });
      await Promise.all([
        refreshCounts(),
        loadCentralSkills(),
        ...folderDeletePreview.affectedAgents.map((agentId) => getSkillsByAgent(agentId)),
      ]);
      toast.success(t("resource.deleteFolderSuccess", { name: folderDeletePreview.bundle.name }));
      setFolderDeletePreview(null);
    } catch (err) {
      toast.error(t("resource.deleteFolderError", { error: String(err) }));
    } finally {
      setIsDeletingFolder(false);
    }
  }

  return (
    <div className="flex h-full flex-col">





      <SidebarTagFilter hasSelection={selectedTags.length > 0} onClear={() => setSelectedTags([])}><TagFilters tags={availableTags} selected={selectedTags} onChange={setSelectedTags}/></SidebarTagFilter>
      <SkillBrowserWorkspace key={groupState.activeCollectionId??'all'} onSelectionChange={keys=>{const ids=new Set(keys.filter(key=>key.startsWith('skill:')).map(key=>key.slice(6)));for(const group of filteredFolders)if(keys.includes(`folder:${group.relativePath}`))for(const skill of group.skills)if(filteredSkills.some(item=>item.id===skill.id))ids.add(skill.id);useCollectionStore.setState({selectedSkillIds:[...ids]});}} onDragSkills={(event,ids)=>{event.dataTransfer.setData(SKILLS_DRAG,JSON.stringify(ids));event.dataTransfer.effectAllowed='copyMove';if(groupState.activeCollectionId&&groupState.activeCollectionId!==UNFILED_COLLECTION)event.dataTransfer.setData(COLLECTION_SOURCE_DRAG,groupState.activeCollectionId);}} onRefresh={handleRefresh} onDetachSkills={canDetachGroup ? detachSkills : undefined} toolbar={viewControl => <SkillBrowserHeader viewControl={viewControl} title={<div>
          <div className="flex items-center gap-2">
            <h1 className="text-sm font-semibold">{groupState.activeCollectionId===UNFILED_COLLECTION?t('groups.unfiled'):groupState.collections.find(item=>item.id===groupState.activeCollectionId)?.name??t("resource.title")}</h1>
            <Button
              variant="ghost"
              size="icon"
              onClick={handleRefresh}
              disabled={isLoading}
              aria-label={t("resource.refresh")}
            >
              <RotateCw className={cn("size-4", isLoading && "animate-spin")} />
            </Button>
          </div>
          <OpenableDirectoryPath iconOnly
            path={resourceLibraryDir}
            displayPath={resourceLibraryDir || t("resource.path")}
          />
        </div>} search={<div className="flex items-center gap-2"><SearchInput
            placeholder={t("resource.searchPlaceholder")}
            value={searchQuery}
            onValueChange={setSearchQuery}
            containerClassName="w-80 min-w-0 max-w-none flex-1"
            aria-label={t("resource.searchPlaceholder")}
          trailing={<SearchScopes value={searchScopes} onChange={setSearchScopes}/>} /></div>} actions={<>
          <Button variant="ghost" size="icon-sm" title={t("resource.addSkills")} onClick={() => setIsLocalAddOpen(true)}>
            <PackagePlus className="size-4" />
            <span className="sr-only">{t("resource.addSkills")}</span>
          </Button>
          <Button
            variant="ghost" size="icon-sm"
            title={t("resource.updateSources")} onClick={handleUpdateSources}
            disabled={isUpdatingSources || isRepositorySyncPreviewLoading || pendingRepositorySync}
          >
            {isUpdatingSources || isRepositorySyncPreviewLoading || pendingRepositorySync ? (
              <Loader2 className="size-4 animate-spin" />
            ) : (
              <RefreshCw className="size-4" />
            )}
            <span className="sr-only">{t("resource.updateSources")}</span>
          </Button>
        </>} />} loading={isLoading} showGithubStars folders={filteredFolders.map(
                    (group): FolderTableItem => ({
                      key: group.relativePath,
                      highlighted: locatedFolderKey === group.relativePath,
                      onSearch: () => handleSearchSkill({ ...group.skills[0], name: group.name }),
                      name: group.name,
                      onChangeRepositoryAddress: group.skills.every(skill => skill.source_repo && skill.source_repo.toLowerCase() === group.skills[0]?.source_repo?.toLowerCase()) ? () => setAddressRepository(group.skills[0].source_repo!) : undefined,
                      dragSourceRepo: group.skills[0]?.source_repo && completeDragRepositories.get(group.skills[0].source_repo.toLowerCase()) && group.skills.every(skill => skill.source_repo?.toLowerCase() === group.skills[0].source_repo?.toLowerCase()) ? group.skills[0].source_repo : undefined,
                      sourceRepo: group.skills.every(skill => skill.source_repo && skill.source_repo.toLowerCase() === group.skills[0]?.source_repo?.toLowerCase()) ? group.skills[0]?.source_repo : undefined,
                      path: group.path,
                      tags: [...new Set(group.skills.flatMap(skill => skill.tags ?? []))],
                      skillCount: group.skillCount,
 skillKeys: group.skills.map(skill => skill.id),
                      notesEditor: <FolderNotesEditor key={group.relativePath} directory={group.path} skillIds={group.skills.map(skill=>skill.id)} />,
                      githubStars: group.skills.find((skill) => skill.github_stars != null)?.github_stars ?? null,
                      installAgents: agents,
                      installSummaryMembers: group.skills,
                      installLinkedAgentIds: group.linkedAgentIds,
                      installReadOnlyAgentIds: group.readOnlyAgentIds,
                      previewNames: group.skills.map((skill) => skill.name),
                      createdAt: earliestSkillCreatedAt(group.skills),
                      updatedAt: latestSkillUpdatedAt(group.skills),
                      onOpen: () => setActiveFolderKey(group.relativePath),
                      onAddToCentral: group.skills.some((skill) => !skill.is_central)
                        ? () => void handleAddFolderToCentral(group)
                        : undefined,
                      addToCentralLabel: t("resource.addToCentralAction"),
                      onRemoveFromCentral: group.skills.every((skill) => skill.is_central)
                        ? () => void handleRemoveFolderFromCentral(group)
                        : undefined,
                      removeFromCentralLabel: t("resource.removeFromCentralAction"),
                      onUpdate: group.skills.some(isSourceBackedSkill)
                        ? () => void handleUpdateFolderSources(group)
                        : undefined,
                      updateLabel: t("workflow.updateRepositorySkills"),
                      reimport: group.skills[0]?.source_repo && group.skills.every(skill=>skill.source_repo?.toLowerCase()===group.skills[0].source_repo?.toLowerCase()) ? async()=>{
                        continueGitHubImportInBackground(`https://github.com/${group.skills[0].source_repo}`,undefined,undefined,true);
                      } : undefined,
                      onRefreshRepository: group.skills.some(skill=>!!resourceSkillSourceRepo(skill)) ? ()=>void useRepositorySyncStore.getState().refreshRepositories([...new Set(group.skills.map(resourceSkillSourceRepo).filter((repo):repo is string=>!!repo))]) : undefined,
                      onDetachGroup: canDetachGroup ? ()=>void detachSkills(group.skills.map(skill=>skill.id)) : undefined,
                      isUpdating:
                        pendingFolderAction === "update" &&
                        pendingFolderActionKey === group.relativePath,
                      isRemovingFromCentral:
                        pendingFolderAction === "central" &&
                        pendingFolderActionKey === group.relativePath,
                      isAddingToCentral:
                        pendingFolderAction === "central" &&
                        pendingFolderActionKey === group.relativePath,
                      onInstall:
                        availableInstallAgents.length > 0
                          ? () => handleOpenInstallFolder(group)
                          : undefined,
                      installLabel: t("resource.installToTargetsAction"),
                      isInstalling:
                        pendingFolderAction === "install" &&
                        pendingFolderActionKey === group.relativePath,
                      onUninstall:
                        group.linkedAgentCount > 0
                          ? () => handleOpenUninstallFolder(group)
                          : undefined,
                      uninstallLabel: t("resource.uninstallFromTargetsAction"),
                      isUninstalling:
                        pendingFolderAction === "uninstall" &&
                        pendingFolderActionKey === group.relativePath,
                      onDelete: () => void handleDeleteFolderClick(group),
                      deleteLabel: t("resource.deleteAction"),
                    })
                  )} storageKey="ResourceLibraryView"  searchActive={Boolean(normalizedSearchQuery || selectedTags.length)}
                  sortField={sortField}
                  sortDirection={sortDirection}
                  onSortChange={(field, direction) => {
                    setSortField(field);
                    setSortDirection(direction);
                  }}
                  skills={sortedSkills.map((skill) => {
                    const normalizedSourceRepo = resourceSkillSourceRepo(skill);
                    return {
                      rowKey: skill.id,
                      onDetachGroup: canDetachGroup ? ()=>void detachSkills([skill.id]) : undefined,
                      batchOperations: {
                        install: async (targets: string[]) => { const result=await installSkill(skill.id,targets,"auto"); await refreshSyncedInstallTargets(); return result; },
                        update: isSourceBackedSkill(skill) ? () => handleUpdateSingleSource(skill) : undefined,
                        delete: async () => { await deleteResourceSkill(skill.id,{cascadeUninstall:true}); await Promise.all([loadCentralSkills(),refreshSyncedInstallTargets()]); },
                        uninstall: skill.linked_agents.length ? async () => { for(const target of skill.linked_agents) await uninstallSkillFromAgent(skill.id,target); await Promise.all([loadResourceLibrary(),refreshSyncedInstallTargets()]); } : undefined,
                      },
 detailRequest: { skillId: skill.id },
                      highlighted: locatedSkillId === skill.id,
                      onSearch: () => handleSearchSkill(skill),
                      name: skill.name,
                      description: skill.description,
                      notes: skill.notes,
                      publisher: normalizedSourceRepo ?? skill.source_author ?? undefined,
                      sourceAuthor: skill.source_author,
                      sourceRepo: normalizedSourceRepo,
                      sourceUrl: skill.source_url,
                      createdAt: skill.created_at,
                      updatedAt: skill.updated_at,
                      tags: (skill.tags ?? []).map((tag) => ({ key: tag, label: tag })),
                      onDetail: () => handleOpenDrawer(skill.id),
                      isCentral: skill.is_central,
                      installAgents: agents,
                      installSummaryMembers: [skill],
                            installLinkedAgentIds: skill.linked_agents,
                      installReadOnlyAgentIds: skill.read_only_agents ?? [],
                      onInstallTo:
                        skill.linked_agents.length === 0
                          ? () => handleInstallClick(skill)
                          : undefined,
                      installToLabel: t("resource.installToTargetsAction"),
                      onUninstallFromPlatform:
                        skill.linked_agents.length > 0
                          ? () => void handleUninstallFromAllTargets(skill)
                          : undefined,
                      uninstallFromLabel: t("resource.uninstallFromTargetsAction"),
                      onInstallToCentral: skill.is_central
                        ? undefined
                        : () => void handleAddToCentral(skill),
                      installToCentralLabel: t("resource.addToCentralAction"),
                      onRemoveFromCentral: skill.is_central
                        ? () => void handleRemoveFromCentral(skill)
                        : undefined,
                      removeFromCentralLabel: t("resource.removeFromCentralAction"),
                      onDeleteFromCentral: () => handleDeleteClick(skill),
                      deleteFromCentralLabel: t("resource.deleteAction"),
                      deleteFromCentralRequiresDialog:
                        skill.linked_agents.length > 0 || (skill.read_only_agents?.length ?? 0) > 0,
                      onUpdateFromSource:
                        isSourceBackedSkill(skill)
                          ? () => void handleUpdateSingleSource(skill)
                          : undefined,
                      updateFromSourceLabel: t("resource.updateAction"),
                      isLoading: updatingSkillId === skill.id || deletingSkillId === skill.id,
                      detailButtonRef: (node) => setDetailButtonRef(skill.id, node),
                      platformIcons: {
                        agents,
                        linkedAgents: skill.linked_agents,
                        readOnlyAgents: skill.read_only_agents ?? [],
                        skillId: skill.id,
                        onToggle: handleTogglePlatform,
                        togglingAgentId,
                      },
                    };
                  })}
                />

      <InstallDialog
        open={isInstallDialogOpen}
        onOpenChange={setIsInstallDialogOpen}
        skill={installTargetSkill}
        agents={availableInstallAgents}
        onInstall={async (skillId, agentIds, method) => {
          await handleInstall(skillId, agentIds, method);
        }}
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


      {addressRepository && <RepositoryAddressButton repository={addressRepository} open onOpenChange={open=>{if(!open)setAddressRepository(null);}}/>}
      <Dialog
        open={!!deleteTargetSkill}
        onOpenChange={(open) => {
          if (!open) {
            setDeleteTargetSkill(null);
          }
        }}
      >
        <DialogContent className="sm:max-w-md">
          <DialogHeader>
            <DialogTitle>
              {t("resource.deleteConfirmTitle", { name: deleteTargetSkill?.name ?? "" })}
            </DialogTitle>
            <DialogDescription>
              {deleteTargetSkill
                ? t("resource.deleteLinkedWarning", {
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
                  void handleDeleteResourceSkill(deleteTargetSkill, true);
                }
              }}
              disabled={!!deleteTargetSkill && deletingSkillId === deleteTargetSkill.id}
            ><ActionIcon action="delete"/>
              {t("resource.deleteCascadeLabel")}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      <RepositoryCheckConfirm open={isCheckConfirmOpen} onOpenChange={setIsCheckConfirmOpen} />
      <AddSkillsDialog open={isLocalAddOpen} onOpenChange={setIsLocalAddOpen}/>

      <Dialog
        open={folderActionMode === "install"}
        onOpenChange={(open) => {
          if (!open && pendingFolderAction !== "install") {
            closeFolderActionDialog();
          }
        }}
      >
        <DialogContent className="sm:max-w-2xl">
          <DialogHeader>
            <DialogTitle>
              {t("skillFolder.installFolderTitle", {
                name: folderActionGroup?.name ?? "",
              })}
            </DialogTitle>
            <DialogDescription>
              {t("skillFolder.installFolderDesc", {
                count: folderActionGroup?.skills.length ?? 0,
              })}
            </DialogDescription>
          </DialogHeader>
          <div className="space-y-4">
            <InstallTargetList
              agents={availableInstallAgents}
              selectedAgentIds={folderInstallTargetIds}
              onToggleAgent={handleFolderInstallTargetChange}
              isCentral={folderActionGroup?.skills.every((skill) => skill.is_central) ?? false}
              emptyMessage={t("installDialog.noPlatforms")}
              ariaLabel={t("skillFolder.installTargetLabel")}
            />
            {availableInstallAgents.some(
              (agent) => agent.shares_central_skills && folderInstallTargetIds.has(agent.id)
            ) ? (
              <p className="text-xs text-amber-600 dark:text-amber-400">
                {t("installDialog.sharedPlatformHint")}
              </p>
            ) : null}
            <div className="space-y-1.5">
              <label className="text-xs font-medium text-muted-foreground">
                {t("installDialog.installMethod")}
              </label>
              <select
                value={folderInstallMethod}
                onChange={(event) =>
                  setFolderInstallMethod(event.target.value as "auto" | "symlink" | "copy")
                }
                className="h-9 w-full rounded-lg border border-input bg-background px-3 text-sm text-foreground outline-none focus-visible:ring-2 focus-visible:ring-ring"
                aria-label={t("installDialog.installMethod")}
              >
                <option value="auto">{t("installDialog.auto")}</option>
                <option value="symlink">{t("installDialog.symlink")}</option>
                <option value="copy">{t("installDialog.copy")}</option>
              </select>
            </div>
          </div>
          <DialogFooter>
            <Button
              variant="outline"
              onClick={closeFolderActionDialog}
              disabled={pendingFolderAction === "install"}
            ><ActionIcon action="cancel"/>
              {t("common.cancel")}
            </Button>
            <Button
              onClick={() => void handleInstallFolderToTarget()}
              disabled={folderInstallTargetIds.size === 0 || pendingFolderAction !== null}
            >
              {pendingFolderAction === "install" ? (
                <Loader2 className="size-4 animate-spin" />
              ) : (
                <PackagePlus className="size-4" />
              )}
              {t("skillFolder.installFolder")}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      <Dialog
        open={folderActionMode === "uninstall"}
        onOpenChange={(open) => {
          if (!open && pendingFolderAction !== "uninstall") {
            closeFolderActionDialog();
          }
        }}
      >
        <DialogContent className="sm:max-w-md">
          <DialogHeader>
            <DialogTitle>
              {t("skillFolder.uninstallFolderTitle", {
                name: folderActionGroup?.name ?? "",
              })}
            </DialogTitle>
            <DialogDescription>
              {t("skillFolder.uninstallFolderDesc", {
                count: folderActionGroup?.skills.length ?? 0,
              })}
            </DialogDescription>
          </DialogHeader>
          <InstallTargetList
            agents={folderActionUninstallAgents}
            selectedAgentIds={folderUninstallTargetIds}
            onToggleAgent={handleFolderUninstallTargetChange}
            emptyMessage={t("skillFolder.noUninstallTargets")}
            ariaLabel={t("skillFolder.uninstallTargetLabel")}
          />
          <DialogFooter>
            <Button
              variant="outline"
              onClick={closeFolderActionDialog}
              disabled={pendingFolderAction === "uninstall"}
            ><ActionIcon action="cancel"/>
              {t("common.cancel")}
            </Button>
            <Button
              variant="destructive"
              onClick={() => void handleUninstallFolderFromTarget()}
              disabled={folderUninstallTargetIds.size === 0 || pendingFolderAction !== null}
            >
              {pendingFolderAction === "uninstall" ? (
                <Loader2 className="size-4 animate-spin" />
              ) : (
                <Trash2 className="size-4" />
              )}
              {t("skillFolder.uninstallFolder")}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      <Dialog
        open={!!folderDeletePreview}
        onOpenChange={(open) => {
          if (!open) {
            setFolderDeletePreview(null);
          }
        }}
      >
        <DialogContent className="sm:max-w-md">
          <DialogHeader>
            <DialogTitle>
              {t("resource.deleteFolderConfirmTitle", {
                name: folderDeletePreview?.bundle.name ?? "",
              })}
            </DialogTitle>
            <DialogDescription>
              {folderDeletePreview
                ? t("resource.deleteFolderConfirmDesc", {
                    count: folderDeletePreview.skills.length,
                    platforms: folderDeletePreview.affectedAgents.join(", ") || t("common.none"),
                  })
                : ""}
            </DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <Button
              variant="outline"
              onClick={() => setFolderDeletePreview(null)}
              disabled={isDeletingFolder}
            ><ActionIcon action="cancel"/>
              {t("common.cancel")}
            </Button>
            <Button
              variant="destructive"
              onClick={() => void handleConfirmDeleteFolder()}
              disabled={isDeletingFolder}
            ><ActionIcon action="delete"/>
              {folderDeletePreview?.affectedAgents.length
                ? t("resource.deleteFolderCascadeLabel")
                : t("resource.deleteFolderLabelShort")}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  );
}
