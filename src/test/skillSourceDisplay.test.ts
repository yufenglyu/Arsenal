import {
  getSkillSourceLineKeys,
  getSkillSourceLocation,
  isExceptionalSkillOrigin,
} from "@/lib/skillSourceDisplay";

describe("skill source display", () => {
  it("labels a platform symlink into Shared Hub", () => {
    expect(
      getSkillSourceLocation({
        is_central: true,
        link_type: "symlink",
        symlink_target: "~/.agents/skills/frontend-design",
      })
    ).toBe("central");
    expect(getSkillSourceLineKeys("symlink", "central").label).toBe(
      "platform.sourceLine.centralSymlink"
    );
  });

  it("labels a Shared Hub entry that links to the skill repository", () => {
    expect(
      getSkillSourceLocation({
        is_central: true,
        link_type: "symlink",
        symlink_target: "~/.arsenal/library/owner/repo/shared-skill",
      })
    ).toBe("resource-library");
    expect(getSkillSourceLineKeys("symlink", "resource-library").label).toBe(
      "platform.sourceLine.resourceSymlink"
    );
  });

  it("labels a real directory in Shared Hub", () => {
    expect(
      getSkillSourceLocation({
        is_central: true,
        link_type: "native",
      })
    ).toBe("central");
    expect(getSkillSourceLineKeys("native", "central").label).toBe(
      "platform.sourceLine.centralNative"
    );
  });

  it("labels a platform copy that is not central", () => {
    expect(
      getSkillSourceLocation({
        is_central: false,
        link_type: "copy",
      })
    ).toBe("standalone");
  });

  it("labels a resource-library origin even without a central-looking target", () => {
    expect(
      getSkillSourceLocation({
        is_central: false,
        link_type: "symlink",
        symlink_target: "~/Skills/resource-linked-skill",
        source: "resource-library",
      })
    ).toBe("resource-library");
  });

  it("keeps managed platform origins exceptional", () => {
    expect(isExceptionalSkillOrigin("compatibility")).toBe(true);
    expect(isExceptionalSkillOrigin("shared-central")).toBe(true);
    expect(isExceptionalSkillOrigin("plugin")).toBe(false);
    expect(isExceptionalSkillOrigin("user")).toBe(false);
  });
});
