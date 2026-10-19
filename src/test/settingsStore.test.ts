import { describe, it, expect, vi, beforeEach } from "vitest";
import { ScanDirectory, AgentWithStatus } from "../types";

// Mock Tauri core before importing the store
vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));

import { invoke } from "@tauri-apps/api/core";
import { useSettingsStore } from "../stores/settingsStore";

// ─── Fixtures ─────────────────────────────────────────────────────────────────

const mockBuiltinDir: ScanDirectory = {
  id: 1,
  path: "~/.agents/skills/",
  label: "Shared Hub",
  is_active: true,
  is_builtin: true,
  added_at: "2026-01-01T00:00:00Z",
};

const mockCustomDir: ScanDirectory = {
  id: 2,
  path: "~/projects/my-project",
  label: "My Project",
  is_active: true,
  is_builtin: false,
  added_at: "2026-01-02T00:00:00Z",
};

const mockScanDirectories: ScanDirectory[] = [mockBuiltinDir, mockCustomDir];

const mockAgent: AgentWithStatus = {
  id: "custom-qclaw",
  display_name: "QClaw",
  global_skills_dir: "~/.qclaw/skills/",
  is_detected: false,
  is_builtin: false,
  is_enabled: true,
};

// ─── Tests ────────────────────────────────────────────────────────────────────

describe("settingsStore", () => {
  beforeEach(() => {
    // Reset store to initial state before each test
    useSettingsStore.setState({
      scanDirectories: [],
      isLoadingScanDirs: false,
      error: null,
      githubPat: "",
      isLoadingGitHubPat: false,
      isSavingGitHubPat: false,
      configDir: "",
      isLoadingConfigDir: false,
      webDavConfig: {
        baseUrl: "",
        username: "",
        password: "",
        remoteDir: "skillshub",
      },
      isLoadingWebDavConfig: false,
      isSavingWebDavConfig: false,
    });
    vi.clearAllMocks();
  });

  // ── Initial State ─────────────────────────────────────────────────────────

  it("has correct initial state", () => {
    const state = useSettingsStore.getState();
    expect(state.scanDirectories).toEqual([]);
    expect(state.isLoadingScanDirs).toBe(false);
    expect(state.error).toBeNull();
  });

  // ── loadScanDirectories ───────────────────────────────────────────────────

  it("loadScanDirectories sets isLoadingScanDirs while loading", async () => {
    let resolve!: (value: ScanDirectory[]) => void;
    vi.mocked(invoke).mockReturnValueOnce(
      new Promise<ScanDirectory[]>((r) => (resolve = r))
    );

    const loadPromise = useSettingsStore.getState().loadScanDirectories();
    expect(useSettingsStore.getState().isLoadingScanDirs).toBe(true);

    resolve(mockScanDirectories);
    await loadPromise;
    expect(useSettingsStore.getState().isLoadingScanDirs).toBe(false);
  });

  it("loadScanDirectories populates scanDirectories on success", async () => {
    vi.mocked(invoke).mockResolvedValueOnce(mockScanDirectories);

    await useSettingsStore.getState().loadScanDirectories();

    const state = useSettingsStore.getState();
    expect(state.scanDirectories).toEqual(mockScanDirectories);
    expect(state.isLoadingScanDirs).toBe(false);
    expect(state.error).toBeNull();
  });

  it("loadScanDirectories calls get_scan_directories command", async () => {
    vi.mocked(invoke).mockResolvedValueOnce([]);

    await useSettingsStore.getState().loadScanDirectories();

    expect(invoke).toHaveBeenCalledWith("get_scan_directories");
  });

  it("loadScanDirectories sets error on failure", async () => {
    vi.mocked(invoke).mockRejectedValueOnce(new Error("DB error"));

    await useSettingsStore.getState().loadScanDirectories();

    const state = useSettingsStore.getState();
    expect(state.error).toContain("DB error");
    expect(state.isLoadingScanDirs).toBe(false);
    expect(state.scanDirectories).toEqual([]);
  });

  // ── addScanDirectory ──────────────────────────────────────────────────────

  it("addScanDirectory appends new directory to the list", async () => {
    // Start with one builtin dir
    useSettingsStore.setState({ scanDirectories: [mockBuiltinDir] });

    vi.mocked(invoke).mockResolvedValueOnce(mockCustomDir);

    const result = await useSettingsStore.getState().addScanDirectory(
      "~/projects/my-project",
      "My Project"
    );

    expect(result).toEqual(mockCustomDir);
    const state = useSettingsStore.getState();
    expect(state.scanDirectories).toHaveLength(2);
    expect(state.scanDirectories[1]).toEqual(mockCustomDir);
  });

  it("addScanDirectory calls add_scan_directory with correct args", async () => {
    vi.mocked(invoke).mockResolvedValueOnce(mockCustomDir);

    await useSettingsStore.getState().addScanDirectory("~/my-dir", "Label");

    expect(invoke).toHaveBeenCalledWith("add_scan_directory", {
      path: "~/my-dir",
      label: "Label",
    });
  });

  it("addScanDirectory passes null for label when not provided", async () => {
    vi.mocked(invoke).mockResolvedValueOnce(mockCustomDir);

    await useSettingsStore.getState().addScanDirectory("~/my-dir");

    expect(invoke).toHaveBeenCalledWith("add_scan_directory", {
      path: "~/my-dir",
      label: null,
    });
  });

  it("updateScanDirectory persists name and path", async () => {
    useSettingsStore.setState({ scanDirectories: [mockCustomDir] });
    const updated = { ...mockCustomDir, label: "Renamed", path: "/tmp/renamed" };
    vi.mocked(invoke).mockResolvedValueOnce(updated);

    const result = await useSettingsStore.getState().updateScanDirectory(
      mockCustomDir.path,
      "/tmp/renamed",
      "Renamed"
    );

    expect(invoke).toHaveBeenCalledWith("update_scan_directory", {
      path: mockCustomDir.path,
      newPath: "/tmp/renamed",
      label: "Renamed",
    });
    expect(result).toEqual(updated);
    expect(useSettingsStore.getState().scanDirectories[0]).toEqual(updated);
  });

  it("addScanDirectory throws on failure", async () => {
    vi.mocked(invoke).mockRejectedValueOnce(new Error("UNIQUE constraint"));

    await expect(
      useSettingsStore.getState().addScanDirectory("/duplicate")
    ).rejects.toThrow("UNIQUE constraint");
  });

  // ── removeScanDirectory ───────────────────────────────────────────────────

  it("removeScanDirectory removes directory from list", async () => {
    useSettingsStore.setState({ scanDirectories: [mockBuiltinDir, mockCustomDir] });

    vi.mocked(invoke).mockResolvedValueOnce(undefined);

    await useSettingsStore.getState().removeScanDirectory("~/projects/my-project");

    const state = useSettingsStore.getState();
    expect(state.scanDirectories).toHaveLength(1);
    expect(state.scanDirectories[0].path).toBe("~/.agents/skills/");
  });

  it("removeScanDirectory calls remove_scan_directory command", async () => {
    useSettingsStore.setState({ scanDirectories: [mockCustomDir] });
    vi.mocked(invoke).mockResolvedValueOnce(undefined);

    await useSettingsStore.getState().removeScanDirectory("~/projects/my-project");

    expect(invoke).toHaveBeenCalledWith("remove_scan_directory", {
      path: "~/projects/my-project",
    });
  });

  it("removeScanDirectory throws on failure", async () => {
    vi.mocked(invoke).mockRejectedValueOnce(new Error("Cannot remove builtin"));

    await expect(
      useSettingsStore.getState().removeScanDirectory("~/.agents/skills/")
    ).rejects.toThrow("Cannot remove builtin");
  });

  // ── toggleScanDirectory ───────────────────────────────────────────────────

  it("toggleScanDirectory updates is_active in local state", async () => {
    useSettingsStore.setState({
      scanDirectories: [
        { ...mockCustomDir, is_active: true },
      ],
    });

    vi.mocked(invoke).mockResolvedValueOnce(undefined);
    await useSettingsStore.getState().toggleScanDirectory("~/projects/my-project", false);

    const state = useSettingsStore.getState();
    expect(state.scanDirectories[0].is_active).toBe(false);
  });

  it("toggleScanDirectory calls set_scan_directory_active command", async () => {
    useSettingsStore.setState({
      scanDirectories: [{ ...mockCustomDir, is_active: true }],
    });

    vi.mocked(invoke).mockResolvedValueOnce(undefined);
    await useSettingsStore.getState().toggleScanDirectory("~/projects/my-project", false);

    expect(invoke).toHaveBeenCalledWith("set_scan_directory_active", {
      path: "~/projects/my-project",
      isActive: false,
    });
  });

  it("toggleScanDirectory re-enables a disabled directory", async () => {
    useSettingsStore.setState({
      scanDirectories: [
        { ...mockCustomDir, is_active: false },
      ],
    });

    vi.mocked(invoke).mockResolvedValueOnce(undefined);
    await useSettingsStore.getState().toggleScanDirectory("~/projects/my-project", true);

    expect(useSettingsStore.getState().scanDirectories[0].is_active).toBe(true);
  });

  it("toggleScanDirectory only affects the targeted directory", async () => {
    useSettingsStore.setState({
      scanDirectories: [
        { ...mockBuiltinDir, is_active: true },
        { ...mockCustomDir, is_active: true },
      ],
    });

    vi.mocked(invoke).mockResolvedValueOnce(undefined);
    await useSettingsStore.getState().toggleScanDirectory("~/projects/my-project", false);

    const state = useSettingsStore.getState();
    // builtin dir should be unchanged
    expect(state.scanDirectories[0].is_active).toBe(true);
    // custom dir should be toggled
    expect(state.scanDirectories[1].is_active).toBe(false);
  });

  it("toggleScanDirectory throws on backend failure", async () => {
    useSettingsStore.setState({
      scanDirectories: [{ ...mockCustomDir, is_active: true }],
    });

    vi.mocked(invoke).mockRejectedValueOnce(new Error("DB error"));
    await expect(
      useSettingsStore.getState().toggleScanDirectory("~/projects/my-project", false)
    ).rejects.toThrow("DB error");
  });

  // ── addCustomAgent ────────────────────────────────────────────────────────

  it("addCustomAgent calls add_custom_agent and returns the agent", async () => {
    vi.mocked(invoke).mockResolvedValueOnce(mockAgent);

    const config = {
      display_name: "QClaw",
      global_skills_dir: "~/.qclaw/skills/",
    };

    const result = await useSettingsStore.getState().addCustomAgent(config);

    expect(result).toEqual(mockAgent);
    expect(invoke).toHaveBeenCalledWith("add_custom_agent", { config });
  });

  it("addCustomAgent throws on failure", async () => {
    vi.mocked(invoke).mockRejectedValueOnce(new Error("UNIQUE constraint"));

    await expect(
      useSettingsStore.getState().addCustomAgent({
        display_name: "Dup",
        global_skills_dir: "/dup",
      })
    ).rejects.toThrow("UNIQUE constraint");
  });

  // ── updateCustomAgent ─────────────────────────────────────────────────────

  it("updateCustomAgent calls update_custom_agent and returns updated agent", async () => {
    const updatedAgent = { ...mockAgent, display_name: "QClaw v2" };
    vi.mocked(invoke).mockResolvedValueOnce(updatedAgent);

    const config = {
      display_name: "QClaw v2",
      global_skills_dir: "~/.qclaw/skills/",
    };

    const result = await useSettingsStore.getState().updateCustomAgent("custom-qclaw", config);

    expect(result).toEqual(updatedAgent);
    expect(invoke).toHaveBeenCalledWith("update_custom_agent", {
      agentId: "custom-qclaw",
      config,
    });
  });

  // ── removeCustomAgent ─────────────────────────────────────────────────────

  it("removeCustomAgent calls remove_custom_agent command", async () => {
    vi.mocked(invoke).mockResolvedValueOnce(undefined);

    await useSettingsStore.getState().removeCustomAgent("custom-qclaw");

    expect(invoke).toHaveBeenCalledWith("remove_custom_agent", {
      agentId: "custom-qclaw",
    });
  });

  it("removeCustomAgent throws on failure", async () => {
    vi.mocked(invoke).mockRejectedValueOnce(new Error("Not found"));

    await expect(
      useSettingsStore.getState().removeCustomAgent("nonexistent")
    ).rejects.toThrow("Not found");
  });

  it("exportAppBackup writes a backup archive to the chosen path", async () => {
    vi.mocked(invoke).mockResolvedValueOnce(undefined);
    const options = {
      includeResourceLibrary: true,
      includeCentralLibrary: false,
      includeAppConfig: true,
      includeInstallations: false,
    };

    await useSettingsStore.getState().exportAppBackup("D:/backups/app.zip", options);

    expect(invoke).toHaveBeenCalledWith("export_app_backup_to_path", {
      destPath: "D:/backups/app.zip",
      options,
    });
  });

  it("importAppBackup passes backup bytes to import_app_backup", async () => {
    vi.mocked(invoke).mockResolvedValueOnce(undefined);

    await useSettingsStore.getState().importAppBackup(new Uint8Array([1, 2, 3]));

    expect(invoke).toHaveBeenCalledWith("import_app_backup", { backup: [1, 2, 3], repositoryOnly: false });
  });

  it("limits WebDAV restore to the repository and collections", async () => {
    vi.mocked(invoke).mockResolvedValue(undefined);
    await useSettingsStore.getState().importAppBackup(new Uint8Array([1]), true);
    expect(invoke).toHaveBeenCalledWith("import_app_backup", { backup: [1], repositoryOnly: true });
  });

  it("listWebDavBackups calls list_webdav_backups with session config", async () => {
    vi.mocked(invoke).mockResolvedValueOnce([]);
    const config = {
      baseUrl: "https://example.com/dav",
      username: "user",
      password: "secret",
      remoteDir: "skillshub",
    };

    await useSettingsStore.getState().listWebDavBackups(config);

    expect(invoke).toHaveBeenCalledWith("list_webdav_backups", { config });
  });

  it("testWebDavConnection calls test_webdav_connection with session config", async () => {
    vi.mocked(invoke).mockResolvedValueOnce(undefined);
    const config = {
      baseUrl: "https://example.com/dav",
      username: "user",
      password: "secret",
      remoteDir: "skillshub",
    };

    await useSettingsStore.getState().testWebDavConnection(config);

    expect(invoke).toHaveBeenCalledWith("test_webdav_connection", { config });
  });

  it("uploadWebDavBackup calls upload_webdav_backup with config and options", async () => {
    const file = {
      name: "skillshub-backup.zip",
      remotePath: "skillshub-backup.zip",
      size: 100,
      modifiedAt: "2026-07-15T00:00:00Z",
    };
    vi.mocked(invoke).mockResolvedValueOnce(file);
    const config = {
      baseUrl: "https://example.com/dav",
      username: "",
      password: "",
      remoteDir: "skillshub",
    };
    const options = {
      includeResourceLibrary: true,
      includeCentralLibrary: true,
      includeAppConfig: true,
      includeInstallations: true,
    };

    await useSettingsStore.getState().uploadWebDavBackup(config, options);

    expect(invoke).toHaveBeenCalledWith("upload_webdav_backup", { config, options });
  });

  it("downloadWebDavBackup calls download_webdav_backup with selected remote path", async () => {
    vi.mocked(invoke).mockResolvedValueOnce([80, 75, 3, 4]);
    const config = {
      baseUrl: "https://example.com/dav",
      username: "user",
      password: "secret",
      remoteDir: "skillshub",
    };

    const backup = await useSettingsStore
      .getState()
      .downloadWebDavBackup(config, "skillshub-backup.zip");

    expect(invoke).toHaveBeenCalledWith("download_webdav_backup", {
      config,
      remotePath: "skillshub-backup.zip",
    });
    expect(backup).toEqual(new Uint8Array([80, 75, 3, 4]));
  });

  it("deleteWebDavBackup calls delete_webdav_backup with selected remote path", async () => {
    vi.mocked(invoke).mockResolvedValueOnce(undefined);
    const config = {
      baseUrl: "https://example.com/dav",
      username: "user",
      password: "secret",
      remoteDir: "skillshub",
    };

    await useSettingsStore
      .getState()
      .deleteWebDavBackup(config, "skillshub-backup.zip");

    expect(invoke).toHaveBeenCalledWith("delete_webdav_backup", {
      config,
      remotePath: "skillshub-backup.zip",
    });
  });

  it("loadWebDavConfig loads persisted WebDAV connection settings", async () => {
    vi.mocked(invoke)
      .mockResolvedValueOnce("https://example.com/dav")
      .mockResolvedValueOnce("user")
      .mockResolvedValueOnce("secret")
      .mockResolvedValueOnce("backups");

    await useSettingsStore.getState().loadWebDavConfig();

    expect(invoke).toHaveBeenNthCalledWith(1, "get_setting", { key: "webdav_base_url" });
    expect(invoke).toHaveBeenNthCalledWith(2, "get_setting", { key: "webdav_username" });
    expect(invoke).toHaveBeenNthCalledWith(3, "get_setting", { key: "webdav_password" });
    expect(invoke).toHaveBeenNthCalledWith(4, "get_setting", { key: "webdav_remote_dir" });
    expect(useSettingsStore.getState().webDavConfig).toEqual({
      baseUrl: "https://example.com/dav",
      username: "user",
      password: "secret",
      remoteDir: "backups",
    });
  });

  it("saveWebDavConfig persists WebDAV connection settings", async () => {
    vi.mocked(invoke).mockResolvedValue(undefined);

    await useSettingsStore.getState().saveWebDavConfig({
      baseUrl: " https://example.com/dav ",
      username: "user",
      password: "secret",
      remoteDir: " backups ",
    });

    expect(invoke).toHaveBeenCalledWith("set_setting", {
      key: "webdav_base_url",
      value: "https://example.com/dav",
    });
    expect(invoke).toHaveBeenCalledWith("set_setting", {
      key: "webdav_username",
      value: "user",
    });
    expect(invoke).toHaveBeenCalledWith("set_setting", {
      key: "webdav_password",
      value: "secret",
    });
    expect(invoke).toHaveBeenCalledWith("set_setting", {
      key: "webdav_remote_dir",
      value: "backups",
    });
    expect(useSettingsStore.getState().webDavConfig).toEqual({
      baseUrl: "https://example.com/dav",
      username: "user",
      password: "secret",
      remoteDir: "backups",
    });
    expect(useSettingsStore.getState().isSavingWebDavConfig).toBe(false);
  });

  // ── clearError ────────────────────────────────────────────────────────────

  it("clearError resets error to null", () => {
    useSettingsStore.setState({ error: "Some error" });
    useSettingsStore.getState().clearError();
    expect(useSettingsStore.getState().error).toBeNull();
  });

  it("loadGitHubPat reads the saved github_pat setting", async () => {
    vi.mocked(invoke).mockResolvedValueOnce(" github_pat_123 ");

    await useSettingsStore.getState().loadGitHubPat();

    expect(invoke).toHaveBeenCalledWith("get_setting", { key: "github_pat" });
    expect(useSettingsStore.getState().githubPat).toBe(" github_pat_123 ");
    expect(useSettingsStore.getState().isLoadingGitHubPat).toBe(false);
  });

  it("saveGitHubPat persists a trimmed github_pat setting", async () => {
    vi.mocked(invoke).mockResolvedValueOnce(undefined);

    await useSettingsStore.getState().saveGitHubPat("  github_pat_abc  ");

    expect(invoke).toHaveBeenCalledWith("set_setting", {
      key: "github_pat",
      value: "  github_pat_abc  ",
    });
    expect(useSettingsStore.getState().githubPat).toBe("github_pat_abc");
    expect(useSettingsStore.getState().isSavingGitHubPat).toBe(false);
  });

  it("clearGitHubPat clears the saved github_pat setting", async () => {
    useSettingsStore.setState({ githubPat: "github_pat_abc" });
    vi.mocked(invoke).mockResolvedValueOnce(undefined);

    await useSettingsStore.getState().clearGitHubPat();

    expect(invoke).toHaveBeenCalledWith("set_setting", {
      key: "github_pat",
      value: "",
    });
    expect(useSettingsStore.getState().githubPat).toBe("");
    expect(useSettingsStore.getState().isSavingGitHubPat).toBe(false);
  });

  it("loadConfigDir reads the current config folder", async () => {
    vi.mocked(invoke).mockResolvedValueOnce("/Users/test/.arsenal");

    await useSettingsStore.getState().loadConfigDir();

    expect(invoke).toHaveBeenCalledWith("get_app_data_dir");
    expect(useSettingsStore.getState().configDir).toBe("/Users/test/.arsenal");
    expect(useSettingsStore.getState().isLoadingConfigDir).toBe(false);
  });

  it("updateConfigDir persists the config folder", async () => {
    vi.mocked(invoke).mockResolvedValueOnce("D:/Apps/SkillsHub/.arsenal");

    const updated = await useSettingsStore.getState().updateConfigDir("D:/Apps/SkillsHub");

    expect(invoke).toHaveBeenCalledWith("update_app_data_dir", {
      path: "D:/Apps/SkillsHub",
    });
    expect(updated).toBe("D:/Apps/SkillsHub/.arsenal");
    expect(useSettingsStore.getState().configDir).toBe("D:/Apps/SkillsHub/.arsenal");
  });
});


describe("file-backed language preferences", () => {
  it("loads language using the settings IPC", async () => {
    vi.mocked(invoke).mockResolvedValue("zh");
    await useSettingsStore.getState().loadLanguage();
    expect(invoke).toHaveBeenCalledWith("get_setting", { key: "language" });
  });

  it("saves language using the settings IPC", async () => {
    vi.mocked(invoke).mockResolvedValue(null);
    await useSettingsStore.getState().saveLanguage("zh");
    expect(invoke).toHaveBeenCalledWith("set_setting", { key: "language", value: "zh" });
  });
});
