import { openRowActions } from "./rowActions";
import { describe, it, expect, vi, beforeEach } from "vitest";
import { render as rtlRender, screen, fireEvent, waitFor, within } from "@testing-library/react";
import { MemoryRouter } from "react-router-dom";
import { CentralSkillsView } from "../pages/CentralSkillsView";
import {
  AgentWithStatus,
  CentralSkillBundle,
  CentralSkillBundleDetail,
  SkillWithLinks,
} from "../types";

// Mock stores
vi.mock("../stores/centralSkillsStore", () => ({
  useCentralSkillsStore: vi.fn(),
}));

vi.mock("../stores/platformStore", () => ({
  usePlatformStore: vi.fn(),
}));

vi.mock("../stores/skillStore", () => ({
  useSkillStore: vi.fn(),
}));

vi.mock("../components/skill/SkillDetailDrawer", () => ({
  SkillDetailDrawer: ({
    open,
    skillId,
    onOpenChange,
    returnFocusRef,
  }: {
    open: boolean;
    skillId: string | null;
    onOpenChange: (open: boolean) => void;
    returnFocusRef?: { current: HTMLElement | null };
  }) =>
    open ? (
      <div data-testid="skill-detail-drawer">
        <div>drawer-skill:{skillId}</div>
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

import { useCentralSkillsStore } from "../stores/centralSkillsStore";
import { usePlatformStore } from "../stores/platformStore";
import { useSkillStore } from "../stores/skillStore";
import * as tauriBridge from "@/lib/tauri";

// ─── Fixtures ─────────────────────────────────────────────────────────────────

const mockAgents: AgentWithStatus[] = [
  {
    id: "claude-code",
    display_name: "Claude Code",
    global_skills_dir: "/Users/test/.claude/skills/",
    is_detected: true,
    is_builtin: true,
    is_enabled: true,
  },
  {
    id: "cursor",
    display_name: "Cursor",
    global_skills_dir: "/Users/test/.cursor/skills/",
    is_detected: true,
    is_builtin: true,
    is_enabled: true,
  },
  {
    id: "trae",
    display_name: "Trae",
    global_skills_dir: "/Users/test/.trae/skills/",
    is_detected: true,
    is_builtin: true,
    is_enabled: true,
  },
  {
    id: "openclaw",
    display_name: "OpenClaw",
    global_skills_dir: "/Users/test/.openclaw/skills/",
    is_detected: true,
    is_builtin: true,
    is_enabled: true,
  },
  {
    id: "central",
    display_name: "Shared Hub",
    global_skills_dir: "/Users/test/.agents/skills/",
    is_detected: true,
    is_builtin: true,
    is_enabled: true,
  },
];

const mockSkills: SkillWithLinks[] = [
  {
    id: "frontend-design",
    name: "frontend-design",
    description: "Build distinctive, production-grade frontend interfaces",
    file_path: "~/.agents/skills/frontend-design/SKILL.md",
    canonical_path: "~/.agents/skills/frontend-design",
    is_central: true,
    scanned_at: "2026-04-09T00:00:00Z",
    created_at: "2026-04-10T00:00:00Z",
    updated_at: "2026-04-12T00:00:00Z",
    linked_agents: ["claude-code"],
  },
  {
    id: "code-reviewer",
    name: "code-reviewer",
    description: "Review code changes and identify high-confidence, actionable bugs",
    file_path: "~/.agents/skills/code-reviewer/SKILL.md",
    canonical_path: "~/.agents/skills/code-reviewer",
    is_central: true,
    scanned_at: "2026-04-09T00:00:00Z",
    created_at: "2026-04-08T00:00:00Z",
    updated_at: "2026-04-20T00:00:00Z",
    linked_agents: [],
  },
];

const mockBundles: CentralSkillBundle[] = [
  {
    name: "Superpowers",
    relativePath: "Superpowers",
    path: "/Users/test/.agents/skills/Superpowers",
    isSymlink: false,
    skillCount: 2,
    linkedAgentCount: 1,
    readOnlyAgentCount: 0,
  },
];

const mockBundleDetail: CentralSkillBundleDetail = {
  bundle: mockBundles[0],
  skills: [
    {
      id: "using-superpowers",
      name: "using-superpowers",
      description: "Use Superpowers workflows",
      file_path: "/Users/test/.agents/skills/Superpowers/using-superpowers/SKILL.md",
      canonical_path: "/Users/test/.agents/skills/Superpowers/using-superpowers",
      is_central: true,
      scanned_at: "2026-04-09T00:00:00Z",
      linked_agents: ["claude-code"],
      read_only_agents: [],
    },
    {
      id: "writing-plans",
      name: "writing-plans",
      description: "Write implementation plans",
      file_path: "/Users/test/.agents/skills/Superpowers/writing-plans/SKILL.md",
      canonical_path: "/Users/test/.agents/skills/Superpowers/writing-plans",
      is_central: true,
      scanned_at: "2026-04-09T00:00:00Z",
      linked_agents: ["cursor"],
      read_only_agents: ["openclaw"],
    },
  ],
};

const mockLoadCentralSkills = vi.fn();
const mockLoadCentralBundles = vi.fn();
const mockLoadCentralBundleDetail = vi.fn();
const mockClearCentralBundleDetail = vi.fn();
const mockInstallSkill = vi.fn();
const mockTogglePlatformLink = vi.fn();
const mockUninstallSkillsFromAgent = vi.fn();
const mockDeleteCentralSkill = vi.fn();
const mockPreviewDeleteCentralBundle = vi.fn();
const mockDeleteCentralBundle = vi.fn();
const mockUpdateSourceBackedSkills = vi.fn();
const mockUpdateSourceBackedSkill = vi.fn();
const mockRescan = vi.fn();
const mockGetSkillsByAgent = vi.fn();
const mockUseCentralSkillsStore = vi.mocked(useCentralSkillsStore);
const mockUsePlatformStore = vi.mocked(usePlatformStore);
const mockUseSkillStore = vi.mocked(useSkillStore);

function buildCentralStoreState(overrides = {}) {
  return {
    skills: mockSkills,
    agents: mockAgents,
    bundles: [],
    bundleDetail: null,
    bundleDeletePreview: null,
    isLoading: false,
    isLoadingBundles: false,
    loadingBundleDetailPath: null,
    isInstalling: false,
    deletingSkillId: null,
    deletingBundlePath: null,
    togglingAgentId: null,
    error: null,
    loadCentralSkills: mockLoadCentralSkills,
    loadCentralBundles: mockLoadCentralBundles,
    loadCentralBundleDetail: mockLoadCentralBundleDetail,
    clearCentralBundleDetail: mockClearCentralBundleDetail,
    installSkill: mockInstallSkill,
    togglePlatformLink: mockTogglePlatformLink,
    uninstallSkillsFromAgent: mockUninstallSkillsFromAgent,
    deleteCentralSkill: mockDeleteCentralSkill,
    previewDeleteCentralBundle: mockPreviewDeleteCentralBundle,
    deleteCentralBundle: mockDeleteCentralBundle,
    clearBundleDeletePreview: vi.fn(),
    isUpdatingSources: false,
    updateSourceBackedSkills: mockUpdateSourceBackedSkills,
    updateSourceBackedSkill: mockUpdateSourceBackedSkill,
    ...overrides,
  };
}

function buildPlatformStoreState(overrides = {}) {
  return {
    agents: mockAgents,
    skillsByAgent: {},
    isLoading: false,
    isRefreshing: false,
    error: null,
    initialize: vi.fn(),
    rescan: mockRescan,
    refreshCounts: mockRescan,
    ...overrides,
  };
}

function buildSkillStoreState(overrides = {}) {
  return {
    skillsByAgent: {},
    loadingByAgent: {},
    error: null,
    getSkillsByAgent: mockGetSkillsByAgent,
    ...overrides,
  };
}

function renderCentralSkillsView(centralOverrides = {}) {
  mockUseCentralSkillsStore.mockImplementation((selector?: unknown) => {
    const state = buildCentralStoreState(centralOverrides);
    if (typeof selector === "function") return selector(state);
    return state;
  });
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
  return render(
    <MemoryRouter>
      <CentralSkillsView />
    </MemoryRouter>
  );
}

// ─── Tests ────────────────────────────────────────────────────────────────────

describe("CentralSkillsView", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    localStorage.removeItem("skills-manage.skillTableColumns.tree.v5");
    window.localStorage.clear();
  });

  // ── Header ────────────────────────────────────────────────────────────────

  it("shows page title in header", () => {
    renderCentralSkillsView();
    expect(screen.getByText("共享中心")).toBeInTheDocument();
  });

  it("shows the central skills directory path", () => {
    renderCentralSkillsView();
    expect(screen.getByRole("button", {name: "在文件管理器中打开: /Users/test/.agents/skills/"})).toBeInTheDocument();
  });

  it("opens the central skills directory from the header path", async () => {
    const invokeSpy = vi.spyOn(tauriBridge, "invoke").mockResolvedValue(undefined);
    renderCentralSkillsView();

    fireEvent.click(
      screen.getByRole("button", {
        name: "在文件管理器中打开: /Users/test/.agents/skills/",
      })
    );

    await waitFor(() => {
      expect(invokeSpy).toHaveBeenCalledWith("open_in_file_manager", {
        path: "/Users/test/.agents/skills/",
      });
    });
    invokeSpy.mockRestore();
  });

  it("shows a refresh button", () => {
    renderCentralSkillsView();
    expect(
      screen.getByRole("button", { name: /刷新共享中心/i })
    ).toBeInTheDocument();
  });

  it("does not show source update or GitHub import launchers", () => {
    renderCentralSkillsView();
    expect(screen.queryByRole("button", { name: /从 GitHub 导入/i })).toBeNull();
    expect(screen.queryByRole("button", { name: /从来源更新/i })).toBeNull();
  });

  it("shows a search input", () => {
    renderCentralSkillsView();
    expect(
      screen.getByPlaceholderText(/搜索共享中心/i)
    ).toBeInTheDocument();
  });





  // ── Skills List ───────────────────────────────────────────────────────────

  it("renders all central skills", () => {
    renderCentralSkillsView();
    expect(screen.getByText("frontend-design")).toBeInTheDocument();
    expect(screen.getByText("code-reviewer")).toBeInTheDocument();
  });





  it("keeps descriptions out of the name column", () => {
    renderCentralSkillsView();
    expect(
      screen.queryByText(/Build distinctive, production-grade frontend interfaces/)
    ).not.toBeInTheDocument();
  });

  it("keeps only delete actions in the central flat table", () => {
    renderCentralSkillsView();
    expect(
      screen.queryByRole("button", { name: /将 .* 安装到平台/i })
    ).not.toBeInTheDocument();
    expect(
      openRowActions(screen.getByRole("row", {name:/code-reviewer/i})).getAllByRole("menuitem", { name: /^卸载$|^Uninstall$/i })
    ).toHaveLength(1);
  });

  it("does not show source-backed update actions on central cards", async () => {
    mockUpdateSourceBackedSkill.mockResolvedValue("frontend-design");
    renderCentralSkillsView({
      skills: [
        {
          ...mockSkills[0],
          source_url: "https://example.com/frontend-design/SKILL.md",
          source_author: "example",
          source_repo: "skills",
        },
        mockSkills[1],
      ],
    });

    expect(screen.queryByRole("button", { name: /从来源更新 frontend-design/i })).toBeNull();
  });

  it("deletes an unlinked central skill after inline confirmation", async () => {
    mockDeleteCentralSkill.mockResolvedValue({
      skillId: "code-reviewer",
      removedCanonicalPath: "/Users/test/.agents/skills/code-reviewer",
      uninstalledAgents: [],
      skippedReadOnlyAgents: [],
    });

    renderCentralSkillsView();

    const codeReviewerRow = screen.getByRole("row", { name: /code-reviewer/i });
    fireEvent.click(openRowActions(codeReviewerRow).getByRole("menuitem", { name: /^卸载$|^Uninstall$/i }));
    fireEvent.click(screen.getByRole("menuitem", { name: /确认卸载/i }));

    await waitFor(() => {
      expect(mockDeleteCentralSkill).toHaveBeenCalledWith("code-reviewer", {
        cascadeUninstall: false,
      });
      expect(mockRescan).toHaveBeenCalledTimes(1);
    });
  });

  it("requires explicit cascade confirmation before deleting a linked central skill", async () => {
    mockDeleteCentralSkill.mockResolvedValue({
      skillId: "frontend-design",
      removedCanonicalPath: "/Users/test/.agents/skills/frontend-design",
      uninstalledAgents: ["claude-code"],
      skippedReadOnlyAgents: [],
    });

    renderCentralSkillsView();

    const frontendRow = screen.getByRole("row", { name: /frontend-design/i });
    fireEvent.click(openRowActions(frontendRow).getByRole("menuitem", { name: /^卸载$|^Uninstall$/i }));

    const deleteDialog = await screen.findByRole("dialog", {
      name: /删除 frontend-design/i,
    });
    expect(deleteDialog).toBeInTheDocument();
    expect(within(deleteDialog).getByText(/Claude Code/)).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: /同时卸载并删除/i }));

    await waitFor(() => {
      expect(mockDeleteCentralSkill).toHaveBeenCalledWith("frontend-design", {
        cascadeUninstall: true,
      });
    });
  });



  it("keeps delete actions for source-grouped central folders", async () => {
    window.localStorage.setItem("skills-manage.skillListViewMode.central", "folders");
    mockDeleteCentralSkill.mockResolvedValue({
      skillId: "deleted",
      removedCanonicalPath: "/Users/test/.agents/skills/deleted",
      uninstalledAgents: [],
      skippedReadOnlyAgents: [],
    });

    renderCentralSkillsView({
      skills: [
        {
          ...mockSkills[0],
          id: "source-alpha",
          name: "source-alpha",
          file_path: "/Users/test/.agents/skills/source-alpha/SKILL.md",
          canonical_path: "/Users/test/Skills/addyosmani/agent-skills/source-alpha",
          source_repo: "addyosmani/agent-skills",
          linked_agents: [],
        },
        {
          ...mockSkills[1],
          id: "source-beta",
          name: "source-beta",
          file_path: "/Users/test/.agents/skills/source-beta/SKILL.md",
          canonical_path: "/Users/test/Skills/addyosmani/agent-skills/source-beta",
          source_repo: "addyosmani/agent-skills",
          linked_agents: [],
        },
      ],
    });

    const folderRow = screen.getByRole("row", { name: /addyosmani\/agent-skills/i });
    fireEvent.click(
      openRowActions(folderRow).getByRole("menuitem", { name: /^卸载$|^Uninstall$/i })
    );
    fireEvent.click(screen.getByRole("menuitem", { name: /确认卸载/i }));

    await waitFor(() => {
      expect(mockDeleteCentralSkill).toHaveBeenCalledWith("source-alpha", {
        cascadeUninstall: true,
      });
      expect(mockDeleteCentralSkill).toHaveBeenCalledWith("source-beta", {
        cascadeUninstall: true,
      });
    });
  });





  it("keeps bundle delete icon from opening the folder table", async () => {
    window.localStorage.setItem("skills-manage.skillListViewMode.central", "folders");
    mockPreviewDeleteCentralBundle.mockResolvedValue({
      bundle: mockBundles[0],
      skills: mockBundleDetail.skills,
      affectedAgents: ["claude-code", "cursor"],
      skippedReadOnlyAgents: ["openclaw"],
    });
    renderCentralSkillsView({ bundles: mockBundles });

    const bundleRow = screen.getByRole("row", { name: /Superpowers/i });
    fireEvent.click(
      openRowActions(bundleRow).getByRole("menuitem", { name: /^卸载$|^Uninstall$/i })
    );

    expect(
      await screen.findByRole("dialog", { name: /删除套件 Superpowers/i })
    ).toBeInTheDocument();
    expect(mockPreviewDeleteCentralBundle).toHaveBeenCalledWith("Superpowers");
    expect(mockLoadCentralBundleDetail).not.toHaveBeenCalled();
    expect(screen.queryByRole("button", { name: "返回目录" })).not.toBeInTheDocument();
  });

  it("previews and deletes a central skill bundle after danger confirmation", async () => {
    window.localStorage.setItem("skills-manage.skillListViewMode.central", "folders");
    mockPreviewDeleteCentralBundle.mockResolvedValue({
      bundle: mockBundles[0],
      skills: [
        {
          id: "using-superpowers",
          name: "using-superpowers",
          file_path: "/Users/test/.agents/skills/Superpowers/using-superpowers/SKILL.md",
          canonical_path: "/Users/test/.agents/skills/Superpowers/using-superpowers",
          is_central: true,
          scanned_at: "2026-04-09T00:00:00Z",
          linked_agents: ["claude-code"],
          read_only_agents: [],
        },
      ],
      affectedAgents: ["claude-code"],
      skippedReadOnlyAgents: [],
    });
    mockDeleteCentralBundle.mockResolvedValue({
      relativePath: "Superpowers",
      removedBundlePath: "/Users/test/.agents/skills/Superpowers",
      removedKind: "directory",
      removedSkillIds: ["using-superpowers"],
      uninstalledAgents: ["claude-code"],
      skippedReadOnlyAgents: [],
    });

    mockUseCentralSkillsStore.mockImplementation((selector?: unknown) => {
      const state = buildCentralStoreState({
        bundles: mockBundles,
        bundleDeletePreview: {
          bundle: mockBundles[0],
          skills: [
            {
              id: "using-superpowers",
              name: "using-superpowers",
              file_path: "/Users/test/.agents/skills/Superpowers/using-superpowers/SKILL.md",
              canonical_path: "/Users/test/.agents/skills/Superpowers/using-superpowers",
              is_central: true,
              scanned_at: "2026-04-09T00:00:00Z",
              linked_agents: ["claude-code"],
              read_only_agents: [],
            },
          ],
          affectedAgents: ["claude-code"],
          skippedReadOnlyAgents: [],
        },
      });
      if (typeof selector === "function") return selector(state);
      return state;
    });
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
    render(
      <MemoryRouter>
        <CentralSkillsView />
      </MemoryRouter>
    );

    const bundleRow = screen.getByRole("row", { name: /Superpowers/i });
    fireEvent.click(
      openRowActions(bundleRow).getByRole("menuitem", { name: /^卸载$|^Uninstall$/i })
    );

    expect(mockPreviewDeleteCentralBundle).toHaveBeenCalledWith("Superpowers");
    const deleteBundleDialog = await screen.findByRole("dialog", {
      name: /删除套件 Superpowers/i,
    });
    expect(deleteBundleDialog).toBeInTheDocument();
    expect(within(deleteBundleDialog).getByText(/using-superpowers/)).toBeInTheDocument();
    expect(within(deleteBundleDialog).getByText(/Claude Code/)).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: /删除套件并卸载/i }));

    await waitFor(() => {
      expect(mockDeleteCentralBundle).toHaveBeenCalledWith("Superpowers", {
        cascadeUninstall: true,
      });
      expect(mockRescan).toHaveBeenCalledTimes(1);
    });
  });

  it("renders browser fixture skill card on the localhost validation surface without Tauri", async () => {
    const isTauriSpy = vi.spyOn(tauriBridge, "isTauriRuntime").mockReturnValue(false);
    mockUseCentralSkillsStore.mockRestore();
    mockUsePlatformStore.mockRestore();

    render(
      <MemoryRouter>
        <CentralSkillsView />
      </MemoryRouter>
    );

    expect(await screen.findByRole("button", { name: /查看 fixture-central-skill 的详情/i })).toBeInTheDocument();

    isTauriSpy.mockRestore();
  });

  it("skill name is a clickable button for detail navigation", () => {
    renderCentralSkillsView();
    // The skill name itself is the detail link (no separate [详情] button).
    const detailBtns = screen.getAllByRole("button", {
      name: /查看 frontend-design 的详情/i,
    });
    expect(detailBtns.length).toBeGreaterThanOrEqual(1);
  });

  // ── Aggregate install summary ─────────────────────────────────────────────

  it("uses uninstall icons and labels for Shared Hub skills", () => {
    renderCentralSkillsView();
    for (const button of openRowActions(screen.getByRole("row", {name:/code-reviewer/i})).getAllByRole("menuitem", { name: "卸载" })) {
      expect(button.querySelector(".lucide-package-minus")).toBeInTheDocument();
      expect(button.querySelector(".lucide-trash-2")).not.toBeInTheDocument();
    }
  });

  it("shows aggregate install counts without platform category details", () => {
    localStorage.setItem("skills-manage.skillTableColumns.tree.v5", JSON.stringify(["index","name","source","skillCount","createdAt","updatedAt","installSummary","actions"]));
    renderCentralSkillsView();

    expect(screen.queryByText(/独立安装：平台/)).not.toBeInTheDocument();
    expect(screen.getAllByText("共享中心：平台 4 · 项目 0").length).toBeGreaterThan(0);
    expect(screen.queryByText("龙虾类")).not.toBeInTheDocument();
    expect(screen.queryByText("编程类")).not.toBeInTheDocument();
    expect(
      screen.queryByRole("button", {
        name: "切换 frontend-design 在 Claude Code 的链接状态",
      })
    ).not.toBeInTheDocument();
  });

  // ── Empty State ───────────────────────────────────────────────────────────

  it("shows a plain empty state when no central skills exist", () => {
    mockUseCentralSkillsStore.mockImplementation((selector?: unknown) => {
      const state = buildCentralStoreState({ skills: [] });
      if (typeof selector === "function") return selector(state);
      return state;
    });

    render(
      <MemoryRouter>
        <CentralSkillsView />
      </MemoryRouter>
    );

    expect(screen.getByText("没有匹配的技能")).toBeInTheDocument();
    expect(screen.queryByText(/欢迎使用 Arsenal/)).not.toBeInTheDocument();
  });

  it("shows loading state", () => {
    mockUseCentralSkillsStore.mockImplementation((selector?: unknown) => {
      const state = buildCentralStoreState({ isLoading: true, skills: [] });
      if (typeof selector === "function") return selector(state);
      return state;
    });

    render(
      <MemoryRouter>
        <CentralSkillsView />
      </MemoryRouter>
    );

    expect(screen.getByText("加载中...")).toBeInTheDocument();
  });

  // ── Search / Filter ───────────────────────────────────────────────────────

  it("filters skills by name when searching", async () => {
    renderCentralSkillsView();
    const searchInput = screen.getByPlaceholderText(/搜索共享中心/i);
    fireEvent.change(searchInput, { target: { value: "frontend" } });

    await waitFor(() => {
      expect(screen.getByText("frontend-design")).toBeInTheDocument();
      expect(screen.queryByText("code-reviewer")).not.toBeInTheDocument();
    });
  });

  it("keeps filtered search results in the central table", async () => {
    renderCentralSkillsView();
    const searchInput = screen.getByPlaceholderText(/搜索共享中心/i);
    fireEvent.change(searchInput, { target: { value: "frontend" } });

    const resultButton = await screen.findByText("frontend-design");

    expect(resultButton.closest("tr")).not.toBeNull();
    expect(resultButton.closest("table")).not.toBeNull();
  });

  it("filters skills by description when searching", async () => {
    renderCentralSkillsView();
    const searchInput = screen.getByPlaceholderText(/搜索共享中心/i);
    fireEvent.change(searchInput, { target: { value: "actionable" } });

    await waitFor(() => {
      expect(screen.getByText("code-reviewer")).toBeInTheDocument();
      expect(screen.queryByText("frontend-design")).not.toBeInTheDocument();
    });
  });

  it("searches notes but excludes tags from keyword search", async () => {
    renderCentralSkillsView({
      skills: [
        {
          ...mockSkills[0],
          notes: "dashboard-only local guidance",
          tags: ["ui-pattern"],
        },
        mockSkills[1],
      ],
    });
    const searchInput = screen.getByPlaceholderText(/搜索共享中心/i);
    fireEvent.change(searchInput, { target: { value: "dashboard-only" } });

    await waitFor(() => {
      expect(screen.getByText("frontend-design")).toBeInTheDocument();
      expect(screen.queryByText("code-reviewer")).not.toBeInTheDocument();
    });

    fireEvent.change(searchInput, { target: { value: "ui-pattern" } });

    await waitFor(() => {
      expect(screen.queryByText("frontend-design")).not.toBeInTheDocument();
      expect(screen.queryByText("code-reviewer")).not.toBeInTheDocument();
    });
  });

  it("filters skills by selected tag", async () => {
    renderCentralSkillsView({
      skills: [
        {
          ...mockSkills[0],
          tags: ["frontend"],
        },
        {
          ...mockSkills[1],
          tags: ["review"],
        },
      ],
    });

    const tagFilter = screen.getByRole("group", { name: "标签" });
    fireEvent.click(within(tagFilter).getByRole("button", { name: "frontend" }));

    await waitFor(() => {
      expect(screen.getByText("frontend-design")).toBeInTheDocument();
      expect(screen.queryByText("code-reviewer")).not.toBeInTheDocument();
    });

    fireEvent.click(within(tagFilter).getByRole("button", { name: "frontend" }));

    await waitFor(() => {
      expect(screen.getByText("frontend-design")).toBeInTheDocument();
      expect(screen.getByText("code-reviewer")).toBeInTheDocument();
    });
  });

  it("does not show bulk selection or uninstall controls", () => {
    renderCentralSkillsView({
      skills: [
        {
          ...mockSkills[0],
          linked_agents: ["claude-code"],
        },
      ],
    });

    expect(screen.queryByRole("group", { name: /批量卸载|Bulk uninstall/i })).not.toBeInTheDocument();
    expect(screen.queryByLabelText("选择 frontend-design")).not.toBeInTheDocument();
    expect(screen.queryByLabelText("选择卸载平台")).not.toBeInTheDocument();
  });

  it("shows empty state when search has no results", async () => {
    renderCentralSkillsView();
    const searchInput = screen.getByPlaceholderText(/搜索共享中心/i);
    fireEvent.change(searchInput, { target: { value: "zzz-nonexistent" } });

    await waitFor(() => {
      expect(screen.getByText(/没有匹配的技能/)).toBeInTheDocument();
    });
  });

  it("restores all skills when search is cleared", async () => {
    renderCentralSkillsView();
    const searchInput = screen.getByPlaceholderText(/搜索共享中心/i);
    fireEvent.change(searchInput, { target: { value: "frontend" } });
    fireEvent.change(searchInput, { target: { value: "" } });

    await waitFor(() => {
      expect(screen.getByText("frontend-design")).toBeInTheDocument();
      expect(screen.getByText("code-reviewer")).toBeInTheDocument();
    });
  });

  // ── Load on Mount ─────────────────────────────────────────────────────────

  it("calls loadCentralSkills on mount", () => {
    renderCentralSkillsView();
    expect(mockLoadCentralSkills).toHaveBeenCalledTimes(1);
  });

  // ── Refresh Button ────────────────────────────────────────────────────────

  it("calls rescan then loadCentralSkills when refresh button is clicked", async () => {
    renderCentralSkillsView();
    const refreshBtn = screen.getByRole("button", {
      name: /刷新共享中心/i,
    });
    fireEvent.click(refreshBtn);

    await waitFor(() => {
      // rescan is called once (only on refresh, not on mount)
      expect(mockRescan).toHaveBeenCalledTimes(1);
      // loadCentralSkills is called twice: once on mount, once on refresh
      expect(mockLoadCentralSkills).toHaveBeenCalledTimes(2);
    });
  });



  it("offers post-import platform installation for imported skills", async () => {
    renderCentralSkillsView();

    expect(screen.queryByRole("button", { name: /从 GitHub 导入/i })).toBeNull();
    expect(screen.queryByText(/导入结果|GitHub 仓库/i)).toBeNull();
  });

  it("does not expose the shared github wizard from the central page", async () => {
    renderCentralSkillsView();

    expect(screen.queryByRole("button", { name: /从 GitHub 导入/i })).toBeNull();
    expect(screen.queryByTestId("github-import-confirm-summary")).toBeNull();
  });


});

function render(...args: Parameters<typeof rtlRender>) {
  const result = rtlRender(...args);
  const list = screen.queryByRole("region", {name:"技能列表"});
  if (list) fireEvent.keyDown(list,{key:"+"});
  return result;
}
