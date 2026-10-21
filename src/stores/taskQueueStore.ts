import { create } from "zustand";
import { persist, createJSONStorage } from "zustand/middleware";
import { invoke } from "@/lib/tauri";
import {listen} from "@tauri-apps/api/event";
import type {GitHubImportProgressPayload} from "@/types";
import i18n from "@/i18n";

export type TaskKind = "import" | "check" | "update" | "ai";
export type TaskStatus =
  | "queued"
  | "running"
  | "awaiting_input"
  | "success"
  | "partial"
  | "failed"
  | "cancelled"
  | "interrupted";
export interface TaskStep {
  command: string;
  args: Record<string, unknown>;
  label: string;
  status?: TaskStatus;
  error?: string;
  result?: unknown;
}
export interface BackgroundTask {
  id: string;
  key: string;
  kind: TaskKind;
  label: string;
  locks: string[];
  steps: TaskStep[];
  status: TaskStatus;
  cancelRequested: boolean;
  createdAt: number;
  clearRequested?: boolean;
  batchId?: string;
  batchLabel?: string;
  parentReportGeneration?: string;
  importProgress?: GitHubImportProgressPayload;
  scope?: {mode: "all" | "selected"; repositories: string[]};
  skippedImportSkills?: string[];
}
type Input = Pick<BackgroundTask, "key" | "kind" | "label" | "steps"> & {
  locks?: string[];
  batchId?: string;
  batchLabel?: string;
  parentReportGeneration?: string;
  scope?: BackgroundTask["scope"];
};
type Handler = (step: TaskStep, result: unknown) => void | Promise<void>;
const handlers = new Map<string, Handler>();
const completionHandlers = new Map<string, (task: BackgroundTask) => void | Promise<void>>();
const failureHandlers = new Map<string, (step: TaskStep) => void | Promise<void>>();
const inputRequirements = new Map<string, (result: unknown) => boolean>();
const executors = new Map<
  string,
  (args: Record<string, unknown>) => Promise<unknown>
>();
export function registerTaskExecutor(
  command: string,
  executor: (args: Record<string, unknown>) => Promise<unknown>,
) {
  executors.set(command, executor);
}
export function registerTaskResult(command: string, handler: Handler) {
  handlers.set(command, handler);
}
export function registerTaskCompletion(command: string, handler: (task: BackgroundTask) => void | Promise<void>) {
  completionHandlers.set(command, handler);
}
export function registerTaskFailure(command: string, handler: (step: TaskStep) => void | Promise<void>) {
  failureHandlers.set(command, handler);
}
export function registerTaskInputRequirement(command: string, requiresInput: (result: unknown) => boolean) {
  inputRequirements.set(command, requiresInput);
}
const running = new Set<string>();
const importOperations = new Map<string, string>();
const limits: Record<TaskKind, number> = {
  check: 5,
  update: 2,
  import: 2,
  ai: 2,
};
export const isTaskActive = (task: BackgroundTask) =>
  task.status === "queued" || task.status === "running";
function trimHistory(tasks: BackgroundTask[]) {
  const retained = new Set(
    tasks
      .filter((t) => t.status === "success")
      .slice(-100)
      .map((t) => t.id),
  );
  return tasks.filter((t) => t.status !== "success" || retained.has(t.id));
}
export function taskErrorMessage(
  error: unknown,
  kind: TaskKind = "check",
): string {
  const text = error instanceof Error ? error.message : String(error);
  const code = error && typeof error === "object" && "code" in error ? String(error.code) : /^([A-Z_]+)(?::|$)/.exec(text)?.[1];
  if (code && ["IMPORT_SOURCE_MISSING","PREVIEW_EXPIRED","SNAPSHOT_UNAVAILABLE","LOCAL_IDENTITY_CHANGED","TARGET_DIRECTORY_CONFLICT","SNAPSHOT_LINK_UNAVAILABLE","UNSAFE_TARGET","UNSAFE_INSTALL_TARGET","UNSAFE_SNAPSHOT_PATH","SNAPSHOT_CACHE_FULL","SNAPSHOT_TOO_LARGE","WRITE_FAILED"].includes(code)) return i18n.t(`workflow.errors.${code}`);
  if (/^address[A-Za-z]+$/.test(text) && i18n.exists(`workflow.address.errors.${text}`)) return i18n.t(`workflow.address.errors.${text}`);
  // Background tasks already store safe, translated errors. Preserve them when
  // a dialog or a check wrapper handles that same error a second time.
  const translatedErrors = i18n.t("workflow.errors", { returnObjects: true });
  if (typeof translatedErrors === "object" && translatedErrors !== null && Object.values(translatedErrors).includes(text)) {
    return text;
  }
  const message = text.toLowerCase();
  if (/preview_expired|local_identity_changed/.test(message)) return i18n.t("workflow.errors.stale");
  if (/snapshot_unavailable/.test(message)) return i18n.t("workflow.errors.remoteVersionChanged");
  if (/invalid github repository url|only https:\/\/ github|only github\.com|repository url (must|is missing)|enter a github repository/.test(message)) return i18n.t("workflow.errors.invalidRepository");
  if (/no importable skills/.test(message)) return i18n.t("workflow.noImportableSkills");
  if (message.includes("preview changed: item category changed")) return i18n.t("workflow.errors.categoryChanged");
  if (message.includes("preview changed: remote content version changed")) return i18n.t("workflow.errors.remoteVersionChanged");
  if (/response body failed|download interrupted|response body incomplete|error decoding response body/.test(message)) {
    return i18n.t(/timeout|timed out/.test(message) ? "workflow.errors.timeout" : "workflow.errors.network");
  }
  const key = /rate.?limit|too many requests|\b429\b|secondary.*limit/.test(
    message,
  )
    ? kind === "ai"
      ? "aiRateLimit"
      : "rateLimit"
    : /preview changed/.test(message)
      ? "stale"
      : /timeout|timed out/.test(message)
        ? "timeout"
        : /401|403|unauthorized|forbidden|api.key/.test(message)
          ? kind === "ai"
            ? "aiAuthorization"
            : "githubAuthorization"
          : /network|connect|dns|fetch/.test(message)
            ? "network"
            : /not found|does not exist|no such file/.test(message)
              ? "missing"
              : /already exists|conflict/.test(message)
                ? "conflict"
                : "other";
  return i18n.t(`workflow.errors.${key}`);
}
interface State {
  tasks: BackgroundTask[];
  enqueue: (task: Input) => string;
  cancel: (id: string) => void;
  retry: (id: string) => void;
  remove: (id: string) => void;
  clearCheckHistory: () => void;
  clear: () => void;
  clearFinished: () => void;
  clearAll: () => void;
  patch: (id: string, patch: Partial<BackgroundTask>) => void;
}
export const useTaskQueueStore = create<State>()(
  persist(
    (set, get) => ({
      tasks: [],
      enqueue: (input) => {
        const duplicate = get().tasks.find(
          (t) => t.key === input.key && isTaskActive(t) && !t.cancelRequested,
        );
        if (duplicate) return duplicate.id;
        const id = crypto.randomUUID();
        set((s) => ({
          tasks: [
            ...s.tasks,
            {
              ...input,
              locks: input.locks ?? [],
              id,
              status: "queued",
              cancelRequested: false,
              createdAt: Date.now(),
            },
          ],
        }));
        pump();
        return id;
      },
      patch: (id, patch) =>
        set((s) => ({
          tasks: trimHistory(
            s.tasks.map((t) => (t.id === id ? { ...t, ...patch } : t)),
          ),
        })),
      cancel: (id) => {
        const task = get().tasks.find((t) => t.id === id);
        if (!task || !isTaskActive(task)) return;
        get().patch(id, {
          cancelRequested: true,
          ...(task.status === "queued" || !running.has(id) ? { status: "cancelled" as const,
            steps: task.steps.map(step => step.status === "running" ? {...step, status: "cancelled" as const} : step) } : {}),
        });
        const operationId = importOperations.get(id);
        if (operationId && !task.cancelRequested) {
          void invoke("cancel_github_operation", {operationId}).catch(() => {
            /* Older backends finish the current step; keep locks until it returns. */
          });
        }
      },
      retry: (id) => {
        const task = get().tasks.find((t) => t.id === id);
        if (
          !task ||
          isTaskActive(task) ||
          get().tasks.some(
            (t) => t.id !== id && t.key === task.key && isTaskActive(t),
          )
        )
          return;
        get().patch(id, {
          status: "queued",
          importProgress: undefined,
          cancelRequested: false,
          steps: task.steps.map((step) => {
            if (step.status === "success") return step;
            const failedRepos =
              step.command ===
              "preview_source_backed_resource_repository_updates"
                ? (
                    step.result as {
                      repositories?: { repository: string; error?: string }[];
                    }
                  )?.repositories
                    ?.filter((r) => r.error)
                    .map((r) => r.repository)
                : undefined;
            return {
              ...step,
              args: failedRepos?.length
                ? { ...step.args, repositories: failedRepos }
                : step.command === "prepare_github_resource_import" ? {...step.args, operationId: undefined} : step.args,
              status: "queued",
              error: undefined,
              result: undefined,
            };
          }),
        });
        pump();
      },
      remove: (id) => {
        const task = get().tasks.find(task => task.id === id);
        if (!task) return;
        if (isTaskActive(task)) {
          if (!task.cancelRequested) return;
          get().cancel(id);
          if (running.has(id)) {
            // Hide the record, but retain its lock and notify waiters at completion.
            get().patch(id, {clearRequested: true});
            return;
          }
        }
        set(s => ({tasks: s.tasks.filter(task => task.id !== id)}));
      },
      clearCheckHistory: () => {
        const isCheck = (task: BackgroundTask) => task.steps.length > 0 && task.steps.every(step => step.command === "preview_source_backed_resource_repository_updates");
        get().tasks.filter(task => task.status === "queued" && isCheck(task)).forEach(task => get().cancel(task.id));
        set(s => ({tasks: s.tasks.flatMap(task => {
          if (!isCheck(task)) return [task];
          // Superseded checks keep their locks until completion but no longer publish records.
          return isTaskActive(task) ? [{...task, clearRequested: true}] : [];
        })}));
      },
      clearFinished: () => set(s => {
        const activeBatches = new Set(s.tasks.filter(isTaskActive).map(task => task.batchId).filter(Boolean));
        return {tasks: s.tasks.filter(task => (task.batchId && activeBatches.has(task.batchId)) || (task.status !== "success" && task.status !== "cancelled"))};
      }),
      clear: () => set((s) => ({ tasks: s.tasks.filter(task => task.status !== "success") })),
      clearAll: () => {
        // Resolve queued task waiters as cancelled before removing their records.
        get().tasks.filter(isTaskActive).forEach(task => get().cancel(task.id));
        set(state => ({tasks: state.tasks.filter(task => task.status === "running")
          .map(task => ({...task, clearRequested: true}))}));
      },
    }),
    {
      name: "skillshub.background-tasks.v1",
      storage: createJSONStorage(() => localStorage),
      partialize: (s) => ({
        tasks: [
          ...s.tasks.filter(t => t.status !== "success" && !t.clearRequested),
          ...s.tasks.filter((t) => t.status === "success").slice(-100),
        ].map(task => ({...task, importProgress: undefined, steps: task.steps.map(step => {
          if (step.command !== "preview_source_backed_resource_repository_updates" || !step.result) return step;
          // Coverage is the whole library for every repository. The report store
          // persists it once; duplicating it in each checkpoint exhausts WebView storage.
          const {coverage: _coverage, ...result} = step.result as Record<string, unknown>;
          return {...step, result};
        })})),
      }),
      merge: (persisted, current) => {
        const tasks = (persisted as { tasks?: BackgroundTask[] })?.tasks ?? [];
        return {
          ...current,
          tasks: tasks.filter(t => !t.clearRequested).map((t) =>
            isTaskActive(t)
              ? {
                  ...t,
                  status: "interrupted" as const,
                  steps: t.steps.map((step) =>
                    step.status === "running"
                      ? { ...step, status: "interrupted" as const }
                      : step,
                  ),
                }
              : t,
          ),
        };
      },
    },
  ),
);
function pump() {
  const state = useTaskQueueStore.getState();
  for (const task of state.tasks.filter((t) => t.status === "queued")) {
    const active = useTaskQueueStore
      .getState()
      .tasks.filter((t) => running.has(t.id));
    if (
      active.filter((t) => t.kind === task.kind).length >= limits[task.kind] ||
      active.some((t) => t.locks.some((lock) => task.locks.some(other => other === lock || (lock === "repo:*" && other.startsWith("repo:")) || (other === "repo:*" && lock.startsWith("repo:")))))
    )
      continue;
    running.add(task.id);
    state.patch(task.id, { status: "running" });
    void execute(task.id);
  }
}
async function execute(id: string) {
  const state = useTaskQueueStore.getState;
  const task = state().tasks.find((t) => t.id === id)!;
  const steps = task.steps.map((s) => ({ ...s }));
  for (let index = 0; index < steps.length; index++) {
    if (state().tasks.find((t) => t.id === id)?.cancelRequested) break;
    const step = steps[index];
    if (step.status === "success") continue;
    step.status = "running";
    state().patch(id, { steps: steps.map((s) => ({ ...s })) });
    const cancellable = step.command === "import_github_repo_skills" || step.command === "prepare_github_resource_import";
    if (cancellable) importOperations.set(id, step.command === "prepare_github_resource_import" && typeof step.args.operationId === "string" ? step.args.operationId : crypto.randomUUID());
    let unlisten: (() => void) | undefined;
    let progressTimer: ReturnType<typeof setTimeout> | undefined;
    let latestProgress: GitHubImportProgressPayload | undefined;
    const flushProgress = () => {
      if (latestProgress && state().tasks.some(task => task.id === id && task.status === "running")) state().patch(id, {importProgress: latestProgress});
      latestProgress = undefined;
      progressTimer = undefined;
    };
    try {
      if (step.command === "import_github_repo_skills") {
        state().patch(id, {importProgress: undefined});
        const operationId = importOperations.get(id);
        try {
          unlisten = await listen<GitHubImportProgressPayload>("github-import:progress", ({payload}) => {
            if (payload.operationId !== operationId) return;
            latestProgress = payload;
            // File events can be frequent; cap queue/UI updates at ten per second.
            if (!progressTimer) progressTimer = setTimeout(flushProgress, 100);
          });
        } catch { /* Import still works when event registration is unavailable. */ }
      }
      const args = cancellable ? {...step.args, operationId: importOperations.get(id)} : step.args;
      const result = await (executors.get(step.command)?.(args) ?? invoke(step.command, args));
      step.result = result;
      step.status = inputRequirements.get(step.command)?.(result) ? "awaiting_input" : "success";
      step.error = undefined;
      if (
        step.command === "preview_source_backed_resource_repository_updates"
      ) {
        const repos =
          (result as { repositories?: { error?: string }[] })?.repositories ??
          [];
        if (repos.some((repo) => repo.error)) {
          step.status = "partial";
          step.error = i18n.t("workflow.checkPartial");
        }
      }
      try {
        await handlers.get(step.command)?.(step, result);
      } catch {
        /* The operation succeeded; a refresh failure must not rerun a committed write. */
      }
    } catch (error) {
      if (cancellable && state().tasks.find(task => task.id === id)?.cancelRequested) {
        step.status = "cancelled";
        step.error = undefined;
        state().patch(id, {steps: steps.map(step => ({...step}))});
        break;
      }
      step.status = "failed";
      // Provider responses and credentials must never be surfaced in the task UI.
      step.error = taskErrorMessage(error, task.kind);
      try {
        await failureHandlers.get(step.command)?.(step);
      } catch {
        /* Keep the original operation failure if a status observer fails. */
      }
    } finally {
      if (progressTimer) clearTimeout(progressTimer);
      flushProgress();
      try { void Promise.resolve(unlisten?.()).catch(() => {}); } catch { /* The event bridge may already be closed. */ }
    }
    importOperations.delete(id);
    state().patch(id, { steps: steps.map((s) => ({ ...s })) });
    if (step.status === "awaiting_input") break;
  }
  const cancelled = state().tasks.find((t) => t.id === id)?.cancelRequested;
  const failures = steps.some(
    (s) => s.status === "failed" || s.status === "partial",
  );
  state().patch(id, {
    status: cancelled
      ? "cancelled"
      : steps.some(step => step.status === "awaiting_input")
        ? "awaiting_input"
      : failures
        ? steps.some((s) => s.status === "success" || s.status === "partial")
          ? "partial"
          : "failed"
        : "success",
  });
  running.delete(id);
  importOperations.delete(id);
  const finished = state().tasks.find(task => task.id === id);
  if (finished) {
    for (const command of new Set(finished.steps.map(step => step.command))) {
      const complete = completionHandlers.get(command);
      if (!complete) continue;
      try {
        await complete(finished);
      } catch {
        /* A refresh failure must not retry a completed write. */
      }
    }
  }
  if (state().tasks.find(t => t.id === id)?.clearRequested) {
    useTaskQueueStore.setState(s => ({tasks: s.tasks.filter(t => t.id !== id)}));
  }
  pump();
}
export function waitForTask(id: string): Promise<BackgroundTask> {
  return new Promise((resolve) => {
    const current = useTaskQueueStore.getState().tasks.find((t) => t.id === id);
    if (current && !isTaskActive(current)) {
      resolve(current);
      return;
    }
    const unsubscribe = useTaskQueueStore.subscribe((state) => {
      const task = state.tasks.find((t) => t.id === id);
      if (task && !isTaskActive(task)) {
        unsubscribe();
        resolve(task);
      }
    });
  });
}
