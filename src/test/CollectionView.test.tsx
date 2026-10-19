import { openRowActions } from "./rowActions";
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { useEffect } from "react";
import { render as rtlRender, screen, fireEvent, waitFor, within } from "@testing-library/react";
import {
  MemoryRouter,
  Route,
  Routes,
  useLocation,
  type Location,
} from "react-router-dom";
import { CollectionView } from "../pages/CollectionView";
import { CollectionDetail, AgentWithStatus, SkillWithLinks } from "../types";
import {
  consumeScrollPosition,
} from "../lib/scrollRestoration";

// Subscribes to location changes so tests can assert on navigation state
// without depending on `window.history.state`, which MemoryRouter does not
// update.
function LocationProbe({
  onChange,
}: {
  onChange: (location: Location) => void;
}) {
  const location = useLocation();
  useEffect(() => {
    onChange(location);
  }, [location, onChange]);
  return null;
}

// Mock stores
vi.mock("../stores/collectionStore", () => ({
  useCollectionStore: vi.fn(),
}));

vi.mock("../stores/platformStore", () => ({
  usePlatformStore: vi.fn(),
}));

vi.mock("../stores/resourceLibraryStore", () => ({
  useResourceLibraryStore: vi.fn(),
}));

import { useCollectionStore } from "../stores/collectionStore";
import { usePlatformStore } from "../stores/platformStore";
import { useResourceLibraryStore } from "../stores/resourceLibraryStore";

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

const mockCollectionDetail: CollectionDetail = {
  id: "col-1",
  name: "Frontend",
  description: "Frontend skills collection",
  created_at: "2026-04-09T00:00:00Z",
  updated_at: "2026-04-09T00:00:00Z",
  skills: [
    {
      id: "frontend-design",
      name: "frontend-design",
      description: "Build distinctive frontend UIs",
      file_path: "~/.agents/skills/frontend-design/SKILL.md",
      is_central: true,
      scanned_at: "2026-04-09T00:00:00Z",
    },
    {
      id: "code-reviewer",
      name: "code-reviewer",
      description: "Review code changes",
      file_path: "~/.agents/skills/code-reviewer/SKILL.md",
      is_central: true,
      scanned_at: "2026-04-09T00:00:00Z",
    },
  ],
};

const mockResourceSkills: SkillWithLinks[] = [
  {
    id: "frontend-design",
    name: "frontend-design",
    description: "Build distinctive frontend UIs",
    file_path: "~/.arsenal/library/frontend-design/SKILL.md",
    canonical_path: "~/.arsenal/library/frontend-design",
    is_central: false,
    scanned_at: "2026-04-09T00:00:00Z",
    linked_agents: [],
    read_only_agents: [],
  },
  {
    id: "code-reviewer",
    name: "code-reviewer",
    description: "Review code changes",
    file_path: "~/.arsenal/library/code-reviewer/SKILL.md",
    canonical_path: "~/.arsenal/library/code-reviewer",
    is_central: false,
    scanned_at: "2026-04-09T00:00:00Z",
    linked_agents: [],
    read_only_agents: [],
  },
];

const mockLoadCollectionDetail = vi.fn();
const mockRemoveSkillFromCollection = vi.fn();
const mockDeleteCollection = vi.fn();
const mockExportCollection = vi.fn();
const mockLoadResourceLibrary = vi.fn();
const mockInstallResourceSkill = vi.fn();
const mockUseCollectionStore = vi.mocked(useCollectionStore);
const mockUsePlatformStore = vi.mocked(usePlatformStore);
const mockUseResourceLibraryStore = vi.mocked(useResourceLibraryStore);

function buildCollectionStoreState(overrides = {}) {
  return {
    collections: [], activeCollectionId:null, groupDetails:[], selectedSkillIds:[], includeDescendants:true, selectGroup:vi.fn(), loadGroupDetails:vi.fn().mockResolvedValue(undefined), changeGroup:vi.fn().mockResolvedValue(undefined),
    currentDetail: mockCollectionDetail,
    isLoading: false,
    isLoadingDetail: false,
    error: null,
    loadCollections: vi.fn(),
    createCollection: vi.fn(),
    updateCollection: vi.fn(),
    deleteCollection: mockDeleteCollection,
    loadCollectionDetail: mockLoadCollectionDetail,
    addSkillToCollection: vi.fn(),
    removeSkillFromCollection: mockRemoveSkillFromCollection,
    batchInstallCollection: vi.fn(),
    exportCollection: mockExportCollection,
    importCollection: vi.fn(),
    refreshCounts: vi.fn(),
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
    rescan: vi.fn(),
    refreshCounts: vi.fn(),
    ...overrides,
  };
}

function buildResourceLibraryStoreState(overrides = {}) {
  return {
    skills: mockResourceSkills,
    agents: mockAgents,
    resourceLibraryDir: "~/.arsenal/library",
    isLoading: false,
    isInstalling: false,
    isUpdatingSources: false,
    togglingAgentId: null,
    deletingSkillId: null,
    error: null,
    loadResourceLibrary: mockLoadResourceLibrary,
    installSkill: mockInstallResourceSkill,
    togglePlatformLink: vi.fn(),
    updateSourceBackedSkills: vi.fn(),
    previewRepositorySync: vi.fn(),
    syncSourceBackedSkills: vi.fn(),
    updateSourceBackedSkill: vi.fn(),
    importGitHubRepoSnapshot: vi.fn(),
    exportDirectoryList: vi.fn(),
    addLocalSkills: vi.fn(),
    createManualSkill: vi.fn(),
    previewDeleteResourceBundle: vi.fn(),
    deleteResourceBundle: vi.fn(),
    deleteResourceSkill: vi.fn(),
    addToCentral: vi.fn(),
    removeFromCentral: vi.fn(),
    ...overrides,
  };
}

function renderCollectionView(collectionId = "col-1", storeOverrides = {}) {
  mockUseCollectionStore.mockImplementation((selector) =>
    selector(buildCollectionStoreState(storeOverrides))
  );
  mockUsePlatformStore.mockImplementation((selector) =>
    selector(buildPlatformStoreState())
  );
  mockUseResourceLibraryStore.mockImplementation((selector) =>
    selector(buildResourceLibraryStoreState())
  );

  return render(
    <MemoryRouter initialEntries={[`/collection/${collectionId}`]}>
      <Routes>
        <Route path="/collection/:collectionId" element={<CollectionView />} />
        <Route path="/central" element={<div>Shared Hub</div>} />
      </Routes>
    </MemoryRouter>
  );
}

// ─── Tests ────────────────────────────────────────────────────────────────────

describe("CollectionView", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    localStorage.removeItem("skills-manage.skillTableColumns.tree.v5");
  });

  // ── Loading and Data Display ───────────────────────────────────────────────

  it("calls loadCollectionDetail on mount", () => {
    renderCollectionView("col-1");
    expect(mockLoadCollectionDetail).toHaveBeenCalledWith("col-1");
  });

  it("renders collection name and description", () => {
    renderCollectionView();
    expect(screen.getByRole("heading", { name: /Frontend · 2/ })).toBeInTheDocument();
    expect(screen.getByText("Frontend skills collection")).toBeInTheDocument();
  });

  it("renders member skills list", () => {
    localStorage.setItem("skills-manage.skillTableColumns.tree.v5", JSON.stringify(["index","name","source","skillCount","createdAt","updatedAt","installSummary","actions"]));
    renderCollectionView();
    expect(screen.getByText("frontend-design")).toBeInTheDocument();
    expect(screen.getByText("code-reviewer")).toBeInTheDocument();
    expect(screen.getByRole("columnheader", { name: "名称" })).toBeInTheDocument();
    expect(screen.getByRole("columnheader", { name: "技能数" })).toBeInTheDocument();
    expect(screen.getAllByText("未安装").length).toBeGreaterThan(0);
    expect(screen.queryByText(/共享中心：平台/)).not.toBeInTheDocument();
  });

  it("does not show a folder view toggle for collections", () => {
    renderCollectionView();
    expect(screen.queryByRole("button", { name: "平铺" })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "目录" })).not.toBeInTheDocument();
  });

  it("shows loading state when isLoadingDetail is true", () => {
    renderCollectionView("col-1", { isLoadingDetail: true, currentDetail: null });
    expect(screen.getByText(/正在加载技能集/i)).toBeInTheDocument();
  });

  it("shows empty skills state when collection has no skills", () => {
    renderCollectionView("col-1", {
      currentDetail: { ...mockCollectionDetail, skills: [] },
    });
    expect(screen.getByText(/没有匹配的技能/i)).toBeInTheDocument();
  });

  // ── Remove Skill ───────────────────────────────────────────────────────────

  it("calls removeSkillFromCollection only after inline confirmation", async () => {
    mockRemoveSkillFromCollection.mockResolvedValueOnce(undefined);
    renderCollectionView();

    const skillRow = screen.getByRole("row", { name: /code-reviewer/i });
    fireEvent.click(openRowActions(skillRow).getByRole("menuitem", { name: "删除" }));
    expect(mockRemoveSkillFromCollection).not.toHaveBeenCalled();

    fireEvent.click(screen.getByRole("menuitem", { name: /确认删除/i }));

    await waitFor(() => {
      expect(mockRemoveSkillFromCollection).toHaveBeenCalledWith("col-1", "code-reviewer");
    });
  });

  it("shows central, target, and trash actions for collection skills", () => {
    renderCollectionView();

    const skillRow = screen.getByRole("row", { name: /code-reviewer/i });
    expect(
      openRowActions(skillRow).getByRole("menuitem", { name: "加入共享中心" })
    ).toBeInTheDocument();
    expect(
      openRowActions(skillRow).getByRole("menuitem", { name: "安装" })
    ).toBeInTheDocument();
    expect(
      openRowActions(skillRow).getByRole("menuitem", { name: "删除" }).querySelector(".lucide-trash-2")
    ).toBeInTheDocument();
  });

  // ── Action Buttons ─────────────────────────────────────────────────────────

  it("renders Edit, Delete, Add Skill, and Install buttons", () => {
    renderCollectionView();
    expect(screen.getByRole("heading", { name: /Frontend · 2/ })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /编辑技能集/i })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /删除技能集/i })).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /导出技能集/i })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /导入技能集/i })).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: /添加技能到技能集/i })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /^安装$/ })).toBeInTheDocument();
  });

  it("opens the install dialog with Shared Hub and every target unchecked", async () => {
    renderCollectionView();
    fireEvent.click(screen.getByRole("button", { name: /^安装$/ }));

    const dialog = await screen.findByRole("dialog", { name: /批量安装 — Frontend/i });
    expect(within(dialog).getByRole("heading", { name: "共享中心" })).toBeInTheDocument();
    expect(within(dialog).getByLabelText("Claude Code")).not.toBeChecked();
    expect(within(dialog).getByLabelText("Cursor")).not.toBeChecked();
    expect(within(dialog).getByLabelText("共享中心")).not.toBeChecked();
  });

  // ── Error State ───────────────────────────────────────────────────────────

  it("shows error when loading fails", () => {
    renderCollectionView("col-1", {
      currentDetail: null,
      isLoadingDetail: false,
      error: "Collection not found",
    });
    expect(screen.getByText(/Collection not found/i)).toBeInTheDocument();
  });

  // ── Return-position restoration ───────────────────────────────────────────

  describe("scroll restoration", () => {
    afterEach(() => {
      // Clear any leftover scroll map entries between tests.
      consumeScrollPosition("collection:col-1");
      consumeScrollPosition("collection:col-2");
    });

    type InitialEntry =
      | string
      | {
          pathname: string;
          state?: unknown;
          search?: string;
          hash?: string;
        };

    function renderWithState(
      initialEntry: InitialEntry,
      storeOverrides: Record<string, unknown> = {},
      onLocationChange?: (location: Location) => void
    ) {
      mockUseCollectionStore.mockImplementation((selector) =>
        selector(buildCollectionStoreState(storeOverrides))
      );
      mockUsePlatformStore.mockImplementation((selector) =>
        selector(buildPlatformStoreState())
      );
      mockUseResourceLibraryStore.mockImplementation((selector) =>
        selector(buildResourceLibraryStoreState())
      );

      return render(
        <MemoryRouter initialEntries={[initialEntry]}>
          {onLocationChange && (
            <LocationProbe onChange={onLocationChange} />
          )}
          <Routes>
            <Route path="/collection/:collectionId" element={<CollectionView />} />
            <Route path="/skill/:skillId" element={<div>detail-route</div>} />
          </Routes>
        </MemoryRouter>
      );
    }







    it("does not restore scroll when the restoration key targets a different collection", async () => {
      // The stored key points at col-2 but the route is col-1 — this should be
      // ignored so we don't cross-contaminate other collection contexts.
      renderWithState({
        pathname: "/collection/col-1",
        state: {
          collectionContext: { collectionId: "col-2" },
          scrollRestoration: { key: "collection:col-2", scrollTop: 999 },
        },
      });

      const scroller = screen
        .getByText("frontend-design")
        .closest("[class*='overflow-y-auto']");
      expect(scroller).not.toBeNull();
      if (!scroller) return;

      // Give the effect an opportunity to run — scrollTop should remain 0.
      await waitFor(() => {
        expect((scroller as HTMLDivElement).scrollTop).toBe(0);
      });
    });




  });
});

function render(...args: Parameters<typeof rtlRender>) {
  const result = rtlRender(...args);
  const list = screen.queryByRole("region", {name:"技能列表"});
  if (list) fireEvent.keyDown(list,{key:"+"});
  return result;
}
