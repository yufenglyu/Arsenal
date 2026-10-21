import { PlatformMenu } from "./PlatformMenu";
import { ProjectDirectoryMenu } from "./ProjectDirectoryMenu";
import { useSettingsStore } from "@/stores/settingsStore";
import { useExpansionShortcuts } from "@/hooks/useExpansionShortcuts";
import {
  useEffect,
  useRef,
  useState,
  type MouseEvent as ReactMouseEvent,
  type PointerEvent as ReactPointerEvent,
} from "react";
import { useNavigate, useLocation } from "react-router-dom";
import {
  Loader2,
  PanelLeft,
  Download,
  Monitor,
  Moon,
  Settings,
  Sun,
  FolderOpen,
  FolderTree,
  AppWindow,
  ChevronRight,
  PackageCheck,
} from "lucide-react";
import { useTranslation } from "react-i18next";
import { save as saveDialog } from "@tauri-apps/plugin-dialog";
import { toast } from "sonner";
import { PlatformIcon } from "@/components/platform/PlatformIcon";
import { usePlatformStore } from "@/stores/platformStore";
import { useCollectionStore } from "@/stores/collectionStore";
import { useResourceLibraryStore } from "@/stores/resourceLibraryStore";
import { useCentralSkillsStore } from "@/stores/centralSkillsStore";
import { useThemeStore } from "@/stores/themeStore";
import { cn } from "@/lib/utils";
import { isInstallTargetAgent } from "@/lib/agents";
import { isProjectAgentId } from "@/lib/projectTargets";
import { useSidebarWidth } from "@/hooks/useSidebarWidth";
import { SharedHubIcon, SkillRepositoryIcon } from "@/components/skill/SkillActionIcons";
import { useSidebarStore } from "@/stores/sidebarStore";
import { ICON_RAIL_WIDTH, ICON_RAIL_BUTTON } from "./iconRail";
import { CollectionTree } from "@/components/collection/CollectionTree";

// ─── Nav Item ────────────────────────────────────────────────────────────────

function NavItem({
  label,
  ariaLabel,
  title,
  isActive,
  onClick,
  icon,
  expanded,
  count,
  status,
  onContextMenu,
  nested = false,
}: {
  label: string;
  nested?: boolean;
  ariaLabel?: string;
  title?: string;
  isActive: boolean;
  onClick: () => void;
  icon: React.ReactNode;
  expanded: boolean;
  count?: number;
  status?: {
    label: string;
    hint: string;
    shared: boolean;
  };
  onContextMenu?: (event: ReactMouseEvent<HTMLButtonElement>) => void;
}) {
  const accessibleLabel = [ariaLabel ?? label, status?.label].filter(Boolean).join(" — ");
  const tooltip = status ? `${title ?? label} — ${status.hint}` : title ?? label;
  return (
    <div className="relative">
      <button
        onClick={onClick}
        onContextMenu={onContextMenu}
        title={tooltip}
        aria-label={accessibleLabel}
        aria-current={isActive ? "page" : undefined}
        className={cn(
          "flex items-center rounded-md transition-colors cursor-pointer",
          expanded ? "w-full" : "mx-auto",
          nested ? "font-normal" : "font-semibold",
          !isActive && "hover:bg-primary/15 hover:text-primary",
          isActive && "bg-hover-bg text-white",
          expanded ? "gap-2.5 px-2.5 py-1.5 text-sm" : ICON_RAIL_BUTTON
        )}
      >
        <span className="shrink-0">{icon}</span>
        {expanded && (
          <>
            <span className="truncate min-w-0 flex-1 text-left">{label}</span>
            {status && (
              <span
                className={cn(
                  "size-1.5 shrink-0 rounded-full",
                  status.shared
                    ? "bg-chart-4"
                    : "bg-primary"
                )}
                title={status.hint}
                aria-hidden="true"
              />
            )}
            {count !== undefined && (
              <span className={cn(
                "min-w-7 text-center text-xs font-normal tabular-nums px-1.5 py-0.5 rounded-full shrink-0",
                isActive
                  ? "text-white"
                  : "text-sidebar-foreground"
              )}>
                {count}
              </span>
            )}
          </>
        )}
      </button>
      {isActive && (
        <span
          className="absolute left-0 top-1.5 bottom-1.5 w-0.5 rounded-r bg-white"
          aria-hidden="true"
        />
      )}
    </div>
  );
}

// ─── Sidebar ────────────────────────────────────────────────────────────────

export function Sidebar({ settingsOpen = false }: {settingsOpen?: boolean} = {}) {
  const repositoryRegion = useRef<HTMLDivElement>(null);
  const [repositoryExpanded,setRepositoryExpanded]=useState(true);
  const [installationCollapsed,setInstallationCollapsed]=usePersistentBoolean("skillshub.sidebar.installationCollapsed",false);
  const installationExpansionKeys=useExpansionShortcuts(()=>setInstallationCollapsed(false),()=>setInstallationCollapsed(true),true);
  const [repositoryHeight, setRepositoryHeight] = useState<number | undefined>(()=>{try{const value=Number(localStorage.getItem("skillshub.sidebar.repositoryHeight"));return value>=80?value:undefined;}catch{return undefined;}});
  function resizeRepository(height:number){const next=Math.max(80,Math.min(Math.max(80,window.innerHeight-240),height));setRepositoryHeight(next);try{localStorage.setItem("skillshub.sidebar.repositoryHeight",String(next));}catch{/* Keep session state. */}}
  const SOFTWARE_COLLAPSED_KEY = "skills-manage:sidebar-software-platforms-collapsed";
  const PROJECTS_COLLAPSED_KEY = "skills-manage:sidebar-project-directories-collapsed";
  const navigate = useNavigate();
  const { pathname } = useLocation();
  const { t } = useTranslation();
  const { agents, skillsByAgent, isLoading } = usePlatformStore();
  const sidebarWidth = useSidebarWidth();
  const loadScanDirectories = useSettingsStore(s => s.loadScanDirectories);
  useEffect(() => { void loadScanDirectories(); }, [loadScanDirectories]);
  const dragState = useRef<{ startX: number; startWidth: number } | null>(null);

  const selectGroup = useCollectionStore((s) => s.selectGroup);
  const loadCollections = useCollectionStore((s) => s.loadCollections);

  const resourceSkillsCount = useResourceLibraryStore((s) => s.skills.length);
  const loadResourceLibrary = useResourceLibraryStore((s) => s.loadResourceLibrary);
  const exportDirectoryList = useResourceLibraryStore((s) => s.exportDirectoryList);
  const centralSkillsCount = useCentralSkillsStore((s) => s.skills.length);
  const loadCentralSkills = useCentralSkillsStore((s) => s.loadCentralSkills);
  const themeMode = useThemeStore((s) => s.mode);
  const cycleThemeMode = useThemeStore((s) => s.cycleMode);

  const expanded = useSidebarStore((s) => s.expanded);
  const setExpanded = useSidebarStore((s) => s.setExpanded);
  const [softwareCollapsed, setSoftwareCollapsed] = usePersistentBoolean(
    SOFTWARE_COLLAPSED_KEY,
    false
  );
  const [projectsCollapsed, setProjectsCollapsed] = usePersistentBoolean(
    PROJECTS_COLLAPSED_KEY,
    false
  );
  const [resourceMenu, setResourceMenu] = useState<{ x: number; y: number } | null>(null);
  const [isExportingDirectoryList, setIsExportingDirectoryList] = useState(false);

  useEffect(() => {
    function closeMenu() {
      setResourceMenu(null);
    }
    window.addEventListener("click", closeMenu);
    window.addEventListener("keydown", closeMenu);
    return () => {
      window.removeEventListener("click", closeMenu);
      window.removeEventListener("keydown", closeMenu);
    };
  }, []);

  useEffect(() => {
    loadCollections();
    loadResourceLibrary();
    loadCentralSkills();
  }, [loadCentralSkills, loadCollections, loadResourceLibrary]);

  useEffect(() => {
    const width = expanded ? sidebarWidth.width : ICON_RAIL_WIDTH;
    document.documentElement.style.setProperty("--app-sidebar-width", `${width}px`);
    return () => {
      document.documentElement.style.removeProperty("--app-sidebar-width");
    };
  }, [expanded, sidebarWidth.width]);

  function handleResizePointerDown(event: ReactPointerEvent<HTMLButtonElement>) {
    event.preventDefault();
    dragState.current = {
      startX: event.clientX,
      startWidth: sidebarWidth.width,
    };

    function handleMove(moveEvent: PointerEvent) {
      const state = dragState.current;
      if (!state) return;
      sidebarWidth.setWidth(state.startWidth + moveEvent.clientX - state.startX);
    }

    function handleUp() {
      dragState.current = null;
      document.removeEventListener("pointermove", handleMove);
      document.removeEventListener("pointerup", handleUp);
    }

    document.addEventListener("pointermove", handleMove);
    document.addEventListener("pointerup", handleUp);
  }

  const platformAgents = agents.filter(
    (a) =>
      isInstallTargetAgent(a) &&
      !isProjectAgentId(a.id) &&
      a.is_enabled
  );
  const projectAgents = agents.filter(
    (a) =>
      isProjectAgentId(a.id) && a.is_enabled
  );

  const softwareExpansionKeys = useExpansionShortcuts(() => setSoftwareCollapsed(false), () => setSoftwareCollapsed(true), true);
  const projectExpansionKeys = useExpansionShortcuts(() => setProjectsCollapsed(false), () => setProjectsCollapsed(true), true);
  const themeLabel = t("topBar.cycleTheme", {
    mode: t(`topBar.themeMode.${themeMode}`),
  });
  const ThemeIcon = themeMode === "system" ? Monitor : themeMode === "light" ? Sun : Moon;


  function handleResourceContextMenu(event: ReactMouseEvent<HTMLButtonElement>) {
    event.preventDefault();
    setResourceMenu({ x: event.clientX, y: event.clientY });
  }

  async function handleExportDirectoryList() {
    if (isExportingDirectoryList) return;
    setResourceMenu(null);
    const selected = await saveDialog({
      title: t("resource.exportDirectoryListTitle"),
      defaultPath: "skill-directory-list.csv",
      filters: [{ name: "CSV", extensions: ["csv"] }],
    });
    if (!selected) return;
    setIsExportingDirectoryList(true);
    try {
      const count = await exportDirectoryList(selected);
      toast.success(t("resource.exportDirectoryListSuccess", { count }));
    } catch (err) {
      toast.error(t("resource.exportDirectoryListError", { error: String(err) }));
    } finally {
      setIsExportingDirectoryList(false);
    }
  }

  return (
    <nav
      className={cn(
        "relative flex flex-col shrink-0 h-full border-r border-border bg-sidebar text-sidebar-foreground transition-[width] duration-200"
      )}
      style={{
        width: expanded ? sidebarWidth.width : ICON_RAIL_WIDTH,
      }}
      aria-label={t("sidebar.mainNav")}
    >
      {/* Toggle button */}
      <div
        className={cn(
          "flex h-11 shrink-0 items-center border-b border-border",
          expanded ? "justify-between px-3" : "justify-center"
        )}
      >
        {expanded && (
          <div className="flex min-w-0 flex-1 items-center gap-2 pr-2">
            <img src="/arsenal.svg" alt="" aria-hidden="true" className="size-3.5 shrink-0" />
            <span className="truncate text-sm font-semibold leading-4 text-sidebar-primary">{t("app.name")}</span>
          </div>
        )}
        <button
          onClick={() => setExpanded(!expanded)}
          className={cn(
            ICON_RAIL_BUTTON,
            "text-muted-foreground hover:text-foreground hover:bg-muted/60"
          )}
          aria-label={expanded ? t("sidebar.collapseSidebar") : t("sidebar.expandSidebar")}
          title={expanded ? t("sidebar.collapseSidebar") : t("sidebar.expandSidebar")}
        >
          <PanelLeft className="size-4" />
        </button>
      </div>

      {/* Scrollable nav items */}
      <div className={cn("flex min-h-0 flex-1 flex-col overflow-y-auto py-2 space-y-0.5", expanded ? "px-1.5" : "px-0")}>
        {/* Skill Repository */}
        {expanded ? <>
          <div ref={repositoryRegion} className={cn("overflow-y-auto",installationCollapsed?"min-h-0 flex-1":"shrink-0")} style={{height:repositoryExpanded&&!installationCollapsed?repositoryHeight:undefined,maxHeight:installationCollapsed?undefined:"calc(100vh - 240px)"}} data-testid="repository-tree-region"><CollectionTree onExpandedChange={setRepositoryExpanded} count={resourceSkillsCount} onExport={()=>void handleExportDirectoryList()}/></div>
          <div role="separator" tabIndex={0} aria-label={t("sidebar.resizeRepositoryHeight")} aria-orientation="horizontal" aria-valuemin={80} aria-valuemax={Math.max(80,window.innerHeight-240)} aria-valuenow={repositoryHeight??repositoryRegion.current?.clientHeight??80} className={cn((!repositoryExpanded||installationCollapsed)&&"hidden", "h-1.5 cursor-row-resize touch-none border-b border-border hover:bg-primary/10 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring")}
            onPointerDown={event=>{event.preventDefault();event.currentTarget.setPointerCapture(event.pointerId);}}
            onPointerMove={event=>{if(event.currentTarget.hasPointerCapture(event.pointerId)&&repositoryRegion.current)resizeRepository(event.clientY-repositoryRegion.current.getBoundingClientRect().top);}}
            onPointerUp={event=>{if(event.currentTarget.hasPointerCapture(event.pointerId))event.currentTarget.releasePointerCapture(event.pointerId);}}
            onKeyDown={event=>{if(event.key==="ArrowUp"||event.key==="ArrowDown"){event.preventDefault();resizeRepository((repositoryHeight??repositoryRegion.current?.clientHeight??160)+(event.key==="ArrowDown"?16:-16));}}}/>
        </> : <NavItem
          label={t("sidebar.resourceLibrary")}
          isActive={(pathname === "/resources" || pathname === "/")}
          onClick={() => {selectGroup(null);navigate("/resources");}}
          onContextMenu={handleResourceContextMenu}
          icon={<SkillRepositoryIcon className="size-4" />}
          expanded={expanded}
          count={resourceSkillsCount}
        />}

        {/* Divider */}
        <div className={cn("my-2", !expanded && "border-t border-sidebar-border/70")} />

        <section aria-label={t("sidebar.skillInstallation")} className={cn("shrink-0",expanded&&installationCollapsed&&"mt-auto border-t border-border pt-2")}>
        {expanded && <button type="button" aria-expanded={!installationCollapsed} onKeyDown={event=>{if(event.key==="Enter"||event.key===" "){event.preventDefault();setInstallationCollapsed(!installationCollapsed);}else installationExpansionKeys(event);}} onClick={event=>event.currentTarget.focus()} onDoubleClick={()=>setInstallationCollapsed(!installationCollapsed)} className="flex w-full items-center gap-2 rounded-md px-1 py-1 text-sm font-semibold hover:bg-primary/10 hover:text-primary focus:bg-primary/10 focus:text-primary">
          <PackageCheck className="size-3.5 shrink-0"/><span>{t("sidebar.skillInstallation")}</span>
        </button>}
        {(!expanded || !installationCollapsed)&&<div className="space-y-0.5">
        {/* Installation targets */}
        {expanded ? (
          <div
            data-testid="central-skills-heading"
            className="flex items-center justify-between gap-1 py-1"
          >
            <button
              type="button"
              onClick={() => navigate("/central")}
              aria-current={pathname === "/central" ? "page" : undefined}
              aria-label={t("sidebar.centralSkills")}
              title={t("sidebar.centralSkills")}
              className={cn(
                "flex min-w-0 flex-1 items-center gap-2 rounded-md pl-6 pr-2 py-1 text-sm font-semibold transition-colors",
                pathname === "/central"
                  ? "bg-hover-bg text-white"
                  : "text-muted-foreground hover:bg-primary/10 hover:text-primary"
              )}
            >
              <SharedHubIcon className="size-3.5 shrink-0" />
              <span className="truncate min-w-0 flex-1 text-left">{t("sidebar.centralSkills")}</span>
              {centralSkillsCount > 0 && (
                <span
                  className={cn(
                    "min-w-7 text-center text-xs font-normal tabular-nums px-1.5 py-0.5 rounded-full shrink-0",
                    pathname === "/central"
                      ? "text-white"
                      : "text-sidebar-foreground"
                  )}
                >
                  {centralSkillsCount}
                </span>
              )}
            </button>
          </div>
        ) : (
          <NavItem
            label={t("sidebar.centralSkills")}
            isActive={pathname === "/central"}
            onClick={() => navigate("/central")}
            icon={<SharedHubIcon className="size-4" />}
            expanded={false}
            count={centralSkillsCount}
          />
        )}

        {/* Software platforms */}
        {expanded ? (
          <PlatformMenu onAdded={()=>{setSoftwareCollapsed(false);}}><div
            onKeyDown={softwareExpansionKeys}
            data-testid="software-platform-heading"
            className="flex items-center justify-between gap-1 py-1"
          >
            <button
              type="button"
              onClick={() => setSoftwareCollapsed(!softwareCollapsed)}
              aria-expanded={!softwareCollapsed}
              className="flex min-w-0 flex-1 items-center gap-2 rounded-md pl-1 pr-2 py-1 text-muted-foreground transition-colors hover:bg-primary/10 hover:text-primary"
            >
              <ChevronRight className={cn("size-3 shrink-0",!softwareCollapsed&&"rotate-90")}/>
              <AppWindow
                data-testid="software-platform-heading-icon"
                className="size-3.5 shrink-0"
              />
              <span className="truncate text-left text-sm font-semibold">
                {t("sidebar.softwarePlatforms")}
              </span><span className="ml-auto min-w-7 text-center text-xs tabular-nums">{platformAgents.length}</span>
            </button>
          </div></PlatformMenu>
        ) : (
          <div className="border-t border-sidebar-border/40 my-1.5" />
        )}

        {isLoading ? (
          <div className={cn(
            "flex items-center py-2 text-muted-foreground text-sm",
            expanded ? "gap-2 px-2.5" : "justify-center"
          )}>
            <Loader2 className="size-4 animate-spin shrink-0" />
            {expanded && <span>{t("sidebar.scanning")}</span>}
          </div>
        ) : (
          <>
            {(!expanded || !softwareCollapsed) && platformAgents.length > 0 && (
              <div onKeyDown={softwareExpansionKeys} className={expanded ? "ml-6 border-l border-sidebar-border/70 pl-2" : ""}>
                {platformAgents.map((agent) => (
                  <PlatformMenu key={agent.id} platform={agent}><NavItem nested
                    label={agent.display_name}
                    isActive={pathname === `/platform/${encodeURIComponent(agent.id)}`}
                    onClick={() => navigate(`/platform/${encodeURIComponent(agent.id)}`)}
                    icon={<PlatformIcon agentId={agent.id} brand displayName={agent.display_name} className={expanded ? "size-[1em]" : "size-4"} size={expanded ? 14 : 16} />}
                    expanded={expanded}
                    count={skillsByAgent[agent.id] ?? 0}
                    status={{
                      label: agent.shares_central_skills
                        ? t("sidebar.sharedDir")
                        : t("sidebar.independentDir"),
                      hint: agent.shares_central_skills
                        ? t("sidebar.sharedDirHint")
                        : t("sidebar.independentDirHint"),
                      shared: !!agent.shares_central_skills,
                    }}
                  /></PlatformMenu>
                ))}
              </div>
            )}

            {expanded ? (
              <ProjectDirectoryMenu onAdded={() => { setProjectsCollapsed(false); }}><div
                onKeyDown={projectExpansionKeys}
                data-testid="project-directories-heading"
                className="flex items-center justify-between gap-1 py-1"
              >
                <button
                  type="button"
                  onClick={() => setProjectsCollapsed(!projectsCollapsed)}
                  aria-expanded={!projectsCollapsed}
                  className="flex min-w-0 flex-1 items-center gap-2 rounded-md pl-1 pr-2 py-1 text-muted-foreground transition-colors hover:bg-primary/10 hover:text-primary"
                >
                  <ChevronRight className={cn("size-3 shrink-0",!projectsCollapsed&&"rotate-90")}/><FolderTree className="size-3.5 shrink-0" />
                  <span className="truncate text-left text-sm font-semibold">
                    {t("sidebar.projectDirectories")}
                  </span><span className="ml-auto min-w-7 text-center text-xs tabular-nums">{projectAgents.length}</span>
                </button>
              </div></ProjectDirectoryMenu>
            ) : (
              <div className="border-t border-sidebar-border/40 my-1.5" />
            )}
            {(!expanded || !projectsCollapsed) && projectAgents.length > 0 && (
              <div onKeyDown={projectExpansionKeys} className={expanded ? "ml-6 border-l border-sidebar-border/70 pl-2" : ""}>
                {projectAgents.map((agent) => (
                  <ProjectDirectoryMenu key={agent.id} agentId={agent.id}><NavItem nested
                    key={agent.id}
                    label={agent.display_name}
                    isActive={pathname === `/platform/${encodeURIComponent(agent.id)}`}
                    onClick={() => navigate(`/platform/${encodeURIComponent(agent.id)}`)}
                    icon={<FolderOpen className="size-4" />}
                    expanded={expanded}
                    count={skillsByAgent[agent.id] ?? 0}
                  /></ProjectDirectoryMenu>
                ))}
              </div>
            )}
          </>
        )}
        </div>}
        </section>
      </div>

      <div id="sidebar-tag-filter" className={cn("shrink-0", !expanded && "hidden")} />

      <div
        className={cn(
          "flex shrink-0 items-center border-t border-border",
          expanded ? "h-10 gap-1 px-1.5" : "flex-col gap-2 px-1 py-2"
        )}
      >
        <button
          type="button"
          onClick={cycleThemeMode}
          title={themeLabel}
          aria-label={themeLabel}
          className={cn(ICON_RAIL_BUTTON, "text-muted-foreground")}
        >
          <ThemeIcon className="size-4" />
        </button>
        <button
          type="button"
          title={t("sidebar.settings")}
          aria-label={t("sidebar.settings")}
          aria-current={(settingsOpen || pathname === "/settings") ? "page" : undefined}
          onClick={() => navigate("/settings")}
          className={cn(
            ICON_RAIL_BUTTON,
            (settingsOpen || pathname === "/settings") ? "bg-primary/15 text-primary" : "text-muted-foreground"
          )}
        >
          <Settings className="size-4" />
        </button>
      </div>

      {resourceMenu ? (
        <div
          role="menu"
          aria-label={t("resource.exportDirectoryList")}
          className="fixed z-50 min-w-44 rounded-lg border border-border bg-popover p-1 text-popover-foreground shadow-lg"
          style={{ left: resourceMenu.x, top: resourceMenu.y }}
          onClick={(event) => event.stopPropagation()}
        >
          <button
            type="button"
            role="menuitem"
            className="flex w-full items-center gap-2 rounded-md px-2.5 py-2 text-left text-sm transition-colors hover:bg-muted focus-visible:bg-muted focus-visible:outline-none"
            onClick={() => void handleExportDirectoryList()}
            disabled={isExportingDirectoryList}
          >
            {isExportingDirectoryList ? (
              <Loader2 className="size-4 animate-spin" />
            ) : (
              <Download className="size-4" />
            )}
            <span>{t("resource.exportDirectoryList")}</span>
          </button>
        </div>
      ) : null}

      {expanded && (
        <button
          type="button"
          aria-label={t("sidebar.resizeSidebar", { defaultValue: "调整侧栏宽度" })}
          title={t("sidebar.resizeSidebar", { defaultValue: "调整侧栏宽度" })}
          onPointerDown={handleResizePointerDown}
          onDoubleClick={sidebarWidth.resetWidth}
          className={cn(
            "absolute right-[-3px] top-0 bottom-0 z-10 w-1.5 cursor-col-resize bg-transparent",
            "transition-colors hover:bg-primary/30 focus-visible:bg-primary/30 focus-visible:outline-none"
          )}
        />
      )}
    </nav>
  );
}

function usePersistentBoolean(key: string, defaultValue: boolean) {
  const [value, setValue] = useState(() => {
    try {
      const stored = window.localStorage.getItem(key);
      return stored === null ? defaultValue : stored === "true";
    } catch {
      return defaultValue;
    }
  });

  function update(next: boolean) {
    setValue(next);
    try {
      window.localStorage.setItem(key, String(next));
    } catch {
      // Ignore storage failures and keep the in-memory preference.
    }
  }

  return [value, update] as const;
}
