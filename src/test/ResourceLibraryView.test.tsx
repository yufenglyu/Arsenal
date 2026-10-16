import { openRowActions } from "./rowActions";
import { AppStatusBar } from "@/components/layout/AppStatusBar";
import { useRepositorySyncStore } from "@/stores/repositorySyncStore";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { fireEvent, render as rtlRender, screen, waitFor, within } from "@testing-library/react";
import { MemoryRouter } from "react-router-dom";
import type { AgentWithStatus, SkillWithLinks } from "@/types";

const mockLoadResourceLibrary = vi.fn();
const mockInstallSkill = vi.fn();
const mockAddToCentral = vi.fn();
const mockRemoveFromCentral = vi.fn();
const mockUninstallSkillFromAgent = vi.fn();
const mockTogglePlatformLink = vi.fn();
const mockUpdateSourceBackedSkills = vi.fn();
const mockPreviewRepositorySync = vi.fn();
const mockSyncSourceBackedSkills = vi.fn();
const mockUpdateSourceBackedSkill = vi.fn();
const mockImportGitHubRepoSnapshot = vi.fn();
const mockAddLocalSkills = vi.fn();
const mockCreateManualSkill = vi.fn();
const mockPreviewDeleteResourceBundle = vi.fn();
const mockDeleteResourceBundle = vi.fn();
const mockDeleteResourceSkill = vi.fn();
const mockRefreshCounts = vi.fn();
const mockLoadCentralSkills = vi.fn();
const mockGetSkillsByAgent = vi.fn();
const mockStartTask = vi.fn();
const mockUpdateTask = vi.fn();
const mockCompleteTask = vi.fn();
const mockFailTask = vi.fn();
const mockListen = vi.fn();
const mockUnlisten = vi.fn();

const agents: AgentWithStatus[] = [
  {
    id: "cursor",
    display_name: "Cursor",
    global_skills_dir: "~/.cursor/skills/",
    is_detected: true,
    is_builtin: true,
    is_enabled: true,
  },
  {
    id: "project:1",
    display_name: "temp",
    global_skills_dir: "~/Projects/temp/.agents/skills",
    project_skills_dir: ".agents/skills",
    is_detected: true,
    is_builtin: false,
    is_enabled: true,
  },
  {
    id: "hermes",
    display_name: "Hermes",
    global_skills_dir: "~/.agents/skills/",
    is_detected: true,
    is_builtin: true,
    is_enabled: true,
    shares_central_skills: true,
  },
  {
    id: "project:home",
    display_name: "Home",
    global_skills_dir: "~/.agents/skills/",
    project_skills_dir: ".agents/skills",
    is_detected: true,
    is_builtin: false,
    is_enabled: true,
    shares_central_skills: true,
  },
];

const defaultSkills: SkillWithLinks[] = [
  {
    id: "resource-demo",
    name: "resource-demo",
    description: "Resource demo",
    file_path: "~/.skillshub/library/example/resource-demo/SKILL.md",
    canonical_path: "~/.skillshub/library/example/resource-demo",
    is_central: false,
    scanned_at: "2026-07-14T00:00:00Z",
    created_at: "2026-07-14T00:00:00Z",
    updated_at: "2026-07-14T00:00:00Z",
    linked_agents: ["cursor"],
    read_only_agents: [],
  },
];
let resourceSkills: SkillWithLinks[] = defaultSkills;

vi.mock("sonner", () => ({
  toast: {
    success: vi.fn(),
    error: vi.fn(),
  },
}));

vi.mock("@/stores/resourceLibraryStore", () => ({
  useResourceLibraryStore: Object.assign((selector: (state: Record<string, unknown>) => unknown) =>
    selector({
      skills: resourceSkills,
      agents,
      resourceLibraryDir: "~/.skillshub/library",
      isLoading: false,
      isUpdatingSources: false,
      togglingAgentId: null,
      deletingSkillId: null,
      loadResourceLibrary: mockLoadResourceLibrary,
      installSkill: mockInstallSkill,
      addToCentral: mockAddToCentral,
      removeFromCentral: mockRemoveFromCentral,
      togglePlatformLink: mockTogglePlatformLink,
      updateSourceBackedSkills: mockUpdateSourceBackedSkills,
      previewRepositorySync: mockPreviewRepositorySync,
      syncSourceBackedSkills: mockSyncSourceBackedSkills,
      updateSourceBackedSkill: mockUpdateSourceBackedSkill,
      importGitHubRepoSnapshot: mockImportGitHubRepoSnapshot,
      exportDirectoryList: vi.fn(),
      addLocalSkills: mockAddLocalSkills,
      createManualSkill: mockCreateManualSkill,
      previewDeleteResourceBundle: mockPreviewDeleteResourceBundle,
      deleteResourceBundle: mockDeleteResourceBundle,
      deleteResourceSkill: mockDeleteResourceSkill,
    }), { getState: () => ({ previewRepositorySync: mockPreviewRepositorySync, loadResourceLibrary: mockLoadResourceLibrary, error: null }) }),
}));

vi.mock("@/stores/platformStore", () => ({
  usePlatformStore: (selector: (state: Record<string, unknown>) => unknown) =>
    selector({ agents, skillsByAgent: {}, refreshCounts: mockRefreshCounts }),
}));

vi.mock("@/stores/centralSkillsStore", () => ({
  useCentralSkillsStore: (selector: (state: Record<string, unknown>) => unknown) =>
    selector({ loadCentralSkills: mockLoadCentralSkills }),
}));

vi.mock("@/stores/skillStore", () => ({
  useSkillStore: (selector: (state: Record<string, unknown>) => unknown) =>
    selector({
      skillsByAgent: {},
      getSkillsByAgent: mockGetSkillsByAgent,
      uninstallSkillFromAgent: mockUninstallSkillFromAgent,
    }),
}));

vi.mock("@/stores/appStatusStore", () => ({
  useAppStatusStore: Object.assign((selector: (state: Record<string, unknown>) => unknown) =>
    selector({
      startTask: mockStartTask,
      updateTask: mockUpdateTask,
      completeTask: mockCompleteTask,
      failTask: mockFailTask,
    }), {getState: () => ({startTask: mockStartTask, updateTask: mockUpdateTask, completeTask: mockCompleteTask, failTask: mockFailTask})}),
}));

vi.mock("@/lib/tauri", () => ({
  isTauriRuntime: () => true,
  listen: (...args: unknown[]) => mockListen(...args),
  invoke: vi.fn(),
}));

import { toast } from "sonner";
import { invoke } from "@/lib/tauri";
import { ResourceLibraryView } from "@/pages/ResourceLibraryView";

describe("ResourceLibraryView delete", () => {
  async function switchBrowserViewMode(mode: "all" | "folders") {
    fireEvent.keyDown(await screen.findByRole("region", {name:"技能列表"}), {key:mode === "folders" ? "-" : "+"});
  }

  function tableDataRows() {
    return screen
      .getAllByRole("row")
      .filter((row) => within(row).queryAllByRole("cell").length > 0);
  }

  beforeEach(() => {
    useRepositorySyncStore.setState({ preview: null, open: false, applied: false, appliedRepositories: [], checkingRepository: null, isChecking: false, error: null, includeAdded: true, removeDeleted: false, repositories: null });
    resourceSkills = defaultSkills;
    mockLoadResourceLibrary.mockReset();
    mockInstallSkill.mockReset();
    mockAddToCentral.mockReset();
    mockTogglePlatformLink.mockReset();
    mockUpdateSourceBackedSkills.mockReset();
    mockUpdateSourceBackedSkills.mockResolvedValue({ items: [] });
    mockPreviewRepositorySync.mockReset();
    mockPreviewRepositorySync.mockImplementation((repositories?: string[]) => Promise.resolve({ repositories: [{repository:repositories?.[0] ?? "owner/repo",added:[],modified:[{skillId:"resource-demo",name:"resource-demo"}],deleted:[],unchanged:[]}] }));
    mockSyncSourceBackedSkills.mockReset();
    mockSyncSourceBackedSkills.mockResolvedValue({ items: [] });
    mockUpdateSourceBackedSkill.mockReset();
    mockImportGitHubRepoSnapshot.mockReset();
    mockAddLocalSkills.mockReset();
    mockCreateManualSkill.mockReset();
    mockPreviewDeleteResourceBundle.mockReset();
    mockPreviewDeleteResourceBundle.mockResolvedValue({
      bundle: {
        name: "example",
        relativePath: "example",
        path: "~/.skillshub/library/example",
        isSymlink: false,
        skillCount: 1,
        linkedAgentCount: 1,
        readOnlyAgentCount: 0,
      },
      skills: resourceSkills,
      affectedAgents: ["cursor"],
      skippedReadOnlyAgents: [],
    });
    mockDeleteResourceBundle.mockReset();
    mockDeleteResourceBundle.mockResolvedValue({
      relativePath: "example",
      removedBundlePath: "~/.skillshub/library/example",
      removedKind: "directory",
      removedSkillIds: ["resource-demo"],
      uninstalledAgents: ["cursor"],
      skippedReadOnlyAgents: [],
    });
    mockDeleteResourceSkill.mockReset();
    mockDeleteResourceSkill.mockResolvedValue({
      skillId: "resource-demo",
      removedCanonicalPath: "~/.skillshub/library/example/resource-demo",
      uninstalledAgents: ["cursor"],
      skippedReadOnlyAgents: [],
    });
    mockRefreshCounts.mockReset();
    mockLoadCentralSkills.mockReset();
    mockGetSkillsByAgent.mockReset();
    mockStartTask.mockReset();
    mockUpdateTask.mockReset();
    mockCompleteTask.mockReset();
    mockFailTask.mockReset();
    mockUnlisten.mockReset();
    mockListen.mockReset();
    mockListen.mockResolvedValue(mockUnlisten);
    vi.mocked(invoke).mockReset();
    vi.mocked(invoke).mockResolvedValue(undefined);
    window.localStorage.removeItem("skills-manage.skillListViewMode.resource-library");
  });

  it("opens the skill repository directory from the header path", async () => {
    render(
      <MemoryRouter>
        <ResourceLibraryView />
      </MemoryRouter>
    );

    fireEvent.click(
      screen.getByRole("button", {
        name: "在文件管理器中打开: ~/.skillshub/library",
      })
    );

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("open_in_file_manager", {
        path: "~/.skillshub/library",
      });
    });
  });

  it("toasts when opening the skill repository directory fails", async () => {
    vi.mocked(invoke).mockRejectedValue("disk missing");
    render(
      <MemoryRouter>
        <ResourceLibraryView />
      </MemoryRouter>
    );

    fireEvent.click(
      screen.getByRole("button", {
        name: "在文件管理器中打开: ~/.skillshub/library",
      })
    );

    await waitFor(() => {
      expect(toast.error).toHaveBeenCalledWith("打开路径失败: disk missing");
    });
  });

  it("opens a cascade confirmation for installed resource skills", async () => {
    render(
      <MemoryRouter>
        <ResourceLibraryView />
      </MemoryRouter>
    );

    fireEvent.click(
      openRowActions(screen.getAllByRole("row").at(-1)!).getByRole("menuitem", { name: /^删除$|^Delete$/i })
    );

    expect(screen.getByRole("dialog")).toBeInTheDocument();
    expect(screen.getByText(/Cursor/)).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: /Uninstall and delete|卸载并删除/i }));

    await waitFor(() => {
      expect(mockDeleteResourceSkill).toHaveBeenCalledWith("resource-demo", {
        cascadeUninstall: true,
      });
    });
    expect(mockRefreshCounts).toHaveBeenCalled();
  });

  it("shows the resource toolbar actions in the requested order", () => {
    render(
      <MemoryRouter>
        <ResourceLibraryView />
      </MemoryRouter>
    );

    const addButton = screen.getByRole("button", { name: /添加技能|Add skills/i });
    const updateButton = screen.getByRole("button", { name: /更新技能|Update skills/i });
    expect(addButton.compareDocumentPosition(updateButton)).toBe(Node.DOCUMENT_POSITION_FOLLOWING);
    expect(screen.queryByRole("button", { name: /导出目录列表|Export directory list/i })).not.toBeInTheDocument();
  });

  it("renders icons for update-from-source and unified import buttons", () => {
    render(
      <MemoryRouter>
        <ResourceLibraryView />
      </MemoryRouter>
    );

    const updateButton = screen.getByRole("button", { name: /更新技能|Update skills/i });
    const importButton = screen.getByRole("button", { name: /添加技能|Add skills/i });

    expect(updateButton.querySelector("svg")).not.toBeNull();
    expect(importButton.querySelector("svg")).not.toBeNull();
  });

  it("finishes the background preview after the repository page unmounts", async () => {
    let finish!: (report: { repositories: [] }) => void;
    mockPreviewRepositorySync.mockImplementation(() => new Promise((resolve) => { finish = resolve; }));
    const view = render(<MemoryRouter><ResourceLibraryView /><AppStatusBar /></MemoryRouter>);
    fireEvent.click(screen.getByRole("button", { name: /更新技能|Update skills/i }));
    expect(mockPreviewRepositorySync).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole("button", {name:"检查更新"}));
    await waitFor(()=>expect(mockPreviewRepositorySync).toHaveBeenCalledTimes(1));
    view.rerender(<MemoryRouter><AppStatusBar /></MemoryRouter>);
    expect(screen.getByRole("button", {name: /任务/})).toHaveTextContent(/运行 1/);
    finish({ repositories: [] });
    await waitFor(() => expect(screen.getByRole("button", {name: /任务/})).toHaveTextContent(/运行 0/));
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    expect(mockSyncSourceBackedSkills).not.toHaveBeenCalled();
    expect(mockUpdateSourceBackedSkills).not.toHaveBeenCalled();
    view.rerender(<MemoryRouter><ResourceLibraryView /><AppStatusBar /></MemoryRouter>);
    fireEvent.click(screen.getByRole("button", {name: /任务/}));
    await screen.findByRole("dialog", {name: "任务"});
    expect(mockPreviewRepositorySync).toHaveBeenCalledTimes(1);
  });

  it("shows per-skill source update for GitHub metadata without a stored URL", () => {
    resourceSkills = [
      {
        ...defaultSkills[0],
        source_repo: "example/skills",
        source_path: "resource-demo/SKILL.md",
        source_url: null,
      },
    ];

    render(
      <MemoryRouter>
        <ResourceLibraryView />
      </MemoryRouter>
    );

    expect(
      openRowActions(screen.getAllByRole("row").at(-1)!).getByRole("menuitem", { name: /^更新$|^Update$/i })
    ).toBeInTheDocument();
  });

  it("falls back to the github source label when source metadata is partial", () => {
    localStorage.setItem("skills-manage.skillTableColumns.tree.v5", JSON.stringify(["index","name","source","skillCount","createdAt","updatedAt","installSummary","actions"]));
    resourceSkills = [
      {
        ...defaultSkills[0],
        source: "github:example/skills",
        source_repo: null,
        source_author: null,
        source_path: "resource-demo/SKILL.md",
        source_url: null,
      },
    ];

    render(
      <MemoryRouter>
        <ResourceLibraryView />
      </MemoryRouter>
    );

    expect(screen.queryByRole("columnheader", {name:"仓库"})).not.toBeInTheDocument();
    expect(
      openRowActions(screen.getAllByRole("row").at(-1)!).getByRole("menuitem", {name: /^更新$|^Update$/i})
    ).toBeInTheDocument();
  });

  it("opens the unified add dialog from the add button", () => {
    render(
      <MemoryRouter>
        <ResourceLibraryView />
      </MemoryRouter>
    );

    fireEvent.click(screen.getByRole("button", { name: /添加技能|Add skills/i }));

    const dialog = screen.getByRole("dialog", { name: /添加技能|Add skills/i });
    expect(dialog).toBeInTheDocument();
    expect(within(dialog).getByLabelText("GitHub")).toBeInTheDocument();
    expect(within(dialog).queryByLabelText(/技能名称或来源路径|Skill name or source path/i)).not.toBeInTheDocument();
    expect(within(dialog).getByLabelText("本地")).toBeInTheDocument();
    expect(within(dialog).queryByText("手动创建")).not.toBeInTheDocument();
    expect(within(dialog).getByRole("button", {name:"添加"})).toBeInTheDocument();
    expect(within(dialog).getByRole("button", {name:"导入"})).toBeInTheDocument();
    expect(within(dialog).getByRole("button", {name:"取消"})).toBeInTheDocument();
  });



  it("sorts resource directories by modified time and direction controls", async () => {
    resourceSkills = [
      {
        ...defaultSkills[0],
        id: "old-skill",
        name: "old-skill",
        file_path: "~/.skillshub/library/zeta/repo/old-skill/SKILL.md",
        canonical_path: "~/.skillshub/library/zeta/repo/old-skill",
        created_at: "2026-07-10T00:00:00Z",
        updated_at: "2026-07-10T00:00:00Z",
      },
      {
        ...defaultSkills[0],
        id: "new-skill",
        name: "new-skill",
        file_path: "~/.skillshub/library/alpha/repo/new-skill/SKILL.md",
        canonical_path: "~/.skillshub/library/alpha/repo/new-skill",
        created_at: "2026-07-11T00:00:00Z",
        updated_at: "2026-07-12T00:00:00Z",
      },
    ];

    render(
      <MemoryRouter>
        <ResourceLibraryView />
      </MemoryRouter>
    );

    await switchBrowserViewMode("folders");
    fireEvent.click(screen.getByRole("button", { name: "更新时间" }));

    await waitFor(() => {
      const rows = tableDataRows();
      expect(rows[0]).toHaveTextContent("zeta/repo");
      expect(rows[1]).toHaveTextContent("alpha/repo");
    });

    fireEvent.click(screen.getByRole("button", { name: "更新时间，升序排序" }));

    await waitFor(() => {
      const rows = tableDataRows();
      expect(rows[0]).toHaveTextContent("alpha/repo");
      expect(rows[1]).toHaveTextContent("zeta/repo");
    });
  });



  it("filters resource directories by selected tag in folder view", async () => {
    resourceSkills = [
      {
        ...defaultSkills[0],
        id: "design-skill",
        name: "design-skill",
        tags: ["design"],
        file_path: "~/.skillshub/library/alpha/repo/design-skill/SKILL.md",
        canonical_path: "~/.skillshub/library/alpha/repo/design-skill",
      },
      {
        ...defaultSkills[0],
        id: "backend-skill",
        name: "backend-skill",
        tags: ["backend"],
        file_path: "~/.skillshub/library/zeta/repo/backend-skill/SKILL.md",
        canonical_path: "~/.skillshub/library/zeta/repo/backend-skill",
      },
    ];

    render(
      <MemoryRouter>
        <ResourceLibraryView />
      </MemoryRouter>
    );

    await switchBrowserViewMode("folders");
    fireEvent.click(screen.getByRole("button", { name: "design" }));

    await waitFor(() => {
      const rows = tableDataRows();
      expect(rows).toHaveLength(2);
      expect(rows[0]).toHaveTextContent("alpha/repo");
      expect(screen.queryByText("zeta/repo")).toBeNull();
    });
  });

  it("refreshes project directory caches after adding a resource skill to Shared Hub", async () => {
    mockAddToCentral.mockResolvedValue(undefined);

    render(
      <MemoryRouter>
        <ResourceLibraryView />
      </MemoryRouter>
    );

    fireEvent.click(
      openRowActions(screen.getAllByRole("row").at(-1)!).getByRole("menuitem", {
        name: /^加入共享中心$|^Add to Shared Hub$/i,
      })
    );

    await waitFor(() => {
      expect(mockAddToCentral).toHaveBeenCalledWith("resource-demo");
    });
    expect(mockLoadCentralSkills).toHaveBeenCalled();
    expect(mockRefreshCounts).toHaveBeenCalled();
    expect(mockGetSkillsByAgent).toHaveBeenCalledWith("cursor");
    expect(mockGetSkillsByAgent).toHaveBeenCalledWith("project:1");
  });

  it("removes the central copy while preserving the resource skill", async () => {
    resourceSkills = [
      {
        ...defaultSkills[0],
        is_central: true,
        linked_agents: ["cursor"],
      },
    ];
    mockRemoveFromCentral.mockResolvedValue(undefined);

    render(
      <MemoryRouter>
        <ResourceLibraryView />
      </MemoryRouter>
    );

    fireEvent.click(
      openRowActions(screen.getAllByRole("row").at(-1)!).getByRole("menuitem", {
        name: /^从共享中心移除$|^Remove from Shared Hub$/i,
      })
    );
    fireEvent.click(screen.getByRole("menuitem", { name: /确认删除|Confirm/i }));

    await waitFor(() => {
      expect(mockRemoveFromCentral).toHaveBeenCalledWith("resource-demo");
    });
    expect(mockLoadCentralSkills).toHaveBeenCalled();
    expect(mockGetSkillsByAgent).toHaveBeenCalledWith("cursor");
  });

  it("previews and confirms deleting a resource directory", async () => {
    render(
      <MemoryRouter>
        <ResourceLibraryView />
      </MemoryRouter>
    );

    await switchBrowserViewMode("folders");
    const folderRow = await screen.findByRole("row", { name: /example/i });
    fireEvent.click(openRowActions(folderRow).getByRole("menuitem", { name: /删除|Delete/i }));

    await waitFor(() => {
      expect(mockPreviewDeleteResourceBundle).toHaveBeenCalledWith("example");
    });
    expect(screen.getByRole("dialog")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: /删除目录并卸载|Delete directory and uninstall/i }));

    await waitFor(() => {
      expect(mockDeleteResourceBundle).toHaveBeenCalledWith("example", {
        cascadeUninstall: true,
      });
    });
  });

  it("updates source-backed skills from a resource folder row", async () => {
    resourceSkills = [
      {
        ...defaultSkills[0],
        source: "github:owner/repo",
        source_repo: "owner/repo",
        source_path: "resource-demo/SKILL.md",
        source_url: "https://raw.githubusercontent.com/owner/repo/main/resource-demo/SKILL.md",
      },
    ];
    mockSyncSourceBackedSkills.mockResolvedValue({
      items: [{ skillId: "resource-demo", name: "resource-demo", status: "updated" }],
    });

    render(
      <MemoryRouter>
        <ResourceLibraryView /><AppStatusBar />
      </MemoryRouter>
    );

    await switchBrowserViewMode("folders");
    const folderRow = await screen.findByRole("row", { name: /example/i });
    fireEvent.click(openRowActions(folderRow).getByRole("menuitem", { name: /^更新$|^Update$/i }));

    await waitFor(() => {
      expect(mockPreviewRepositorySync).toHaveBeenCalledWith(["owner/repo"]);
    });
    expect(mockSyncSourceBackedSkills).not.toHaveBeenCalled();
    vi.mocked(invoke).mockResolvedValue(undefined);
    fireEvent.click(screen.getByRole("button", {name: /任务/}));
    fireEvent.click(await screen.findByRole("checkbox", {name:"resource-demo"}));
    fireEvent.click(await screen.findByRole("button", { name: "应用更新" }));
    await waitFor(()=>expect(invoke).toHaveBeenCalledWith("apply_repository_update_item",expect.objectContaining({repository:"owner/repo",skillId:"resource-demo",action:"modified"})));

  });

  it("groups folder install targets by software platform and project directory", async () => {
    resourceSkills = defaultSkills.map((skill) => ({ ...skill, linked_agents: [] }));
    render(
      <MemoryRouter>
        <ResourceLibraryView />
      </MemoryRouter>
    );

    await switchBrowserViewMode("folders");
    const folderRow = await screen.findByRole("row", { name: /example/i });
    fireEvent.click(
      openRowActions(folderRow).getByRole("menuitem", { name: /安装|Install/i,
      })
    );

    const dialog = await screen.findByRole("dialog", {
      name: /安装目录 example|Install folder example/i,
    });

    expect(
      within(dialog).getByRole("heading", { name: /软件平台|Software platforms/i })
    ).toBeInTheDocument();
    expect(
      within(dialog).getByRole("heading", { name: /项目目录|Project directories/i })
    ).toBeInTheDocument();
    expect(within(dialog).getByLabelText("Cursor")).toBeInTheDocument();
    expect(within(dialog).getByLabelText("temp")).toBeInTheDocument();
    expect(within(dialog).getByLabelText("Hermes")).toBeEnabled();
    expect(within(dialog).getByLabelText("Home")).toBeEnabled();
    expect(within(dialog).getAllByText("将加入共享中心")).toHaveLength(2);
    fireEvent.click(within(dialog).getByLabelText("Home"));
    expect(
      within(dialog).getByText(/选中的共享平台会按共享中心规则同步/)
    ).toBeInTheDocument();
  });

  it("disables the shared folder install target when every skill is already central", async () => {
    resourceSkills = defaultSkills.map((skill) => ({
      ...skill,
      is_central: true,
      linked_agents: [],
    }));
    render(
      <MemoryRouter>
        <ResourceLibraryView />
      </MemoryRouter>
    );

    await switchBrowserViewMode("folders");
    const folderRow = await screen.findByRole("row", { name: /example/i });
    fireEvent.click(
      openRowActions(folderRow).getByRole("menuitem", { name: /安装|Install/i,
      })
    );

    const dialog = await screen.findByRole("dialog", {
      name: /安装目录 example|Install folder example/i,
    });
    expect(within(dialog).getByLabelText("Hermes")).toHaveAttribute("aria-disabled", "true");
    expect(within(dialog).getByLabelText("Home")).toHaveAttribute("aria-disabled", "true");
    expect(within(dialog).getAllByText("已通过共享中心共享")).toHaveLength(2);
  });

  it("shows only independent target counts for non-central resource skills", () => {
    localStorage.setItem("skills-manage.skillTableColumns.tree.v5", JSON.stringify(["index","name","source","skillCount","createdAt","updatedAt","installSummary","actions"]));
    render(
      <MemoryRouter>
        <ResourceLibraryView />
      </MemoryRouter>
    );

    expect(screen.getAllByText("独立安装：平台 1 · 项目 0")[0]).toBeInTheDocument();
    expect(screen.queryByText(/共享中心：平台/)).not.toBeInTheDocument();
  });
  it.each(["all", "folders"])("locates and highlights the repository row in %s view", async (view) => {
    render(<MemoryRouter initialEntries={[`/resources?locate=resource-demo&view=${view}`]}><ResourceLibraryView /></MemoryRouter>);
    await waitFor(() => {
      const row = screen.getByRole("row", { name: view === "all" ? /resource-demo/ : /example/ });
      expect(row).toHaveClass("bg-primary/10");
      expect(openRowActions(row).queryByRole("menuitem", { name: "搜索技能（GitHub / Google）" })).not.toBeInTheDocument();
    });
  });

  it.each([false, true])("keeps one global activity entry after a check (applied=%s)", (applied) => {
    useRepositorySyncStore.setState({ preview: { repositories: [] }, applied });
    render(<MemoryRouter><AppStatusBar /></MemoryRouter>);
    expect(screen.getByRole("button", {name: /任务/})).toBeInTheDocument();
    expect(screen.queryByRole("button", {name: /更新状态|Update status/i})).not.toBeInTheDocument();
  });


});

function render(...args: Parameters<typeof rtlRender>) {
  const result = rtlRender(...args);
  const list = screen.queryByRole("region", {name:"技能列表"});
  if (list) fireEvent.keyDown(list,{key:"+"});
  return result;
}
