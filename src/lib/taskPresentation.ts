import type {BackgroundTask, TaskStatus} from "@/stores/taskQueueStore";
import {isTaskActive} from "@/stores/taskQueueStore";

export type TaskTab = "import" | "update";
export type TaskFilter = "all" | "active" | "attention" | "failed" | "finished";
// Extract only the public repository identity, never URL credentials/query/ref.
export function importRepository(tasks: Pick<BackgroundTask, "steps">[]): string | null {
  for (const task of tasks) for (const step of task.steps) {
    if (!["prepare_github_resource_import", "import_github_repo_skills"].includes(step.command) || typeof step.args.repoUrl !== "string") continue;
    let path = step.args.repoUrl.trim();
    if (/^https?:\/\//i.test(path)) {
      try {const url = new URL(path); if(url.hostname.toLowerCase() !== "github.com") continue; path = url.pathname.replace(/^\//, "");} catch {continue;}
    }
    const match = /^([\w-]+)\/([\w.-]+)(?:\/|$)/.exec(path);
    if (match) return `${match[1]}/${match[2].replace(/\.git$/, "")}`;
  }
  return null;
}
export function taskTab(task: BackgroundTask): TaskTab {
  return task.kind === "import" ? "import" : "update";
}
export function groupStatus(tasks: BackgroundTask[]): TaskStatus | "stopping" {
  const active = tasks.filter(isTaskActive);
  if (active.length) return active.every(task => task.cancelRequested) ? "stopping"
    : active.some(task => task.status === "running") ? "running" : "queued";
  if (tasks.some(task => task.status === "awaiting_input")) return "awaiting_input";
  if (tasks.every(task => task.status === "failed")) return "failed";
  if (tasks.every(task => task.status === "interrupted")) return "interrupted";
  if (tasks.some(task => ["failed", "partial", "interrupted"].includes(task.status))) return "partial";
  return tasks.some(task => task.status === "cancelled") ? "cancelled" : "success";
}
export function matchesTaskFilter(status: ReturnType<typeof groupStatus>, filter: TaskFilter) {
  if (filter === "all") return true;
  if (filter === "active") return ["running", "queued", "stopping"].includes(status);
  if (filter === "attention") return ["awaiting_input", "partial", "failed", "interrupted"].includes(status);
  if (filter === "failed") return ["partial", "failed", "interrupted"].includes(status);
  return ["success", "cancelled"].includes(status);
}
export function taskPriority(status: ReturnType<typeof groupStatus>) {
  return matchesTaskFilter(status, "active") ? 0 : matchesTaskFilter(status, "attention") ? 1 : 2;
}

export interface ImportDetailRow {key: string; name: string; status: TaskStatus | "imported" | "skipped" | "ready"; error?: string}
function records(value: unknown): Record<string, unknown>[] {
  return Array.isArray(value) ? value.filter((item): item is Record<string, unknown> => !!item && typeof item === "object") : [];
}
// Read only the documented skill/result fields. Never render arbitrary IPC payloads.
export function importDetailRows(tasks: BackgroundTask[]): ImportDetailRow[] {
  const importing = tasks.some(task => task.steps.some(step => step.command === "import_github_repo_skills"));
  return tasks.flatMap(task => task.steps.flatMap((step, index): ImportDetailRow[] => {
    const key = `${task.id}:${index}`;
    const result = step.result && typeof step.result === "object" ? step.result as Record<string, unknown> : {};
    if (step.command === "prepare_github_resource_import") {
      if (importing) return [];
      return records(result.skills).flatMap((skill, i) => typeof skill.name === "string" ? [{key:`${key}:${i}`,name:skill.name,
        status:task.status === "awaiting_input" ? skill.conflict ? "awaiting_input" : "ready" : step.status ?? task.status}] : []);
    }
    const imported = records(result.importedSkills).flatMap((skill, i) => typeof skill.skillName === "string"
      ? [{key:`${key}:imported:${i}`,name:skill.skillName,status:"imported" as const}] : []);
    const skipped = Array.isArray(result.skippedSkills) ? result.skippedSkills.flatMap((name, i) => typeof name === "string"
      ? [{key:`${key}:skipped:${i}`,name,status:"skipped" as const}] : []) : [];
    if (imported.length || skipped.length) return [...imported, ...skipped];
    if (step.command === "import_github_repo_skills") {
      return records(step.args.selections).flatMap((selection, i) => typeof selection.sourcePath === "string" ? [{
        key:`${key}:${i}`,name:selection.sourcePath,status:selection.resolution === "skip" ? "skipped" : step.status === "running"
          ? task.importProgress?.currentSkill === selection.sourcePath ? "running" : "queued" : step.status ?? task.status,error:step.error,
      }] : []);
    }
    return [{key,name:step.label,status:step.status ?? task.status,error:step.error}];
  }));
}
