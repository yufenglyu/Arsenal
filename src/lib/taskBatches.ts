import { activityGroups, isTaskVisible } from "./activityCenter";
import { groupStatus, importRepository } from "./taskPresentation";
import {
  isTaskActive,
  type BackgroundTask,
  type TaskStatus,
} from "@/stores/taskQueueStore";
import type { AppStatusTask } from "@/stores/appStatusStore";
import type {
  RepositorySyncPreview,
  RepositorySyncPreviewReport,
} from "@/types";

export const CHECK_REPOSITORIES =
  "preview_source_backed_resource_repository_updates";
const needsSnapshotCheck = (repo?: RepositorySyncPreview) => !repo || !!repo.error ||
  (!repo.snapshotId && (repositoryHasChanges(repo) || !repo.remoteRef));
export const retryableTask = (task: BackgroundTask) =>
  ["failed", "partial", "interrupted", "cancelled"].includes(task.status);
export const repositoryHasChanges = (repo: RepositorySyncPreview) =>
  !!(
    repo.added.length +
    repo.modified.length +
    repo.deleted.length +
    (repo.documentChanges?.length ?? 0)
  );
export const repositoryHasDeletions = (repo: RepositorySyncPreview) =>
  !!repo.deleted.length ||
  !!repo.documentChanges?.some((file) => file.status === "deleted") ||
  repo.modified.some((item) =>
    item.files?.some((file) => file.status === "deleted"),
  );

export interface ReportState {
  dismissedRepositories?: string[];
  preview: RepositorySyncPreviewReport | null;
  reportGeneration: string | null;
  reportCheckedAt: number | null;
  repositories: string[] | null;
  requestedRepositories: string[] | null;
  error: string | null;
  isChecking: boolean;
  checkingRepository: string | null;
  updateErrors: Record<string, string>;
}
export interface RepositoryTaskRow {
  repository: string;
  preview?: RepositorySyncPreview;
  status: TaskStatus;
  error?: string;
  needsCheck: boolean;
  pending: boolean;
  applied?: boolean;
}
export interface TaskBatch {
  id: string;
  kind: "import" | "refresh" | "update";
  label: string;
  repository?: string;
  scope?: BackgroundTask["scope"];
  status: ReturnType<typeof groupStatus>;
  tasks: BackgroundTask[];
  createdAt: number;
  report?: RepositoryTaskRow[];
  reportError?: string;
  legacy?: AppStatusTask;
}

export function reportRows(
  state: ReportState,
  tasks: BackgroundTask[],
): RepositoryTaskRow[] {
  const rows = new Map<string, RepositoryTaskRow>();
  for (const repo of state.preview?.repositories ?? [])
    rows.set(repo.repository.toLowerCase(), {
      repository: repo.repository,
      preview: repo,
      status: repo.error ? "failed" : "success",
      error: repo.error ?? undefined,
      needsCheck: needsSnapshotCheck(repo),
      pending: repositoryHasChanges(repo),
      applied: !!repo.updated?.length,
    });
  // Newer checks supersede older outcomes for the same repository. Only known
  // identity/status fields are read; arbitrary args/results never reach the UI.
  for (const task of [...tasks].sort((a, b) => a.createdAt - b.createdAt))
    for (const step of task.steps) {
      if (step.command !== CHECK_REPOSITORIES) continue;
      const names = Array.isArray(step.args.repositories)
        ? step.args.repositories.filter(
            (name): name is string => typeof name === "string",
          )
        : [];
      for (const name of names) {
        const previous = rows.get(name.toLowerCase());
        const status = step.status ?? task.status;
        if (status === "success" && previous) {
          // A later successful retry clears the older failed step, while preserving
          // the report's snapshot and change inventory.
          rows.set(name.toLowerCase(), {
            ...previous,
            status: previous.preview?.error ? "failed" : "success",
            error: previous.preview?.error ?? undefined,
            needsCheck: needsSnapshotCheck(previous.preview),
          });
        } else
          rows.set(name.toLowerCase(), {
            repository: name,
            ...previous,
            status,
            error: step.error ?? previous?.error,
            needsCheck: status !== "success" || needsSnapshotCheck(previous?.preview),
            pending: previous?.pending ?? false,
          });
      }
    }
  for (const [key, row] of rows) {
    const lastCheck = Math.max(-1, ...tasks.filter(task => task.steps.some(step => step.command === CHECK_REPOSITORIES && step.status === "success" && Array.isArray(step.args.repositories) && step.args.repositories.some(name => typeof name === "string" && name.toLowerCase() === key))).map(task => task.createdAt));
    const applyTask = [...tasks]
      .reverse()
      .find((task) => task.createdAt >= lastCheck &&
        task.steps.some(
          (step) =>
            step.command === "apply_repository_snapshot" &&
            step.args.snapshotId === row.preview?.snapshotId,
        ),
      );
    const error = row.preview?.snapshotId
      ? state.updateErrors[`mirror:${row.preview.snapshotId}`]
      : undefined;
    if (applyTask)
      rows.set(key, {
        ...row,
        status: applyTask.status,
        error: error ?? applyTask.steps.find((step) => step.error)?.error,
        pending: applyTask.status === "success" ? false : row.pending,
        applied: applyTask.status === "success" || row.applied,
      });
    else if (error) rows.set(key, { ...row, status: "failed", error });
    const current = rows.get(key)!;
    if (current.error && (applyTask || /PREVIEW_EXPIRED|SNAPSHOT_UNAVAILABLE|LOCAL_IDENTITY_CHANGED|LOCAL_CONTENT_CHANGED/i.test(current.error))) rows.set(key, {...current, needsCheck: true});
  }
  return [...rows.values()].filter(row=>!state.dismissedRepositories?.includes(row.repository.toLowerCase())).sort((a, b) => a.repository.localeCompare(b.repository));
}

export function buildTaskBatches(
  allTasks: BackgroundTask[],
  state: ReportState,
  legacy: AppStatusTask | null,
): TaskBatch[] {
  const tasks = allTasks.filter(isTaskVisible);
  const children = tasks.filter(
    (task) =>
      !!state.reportGeneration &&
      task.parentReportGeneration === state.reportGeneration,
  );
  const hasReport =
    !!state.preview ||
    !!state.error ||
    state.isChecking ||
    !!state.checkingRepository ||
    children.length > 0;
  const batches: TaskBatch[] = activityGroups(
    tasks.filter((task) => !hasReport || !children.includes(task)),
  ).map((group) => {
    const refresh = group.items.every(
      (task) =>
        task.steps.length &&
        task.steps.every((step) =>
          ["refresh_repository", "refresh_repository_stars"].includes(
            step.command,
          ),
        ),
    );
    const repo =
      refresh && group.items.length === 1
        ? group.items[0].steps[0].args.repository
        : undefined;
    return {
      id: group.id,
      kind:
        group.items[0].kind === "import"
          ? "import"
          : refresh
            ? "refresh"
            : "update",
      label: group.label,
      repository:
        group.items[0].kind === "import"
          ? (importRepository(group.items) ?? undefined)
          : typeof repo === "string"
            ? repo
            : undefined,
      scope: group.items[0].scope,
      status: groupStatus(group.items),
      tasks: group.items,
      createdAt: Math.min(...group.items.map((task) => task.createdAt)),
    };
  });
  if (hasReport) {
    const rows = reportRows(state, children);
    const active = children.filter(isTaskActive);
    const failed = rows.some(
      (row) =>
        ["failed", "partial", "interrupted"].includes(row.status) || row.error,
    );
    const stopped = rows.some((row) => row.status === "cancelled");
    const pending = rows.some((row) => row.pending || row.needsCheck);
    const scope = children.find((task) => task.scope)?.scope ?? {
      mode:
        state.repositories?.length || state.requestedRepositories?.length
          ? "selected"
          : "all",
      repositories:
        state.repositories ??
        state.requestedRepositories ??
        rows.map((row) => row.repository),
    };
    batches.push({
      id: "report",
      kind: "update",
      label: "",
      scope,
      status: active.length
        ? groupStatus(active)
        : state.isChecking || state.checkingRepository
          ? "running"
          : state.error || failed
            ? "partial"
            : stopped
              ? "cancelled"
              : pending
                ? "awaiting_input"
                : "success",
      tasks: children,
      createdAt: children.length
        ? Math.min(
            state.reportCheckedAt ?? Infinity,
            ...children.map((task) => task.createdAt),
          )
        : (state.reportCheckedAt ?? 0),
      report: rows,
      reportError: state.error ?? undefined,
    });
  }
  if (
    legacy?.kind &&
    legacy.status !== "idle" &&
    !tasks.some((task) => task.id === legacy.id)
  )
    batches.push({
      id: `status:${legacy.id}`,
      kind: legacy.kind,
      label: legacy.label,
      status:
        legacy.status === "running"
          ? "running"
          : legacy.status === "error"
            ? "partial"
            : "success",
      tasks: [],
      createdAt: Date.parse(legacy.startedAt ?? "") || 0,
      legacy,
    });
  // Fixed creation order: submitting, retrying and completing do not reorder rows.
  return batches.sort(
    (a, b) => b.createdAt - a.createdAt || a.id.localeCompare(b.id),
  );
}
