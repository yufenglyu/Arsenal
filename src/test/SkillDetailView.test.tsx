import { open as openExternal } from "@tauri-apps/plugin-shell";
import { describe, it, expect, vi, beforeEach } from "vitest";
import { act, render, screen, fireEvent, waitFor, within } from "@testing-library/react";
import { MemoryRouter } from "react-router-dom";
import { SkillDetailView } from "../components/skill/SkillDetailView";
import {
  AgentWithStatus,
  SkillDetail as SkillDetailType,
  SkillDirectoryNode as SkillDirectoryNodeType,
} from "../types";

const { mockTauriInvoke, mockIsTauriRuntime } = vi.hoisted(() => ({
  mockTauriInvoke: vi.fn(),
  mockIsTauriRuntime: vi.fn(() => true),
}));

vi.mock("@/lib/tauri", () => ({
  invoke: mockTauriInvoke,
  isTauriRuntime: mockIsTauriRuntime,
}));

vi.mock("@tauri-apps/plugin-shell", () => ({ open: vi.fn().mockResolvedValue(undefined) }));

// ─── Mock stores ──────────────────────────────────────────────────────────────

vi.mock("../stores/skillDetailStore", () => ({
  useSkillDetailStore: vi.fn(),
}));

vi.mock("../stores/platformStore", () => ({
  usePlatformStore: vi.fn(),
}));

// ─── Mock CollectionPickerDialog ──────────────────────────────────────────────

vi.mock("../components/collection/CollectionPickerDialog", () => ({
  CollectionPickerDialog: ({
    open,
    onOpenChange,
    onAdded,
    currentCollectionIds,
  }: {
    open: boolean;
    onOpenChange: (open: boolean) => void;
    skillId: string;
    currentCollectionIds: string[];
    onAdded: () => void;
  }) =>
    open ? (
      <div
        data-testid="collection-picker-dialog"
        data-current-collection-ids={currentCollectionIds.join(",")}
      >
        <button onClick={() => { onAdded(); onOpenChange(false); }}>
          Confirm add to collection
        </button>
        <button onClick={() => onOpenChange(false)}>Cancel picker</button>
      </div>
    ) : null,
}));

import { useSkillDetailStore } from "../stores/skillDetailStore";
import { usePlatformStore } from "../stores/platformStore";

// ─── Mock react-markdown ──────────────────────────────────────────────────────

vi.mock("react-markdown", () => ({
  default: ({
    children,
    remarkPlugins,
  }: {
    children: string;
    remarkPlugins?: unknown[];
  }) => (
    <div
      data-testid="react-markdown"
      data-has-remark-gfm={remarkPlugins && remarkPlugins.length > 0 ? "true" : "false"}
    >
      {children}
    </div>
  ),
}));

// ─── Fixtures ─────────────────────────────────────────────────────────────────

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

const mockDetail: SkillDetailType = {
  id: "frontend-design",
  row_id: "frontend-design",
  name: "frontend-design",
  description: "Build distinctive, production-grade frontend interfaces",
  file_path: "~/.agents/skills/frontend-design/SKILL.md",
  dir_path: "~/.agents/skills/frontend-design",
  canonical_path: "~/.agents/skills/frontend-design",
  is_central: true,
  source: "native",
  scanned_at: "2026-04-09T00:00:00Z",
  source_kind: null,
  source_root: null,
  is_read_only: false,
  conflict_group: null,
  conflict_count: 0,
  collections: [],
  installations: [
    {
      skill_id: "frontend-design",
      agent_id: "claude-code",
      installed_path: "~/.claude/skills/frontend-design",
      link_type: "symlink",
      symlink_target: "~/.agents/skills/frontend-design",
      installed_at: "2026-04-09T12:00:00Z",
    },
  ],
};

const mockCompatibilityDetail: SkillDetailType = {
  ...mockDetail,
  row_id: "cursor::compatibility::frontend-design",
  file_path: "~/.agents/skills/frontend-design/SKILL.md",
  dir_path: "~/.agents/skills/frontend-design",
  canonical_path: undefined,
  is_central: false,
  source: "compatibility",
  source_kind: "compatibility",
  source_root: "~/.agents/skills",
  is_read_only: true,
  installations: [],
  collections: [],
};

const mockPlatformManagedDetail: SkillDetailType = {
  ...mockDetail,
  row_id: "cursor::frontend-design",
  file_path: "~/.cursor/skills/frontend-design/SKILL.md",
  dir_path: "~/.cursor/skills/frontend-design",
  is_central: false,
  source: "native",
  source_kind: null,
  source_root: null,
  is_read_only: false,
  collections: [
    {
      id: "platform-local",
      name: "Platform Local",
      description: "Platform-managed skills",
      created_at: "2026-04-09T00:00:00Z",
      updated_at: "2026-04-09T00:00:00Z",
    },
  ],
};

const mockContent =
  "---\nname: frontend-design\ndescription: Build distinctive, production-grade frontend interfaces\nmetadata:\n  openclaw:\n    requires:\n      anyBins:\n        - bun\n        - npx\n---\n\n# Frontend Design\n\nContent here.";

const mockCompatibilityContent =
  "---\nname: frontend-design\ndescription: Plugin copy\n---\n\n# Plugin Frontend Design\n\nPlugin content.";

const mockPlatformContent =
  "---\nname: frontend-design\ndescription: User copy\n---\n\n# User Frontend Design\n\nUser content.";

const mockNotesContent = "Project notes for frontend design.";

const mockDirectoryTree: SkillDirectoryNodeType[] = [
  {
    name: "docs",
    path: "~/.agents/skills/frontend-design/docs",
    relative_path: "docs",
    is_dir: true,
    children: [
      {
        name: "notes.txt",
        path: "~/.agents/skills/frontend-design/docs/notes.txt",
        relative_path: "docs/notes.txt",
        is_dir: false,
        children: [],
      },
    ],
  },
  {
    name: "SKILL.md",
    path: "~/.agents/skills/frontend-design/SKILL.md",
    relative_path: "SKILL.md",
    is_dir: false,
    children: [],
  },
];

const mockLoadDetail = vi.fn();
const mockInstallSkill = vi.fn();
const mockUninstallSkill = vi.fn();
const mockLoadCachedExplanation = vi.fn();
const mockGenerateExplanation = vi.fn();
const mockGenerateTags = vi.fn();
const mockRefreshExplanation = vi.fn();
const mockCleanupExplanationListeners = vi.fn();
const mockReset = vi.fn();
const mockRescan = vi.fn();
const mockRefreshCounts = vi.fn();
const mockRefreshInstallations = vi.fn();
const mockUpdateMetadata = vi.fn();
const mockUpdateSourceMetadata = vi.fn();

function buildDetailStoreState(overrides = {}) {
  return {
    detail: mockDetail,
    content: mockContent,
    isLoading: false,
    installingAgentId: null,
    error: null,
    explanation: null,
    isExplanationLoading: false,
    isExplanationStreaming: false,
    explanationError: null,
    explanationErrorInfo: null,
    loadDetail: mockLoadDetail,
    loadCachedExplanation: mockLoadCachedExplanation,
    generateExplanation: mockGenerateExplanation,
    generateTags: mockGenerateTags,
    refreshExplanation: mockRefreshExplanation,
    clearNoteGeneration: vi.fn(),
    installSkill: mockInstallSkill,
    uninstallSkill: mockUninstallSkill,
    refreshInstallations: mockRefreshInstallations,
    updateMetadata: mockUpdateMetadata,
    updateSourceMetadata: mockUpdateSourceMetadata,
    cleanupExplanationListeners: mockCleanupExplanationListeners,
    reset: mockReset,
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
    refreshCounts: mockRefreshCounts,
    ...overrides,
  };
}

function applyStoreMocks(detailOverrides = {}, platformOverrides = {}) {
  vi.mocked(useSkillDetailStore).mockImplementation((selector?: unknown) => {
    const state = buildDetailStoreState(detailOverrides);
    if (typeof selector === "function") return selector(state);
    return state;
  });
  vi.mocked(usePlatformStore).mockImplementation((selector?: unknown) => {
    const state = buildPlatformStoreState(platformOverrides);
    if (typeof selector === "function") return selector(state);
    return state;
  });
}

function renderView(
  skillId = "frontend-design",
  variant: "page" | "drawer" = "page",
  options?: { skipMockSetup?: boolean }
) {
  if (!options?.skipMockSetup) {
    applyStoreMocks();
  }
  return render(
    <MemoryRouter>
      <SkillDetailView skillId={skillId} variant={variant} />
    </MemoryRouter>
  );
}

// ─── Tests ────────────────────────────────────────────────────────────────────

describe("SkillDetailView", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mockIsTauriRuntime.mockImplementation(() => {
      const w = window as unknown as {
        __TAURI__?: unknown;
        __TAURI_INTERNALS__?: unknown;
      };
      return Boolean(w.__TAURI__ || w.__TAURI_INTERNALS__);
    });
    mockTauriInvoke.mockImplementation(async (command, args) => {
      if (command === "list_skill_directory") {
        return mockDirectoryTree;
      }
      if (command === "read_file_by_path") {
        if (args && typeof args === "object" && "path" in args) {
          return args.path === "~/.agents/skills/frontend-design/docs/notes.txt"
            ? mockNotesContent
            : mockContent;
        }
      }
      if (command === "open_in_file_manager") {
        return null;
      }
      throw new Error(`Unhandled invoke command: ${String(command)}`);
    });
  });

  it("opens each source link once without triggering the shell body listener", () => {
    mockIsTauriRuntime.mockReturnValue(true);
    applyStoreMocks({ detail: { ...mockDetail, source_repo: "Example/Skills", source_path: "demo/SKILL.md" } });
    renderView("frontend-design", "page", { skipMockSetup: true });
    const delegatedOpen = vi.fn();
    const shellListener = (event: MouseEvent) => {
      const anchor = (event.target as Element).closest("a");
      if (anchor?.target === "_blank") delegatedOpen(anchor.href);
    };
    document.body.addEventListener("click", shellListener);
    try {
      fireEvent.click(screen.getByRole("link", { name: "Example/Skills" }));
      expect(openExternal).toHaveBeenCalledTimes(1);
      expect(openExternal).toHaveBeenLastCalledWith("https://github.com/Example/Skills");
      fireEvent.click(screen.getByRole("link", { name: "demo/SKILL.md" }));
      expect(openExternal).toHaveBeenCalledTimes(2);
      expect(openExternal).toHaveBeenLastCalledWith("https://github.com/Example/Skills/blob/HEAD/demo/SKILL.md");
      expect(delegatedOpen).not.toHaveBeenCalled();
    } finally {
      document.body.removeEventListener("click", shellListener);
    }
  });

  // ── Shell-agnostic: no back button / breadcrumb is rendered here ─────────

  it("does not render a back button (that belongs to the outer shell)", () => {
    renderView();
    expect(screen.queryByRole("button", { name: /返回/i })).toBeNull();
  });

  // ── Skill name & description ──────────────────────────────────────────────

  it("shows skill name in ViewHeader h1", () => {
    renderView();
    expect(screen.getByRole("heading", { name: /frontend-design/i })).toBeInTheDocument();
  });

  it("shows skill description in ViewHeader", () => {
    renderView();
    expect(
      screen.getAllByText("Build distinctive, production-grade frontend interfaces")[0]
    ).toBeInTheDocument();
  });

  it("renders optional leading slot when provided", () => {
    applyStoreMocks();
    render(
      <MemoryRouter>
        <SkillDetailView
          skillId="frontend-design"
          variant="drawer"
          leading={<span data-testid="leading-slot">L</span>}
        />
      </MemoryRouter>
    );
    expect(screen.getByTestId("leading-slot")).toBeInTheDocument();
  });

  // ── Metadata ──────────────────────────────────────────────────────────────

  it("shows a compact basic metadata section", () => {
    renderView();
    const metadataRegion = screen.getByRole("region", { name: /基本信息/i });
    expect(within(metadataRegion).getByText("native")).toBeInTheDocument();
    expect(within(metadataRegion).getByText("~/.agents/skills/frontend-design")).toBeInTheDocument();
    expect(within(metadataRegion).queryByText(/扫描时间/i)).toBeNull();
    expect(within(metadataRegion).queryByText(/文件路径/i)).toBeNull();
    expect(within(metadataRegion).queryByText(/规范路径/i)).toBeNull();
  });

  it("shows file tree above basic metadata and keeps directories collapsed by default", async () => {
    renderView();

    const filesRegion = await screen.findByRole("region", { name: /技能文件/i });
    const metadataRegion = await screen.findByRole("region", { name: /基本信息/i });
    const docsButton = await within(filesRegion).findByRole("button", { name: "docs" });

    expect(filesRegion.compareDocumentPosition(metadataRegion) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    expect(within(filesRegion).getByText("docs")).toBeInTheDocument();
    expect(within(filesRegion).getByRole("button", { name: "SKILL.md" })).toBeInTheDocument();
    expect(docsButton).toHaveAttribute("aria-expanded", "false");
    expect(within(filesRegion).queryByRole("button", { name: "notes.txt" })).toBeNull();

    fireEvent.click(docsButton);
    expect(docsButton).toHaveAttribute("aria-expanded", "true");
    expect(within(filesRegion).getByRole("button", { name: "notes.txt" })).toBeInTheDocument();
  });

  it("shows source", () => {
    renderView();
    const metadataRegion = screen.getByRole("region", { name: /基本信息/i });
    expect(within(metadataRegion).getByText("native")).toBeInTheDocument();
  });

  it("shows the resource directory and shared link path for central symlink skills", () => {
    applyStoreMocks({
      detail: {
        ...mockDetail,
        is_central: true,
        file_path: "D:/SkillsHub/repository/owner/repo/frontend-design/SKILL.md",
        dir_path: "D:/SkillsHub/repository/owner/repo/frontend-design",
        canonical_path: "C:/Users/test/.agents/skills/frontend-design",
      },
    });
    renderView("frontend-design", "page", { skipMockSetup: true });

    const metadataRegion = screen.getByRole("region", { name: /基本信息/i });
    expect(within(metadataRegion).getByText("存储路径")).toBeInTheDocument();
    expect(
      within(metadataRegion).getByText("D:\\SkillsHub\\repository\\owner\\repo\\frontend-design")
    ).toBeInTheDocument();
    expect(within(metadataRegion).getByText("当前安装路径")).toBeInTheDocument();
    expect(
      within(metadataRegion).getByText("C:\\Users\\test\\.agents\\skills\\frontend-design")
    ).toBeInTheDocument();
  });

  it("shows the resource directory and link path for platform symlink skills", () => {
    applyStoreMocks({
      detail: {
        ...mockDetail,
        is_central: false,
        file_path: "D:/SkillsHub/repository/owner/repo/frontend-design/SKILL.md",
        dir_path: "D:/SkillsHub/repository/owner/repo/frontend-design",
        canonical_path: "C:/Users/test/.cursor/skills/frontend-design",
      },
    });
    renderView("frontend-design", "page", { skipMockSetup: true });

    const metadataRegion = screen.getByRole("region", { name: /基本信息/i });
    expect(
      within(metadataRegion).getByText("D:\\SkillsHub\\repository\\owner\\repo\\frontend-design")
    ).toBeInTheDocument();
    expect(within(metadataRegion).getByText("当前安装路径")).toBeInTheDocument();
    expect(
      within(metadataRegion).getByText("C:\\Users\\test\\.cursor\\skills\\frontend-design")
    ).toBeInTheDocument();
    expect(within(metadataRegion).queryByText("文件路径")).toBeNull();
  });

  it("shows GitHub repository source without exposing the basic info editor", () => {
    applyStoreMocks({
      detail: {
        ...mockDetail,
        is_central: false,
        source: "resource-library",
        source_repo: "owner/repo",
        source_url: "https://github.com/owner/repo",
        source_author: "skills@1.5.23",
        source_path: "frontend-design",
      },
    });
    renderView("frontend-design", "page", { skipMockSetup: true });

    const metadataRegion = screen.getByRole("region", { name: /基本信息/i });
    expect(within(metadataRegion).getByText("owner/repo")).toBeInTheDocument();
    expect(within(metadataRegion).getByText("frontend-design")).toBeInTheDocument();
    expect(within(metadataRegion).queryByText("https://github.com/owner/repo")).toBeNull();
    expect(within(metadataRegion).queryByText("skills@1.5.23")).toBeNull();
    expect(screen.queryByDisplayValue("resource-library")).toBeNull();
    expect(screen.queryByRole("button", { name: /保存基本信息/i })).toBeNull();
  });

  it("allows manual resource skills to maintain basic source info", async () => {
    mockUpdateSourceMetadata.mockResolvedValue(undefined);
    applyStoreMocks({
      detail: {
        ...mockDetail,
        is_central: false,
        source: "manual",
        source_repo: null,
        source_url: null,
        source_author: null,
        source_path: null,
      },
    });
    renderView("frontend-design", "page", { skipMockSetup: true });

    const metadataRegion = screen.getByRole("region", { name: /基本信息/i });
    fireEvent.change(within(metadataRegion).getByDisplayValue("manual"), {
      target: { value: "github" },
    });
    fireEvent.change(within(metadataRegion).getByPlaceholderText("owner/repo"), {
      target: { value: "example/skills" },
    });
    fireEvent.click(screen.getByRole("button", { name: /保存基本信息/i }));

    await waitFor(() => {
      expect(mockUpdateSourceMetadata).toHaveBeenCalledWith("frontend-design", {
        sourceType: "github",
        sourceUrl: null,
        sourceAuthor: null,
        sourceRepo: "example/skills",
        sourcePath: null,
      });
    });
  });

  it("saves editable local notes without changing tags", async () => {
    mockUpdateMetadata.mockResolvedValue(undefined);
    applyStoreMocks({
      detail: {
        ...mockDetail,
        notes: "Existing note",
        tags: ["frontend"],
      },
    });
    renderView("frontend-design", "page", { skipMockSetup: true });

    const notesInput = screen.getByPlaceholderText(/记录此技能的用途/i);
    fireEvent.change(notesInput, { target: { value: "Use for dashboard work" } });
    fireEvent.click(within(screen.getByRole("region", {name:"技能备注"})).getByRole("button", {name:"保存"}));

    await waitFor(() => {
      expect(mockUpdateMetadata).toHaveBeenCalledWith("frontend-design", {
        notes: "Use for dashboard work",
        tags: ["frontend"],
      });
    });
  });

  it("saves editable local tags without changing notes", async () => {
    mockUpdateMetadata.mockResolvedValue(undefined);
    applyStoreMocks({
      detail: {
        ...mockDetail,
        notes: "Existing note",
        tags: ["frontend"],
      },
    });
    renderView("frontend-design", "page", { skipMockSetup: true });

    const tagsInput = screen.getByPlaceholderText(/输入标签/i);
    fireEvent.change(tagsInput, { target: { value: "ui, dashboard, UI" } });
    fireEvent.click(within(screen.getByRole("region", {name:"技能标签"})).getByRole("button", {name:"保存"}));

    await waitFor(() => {
      expect(mockUpdateMetadata).toHaveBeenCalledWith("frontend-design", {
        notes: "Existing note",
        tags: ["ui", "dashboard"],
      });
    });
  });

  it("hides local notes and tags editors for read-only compatibility skills", () => {
    applyStoreMocks({
      detail: mockCompatibilityDetail,
      content: mockCompatibilityContent,
    });

    render(
      <MemoryRouter>
        <SkillDetailView
          skillId="frontend-design"
          agentId="cursor"
          rowId="cursor::compatibility::frontend-design"
          variant="drawer"
        />
      </MemoryRouter>
    );

    expect(screen.queryByRole("region", { name: /技能备注/i })).toBeNull();
    expect(screen.queryByRole("region", { name: /技能标签/i })).toBeNull();
  });

  it("shows a read-only compatibility source state and blocks management actions", () => {
    applyStoreMocks({
      detail: mockCompatibilityDetail,
      content: mockCompatibilityContent,
    });

    render(
      <MemoryRouter>
        <SkillDetailView
          skillId="frontend-design"
          agentId="cursor"
          rowId="cursor::compatibility::frontend-design"
          variant="drawer"
        />
      </MemoryRouter>
    );

    const sourceStatusRegion = screen.getByRole("region", { name: /来源状态|Source status/i });
    expect(
      within(sourceStatusRegion).getByText(/共享中心可见|Seen from Shared Hub/i)
    ).toBeInTheDocument();
    expect(
      within(sourceStatusRegion).getByText(/只读来源|Read-only source/i)
    ).toBeInTheDocument();
    const metadataRegion = screen.getByRole("region", { name: /基本信息/i });
    expect(
      within(metadataRegion).getByText("~/.agents/skills/frontend-design")
    ).toBeInTheDocument();
    expect(
      screen.getByText(/只读观测副本仅供查看|display-only/i)
    ).toBeInTheDocument();
    expect(
      screen.getByText(/不可调整技能集|Bundle management is unavailable/i)
    ).toBeInTheDocument();
    expect(
      screen.queryByRole("button", { name: /切换 .* 的链接状态/i })
    ).toBeNull();
    expect(
      screen.queryByRole("button", { name: /加入技能集/i })
    ).toBeNull();
  });

  it("keeps platform-local detail manageable", () => {
    applyStoreMocks({
      detail: mockPlatformManagedDetail,
      content: mockPlatformContent,
    });

    render(
      <MemoryRouter>
        <SkillDetailView
          skillId="frontend-design"
          agentId="cursor"
          rowId="cursor::frontend-design"
          variant="drawer"
        />
      </MemoryRouter>
    );

    expect(screen.queryByRole("region", { name: /来源状态|Source status/i })).toBeNull();
    expect(screen.queryByText(/只读来源|Read-only source/i)).toBeNull();
    expect(screen.getByText("~/.cursor/skills/frontend-design")).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: /加入技能集/i })
    ).toBeInTheDocument();
    expect(screen.getByText("Platform Local")).toBeInTheDocument();
  });

  // ── Skill Bundles ─────────────────────────────────────────────────────────

  it("shows collections section", () => {
    renderView();
    expect(screen.getByRole("region", { name: /技能合集/i })).toBeInTheDocument();
  });

  it("shows Add to collection button", () => {
    renderView();
    expect(
      screen.getByRole("button", { name: /加入技能集/i })
    ).toBeInTheDocument();
  });

  it("shows collection tags when collections are present", () => {
    applyStoreMocks({
      detail: {
        ...mockDetail,
        collections: [
          {
            id: "frontend",
            name: "Frontend",
            description: "Frontend patterns",
            created_at: "2026-04-09T00:00:00Z",
            updated_at: "2026-04-09T00:00:00Z",
          },
          {
            id: "design-system",
            name: "Design System",
            description: "Shared UI system",
            created_at: "2026-04-09T00:00:00Z",
            updated_at: "2026-04-09T00:00:00Z",
          },
        ],
      },
    });
    render(
      <MemoryRouter>
        <SkillDetailView skillId="frontend-design" variant="page" />
      </MemoryRouter>
    );
    expect(screen.getByText("Frontend")).toBeInTheDocument();
    expect(screen.getByText("Design System")).toBeInTheDocument();
  });

  // ── SKILL.md Preview ──────────────────────────────────────────────────────

  it("shows SKILL.md preview as markdown content", () => {
    renderView();
    expect(screen.getByRole("region", { name: /预览/i })).toBeInTheDocument();
  });

  it("does not show preview or raw source tabs for read-only content", () => {
    renderView();
    expect(screen.queryByRole("tab", { name: /预览模式/i })).toBeNull();
    expect(screen.queryByRole("tab", { name: /原始源码/i })).toBeNull();
  });

  it("renders the right detail sidebar wider by default with a resize handle", () => {
    renderView();

    const sidebar = screen.getByTestId("skill-detail-right-sidebar");
    expect(sidebar).toHaveStyle({ "--skill-detail-sidebar-width": "512px" });
    expect(screen.getByRole("separator", { name: /调整详情栏宽度/i })).toBeInTheDocument();
  });

  it("resizes the right detail sidebar by dragging the handle", () => {
    renderView();

    const sidebar = screen.getByTestId("skill-detail-right-sidebar");
    const handle = screen.getByRole("separator", { name: /调整详情栏宽度/i });

    fireEvent.mouseDown(handle, { clientX: 1200 });
    fireEvent.mouseMove(document, { clientX: 1100 });
    fireEvent.mouseUp(document);

    expect(sidebar).toHaveStyle({ "--skill-detail-sidebar-width": "612px" });
  });

  it("hides AI Explanation tab and shows the AI note button in the notes region", () => {
    renderView();
    expect(screen.queryByRole("tab", { name: /AI 解释/i })).toBeNull();
    const notesRegion = screen.getByRole("region", { name: /技能备注/i });
    expect(within(notesRegion).getByRole("button", { name: /AI 备注/i })).toBeInTheDocument();
    expect(within(notesRegion).queryByRole("button", { name: /AI 生成备注/i })).toBeNull();
  });

  it("places AI note and save note buttons in the same row with matching style", () => {
    renderView();

    const notesRegion = screen.getByRole("region", { name: /技能备注/i });
    const aiButton = within(notesRegion).getByRole("button", { name: /AI 备注/i });
    const saveButton = within(notesRegion).getByRole("button", { name: "保存" });
    const buttonRow = aiButton.parentElement;

    expect(buttonRow).toBe(saveButton.parentElement);
    expect(buttonRow).toHaveClass("flex");
    expect(aiButton.querySelector(".lucide-sparkles")).not.toBeNull();
    expect(saveButton.querySelector(".lucide-save")).not.toBeNull();
    expect(aiButton).toHaveClass("h-8", "border");
    expect(saveButton).toHaveClass("h-8", "border");
  });

  it("renders markdown content by default in preview", () => {
    renderView();
    const markdownPane = screen.getByRole("region", { name: /预览/i });
    expect(markdownPane).toBeInTheDocument();
    expect(screen.getByTestId("react-markdown")).toBeInTheDocument();
    expect(screen.getByTestId("react-markdown")).toHaveAttribute("data-has-remark-gfm", "true");
  });

  it("renders frontmatter card in preview", () => {
    renderView();
    const markdown = screen.getByRole("region", { name: /预览/i });
    expect(within(markdown).getByRole("heading", { name: /Frontmatter/i })).toBeInTheDocument();
    expect(within(markdown).getByText("frontend-design")).toBeInTheDocument();
    expect(within(markdown).getByText("Build distinctive, production-grade frontend interfaces")).toBeInTheDocument();
    expect(within(markdown).getByText("bun")).toBeInTheDocument();
    expect(within(markdown).getByText("npx")).toBeInTheDocument();
    expect(screen.getByTestId("react-markdown")).toHaveTextContent("# Frontend Design");
  });

  it("strips BOM-prefixed frontmatter before rendering markdown", () => {
    applyStoreMocks({
      content:
        "\uFEFF---\r\nname: wrangler\r\ndescription: Cloudflare Workers CLI\r\n---\r\n\r\n# Wrangler CLI\r\n\r\nBody.",
    });
    renderView("frontend-design", "page", { skipMockSetup: true });

    const markdown = screen.getByRole("region", { name: /预览/i });
    expect(within(markdown).getByText("wrangler")).toBeInTheDocument();
    expect(screen.getByTestId("react-markdown")).toHaveTextContent("# Wrangler CLI");
    expect(screen.getByTestId("react-markdown")).not.toHaveTextContent("name: wrangler");
    expect(screen.getByTestId("react-markdown")).not.toHaveTextContent("---");
  });

  it("falls back to raw frontmatter display when frontmatter is malformed", () => {
    applyStoreMocks({
      content:
        "---\nname: broken-skill\ndescription: Broken summary\nmetadata: [oops\n---\n\n# Broken Skill\n\nBody.",
    });
    renderView("frontend-design", "page", { skipMockSetup: true });

    const markdown = screen.getByRole("region", { name: /预览/i });
    expect(within(markdown).getByRole("heading", { name: /Frontmatter/i })).toBeInTheDocument();
    expect(within(markdown).getByText(/这段 frontmatter 无法稳定解析/i)).toBeInTheDocument();
    expect(within(markdown).getAllByText(/name: broken-skill/).length).toBeGreaterThan(0);
    expect(within(markdown).getAllByText(/description: Broken summary/).length).toBeGreaterThan(0);
    expect(screen.getByTestId("react-markdown")).toHaveTextContent("# Broken Skill");
  });

  it("switches preview when a non-markdown file is selected from the file tree", async () => {
    renderView();

    const filesRegion = await screen.findByRole("region", { name: /技能文件/i });
    fireEvent.click(await within(filesRegion).findByRole("button", { name: "docs" }));
    fireEvent.click(await within(filesRegion).findByRole("button", { name: "notes.txt" }));

    await waitFor(() => {
      expect(screen.getByRole("region", { name: /预览|Preview/i })).toBeInTheDocument();
      expect(screen.getByText(mockNotesContent)).toBeInTheDocument();
    });

    expect(screen.getByText("docs/notes.txt")).toBeInTheDocument();
    expect(screen.queryByTestId("react-markdown")).not.toBeInTheDocument();
  });

  it("defaults inspector documents to SKILL.md and browses folders in the file dropdown", async () => {
    applyStoreMocks();
    render(<MemoryRouter><SkillDetailView skillId="frontend-design" variant="inspector" inspectorTab="document" /></MemoryRouter>);
    const select = screen.getByRole("button",{name:"选择文件"});
    await waitFor(() => expect(select).toBeEnabled());
    expect(select).toHaveTextContent("SKILL.md");
    expect(screen.queryByRole("region",{name:/技能文件/})).not.toBeInTheDocument();
    fireEvent.click(select);
    const folder = await screen.findByRole("button",{name:"docs"});
    expect(folder).toHaveAttribute("aria-expanded","false");
    expect(screen.queryByRole("button",{name:"notes.txt"})).not.toBeInTheDocument();
    fireEvent.click(folder);
    fireEvent.click(await screen.findByRole("button",{name:"notes.txt"}));
    expect(await screen.findByText(mockNotesContent)).toBeVisible();
    expect(select).toHaveTextContent("docs/notes.txt");
    expect(screen.queryByRole("button",{name:"docs"})).not.toBeInTheDocument();
    fireEvent.click(select);
    fireEvent.click(await screen.findByRole("button",{name:"SKILL.md"}));
    await waitFor(() => expect(screen.getByTestId("react-markdown")).toBeVisible());
  });

  it("loads cached explanation when content is available", async () => {
    renderView();
    await waitFor(() => {
      expect(mockLoadCachedExplanation).toHaveBeenCalledWith("frontend-design", "zh");
    });
  });

  it("loads cached explanation with the selected compatibility row id", async () => {
    applyStoreMocks({
      detail: mockCompatibilityDetail,
      content: mockCompatibilityContent,
    });

    render(
      <MemoryRouter>
        <SkillDetailView
          skillId="frontend-design"
          agentId="cursor"
          rowId="cursor::compatibility::frontend-design"
          variant="page"
        />
      </MemoryRouter>
    );

    await waitFor(() => {
      expect(mockLoadCachedExplanation).toHaveBeenCalledWith(
        "cursor::compatibility::frontend-design",
        "zh"
      );
    });
  });

  it("uses the resolved platform detail row id for cached explanation lookup when rowId is omitted", async () => {
    applyStoreMocks({
      detail: mockPlatformManagedDetail,
      content: mockPlatformContent,
    });

    render(
      <MemoryRouter>
        <SkillDetailView
          skillId="frontend-design"
          agentId="cursor"
          variant="page"
        />
      </MemoryRouter>
    );

    await waitFor(() => {
      expect(mockLoadCachedExplanation).toHaveBeenCalledWith(
        "cursor::frontend-design",
        "zh"
      );
    });
  });

  it("copies cached AI explanation into the notes textarea", async () => {
    applyStoreMocks({ explanation: "这是缓存的技能解释。" });
    render(
      <MemoryRouter>
        <SkillDetailView skillId="frontend-design" variant="page" />
      </MemoryRouter>
    );

    fireEvent.click(screen.getByRole("button", { name: /AI 备注/i }));
    expect(screen.getByPlaceholderText(/记录此技能的用途/i)).toHaveValue("这是缓存的技能解释。");
  });

  it("calls generateExplanation from the AI note button", async () => {
    renderView();
    fireEvent.click(screen.getByRole("button", { name: /AI 备注/i }));
    await waitFor(() => {
      expect(mockRefreshExplanation).toHaveBeenCalledWith("frontend-design", mockContent, "zh");
    });
  });

  it("calls generateExplanation with the resolved platform row id", async () => {
    applyStoreMocks({
      detail: mockPlatformManagedDetail,
      content: mockPlatformContent,
    });

    render(
      <MemoryRouter>
        <SkillDetailView
          skillId="frontend-design"
          agentId="cursor"
          variant="page"
        />
      </MemoryRouter>
    );

    fireEvent.click(screen.getByRole("button", { name: /AI 备注/i }));

    await waitFor(() => {
      expect(mockRefreshExplanation).toHaveBeenCalledWith(
        "cursor::frontend-design",
        mockPlatformContent,
        "zh"
      );
    });
  });

  it("regenerates even when an explanation is cached", async () => {
    applyStoreMocks({ explanation: "这是缓存的技能解释。" });
    render(
      <MemoryRouter>
        <SkillDetailView skillId="frontend-design" variant="page" />
      </MemoryRouter>
    );

    fireEvent.click(screen.getByRole("button", { name: /AI 备注/i }));
    expect(mockGenerateExplanation).not.toHaveBeenCalled();
    expect(mockRefreshExplanation).toHaveBeenCalledWith("frontend-design", mockContent, "zh");
  });

  it("preserves original notes until generation finishes", async () => {
    let storeState = buildDetailStoreState();
    vi.mocked(useSkillDetailStore).mockImplementation((selector?: unknown) => {
      if (typeof selector === "function") return selector(storeState);
      return storeState;
    });
    vi.mocked(usePlatformStore).mockImplementation((selector?: unknown) => {
      const state = buildPlatformStoreState();
      if (typeof selector === "function") return selector(state);
      return state;
    });

    const { rerender } = render(
      <MemoryRouter>
        <SkillDetailView skillId="frontend-design" variant="page" />
      </MemoryRouter>
    );

    fireEvent.click(screen.getByRole("button", { name: /AI 备注/i }));

    storeState = buildDetailStoreState({
      explanation: "一句",
      isExplanationLoading: true,
      isExplanationStreaming: true,
    });
    rerender(
      <MemoryRouter>
        <SkillDetailView skillId="frontend-design" variant="page" />
      </MemoryRouter>
    );

    await waitFor(() => {
      expect(screen.getByPlaceholderText(/记录此技能的用途/i)).not.toHaveValue("一句");
    });

    storeState = buildDetailStoreState({
      explanation: "一句完整的 AI 备注。",
      isExplanationLoading: false,
      isExplanationStreaming: false,
    });
    rerender(
      <MemoryRouter>
        <SkillDetailView skillId="frontend-design" variant="page" />
      </MemoryRouter>
    );

    await waitFor(() => {
      expect(screen.getByPlaceholderText(/记录此技能的用途/i)).toHaveValue("一句完整的 AI 备注。");
    });
  });

  it("shows explanation loading state while a request is in flight", async () => {
    applyStoreMocks({
      explanation: null,
      isExplanationLoading: true,
      isExplanationStreaming: true,
    });

    render(
      <MemoryRouter>
        <SkillDetailView skillId="frontend-design" variant="page" />
      </MemoryRouter>
    );

    expect(screen.getByRole("button", { name: /正在加载 AI 解释/i })).toBeDisabled();
  });

  it("shows streaming indicator in the notes region once explanation content starts arriving", async () => {
    applyStoreMocks({
      explanation: "第一段解释",
      isExplanationLoading: false,
      isExplanationStreaming: true,
    });

    render(
      <MemoryRouter>
        <SkillDetailView skillId="frontend-design" variant="page" />
      </MemoryRouter>
    );

    const notesRegion = screen.getByRole("region", { name: /技能备注/i });
    expect(within(notesRegion).getByText(/正在生成解释/i)).toBeInTheDocument();
  });

  it("shows recoverable explanation error state in the notes region", async () => {
    applyStoreMocks({
      explanation: null,
      explanationError: "代理连接失败",
      explanationErrorInfo: {
        message: "代理连接失败",
        details: "error sending request",
        kind: "proxy",
        retryable: true,
        fallbackTried: true,
      },
    });

    render(
      <MemoryRouter>
        <SkillDetailView skillId="frontend-design" variant="page" />
      </MemoryRouter>
    );

    const notesRegion = screen.getByRole("region", { name: /技能备注/i });
    expect(within(notesRegion).getByText("代理连接失败")).toBeInTheDocument();
    expect(within(notesRegion).getByText(/备用端点也无法访问/i)).toBeInTheDocument();
    expect(screen.queryByText("这是缓存的技能解释。")).not.toBeInTheDocument();
    expect(within(notesRegion).getByRole("button", { name: /AI 备注/i })).toBeEnabled();
  });

  it("keeps retry action available after explanation failure", async () => {
    applyStoreMocks({
      explanation: null,
      explanationError: "temporary failure",
    });

    render(
      <MemoryRouter>
        <SkillDetailView skillId="frontend-design" variant="page" />
      </MemoryRouter>
    );

    fireEvent.click(screen.getByRole("button", { name: /AI 备注/i }));

    await waitFor(() => {
      expect(mockRefreshExplanation).toHaveBeenCalledWith("frontend-design", mockContent, "zh");
    });
  });

  // ── Loading state ─────────────────────────────────────────────────────────

  it("shows loading state when isLoading is true", () => {
    applyStoreMocks({ isLoading: true, detail: null });
    render(
      <MemoryRouter>
        <SkillDetailView skillId="frontend-design" variant="page" />
      </MemoryRouter>
    );
    expect(screen.getByText(/正在加载技能详情/i)).toBeInTheDocument();
  });

  // ── Error state ───────────────────────────────────────────────────────────

  it("shows error message when error occurs", () => {
    applyStoreMocks({ error: "Skill not found", detail: null });
    render(
      <MemoryRouter>
        <SkillDetailView skillId="frontend-design" variant="page" />
      </MemoryRouter>
    );
    expect(screen.getByText("Skill not found")).toBeInTheDocument();
  });

  it("renders a safe browser fallback when the Tauri bridge is unavailable", async () => {
    // `setup.ts` defines these without `configurable: true`, so we cannot
    // reuse `Object.defineProperty` to swap them out — assign directly
    // instead, which is allowed because `writable: true` was set initially.
    const w = window as unknown as {
      __TAURI__?: unknown;
      __TAURI_INTERNALS__?: unknown;
    };
    const prevTauri = w.__TAURI__;
    const prevInternals = w.__TAURI_INTERNALS__;
    w.__TAURI__ = undefined;
    w.__TAURI_INTERNALS__ = undefined;

    try {
      applyStoreMocks({ detail: null, content: null, error: null, isLoading: false });

      render(
        <MemoryRouter>
          <SkillDetailView skillId="defuddle" variant="page" />
        </MemoryRouter>
      );

      expect(await screen.findByText(/技能详情需要桌面运行时/i)).toBeInTheDocument();
      expect(screen.getByText(/浏览器预览中该路由现在会安全渲染/i)).toBeInTheDocument();
    } finally {
      w.__TAURI__ = prevTauri;
      w.__TAURI_INTERNALS__ = prevInternals;
    }
  });

  // ── Store calls ───────────────────────────────────────────────────────────

  it("calls loadDetail on mount with skillId prop", () => {
    renderView("frontend-design");
    expect(mockLoadDetail).toHaveBeenCalledWith({
      skillId: "frontend-design",
      agentId: undefined,
      rowId: undefined,
    });
  });

  it("passes source-aware row identity into loadDetail when provided", () => {
    applyStoreMocks();
    render(
      <MemoryRouter>
        <SkillDetailView
          skillId="frontend-design"
          agentId="cursor"
          rowId="cursor::compatibility::frontend-design"
          variant="drawer"
        />
      </MemoryRouter>
    );

    expect(mockLoadDetail).toHaveBeenCalledWith({
      skillId: "frontend-design",
      agentId: "cursor",
      rowId: "cursor::compatibility::frontend-design",
    });
  });

  it("switching duplicate platform rows updates path, content, and management affordances", async () => {
    applyStoreMocks({
      detail: mockCompatibilityDetail,
      content: mockCompatibilityContent,
    });

    const { rerender } = render(
      <MemoryRouter>
        <SkillDetailView
          skillId="frontend-design"
          agentId="cursor"
          rowId="cursor::compatibility::frontend-design"
          variant="drawer"
        />
      </MemoryRouter>
    );

    expect(screen.getByText("~/.agents/skills/frontend-design")).toBeInTheDocument();
    expect(screen.getByTestId("react-markdown")).toHaveTextContent("# Plugin Frontend Design");
    expect(screen.queryByRole("button", { name: /加入技能集/i })).toBeNull();

    mockLoadDetail.mockClear();
    applyStoreMocks({
      detail: mockPlatformManagedDetail,
      content: mockPlatformContent,
    });

    rerender(
      <MemoryRouter>
        <SkillDetailView
          skillId="frontend-design"
          agentId="cursor"
          rowId="cursor::frontend-design"
          variant="drawer"
        />
      </MemoryRouter>
    );

    await waitFor(() => {
      expect(mockLoadDetail).toHaveBeenCalledWith({
        skillId: "frontend-design",
        agentId: "cursor",
        rowId: "cursor::frontend-design",
      });
    });

    expect(screen.getByText("~/.cursor/skills/frontend-design")).toBeInTheDocument();
    expect(screen.getByTestId("react-markdown")).toHaveTextContent("# User Frontend Design");
    expect(screen.getByRole("button", { name: /加入技能集/i })).toBeInTheDocument();
    expect(screen.queryByText(/只读来源|Read-only source/i)).toBeNull();
  });

  it("retries a failed duplicate detail load with the same row identity", async () => {
    applyStoreMocks({
      detail: null,
      content: null,
      error: "Multiple rows found",
    });

    render(
      <MemoryRouter>
        <SkillDetailView
          skillId="frontend-design"
          agentId="cursor"
          rowId="cursor::frontend-design"
          variant="drawer"
        />
      </MemoryRouter>
    );

    mockLoadDetail.mockClear();
    fireEvent.click(screen.getByRole("button", { name: /重试/i }));

    await waitFor(() => {
      expect(mockLoadDetail).toHaveBeenCalledWith({
        skillId: "frontend-design",
        agentId: "cursor",
        rowId: "cursor::frontend-design",
      });
    });
  });

  it("calls reset on unmount", () => {
    const { unmount } = renderView();
    unmount();
    expect(mockReset).toHaveBeenCalled();
  });

  // ── CollectionPickerDialog integration ────────────────────────────────────

  it("does not render CollectionPickerDialog by default", () => {
    renderView();
    expect(screen.queryByTestId("collection-picker-dialog")).toBeNull();
  });

  it("opens CollectionPickerDialog when Add to collection is clicked", async () => {
    renderView();
    const addBtn = screen.getByRole("button", { name: /加入技能集/i });
    fireEvent.click(addBtn);
    await waitFor(() => {
      expect(screen.getByTestId("collection-picker-dialog")).toBeInTheDocument();
    });
  });

  it("passes current collection ids into CollectionPickerDialog for preselection", async () => {
    applyStoreMocks({
      detail: {
        ...mockDetail,
        collections: [
          {
            id: "frontend",
            name: "Frontend",
            description: "Frontend patterns",
            created_at: "2026-04-09T00:00:00Z",
            updated_at: "2026-04-09T00:00:00Z",
          },
          {
            id: "design-system",
            name: "Design System",
            description: "Shared UI system",
            created_at: "2026-04-09T00:00:00Z",
            updated_at: "2026-04-09T00:00:00Z",
          },
        ],
      },
    });
    renderView("frontend-design", "page", { skipMockSetup: true });

    fireEvent.click(screen.getByRole("button", { name: /加入技能集/i }));

    await waitFor(() => {
      expect(screen.getByTestId("collection-picker-dialog")).toHaveAttribute(
        "data-current-collection-ids",
        "frontend,design-system"
      );
    });
  });

  it("closes CollectionPickerDialog when cancel is clicked inside it", async () => {
    renderView();
    fireEvent.click(screen.getByRole("button", { name: /加入技能集/i }));
    await waitFor(() => {
      expect(screen.getByTestId("collection-picker-dialog")).toBeInTheDocument();
    });
    fireEvent.click(screen.getByRole("button", { name: /Cancel picker/i }));
    await waitFor(() => {
      expect(screen.queryByTestId("collection-picker-dialog")).toBeNull();
    });
  });

  it("restores focus to the add-to-collection trigger after closing the picker", async () => {
    renderView();
    const addBtn = screen.getByRole("button", { name: /加入技能集/i });
    fireEvent.click(addBtn);

    await waitFor(() => {
      expect(screen.getByTestId("collection-picker-dialog")).toBeInTheDocument();
    });

    fireEvent.click(screen.getByRole("button", { name: /Cancel picker/i }));

    await waitFor(() => {
      expect(screen.queryByTestId("collection-picker-dialog")).toBeNull();
    });

    expect(addBtn).toHaveFocus();
  });

  it("calls loadDetail to refresh skill after collections are added", async () => {
    renderView();
    mockLoadDetail.mockClear(); // clear the initial load call

    fireEvent.click(screen.getByRole("button", { name: /加入技能集/i }));
    await waitFor(() => {
      expect(screen.getByTestId("collection-picker-dialog")).toBeInTheDocument();
    });

    // Simulate confirming the picker (which calls onAdded then closes)
    fireEvent.click(screen.getByRole("button", { name: /Confirm add to collection/i }));

    await waitFor(() => {
      expect(mockLoadDetail).toHaveBeenCalledWith({
        skillId: "frontend-design",
        agentId: undefined,
        rowId: undefined,
      });
    });
  });
});


describe("AI tags", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.mocked(useSkillDetailStore).mockImplementation((selector) => selector(buildDetailStoreState()));
    vi.mocked(usePlatformStore).mockImplementation((selector) => selector(buildPlatformStoreState()));
  });

  it("fills editable tags and saves only after confirmation", async () => {
    mockGenerateTags.mockResolvedValue(["前端开发", "界面设计", "交互设计"]);
    renderView();
    fireEvent.click(screen.getByRole("button", { name: "AI 标签" }));
    await waitFor(() => expect(screen.getByPlaceholderText(/输入标签/)).toHaveValue("前端开发, 界面设计, 交互设计"));
    expect(mockGenerateTags).toHaveBeenCalledWith(mockContent);
    expect(mockUpdateMetadata).not.toHaveBeenCalled();
    fireEvent.click(within(screen.getByRole("region", {name:"技能标签"})).getByRole("button", {name:"保存"}));
    await waitFor(() => expect(mockUpdateMetadata).toHaveBeenCalledWith(mockDetail.id, {
      notes: mockDetail.notes ?? null, tags: ["前端开发", "界面设计", "交互设计"],
    }));
  });

  it.each([["技能备注", "保存"], ["技能标签", "保存"], ["技能备注", "清空"], ["技能标签", "清空"]])("only shows feedback for %s %s", async (regionName, action) => {
    let finish!: () => void;
    mockUpdateMetadata.mockImplementationOnce(() => new Promise<void>(resolve => { finish = resolve; }));
    renderView();
    const region = screen.getByRole("region", {name: regionName});
    const other = screen.getByRole("region", {name: regionName === "技能备注" ? "技能标签" : "技能备注"});
    fireEvent.click(within(region).getByRole("button", {name: action}));
    expect(within(other).getByRole("button", {name: "保存"})).toBeInTheDocument();
    expect(within(other).getByRole("button", {name: "保存"})).toHaveClass("disabled:opacity-50");
    expect(within(other).getByRole("button", {name: "清空"})).toHaveClass("disabled:opacity-50");
    expect(other.querySelector(".animate-spin")).toBeNull();
    expect(region.querySelectorAll(".animate-spin")).toHaveLength(1);
    if (action === "清空") expect(within(region).getByRole("button", {name: "保存"})).toBeInTheDocument();
    await act(async () => finish());
  });

  it("appends AI suggestions to existing draft tags without duplicates", async () => {
    mockGenerateTags.mockResolvedValue(["UI", "新标签"]);
    renderView();
    fireEvent.change(screen.getByPlaceholderText(/输入标签/), {target: {value: "原有标签, ui"}});
    fireEvent.click(screen.getByRole("button", {name: "AI 标签"}));
    await waitFor(() => expect(screen.getByPlaceholderText(/输入标签/)).toHaveValue("原有标签, ui, 新标签"));
    expect(mockUpdateMetadata).not.toHaveBeenCalled();
  });

  it("preserves existing input on failure", async () => {
    mockGenerateTags.mockRejectedValue(new Error("AI unavailable"));
    renderView();
    fireEvent.change(screen.getByPlaceholderText(/输入标签/), { target: { value: "原有标签" } });
    fireEvent.click(screen.getByRole("button", { name: "AI 标签" }));
    await waitFor(() => expect(screen.getByRole("button", { name: "AI 标签" })).toBeEnabled());
    expect(screen.getByPlaceholderText(/输入标签/)).toHaveValue("原有标签");
    expect(mockUpdateMetadata).not.toHaveBeenCalled();
  });
  it("clears saved tags and ignores suggestions that arrive after clear", async () => {
    let finish!: (tags:string[])=>void;
    mockGenerateTags.mockImplementation(()=>new Promise<string[]>(resolve=>{finish=resolve;}));
    renderView();
    fireEvent.click(screen.getByRole("button", {name:"AI 标签"}));
    fireEvent.click(within(screen.getByRole("region", {name:"技能标签"})).getByRole("button", {name:"清空"}));
    await waitFor(()=>expect(mockUpdateMetadata).toHaveBeenCalledWith(mockDetail.id,{notes:mockDetail.notes??null,tags:[]}));
    await act(async()=>finish(["late suggestion"]));
    expect(screen.getByPlaceholderText(/输入标签/)).toHaveValue("");
  });

});
