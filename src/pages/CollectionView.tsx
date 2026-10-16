import { ActionIcon } from "@/components/ui/action-icon";
import { SearchInput } from "@/components/ui/search-input";
import { SearchScopes } from "@/components/skill/SearchScopes";
import { useSearchScopes, matchesSearch, matchesTags } from "@/lib/skillFilters";
import { TagFilters } from "@/components/skill/TagFilters";
import { SidebarTagFilter } from "@/components/layout/SidebarTagFilter";
import { SkillBrowserHeader } from "@/components/skill/SkillBrowserHeader";
import { OpenableDirectoryPath } from "@/components/common/OpenableDirectoryPath";
import { SkillBrowserWorkspace } from "@/components/skill/SkillBrowserWorkspace";
import { repositoryLocationUrl } from "@/lib/skillNavigation";
import {
Loader2,
RotateCw,
PackagePlus,
Pencil,
Plus,
Trash2
} from "lucide-react";
import { useEffect,useMemo,useRef,useState } from "react";
import { useTranslation } from "react-i18next";
import { useLocation,useNavigate,useParams } from "react-router-dom";
import { toast } from "sonner";

import { InstallDialog } from "@/components/central/InstallDialog";
import { CollectionEditor } from "@/components/collection/CollectionEditor";
import { CollectionInstallDialog } from "@/components/collection/CollectionInstallDialog";
import { SkillPickerDialog } from "@/components/collection/SkillPickerDialog";
import { Button } from "@/components/ui/button";
import {
consumeScrollPosition,
createScrollRestorationState,
} from "@/lib/scrollRestoration";
import {
sortBySkillBrowserOrder,
type SkillSortDirection,
type SkillSortField,
} from "@/lib/skillSort";
import { useCollectionStore } from "@/stores/collectionStore";
import { usePlatformStore } from "@/stores/platformStore";
import { useResourceLibraryStore } from "@/stores/resourceLibraryStore";
import { SkillWithLinks } from "@/types";

// Scroll-restoration key shared with `CollectionsListView` so list-level and
// single-collection pages interoperate under the same restoration contract.
function collectionScrollKey(collectionId: string): string {
  return `collection:${collectionId}`;
}

// ─── CollectionView ───────────────────────────────────────────────────────────

export function CollectionView() {
  const { collectionId } = useParams<{ collectionId: string }>();
  const navigate = useNavigate();
  const location = useLocation();
  const { t } = useTranslation();

  const currentDetail = useCollectionStore((s) => s.currentDetail);
  const isLoadingDetail = useCollectionStore((s) => s.isLoadingDetail);
  const error = useCollectionStore((s) => s.error);
  const loadCollectionDetail = useCollectionStore((s) => s.loadCollectionDetail);
  const removeSkillFromCollection = useCollectionStore((s) => s.removeSkillFromCollection);
  const deleteCollection = useCollectionStore((s) => s.deleteCollection);
  const batchInstallCollection = useCollectionStore((s) => s.batchInstallCollection);
  const addSkillToCollection = useCollectionStore((s) => s.addSkillToCollection);

  const refreshCounts = usePlatformStore((s) => s.refreshCounts);

  const resourceLibraryDir = useResourceLibraryStore(s => s.resourceLibraryDir);
  const resourceSkills = useResourceLibraryStore((s) => s.skills);
  const resourceAgents = useResourceLibraryStore((s) => s.agents);
  const togglingAgentId = useResourceLibraryStore((s) => s.togglingAgentId);
  const loadResourceLibrary = useResourceLibraryStore((s) => s.loadResourceLibrary);
  const installResourceSkill = useResourceLibraryStore((s) => s.installSkill);
  const togglePlatformLink = useResourceLibraryStore((s) => s.togglePlatformLink);
  const addToCentral = useResourceLibraryStore((s) => s.addToCentral);
  const removeFromCentral = useResourceLibraryStore((s) => s.removeFromCentral);

  // Dialog open states.
  const [isEditorOpen, setIsEditorOpen] = useState(false);
  const [isNewEditorOpen, setIsNewEditorOpen] = useState(false);
  const [isPickerOpen, setIsPickerOpen] = useState(false);
  const [isInstallOpen, setIsInstallOpen] = useState(false);
  const [isDeleting, setIsDeleting] = useState(false);
  const [deleteError, setDeleteError] = useState<string | null>(null);
  const [installTargetSkill, setInstallTargetSkill] = useState<SkillWithLinks | null>(null);
  const [isSingleInstallOpen, setIsSingleInstallOpen] = useState(false);
  const [sortField, setSortField] = useState<SkillSortField>("name");
  const [sortDirection, setSortDirection] = useState<SkillSortDirection>("asc");

  const [search,setSearch]=useState("");
  const [searchScopes,setSearchScopes]=useSearchScopes();
  const [selectedTags,setSelectedTags]=useState<string[]>([]);
  const skillsContainerRef = useRef<HTMLDivElement | null>(null);

  // Restoration state carried through navigation when returning from a skill
  // detail. The context is already present in the URL (collectionId), but the
  // scroll offset needs to be re-applied after data hydrates.
  //
  // React Router preserves `location.state` across `navigate(-1)` only when
  // the previous history entry was pushed *with* state. Entering
  // /collection/:id from the sidebar or list view has no state, so on
  // back-navigation we also rely on the in-memory scroll map, synthesising a
  // restoration entry keyed on the current collectionId.
  const locationRestorationState = location.state?.scrollRestoration as
    | { key?: string; scrollTop?: number }
    | undefined;
  const restorationState: { key?: string; scrollTop?: number } | undefined =
    locationRestorationState ??
    (collectionId ? { key: collectionScrollKey(collectionId) } : undefined);

  // Load collection detail on mount and when collectionId changes.
  useEffect(() => {
    if (collectionId) {
      loadCollectionDetail(collectionId);
    }
  }, [collectionId, loadCollectionDetail]);

  // Ensure resource library skills are loaded so collection installs use the library source.
  useEffect(() => {
    if (resourceSkills.length === 0) {
      loadResourceLibrary();
    }
  }, [loadResourceLibrary, resourceSkills.length]);

  // Scroll restoration: once the collection detail for this route's
  // collectionId has finished hydrating, restore the previously recorded
  // skill-list scroll offset. We prefer the in-memory map populated by
  // SkillDetail's back handler, and fall back to the `scrollTop` packed into
  // `location.state` for tests/hosts that don't preserve state through back
  // navigation. After a successful restore we clear the navigation state so
  // that later interactions can't re-apply the stale offset.
  useEffect(() => {
    if (!collectionId) return;
    if (!currentDetail || currentDetail.id !== collectionId) return;
    if (!restorationState?.key) return;
    if (restorationState.key !== collectionScrollKey(collectionId)) return;
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
    collectionId,
    currentDetail,
    restorationState?.key,
    restorationState?.scrollTop,
    navigate,
    location.pathname,
  ]);

  const collectionSkillsWithLinks = useMemo(
    () =>
      (currentDetail?.skills ?? []).map((skill): SkillWithLinks => {
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
    [currentDetail?.skills, resourceSkills]
  );
  const sortedCollectionSkills = useMemo(
    () => sortBySkillBrowserOrder(collectionSkillsWithLinks.filter(skill=>matchesTags(skill.tags,selectedTags)&&matchesSearch(skill,search,searchScopes,skill.source_repo??"")), sortField, sortDirection),
    [collectionSkillsWithLinks, sortDirection, sortField, selectedTags, search, searchScopes]
  );

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

  async function handleTogglePlatform(skillId: string, agentId: string) {
    try {
      await togglePlatformLink(skillId, agentId);
      await refreshCounts();
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
    if (!collectionId) return;
    try {
      await removeSkillFromCollection(collectionId, skillId);
    } catch (err) {
      toast.error(t("collection.removeSkillError", { error: String(err) }));
    }
  }

  async function handleDelete() {
    if (!collectionId || !currentDetail) return;
    if (!window.confirm(t("collection.deleteConfirm", { name: currentDetail.name }))) {
      return;
    }
    setIsDeleting(true);
    setDeleteError(null);
    try {
      await deleteCollection(collectionId);
      navigate("/central");
    } catch (err) {
      setDeleteError(String(err));
      toast.error(t("collection.deleteError", { error: String(err) }));
    } finally {
      setIsDeleting(false);
    }
  }

  async function handleAddSkills(skillIds: string[]) {
    if (!collectionId) return;
    try {
      // Add skills sequentially.
      for (const skillId of skillIds) {
        await addSkillToCollection(collectionId, skillId);
      }
    } catch (err) {
      toast.error(t("collection.addSkillError", { error: String(err) }));
    }
  }

  // ── Loading ──────────────────────────────────────────────────────────────

  if (isLoadingDetail) {
    return (
      <div className="flex items-center justify-center h-full gap-3 text-muted-foreground">
        <Loader2 className="size-4 animate-spin" />
        <span className="text-sm">{t("collection.loading")}</span>
      </div>
    );
  }

  // ── Error ────────────────────────────────────────────────────────────────

  if (error && !currentDetail) {
    return (
      <div className="flex flex-col items-center justify-center h-full gap-3 text-muted-foreground">
        <p className="text-sm text-destructive">{error}</p>
        <Button variant="outline" size="sm" onClick={() => navigate(-1)}><ActionIcon action="back"/>
          {t("collection.goBack")}
        </Button>
      </div>
    );
  }

  if (!currentDetail) {
    return null;
  }

  // ── Main View ────────────────────────────────────────────────────────────

  return (
    <div className="flex flex-col h-full">
      <SidebarTagFilter hasSelection={selectedTags.length > 0} onClear={() => setSelectedTags([])}><TagFilters tags={[...new Map(collectionSkillsWithLinks.flatMap(s=>(s.tags??[]).map(tag=>[tag.toLowerCase(),tag])))] .map(([key,label])=>({key,label}))} selected={selectedTags} onChange={setSelectedTags}/></SidebarTagFilter>

      {/* Header */}
      <div className="border-b border-border px-6 py-4">
        <div className="flex items-start justify-between gap-4">
          <div className="min-w-0 flex-1">
            <h1 className="text-sm font-semibold truncate">
              {currentDetail.name}
              <span className="font-normal text-muted-foreground">
                {" "}· {currentDetail.skills.length}
              </span>
            </h1>
            {currentDetail.description && (
              <p className="text-sm text-muted-foreground mt-0.5">
                {currentDetail.description}
              </p>
            )}
          </div>

          <div className="flex items-center gap-2 shrink-0">
            <Button
              variant="outline"
              size="sm"
              onClick={() => setIsNewEditorOpen(true)}
            >
              <Plus className="size-4" />
              <span>{t("sidebar.newCollectionLabel")}</span>
            </Button>
            <Button
              variant="outline"
              size="sm"
              onClick={() => setIsEditorOpen(true)}
              aria-label={t("collection.editLabel")}
            >
              <Pencil className="size-4" />
              <span>{t("collection.edit")}</span>
            </Button>
            <Button
              variant="outline"
              size="sm"
              onClick={handleDelete}
              disabled={isDeleting}
              aria-label={t("collection.deleteLabel")}
              className="text-destructive hover:text-destructive hover:bg-destructive/10 border-destructive/30"
            >
              {isDeleting ? (
                <Loader2 className="size-4 animate-spin" />
              ) : (
                <Trash2 className="size-4" />
              )}
              <span>{t("collection.delete")}</span>
            </Button>
            <Button
              variant="outline"
              size="sm"
              onClick={() => setIsInstallOpen(true)}
              disabled={currentDetail.skills.length === 0}
              aria-label={t("collection.batchInstallLabel")}
            >
              <PackagePlus className="size-4" />
              <span>{t("collection.batchInstall")}</span>
            </Button>
            <Button
              variant="default"
              size="sm"
              onClick={() => setIsPickerOpen(true)}
              aria-label={t("collection.addSkillLabel")}
            >
              <Plus className="size-4" />
              <span>{t("collection.addSkill")}</span>
            </Button>
          </div>
        </div>

        {deleteError && (
          <p className="text-xs text-destructive mt-2" role="alert">
            {deleteError}
          </p>
        )}
      </div>

      {/* Skills list */}
      <SkillBrowserWorkspace storageKey="CollectionView" searchActive={false} loading={isLoadingDetail}
        onRefresh={() => Promise.all([loadCollectionDetail(currentDetail.id), refreshCounts()])}
        toolbar={viewControl => <SkillBrowserHeader viewControl={viewControl} title={<>
          <Button variant="ghost" size="icon" aria-label={t("collection.refresh")} disabled={isLoadingDetail} onClick={() => void Promise.all([loadCollectionDetail(currentDetail.id), refreshCounts()])}><RotateCw className="size-4"/></Button>
          <OpenableDirectoryPath iconOnly path={resourceLibraryDir ?? ""}/>
        </>} search={<SearchInput containerClassName="min-w-0 flex-1" value={search} onValueChange={setSearch} placeholder={t("resource.searchPlaceholder")} trailing={<SearchScopes value={searchScopes} onChange={setSearchScopes}/>} />}/>}

                sortField={sortField}
                sortDirection={sortDirection}
                onSortChange={(field, direction) => {
                  setSortField(field);
                  setSortDirection(direction);
                }}
                skills={sortedCollectionSkills.map((skill) => ({
                  rowKey: skill.id,
 detailRequest: { skillId: skill.id },
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
                  tags: (skill.tags ?? []).map((tag) => ({ key: tag, label: tag })),
                  onDetail: () =>
                    navigate(`/skill/${skill.id}`, {
                      state: {
                        collectionContext: {
                          collectionId: currentDetail.id,
                        },
                        scrollRestoration: createScrollRestorationState(
                          collectionScrollKey(currentDetail.id),
                          skillsContainerRef.current?.scrollTop ?? 0
                        ),
                      },
                    }),
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
                  installAgents: resourceAgents,
                  installLinkedAgentIds: skill.linked_agents,
                  installReadOnlyAgentIds: skill.read_only_agents ?? [],
                  platformIcons: {
                    agents: resourceAgents,
                    linkedAgents: skill.linked_agents,
                    readOnlyAgents: skill.read_only_agents ?? [],
                    skillId: skill.id,
                    onToggle: handleTogglePlatform,
                    togglingAgentId,
                  },
                }))}
              />

      {/* Dialogs */}
      <CollectionEditor
        open={isEditorOpen}
        onOpenChange={setIsEditorOpen}
        collection={{
          id: currentDetail.id,
          name: currentDetail.name,
          description: currentDetail.description,
          created_at: currentDetail.created_at,
          updated_at: currentDetail.updated_at,
        }}
      />

      <SkillPickerDialog
        open={isPickerOpen}
        onOpenChange={setIsPickerOpen}
        existingSkillIds={currentDetail.skills.map((s) => s.id)}
        onAdd={handleAddSkills}
      />

      <CollectionEditor
        open={isNewEditorOpen}
        onOpenChange={setIsNewEditorOpen}
        collection={null}
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

      <InstallDialog
        open={isSingleInstallOpen}
        onOpenChange={setIsSingleInstallOpen}
        skill={installTargetSkill}
        agents={resourceAgents}
        onInstall={handleInstallSingleSkill}
      />

    </div>
  );
}
