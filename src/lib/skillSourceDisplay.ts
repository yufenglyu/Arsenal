export type SkillSourceLocation = "central" | "resource-library" | "standalone";

export function getSkillSourceLineKeys(
  sourceType: string,
  sourceLocation: SkillSourceLocation = "standalone"
): { label: string; hint: string } {
  const location =
    sourceLocation === "central"
      ? "central"
      : sourceLocation === "resource-library"
        ? "resource"
        : "standalone";
  const kind =
    sourceType === "symlink" ? "Symlink" : sourceType === "native" ? "Native" : "Copy";
  const label = `platform.sourceLine.${location}${kind}`;
  return { label, hint: `${label}Hint` };
}

export function isExceptionalSkillOrigin(
  originKind?: string | null
): originKind is "compatibility" | "shared-central" {
  return originKind === "compatibility" || originKind === "shared-central";
}

export function getSkillSourceLocation(skill: {
  is_central?: boolean;
  source?: string | null;
  source_url?: string | null;
  source_repo?: string | null;
  link_type?: string;
  symlink_target?: string | null;
}): SkillSourceLocation {
  const target = normalizePath(skill.symlink_target);
  if (skill.link_type === "symlink") {
    if (targetLooksLikeCentralSkills(target)) {
      return "central";
    }
    if (
      skill.is_central ||
      looksLikeResourceOrigin(skill) ||
      targetLooksLikeResourceLibrary(target)
    ) {
      return "resource-library";
    }
    return "standalone";
  }

  if (skill.is_central) {
    return "central";
  }

  if (looksLikeResourceOrigin(skill)) {
    return "resource-library";
  }

  return "standalone";
}

function normalizePath(value?: string | null): string {
  return (value ?? "").replace(/\\/g, "/").toLowerCase();
}

function targetLooksLikeCentralSkills(target: string): boolean {
  return target.includes("/.agents/skills/") || target.endsWith("/.agents/skills");
}

function targetLooksLikeResourceLibrary(target: string): boolean {
  return (
    target.includes("/.arsenal/") ||
    target.includes("/.skillshub/") ||
    target.includes("resource-library") ||
    target.includes("/library/")
  );
}

function looksLikeResourceOrigin(skill: {
  source?: string | null;
  source_url?: string | null;
  source_repo?: string | null;
}): boolean {
  const source = skill.source?.toLowerCase();
  return (
    source === "resource-library" ||
    source === "manual" ||
    Boolean(source?.startsWith("github:")) ||
    Boolean(skill.source_url) ||
    Boolean(skill.source_repo)
  );
}
