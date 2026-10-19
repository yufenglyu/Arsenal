import type {BackgroundTask} from "@/stores/taskQueueStore";
import {isTaskActive} from "@/stores/taskQueueStore";
import {repositoryUpdateRows} from "./repositoryUpdateRows";
import type {RepositorySyncPreviewReport} from "@/types";

export type ActivityView = "pending" | "active" | "history";
// AI work keeps its executor/lifecycle, but belongs to its originating editor.
export const isTaskVisible = (task: BackgroundTask) => task.kind !== "ai" && !task.clearRequested;
export const needsAttention = (task: BackgroundTask) => ["failed", "partial", "interrupted", "awaiting_input"].includes(task.status);
export function pendingUpdateCount(preview: RepositorySyncPreviewReport | null, ignored: string[], tasks: BackgroundTask[]) {
  if (preview?.repositories.some(repo => repo.snapshotId)) return preview.repositories.filter(repo => (repo.error || repo.added.length || repo.modified.length || repo.deleted.length || repo.documentChanges?.length) && !tasks.some(task => task.key === `mirror:${repo.snapshotId}` && isTaskActive(task))).length;
  return repositoryUpdateRows(preview?.repositories ?? [], {}).filter(row => row.category !== "unchanged" && row.category !== "updated" && !ignored.includes(row.key) &&
    !tasks.some(task => (task.key === row.key || task.key.startsWith(`${row.pairKey}:replace:`)) && isTaskActive(task))).length + (preview?.repositories.filter(repo => repo.error).length ?? 0);
}
export function attentionTasks(tasks: BackgroundTask[], preview: RepositorySyncPreviewReport | null, checkedAt: Record<string, number>) {
  return tasks.filter(task => isTaskVisible(task) && needsAttention(task) &&
    !tasks.slice(tasks.indexOf(task) + 1).some(later => later.key === task.key) &&
    !task.steps.every(step => {
      if (step.command === "apply_repository_update_item" && task.status !== "interrupted") return !!preview?.repositories.some(repo => repo.repository.toLowerCase() === String(step.args.repository).toLowerCase());
      if (step.command === "preview_source_backed_resource_repository_updates") {
        const report = step.result as RepositorySyncPreviewReport | undefined;
        const scope = Array.isArray(step.args.repositories) ? step.args.repositories : (report?.repositories ?? preview?.repositories)?.map(repo => repo.repository);
        return !!scope?.length && scope.every(repo => (checkedAt[String(repo).toLowerCase()] ?? 0) >= task.createdAt);
      }
      return false;
    }));
}
export function activityGroups(tasks: BackgroundTask[]) {
  const groups = new Map<string, BackgroundTask[]>();
  for (const task of tasks.filter(isTaskVisible)) {
    const key = task.batchId ?? task.id;
    groups.set(key, [...(groups.get(key) ?? []), task]);
  }
  return [...groups.entries()].reverse().map(([id, items]) => ({id, items, label: items[0].batchLabel ?? items[0].label}));
}

export function activityAttention(tasks: BackgroundTask[], preview: RepositorySyncPreviewReport | null, checkedAt: Record<string, number>, error: string | null) {
  const attention = attentionTasks(tasks, preview, checkedAt).filter(task => !error || !task.steps.every(step => step.command === "preview_source_backed_resource_repository_updates"));
  return {attention, count: new Set(attention.map(task => task.batchId ?? task.id)).size};
}
