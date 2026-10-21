vi.mock("../stores/settingsStore", () => {
  const state = {scanDirectories: [], loadScanDirectories: vi.fn()};
  return {useSettingsStore: (selector?: (value: typeof state) => unknown) => selector ? selector(state) : state};
});
import { describe, it, expect, vi, beforeEach } from "vitest";
import { act, render, screen, fireEvent, within, waitFor } from "@testing-library/react";
import { MemoryRouter, useLocation } from "react-router-dom";
import { Sidebar } from "../components/layout/Sidebar";
import { usePlatformStore } from "../stores/platformStore";
import { useResourceLibraryStore } from "../stores/resourceLibraryStore";
import { useCentralSkillsStore } from "../stores/centralSkillsStore";
import { useThemeStore } from "../stores/themeStore";
import { useSidebarStore } from "../stores/sidebarStore";
import type { AgentWithStatus } from "../types";

const mockSaveDialog = vi.hoisted(() => vi.fn());
const mockExportDirectoryList = vi.hoisted(() => vi.fn());

// Mock the platformStore to avoid real Tauri invocations
vi.mock("../stores/platformStore", () => ({
  usePlatformStore: vi.fn(),
}));

vi.mock("../stores/resourceLibraryStore", () => ({
  useResourceLibraryStore: vi.fn(),
}));

vi.mock("../stores/centralSkillsStore", () => ({
  useCentralSkillsStore: vi.fn(),
}));

vi.mock("../stores/themeStore", () => ({
  useThemeStore: vi.fn(),
}));

vi.mock("@tauri-apps/plugin-dialog", () => ({
  save: (...args: unknown[]) => mockSaveDialog(...args),
}));

vi.mock("sonner", () => ({
  toast: {
    success: vi.fn(),
    error: vi.fn(),
  },
}));

// Mock the collectionStore
vi.mock("../stores/collectionStore", () => ({
  useCollectionStore: vi.fn(),
}));

import { useCollectionStore } from "../stores/collectionStore";

const mockAgents: AgentWithStatus[] = [
  {
    id: "claude-code",
    display_name: "Claude Code",
    global_skills_dir: "~/.claude/skills/",
    is_detected: true,
    is_builtin: true,
    is_enabled: true,
  },
  {
    id: "cursor",
    display_name: "Cursor",
    global_skills_dir: "~/.cursor/skills/",
    is_detected: true,
    is_builtin: true,
    is_enabled: true,
  },
  {
    id: "central",
    display_name: "Shared Hub",
    global_skills_dir: "~/.agents/skills/",
    is_detected: true,
    is_builtin: true,
    is_enabled: true,
  },
];

const defaultStoreState = {
  agents: mockAgents,
  skillsByAgent: {
    "claude-code": 5,
    cursor: 3,
    central: 10,
  },
  isLoading: false,
  isRefreshing: false,
  error: null,
  initialize: vi.fn(),
  rescan: vi.fn(),
  refreshCounts: vi.fn(),
};

type SidebarPlatformState = Omit<typeof defaultStoreState, "skillsByAgent"> & {
  skillsByAgent: Record<string, number>;
};

const defaultCollectionState = {
  collections: [],
  activeCollectionId:null, groupDetails:[], selectedSkillIds:[], includeDescendants:true, selectGroup:vi.fn(), loadGroupDetails:vi.fn().mockResolvedValue(undefined), changeGroup:vi.fn().mockResolvedValue(undefined),
    currentDetail: null,
  isLoading: false,
  isLoadingDetail: false,
  error: null,
  loadCollections: vi.fn(),
  createCollection: vi.fn(),
  updateCollection: vi.fn(),
  deleteCollection: vi.fn(),
  loadCollectionDetail: vi.fn(),
  addSkillToCollection: vi.fn(),
  removeSkillFromCollection: vi.fn(),
  batchInstallCollection: vi.fn(),
  exportCollection: vi.fn(),
  importCollection: vi.fn(),
  refreshCounts: vi.fn(),
};

const defaultResourceLibraryState = {
  skills: [],
  agents: [],
  resourceLibraryDir: "~/.arsenal/library",
  isLoading: false,
  isUpdatingSources: false,
  togglingAgentId: null,
  loadResourceLibrary: vi.fn(),
  installSkill: vi.fn(),
  togglePlatformLink: vi.fn(),
  updateSourceBackedSkills: vi.fn(),
  updateSourceBackedSkill: vi.fn(),
  addToCentral: vi.fn(),
  removeFromCentral: vi.fn(),
  exportDirectoryList: mockExportDirectoryList,
};

const defaultCentralSkillsState = {
  skills: [],
  loadCentralSkills: vi.fn(),
};

const mockCycleThemeMode = vi.fn();

function LocationProbe() {
  const location = useLocation();
  return <div data-testid="location-path">{location.pathname}</div>;
}

function renderSidebar(
  initialPath = "/central",
  options: {
    platformState?: SidebarPlatformState;
    centralSkillsCount?: number;
  } = {}
) {
  vi.mocked(usePlatformStore).mockReturnValue(defaultStoreState);
  if (options.platformState) {
    vi.mocked(usePlatformStore).mockReturnValue(options.platformState);
  }
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  vi.mocked(useResourceLibraryStore).mockImplementation((selector: any) =>
    selector(defaultResourceLibraryState)
  );
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  vi.mocked(useCentralSkillsStore).mockImplementation((selector: any) =>
    selector({
      ...defaultCentralSkillsState,
      skills: Array.from({ length: options.centralSkillsCount ?? 0 }, (_, index) => ({
        id: `central-${index}`,
      })),
    })
  );
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  vi.mocked(useThemeStore).mockImplementation((selector: any) =>
    selector({
      mode: "system",
      resolvedTheme: "dark",
      cycleMode: mockCycleThemeMode,
    })
  );
  return render(
    <MemoryRouter initialEntries={[initialPath]}>
      <Sidebar />
      <LocationProbe />
    </MemoryRouter>
  );
}

describe("Sidebar", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mockSaveDialog.mockReset();
    mockExportDirectoryList.mockReset();
    window.localStorage?.clear?.();
    useSidebarStore.setState({ expanded: true });
    // Default: collection store returns empty state.
    vi.mocked(useCollectionStore).mockImplementation((selector) =>
      selector(defaultCollectionState)
    );
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    vi.mocked(useResourceLibraryStore).mockImplementation((selector: any) =>
      selector(defaultResourceLibraryState)
    );
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    vi.mocked(useCentralSkillsStore).mockImplementation((selector: any) =>
      selector(defaultCentralSkillsState)
    );
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    vi.mocked(useThemeStore).mockImplementation((selector: any) =>
      selector({
        mode: "system",
        resolvedTheme: "dark",
        cycleMode: mockCycleThemeMode,
      })
    );
  });

  // ── Rendering ─────────────────────────────────────────────────────────────

  it("renders expanded sidebar by default", () => {
    const { container } = renderSidebar();
    const nav = container.querySelector("nav");
    expect(nav).toHaveStyle({ width: "280px" });
  });

  it("resizes expanded sidebar by dragging the resize handle", () => {
    const { container } = renderSidebar();
    const nav = container.querySelector("nav");
    const resizeHandle = screen.getByRole("button", { name: /调整侧栏宽度/ });

    fireEvent.pointerDown(resizeHandle, { clientX: 280 });
    act(() => {
      document.dispatchEvent(new PointerEvent("pointermove", { clientX: 340 }));
      document.dispatchEvent(new PointerEvent("pointerup"));
    });

    expect(nav).toHaveStyle({ width: "340px" });
  });

  it("renders platform agents as icon buttons", () => {
    renderSidebar();
    // Should show platform agents as buttons with title tooltips (not the central one)
    expect(screen.getByRole("button", { name: /Claude Code/ })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /Cursor/ })).toBeInTheDocument();
  });

  it("renders Shared Hub icon button", () => {
    renderSidebar();
    expect(screen.getByRole("button", { name: /共享中心/ })).toBeInTheDocument();
  });

  it("replaces Skill Bundles with repository groups", () => {
    renderSidebar();
    // Use exact string match to avoid also matching "导入技能集"
    expect(screen.queryByRole("button", { name: "技能分组" })).not.toBeInTheDocument();
    expect(screen.getByRole("tree",{name:"技能仓库"})).toBeInTheDocument();
  });

  it("groups installation targets under one persistent heading while keeping collapsed rail navigation",()=>{
    const view=renderSidebar();
    const section=screen.getByRole('region',{name:'技能安装'});
    expect(within(section).getByRole('button',{name:'共享中心'})).toBeInTheDocument();
    expect(within(section).getByText('软件平台')).toBeInTheDocument();
    expect(within(section).getByText('项目目录')).toBeInTheDocument();
    fireEvent.click(screen.getByRole('button',{name:'技能安装'}));
    expect(screen.getByRole('button',{name:'技能安装'})).toHaveFocus();
    fireEvent.doubleClick(screen.getByRole('button',{name:'技能安装'}));
    expect(screen.queryByRole('button',{name:'共享中心'})).not.toBeInTheDocument();
    view.unmount();renderSidebar();
    expect(screen.getByRole('button',{name:'技能安装'})).toHaveAttribute('aria-expanded','false');
    fireEvent.doubleClick(screen.getByRole('button',{name:'技能安装'}));
    expect(screen.getByRole('button',{name:'共享中心'})).toBeInTheDocument();
    expect(screen.queryByText('AI 工具与能力管理')).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole('button',{name:'折叠侧边栏'}));
    expect(screen.getByRole('button',{name:'共享中心'})).toBeInTheDocument();
    expect(screen.getByRole('button',{name:/Claude Code/})).toBeInTheDocument();
  });

  it("toggles the repository from its title and restores its resized height",()=>{
    localStorage.setItem('skillshub.sidebar.repositoryHeight','240');
    renderSidebar();
    const title=screen.getByRole('button',{name:'技能仓库'});
    const region=screen.getByTestId('repository-tree-region');
    expect(region).toHaveStyle({height:'240px'});
    fireEvent.click(title);
    expect(title).toHaveAttribute('aria-expanded','true');
    fireEvent.doubleClick(title);
    expect(title).toHaveAttribute('aria-expanded','false');
    expect(screen.queryByText('未分类')).not.toBeInTheDocument();
    expect(region.style.height).toBe('');
    expect(screen.getByRole('button',{name:'技能安装'})).toHaveAttribute('aria-expanded','true');
    fireEvent.doubleClick(title);
    expect(title).toHaveAttribute('aria-expanded','true');
    expect(screen.getByText('未分类')).toBeInTheDocument();
    expect(region).toHaveStyle({height:'240px'});
  });

  it("creates repository groups from the sidebar", () => {
    renderSidebar();
    expect(screen.queryByRole("button", { name: /新建分组/ })).not.toBeInTheDocument();
    fireEvent.contextMenu(screen.getByRole("button",{name:"技能仓库"}));
    expect(screen.getByRole("menuitem",{name:"新建分组"})).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /导入技能集/i })).not.toBeInTheDocument();
  });

  it("renders Settings in the bottom utility area", () => {
    renderSidebar();
    expect(screen.getByRole("button", { name: /设置/ })).toBeInTheDocument();
  });

  it("renders and handles the theme button in the bottom utility area", () => {
    renderSidebar();
    fireEvent.click(screen.getByRole("button", { name: /切换主题|Cycle theme/i }));
    expect(mockCycleThemeMode).toHaveBeenCalledTimes(1);
  });

  it("does not render legacy section headers", () => {
    renderSidebar();
    // No "By Tool" header
    expect(screen.queryByText("按工具")).not.toBeInTheDocument();
    // No "+新建" text button
    expect(screen.queryByText("+ 新建")).not.toBeInTheDocument();
  });

  // ── Loading State ─────────────────────────────────────────────────────────

  it("shows loading spinner when isLoading is true", () => {
    vi.mocked(usePlatformStore).mockReturnValue({
      ...defaultStoreState,
      isLoading: true,
    });
    render(
      <MemoryRouter>
        <Sidebar />
      </MemoryRouter>
    );
    // Should show a spinner (Loader2 with animate-spin)
    const spinner = document.querySelector(".animate-spin");
    expect(spinner).toBeInTheDocument();
  });

  it("hides platform buttons when loading", () => {
    vi.mocked(usePlatformStore).mockReturnValue({
      ...defaultStoreState,
      isLoading: true,
    });
    render(
      <MemoryRouter>
        <Sidebar />
      </MemoryRouter>
    );
    expect(screen.queryByRole("button", { name: /Claude Code/ })).not.toBeInTheDocument();
  });

  // ── Active Route Highlighting ─────────────────────────────────────────────

  it("highlights active platform route in sidebar", () => {
    renderSidebar("/platform/claude-code");
    const claudeButton = screen.getByRole("button", { name: /Claude Code/ });
    expect(claudeButton.className).toContain("bg-hover-bg");
  });

  it("highlights Shared Hub when on /central", () => {
    renderSidebar("/central");
    const centralButton = screen.getByRole("button", { name: /共享中心/ });
    expect(centralButton.className).toContain("bg-hover-bg");
  });

  it("shows the central skill count from the central library state", () => {
    renderSidebar("/central", {
      centralSkillsCount: 2,
      platformState: {
        ...defaultStoreState,
        skillsByAgent: { ...defaultStoreState.skillsByAgent, central: 0 },
      },
    });

    const centralButton = screen.getByRole("button", { name: /共享中心|Shared Hub/i });
    expect(within(centralButton).getByText("2")).toBeInTheDocument();
  });

  it("highlights Skill Repository when on root route", () => {
    renderSidebar("/");
    const resourceButton = screen.getByRole("button", { name: "技能仓库" });
    const centralButton = screen.getByRole("button", { name: /共享中心/ });
    expect(resourceButton.className).toContain("bg-hover-bg");
    expect(centralButton.className).not.toContain("bg-hover-bg");
  });

  it("highlights Settings in the sidebar", () => {
    renderSidebar("/settings");
    const settingsButton = screen.getByRole("button", { name: /设置/ });
    expect(settingsButton).toHaveAttribute("aria-current", "page");
  });

  // ── Empty States ──────────────────────────────────────────────────────────

  it("shows no platform buttons when only central agent exists", () => {
    vi.mocked(usePlatformStore).mockReturnValue({
      ...defaultStoreState,
      agents: [
        {
          id: "central",
          display_name: "Shared Hub",
          global_skills_dir: "~/.agents/skills/",
          is_detected: true,
          is_builtin: true,
          is_enabled: true,
        },
      ],
    });
    render(
      <MemoryRouter>
        <Sidebar />
      </MemoryRouter>
    );
    expect(screen.queryByRole("button", { name: /Claude Code/ })).not.toBeInTheDocument();
  });

  it("shows enabled agents with zero skills", () => {
    vi.mocked(usePlatformStore).mockReturnValue({
      ...defaultStoreState,
      skillsByAgent: {
        "claude-code": 0,
        cursor: 3,
        central: 10,
      },
    });
    render(
      <MemoryRouter>
        <Sidebar />
      </MemoryRouter>
    );
    expect(within(screen.getByText("软件平台").closest("button")!).getByText(String(defaultStoreState.agents.filter(a => a.id !== "central" && !a.id.startsWith("project:") && a.is_enabled).length))).toBeInTheDocument();
    expect(within(screen.getByRole("button", { name: /Claude Code/ })).getByText("0")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /Cursor/ })).toBeInTheDocument();
  });

  it("hides disabled agents even when they still have cached skills", () => {
    vi.mocked(usePlatformStore).mockReturnValue({
      ...defaultStoreState,
      agents: mockAgents.map((agent) =>
        agent.id === "claude-code" ? { ...agent, is_enabled: false } : agent
      ),
      skillsByAgent: {
        "claude-code": 5,
        cursor: 3,
        central: 10,
      },
    });
    render(
      <MemoryRouter>
        <Sidebar />
      </MemoryRouter>
    );
    expect(screen.queryByRole("button", { name: /Claude Code/ })).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: /Cursor/ })).toBeInTheDocument();
  });

  it("defaults missing counts to zero and removes platforms when disabled", () => {
    window.localStorage.setItem("skills-manage:show-all-platforms", "true");
    const view = renderSidebar("/central", { platformState: {
      ...defaultStoreState, skillsByAgent: {},
    }});
    expect(within(screen.getByRole("button", { name: /Claude Code/ })).getByText("0")).toBeInTheDocument();
    vi.mocked(usePlatformStore).mockReturnValue({
      ...defaultStoreState,
      agents: mockAgents.map(agent => ({ ...agent, is_enabled: false })),
    });
    view.rerender(<MemoryRouter><Sidebar /><LocationProbe /></MemoryRouter>);
    expect(screen.queryByRole("button", { name: /Claude Code/ })).not.toBeInTheDocument();
  });

  // ── Navigation ────────────────────────────────────────────────────────────

  it("platform buttons are clickable", () => {
    renderSidebar();
    const claudeButton = screen.getByRole("button", { name: /Claude Code/ });
    expect(claudeButton).not.toBeDisabled();
    fireEvent.click(claudeButton);
  });

  it("Shared Hub button is clickable", () => {
    renderSidebar();
    const centralButton = screen.getByRole("button", { name: /共享中心/ });
    expect(centralButton).not.toBeDisabled();
    fireEvent.click(centralButton);
  });

  // ── Skill Bundles ─────────────────────────────────────────────────────────

  it("collections button navigates to /collections list page", () => {
    vi.mocked(useCollectionStore).mockImplementation((selector) =>
      selector({
        ...defaultCollectionState,
        collections: [
          { id: "col-1", name: "Frontend", created_at: "2026-04-09T00:00:00Z", updated_at: "2026-04-09T00:00:00Z" },
          { id: "col-2", name: "Backend", created_at: "2026-04-09T00:00:00Z", updated_at: "2026-04-09T00:00:00Z" },
        ],
      })
    );
    renderSidebar();
    expect(screen.queryByRole("button", { name: "技能分组" })).not.toBeInTheDocument();
    expect(screen.getByRole("tree",{name:"技能仓库"})).toBeInTheDocument();
  });

  it("highlights active collection route", () => {
    vi.mocked(useCollectionStore).mockImplementation((selector) =>
      selector({
        ...defaultCollectionState,
        collections: [
          { id: "col-1", name: "Frontend", created_at: "2026-04-09T00:00:00Z", updated_at: "2026-04-09T00:00:00Z" },
        ],
      })
    );
    vi.mocked(usePlatformStore).mockReturnValue(defaultStoreState);
    render(
      <MemoryRouter initialEntries={["/collections"]}>
        <Sidebar />
      </MemoryRouter>
    );
    // The collections icon button should be highlighted (exact match)
    fireEvent.click(screen.getByRole("button",{name:"Frontend 0/0"}));
    expect(defaultCollectionState.selectGroup).toHaveBeenCalledWith("col-1");
  });

  it("nests groups under the repository root before Shared Hub", () => {
    renderSidebar();
    const resourceButton = screen.getByRole("button", { name: "技能仓库" });
    const collectionsButton = screen.getByRole("tree", { name: "技能仓库" });
    const centralButton = screen.getByRole("button", { name: "共享中心" });

    expect(collectionsButton).toContainElement(resourceButton);
    expect(screen.queryByText("分组")).not.toBeInTheDocument();
    expect(
      collectionsButton.compareDocumentPosition(centralButton) &
        Node.DOCUMENT_POSITION_FOLLOWING
    ).toBeTruthy();
  });

  // ── Project directories ─────────────────────────────────────────────────

  it("renders project directories below software platforms", () => {
    renderSidebar("/central", {
      platformState: {
        ...defaultStoreState,
        agents: [
          ...defaultStoreState.agents,
          {
            id: "project:7",
            display_name: "Demo Project",
            global_skills_dir: "D:\\Projects\\Demo\\.agents\\skills",
            project_skills_dir: ".agents/skills",
            is_detected: true,
            is_builtin: false,
            is_enabled: true,
          },
        ],
        skillsByAgent: {
          ...defaultStoreState.skillsByAgent,
          "project:7": 2,
        },
      },
    });
    expect(screen.getByText("项目目录")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /Demo Project/ })).toBeInTheDocument();
  });

  it("indents software platform list like project directories", () => {
    renderSidebar("/central", {
      platformState: {
        ...defaultStoreState,
        agents: [
          ...defaultStoreState.agents,
          {
            id: "openclaw",
            display_name: "OpenClaw",
            global_skills_dir: "~/.openclaw/skills/",
            is_detected: true,
            is_builtin: true,
            is_enabled: true,
          },
          {
            id: "project:7",
            display_name: "Demo Project",
            global_skills_dir: "D:\\Projects\\Demo\\.agents\\skills",
            project_skills_dir: ".agents/skills",
            is_detected: true,
            is_builtin: false,
            is_enabled: true,
          },
        ],
        skillsByAgent: {
          ...defaultStoreState.skillsByAgent,
          openclaw: 1,
          "project:7": 2,
        },
      },
    });

    const nestedListClass = "ml-6 border-l border-sidebar-border/70 pl-2";
    expect(screen.queryByText("龙虾类")).not.toBeInTheDocument();
    expect(screen.queryByText("编程类")).not.toBeInTheDocument();
    const openClawButton = screen.getByRole("button", { name: /OpenClaw/ });
    const claudeButton = screen.getByRole("button", { name: /Claude Code/ });
    const projectButton = screen.getByRole("button", { name: /Demo Project/ });

    const platformList = openClawButton.closest("div.ml-6");
    const projectList = projectButton.closest("div.ml-6");

    expect(platformList).toHaveClass(...nestedListClass.split(" "));
    expect(platformList?.contains(claudeButton)).toBe(true);
    expect(projectList).toHaveClass(...nestedListClass.split(" "));
  });

  it("expands and collapses sidebar groups independently with plus and minus", () => {
    renderSidebar();
    const software = screen.getByText("软件平台").closest("button")!;
    const projects = screen.getByText("项目目录").closest("button")!;
    fireEvent.keyDown(software, {key:"-"});
    expect(software).toHaveAttribute("aria-expanded", "false");
    fireEvent.keyDown(software, {key:"+", shiftKey:true});
    expect(software).toHaveAttribute("aria-expanded", "true");
    fireEvent.keyDown(projects, {key:"-"});
    expect(projects).toHaveAttribute("aria-expanded", "false");
    expect(software).toHaveAttribute("aria-expanded", "true");
    fireEvent.keyDown(projects, {key:"+", code:"NumpadAdd"});
    expect(projects).toHaveAttribute("aria-expanded", "true");
  });

  it("expands and collapses sidebar groups independently with arrow keys", () => {
    renderSidebar();
    const software = screen.getByText("软件平台").closest("button")!;
    const projects = screen.getByText("项目目录").closest("button")!;
    fireEvent.keyDown(software, {key:"ArrowLeft"});
    expect(software).toHaveAttribute("aria-expanded", "false");
    fireEvent.keyDown(software, {key:"ArrowRight"});
    expect(software).toHaveAttribute("aria-expanded", "true");
    fireEvent.keyDown(projects, {key:"ArrowLeft"});
    expect(projects).toHaveAttribute("aria-expanded", "false");
    expect(software).toHaveAttribute("aria-expanded", "true");
    fireEvent.keyDown(projects, {key:"ArrowRight"});
    expect(projects).toHaveAttribute("aria-expanded", "true");
  });

  it("keeps only collapse buttons in platform and project headings", () => {
    renderSidebar();
    expect(within(screen.getByTestId("software-platform-heading")).getAllByRole("button")).toHaveLength(1);
    expect(within(screen.getByTestId("project-directories-heading")).getAllByRole("button")).toHaveLength(1);
    expect(screen.queryByRole("button", { name: "显示所有平台" })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "显示空项目目录" })).not.toBeInTheDocument();
  });

  it("renders the software platform heading like a primary sidebar title with an icon", () => {
    renderSidebar();

    const heading = screen.getByText("软件平台");
    const titleRow = heading.closest("[data-testid='software-platform-heading']");
    expect(titleRow).toBeInTheDocument();
    expect(titleRow?.querySelector("svg")).toBeInTheDocument();
    expect(heading.className).toContain("text-sm");
    expect(heading.className).toContain("font-semibold");
  });

  it("uses a distinct icon for the software platform heading and platform items", () => {
    renderSidebar();

    const headingIcon = screen.getByTestId("software-platform-heading-icon");
    const claudeButton = screen.getByRole("button", { name: /Claude Code/ });
    const platformIcon = claudeButton.querySelector("svg");

    expect(headingIcon).toBeInTheDocument();
    expect(platformIcon).toBeInTheDocument();
    expect(headingIcon).not.toEqual(platformIcon);
    expect(headingIcon).not.toHaveClass("text-primary");
  });

  it("places central skills below the divider with software platforms and project directories", () => {
    renderSidebar("/central", {
      platformState: {
        ...defaultStoreState,
        agents: [
          ...mockAgents,
          {
            id: "openclaw",
            display_name: "OpenClaw",
            global_skills_dir: "~/.openclaw/skills/",
            is_detected: true,
            is_builtin: true,
            is_enabled: true,
          },
        ],
        skillsByAgent: {
          ...defaultStoreState.skillsByAgent,
          openclaw: 1,
        } as Record<string, number>,
      },
    });

    const collections = screen.getByRole("tree", { name: "技能仓库" });
    const central = screen.getByRole("button", { name: /共享中心|Shared Hub/i });
    const platformHeading = screen.getByText("软件平台");
    const projectHeading = screen.getByText("项目目录");

    expect(screen.queryByText("龙虾类")).not.toBeInTheDocument();
    expect(screen.queryByText("编程类")).not.toBeInTheDocument();
    expect(collections.compareDocumentPosition(central)).toBe(Node.DOCUMENT_POSITION_FOLLOWING);
    expect(central.compareDocumentPosition(platformHeading)).toBe(Node.DOCUMENT_POSITION_FOLLOWING);
    expect(platformHeading.compareDocumentPosition(projectHeading)).toBe(Node.DOCUMENT_POSITION_FOLLOWING);
    expect(central.closest("[data-testid='central-skills-heading']")).toBeTruthy();
    expect(platformHeading.closest("[data-testid='software-platform-heading']")).toBeTruthy();
    expect(projectHeading.closest("[data-testid='project-directories-heading']")).toBeTruthy();
  });

  it("shows empty platforms and project directories with zero counts", () => {
    renderSidebar("/central", {
      platformState: {
        ...defaultStoreState,
        agents: [
          ...defaultStoreState.agents,
          {
            id: "project:empty",
            display_name: "Empty Project",
            global_skills_dir: "D:\\Projects\\Empty\\.agents\\skills",
            project_skills_dir: ".agents/skills",
            is_detected: true,
            is_builtin: false,
            is_enabled: true,
          },
        ],
        skillsByAgent: {
          "claude-code": 0,
          cursor: 3,
          central: 10,
          "project:empty": 0,
        },
      },
    });

    expect(within(screen.getByRole("button", { name: /Claude Code/ })).getByText("0")).toBeInTheDocument();
    expect(within(screen.getByRole("button", { name: /Empty Project/ })).getByText("0")).toBeInTheDocument();
  });

  it("marks independent and shared software platforms after the name", () => {
    renderSidebar("/central", {
      platformState: {
        ...defaultStoreState,
        agents: [
          { ...mockAgents[0], shares_central_skills: false },
          { ...mockAgents[1], shares_central_skills: true },
          mockAgents[2],
        ],
      },
    });

    const independent = screen.getByRole("button", { name: /Claude Code — 独立目录/ });
    const shared = screen.getByRole("button", { name: /Cursor — 共享目录/ });
    expect(independent).toHaveAttribute(
      "title",
      expect.stringContaining("该平台使用独立的技能目录")
    );
    expect(shared).toHaveAttribute(
      "title",
      expect.stringContaining("该平台技能目录与共享中心指向同一位置")
    );
    expect(screen.getByRole("button", { name: "共享中心" })).toBeInTheDocument();
  });

  // ── Collapse Toggle ───────────────────────────────────────────────────────

  it("renders collapse toggle button", () => {
    renderSidebar();
    // Default is expanded, so the button label is "collapse"
    expect(screen.getByRole("button", { name: /折叠侧边栏/i })).toBeInTheDocument();
  });

  it("exports the resource directory list from the resource nav context menu", async () => {
    mockSaveDialog.mockResolvedValue("D:\\exports\\skill-directory-list.csv");
    mockExportDirectoryList.mockResolvedValue(3);

    renderSidebar("/resources");

    fireEvent.contextMenu(screen.getByRole("button", { name: "技能仓库" }));
    fireEvent.click(screen.getByRole("menuitem", { name: "导出目录列表" }));

    await waitFor(() => {
      expect(mockExportDirectoryList).toHaveBeenCalledWith("D:\\exports\\skill-directory-list.csv");
    });
  });
});

it('uses a 40px collapsed rail and stacks theme above settings',()=>{
 renderSidebar();fireEvent.click(screen.getByRole('button',{name:'折叠侧边栏'}));
 expect(screen.getByRole('navigation',{name:'主导航'})).toHaveStyle({width:'40px'});
 const theme=screen.getByRole('button',{name:/切换主题/});const settings=screen.getByRole('button',{name:'设置'});
 expect(theme.parentElement).toBe(settings.parentElement);expect(theme.parentElement).toHaveClass('flex-col');
 expect(theme).toHaveClass('size-8');expect(settings).toHaveClass('size-8');
});

it('resizes the repository region and retains its height',()=>{
 useSidebarStore.setState({expanded:true});
 localStorage.setItem('skillshub.sidebar.repositoryHeight','160');
 renderSidebar();
 const handle=screen.getByRole('separator',{name:'调整技能仓库区域高度'});
 fireEvent.keyDown(handle,{key:'ArrowDown'});
 expect(screen.getByTestId('repository-tree-region')).toHaveStyle({height:'176px'});
 expect(localStorage.getItem('skillshub.sidebar.repositoryHeight')).toBe('176');
 fireEvent.click(screen.getByRole('button',{name:'折叠侧边栏'}));
 fireEvent.click(screen.getByRole('button',{name:'技能仓库'}));
 expect(screen.getByRole('button',{name:'展开侧边栏'})).toBeInTheDocument();
});
