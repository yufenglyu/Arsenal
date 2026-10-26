vi.mock("../components/notebook/NotebookSettings", () => ({ NotebookSettings: () => null }));
import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, fireEvent, waitFor, within } from "@testing-library/react";
import { MemoryRouter, Route, Routes } from "react-router-dom";
import { SettingsView, APP_VERSION } from "../pages/SettingsView";
import { ScanDirectory, AgentWithStatus, AppUpdateInfo } from "../types";
import { invoke } from "@tauri-apps/api/core";

const mockOpenDialog = vi.fn();
const mockSaveDialog = vi.fn();

// Mock stores
vi.mock("../stores/settingsStore", () => ({
  useSettingsStore: vi.fn(),
}));

vi.mock("../stores/platformStore", () => ({
  usePlatformStore: vi.fn(),
}));

vi.mock("../stores/centralSkillsStore", () => ({
  useCentralSkillsStore: vi.fn(),
}));

vi.mock("../stores/resourceLibraryStore", () => ({
  useResourceLibraryStore: vi.fn(),
}));

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));

vi.mock("@tauri-apps/plugin-dialog", () => ({
  open: (...args: unknown[]) => mockOpenDialog(...args),
  save: (...args: unknown[]) => mockSaveDialog(...args),
}));

vi.mock("sonner", () => ({
  toast: {
    success: vi.fn(),
    error: vi.fn(),
  },
}));

import { useSettingsStore, type BackupImportProgress } from "../stores/settingsStore";
import { usePlatformStore } from "../stores/platformStore";
import { useCentralSkillsStore } from "../stores/centralSkillsStore";
import { useResourceLibraryStore } from "../stores/resourceLibraryStore";
import { toast } from "sonner";

// ─── Fixtures ─────────────────────────────────────────────────────────────────

const mockBuiltinDir: ScanDirectory = {
  id: 1,
  path: "/Users/test/.agents/skills/",
  label: "Shared Hub",
  is_active: true,
  is_builtin: true,
  added_at: "2026-01-01T00:00:00Z",
};

const mockCustomDir: ScanDirectory = {
  id: 2,
  path: "/Users/test/projects/my-project",
  label: "My Project",
  is_active: true,
  is_builtin: false,
  added_at: "2026-01-02T00:00:00Z",
};

const mockCustomAgent: AgentWithStatus = {
  id: "custom-qclaw",
  display_name: "QClaw",
  global_skills_dir: "/Users/test/.qclaw/skills/",
  is_detected: false,
  is_builtin: false,
  is_enabled: true,
};

const mockBuiltinAgent: AgentWithStatus = {
  id: "claude-code",
  display_name: "Claude Code",
  global_skills_dir: "/Users/test/.claude/skills/",
  is_detected: true,
  is_builtin: true,
  is_enabled: true,
};

const mockMissingBuiltinAgent: AgentWithStatus = {
  id: "cursor",
  display_name: "Cursor",
  global_skills_dir: "/Users/test/.cursor/skills/",
  is_detected: false,
  is_builtin: true,
  is_enabled: true,
};

// ─── Helpers ──────────────────────────────────────────────────────────────────

function setupMocks({
  scanDirs = [] as ScanDirectory[],
  isLoadingScanDirs = false,
  agents = [] as AgentWithStatus[],
  loadScanDirectories = vi.fn(),
  addScanDirectory = vi.fn(),
  updateScanDirectory = vi.fn(),
  removeScanDirectory = vi.fn(),
  toggleScanDirectory = vi.fn(),
  addCustomAgent = vi.fn(),
  updateCustomAgent = vi.fn(),
  removeCustomAgent = vi.fn(),
  toggleAgentEnabled = vi.fn(),
  openPlatformDir = vi.fn(),
  githubPat = "",
  isLoadingGitHubPat = false,
  isSavingGitHubPat = false,
  resourceLibraryDir = "~/.arsenal/library",
  isLoadingResourceLibraryDir = false,
  loadGitHubPat = vi.fn(),
  saveGitHubPat = vi.fn(),
  clearGitHubPat = vi.fn(),
  updateCentralSkillsDir = vi.fn(),
  loadResourceLibraryDir = vi.fn(),
  updateResourceLibraryDir = vi.fn(),
  configDir = "",
  isLoadingConfigDir = false,
  loadConfigDir = vi.fn(),
  updateConfigDir = vi.fn(),
  exportAppBackup = vi.fn(),
  backupImportProgress = null as BackupImportProgress | null,
  importAppBackup = vi.fn(),
  listWebDavBackups = vi.fn(),
  testWebDavConnection = vi.fn(),
  uploadWebDavBackup = vi.fn(),
  restoreWebDavBackup = vi.fn(),
  deleteWebDavBackup = vi.fn(),
  webDavConfig = {
    baseUrl: "",
    username: "",
    password: "",
    remoteDir: "arsenal",
  },
  isLoadingWebDavConfig = false,
  isSavingWebDavConfig = false,
  loadWebDavConfig = vi.fn(),
  saveWebDavConfig = vi.fn(),
  updateInfo = null as AppUpdateInfo | null,
  isCheckingUpdate = false,
  checkAppUpdate = vi.fn(),
  loadCentralSkills = vi.fn(),
  rescan = vi.fn(),
  refreshCounts = vi.fn(),
} = {}) {
  vi.mocked(useSettingsStore).mockImplementation((selector) =>
    selector({
      loadLanguage: vi.fn(),
      saveLanguage: vi.fn(),
      scanDirectories: scanDirs,
      isLoadingScanDirs,
      error: null,
      loadScanDirectories,
      addScanDirectory,
      updateScanDirectory,
      removeScanDirectory,
      toggleScanDirectory,
      addCustomAgent,
      updateCustomAgent,
      removeCustomAgent,
      toggleAgentEnabled,
      openPlatformDir,
      githubPat,
      isLoadingGitHubPat,
      isSavingGitHubPat,
      resourceLibraryDir,
      isLoadingResourceLibraryDir,
      loadGitHubPat,
      saveGitHubPat,
      clearGitHubPat,
      updateCentralSkillsDir,
      loadResourceLibraryDir,
      updateResourceLibraryDir,
      configDir,
      isLoadingConfigDir,
      loadConfigDir,
      updateConfigDir,
      exportAppBackup,
      backupImportProgress,
      importAppBackup,
      listWebDavBackups,
      testWebDavConnection,
      uploadWebDavBackup,
      restoreWebDavBackup,
      deleteWebDavBackup,
      webDavConfig,
      isLoadingWebDavConfig,
      isSavingWebDavConfig,
      loadWebDavConfig,
      saveWebDavConfig,
      updateInfo,
      isCheckingUpdate,
      checkAppUpdate,
      clearError: vi.fn(),
    })
  );

  vi.mocked(usePlatformStore).mockImplementation((selector) =>
    selector({
      agents,
      skillsByAgent: {},
      isLoading: false,
      isRefreshing: false,
      error: null,
      initialize: vi.fn(),
      rescan,
      refreshCounts,
    })
  );
  Object.assign(usePlatformStore, {
    getState: () => ({ error: null, isRefreshing: false }),
  });

  vi.mocked(useCentralSkillsStore).mockImplementation((selector) =>
    selector({
      loadCentralSkills,
    } as never)
  );

  vi.mocked(useResourceLibraryStore).mockImplementation((selector) =>
    selector({
      loadResourceLibrary: vi.fn(),
    } as never)
  );
}

function renderSettingsView(expandSections = true) {
  const result = render(
    <MemoryRouter initialEntries={["/settings"]}>
      <Routes>
        <Route path="/settings" element={<SettingsView />} />
      </Routes>
    </MemoryRouter>
  );
  for (const title of expandSections ? ["路径配置", "访问令牌", "备份恢复", "AI 服务"] : []) {
    const summary = screen.queryByText(title)?.closest("summary");
    if (summary) fireEvent.click(summary);
  }
  return result;
}


function expandSoftwarePlatforms() {
  fireEvent.click(screen.getByRole("button", { name: "展开软件平台列表" }));
}

// ─── Tests ────────────────────────────────────────────────────────────────────

describe("SettingsView", () => {
  it("preserves provider drafts and persists profiles only when saved", async () => {
    setupMocks();
    renderSettingsView();
    const save = within(screen.getByRole("region",{name:"连接配置"})).getByRole("button",{name:"保存配置"});
    await waitFor(() => expect(save).toBeEnabled());
    const model = screen.getByPlaceholderText(/模型名称|Model name/);
    const key = screen.getByPlaceholderText("sk-...");
    fireEvent.change(model,{target:{value:"fixture-custom-model"}});
    fireEvent.change(key,{target:{value:"fixture-key"}});
    fireEvent.click(screen.getByRole("button",{name:"DeepSeek"}));
    expect(key).toHaveValue("");
    fireEvent.change(model,{target:{value:"fixture-deepseek-model"}});
    fireEvent.click(screen.getByRole("button",{name:"Claude"}));
    expect(model).toHaveValue("fixture-custom-model");
    expect(key).toHaveValue("fixture-key");
    expect(vi.mocked(invoke).mock.calls.some(([command]) => command === "set_setting")).toBe(false);
    fireEvent.click(save);
    await waitFor(() => expect(invoke).toHaveBeenCalledWith("set_setting",{key:"ai_model",value:"fixture-custom-model"}));
    expect(invoke).toHaveBeenCalledWith("set_setting",{key:"ai_connection_profiles",value:expect.stringContaining("fixture-deepseek-model")});
  });
  beforeEach(() => {
    vi.clearAllMocks();
    mockOpenDialog.mockReset();
    mockSaveDialog.mockReset();
    mockSaveDialog.mockResolvedValue("D:\\backups\\arsenal-backup.zip");
    vi.mocked(invoke).mockResolvedValue(null);
  });

  // ── Rendering ─────────────────────────────────────────────────────────────

  it("renders the settings header", () => {
    setupMocks();
    renderSettingsView();
    expect(screen.getByRole("heading", { name: "设置" })).toBeTruthy();
  });

  it("renders the github token section", () => {
    setupMocks();
    renderSettingsView();
    expect(screen.getByText("访问令牌")).toBeTruthy();
  });

  it("renders the existing settings sections", () => {
    setupMocks();
    renderSettingsView();
    expect(screen.getByRole("heading", { name: "软件平台" })).toBeTruthy();
    expect(screen.queryByText("项目目录")).toBeNull();
    expect(screen.getAllByText("软件平台").length).toBeGreaterThan(0);
    expect(screen.getByRole("heading", { name: "路径配置" })).toBeTruthy();
    expect(screen.queryByRole("heading", { name: "扫描目录" })).toBeNull();
    expect(screen.queryByRole("heading", { name: "自定义平台" })).toBeNull();
    expect(screen.getByText("关于")).toBeTruthy();
  });

  it("groups directory settings in path config then software platform order", () => {
    setupMocks();
    renderSettingsView();

    const pathsConfig = screen.getByRole("heading", { name: "路径配置" });
    const config = screen.getByLabelText("配置文件路径");
    const resource = screen.getByLabelText("技能仓库目录");
    const central = screen.getByLabelText("共享中心目录");
    const skillLocation = screen.getByRole("heading", { name: "软件平台" });

    expect(pathsConfig.compareDocumentPosition(config) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    expect(config.compareDocumentPosition(resource) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    expect(resource.compareDocumentPosition(central) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    expect(central.compareDocumentPosition(skillLocation) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
  });

  it("calls loadScanDirectories on mount", () => {
    const loadScanDirectories = vi.fn();
    setupMocks({ loadScanDirectories });
    renderSettingsView();
    expect(loadScanDirectories).toHaveBeenCalled();
  });

  it("calls loadGitHubPat on mount", () => {
    const loadGitHubPat = vi.fn();
    setupMocks({ loadGitHubPat });
    renderSettingsView();
    expect(loadGitHubPat).toHaveBeenCalled();
  });

  it("calls loadWebDavConfig on mount", () => {
    const loadWebDavConfig = vi.fn();
    setupMocks({ loadWebDavConfig });
    renderSettingsView();
    expect(loadWebDavConfig).toHaveBeenCalled();
  });

  it("does not render backup content selectors because backups are always complete", () => {
    setupMocks();
    renderSettingsView();

    expect(screen.queryByText("备份内容")).toBeNull();
    expect(screen.queryByRole("checkbox", { name: "技能仓库" })).toBeNull();
    expect(screen.queryByRole("checkbox", { name: "共享中心" })).toBeNull();
    expect(screen.queryByRole("checkbox", { name: "软件配置" })).toBeNull();
    expect(screen.queryByRole("checkbox", { name: "技能安装的平台" })).toBeNull();
  });

  it("uses the localized WebDAV URL placeholder", () => {
    setupMocks();
    renderSettingsView();

    expect(screen.getByPlaceholderText("例如 https://dav.example.com/backups")).toBeTruthy();
  });

  it("renders persisted WebDAV connection settings", () => {
    setupMocks({
      webDavConfig: {
        baseUrl: "https://example.com/dav",
        username: "saved-user",
        password: "saved-secret",
        remoteDir: "saved-dir",
      },
    });
    renderSettingsView();

    expect(screen.getByLabelText("WebDAV URL")).toHaveValue("https://example.com/dav");
    expect(screen.getByLabelText("用户名")).toHaveValue("saved-user");
    expect(screen.getByLabelText("密码或 Token")).toHaveValue("saved-secret");
    expect(screen.getByLabelText("远端目录")).toHaveValue("saved-dir");
  });

  it("saves WebDAV connection settings from the form", async () => {
    const saveWebDavConfig = vi.fn().mockResolvedValue(undefined);
    setupMocks({ saveWebDavConfig });
    renderSettingsView();

    fireEvent.change(screen.getByLabelText("WebDAV URL"), {
      target: { value: "https://example.com/dav" },
    });
    fireEvent.change(screen.getByLabelText("用户名"), {
      target: { value: "saved-user" },
    });
    fireEvent.change(screen.getByLabelText("密码或 Token"), {
      target: { value: "saved-secret" },
    });
    fireEvent.change(screen.getByLabelText("远端目录"), {
      target: { value: "saved-dir" },
    });
    fireEvent.click(screen.getAllByRole("button", { name: "保存配置" })[0]);

    await waitFor(() => {
      expect(saveWebDavConfig).toHaveBeenCalledWith({
        baseUrl: "https://example.com/dav",
        username: "saved-user",
        password: "saved-secret",
        remoteDir: "saved-dir",
      });
    });
  });

  it("local export always includes all backup content", async () => {
    const exportAppBackup = vi.fn().mockResolvedValue(undefined);
    setupMocks({ exportAppBackup });
    renderSettingsView();

    fireEvent.click(screen.getByRole("button", { name: "导出备份" }));

    await waitFor(() => {
      expect(mockSaveDialog).toHaveBeenCalled();
      expect(exportAppBackup).toHaveBeenCalledWith("D:\\backups\\arsenal-backup.zip", {
        includeResourceLibrary: true,
        includeCentralLibrary: false,
        includeAppConfig: true,
        includeInstallations: true,
      });
    });
  });

  it("does not export when the save dialog is cancelled", async () => {
    const exportAppBackup = vi.fn();
    mockSaveDialog.mockResolvedValueOnce(null);
    setupMocks({ exportAppBackup });
    renderSettingsView();

    fireEvent.click(screen.getByRole("button", { name: "导出备份" }));

    await waitFor(() => {
      expect(mockSaveDialog).toHaveBeenCalled();
    });
    expect(exportAppBackup).not.toHaveBeenCalled();
  });

  it("refreshes and renders WebDAV backup files", async () => {
    const listWebDavBackups = vi.fn().mockResolvedValue([
      {
        name: "arsenal-backup-2026-07-15-120000.zip",
        remotePath: "arsenal-backup-2026-07-15-120000.zip",
        size: 42,
        modifiedAt: "2026-07-15T12:00:00Z",
      },
    ]);
    setupMocks({ listWebDavBackups });
    renderSettingsView();

    fireEvent.change(screen.getByLabelText("WebDAV URL"), {
      target: { value: "https://example.com/dav" },
    });
    fireEvent.change(screen.getByLabelText("远端目录"), {
      target: { value: "arsenal" },
    });
    fireEvent.click(screen.getByRole("button", { name: "查看远端" }));

    expect(await screen.findByText("arsenal-backup-2026-07-15-120000.zip")).toBeTruthy();
  });

  it("shows WebDAV backup times in the local timezone", async () => {
    const listWebDavBackups = vi.fn().mockResolvedValue([
      {
        name: "arsenal-backup-2026-08-19-135638.zip",
        remotePath: "arsenal-backup-2026-08-19-135638.zip",
        size: 42,
        modifiedAt: "Wed, 19 Aug 2026 13:56:38 GMT",
      },
    ]);
    setupMocks({ listWebDavBackups });
    renderSettingsView();

    fireEvent.change(screen.getByLabelText("WebDAV URL"), {
      target: { value: "https://example.com/dav" },
    });
    fireEvent.change(screen.getByLabelText("远端目录"), {
      target: { value: "arsenal" },
    });
    fireEvent.click(screen.getByRole("button", { name: "查看远端" }));

    const gmt = "Wed, 19 Aug 2026 13:56:38 GMT";
    expect(await screen.findByText("arsenal-backup-2026-08-19-135638.zip")).toBeTruthy();
    expect(screen.getByText(new Date(gmt).toLocaleString(undefined, {
      year: "numeric",
      month: "2-digit",
      day: "2-digit",
      hour: "2-digit",
      minute: "2-digit",
      second: "2-digit",
      hour12: false,
    }))).toBeTruthy();
    expect(screen.queryByText(/GMT/)).toBeNull();
  });

  it("clears stale WebDAV selections when the connection config changes", async () => {
    const listWebDavBackups = vi.fn().mockResolvedValue([
      {
        name: "arsenal-backup-2026-07-15-120000.zip",
        remotePath: "arsenal-backup-2026-07-15-120000.zip",
      },
    ]);
    setupMocks({ listWebDavBackups });
    renderSettingsView();

    fireEvent.change(screen.getByLabelText("WebDAV URL"), {
      target: { value: "https://example.com/dav" },
    });
    fireEvent.change(screen.getByLabelText("远端目录"), {
      target: { value: "arsenal" },
    });
    fireEvent.click(screen.getByRole("button", { name: "查看远端" }));

    expect(await screen.findByText("arsenal-backup-2026-07-15-120000.zip")).toBeTruthy();
    expect(screen.getByRole("button", { name: "导入选中" })).toBeEnabled();

    fireEvent.change(screen.getByLabelText("WebDAV URL"), {
      target: { value: "https://other.example.com/dav" },
    });

    await waitFor(() => {
      expect(screen.queryByText("arsenal-backup-2026-07-15-120000.zip")).toBeNull();
      expect(screen.getByRole("button", { name: "导入选中" })).toBeDisabled();
    });
  });

  it("disables all backup actions while a local export is running", async () => {
    let resolveExport: () => void = () => undefined;
    const exportAppBackup = vi.fn(
      () =>
        new Promise<void>((resolve) => {
          resolveExport = resolve;
        })
    );
    setupMocks({ exportAppBackup });
    renderSettingsView();

    fireEvent.click(screen.getByRole("button", { name: "导出备份" }));

    await waitFor(() => {
      expect(screen.getByRole("button", { name: "导出备份" })).toBeDisabled();
      expect(screen.getByRole("button", { name: "导入备份" })).toBeDisabled();
      expect(screen.getByRole("button", { name: "测试连接" })).toBeDisabled();
      expect(screen.getByRole("button", { name: "查看远端" })).toBeDisabled();
      expect(screen.getByRole("button", { name: "上传备份" })).toBeDisabled();
      expect(screen.getByRole("button", { name: "删除选中" })).toBeDisabled();
    });

    resolveExport();

    await waitFor(() => {
      expect(screen.getByRole("button", { name: "导出备份" })).toBeEnabled();
      expect(screen.getByRole("button", { name: "查看远端" })).toBeEnabled();
    });
  });

  it("tests the WebDAV connection with the current form config", async () => {
    const testWebDavConnection = vi.fn().mockResolvedValue(undefined);
    setupMocks({ testWebDavConnection });
    renderSettingsView();

    fireEvent.change(screen.getByLabelText("WebDAV URL"), {
      target: { value: "https://example.com/dav" },
    });
    fireEvent.change(screen.getByLabelText("远端目录"), {
      target: { value: "arsenal" },
    });
    fireEvent.click(screen.getByRole("button", { name: "测试连接" }));

    await waitFor(() => {
      expect(testWebDavConnection).toHaveBeenCalledWith(
        expect.objectContaining({
          baseUrl: "https://example.com/dav",
          remoteDir: "arsenal",
        })
      );
      expect(toast.success).toHaveBeenCalledWith("WebDAV 连接测试成功");
    });
  });

  it("uploads a WebDAV backup then refreshes the remote list", async () => {
    const listWebDavBackups = vi.fn().mockResolvedValue([]);
    const uploadWebDavBackup = vi.fn().mockResolvedValue({
      name: "arsenal-backup.zip",
      remotePath: "arsenal-backup.zip",
    });
    setupMocks({ listWebDavBackups, uploadWebDavBackup });
    renderSettingsView();

    fireEvent.change(screen.getByLabelText("WebDAV URL"), {
      target: { value: "https://example.com/dav" },
    });
    fireEvent.change(screen.getByLabelText("远端目录"), {
      target: { value: "arsenal" },
    });
    fireEvent.click(screen.getByRole("button", { name: "上传备份" }));

    await waitFor(() => {
      expect(uploadWebDavBackup).toHaveBeenCalledWith(
        expect.objectContaining({
          baseUrl: "https://example.com/dav",
          remoteDir: "arsenal",
        }),
        {
          includeResourceLibrary: true,
          includeCentralLibrary: false,
          includeAppConfig: true,
          includeInstallations: true,
        }
      );
    });
    expect(listWebDavBackups).toHaveBeenCalled();
  });

  it("shows a distinct localized error when refresh fails after upload", async () => {
    const listWebDavBackups = vi.fn().mockRejectedValue("WebDAV list failed: connection failed");
    const uploadWebDavBackup = vi.fn().mockResolvedValue({
      name: "arsenal-backup.zip",
      remotePath: "arsenal-backup.zip",
    });
    setupMocks({ listWebDavBackups, uploadWebDavBackup });
    renderSettingsView();

    fireEvent.change(screen.getByLabelText("WebDAV URL"), {
      target: { value: "https://example.com/dav" },
    });
    fireEvent.change(screen.getByLabelText("远端目录"), {
      target: { value: "arsenal" },
    });
    fireEvent.click(screen.getByRole("button", { name: "上传备份" }));

    await waitFor(() => {
      expect(toast.success).toHaveBeenCalledWith("备份已上传到 WebDAV");
      expect(toast.error).toHaveBeenCalledWith("上传成功，但刷新远端备份列表失败: 网络连接失败");
    });
    expect(toast.error).not.toHaveBeenCalledWith(expect.stringContaining("上传 WebDAV 备份失败"));
  });

  it("localizes WebDAV backend errors without exposing raw details", async () => {
    const rawError = { message: "WebDAV list failed: internal transport detail 12345" };
    const listWebDavBackups = vi.fn().mockRejectedValue(rawError);
    setupMocks({ listWebDavBackups });
    renderSettingsView();

    fireEvent.change(screen.getByLabelText("WebDAV URL"), {
      target: { value: "https://example.com/dav" },
    });
    fireEvent.change(screen.getByLabelText("远端目录"), {
      target: { value: "arsenal" },
    });
    fireEvent.click(screen.getByRole("button", { name: "查看远端" }));

    await waitFor(() => {
      expect(toast.error).toHaveBeenCalledWith("刷新远端备份失败: 远端服务请求失败");
    });
    expect(toast.error).not.toHaveBeenCalledWith(expect.stringContaining(rawError.message));
    expect(toast.error).not.toHaveBeenCalledWith(expect.stringContaining("12345"));
  });

  it("deletes the selected WebDAV backup then refreshes the remote list", async () => {
    const deleteWebDavBackup = vi.fn().mockResolvedValue(undefined);
    const listWebDavBackups = vi
      .fn()
      .mockResolvedValueOnce([
        {
          name: "arsenal-backup.zip",
          remotePath: "arsenal-backup.zip",
        },
      ])
      .mockResolvedValueOnce([]);
    setupMocks({ deleteWebDavBackup, listWebDavBackups });
    renderSettingsView();

    fireEvent.change(screen.getByLabelText("WebDAV URL"), {
      target: { value: "https://example.com/dav" },
    });
    fireEvent.change(screen.getByLabelText("远端目录"), {
      target: { value: "arsenal" },
    });
    fireEvent.click(screen.getByRole("button", { name: "查看远端" }));
    await screen.findByText("arsenal-backup.zip");
    fireEvent.click(screen.getByRole("radio", { name: /arsenal-backup\.zip/ }));
    fireEvent.click(screen.getByRole("button", { name: "删除选中" }));

    await waitFor(() => {
      expect(deleteWebDavBackup).toHaveBeenCalledWith(
        expect.objectContaining({ baseUrl: "https://example.com/dav", remoteDir: "arsenal" }),
        "arsenal-backup.zip"
      );
      expect(toast.success).toHaveBeenCalledWith("远端备份已删除");
    });
    expect(listWebDavBackups).toHaveBeenCalledTimes(2);
  });

  it("imports a local backup through the native path picker", async () => {
    const importAppBackup = vi.fn().mockResolvedValue(undefined);
    setupMocks({ importAppBackup });
    mockOpenDialog.mockResolvedValueOnce("/backups/large.zip");
    renderSettingsView();
    fireEvent.click(screen.getByRole("button", {name: "导入备份"}));
    await waitFor(() => expect(importAppBackup).toHaveBeenCalledWith("/backups/large.zip"));
    expect(screen.queryByLabelText("导入备份", {selector: "input"})).not.toBeInTheDocument();
  });

  it("does not start a restore when the native picker is cancelled", async () => {
    const importAppBackup = vi.fn();
    setupMocks({ importAppBackup });
    mockOpenDialog.mockResolvedValueOnce(null);
    renderSettingsView();
    fireEvent.click(screen.getByRole("button", {name: "导入备份"}));
    await waitFor(() => expect(screen.getByRole("button", {name: "导入备份"})).not.toBeDisabled());
    expect(importAppBackup).not.toHaveBeenCalled();
  });

  it("shows backend validation progress while keeping restore actions disabled", async () => {
    setupMocks({
      importAppBackup: vi.fn(() => new Promise<void>(() => {})),
      backupImportProgress: {phase: "validating", completedBytes: 0, totalBytes: null},
    });
    mockOpenDialog.mockResolvedValueOnce("/backups/large.zip");
    renderSettingsView();
    fireEvent.click(screen.getByRole("button", {name: "导入备份"}));
    expect(await screen.findByRole("status")).toHaveTextContent("正在校验备份");
    expect(screen.getByRole("button", {name: "导入备份"})).toBeDisabled();
    expect(screen.getByRole("button", {name: "导出备份"})).toBeDisabled();
  });

  it("imports the selected WebDAV backup", async () => {
    const restoreWebDavBackup = vi.fn().mockResolvedValue(undefined);
    const importAppBackup = vi.fn().mockResolvedValue(undefined);
    setupMocks({
      restoreWebDavBackup,
      importAppBackup,
      listWebDavBackups: vi.fn().mockResolvedValue([
        {
          name: "arsenal-backup.zip",
          remotePath: "arsenal-backup.zip",
        },
      ]),
    });
    renderSettingsView();

    fireEvent.change(screen.getByLabelText("WebDAV URL"), {
      target: { value: "https://example.com/dav" },
    });
    fireEvent.change(screen.getByLabelText("远端目录"), {
      target: { value: "arsenal" },
    });
    fireEvent.click(screen.getByRole("button", { name: "查看远端" }));
    await screen.findByText("arsenal-backup.zip");
    fireEvent.click(screen.getByRole("radio", { name: /arsenal-backup\.zip/ }));
    fireEvent.click(screen.getByRole("button", { name: "导入选中" }));

    await waitFor(() => {
      expect(restoreWebDavBackup).toHaveBeenCalledWith(
        expect.objectContaining({ baseUrl: "https://example.com/dav", remoteDir: "arsenal" }),
        "arsenal-backup.zip"
      );
    });
    expect(importAppBackup).not.toHaveBeenCalled();
  });

  it("renders the saved github pat value and explanation copy", () => {
    setupMocks({ githubPat: "github_pat_saved" });
    renderSettingsView();

    expect(screen.getByLabelText("GitHub Personal Access Token")).toHaveValue("github_pat_saved");
    const githubHint = screen.getByLabelText(/它绝不会被发送到公共镜像或代理回退链路/);
    expect(githubHint).toHaveAttribute("title", expect.stringContaining("当 GitHub 预览/导入遇到限流"));
  });

  it("saves the github pat from settings", async () => {
    const saveGitHubPat = vi.fn().mockResolvedValue(undefined);
    setupMocks({ githubPat: "", saveGitHubPat });
    renderSettingsView();

    fireEvent.change(screen.getByLabelText("GitHub Personal Access Token"), {
      target: { value: "  github_pat_new  " },
    });
    fireEvent.click(screen.getAllByRole("button", { name: "保存" })[0]);

    await waitFor(() => {
      expect(saveGitHubPat).toHaveBeenCalledWith("  github_pat_new  ");
    });
    expect(await screen.findByText("GitHub 令牌已保存")).toBeTruthy();
  });

  it("clears the github pat from settings", async () => {
    const clearGitHubPat = vi.fn().mockResolvedValue(undefined);
    setupMocks({ githubPat: "github_pat_saved", clearGitHubPat });
    renderSettingsView();

    fireEvent.click(screen.getByRole("button", { name: "清除" }));

    await waitFor(() => {
      expect(clearGitHubPat).toHaveBeenCalled();
    });
    expect(await screen.findByText("GitHub 令牌已清除")).toBeTruthy();
  });

  it("moves project directory management out of settings", () => {
    setupMocks({ scanDirs: [mockCustomDir] });
    renderSettingsView();
    expect(screen.queryByTestId("settings-project-directories-header")).not.toBeInTheDocument();
    expect(screen.queryByRole("button", {name:"添加项目目录"})).not.toBeInTheDocument();
  });

  // ── Custom Platforms section ──────────────────────────────────────────────

  it("shows empty state when no software platforms", () => {
    setupMocks({ agents: [] });
    renderSettingsView();
    expandSoftwarePlatforms();
    expect(screen.getByText("暂无软件平台。点击「添加平台」注册平台。")).toBeTruthy();
  });

  it("keeps the software platform list collapsed by default", () => {
    setupMocks({ agents: [mockBuiltinAgent] });
    renderSettingsView();

    expect(screen.queryByText("Claude Code")).toBeNull();
    expect(screen.getByRole("button", { name: "展开软件平台列表" })).toHaveAttribute(
      "aria-expanded",
      "false"
    );
  });

  it("renders builtin platform with edit and remove actions", () => {
    setupMocks({ agents: [mockBuiltinAgent] });
    renderSettingsView();
    expandSoftwarePlatforms();

    expect(screen.getByText("Claude Code")).toBeTruthy();
    expect(screen.getByRole("button", { name: `编辑平台 ${mockBuiltinAgent.display_name}` })).toBeTruthy();
    expect(screen.getByRole("button", { name: `删除平台 ${mockBuiltinAgent.display_name}` })).toBeTruthy();
  });

  it("distinguishes builtin platforms with existing skills directories from missing ones", () => {
    setupMocks({ agents: [mockBuiltinAgent, mockMissingBuiltinAgent] });
    renderSettingsView();
    expandSoftwarePlatforms();

    expect(screen.getByText("Claude Code")).toBeTruthy();
    expect(screen.getByText("Cursor", {selector:"div"})).toBeTruthy();
    expect(screen.getByText("已检测到")).toBeTruthy();
    expect(screen.getByText("未检测到")).toBeTruthy();
  });

  it("lists all software platforms in one group without lobster/coding split", () => {
    setupMocks({ agents: [mockBuiltinAgent, mockCustomAgent] });
    renderSettingsView();
    expandSoftwarePlatforms();

    expect(screen.queryByText("龙虾类")).toBeNull();
    expect(screen.queryByText("编程类")).toBeNull();
    expect(screen.getByText("Claude Code")).toBeTruthy();
    expect(screen.getByText("QClaw")).toBeTruthy();
  });

  it("shows detected and enabled platform counts in section headers", () => {
    setupMocks({ agents: [mockBuiltinAgent, mockMissingBuiltinAgent, mockCustomAgent] });
    renderSettingsView();
    expandSoftwarePlatforms();

    expect(screen.queryByText("内置 2")).toBeNull();
    expect(screen.getByText("已检测到 1")).toBeTruthy();
    expect(screen.getByText("已启用 3")).toBeTruthy();
  });

  it("shows enable switches for software platforms", () => {
    setupMocks({ agents: [mockBuiltinAgent] });
    renderSettingsView();
    expandSoftwarePlatforms();

    expect(
      screen.getByRole("switch", { name: `启用 ${mockBuiltinAgent.display_name}` })
    ).toBeTruthy();
  });

  it("renders custom platform with name and path", () => {
    setupMocks({ agents: [mockBuiltinAgent, mockCustomAgent] });
    renderSettingsView();
    expandSoftwarePlatforms();
    expect(screen.getByText("QClaw")).toBeTruthy();
    expect(screen.getByText("/Users/test/.qclaw/skills/")).toBeTruthy();
  });

  it("shows edit button for custom platforms", () => {
    setupMocks({ agents: [mockBuiltinAgent, mockCustomAgent] });
    renderSettingsView();
    expandSoftwarePlatforms();
    expect(
      screen.getByRole("button", { name: `编辑平台 ${mockCustomAgent.display_name}` })
    ).toBeTruthy();
  });

  it("shows remove button for custom platforms", () => {
    setupMocks({ agents: [mockBuiltinAgent, mockCustomAgent] });
    renderSettingsView();
    expandSoftwarePlatforms();
    expect(
      screen.getByRole("button", { name: `删除平台 ${mockCustomAgent.display_name}` })
    ).toBeTruthy();
  });

  it("shows add platform button", () => {
    setupMocks();
    renderSettingsView();
    expandSoftwarePlatforms();
    expect(screen.getByRole("button", { name: "添加平台" })).toBeTruthy();
  });

  it("opens add platform dialog when button is clicked", async () => {
    setupMocks();
    renderSettingsView();
    expandSoftwarePlatforms();
    fireEvent.click(screen.getByRole("button", { name: "添加平台" }));
    await waitFor(() => {
      expect(screen.getByRole("heading", { name: "添加平台" })).toBeTruthy();
    });
  });

  it("lets users specify the platform ID when adding a platform", async () => {
    const addCustomAgent = vi.fn().mockResolvedValue(mockCustomAgent);
    const rescan = vi.fn().mockResolvedValue(undefined);
    setupMocks({ addCustomAgent, rescan });
    renderSettingsView();
    expandSoftwarePlatforms();

    fireEvent.click(screen.getByRole("button", { name: "添加平台" }));
    fireEvent.change(await screen.findByLabelText("平台名称 *"), {
      target: { value: "My Platform" },
    });
    expect(screen.getByLabelText("平台 ID")).toHaveValue("my-platform");

    fireEvent.change(screen.getByLabelText("平台 ID"), {
      target: { value: "custom-platform-id" },
    });
    fireEvent.change(screen.getByLabelText("技能目录路径 *"), {
      target: { value: "/Users/test/.custom-platform/skills" },
    });
    fireEvent.click(screen.getByRole("button", { name: "添加" }));

    await waitFor(() => {
      expect(addCustomAgent).toHaveBeenCalledWith({
        id: "custom-platform-id",
        display_name: "My Platform",
        global_skills_dir: "/Users/test/.custom-platform/skills",
      });
    });
  });

  it("opens edit platform dialog when edit button is clicked", async () => {
    setupMocks({ agents: [mockBuiltinAgent, mockCustomAgent] });
    renderSettingsView();
    expandSoftwarePlatforms();
    fireEvent.click(
      screen.getByRole("button", { name: `编辑平台 ${mockCustomAgent.display_name}` })
    );
    await waitFor(() => {
      expect(screen.getByText("编辑平台")).toBeTruthy();
    });
  });

  it("lets users edit a platform ID", async () => {
    const updateCustomAgent = vi.fn().mockResolvedValue({
      ...mockCustomAgent,
      id: "renamed-platform",
    });
    const rescan = vi.fn().mockResolvedValue(undefined);
    setupMocks({
      agents: [mockBuiltinAgent, mockCustomAgent],
      updateCustomAgent,
      rescan,
    });
    renderSettingsView();
    expandSoftwarePlatforms();

    fireEvent.click(
      screen.getByRole("button", { name: `编辑平台 ${mockCustomAgent.display_name}` })
    );
    await waitFor(() => {
      expect(screen.getByText("编辑平台")).toBeTruthy();
    });
    const idInput = screen.getByLabelText(/平台 ID/);
    expect(idInput).toHaveValue(mockCustomAgent.id);

    fireEvent.change(idInput, {
      target: { value: "renamed-platform" },
    });
    fireEvent.click(screen.getAllByRole("button", { name: "保存" })[0]);

    await waitFor(() => {
      expect(updateCustomAgent).toHaveBeenCalledWith(mockCustomAgent.id, {
        id: "renamed-platform",
        display_name: mockCustomAgent.display_name,
        global_skills_dir: mockCustomAgent.global_skills_dir,
      });
    });
  });

  it("opens editable platform dialog for builtin platforms", async () => {
    setupMocks({ agents: [mockBuiltinAgent] });
    renderSettingsView();
    expandSoftwarePlatforms();
    fireEvent.click(
      screen.getByRole("button", { name: `编辑平台 ${mockBuiltinAgent.display_name}` })
    );
    await waitFor(() => {
      expect(screen.getByText("编辑平台")).toBeTruthy();
      expect(screen.getByLabelText("平台名称 *")).toBeEnabled();
      expect(screen.getAllByRole("button", { name: "保存" })[0]).toBeTruthy();
    });
  });

  it("removes a custom platform after inline confirmation", async () => {
    const removeCustomAgent = vi.fn().mockResolvedValue(undefined);
    const rescan = vi.fn().mockResolvedValue(undefined);
    setupMocks({
      agents: [mockBuiltinAgent, mockCustomAgent],
      removeCustomAgent,
      rescan,
    });
    renderSettingsView();
    expandSoftwarePlatforms();

    fireEvent.click(
      screen.getByRole("button", { name: `删除平台 ${mockCustomAgent.display_name}` })
    );
    expect(removeCustomAgent).not.toHaveBeenCalled();

    fireEvent.click(screen.getByRole("button", { name: "确认删除" }));

    await waitFor(() => {
      expect(removeCustomAgent).toHaveBeenCalledWith(mockCustomAgent.id);
    });
  });

  it("triggers rescan after removing a platform", async () => {
    const removeCustomAgent = vi.fn().mockResolvedValue(undefined);
    const rescan = vi.fn().mockResolvedValue(undefined);
    setupMocks({
      agents: [mockBuiltinAgent, mockCustomAgent],
      removeCustomAgent,
      rescan,
    });
    renderSettingsView();
    expandSoftwarePlatforms();

    fireEvent.click(
      screen.getByRole("button", { name: `删除平台 ${mockCustomAgent.display_name}` })
    );
    fireEvent.click(screen.getByRole("button", { name: "确认删除" }));

    await waitFor(() => {
      expect(rescan).toHaveBeenCalled();
    });
  });

  it("keeps project directories out of the coding software platform list", () => {
    const projectAgent: AgentWithStatus = {
      id: "project:2",
      display_name: "My Project",
      global_skills_dir: `${mockCustomDir.path}/.agents/skills`,
      project_skills_dir: ".agents/skills",
      is_detected: true,
      is_builtin: false,
      is_enabled: true,
    };
    setupMocks({
      scanDirs: [mockCustomDir],
      agents: [mockBuiltinAgent, projectAgent],
    });
    renderSettingsView();

    expandSoftwarePlatforms();

    expect(screen.getByText("Claude Code")).toBeTruthy();
    expect(screen.queryByText("My Project")).toBeNull();
    expect(screen.queryByText("自定义平台")).toBeNull();
    expect(screen.queryByText(`${mockCustomDir.path}/.agents/skills`)).toBeNull();
    expect(screen.queryByText(mockCustomDir.path)).toBeNull();
  });

  it("shows independent and shared directory labels on software platforms", () => {
    const sharedAgent: AgentWithStatus = {
      ...mockMissingBuiltinAgent,
      shares_central_skills: true,
    };
    setupMocks({
      agents: [{ ...mockBuiltinAgent, shares_central_skills: false }, sharedAgent],
    });
    renderSettingsView();
    expandSoftwarePlatforms();

    expect(screen.getByText("独立目录")).toBeTruthy();
    expect(screen.getByText("共享目录")).toBeTruthy();
  });

  it("refreshes software platforms and project directories together", async () => {
    const refreshCounts = vi.fn().mockResolvedValue(undefined);
    const loadScanDirectories = vi.fn().mockResolvedValue(undefined);
    setupMocks({
      agents: [mockBuiltinAgent],
      scanDirs: [mockCustomDir],
      refreshCounts,
      loadScanDirectories,
    });
    renderSettingsView();
    expandSoftwarePlatforms();
    const loadCallsAfterMount = loadScanDirectories.mock.calls.length;

    fireEvent.click(
      screen.getByRole("button", { name: "刷新软件平台" })
    );

    await waitFor(() => {
      expect(refreshCounts).toHaveBeenCalledTimes(1);
      expect(loadScanDirectories.mock.calls.length).toBeGreaterThan(loadCallsAfterMount);
    });
    expect(screen.getByRole("button", { name: "刷新软件平台" })).toHaveTextContent(
      "刷新"
    );
  });

  it("disables the locations refresh button while a scan is in flight", async () => {
    let resolveRefresh: () => void = () => {};
    const refreshCounts = vi.fn().mockImplementation(
      () =>
        new Promise<void>((resolve) => {
          resolveRefresh = resolve;
        })
    );
    setupMocks({
      agents: [mockBuiltinAgent, mockMissingBuiltinAgent],
      scanDirs: [mockCustomDir],
      refreshCounts,
    });
    renderSettingsView();
    expandSoftwarePlatforms();

    const refreshButton = screen.getByRole("button", { name: "刷新软件平台" });
    fireEvent.click(refreshButton);
    fireEvent.click(refreshButton);

    await waitFor(() => {
      expect(refreshButton).toBeDisabled();
    });
    expect(refreshCounts).toHaveBeenCalledTimes(1);

    resolveRefresh();
    await waitFor(() => {
      expect(refreshButton).not.toBeDisabled();
    });
  });

  // ── About section ─────────────────────────────────────────────────────────

  it("shows the app version in the about section", () => {
    setupMocks();
    renderSettingsView();
    expect(screen.getByText(`Arsenal v${APP_VERSION}`)).toBeTruthy();
  });

  it("shows an editable config folder path", () => {
    setupMocks({
      scanDirs: [mockBuiltinDir],
      agents: [mockBuiltinAgent],
      configDir: "/Users/test/.arsenal",
    });
    renderSettingsView();
    expect(screen.getByLabelText("配置文件路径")).toHaveValue("/Users/test/.arsenal");
    expect(screen.getByRole("button", { name: "浏览配置文件路径" })).toBeTruthy();
    expect(screen.getByRole("button", { name: "打开配置文件路径" })).toBeTruthy();
    expect(screen.getByRole("button", { name: "浏览技能仓库路径" })).toBeTruthy();
    expect(screen.getByRole("button", { name: "打开技能仓库路径" })).toBeTruthy();
    expect(screen.getByRole("button", { name: "浏览共享中心路径" })).toBeTruthy();
    expect(screen.getByRole("button", { name: "打开共享中心路径" })).toBeTruthy();
  });

  it("saves a custom config folder path", async () => {
    const updateConfigDir = vi.fn().mockResolvedValue("D:/Apps/Arsenal/.arsenal");
    setupMocks({
      configDir: "/Users/test/.arsenal",
      updateConfigDir,
    });
    renderSettingsView();

    fireEvent.change(screen.getByLabelText("配置文件路径"), {
      target: { value: "D:/Apps/Arsenal" },
    });
    fireEvent.keyDown(screen.getByLabelText("配置文件路径"), { key: "Enter" });

    await waitFor(() => {
      expect(updateConfigDir).toHaveBeenCalledWith("D:/Apps/Arsenal");
    });
    expect(await screen.findByText("配置文件路径已保存，请重启应用后生效")).toBeTruthy();
  });

  it("browses a config folder and saves the selected path", async () => {
    const updateConfigDir = vi.fn().mockResolvedValue("D:/Apps/Arsenal/.arsenal");
    mockOpenDialog.mockResolvedValueOnce("D:/Apps/Arsenal");
    setupMocks({
      configDir: "/Users/test/.arsenal",
      updateConfigDir,
    });
    renderSettingsView();

    fireEvent.click(screen.getByRole("button", { name: "浏览配置文件路径" }));

    await waitFor(() => {
      expect(updateConfigDir).toHaveBeenCalledWith("D:\\Apps\\Arsenal");
    });
  });

  it("opens the config folder in the file manager", async () => {
    setupMocks({ configDir: "/Users/test/.arsenal" });
    renderSettingsView();

    fireEvent.click(screen.getByRole("button", { name: "打开配置文件路径" }));

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("open_in_file_manager", {
        path: "/Users/test/.arsenal",
      });
    });
  });

  it("shows version label", () => {
    setupMocks();
    renderSettingsView();
    expect(screen.getByText("应用版本")).toBeTruthy();
  });

  it("checks for app updates from the about section", async () => {
    const checkAppUpdate = vi.fn().mockResolvedValue({
      currentVersion: "0.12.0",
      latestVersion: "0.13.0",
      latestUrl: "https://github.com/yufenglyu/Arsenal/releases/tag/v0.13.0",
      isUpdateAvailable: true,
      releaseName: "v0.13.0",
      publishedAt: "2026-07-22T00:00:00Z",
    });
    setupMocks({ checkAppUpdate });
    renderSettingsView();

    fireEvent.click(screen.getByRole("button", { name: "检查更新" }));

    await waitFor(() => {
      expect(checkAppUpdate).toHaveBeenCalled();
    });
    expect(await screen.findByText("发现新版本 v0.13.0。")).toBeTruthy();
  });

  it("shows the latest release link after an update check", () => {
    setupMocks({
      updateInfo: {
        currentVersion: "0.12.0",
        latestVersion: "0.12.1",
        latestUrl: "https://github.com/yufenglyu/Arsenal/releases/tag/v0.12.1",
        isUpdateAvailable: true,
        releaseName: "v0.12.1",
        publishedAt: "2026-07-22T00:00:00Z",
      },
    });
    renderSettingsView();

    expect(screen.getByText("最新版本 v0.12.1")).toBeTruthy();
    expect(screen.getByRole("link", { name: "打开发布页" })).toHaveAttribute(
      "href",
      "https://github.com/yufenglyu/Arsenal/releases/tag/v0.12.1"
    );
  });

  it("shows config folder path label", () => {
    setupMocks();
    renderSettingsView();
    expect(screen.getAllByText("配置文件路径").length).toBeGreaterThan(0);
  });

  it("does not render theme flavor or accent controls in settings", () => {
    setupMocks();
    renderSettingsView();

    expect(screen.queryByText("主题风格")).toBeNull();
    expect(screen.queryByRole("button", { name: /Mocha|Macchiato|Frappé|Latte/ })).toBeNull();
    expect(screen.queryByText("强调色")).toBeNull();
    expect(screen.queryByRole("radiogroup", { name: "强调色" })).toBeNull();
  });
});

it("collapses paths and access token by default and renders one platform title with an icon", () => {
  setupMocks();
  renderSettingsView(false);
  for (const title of ["路径配置", "访问令牌"]) {
    const summary=screen.getByText(title).closest("summary")!;
    const details=summary.closest("details")!;
    expect(details).not.toHaveAttribute("open");
    fireEvent.click(summary);
    expect(details).toHaveAttribute("open");
    fireEvent.click(summary);
    expect(details).not.toHaveAttribute("open");
  }
  expect(screen.getAllByText("软件平台")).toHaveLength(1);
  expect(screen.getByTestId("settings-software-platforms-header").querySelector("svg.lucide-app-window")).not.toBeNull();
});
