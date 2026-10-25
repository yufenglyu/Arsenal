import { create } from "zustand";
import { isTaskActive, type BackgroundTask } from "./taskQueueStore";
import { repositoryHasDeletions, type RepositoryTaskRow } from "@/lib/taskBatches";

export function repositoryRisk(row: RepositoryTaskRow) {
  if (!row.preview) return null;
  if (repositoryHasDeletions(row.preview)) return "deletions";
  if (row.preview.localModified || (row.preview.localModified == null && row.preview.modified.length > 0)) return "localChanges";
  return null;
}

export function repositoryLocked(row: RepositoryTaskRow, tasks: BackgroundTask[]) {
  return tasks.some(task => isTaskActive(task) && task.locks.some(lock =>
    lock === "repo:*" || lock === `repo:${row.repository.toLowerCase()}`));
}

export function repositorySelectable(row: RepositoryTaskRow, tasks: BackgroundTask[], migrating: string | null) {
  return row.pending && !["queued", "running"].includes(row.status) && !row.needsCheck && !row.error && !!row.preview?.snapshotId &&
    !migrating && !repositoryLocked(row, tasks);
}

export const useTaskReviewStore = create<{
  excluded: string[];
  approved: string[];
  select: (id: string, value: boolean) => void;
  approve: (id: string) => void;
  forget: (ids: string[]) => void;
}>((set) => ({
  excluded: [], approved: [],
  select: (id, value) => set(s => ({excluded: value ? s.excluded.filter(x => x !== id) : [...new Set([...s.excluded, id])]})),
  approve: id => set(s => ({approved: [...new Set([...s.approved, id])], excluded: s.excluded.filter(x => x !== id)})),
  forget: ids => set(s => ({approved: s.approved.filter(id => !ids.includes(id)), excluded: s.excluded.filter(id => !ids.includes(id))})),
}));

export function selectedRepositories(rows: RepositoryTaskRow[], tasks: BackgroundTask[], migrating: string | null, selection: {excluded: string[]; approved: string[]}) {
  return rows.filter(row => repositorySelectable(row, tasks, migrating) &&
    !selection.excluded.includes(row.preview!.snapshotId!) &&
    (!repositoryRisk(row) || selection.approved.includes(row.preview!.snapshotId!)));
}
