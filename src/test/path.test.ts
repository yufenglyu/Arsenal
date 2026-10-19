import {
  compactHomePath,
  deriveHomeDir,
  describeSkillsPattern,
  formatPathForDisplay,
  getPathBasename,
  joinPathForDisplay,
  normalizePathSeparators,
  pathsShareSkillsRoot,
} from "@/lib/path";

describe("path helpers", () => {
  it("normalizes windows separators", () => {
    expect(normalizePathSeparators("C:\\Users\\alice\\.claude\\skills")).toBe(
      "C:/Users/alice/.claude/skills"
    );
  });

  it("compacts unix home paths", () => {
    expect(compactHomePath("/Users/alice/.agents/skills")).toBe("~/.agents/skills");
    expect(compactHomePath("/home/alice/projects/demo")).toBe("~/projects/demo");
  });

  it("compacts windows home paths", () => {
    expect(compactHomePath("C:\\Users\\alice\\.cursor\\skills")).toBe(
      "~/.cursor/skills"
    );
  });

  it("keeps tilde paths stable", () => {
    expect(compactHomePath("~/.arsenal/db.sqlite")).toBe("~/.arsenal/db.sqlite");
  });

  it("keeps unix paths full for display", () => {
    expect(formatPathForDisplay("/Users/alice/.agents/skills")).toBe(
      "/Users/alice/.agents/skills"
    );
    expect(formatPathForDisplay("/home/alice/projects/demo")).toBe(
      "/home/alice/projects/demo"
    );
  });

  it("renders windows display paths with drive letters and backslashes", () => {
    expect(formatPathForDisplay("C:/Users/alice/.cursor/skills")).toBe(
      "C:\\Users\\alice\\.cursor\\skills"
    );
    expect(formatPathForDisplay("C:\\Users\\alice\\.claude\\skills")).toBe(
      "C:\\Users\\alice\\.claude\\skills"
    );
    expect(formatPathForDisplay("C:\\Users\\alice\\.agents/skills/demo/SKILL.md")).toBe(
      "C:\\Users\\alice\\.agents\\skills\\demo\\SKILL.md"
    );
    expect(formatPathForDisplay("\\\\?\\C:\\Users\\alice\\.agents\\skills")).toBe(
      "C:\\Users\\alice\\.agents\\skills"
    );
  });

  it("derives home directories from unix and windows paths", () => {
    expect(deriveHomeDir("/Users/alice/.agents/skills")).toBe("/Users/alice");
    expect(deriveHomeDir("/home/alice/projects/demo")).toBe("/home/alice");
    expect(deriveHomeDir("C:/Users/alice/.cursor/skills")).toBe("C:\\Users\\alice");
  });

  it("joins relative paths using platform-native display style", () => {
    expect(joinPathForDisplay("/Users/alice", ".arsenal/db.sqlite")).toBe(
      "/Users/alice/.arsenal/db.sqlite"
    );
    expect(joinPathForDisplay("C:\\Users\\alice", ".arsenal/db.sqlite")).toBe(
      "C:\\Users\\alice\\.arsenal\\db.sqlite"
    );
  });

  it("extracts basenames for unix and windows paths", () => {
    expect(getPathBasename("/Users/alice/.claude/skills/review")).toBe("review");
    expect(getPathBasename("C:\\Users\\alice\\.claude\\skills\\review")).toBe("review");
  });

  it("describes skill patterns relative to home", () => {
    expect(describeSkillsPattern("/Users/alice/.claude/skills")).toBe(".claude/skills");
    expect(describeSkillsPattern("C:\\Users\\alice\\.cursor\\skills")).toBe(
      ".cursor/skills"
    );
  });

  it("compares skill roots across separators and home-path forms", () => {
    expect(
      pathsShareSkillsRoot(
        "C:\\Users\\alice\\.agents\\skills",
        "C:/Users/alice/.agents/skills/"
      )
    ).toBe(true);
    expect(
      pathsShareSkillsRoot("/Users/alice/.agents/skills", "~/.agents/skills")
    ).toBe(true);
    expect(
      pathsShareSkillsRoot("~/.agents/skills", "~/.cursor/skills")
    ).toBe(false);
  });
});
