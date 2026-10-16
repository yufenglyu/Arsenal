import { SearchScopes } from "@/components/skill/SearchScopes";
import { useSearchScopes, matchesSearch } from "@/lib/skillFilters";
import { TagFilters } from "@/components/skill/TagFilters";
import { matchesTags } from "@/lib/skillFilters";
import { sortBySkillBrowserOrder, type SkillSortField, type SkillSortDirection } from "@/lib/skillSort";
import { repositoryLocationUrl } from "@/lib/skillNavigation";
import { useEffect, useMemo, useRef, useState } from "react";
import { Plus, RotateCw } from "lucide-react";
import { useNavigate, useLocation } from "react-router-dom";
import { toast } from "sonner";
import { useTranslation } from "react-i18next";

import { Button } from "@/components/ui/button";
import { useCollectionStore, getCollectionDetails } from "@/stores/collectionStore";
import { usePlatformStore } from "@/stores/platformStore";
import { useResourceLibraryStore } from "@/stores/resourceLibraryStore";
import { CollectionEditor } from "@/components/collection/CollectionEditor";
import { SkillPickerDialog } from "@/components/collection/SkillPickerDialog";
import { CollectionInstallDialog } from "@/components/collection/CollectionInstallDialog";
import { InstallDialog } from "@/components/central/InstallDialog";
import { SkillDetailDrawer } from "@/components/skill/SkillDetailDrawer";
import { OpenableDirectoryPath } from "@/components/common/OpenableDirectoryPath";
import { SkillBrowserHeader } from "@/components/skill/SkillBrowserHeader";
import { SearchInput } from "@/components/ui/search-input";
import { SkillBrowserWorkspace } from "@/components/skill/SkillBrowserWorkspace";
import { CollectionDetail, SkillWithLinks } from "@/types";
import { SidebarTagFilter } from "@/components/layout/SidebarTagFilter";
import { cn } from "@/lib/utils";

import {
  consumeScrollPosition,
  consumeReturnContext,
} from "@/lib/scrollRestoration";

// Build a stable scroll-restoration key for a given collection id. The
// returned key scopes restoration state to that specific collection so that
// returning to the collections list only restores scroll if the user is still
// looking at the same collection context they originally left from.
function collectionScrollKey(collectionId: string): string {
  return `collection:${collectionId}`;
}

// Shared scope for the "collections" return-context map. Kept as a constant
// so the forward (CollectionsListView emit) and return (this view mounting)
// sides always agree on the key.
const COLLECTIONS_RETURN_SCOPE = "collections";

// ─── CollectionsListView ─────────────────────────────────────────────────────

export function CollectionsListView() {
  const { t } = useTranslation();
  const navigate = useNavigate();
  const [sortField, setSortField] = useState<SkillSortField>("name");
  const [sortDirection, setSortDirection] = useState<SkillSortDirection>("asc");
  const location = useLocation();
  const [search, setSearch] = useState("");
  const [searchScopes,setSearchScopes]=useSearchScopes();
  const [selectedTags, setSelectedTags] = useState<string[]>([]);
  const [collectionDetails, setCollectionDetails] = useState<CollectionDetail[]>([]);

  // Collection store
  const collections = useCollectionStore((s) => s.collections);
  const isLoading = useCollectionStore((s) => s.isLoading);
  const loadCollections = useCollectionStore((s) => s.loadCollections);
  const currentDetail = useCollectionStore((s) => s.currentDetail);
  const isLoadingDetail = useCollectionStore((s) => s.isLoadingDetail);
  const loadCollectionDetail = useCollectionStore((s) => s.loadCollectionDetail);

  const removeSkillFromCollection = useCollectionStore((s) => s.removeSkillFromCollection);
  const deleteCollection = useCollectionStore((s) => s.deleteCollection);
  const batchInstallCollection = useCollectionStore((s) => s.batchInstallCollection);
  const addSkillToCollection = useCollectionStore((s) => s.addSkillToCollection);

  const refreshCounts = usePlatformStore((s) => s.refreshCounts);

  // Resource library skills (for resolving SkillWithLinks before opening InstallDialog)
  const resourceLibraryDir = useResourceLibraryStore((s) => s.resourceLibraryDir);
  const resourceSkills = useResourceLibraryStore((s) => s.skills);
  const resourceAgents = useResourceLibraryStore((s) => s.agents);
  const loadResourceLibrary = useResourceLibraryStore((s) => s.loadResourceLibrary);
  const installResourceSkill = useResourceLibraryStore((s) => s.installSkill);
  const togglePlatformLink = useResourceLibraryStore((s) => s.togglePlatformLink);
  const addToCentral = useResourceLibraryStore((s) => s.addToCentral);
  const removeFromCentral = useResourceLibraryStore((s) => s.removeFromCentral);

  // Restoration context supplied via navigation state when returning from a
  // skill detail. `collectionContext` identifies the collection we should
  // re-focus; `scrollRestoration` carries the prior skill-list scroll offset.
  //
  // React Router preserves `location.state` across `navigate(-1)` only when
  // the previous history entry was originally pushed *with* state. Entering
  // /collections from the sidebar has no state, so on back-navigation we
  // also consult the in-memory return-context map populated by the forward
  // link. Consuming the fallback map is done lazily (inside useState's
  // initializer) so it fires exactly once on mount.
  const locationCollectionContext = location.state?.collectionContext as
    | { collectionId?: string }
    | undefined;
  const locationRestorationState = location.state?.scrollRestoration as
    | { key?: string; scrollTop?: number }
    | undefined;

  // Local state. When a collection context was restored from navigation state
  // (either via location.state on entry, or via the in-memory return-context
  // map on back-navigation) we seed `selectedId` with it so the
  // auto-select-first-collection effect below does not override the user's
  // prior focus while data is loading.
  const [selectedId, setSelectedId] = useState<string | null>(() => {
    if (locationCollectionContext?.collectionId) {
      return locationCollectionContext.collectionId;
    }
    const fallback = consumeReturnContext(COLLECTIONS_RETURN_SCOPE);
    if (fallback && typeof fallback.collectionId === "string") {
      return fallback.collectionId;
    }
    return null;
  });
  // Synthesise a scroll-restoration entry from the initial selectedId so the
  // restoration effect below can pull the saved offset out of the in-memory
  // scroll map even when the back-navigation replays a state-less /collections
  // history entry.
  const [fallbackRestorationKey] = useState<string | null>(() =>
    selectedId && !locationRestorationState
      ? collectionScrollKey(selectedId)
      : null
  );
  const restorationState: { key?: string; scrollTop?: number } | undefined =
    locationRestorationState ??
    (fallbackRestorationKey ? { key: fallbackRestorationKey } : undefined);
  const [isEditorOpen, setIsEditorOpen] = useState(false);
  const [isEditOpen, setIsEditOpen] = useState(false);
  const [isPickerOpen, setIsPickerOpen] = useState(false);
  const [isInstallOpen, setIsInstallOpen] = useState(false);
  const [isDeleting, setIsDeleting] = useState(false);
  const [installTargetSkill, setInstallTargetSkill] = useState<SkillWithLinks | null>(null);
  const [isSingleInstallOpen, setIsSingleInstallOpen] = useState(false);
  const [drawerSkillId, setDrawerSkillId] = useState<string | null>(null);
  const [isDrawerOpen, setIsDrawerOpen] = useState(false);

  const skillsContainerRef = useRef<HTMLDivElement | null>(null);
  const detailButtonRefs = useRef<Record<string, HTMLButtonElement | null>>({});

  // Load collections on mount.
  useEffect(() => {
    loadCollections();
  }, [loadCollections]);

  // Auto-select first collection when none is selected.
  // If we previously seeded selectedId from location.state or the in-memory
  // return-context map, selectedId is already truthy on first render so this
  // effect becomes a no-op and the restored focus is preserved.
  useEffect(() => {
    if (!selectedId && collections.length > 0) {
      setSelectedId(collections[0].id);
    }
  }, [selectedId, collections]);

  // If the restored collection id no longer exists (e.g. the collection was
  // deleted in another tab while the user was reading a skill detail), fall
  // back to the first available collection so the user still sees something
  // useful instead of an empty shell.
  useEffect(() => {
    if (!selectedId || collections.length === 0) return;
    const stillExists = collections.some((c) => c.id === selectedId);
    if (!stillExists) {
      setSelectedId(collections[0].id);
    }
  }, [selectedId, collections]);

  // Load detail when selection changes.
  useEffect(() => {
    if (selectedId) {
      loadCollectionDetail(selectedId);
    }
  }, [selectedId, loadCollectionDetail]);

  // Ensure resource library skills are loaded so collection installs use the library source.
  useEffect(() => {
    if (resourceSkills.length === 0) {
      loadResourceLibrary();
    }
  }, [loadResourceLibrary, resourceSkills.length]);

  // Scroll restoration: once the collection detail for the currently
  // selected collection has finished hydrating, restore the previously
  // recorded skill-list scroll offset. We only run after the detail matches
  // our selected id so we never restore onto the wrong collection's list.
  //
  // We prefer the in-memory scroll map (populated by SkillDetail's back
  // handler on the real list → detail → back flow) and fall back to the
  // `scrollTop` that was packed directly into `location.state`, which
  // covers tests plus any host that does not preserve state across the
  // back navigation. Restoration remains stable when the collection's
  // membership has changed but the context is still valid — we do not
  // require an exact skill-count match, only a matching collection id.
  //
  // After a successful restore we clear the navigation state so that later
  // interactions (removing a skill, scrolling, switching collections and
  // returning) can't re-apply the stale offset.
  useEffect(() => {
    if (!selectedId) return;
    if (!currentDetail || currentDetail.id !== selectedId) return;
    if (!restorationState?.key) return;
    if (restorationState.key !== collectionScrollKey(selectedId)) return;
    const container = skillsContainerRef.current;
    if (!container) return;

    let scrollTop = consumeScrollPosition(restorationState.key);
    if (scrollTop === null && typeof restorationState.scrollTop === "number") {
      scrollTop = restorationState.scrollTop;
    }
    if (scrollTop === null) return;

    container.scrollTop = scrollTop;
    navigate(location.pathname, { replace: true, state: null });
  }, [
    selectedId,
    currentDetail,
    restorationState?.key,
    restorationState?.scrollTop,
    navigate,
    location.pathname,
  ]);

  const collectionSkillsWithLinks = useMemo(
    () =>
      (currentDetail?.id === selectedId ? currentDetail.skills : []).map((skill): SkillWithLinks => {
        const resourceSkill = resourceSkills.find((candidate) => candidate.id === skill.id);
        return {
          ...resourceSkill,
          ...skill,
          canonical_path: skill.canonical_path ?? resourceSkill?.canonical_path,
          source: skill.source ?? resourceSkill?.source,
          source_url: skill.source_url ?? resourceSkill?.source_url,
          source_author: skill.source_author ?? resourceSkill?.source_author,
          source_repo: skill.source_repo ?? resourceSkill?.source_repo,
          source_path: skill.source_path ?? resourceSkill?.source_path,
          notes: resourceSkill ? resourceSkill.notes : skill.notes,
          tags: resourceSkill ? resourceSkill.tags : skill.tags,
          is_central: resourceSkill?.is_central ?? skill.is_central,
          linked_agents: resourceSkill?.linked_agents ?? [],
          read_only_agents: resourceSkill?.read_only_agents ?? [],
        };
      }),
    [currentDetail, selectedId, resourceSkills]
  );

  // ── Handlers ───────────────────────────────────────────────────────────────



  function setDetailButtonRef(skillId: string, node: HTMLButtonElement | null) {
    detailButtonRefs.current[skillId] = node;
  }

  function handleOpenDrawer(skillId: string) {
    setDrawerSkillId(skillId);
    setIsDrawerOpen(true);
  }

  function handleInstallSingleSkillClick(skillId: string) {
    const targetFromResource = resourceSkills.find((s) => s.id === skillId);
    const targetFromCollection = currentDetail?.skills.find((s) => s.id === skillId);
    const target: SkillWithLinks | null =
      targetFromResource ??
      (targetFromCollection
        ? {
            ...targetFromCollection,
            linked_agents: [],
            read_only_agents: [],
          }
        : null);
    if (!target) {
      toast.error(t("central.installError", { error: t("platform.notFound") }));
      return;
    }
    setInstallTargetSkill(target);
    setIsSingleInstallOpen(true);
  }

  async function handleInstallSingleSkill(skillId: string, agentIds: string[], method: string) {
    try {
      const result = await installResourceSkill(skillId, agentIds, method);
      await refreshCounts();
      if (result.failed.length > 0) {
        const failedNames = result.failed.map((f) => f.agent_id).join(", ");
        toast.error(t("central.installPartialFail", { platforms: failedNames }));
      }
    } catch (err) {
      toast.error(t("central.installError", { error: String(err) }));
    }
  }

  async function handleAddSkillToCentral(skillId: string) {
    try {
      await addToCentral(skillId);
      await refreshCounts();
    } catch (err) {
      toast.error(t("resource.addToCentralError", { error: String(err) }));
    }
  }

  async function handleRemoveSkillFromCentral(skillId: string) {
    try {
      await removeFromCentral(skillId);
      await refreshCounts();
    } catch (err) {
      toast.error(t("resource.removeFromCentralError", { error: String(err) }));
    }
  }

  async function handleUninstallSkillFromTargets(skill: SkillWithLinks) {
    try {
      for (const agentId of skill.linked_agents) {
        await togglePlatformLink(skill.id, agentId);
      }
      await refreshCounts();
    } catch (err) {
      toast.error(t("detail.uninstallError", { error: String(err) }));
    }
  }

  async function handleRemoveSkill(skillId: string) {
    if (!selectedId) return;
    try {
      await removeSkillFromCollection(selectedId, skillId);
    } catch (err) {
      toast.error(t("collection.removeSkillError", { error: String(err) }));
    }
  }

  async function handleDelete(id: string, name: string) {
    if (!window.confirm(t("collection.deleteConfirm", { name }))) return;
    setIsDeleting(true);
    try {
      await deleteCollection(id);
      setSelectedId(null);
    } catch (err) {
      toast.error(t("collection.deleteError", { error: String(err) }));
    } finally {
      setIsDeleting(false);
    }
  }

  async function handleAddSkills(skillIds: string[]) {
    if (!selectedId) return;
    try {
      for (const skillId of skillIds) {
        await addSkillToCollection(selectedId, skillId);
      }
    } catch (err) {
      toast.error(t("collection.addSkillError", { error: String(err) }));
    }
  }

  async function handleRefresh() {
    try {
      await Promise.all([loadCollections(), loadResourceLibrary(), refreshCounts()]);
      if (selectedId) {
        await loadCollectionDetail(selectedId);
      }
    } catch (err) {
      toast.error(t("collection.refreshError", { error: String(err) }));
    }
  }

  const tagLoadError = t("collection.refreshError", {error:t("common.error")});
  const collectionIndexKey = JSON.stringify(collections.map(collection => [collection.id, collection.updated_at]));
  useEffect(() => {
    let active = true;
    const entries = JSON.parse(collectionIndexKey) as [string, string][];
    void getCollectionDetails(entries.map(([id]) => id)).then(details => {
      if (active) setCollectionDetails(details);
    }).catch(() => { if (active) toast.error(tagLoadError); });
    return () => { active = false; };
  }, [collectionIndexKey, tagLoadError]);
  const collectionTags = useMemo(() => {
    const details = new Map(collectionDetails.map(detail => [detail.id, detail]));
    if (currentDetail) details.set(currentDetail.id, currentDetail);
    const resources = new Map(resourceSkills.map(skill => [skill.id, skill]));
    return new Map([...details].map(([id, detail]) => [id, [...new Set(detail.skills.flatMap(skill =>
      resources.get(skill.id)?.tags ?? skill.tags ?? []).map(tag => tag.trim()).filter(Boolean))]]));
  }, [collectionDetails, currentDetail, resourceSkills]);
  const availableTags = useMemo(() => [...new Map(collections.flatMap(collection =>
    (collectionTags.get(collection.id) ?? []).map(tag => [tag.toLowerCase(), tag] as const)))].sort((a,b) => a[1].localeCompare(b[1])), [collections, collectionTags]);

  // ── Render ─────────────────────────────────────────────────────────────────

  return (
    <div className="flex flex-col h-full">
      <SidebarTagFilter hasSelection={selectedTags.length > 0} onClear={() => setSelectedTags([])}><TagFilters tags={availableTags.map(([key,label])=>({key,label}))} selected={selectedTags} onChange={setSelectedTags}/></SidebarTagFilter>
      <SkillBrowserWorkspace
        storageKey="collections" collectionsMode loading={isLoading || isLoadingDetail} onRefresh={handleRefresh}
        sortField={sortField} sortDirection={sortDirection}
        onSortChange={(field, direction) => { setSortField(field); setSortDirection(direction); }}
        toolbar={viewControl => <SkillBrowserHeader viewControl={viewControl} actions={<Button variant="ghost" size="icon" title={t("sidebar.newCollectionLabel")} aria-label={t("sidebar.newCollectionLabel")} onClick={() => setIsEditorOpen(true)}><Plus className="size-4" /></Button>} title={<>
          <h1>{t("sidebar.collections")}</h1>
          <Button variant="ghost" size="icon" title={t("collection.refresh")} aria-label={t("collection.refresh")} onClick={() => void handleRefresh()} disabled={isLoading || isLoadingDetail}><RotateCw className={cn("size-4", isLoading && "animate-spin")} /></Button>
          <OpenableDirectoryPath iconOnly path={resourceLibraryDir ?? ""} />
        </>} search={<div className="flex items-center gap-2"><SearchInput containerClassName="min-w-0 flex-1" value={search} onValueChange={setSearch} placeholder={t("collection.searchCollections")} trailing={<SearchScopes value={searchScopes} onChange={setSearchScopes}/>} /></div>} />}
        folders={collections.filter(collection => (matchesSearch({name:collection.name,description:collection.description},search,searchScopes,collection.name) || (collectionDetails.find(detail=>detail.id===collection.id)?.skills ?? []).some(skill=>matchesSearch(resourceSkills.find(s=>s.id===skill.id)??skill,search,searchScopes,collection.name)))
          && (!selectedTags.length || (collectionDetails.find(detail=>detail.id===collection.id)?.skills ?? []).some(skill=>matchesTags(resourceSkills.find(s=>s.id===skill.id)?.tags??skill.tags,selectedTags)))).map(collection => ({
          batchOperations: {
            install: (targets: string[]) => batchInstallCollection(collection.id,targets),
            delete: () => deleteCollection(collection.id),
          },
          installAgents: resourceAgents,
          key: collection.id, name: collection.name, path: "", expandable: false,
          tags: collectionTags.get(collection.id),
          skillCount: currentDetail?.id === collection.id ? currentDetail.skills.length : 0,
          skillKeys: currentDetail?.id === collection.id ? currentDetail.skills.map(skill => skill.id) : [],
          createdAt: collection.created_at, updatedAt: collection.updated_at,
          highlighted: selectedId === collection.id,
          installSummaryMembers: currentDetail?.id === collection.id ? collectionSkillsWithLinks : [],
          skillListLoading: selectedId === collection.id && (isLoadingDetail || currentDetail?.id !== collection.id),
          onOpen: () => {}, onSelect: () => setSelectedId(collection.id),
          onEdit: () => { setSelectedId(collection.id); setIsEditOpen(true); },
          onAddSkills: () => { setSelectedId(collection.id); setIsPickerOpen(true); },
          onInstall: () => { setSelectedId(collection.id); setIsInstallOpen(true); },
          installLabel: t("collection.batchInstall"),
          onDelete: () => void handleDelete(collection.id, collection.name),
          deleteLabel: t("collection.delete"), deleteRequiresConfirmation: false, isDeleting,
          metadata: <>
            {collection.description && <p className="text-sm whitespace-pre-wrap">{collection.description}</p>}
          </>,
        }))}
                          skills={sortBySkillBrowserOrder(collectionSkillsWithLinks.filter(skill=>matchesTags(skill.tags,selectedTags)&&matchesSearch(skill,search,searchScopes,currentDetail?.name)), sortField, sortDirection).map((skill) => ({
                            rowKey: skill.id,
                            batchOperations: {
                              install: async (targets:string[]) => { const result=await installResourceSkill(skill.id,targets,"auto"); await refreshCounts(); return result; },
                              delete: selectedId ? () => removeSkillFromCollection(selectedId,skill.id) : undefined,
                            },
                  onLocate: () => navigate(repositoryLocationUrl(skill, false)),
                            name: skill.name,
                            description: skill.description,
                            notes: skill.notes,
                            sourceAuthor: skill.source_author,
                            sourceRepo: skill.source_repo,
                            sourceUrl: skill.source_url,
                            createdAt: skill.created_at,
                            updatedAt: skill.updated_at,
                            isCentral: skill.is_central,
                            tags: (skill.tags ?? []).map((tag) => ({
                              key: tag,
                              label: tag,
                            })),
                            installAgents: resourceAgents,
                            installSummaryMembers: [skill],
                            installLinkedAgentIds: skill.linked_agents,
                            installReadOnlyAgentIds: skill.read_only_agents ?? [],
                            onDetail: () => handleOpenDrawer(skill.id),
                            detailButtonRef: (node) => setDetailButtonRef(skill.id, node),
                            onInstallTo: () => handleInstallSingleSkillClick(skill.id),
                            onUninstallFromPlatform:
                              skill.linked_agents.length > 0
                                ? () => void handleUninstallSkillFromTargets(skill)
                                : undefined,
                            onInstallToCentral: skill.is_central
                              ? undefined
                              : () => void handleAddSkillToCentral(skill.id),
                            onRemoveFromCentral: skill.is_central
                              ? () => void handleRemoveSkillFromCentral(skill.id)
                              : undefined,
                            onRemove: () => handleRemoveSkill(skill.id),
                            removeLabel: t("resource.deleteAction"),
                          }))}
      />

      {/* Dialogs */}
      <CollectionEditor open={isEditorOpen} onOpenChange={setIsEditorOpen} collection={null} />

      {currentDetail && currentDetail.id === selectedId && !isLoadingDetail && (
        <>
          <CollectionEditor open={isEditOpen} onOpenChange={setIsEditOpen} collection={{
            id: currentDetail.id,
            name: currentDetail.name,
            description: currentDetail.description,
            created_at: currentDetail.created_at,
            updated_at: currentDetail.updated_at,
          }} />
          <SkillPickerDialog
            open={isPickerOpen}
            onOpenChange={setIsPickerOpen}
            existingSkillIds={currentDetail.skills.map((s) => s.id)}
            onAdd={handleAddSkills}
          />
          <CollectionInstallDialog
            open={isInstallOpen}
            onOpenChange={setIsInstallOpen}
            collectionName={currentDetail.name}
            skillCount={currentDetail.skills.length}
            agents={resourceAgents}
            isCentral={collectionSkillsWithLinks.every((skill) => skill.is_central)}
            onInstall={(agentIds) => batchInstallCollection(currentDetail.id, agentIds)}
          />
        </>
      )}

      <InstallDialog
        open={isSingleInstallOpen}
        onOpenChange={setIsSingleInstallOpen}
        skill={installTargetSkill}
        agents={resourceAgents}
        onInstall={handleInstallSingleSkill}
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
    </div>
  );
}
