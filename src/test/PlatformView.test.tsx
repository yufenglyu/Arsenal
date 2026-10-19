import { openRowActions } from "./rowActions";
import { toast } from "sonner";
import { describe, it, expect, vi, beforeEach } from "vitest";
import { render as rtlRender, screen, fireEvent, waitFor, within } from "@testing-library/react";
import {
  MemoryRouter,
  Route,
  Routes,
} from "react-router-dom";
import { PlatformView } from "../pages/PlatformView";
import { AgentWithStatus, ScannedSkill } from "../types";

// Mock stores
vi.mock("../stores/platformStore", () => ({
  usePlatformStore: vi.fn(),
}));

vi.mock("../stores/skillStore", () => ({
  useSkillStore: vi.fn(),
}));

vi.mock("../stores/centralSkillsStore", () => ({
  BROWSER_FIXTURE_AGENTS: [],
  useCentralSkillsStore: vi.fn(),
}));

vi.mock("../components/skill/SkillDetailDrawer", () => ({
  SkillDetailDrawer: ({
    open,
    skillId,
    agentId,
    rowId,
    onOpenChange,
    returnFocusRef,
  }: {
    open: boolean;
    skillId: string | null;
    agentId?: string | null;
    rowId?: string | null;
    onOpenChange: (open: boolean) => void;
    returnFocusRef?: { current: HTMLElement | null };
  }) =>
    open ? (
      <div data-testid="skill-detail-drawer">
        <div>drawer-skill:{skillId}</div>
        <div>drawer-agent:{agentId ?? "none"}</div>
        <div>drawer-row:{rowId ?? "none"}</div>
        <button
          onClick={() => {
            onOpenChange(false);
            returnFocusRef?.current?.focus();
          }}
        >
          Close drawer
        </button>
      </div>
    ) : null,
}));

import { usePlatformStore } from "../stores/platformStore";
import { useSkillStore } from "../stores/skillStore";
import { useCentralSkillsStore } from "../stores/centralSkillsStore";
import * as tauriBridge from "@/lib/tauri";


// ─── Fixtures ─────────────────────────────────────────────────────────────────

const mockAgent: AgentWithStatus = {
  id: "claude-code",
  display_name: "Claude Code",
  global_skills_dir: "/Users/test/.claude/skills/",
  is_detected: true,
  is_builtin: true,
  is_enabled: true,
};



const mockSkills: ScannedSkill[] = [
  {
    id: "frontend-design",
    name: "frontend-design",
    description: "Build distinctive, production-grade frontend interfaces",
    file_path: "~/.claude/skills/frontend-design/SKILL.md",
    dir_path: "~/.claude/skills/frontend-design",
    link_type: "symlink",
    symlink_target: "~/.agents/skills/frontend-design",
    installation_source: "independent",
    is_central: true,
  },
  {
    id: "code-reviewer",
    name: "code-reviewer",
    description: "Review code changes and identify high-confidence actionable bugs",
    file_path: "~/.claude/skills/code-reviewer/SKILL.md",
    dir_path: "~/.claude/skills/code-reviewer",
    link_type: "copy",
    installation_source: "independent",
    is_central: false,
  },
  {
    id: "resource-linked-skill",
    name: "resource-linked-skill",
    description: "Installed directly from the skill repository",
    file_path: "~/.claude/skills/resource-linked-skill/SKILL.md",
    dir_path: "~/.claude/skills/resource-linked-skill",
    link_type: "symlink",
    symlink_target: "~/Skills/resource-linked-skill",
    installation_source: "independent",
    is_central: false,
    source: "resource-library",
  },
];





const mockCompatibilityCentralSkills: ScannedSkill[] = [
  {
    id: "algorithmic-art",
    row_id: "amp::compatibility::algorithmic-art",
    name: "algorithmic-art",
    description: "Creating algorithmic art",
    file_path: "/Users/test/.agents/skills/anthropics/algorithmic-art/SKILL.md",
    dir_path: "/Users/test/.agents/skills/anthropics/algorithmic-art",
    link_type: "copy",
    is_central: true,
    source_kind: "compatibility",
    source_root: "/Users/test/.agents/skills",
    installation_source: "shared",
    is_read_only: true,
  },
  {
    id: "defuddle",
    row_id: "amp::compatibility::defuddle",
    name: "defuddle",
    description: "Extract clean markdown",
    file_path: "/Users/test/.agents/skills/kepano/defuddle/SKILL.md",
    dir_path: "/Users/test/.agents/skills/kepano/defuddle",
    link_type: "copy",
    is_central: true,
    source_kind: "compatibility",
    source_root: "/Users/test/.agents/skills",
    installation_source: "shared",
    is_read_only: true,
  },
];

const mockDuplicatePlatformSkills: ScannedSkill[] = [
  {
    id: "shared-skill",
    row_id: "claude-code::shared-skill",
    name: "shared-skill",
    description: "Platform copy",
    file_path: "~/.claude/skills/shared-skill/SKILL.md",
    dir_path: "~/.claude/skills/shared-skill",
    link_type: "native",
    installation_source: "independent",
    is_central: false,
    source_kind: null,
    source_root: null,
    is_read_only: false,
    conflict_count: 2,
  },
  {
    id: "shared-skill",
    row_id: "claude-code::compatibility::shared-skill",
    name: "shared-skill",
    description: "Shared Hub visible copy",
    file_path: "~/.agents/skills/shared-skill/SKILL.md",
    dir_path: "~/.agents/skills/shared-skill",
    link_type: "native",
    is_central: false,
    source_kind: "compatibility",
    source_root: "~/.agents/skills",
    installation_source: "shared",
    is_read_only: true,
    conflict_count: 2,
  },
];

const mockDuplicatePlatformSkillsWithDistinctIds: ScannedSkill[] = [
  {
    id: "shared-skill-id",
    row_id: "claude-code::shared-skill-id",
    name: "Shared skill",
    description: "Platform copy",
    file_path: "~/.claude/skills/shared-skill/SKILL.md",
    dir_path: "~/.claude/skills/shared-skill",
    link_type: "native",
    installation_source: "independent",
    is_central: false,
    source_kind: null,
    source_root: null,
    is_read_only: false,
    conflict_count: 2,
  },
  {
    id: "shared-skill-id",
    row_id: "claude-code::compatibility::shared-skill-id",
    name: "Shared skill",
    description: "Shared Hub visible copy",
    file_path: "~/.agents/skills/shared-skill/SKILL.md",
    dir_path: "~/.agents/skills/shared-skill",
    link_type: "native",
    is_central: false,
    source_kind: "compatibility",
    source_root: "~/.agents/skills",
    installation_source: "shared",
    is_read_only: true,
    conflict_count: 2,
  },
];

const mockGetSkillsByAgent = vi.fn();
const mockLoadCentralSkills = vi.fn();
const mockInstallSkill = vi.fn();
const mockUninstallSkillFromAgent = vi.fn();
const mockRefreshCounts = vi.fn();
const mockUsePlatformStore = vi.mocked(usePlatformStore);
const mockUseSkillStore = vi.mocked(useSkillStore);
const mockUseCentralSkillsStore = vi.mocked(useCentralSkillsStore);

function buildPlatformStoreState(overrides = {}) {
  return {
    agents: [mockAgent],
    skillsByAgent: { "claude-code": 2 },
    isLoading: false,
    isRefreshing: false,
    scanGeneration: 1,
    error: null,
    initialize: vi.fn(),
    rescan: vi.fn(),
    refreshCounts: mockRefreshCounts,
    ...overrides,
  };
}

function buildSkillStoreState(overrides = {}) {
  return {
    skillsByAgent: { "claude-code": mockSkills },
    loadingByAgent: { "claude-code": false },
    pendingSkillActionKeys: {},
    error: null,
    getSkillsByAgent: mockGetSkillsByAgent,
    uninstallSkillFromAgent: mockUninstallSkillFromAgent,
    ...overrides,
  };
}

function buildCentralSkillsStoreState(overrides = {}) {
  return {
    skills: [],
    agents: [mockAgent],
    loadCentralSkills: mockLoadCentralSkills,
    installSkill: mockInstallSkill,
    ...overrides,
  };
}

function installDefaultStoreMocks() {
  mockUsePlatformStore.mockImplementation((selector?: unknown) => {
    const state = buildPlatformStoreState();
    if (typeof selector === "function") return selector(state);
    return state;
  });
  mockUseSkillStore.mockImplementation((selector?: unknown) => {
    const state = buildSkillStoreState();
    if (typeof selector === "function") return selector(state);
    return state;
  });
  mockUseCentralSkillsStore.mockImplementation((selector?: unknown) => {
    const state = buildCentralSkillsStoreState();
    if (typeof selector === "function") return selector(state);
    return state;
  });
}

function renderPlatformView(agentId = "claude-code") {
  return render(
    <MemoryRouter initialEntries={[`/platform/${agentId}`]}>
      <Routes>
        <Route path="/platform/:agentId" element={<PlatformView />} />
      </Routes>
    </MemoryRouter>
  );
}





// ─── Tests ────────────────────────────────────────────────────────────────────

describe("PlatformView", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    localStorage.removeItem("skills-manage.skillTableColumns.tree.v5");
    window.localStorage.clear();
    mockRefreshCounts.mockReset();
    mockUninstallSkillFromAgent.mockReset();
    installDefaultStoreMocks();
  });

  // ── Header ────────────────────────────────────────────────────────────────

  it.each(["claude-code", "project:1"])("filters %s skills by sidebar tags and combines with search", (agentId) => {
    const agent = {...mockAgent,id:agentId};
    const platformState = buildPlatformStoreState({agents:[agent]});
    const skillState = buildSkillStoreState({skillsByAgent:{[agentId]:mockSkills}});
    const centralState = buildCentralSkillsStoreState({skills:mockSkills.map(skill => ({
      ...skill, tags:skill.id === "frontend-design" ? ["Frontend"] : ["Review"], linked_agents:[], read_only_agents:[],
    }))});
    mockUsePlatformStore.mockImplementation((selector?: unknown) => typeof selector === "function" ? selector(platformState) : platformState);
    mockUseSkillStore.mockImplementation((selector?: unknown) => typeof selector === "function" ? selector(skillState) : skillState);
    mockUseCentralSkillsStore.mockImplementation((selector?: unknown) => typeof selector === "function" ? selector(centralState) : centralState);
    renderPlatformView(encodeURIComponent(agentId));
    const tags = within(screen.getByRole("group",{name:"标签"}));
    const frontend = tags.getByRole("button",{name:"Frontend"});
    fireEvent.click(frontend);
    expect(frontend).toHaveAttribute("aria-pressed","true");
    expect(screen.getByRole("button",{name:"查看 frontend-design 的详情"})).toBeInTheDocument();
    expect(screen.queryByRole("button",{name:"查看 code-reviewer 的详情"})).not.toBeInTheDocument();
    const search = screen.getByRole("textbox");
    fireEvent.change(search,{target:{value:"code-reviewer"}});
    expect(screen.queryByRole("button",{name:"查看 frontend-design 的详情"})).not.toBeInTheDocument();
    fireEvent.change(search,{target:{value:""}});
    fireEvent.click(frontend,{ctrlKey:true});
    expect(frontend).toHaveAttribute("aria-pressed","false");
    expect(screen.getByRole("button",{name:"查看 code-reviewer 的详情"})).toBeInTheDocument();
  });

  it("shows platform name in header", () => {
    renderPlatformView();
    expect(screen.getByText("Claude Code")).toBeInTheDocument();
  });

  it("shows platform directory path in header", () => {
    renderPlatformView();
    expect(screen.getByRole("button", {name: "在文件管理器中打开: /Users/test/.claude/skills/"})).toBeInTheDocument();
  });

  it("opens the software platform directory from the header path", async () => {
    const invokeSpy = vi.spyOn(tauriBridge, "invoke").mockResolvedValue(undefined);
    renderPlatformView();

    fireEvent.click(
      screen.getByRole("button", {
        name: "在文件管理器中打开: /Users/test/.claude/skills/",
      })
    );

    await waitFor(() => {
      expect(invokeSpy).toHaveBeenCalledWith("open_in_file_manager", {
        path: "/Users/test/.claude/skills/",
      });
    });
    invokeSpy.mockRestore();
  });

  it("opens a project directory path from the header", async () => {
    const projectAgent: AgentWithStatus = {
      id: "project:1",
      display_name: "temp",
      global_skills_dir: "/Users/test/Projects/temp/.agents/skills",
      project_skills_dir: ".agents/skills",
      is_detected: true,
      is_builtin: false,
      is_enabled: true,
    };
    mockUsePlatformStore.mockImplementation((selector?: unknown) => {
      const state = buildPlatformStoreState({
        agents: [projectAgent],
        skillsByAgent: { "project:1": 0 },
      });
      if (typeof selector === "function") return selector(state);
      return state;
    });
    mockUseSkillStore.mockImplementation((selector?: unknown) => {
      const state = buildSkillStoreState({
        skillsByAgent: { "project:1": [] },
        loadingByAgent: { "project:1": false },
      });
      if (typeof selector === "function") return selector(state);
      return state;
    });

    const invokeSpy = vi.spyOn(tauriBridge, "invoke").mockResolvedValue(undefined);
    renderPlatformView(encodeURIComponent("project:1"));

    fireEvent.click(
      screen.getByRole("button", {
        name: "在文件管理器中打开: /Users/test/Projects/temp/.agents/skills",
      })
    );

    await waitFor(() => {
      expect(invokeSpy).toHaveBeenCalledWith("open_in_file_manager", {
        path: "/Users/test/Projects/temp/.agents/skills",
      });
    });
    invokeSpy.mockRestore();
  });

  // ── Skill List ────────────────────────────────────────────────────────────

  it("renders skill cards for all skills", () => {
    renderPlatformView();
    expect(screen.getByText("frontend-design")).toBeInTheDocument();
    expect(screen.getByText("code-reviewer")).toBeInTheDocument();
  });





  it("groups read-only central compatibility skills by their central source root", () => {
    window.localStorage.setItem("skills-manage.skillListViewMode.platform", "folders");
    mockUseSkillStore.mockImplementation((selector?: unknown) => {
      const state = buildSkillStoreState({
        skillsByAgent: { "claude-code": mockCompatibilityCentralSkills },
      });
      if (typeof selector === "function") return selector(state);
      return state;
    });

    renderPlatformView();
    fireEvent.keyDown(screen.getByRole("region",{name:"技能列表"}),{key:"-"});

    expect(screen.getByText("anthropics")).toBeInTheDocument();
    expect(screen.getByText("kepano")).toBeInTheDocument();
    expect(
      screen.queryByRole("button", { name: /查看 algorithmic-art 的详情/i })
    ).not.toBeInTheDocument();
  });



  it("shows per-row installation sources and both sources for a mixed folder", () => {
    localStorage.setItem("skills-manage.skillTableColumns.tree.v5", JSON.stringify(["index","name","source","skillCount","createdAt","updatedAt","installSummary","actions"]));
    mockUseSkillStore.mockImplementation((selector?: unknown) => {
      const state = buildSkillStoreState({
        skillsByAgent: { "claude-code": mockDuplicatePlatformSkills.map((skill) => ({ ...skill, is_read_only: false, source_repo: "example/skills" })) },
      });
      return typeof selector === "function" ? selector(state) : state;
    });
    renderPlatformView();
    expect(screen.getByText("独立安装")).toBeInTheDocument();
    expect(screen.getByText("共享中心")).toBeInTheDocument();
    fireEvent.keyDown(screen.getByRole("region",{name:"技能列表"}),{key:"-"});
    expect(screen.getByText("独立安装、共享中心")).toBeInTheDocument();
    expect(within(screen.getByRole("table")).queryByText(/个技能/)).not.toBeInTheDocument();
    expect(screen.getByRole("columnheader", { name: "安装来源" })).toBeInTheDocument();
    fireEvent.contextMenu(screen.getByRole("columnheader", { name: "安装来源" }));
    expect(screen.getByRole("checkbox", { name: "安装来源" })).toBeInTheDocument();
  });







  it("keeps platform source status labels out of flat table rows", () => {
    renderPlatformView();
    expect(screen.queryByText("链接到共享中心")).not.toBeInTheDocument();
    expect(screen.queryByText("本平台副本")).not.toBeInTheDocument();
    expect(screen.queryByText("链接到仓库")).not.toBeInTheDocument();
  });

  it("labels a shared Shared Hub symlink as linked from the skill repository", () => {
    const antigravityAgent: AgentWithStatus = {
      id: "antigravity",
      display_name: "Antigravity",
      global_skills_dir: "/Users/test/.agents/skills/",
      is_detected: true,
      is_builtin: true,
      is_enabled: true,
    };
    mockUsePlatformStore.mockImplementation((selector?: unknown) => {
      const state = buildPlatformStoreState({
        agents: [antigravityAgent],
        skillsByAgent: { antigravity: 1 },
      });
      if (typeof selector === "function") return selector(state);
      return state;
    });
    mockUseSkillStore.mockImplementation((selector?: unknown) => {
      const state = buildSkillStoreState({
        skillsByAgent: {
          antigravity: [
            {
              id: "promoted-skill",
              name: "promoted-skill",
              description: "Promoted from the skill repository",
              file_path: "~/.agents/skills/promoted-skill/SKILL.md",
              dir_path: "~/.agents/skills/promoted-skill",
              link_type: "symlink",
              symlink_target: "~/.arsenal/library/promoted-skill",
              installation_source: "independent",
    is_central: true,
              source: "resource-library",
            },
          ],
        },
      });
      if (typeof selector === "function") return selector(state);
      return state;
    });

    renderPlatformView("antigravity");

    expect(screen.queryByText("链接到仓库")).not.toBeInTheDocument();
    expect(
      screen.queryByText((_, element) => element?.textContent?.replace(/\s+/g, " ").trim() === "文件在共享中心")
    ).not.toBeInTheDocument();
  });

  it("renders browser fixture installed card on the localhost validation surface without Tauri", async () => {
    const isTauriSpy = vi.spyOn(tauriBridge, "isTauriRuntime").mockReturnValue(false);

    mockUseSkillStore.mockImplementation((selector?: unknown) => {
      const state = buildSkillStoreState({
        skillsByAgent: {
          "claude-code": [
            {
              id: "fixture-central-skill",
              name: "fixture-central-skill",
              description: "Browser fixture skill sourced from the central library",
              file_path: "~/.claude/skills/fixture-central-skill/SKILL.md",
              dir_path: "~/.claude/skills/fixture-central-skill",
              link_type: "symlink",
              symlink_target: "~/.agents/skills/fixture-central-skill",
              installation_source: "independent",
    is_central: true,
            },
          ],
        },
      });
      if (typeof selector === "function") return selector(state);
      return state;
    });

    render(
      <MemoryRouter initialEntries={["/platform/claude-code"]}>
        <Routes>
          <Route path="/platform/:agentId" element={<PlatformView />} />
        </Routes>
      </MemoryRouter>
    );

    expect(await screen.findByRole("button", { name: /查看 fixture-central-skill 的详情/i })).toBeInTheDocument();
    expect(screen.queryByText("链接到共享中心")).not.toBeInTheDocument();

    isTauriSpy.mockRestore();
  });

  // ── Empty State ───────────────────────────────────────────────────────────

  it("shows empty state when platform has no skills", () => {
    mockUsePlatformStore.mockImplementation((selector?: unknown) => {
      const state = buildPlatformStoreState({
        skillsByAgent: { "claude-code": 0 },
      });
      if (typeof selector === "function") return selector(state);
      return state;
    });
    mockUseSkillStore.mockImplementation((selector?: unknown) => {
      const state = buildSkillStoreState({
        skillsByAgent: { "claude-code": [] },
      });
      if (typeof selector === "function") return selector(state);
      return state;
    });

    render(
      <MemoryRouter initialEntries={["/platform/claude-code"]}>
        <Routes>
          <Route path="/platform/:agentId" element={<PlatformView />} />
        </Routes>
      </MemoryRouter>
    );

    expect(
      screen.getByText(/没有匹配的技能/)
    ).toBeInTheDocument();
  });

  // ── Platform Not Found ────────────────────────────────────────────────────

  it("shows not found when agent doesn't exist", () => {
    mockUsePlatformStore.mockImplementation((selector?: unknown) => {
      const state = buildPlatformStoreState({ agents: [] });
      if (typeof selector === "function") return selector(state);
      return state;
    });
    mockUseSkillStore.mockImplementation((selector?: unknown) => {
      const state = buildSkillStoreState({ skillsByAgent: {} });
      if (typeof selector === "function") return selector(state);
      return state;
    });

    render(
      <MemoryRouter initialEntries={["/platform/unknown"]}>
        <Routes>
          <Route path="/platform/:agentId" element={<PlatformView />} />
        </Routes>
      </MemoryRouter>
    );

    expect(screen.getByText("未找到平台")).toBeInTheDocument();
  });

  // ── Search / Filter ───────────────────────────────────────────────────────

  it("renders search input", () => {
    renderPlatformView();
    expect(
      screen.getByPlaceholderText(/搜索技能/)
    ).toBeInTheDocument();
  });

  it("filters skills by name when searching", async () => {
    renderPlatformView();
    const searchInput = screen.getByPlaceholderText(/搜索技能/);
    fireEvent.change(searchInput, { target: { value: "frontend" } });

    await waitFor(() => {
      expect(screen.getByText("frontend-design")).toBeInTheDocument();
      expect(screen.queryByText("code-reviewer")).not.toBeInTheDocument();
    });
  });

  it("filters skills by description when searching", async () => {
    renderPlatformView();
    const searchInput = screen.getByPlaceholderText(/搜索技能/);
    fireEvent.change(searchInput, { target: { value: "actionable" } });

    await waitFor(() => {
      expect(screen.getByText("code-reviewer")).toBeInTheDocument();
      expect(screen.queryByText("frontend-design")).not.toBeInTheDocument();
    });
  });

  it("shows all skills when search is cleared", async () => {
    renderPlatformView();
    const searchInput = screen.getByPlaceholderText(/搜索技能/);
    fireEvent.change(searchInput, { target: { value: "frontend" } });
    fireEvent.change(searchInput, { target: { value: "" } });

    await waitFor(() => {
      expect(screen.getByText("frontend-design")).toBeInTheDocument();
      expect(screen.getByText("code-reviewer")).toBeInTheDocument();
    });
  });

  it("shows empty state message when search has no results", async () => {
    renderPlatformView();
    const searchInput = screen.getByPlaceholderText(/搜索技能/);
    fireEvent.change(searchInput, { target: { value: "nonexistent-skill-xyz" } });

    await waitFor(() => {
      expect(screen.getByText(/没有匹配的技能/)).toBeInTheDocument();
    });
  });

  // ── Data Loading ──────────────────────────────────────────────────────────

  it("calls getSkillsByAgent on mount", () => {
    renderPlatformView();
    expect(mockGetSkillsByAgent).toHaveBeenCalledWith("claude-code");
  });





  it("shows duplicate platform rows with read-only list treatment", () => {
    mockUseSkillStore.mockImplementation((selector?: unknown) => {
      const state = buildSkillStoreState({
        skillsByAgent: { "claude-code": mockDuplicatePlatformSkills },
      });
      if (typeof selector === "function") return selector(state);
      return state;
    });

    renderPlatformView();

    expect(screen.getAllByRole("button", { name: /查看 shared-skill 的详情/i })).toHaveLength(2);

    const rows = screen
      .getAllByRole("button", { name: /查看 shared-skill 的详情/i })
      .map((button) => button.closest("tr"))
      .filter((row): row is HTMLTableRowElement => row !== null);
    const userCard = rows[0];
    const pluginCard = rows[1];

    expect(userCard).not.toBeNull();
    expect(pluginCard).not.toBeNull();

    if (!userCard || !pluginCard) {
      return;
    }

    expect(
      openRowActions(userCard as HTMLElement).queryByRole("menuitem", { name: /将 shared-skill 安装到平台/i,
      })
    ).not.toBeInTheDocument();
    expect(
      openRowActions(userCard as HTMLElement).getByRole("menuitem", { name: /卸载/i,
      })
    ).toBeInTheDocument();
    expect(
      openRowActions(pluginCard as HTMLElement).queryByRole("menuitem", { name: /将 shared-skill 安装到平台/i,
      })
    ).not.toBeInTheDocument();
    expect(
      openRowActions(pluginCard as HTMLElement).queryByRole("menuitem", { name: /卸载/i,
      })
    ).toBeInTheDocument();
  });

  it("renders uninstall actions for writable platform skills", () => {
    renderPlatformView();

    const frontendRow = screen.getByRole("row", { name: /frontend-design/i });
    const deleteButton = openRowActions(frontendRow).getByRole("menuitem", { name: /卸载/i,
    });
    const actionCell = deleteButton.closest("[role=menu]");
    expect(actionCell).not.toBeNull();
    expect(within(actionCell as HTMLElement).getAllByRole("menuitem")).toHaveLength(2);
    expect(within(actionCell as HTMLElement).getByRole("menuitem", { name: "定位到技能仓库" })).toBeInTheDocument();
    expect(actionCell?.querySelector(".lucide-package-minus")).toBeInTheDocument();
    const codeReviewerRow = screen.getByRole("row", { name: /code-reviewer/i });
    expect(
      openRowActions(codeReviewerRow).getByRole("menuitem", { name: /卸载/i })
    ).toBeInTheDocument();
  });

  it("uninstalls a skill from the current platform and refreshes counts", async () => {
    renderPlatformView();

    const frontendRow = screen.getByRole("row", { name: /code-reviewer/i });
    fireEvent.click(
      openRowActions(frontendRow).getByRole("menuitem", { name: /卸载/i })
    );
    expect(mockUninstallSkillFromAgent).not.toHaveBeenCalled();

    fireEvent.click(screen.getByRole("menuitem", { name: /确认删除/i }));

    await waitFor(() => {
      expect(mockUninstallSkillFromAgent).toHaveBeenCalledWith(
        "code-reviewer",
        "claude-code"
      );
    });
    expect(mockRefreshCounts).toHaveBeenCalledTimes(1);
  });

  it("cancels the armed uninstall state when clicking outside the card actions", async () => {
    renderPlatformView();

    const frontendRow = screen.getByRole("row", { name: /code-reviewer/i });
    fireEvent.click(
      openRowActions(frontendRow).getByRole("menuitem", { name: /卸载/i })
    );
    expect(screen.getByRole("menuitem", { name: /确认删除/i })).toBeInTheDocument();

    fireEvent.pointerDown(document.body);

    await waitFor(() => {
      expect(screen.queryByRole("menuitem", { name: /确认删除/i })).not.toBeInTheDocument();
    });
    expect(mockUninstallSkillFromAgent).not.toHaveBeenCalled();
  });

  it("does not offer bulk selection or bulk uninstall on platform and project views", () => {
    renderPlatformView();

    expect(screen.queryByRole("checkbox", { name: /选择 / })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /卸载所选/ })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /全选可见|Select visible/ })).not.toBeInTheDocument();
  });





  it("re-fetches the live platform list after a scan generation change and removes stale duplicate rows without clearing the search query", async () => {
    let platformState = buildPlatformStoreState({
      scanGeneration: 1,
      skillsByAgent: { "claude-code": 2 },
    });
    let skillState = buildSkillStoreState({
      skillsByAgent: { "claude-code": mockDuplicatePlatformSkillsWithDistinctIds },
    });

    mockUsePlatformStore.mockImplementation((selector?: unknown) => {
      if (typeof selector === "function") return selector(platformState);
      return platformState;
    });
    mockUseSkillStore.mockImplementation((selector?: unknown) => {
      if (typeof selector === "function") return selector(skillState);
      return skillState;
    });

    const view = renderPlatformView();

    const searchInput = screen.getByPlaceholderText(/搜索技能/);
    fireEvent.change(searchInput, { target: { value: "Shared skill" } });

    await waitFor(() => {
      expect(
        screen.getAllByRole("button", { name: /查看 Shared skill 的详情/i })
      ).toHaveLength(2);
    });

    mockGetSkillsByAgent.mockClear();

    platformState = buildPlatformStoreState({
      scanGeneration: 2,
      skillsByAgent: { "claude-code": 2 },
    });
    skillState = buildSkillStoreState({
      skillsByAgent: {
        "claude-code": [
          mockDuplicatePlatformSkillsWithDistinctIds[1],
          {
            id: "other-skill",
            name: "Other skill",
            description: "Non-matching survivor",
            file_path: "~/.claude/skills/other-skill/SKILL.md",
            dir_path: "~/.claude/skills/other-skill",
            link_type: "native",
            installation_source: "independent",
    is_central: false,
            source_kind: null,
            source_root: null,
            is_read_only: false,
          },
        ],
      },
    });

    view.rerender(
      <MemoryRouter initialEntries={["/platform/claude-code"]}>
        <Routes>
          <Route path="/platform/:agentId" element={<PlatformView />} />
        </Routes>
      </MemoryRouter>
    );

    await waitFor(() => {
      expect(mockGetSkillsByAgent).toHaveBeenCalledWith("claude-code");
    });

    expect(searchInput).toHaveValue("Shared skill");
    expect(
      screen.getAllByRole("button", { name: /查看 Shared skill 的详情/i })
    ).toHaveLength(1);
    expect(screen.queryByText("Other skill")).not.toBeInTheDocument();
  });


});

it("blocks shared skill and mixed folder uninstall before any mutation", () => {
  const error = vi.spyOn(toast, "error").mockImplementation(() => "toast");
  installDefaultStoreMocks();
  mockUninstallSkillFromAgent.mockReset();
  mockUseSkillStore.mockImplementation((selector?: unknown) => {
    const state = buildSkillStoreState({ skillsByAgent: { "claude-code": mockSkills.map(skill => ({ ...skill, source_repo: "example/mixed" })) } });
    return typeof selector === "function" ? selector(state) : state;
  });
  window.localStorage.clear();
  renderPlatformView();
  fireEvent.click(openRowActions(screen.getByRole("row", { name: /frontend-design/i })).getByRole("menuitem", { name: /卸载/i }));
  expect(error).toHaveBeenCalledWith("此技能通过共享中心安装，请前往共享中心卸载。");
  expect(mockUninstallSkillFromAgent).not.toHaveBeenCalled();
  fireEvent.keyDown(screen.getByRole("region",{name:"技能列表"}),{key:"-"});
  fireEvent.click(openRowActions(screen.getByRole("row", { name: /example\/mixed/i })).getByRole("menuitem", { name: /卸载/i }));
  expect(error).toHaveBeenCalledWith("此目录包含通过共享中心安装的技能，请前往共享中心卸载。");
  expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  expect(mockUninstallSkillFromAgent).not.toHaveBeenCalled();
  error.mockRestore();
});

function render(...args: Parameters<typeof rtlRender>) {
  const result = rtlRender(...args);
  const list = screen.queryByRole("region", {name:"技能列表"});
  if (list) fireEvent.keyDown(list,{key:"+"});
  return result;
}
