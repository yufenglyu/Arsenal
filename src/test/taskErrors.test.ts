import { expect, it } from "vitest";
import i18n from "@/i18n";
import { taskErrorMessage } from "@/stores/taskQueueStore";
import { describeTaskError, normalizeTaskResult } from "@/lib/taskErrors";

it.each([
  ["Invalid repository file tree", "invalidRemoteResponse"],
  ["Repository file tree is incomplete; existing documents were preserved", "incompleteRepositoryTree"],
  ["Repository star count is unavailable", "repositoryMetadataMissing"],
  ["Repository README is not valid UTF-8", "invalidEncoding"],
  ["Repository root SKILL.md is missing valid frontmatter.", "invalidSkillMetadata"],
  ["Invalid repository import rules", "invalidImportRules"],
  ["Failed to stage repository README: Access is denied. (os error 5)", "filePermission"],
  ["Failed to install staged skill: file in use (os error 32)", "fileBusy"],
  ["No space left on device (os error 28)", "diskFull"],
  ["Read-only file system (os error 30)", "readOnly"],
  ["GitHub response HTTP 502", "remoteService"],
  ["GitHub repository not found.", "repositoryMissing"],
  ["请先在设置中配置 AI API Key", "aiNotConfigured"],
  ["AI 返回的标签格式无效，请重试", "invalidAiResponse"],
] as const)("explains %s with a specific recovery action", (raw, key) => {
  expect(taskErrorMessage(raw)).toBe(i18n.t(`workflow.errors.${key}`));
  expect(taskErrorMessage(raw)).not.toBe(i18n.t("workflow.errors.other"));
});

it("keeps diagnostics stable across repeated rendering and language changes", async () => {
  const original = i18n.language;
  try {
    await i18n.changeLanguage("en");
    const english = taskErrorMessage("Permission denied (os error 13)");
    await i18n.changeLanguage("zh");
    expect(taskErrorMessage(english)).toBe(i18n.t("workflow.errors.filePermission"));
    const unknown = taskErrorMessage("opaque provider response", "import", "reimport_github_repo_skills");
    expect(taskErrorMessage(unknown)).toBe(unknown);
    expect(taskErrorMessage("网络连接失败，请检查网络后重试")).toBe(i18n.t("workflow.errors.network"));
    expect(taskErrorMessage("GitHub 拒绝访问，请检查仓库权限及 GitHub 访问凭据")).toBe(i18n.t("workflow.errors.githubAuthorization"));
    expect(taskErrorMessage("操作失败，请检查配置与目标状态后重试")).toBe(i18n.t("workflow.errors.reasonMissing"));
  } finally {await i18n.changeLanguage(original);}
});

it("preserves structured errors and more specific filesystem causes", () => {
  expect(describeTaskError({code:"TARGET_DIRECTORY_CONFLICT",message:"private-value"}).code).toBe("TARGET_DIRECTORY_CONFLICT");
  expect(taskErrorMessage("WRITE_FAILED: content verification failed")).toBe(i18n.t("workflow.errors.contentVerification"));
  expect(taskErrorMessage({code:"WRITE_FAILED",message:"Access is denied (os error 5) private-value"})).toBe(i18n.t("workflow.errors.filePermission"));
});

it("sanitizes per-repository failures before persistence and detects missing results", () => {
  const command = "preview_source_backed_resource_repository_updates";
  const result = normalizeTaskResult({repositories:[{repository:"example/failed",error:"Failed to write private-value (os error 32)"}]}, "check", command, {repositories:["example/failed","example/missing"]}) as {repositories:{repository:string;error:string;errorCode:string}[]};
  expect(result.repositories).toHaveLength(2);
  expect(result.repositories[0].error).toBe(i18n.t("workflow.errors.fileBusy"));
  expect(result.repositories[1].errorCode).toBe("TASK_RESULT_MISSING");
  expect(JSON.stringify(result)).not.toContain("private-value");
  expect(()=>normalizeTaskResult(undefined, "check", command)).toThrow();
  expect(()=>normalizeTaskResult(null, "check", "refresh_repository")).toThrow();
  expect(normalizeTaskResult(0, "check", "refresh_repository")).toBe(0);
});

it("keeps sensitive payloads out of messages for every task kind", () => {
  for (const kind of ["check", "update", "import", "ai"] as const) {
    const message = taskErrorMessage(new Error('unrecognized payload password=private-value https://name:private-value@example.test/?token=private-value C:\\private\\notes.txt'), kind);
    expect(message).not.toMatch(/private-value|private|example.test|password=|token=/);
    expect(message).not.toBe(i18n.t("workflow.errors.other"));
    expect(message).toMatch(/TASK_UNKNOWN/);
  }
});
