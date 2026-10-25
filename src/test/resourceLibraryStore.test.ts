import { beforeEach, describe, expect, it, vi } from "vitest";
import type { BatchInstallResult, SkillWithLinks } from "@/types";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));

import { invoke } from "@tauri-apps/api/core";
import { useResourceLibraryStore } from "@/stores/resourceLibraryStore";

const resourceSkill: SkillWithLinks = {
  id: "resource-skill",
  name: "Resource Skill",
  file_path: "C:/library/resource-skill/SKILL.md",
  canonical_path: "C:/library/resource-skill",
  is_central: false,
  scanned_at: "2026-07-18T00:00:00Z",
  linked_agents: [],
  read_only_agents: [],
};

describe("resourceLibraryStore platform installs", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    useResourceLibraryStore.setState({
      skills: [resourceSkill],
      agents: [],
      resourceLibraryDir: "C:/library",
      isLoading: false,
      isInstalling: false,
      isUpdatingSources: false,
      togglingAgentId: null,
      deletingSkillId: null,
      error: null,
    });
  });

  it("coalesces concurrent loads while keeping cached skills visible", async () => {
    let resolveSkills!: (skills: SkillWithLinks[]) => void;
    const pending = new Promise<SkillWithLinks[]>((resolve) => { resolveSkills = resolve; });
    vi.mocked(invoke).mockImplementation(async (command) => {
      if (command === "get_resource_library_skills") return pending;
      if (command === "get_skill_resource_library_dir") return "C:/library";
      return [];
    });
    const first = useResourceLibraryStore.getState().loadResourceLibrary();
    const second = useResourceLibraryStore.getState().loadResourceLibrary();
    expect(first).toBe(second);
    expect(useResourceLibraryStore.getState().skills).toEqual([resourceSkill]);
    resolveSkills([resourceSkill]);
    await Promise.all([first, second]);
    expect(vi.mocked(invoke).mock.calls.filter(([command]) => command === "get_resource_library_skills")).toHaveLength(1);
    expect(useResourceLibraryStore.getState().isLoading).toBe(false);
  });

  it("reloads after an in-flight read when a background mutation requires fresh data", async () => {
    let resolveSkills!: (skills: SkillWithLinks[]) => void;
    const pending = new Promise<SkillWithLinks[]>(resolve => { resolveSkills = resolve; });
    const replacement = {...resourceSkill, id: "replacement"};
    let reads = 0;
    vi.mocked(invoke).mockImplementation(async command => {
      if (command === "get_resource_library_skills") return ++reads === 1 ? pending : [replacement];
      if (command === "get_skill_resource_library_dir") return "C:/library";
      return [];
    });
    const first = useResourceLibraryStore.getState().loadResourceLibrary();
    const fresh = useResourceLibraryStore.getState().loadResourceLibrary({fresh: true});
    resolveSkills([resourceSkill]);
    await Promise.all([first, fresh]);
    expect(reads).toBe(2);
    expect(useResourceLibraryStore.getState().skills).toEqual([replacement]);
  });

  it("uses the resource-specific batch command", async () => {
    const result: BatchInstallResult = { succeeded: ["codex"], failed: [] };
    vi.mocked(invoke).mockResolvedValueOnce(result).mockResolvedValueOnce([resourceSkill]);

    await useResourceLibraryStore
      .getState()
      .installSkill("resource-skill", ["codex"], "symlink");

    expect(invoke).toHaveBeenNthCalledWith(1, "batch_install_resource_skill_to_agents", {
      skillId: "resource-skill",
      agentIds: ["codex"],
      method: "symlink",
    });
  });

  it("uses the resource-specific command for a platform icon toggle", async () => {
    vi.mocked(invoke)
      .mockResolvedValueOnce({ succeeded: ["codex"], failed: [] })
      .mockResolvedValueOnce([{ ...resourceSkill, linked_agents: ["codex"] }]);

    await useResourceLibraryStore.getState().togglePlatformLink("resource-skill", "codex");

    expect(invoke).toHaveBeenNthCalledWith(1, "batch_install_resource_skill_to_agents", {
      skillId: "resource-skill",
      agentIds: ["codex"],
      method: "auto",
    });
  });

  it("uses resource-specific source update commands", async () => {
    vi.mocked(invoke)
      .mockResolvedValueOnce({ items: [] })
      .mockResolvedValueOnce([resourceSkill])
      .mockResolvedValueOnce("resource-skill")
      .mockResolvedValueOnce([resourceSkill]);

    await useResourceLibraryStore.getState().updateSourceBackedSkills();
    await useResourceLibraryStore.getState().updateSourceBackedSkill("resource-skill");

    expect(invoke).toHaveBeenNthCalledWith(1, "update_source_backed_resource_skills");
    expect(invoke).toHaveBeenNthCalledWith(3, "update_source_backed_resource_skill", {
      skillId: "resource-skill",
    });
  });

  it("uses repository preview and sync commands for source updates", async () => {
    vi.mocked(invoke)
      .mockResolvedValueOnce({ repositories: [] })
      .mockResolvedValueOnce([{ ...resourceSkill, github_stars: 123 }])
      .mockResolvedValueOnce({ items: [] })
      .mockResolvedValueOnce([resourceSkill]);

    await useResourceLibraryStore.getState().previewRepositorySync();
    expect(useResourceLibraryStore.getState().skills[0].github_stars).toBe(123);
    await useResourceLibraryStore
      .getState()
      .syncSourceBackedSkills({ includeAdded: true, removeDeleted: true });

    expect(invoke).toHaveBeenNthCalledWith(
      1,
      "preview_source_backed_resource_repository_updates",
      { repositories: null }
    );
    expect(invoke).toHaveBeenNthCalledWith(3, "sync_source_backed_resource_skills", {
      options: { includeAdded: true, removeDeleted: true },
    });
  });
  it("retains a completed repository check if only refreshing the list fails", async () => {
    vi.mocked(invoke).mockResolvedValueOnce({repositories: []}).mockRejectedValueOnce(new Error("File private-value in use (os error 32)"));
    const report = await useResourceLibraryStore.getState().previewRepositorySync();
    expect(report.repositories).toEqual([]);
    expect(report).toHaveProperty("viewRefreshError");
    expect(JSON.stringify(report)).not.toContain("private-value");
  });

  it("removes only the central copy and reloads the resource skill", async () => {
    const restoredSkill = {
      ...resourceSkill,
      is_central: false,
      canonical_path: "C:/library/resource-skill",
    };
    vi.mocked(invoke)
      .mockResolvedValueOnce({
        skillId: "resource-skill",
        removedCentralPath: "C:/.agents/skills/resource-skill",
        resourcePath: "C:/library/resource-skill",
        uninstalledAgents: ["codex"],
      })
      .mockResolvedValueOnce([restoredSkill]);

    await useResourceLibraryStore
      .getState()
      .removeFromCentral("resource-skill");

    expect(invoke).toHaveBeenNthCalledWith(
      1,
      "remove_resource_skill_from_central",
      { skillId: "resource-skill" }
    );
    expect(useResourceLibraryStore.getState().skills).toEqual([restoredSkill]);
  });
});
