import { openRowActions } from "./rowActions";
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { useEffect } from "react";
import { render, screen, fireEvent, waitFor, within } from "@testing-library/react";
import { MemoryRouter, Route, Routes, useLocation, type Location } from "react-router-dom";
import { CollectionsListView } from "../pages/CollectionsListView";
import {
  Collection,
  CollectionDetail,
  AgentWithStatus,
  SkillWithLinks,
} from "../types";
import {
  clearReturnContext,
  consumeScrollPosition,
} from "../lib/scrollRestoration";

vi.mock("../components/skill/SkillDetailView", () => ({ MetadataRow: ({label,value}: {label:string;value:string}) => <div>{label}: {value}</div>, SkillDetailView: ({ skillId }: { skillId: string }) => <div data-testid="inspector">{skillId}</div> }));

// ─── Mock stores ──────────────────────────────────────────────────────────────

vi.mock("../stores/collectionStore", () => ({
  useCollectionStore: vi.fn(),
  getCollectionDetails: (...args: unknown[]) => mockGetCollectionDetails(...args),
}));

vi.mock("../stores/platformStore", () => ({
  usePlatformStore: vi.fn(),
}));

vi.mock("../stores/resourceLibraryStore", () => ({
  useResourceLibraryStore: vi.fn(),
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
    id: "central",
    display_name: "Shared Hub",
    global_skills_dir: "~/.agents/skills/",
    is_detected: true,
    is_builtin: true,
    is_enabled: true,
  },
];

const mockCollections: Collection[] = [
  {
    id: "col-1",
    name: "Frontend",
    description: "Frontend skills",
    created_at: "2026-04-09T00:00:00Z",
    updated_at: "2026-04-09T00:00:00Z",
  },
  {
    id: "col-2",
    name: "Backend",
    description: "Backend skills",
    created_at: "2026-04-09T00:00:00Z",
    updated_at: "2026-04-09T00:00:00Z",
  },
  {
    id: "col-3",
    name: "Infra",
    description: "Infrastructure skills",
    created_at: "2026-04-09T00:00:00Z",
    updated_at: "2026-04-09T00:00:00Z",
  },
];

const mockDetailCol1: CollectionDetail = {
  id: "col-1",
  name: "Frontend",
  description: "Frontend skills",
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

const mockDetailCol2: CollectionDetail = {
  id: "col-2",
  name: "Backend",
  description: "Backend skills",
  created_at: "2026-04-09T00:00:00Z",
  updated_at: "2026-04-09T00:00:00Z",
  skills: [
    {
      id: "api-designer",
      name: "api-designer",
      description: "Design REST APIs",
      file_path: "~/.agents/skills/api-designer/SKILL.md",
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
  },
];

// ─── Store builders ───────────────────────────────────────────────────────────

const mockLoadCollections = vi.fn();
const mockLoadCollectionDetail = vi.fn();
const mockGetCollectionDetails = vi.fn(async (..._args: unknown[]) => [mockDetailCol1, {...mockDetailCol2, skills:mockDetailCol2.skills.map(skill => ({...skill,tags:["API"]}))}]);
const mockRemoveSkill = vi.fn();
const mockDeleteCollection = vi.fn();
const mockBatchInstallCollection = vi.fn();
const mockExportCollection = vi.fn();
const mockImportCollection = vi.fn();
const mockAddSkillToCollection = vi.fn();
const mockRefreshCounts = vi.fn();
const mockLoadResourceLibrary = vi.fn();
const mockInstallResourceSkill = vi.fn();

const mockUseCollectionStore = vi.mocked(useCollectionStore);
const mockUsePlatformStore = vi.mocked(usePlatformStore);
const mockUseResourceLibraryStore = vi.mocked(useResourceLibraryStore);

function buildCollectionStoreState(overrides: Record<string, unknown> = {}) {
  return {
    collections: mockCollections,
    currentDetail: mockDetailCol1,
    isLoading: false,
    isLoadingDetail: false,
    error: null,
    loadCollections: mockLoadCollections,
    createCollection: vi.fn(),
    updateCollection: vi.fn(),
    deleteCollection: mockDeleteCollection,
    loadCollectionDetail: mockLoadCollectionDetail,
    getCollectionDetails: mockGetCollectionDetails,
    addSkillToCollection: mockAddSkillToCollection,
    removeSkillFromCollection: mockRemoveSkill,
    batchInstallCollection: mockBatchInstallCollection,
    exportCollection: mockExportCollection,
    importCollection: mockImportCollection,
    refreshCounts: vi.fn(),
    ...overrides,
  };
}

function buildPlatformStoreState() {
  return {
    agents: mockAgents,
    skillsByAgent: {},
    isLoading: false,
    isRefreshing: false,
    error: null,
    initialize: vi.fn(),
    rescan: vi.fn(),
    refreshCounts: mockRefreshCounts,
  };
}

function buildResourceLibraryStoreState() {
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
    updateSourceBackedSkill: vi.fn(),
    createManualSkill: vi.fn(),
    previewDeleteResourceBundle: vi.fn(),
    deleteResourceBundle: vi.fn(),
    deleteResourceSkill: vi.fn(),
    addToCentral: vi.fn(),
    removeFromCentral: vi.fn(),
  };
}

function applyStoreMocks(collectionOverrides: Record<string, unknown> = {}) {
  mockUseCollectionStore.mockImplementation((selector: unknown) => {
    const state = buildCollectionStoreState(collectionOverrides);
    if (typeof selector === "function") return selector(state);
    return state;
  });
  mockUsePlatformStore.mockImplementation((selector: unknown) => {
    const state = buildPlatformStoreState();
    if (typeof selector === "function") return selector(state);
    return state;
  });
  mockUseResourceLibraryStore.mockImplementation((selector: unknown) => {
    const state = buildResourceLibraryStoreState();
    if (typeof selector === "function") return selector(state);
    return state;
  });
}

type InitialEntry =
  | string
  | { pathname: string; state?: unknown; search?: string; hash?: string };

// Helper that subscribes to location changes and forwards them to a spy
// function. Lets tests assert on the state we emit when navigating into a
// skill detail without relying on `window.history.state`, which MemoryRouter
// does not update.
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

function renderList(
  initialEntry: InitialEntry = "/collections",
  collectionOverrides: Record<string, unknown> = {},
  onLocationChange?: (location: Location) => void
) {
  applyStoreMocks(collectionOverrides);
  const result = render(
    <MemoryRouter initialEntries={[initialEntry]}>
      {onLocationChange && <LocationProbe onChange={onLocationChange} />}
      <Routes>
        <Route path="/collections" element={<CollectionsListView />} />
        <Route path="/skill/:skillId" element={<div>detail-route</div>} />
      </Routes>
    </MemoryRouter>
  );
  const list = screen.queryByRole("region", {name:"技能列表"});
  if (list) fireEvent.keyDown(list,{key:"+"});
  return result;
}

// ─── Tests ────────────────────────────────────────────────────────────────────

describe("CollectionsListView", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    localStorage.clear();
  });

  afterEach(() => {
    consumeScrollPosition("collection:col-1");
    consumeScrollPosition("collection:col-2");
    consumeScrollPosition("collection:col-3");
    clearReturnContext("collections");
  });

  // ── Basic rendering ───────────────────────────────────────────────────────

  it("filters collections by member tags before opening their details", async () => {
    renderList();
    const tags = within(screen.getByRole("group",{name:"标签"}));
    const api = await tags.findByRole("button",{name:"API"});
    fireEvent.click(api);
    expect(api).toHaveAttribute("aria-pressed","true");
    expect(screen.getByRole("button",{name:"Backend"})).toBeInTheDocument();
    expect(screen.queryByRole("button",{name:"Frontend"})).not.toBeInTheDocument();
    expect(screen.queryByRole("button",{name:"Infra"})).not.toBeInTheDocument();
    const search = screen.getByRole("textbox");
    fireEvent.change(search,{target:{value:"Frontend"}});
    expect(screen.queryByRole("button",{name:"Backend"})).not.toBeInTheDocument();
    fireEvent.change(search,{target:{value:""}});
    fireEvent.click(api,{ctrlKey:true});
    expect(screen.getByRole("button",{name:"Frontend"})).toBeInTheDocument();
    expect(screen.getByRole("button",{name:"Infra"})).toBeInTheDocument();
  });

  it("renders collection chips for all collections", () => {
    renderList();
    // "Frontend" appears both as a chip and in the collection header for the
    // auto-selected first collection — so we assert it shows up at least once.
    expect(screen.getAllByText("Frontend").length).toBeGreaterThanOrEqual(1);
    expect(screen.getByText("Backend")).toBeInTheDocument();
    expect(screen.getByText("Infra")).toBeInTheDocument();
  });

  it("shows a refresh button next to the collections title", () => {
    renderList();
    expect(screen.getByRole("button", { name: "刷新技能分组" })).toBeInTheDocument();
  });

  it("reloads collections, the selected detail, and the skill repository on refresh", async () => {
    renderList();
    mockLoadCollections.mockClear();
    mockLoadCollectionDetail.mockClear();
    mockLoadResourceLibrary.mockClear();
    mockRefreshCounts.mockClear();

    fireEvent.click(screen.getByRole("button", { name: "刷新技能分组" }));

    await waitFor(() => {
      expect(mockLoadCollections).toHaveBeenCalled();
      expect(mockLoadResourceLibrary).toHaveBeenCalled();
      expect(mockRefreshCounts).toHaveBeenCalled();
      expect(mockLoadCollectionDetail).toHaveBeenCalledWith("col-1");
    });
  });

  it("deletes the right-clicked collection instead of the selected collection", async () => {
    const confirm = vi.spyOn(window, "confirm").mockReturnValue(true);
    renderList();
    fireEvent.click(openRowActions(screen.getByRole("button", {name:"Backend"}).closest("tr")!).getByRole("menuitem", {name:"删除"}));
    await waitFor(() => expect(mockDeleteCollection).toHaveBeenCalledWith("col-2"));
    expect(confirm).toHaveBeenCalledWith(expect.stringContaining("Backend"));
    confirm.mockRestore();
  });

  it("auto-selects the first collection when no context is provided", () => {
    renderList();
    // loadCollectionDetail should have been called for the first collection.
    expect(mockLoadCollectionDetail).toHaveBeenCalledWith("col-1");
  });

  it("keeps the selected collection in a compact toolbar without import, export, or folder views", () => {
    renderList();

    expect(screen.queryByRole("button", { name: /导入技能集/i })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /^导出$/ })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "平铺" })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "目录" })).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: /新建分组/i })).toBeInTheDocument();
    const list = screen.getByLabelText("技能列表", {selector:"section"});
    expect(within(list).getAllByRole("row")).toHaveLength(4);
    const menu = openRowActions(screen.getByRole("button", {name:"Frontend"}).closest("tr")!);
    for (const name of ["编辑", "删除", "安装", "添加技能"]) expect(menu.getByRole("menuitem", {name})).toBeInTheDocument();

  });

  it("opens the install dialog with Shared Hub and every target unchecked", async () => {
    renderList();

    fireEvent.click(openRowActions(screen.getByRole("button", {name:"Frontend"}).closest("tr")!).getByRole("menuitem", {name:"安装"}));

    const dialog = await screen.findByRole("dialog", { name: /批量安装 — Frontend/i });
    expect(within(dialog).getByRole("heading", { name: "共享中心" })).toBeInTheDocument();
    expect(within(dialog).getByLabelText("Claude Code")).not.toBeChecked();
    expect(within(dialog).getByLabelText("共享中心")).not.toBeChecked();
  });

  it("requires a second confirmation click before removing a skill from the selected collection", async () => {
    mockRemoveSkill.mockResolvedValueOnce(undefined);
    renderList();

    const skillRow = screen.getByRole("row", { name: /frontend-design/i });
    fireEvent.click(openRowActions(skillRow).getByRole("menuitem", { name: "删除" }));
    expect(mockRemoveSkill).not.toHaveBeenCalled();

    fireEvent.click(screen.getByRole("menuitem", { name: /确认删除/i }));

    await waitFor(() => {
      expect(mockRemoveSkill).toHaveBeenCalledWith("col-1", "frontend-design");
    });
  });

  // ── Collection context restoration (COLL-RETURN-001) ─────────────────────

  it("restores the prior collection context from navigation state on entry", async () => {
    renderList(
      {
        pathname: "/collections",
        state: {
          collectionContext: { collectionId: "col-2" },
          scrollRestoration: { key: "collection:col-2", scrollTop: 0 },
        },
      },
      { currentDetail: mockDetailCol2 }
    );

    // Should load the restored collection rather than auto-selecting col-1.
    expect(mockLoadCollectionDetail).toHaveBeenCalledWith("col-2");
    await waitFor(() => {
      expect(screen.getByText("api-designer")).toBeInTheDocument();
    });
  });

  // ── Forward navigation state emission ─────────────────────────────────────

  it("selects skills in the persistent inspector without leaving the collection", () => {
    const locations: Location[] = [];
    renderList("/collections", {}, loc => locations.push(loc));
    fireEvent.click(screen.getByRole("button", { name: /查看 frontend-design 的详情/ }));
    expect(screen.getByTestId("inspector")).toHaveTextContent("frontend-design");
    fireEvent.click(screen.getByRole("button", {name:"Frontend"}));
    fireEvent.click(screen.getByRole("button", { name: /查看 code-reviewer 的详情/ }));
    expect(screen.getByTestId("inspector")).toHaveTextContent("code-reviewer");
    expect(screen.queryByTestId("skill-detail-drawer")).not.toBeInTheDocument();
    expect(locations.at(-1)?.pathname).toBe("/collections");
  });

  it("preserves the collection and list position when hiding the inspector", () => {
    renderList();
    const trigger = screen.getByRole("button", { name: /查看 frontend-design 的详情/ });
    const scroller = screen.getByRole("button", {name:"Frontend"}).closest("[class*='overflow-y-auto']") as HTMLElement;
    scroller.scrollTop = 260;
    fireEvent.click(trigger);
    fireEvent.click(screen.getByRole("button", { name: "收起预览" }));
    expect(screen.getByRole("button", { name: "Frontend" })).toBeInTheDocument();
    expect(scroller.scrollTop).toBe(260);
    fireEvent.click(screen.getByRole("button", { name: "显示预览" }));
    expect(screen.getByTestId("inspector")).toHaveTextContent("frontend-design");
  });
});
