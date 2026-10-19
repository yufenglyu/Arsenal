vi.mock("../components/notebook/NotebookNavigation", () => ({NotebookNavigation: () => null}));
vi.mock("../stores/settingsStore", () => {
  const state = {scanDirectories: [], loadScanDirectories: vi.fn()};
  return {useSettingsStore: (selector?: (value: typeof state) => unknown) => selector ? selector(state) : state};
});
import { describe, it, expect, vi } from "vitest";
import { render, screen, act } from "@testing-library/react";
import { MemoryRouter } from "react-router-dom";
import App from "../App";

// Mock platformStore to prevent real Tauri invoke calls during tests
vi.mock("../stores/platformStore", () => ({
  usePlatformStore: vi.fn().mockImplementation((selector?: unknown) => {
    const state = {
      agents: [],
      skillsByAgent: {},
      isLoading: false,
      error: null,
      initialize: vi.fn(),
      rescan: vi.fn(),
    };
    if (typeof selector === "function") {
      return selector(state);
    }
    return state;
  }),
}));

// Mock collectionStore to prevent real Tauri invoke calls during tests
vi.mock("../stores/collectionStore", () => ({
  useCollectionStore: Object.assign(vi.fn().mockImplementation((selector?: unknown) => {
    const state = {
      collections: [], activeCollectionId:null, groupDetails:[], selectedSkillIds:[], includeDescendants:true, selectGroup:vi.fn(), loadGroupDetails:vi.fn().mockResolvedValue(undefined), changeGroup:vi.fn().mockResolvedValue(undefined),
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
    };
    if (typeof selector === "function") {
      return selector(state);
    }
    return state;
  }), {setState:vi.fn()}),
}));

// Mock centralSkillsStore to prevent async state updates that cause act() warnings
vi.mock("../stores/centralSkillsStore", () => ({
  useCentralSkillsStore: vi.fn().mockImplementation((selector?: unknown) => {
    const state = {
      skills: [],
      agents: [],
      isLoading: false,
      isInstalling: false,
      error: null,
      loadCentralSkills: vi.fn().mockResolvedValue(undefined),
      installSkill: vi.fn(),
    };
    if (typeof selector === "function") {
      return selector(state);
    }
    return state;
  }),
}));

vi.mock("../stores/resourceLibraryStore", () => ({
  useResourceLibraryStore: vi.fn().mockImplementation((selector?: unknown) => {
    const state = {
      skills: [],
      agents: [],
      resourceLibraryDir: "~/.arsenal/library",
      isLoading: false,
      isUpdatingSources: false,
      togglingAgentId: null,
      loadResourceLibrary: vi.fn().mockResolvedValue(undefined),
      installSkill: vi.fn(),
      togglePlatformLink: vi.fn(),
      updateSourceBackedSkills: vi.fn(),
      updateSourceBackedSkill: vi.fn(),
      addToCentral: vi.fn(),
      removeFromCentral: vi.fn(),
    };
    if (typeof selector === "function") {
      return selector(state);
    }
    return state;
  }),
}));

describe("App", () => {
  it("redirects the root route to the resource library", async () => {
    await act(async () => {
      render(
        <MemoryRouter initialEntries={["/"]}>
          <App />
        </MemoryRouter>
      );
    });

    expect(screen.getByTestId("browser-status-slot")).toContainElement(screen.getByRole("contentinfo"));
    expect(screen.getAllByText("技能仓库").length).toBeGreaterThanOrEqual(1);
    expect(screen.queryByRole("heading", { name: "共享中心" })).not.toBeInTheDocument();
  });

  it("renders the app shell with sidebar branding", async () => {
    await act(async () => {
      render(
        <MemoryRouter initialEntries={["/central"]}>
          <App />
        </MemoryRouter>
      );
    });
    expect(screen.getByText("Arsenal")).toBeInTheDocument();
  });

  it("renders sidebar with icon-only navigation", async () => {
    await act(async () => {
      render(
        <MemoryRouter initialEntries={["/central"]}>
          <App />
        </MemoryRouter>
      );
    });
    // "共享中心" appears as icon button tooltip in sidebar + possibly in main content header
    expect(screen.getAllByText("共享中心").length).toBeGreaterThanOrEqual(1);
    // Icon-only sidebar has no "By Tool" section header
    expect(screen.queryByText("按工具")).not.toBeInTheDocument();
  });
});
