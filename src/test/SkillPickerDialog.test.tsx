import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, waitFor } from "@testing-library/react";

import { SkillPickerDialog } from "@/components/collection/SkillPickerDialog";
import { useResourceLibraryStore } from "@/stores/resourceLibraryStore";
import type { SkillWithLinks } from "@/types";

vi.mock("@/stores/resourceLibraryStore", () => ({
  useResourceLibraryStore: vi.fn(),
}));

const mockLoadResourceLibrary = vi.fn();

const resourceSkills: SkillWithLinks[] = [
  {
    id: "resource-skill",
    name: "resource-skill",
    description: "Skill from resource library",
    file_path: "~/.arsenal/library/resource-skill/SKILL.md",
    canonical_path: "~/.arsenal/library/resource-skill",
    is_central: false,
    scanned_at: "2026-04-09T00:00:00Z",
    linked_agents: [],
    read_only_agents: [],
  },
];

function buildResourceStoreState(overrides = {}) {
  return {
    skills: resourceSkills,
    agents: [],
    resourceLibraryDir: "~/.arsenal/library",
    isLoading: false,
    isInstalling: false,
    isUpdatingSources: false,
    togglingAgentId: null,
    deletingSkillId: null,
    error: null,
    loadResourceLibrary: mockLoadResourceLibrary,
    installSkill: vi.fn(),
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

function renderDialog(overrides = {}) {
  vi.mocked(useResourceLibraryStore).mockImplementation((selector) =>
    selector(buildResourceStoreState(overrides))
  );

  return render(
    <SkillPickerDialog
      open
      onOpenChange={vi.fn()}
      existingSkillIds={[]}
      onAdd={vi.fn()}
    />
  );
}

describe("SkillPickerDialog", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mockLoadResourceLibrary.mockResolvedValue(undefined);
  });

  it("loads and displays skills from the resource library", async () => {
    renderDialog();

    await waitFor(() => {
      expect(mockLoadResourceLibrary).toHaveBeenCalledTimes(1);
    });
    expect(screen.getByText("resource-skill")).toBeInTheDocument();
    expect(screen.getByText("Skill from resource library")).toBeInTheDocument();
  });
});
