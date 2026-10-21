import {StatusBarSlot} from "@/components/layout/StatusBarSlot";
import { useNotebookStore } from "@/stores/notebookStore";
import { RepositoryNotebook } from "@/components/notebook/RepositoryNotebook";
import { RepositoryDocument } from "./RepositoryDocument";
import { useRepositoryDocumentStore } from "@/stores/repositoryDocumentStore";
import { ActionIcon } from "@/components/ui/action-icon";
import { useBrowserStatusStore } from "@/stores/browserStatusStore";
import { Link, useLocation } from "react-router-dom";
import { buildMembershipInstallSummary } from "@/lib/installSummary";
import { useExpansionShortcuts } from "@/hooks/useExpansionShortcuts";
import { useConfiguredHotkey } from "@/hooks/useConfiguredHotkey";
import { useContext, useEffect, useLayoutEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { useTranslation } from "react-i18next";
import { PanelRight, FolderOpen, List, FolderTree, Info, ChevronRight } from "lucide-react";
import { SkillBrowserTable, InstallSummaryCell, type SkillBrowserTableProps, type FolderTableItem, type SkillTableItem } from "./SkillBrowserTable";
import { SkillDetailView, MetadataRow } from "./SkillDetailView";
import { skillSourceLinks } from "@/lib/skillSourceLinks";
import { Button } from "@/components/ui/button";
import { useSkillTableColumns } from "@/hooks/useSkillTableColumns";
import { cn } from "@/lib/utils";
import { ICON_RAIL_WIDTH, ICON_RAIL_BUTTON } from "@/components/layout/iconRail";

type Tab = "overview" | "document" | "notebook";
type Props = Omit<SkillBrowserTableProps, "kind" | "visibleColumns"> & { storageKey: string; searchActive?: boolean; agentId?: string; loading?: boolean; toolbar?: ReactNode | ((viewControl: ReactNode) => ReactNode); onRefresh?: () => void | Promise<unknown>; collectionsMode?: boolean; allowFlatView?: boolean; forceFlat?: boolean };
function readPreference<T>(key: string, fallback: T): T {
  try { return JSON.parse(localStorage.getItem(key) ?? "null") ?? fallback; } catch { return fallback; }
}
function readExpanded(key: string): Set<string> {
  const saved = readPreference<unknown>(key, []);
  return new Set(Array.isArray(saved) ? saved.filter((value): value is string => typeof value === "string") : []);
}
function readWidth(): number {
  const saved = readPreference<unknown>("skillshub.browser.width", 45);
  return typeof saved === "number" && Number.isFinite(saved) ? Math.min(70, Math.max(30, saved)) : 45;
}
function writePreference(key: string, value: unknown) { try { localStorage.setItem(key, JSON.stringify(value)); } catch { /* Keep in-memory state. */ } }

export function SkillBrowserWorkspace({ storageKey, skills = [], folders = [], searchActive = false, agentId, loading = false, toolbar, collectionsMode = false, allowFlatView = true, forceFlat = false, onRefresh, ...tableProps }: Props) {
  const { t } = useTranslation();
  const statusSlot = useContext(StatusBarSlot);
  const { pathname } = useLocation();
  const notebooks = useNotebookStore(state => state.rows);
  useEffect(() => { void useNotebookStore.getState().load(); }, []);
  const key = `skillshub.browser.${storageKey}.${agentId ?? ""}`;
  const [flatPreference, setFlatPreference] = useState(() => readPreference(`${key}.flat`, false));
  const flat = forceFlat || (allowFlatView && flatPreference);
  const scrollPositions = useRef<{key: string; tree?: number; flat?: number}>({key});
  const pendingScroll = useRef<number | null>(null);
  const switchView = () => {
    if (scrollPositions.current.key !== key) scrollPositions.current = {key};
    const currentTop = listScroll.current?.scrollTop ?? 0;
    scrollPositions.current[flat ? "flat" : "tree"] = currentTop;
    pendingScroll.current = scrollPositions.current[flat ? "tree" : "flat"] ?? currentTop;
    setFlatPreference(!flat);
    writePreference(`${key}.flat`, !flat);
  };
  const { visibleColumns, toggleColumn, resetColumns } = useSkillTableColumns(flat ? "flat" : "tree");
  const [expanded, setExpanded] = useState<Set<string>>(() => readExpanded(`${key}.expanded`));
  const [selectedRowKeys, setSelectedRowKeys] = useState<string[]>([]);
  const [selected, setSelected] = useState<{ kind: "skill" | "folder"; key: string } | null>(null);
  const [tab, setTab] = useState<Tab>(() => {
    const saved = readPreference<string>("skillshub.browser.tab", "overview");
    return ["overview", "document", "notebook"].includes(saved) ? saved as Tab : "overview";
  });
  useEffect(() => {
    const open = () => setTab("notebook");
    window.addEventListener("skillshub-open-notebook", open);
    return () => window.removeEventListener("skillshub-open-notebook", open);
  }, []);
  const [width, setWidth] = useState(readWidth);
  const [collapsed, updateCollapsed] = useState(()=>readPreference("skillshub.browser.collapsed",false));
  const setCollapsed = (value:boolean) => {updateCollapsed(value);writePreference("skillshub.browser.collapsed",value);};
  const root = useRef<HTMLDivElement>(null);
  const listScroll = useRef<HTMLDivElement>(null);
  useLayoutEffect(() => {
    if (pendingScroll.current !== null && listScroll.current) {
      listScroll.current.scrollTop = pendingScroll.current;
      pendingScroll.current = null;
    }
  }, [flat]);
  const [scrollbarWidth, setScrollbarWidth] = useState(0);
  useLayoutEffect(() => {
    const element = listScroll.current;
    if (!element) return;
    const header = element.querySelector("thead");
    const measure = () => {
      setScrollbarWidth(Math.max(0, element.offsetWidth - element.clientWidth));
      element.style.setProperty("--skill-table-header-height", `${header ? header.getBoundingClientRect().height + 2 : 0}px`);
    };
    measure();
    if (typeof ResizeObserver === "undefined") return;
    const observer = new ResizeObserver(measure);
    observer.observe(element);
    if (header) observer.observe(header);
    return () => observer.disconnect();
  }, [flat, loading, skills.length, folders.length]);
  useEffect(() => { setFlatPreference(readPreference(`${key}.flat`, false)); setExpanded(readExpanded(`${key}.expanded`)); setSelected(null); setSelectedRowKeys([]); if (listScroll.current) listScroll.current.scrollTop = 0; }, [key]);
  const id = (skill: SkillTableItem) => skill.rowKey ?? skill.name;
  const groups = useMemo(() => {
    const assigned = new Set<string>();
    const results: FolderTableItem[] = folders.map(folder => {
      const keys = new Set(folder.skillKeys ?? []);
      const children = skills.filter(skill => keys.has(id(skill)));
      children.forEach(skill => assigned.add(id(skill)));
      return { ...folder, children };
    }).filter(folder => !searchActive || folder.children!.length > 0);
    const ungrouped = new Map<string, SkillTableItem[]>();
    skills.filter(skill => !collectionsMode && !assigned.has(id(skill))).forEach(skill => {
      const repo = skill.sourceRepo ?? t("browser.localSkills");
      const group = ungrouped.get(repo) ?? []; group.push(skill); ungrouped.set(repo, group);
    });
    for (const [name, children] of ungrouped) results.push({ key: `source:${name}`, name, path: "", skillCount: children.length, createdAt: children.map(s => s.createdAt).filter(Boolean).sort().at(-1), updatedAt: children.map(s => s.updatedAt).filter(Boolean).sort().at(-1), children, onOpen: () => {}, installAgents: children[0]?.installAgents, installSummaryMembers: children.flatMap(s => s.installSummaryMembers ?? [{ id: id(s), is_central: s.isCentral ?? false, linked_agents: s.installLinkedAgentIds ?? [], read_only_agents: s.installReadOnlyAgentIds ?? [] }]) });
    // Input skill rows are already sorted by each page; apply the same ordering to groups.
    const multiplier = tableProps.sortDirection === "desc" ? -1 : 1;
    return results.sort((a,b) => {
      const field = tableProps.sortField;
      if (field === "githubStars") {
        if (a.githubStars == null || b.githubStars == null) return a.githubStars == null ? (b.githubStars == null ? a.name.localeCompare(b.name) : 1) : -1;
        return (a.githubStars - b.githubStars) * multiplier;
      }
      if (field === "skillCount") return (a.skillCount - b.skillCount) * multiplier;
      if (field === "createdAt" || field === "updatedAt") return ((Date.parse(a[field] ?? "") || 0) - (Date.parse(b[field] ?? "") || 0)) * multiplier;
      return a.name.localeCompare(b.name, undefined, { numeric: true, sensitivity: "base" }) * multiplier;
    });
  }, [folders, skills, collectionsMode, searchActive, t, tableProps.sortDirection, tableProps.sortField]);
  useEffect(() => {
    if (!groups.length) useBrowserStatusStore.getState().setStats({path:pathname,collections:collectionsMode,groups:0,skills:0,selected:0,selectedGroups:0,installed:0});
  }, [pathname,collectionsMode,groups.length]);
  useConfiguredHotkey("toggleSkillViewMode", () => {
    if (!document.querySelector('[role="dialog"]')) switchView();
  }, {enabled: allowFlatView, allowInEditable: true});
  useConfiguredHotkey("refreshSkillList", () => {
    if (!loading && !document.querySelector('[role="dialog"]')) void onRefresh?.();
  }, {enabled: !!onRefresh, allowInEditable: true});
  const expansionKeys = useExpansionShortcuts(
    () => { const next = new Set(groups.map(group => group.key)); setExpanded(next); writePreference(`${key}.expanded`, [...next]); },
    () => { setExpanded(new Set()); writePreference(`${key}.expanded`, []); },
  );
  const selectedSkill = selected?.kind === "skill" ? skills.find(s => id(s) === selected.key) : undefined;
  const selectedFolder = selected?.kind === "folder" ? groups.find(g => g.key === selected.key) : undefined;
  const canUseNotebook = (folder?: FolderTableItem) => !collectionsMode && !!folder && Boolean(folder.path || folder.sourceRepo || folder.children?.some(child => child.sourceRepo));
  const activeTab = (tab === "notebook" && !canUseNotebook(selectedFolder)) || (collectionsMode && !selectedSkill && tab === "document") ? "overview" : tab;
  const selectionTags = useMemo(() => {
    const keys = new Set(selectedRowKeys);
    return [
      ...groups.filter(folder => keys.has(`folder:${folder.key}`)).flatMap(folder =>
        folder.tags ?? folder.children?.flatMap(skill => skill.tags?.map(tag => tag.label) ?? []) ?? []),
      ...skills.filter(skill => keys.has(`skill:${id(skill)}`)).flatMap(skill => skill.tags?.map(tag => tag.label) ?? []),
    ];
  }, [selectedRowKeys, groups, skills]);
  useEffect(() => {
    useRepositoryDocumentStore.getState().setHighlightedTags(selectionTags);
    return () => useRepositoryDocumentStore.getState().setHighlightedTags([]);
  }, [selectionTags]);
  const highlightedSkillKey = skills.find(s => s.highlighted)?.rowKey;
  const highlightedFolderKey = folders.find(f => f.highlighted)?.key;
  useEffect(() => {
    if (highlightedSkillKey) { setSelectedRowKeys([`skill:${highlightedSkillKey}`]); setSelected({kind:"skill",key:highlightedSkillKey}); }
    else if (highlightedFolderKey) { setSelectedRowKeys([`folder:${highlightedFolderKey}`]); setSelected({kind:"folder",key:highlightedFolderKey}); }
  }, [highlightedSkillKey, highlightedFolderKey]);
  const chooseSkill = (skill: SkillTableItem) => { setSelectedRowKeys([`skill:${id(skill)}`]); setSelected({kind:"skill",key:id(skill)}); };
  const toggle = (folderKey: string) => setExpanded(previous => { const next = new Set(previous); if(next.has(folderKey)) next.delete(folderKey); else next.add(folderKey); writePreference(`${key}.expanded`, [...next]); return next; });
  const treeRows = groups.map(folder => ({ ...folder,
    hasNotebook: canUseNotebook(folder) && notebooks.some(note => (note.notebook.trim() || note.obsidian) && folder.children?.some(child => note.skillIds.includes(child.detailRequest?.skillId ?? child.rowKey ?? ""))),
    onNotebook: !canUseNotebook(folder) ? undefined : () => { setSelectedRowKeys([`folder:${folder.key}`]); setSelected({kind:"folder",key:folder.key}); setTab("notebook"); },
    highlighted: selectedFolder?.key === folder.key,
    scrollToRow: folder.highlighted,
    expanded: folder.expandable !== false && (searchActive || expanded.has(folder.key) || Boolean(folder.children?.some(s => s.highlighted))),
    onToggle: folder.expandable === false ? undefined : () => toggle(folder.key),
    onOpen: () => { setSelectedRowKeys([`folder:${folder.key}`]); setSelected({kind:"folder",key:folder.key}); folder.onSelect?.(); },
    children: folder.children?.map(skill => ({ ...skill, scrollToRow: skill.highlighted, highlighted: selectedSkill && id(selectedSkill) === id(skill), onDetail: () => chooseSkill(skill) })),
  }));
  const folderSources = [...new Map((selectedFolder?.children ?? []).filter(skill => skill.sourceRepo || skill.sourceUrl).map(skill => {
    const links = skillSourceLinks({source_repo:skill.sourceRepo, source_url:skill.sourceUrl});
    return [links.repository ?? skill.sourceRepo ?? skill.sourceUrl, {label:skill.sourceRepo ?? links.repository ?? skill.sourceUrl!, ...links}];
  })).values()];
  const request = selectedSkill?.detailRequest ?? (selectedSkill ? { skillId: id(selectedSkill), agentId } : undefined);
  const activeAgents = selectedSkill?.installAgents ?? selectedFolder?.installAgents ?? [];
  const members = selectedSkill ? selectedSkill.installSummaryMembers ?? [{id:id(selectedSkill),is_central:selectedSkill.isCentral ?? false,linked_agents:selectedSkill.installLinkedAgentIds ?? [],read_only_agents:selectedSkill.installReadOnlyAgentIds ?? []}] : selectedFolder?.installSummaryMembers ?? [];
  const summary = buildMembershipInstallSummary(members, activeAgents);
  const installationSummary = <section aria-label={t("skillBrowser.columns.installSummary")}>
    <details open className="group/overview-section">
      <summary className="overview-section-title"><ActionIcon action="install"/>{t("skillBrowser.columns.installSummary")}<ChevronRight className="size-[1em] shrink-0 group-open/overview-section:rotate-90" aria-hidden="true"/></summary>
      <div className="space-y-3 rounded-lg border border-border/70 bg-transparent p-3">
        <InstallSummaryCell agents={activeAgents} members={members}/>
        {[...summary.shared, ...summary.directPlatforms, ...summary.directProjects].filter((target, index, all) => all.findIndex(item => item.id === target.id) === index).map(target => <Link key={target.id} to={`/platform/${encodeURIComponent(target.id)}`} className="flex items-center justify-between gap-2 text-sm hover:text-primary"><span className="min-w-0 truncate">{target.name}</span><span className="shrink-0 text-xs text-muted-foreground">{t(summary.shared.some(item => item.id === target.id) ? "skillBrowser.installationSource.shared" : "skillBrowser.installationSource.independent")}</span></Link>)}
      </div>
    </details>
  </section>;
  function resize(clientX: number) {
    const bounds = root.current?.getBoundingClientRect(); if(!bounds) return;
    const next = Math.min(70,Math.max(30,(bounds.right-clientX)/bounds.width*100)); setWidth(next); writePreference("skillshub.browser.width",next);
  }
  const viewControl = allowFlatView ? <Button type="button" size="icon-sm" variant="ghost" title={t(flat ? "browser.groupedView" : "browser.flatView")} aria-label={t(flat ? "browser.groupedView" : "browser.flatView")} aria-pressed={flat} onClick={switchView}>{flat ? <FolderTree className="size-4"/> : <List className="size-4"/>}</Button> : null;
  return <div ref={root} className="flex min-h-0 min-w-0 flex-1 overflow-hidden" data-testid="skill-browser-workspace">
    <section tabIndex={0} onKeyDown={expansionKeys} onMouseDown={event => {
      const target = event.target;
      // Toolbar focus belongs to its controls. Hiding a details popup while the
      // browser is still handling its label's mouse press can crash WebView.
      if (!(target instanceof Element) || !listScroll.current?.contains(target)
        || target.closest('button,input,textarea,select,a,label,summary,details,[contenteditable="true"],[role="menu"],[role="dialog"]')) return;
      event.currentTarget.focus({preventScroll:true});
    }} className="flex min-w-0 flex-1 flex-col overflow-hidden outline-none" aria-label={t("browser.list")}>
      <div className="flex h-11 shrink-0 items-center border-b border-border [&>header]:h-full [&>header]:flex-1 [&>header]:border-b-0" style={{paddingRight: scrollbarWidth}}>{typeof toolbar === "function" ? toolbar(viewControl) : <div className="flex h-full items-center gap-1 px-2">{toolbar}{viewControl}</div>}</div>
      <div ref={listScroll} className="skill-browser-list min-h-0 flex-1 overflow-y-auto overflow-x-hidden [overflow-anchor:none]">
        {loading && !groups.length ? <p className="p-8 text-center text-sm text-muted-foreground">{t("common.loading")}</p> : groups.length ? <SkillBrowserTable {...tableProps} onSelectionChange={keys=>{setSelectedRowKeys(keys);tableProps.onSelectionChange?.(keys);}} statusPath={pathname} statusCollections={collectionsMode} kind={flat ? "skill" : "folder"} tree visibleColumns={collectionsMode ? new Set([...visibleColumns].filter(column => !["skillCount", "githubStars", "installSummary"].includes(column))) : visibleColumns} skills={skills.map(skill => ({...skill, highlighted: selectedSkill && id(selectedSkill) === id(skill), onDetail: () => chooseSkill(skill)}))} folders={treeRows} onToggleColumn={toggleColumn} onResetColumns={resetColumns} stickyHeaderTop="0px" className="rounded-none border-0 shadow-none"/> : <p className="p-8 text-center text-sm text-muted-foreground">{t("browser.noMatches")}</p>}
      </div>
      <div ref={statusSlot} className="shrink-0" data-testid="browser-status-slot"/>
    </section>
    {collapsed && <aside style={{width:ICON_RAIL_WIDTH}} className="shrink-0 border-l border-border bg-sidebar" aria-label={t("browser.preview")}>
      <div className="flex h-11 items-center justify-center border-b border-border"><button className={cn(ICON_RAIL_BUTTON,"text-muted-foreground")} title={t("browser.showPreview")} aria-label={t("browser.showPreview")} onClick={()=>setCollapsed(false)}><PanelRight/></button></div>
      <div className="flex flex-col items-center gap-2 py-2">{(["overview","document","notebook"] as Tab[]).map(value=><button key={value} className={cn(ICON_RAIL_BUTTON,"text-muted-foreground disabled:opacity-50",activeTab===value&&"bg-primary/15 text-primary")} title={t(`browser.${value}`)} aria-label={t(`browser.${value}`)} disabled={value==="notebook" ? !canUseNotebook(selectedFolder) : value==="document" && !selectedSkill && (!selectedFolder || collectionsMode)} onClick={()=>{setTab(value);writePreference("skillshub.browser.tab",value);}}><ActionIcon action={value==="notebook"?"notebook":value==="document"?"document":"info"}/></button>)}</div>
    </aside>}
    {!collapsed && <>
      <div role="separator" tabIndex={0} aria-label={t("browser.resize")} aria-orientation="vertical" aria-valuenow={Math.round(width)} aria-valuemin={30} aria-valuemax={70} className="relative w-1.5 shrink-0 cursor-col-resize touch-none bg-background after:pointer-events-none after:absolute after:inset-y-0 after:left-0 after:w-px after:bg-border hover:bg-primary/10 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-ring"
        onPointerDown={e=>{e.preventDefault();e.currentTarget.setPointerCapture(e.pointerId);}}
        onPointerMove={e=>{if(e.currentTarget.hasPointerCapture(e.pointerId))resize(e.clientX);}}
        onPointerUp={e=>{if(e.currentTarget.hasPointerCapture(e.pointerId))e.currentTarget.releasePointerCapture(e.pointerId);}}
        onKeyDown={e=>{if(e.key==="ArrowLeft"||e.key==="ArrowRight"){e.preventDefault();const next=Math.min(70,Math.max(30,width+(e.key==="ArrowLeft"?2:-2)));setWidth(next);writePreference("skillshub.browser.width",next);}}}/>
      <aside style={{flexBasis:`${width}%`}} className="flex min-w-0 shrink-0 flex-col overflow-hidden" aria-label={t("browser.preview")}>
        <div role="tablist" className="flex h-11 shrink-0 items-stretch gap-4 border-b border-border px-4">
          {((selectedSkill ? ["overview","document"] : !collectionsMode && selectedFolder ? ["overview","document", ...(canUseNotebook(selectedFolder) ? ["notebook"] : [])] : ["overview"]) as Tab[]).map(value=><button role="tab" aria-selected={activeTab===value} key={value} onClick={()=>{setTab(value);writePreference("skillshub.browser.tab",value);}} className={cn("inline-flex items-center gap-2 border-b-2 border-transparent px-1 text-sm font-semibold text-muted-foreground",activeTab===value&&"border-primary text-primary")}>{value === "notebook" ? <ActionIcon action="notebook"/> : <ActionIcon action={value === "document" ? "document" : "info"}/>}{t(`browser.${value}`)}</button>)}
          <Button className="my-auto ml-auto text-muted-foreground hover:text-foreground" size="icon-sm" variant="ghost" aria-label={t("browser.hidePreview")} onClick={()=>setCollapsed(true)}><PanelRight className="size-4 shrink-0"/></Button>
        </div>
        {selectedSkill && request ? <>
          <div className="min-h-0 flex-1"><SkillDetailView key={JSON.stringify(request)} {...request} variant="inspector" inspectorTab={activeTab==="document"?"document":"overview"} installationSummary={installationSummary}/></div>
        </> : selectedFolder && activeTab === "notebook" ? <div className="min-h-0 flex-1"><RepositoryNotebook key={selectedFolder.key} directory={selectedFolder.path} skillId={selectedFolder.children?.[0]?.detailRequest?.skillId ?? selectedFolder.children?.[0]?.rowKey ?? ""}/></div>
        : selectedFolder && activeTab === "document" ? <RepositoryDocument key={selectedFolder.key} directory={selectedFolder.path} skillId={selectedFolder.children?.[0]?.detailRequest?.skillId ?? selectedFolder.children?.[0]?.rowKey} revision={selectedFolder.updatedAt}
          sourceUrl={selectedFolder.sourceRepo ? `https://github.com/${selectedFolder.sourceRepo}/blob/HEAD/README.md` : undefined}/>
        : selectedFolder ? <div className="min-h-0 flex-1 space-y-4 overflow-auto p-4">
          <div className="flex items-center gap-2"><FolderOpen className="size-4 shrink-0 text-muted-foreground" aria-hidden="true"/><h2 className="overview-item-title min-w-0 break-all font-semibold">{selectedFolder.name}</h2></div>
          <>
            {activeTab === "overview" && <section aria-label={t("detail.metadataRegion")}>
              <details open className="group/overview-section"><summary className="overview-section-title"><Info className="size-4 shrink-0" aria-hidden="true"/>{t("detail.metadataRegion")}<ChevronRight className="size-4 shrink-0 group-open/overview-section:rotate-90" aria-hidden="true"/></summary>
              <div className="space-y-3 rounded-lg border border-border/70 bg-transparent p-3">
                {selectedFolder.metadata ?? <>{folderSources.length ? folderSources.map(source => <MetadataRow key={source.label} label={t("detail.sourceRepo")} value={source.label} href={source.repository} directory={source.repositoryDirectory}/>) : <MetadataRow label={t("detail.sourceRepo")} value="—"/>}
                <MetadataRow label={t("detail.storagePath")} value={selectedFolder.path || "—"} directory={selectedFolder.path || undefined}/></>}
              </div>
              </details>
            </section>}
            {selectedFolder.notesEditor}
            {installationSummary}
            <section aria-label={t("browser.skillList")}>
              <details open className="group/overview-section"><summary className="overview-section-title"><List className="size-4 shrink-0" aria-hidden="true"/>{t("browser.skillList")}<ChevronRight className="size-4 shrink-0 group-open/overview-section:rotate-90" aria-hidden="true"/></summary>
              <div className="overflow-hidden rounded-lg border border-border/70">
                {selectedFolder.skillListLoading ? <p className="p-3 text-sm text-muted-foreground">{t("common.loading")}</p> : (
                  <SkillBrowserTable key={selectedFolder.key} onSelectionChange={keys=>{setSelectedRowKeys(keys);tableProps.onSelectionChange?.(keys);}} compactList kind="skill" className="border-0 bg-transparent shadow-none [&_thead]:bg-transparent [&_th]:bg-transparent" visibleColumns={new Set(["index", "name"])} skills={(selectedFolder.children ?? []).map(skill => ({
                    ...skill,
                    onDetail: () => {
                      if (selectedFolder.expandable !== false) setExpanded(previous => new Set([...previous, selectedFolder.key]));
                      chooseSkill(skill);
                    },
                  }))} />
                )}
              </div>
              </details>
            </section>

          </>
        </div> : <div className="flex flex-1 items-center justify-center p-6 text-center text-sm text-muted-foreground">{t("browser.selectItem")}</div>}
      </aside>
    </>}
  </div>;
}
