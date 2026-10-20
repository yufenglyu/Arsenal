import { ActionIcon } from "@/components/ui/action-icon";
import {repositoryRisk, selectedRepositories, useTaskReviewStore} from "@/stores/taskReviewStore";
import { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import {
  ChevronRight,
  CircleAlert,
  Download,
  FolderSync,
  ListTodo,
  RefreshCw,
  RotateCcw,
} from "lucide-react";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogTitle,
  DialogDescription,
} from "@/components/ui/dialog";
import { HintIcon } from "@/components/ui/hint-icon";
import { useTaskQueueStore, isTaskActive } from "@/stores/taskQueueStore";
import { useRepositorySyncStore } from "@/stores/repositorySyncStore";
import { useAppStatusStore } from "@/stores/appStatusStore";
import {
  PREPARE_GITHUB_IMPORT,
  preparedImportPreview,
} from "@/stores/importPreparationStore";
import {
  buildTaskBatches,
  CHECK_REPOSITORIES,
  retryableTask,
  type TaskBatch,
} from "@/lib/taskBatches";
import { importDetailRows } from "@/lib/taskPresentation";
import { cn } from "@/lib/utils";
import { TaskImportReview } from "./TaskImportReview";
import { TaskRepositoryReview } from "./TaskRepositoryReview";
import { TaskBatchDetails } from "./TaskBatchDetails";
import { useTaskCloseGuard } from "./useTaskCloseGuard";

const activeStatus = (status: string) =>
  ["running", "queued", "stopping"].includes(status);
const failedStatus = (status: string) =>
  ["failed", "partial", "interrupted"].includes(status);

export function TaskCenter() {
  const { t } = useTranslation();
  const state = useRepositorySyncStore();
  const tasks = useTaskQueueStore((s) => s.tasks);
  const legacy = useAppStatusStore((s) => s.task);
  const batches = buildTaskBatches(tasks, state, legacy);
  const [tab, setTab] = useState<"current" | "history">("current");
  const [expanded, setExpanded] = useState<Set<string>>(new Set());
  const [archived, setArchived] = useState<Set<string>>(() => new Set(batches.filter(batch => batch.status === "success").map(batch => batch.id)));
  const selection = useTaskReviewStore();
  const [submitError, setSubmitError] = useState(false);
  const reportRows = batches.find(batch => batch.report !== undefined)?.report ?? [];
  const choices = selectedRepositories(reportRows, tasks, state.migratingRepository, selection);
  const riskCount = reportRows.filter(row => row.pending && !row.needsCheck && repositoryRisk(row) && !selection.approved.includes(row.preview?.snapshotId ?? "")).length;
  const recheckCount = reportRows.filter(row => row.needsCheck).length;
  const collapse = (id: string) => setExpanded(previous => { const next = new Set(previous); next.delete(id); return next; });
  const [visited, setVisited] = useState<Set<string>>(new Set());
  const wasOpen = useRef(false);
  useTaskCloseGuard();
  const expand = (id: string | null) => {
    setExpanded(previous => id ? new Set(previous).add(id) : new Set());
    if (id) setVisited((previous) => new Set(previous).add(id));
  };
  const isHistory = (batch: TaskBatch) =>
    archived.has(batch.id) && batch.status === "success";
  const counts = {
    running: batches.filter((batch) => activeStatus(batch.status)).length,
    pending: batches.filter(
      (batch) => !activeStatus(batch.status) && batch.status !== "success",
    ).length,
  };
  const reportId = batches.find((batch) => batch.report)?.id;
  const currentCount = batches.filter((batch) => !isHistory(batch)).length;
  const historyCount = batches.length - currentCount;
  useEffect(() => {
    if (!state.open) {
      wasOpen.current = false;
      return;
    }
    if (wasOpen.current) return;
    wasOpen.current = true;
    if (
      state.centerView === "history" &&
      legacy?.kind &&
      legacy.status !== "idle"
    ) {
      const id = `status:${legacy.id}`;
      setTab(legacy.status === "success" ? "history" : "current");
      setExpanded(previous => id ? new Set(previous).add(id) : new Set());
      setVisited((previous) => new Set(previous).add(id));
      return;
    }
    setTab(
      state.centerView === "history" && currentCount === 0
        ? "history"
        : "current",
    );
    if (state.centerView === "pending" && reportId) {
      setExpanded(previous => new Set(previous).add(reportId));
      setVisited((previous) => new Set(previous).add(reportId));
    }
  }, [state.open, state.centerView, reportId, currentCount, legacy]);
  const title = (batch: TaskBatch) =>
    batch.report !== undefined
      ? t(
          batch.scope?.mode === "all"
            ? "workflow.batch.updateLibrary"
            : "workflow.batch.updateSelected",
        )
      : batch.kind === "refresh"
        ? t(
            batch.repository
              ? "workflow.batch.refreshSingle"
              : "workflow.refreshRepositoryInfo",
          )
        : batch.kind === "import"
          ? t("workflow.batch.importTitle")
          : batch.label;
  const scope = (batch: TaskBatch) =>
    batch.scope && (batch.report !== undefined || !batch.repository)
      ? t(
          batch.scope.mode === "all"
            ? "workflow.batch.scopeAll"
            : "workflow.batch.scopeSelected",
          {
            count:
              batch.scope.repositories.length ||
              batch.report?.length ||
              batch.tasks.length,
          },
        )
      : batch.kind === "refresh" && !batch.repository
        ? t("workflow.batch.scopeCount", { count: batch.tasks.length })
        : null;
  const progress = (batch: TaskBatch) => {
    if (batch.report?.length)
      return {
        done: batch.report.filter((row) => row.status === "success").length,
        total: Math.max(
          batch.scope?.repositories.length ?? 0,
          batch.report.length,
        ),
        determinate: true,
      };
    const file = batch.tasks.find(
      (task) =>
        isTaskActive(task) &&
        task.importProgress?.phase === "writing" &&
        task.importProgress.totalFiles > 0,
    )?.importProgress;
    const steps = batch.tasks
      .flatMap((task) => task.steps)
      .filter(
        (step) =>
          !batch.tasks.some((task) =>
            task.steps.some(
              (item) => item.command === "import_github_repo_skills",
            ),
          ) || step.command !== PREPARE_GITHUB_IMPORT,
      );
    return file
      ? { done: file.completedFiles, total: file.totalFiles, determinate: true }
      : {
          done: steps.filter((step) => step.status === "success").length,
          total: steps.length,
          determinate:
            steps.length > 1 || steps.some((step) => step.status === "success"),
        };
  };
  const summary = (batch: TaskBatch) => {
    const { done, total } = progress(batch);
    if (batch.status === "stopping") return t("workflow.stopping");
    if (activeStatus(batch.status)) {
      if (batch.status === "queued") return t("workflow.taskStatus.queued");
      const checking =
        batch.report !== undefined &&
        !batch.tasks.some(
          (task) => isTaskActive(task) && task.kind === "update",
        );
      const importing = batch.tasks.find(
        (task) => task.importProgress,
      )?.importProgress;
      const label = checking
        ? t("workflow.batch.checking")
        : batch.report !== undefined
          ? t("workflow.batch.syncing")
          : batch.kind === "refresh"
            ? t("workflow.batch.refreshing")
            : importing
              ? t(`workflow.taskPanel.importStage.${importing.phase}`)
              : t("workflow.taskStatus.running");
      return total > 1 ? `${label} · ${done} / ${total}` : label;
    }
    if (batch.status === "cancelled") return t("workflow.batch.stopped");
    if (batch.report) {
      const failed = batch.report.filter(
        (row) => row.error || failedStatus(row.status),
      ).length;
      if (failed || batch.reportError)
        return t("workflow.batch.failedCount", { count: failed || 1 });
      const pending = batch.report.filter(
        (row) => row.pending || row.needsCheck,
      ).length;
      if (pending) return t("workflow.batch.reviewCount", { count: pending });
      return t("workflow.batch.updateResult", {
        updated: batch.report.filter((row) => row.applied).length,
        unchanged: batch.report.filter((row) => !row.applied).length,
      });
    }
    if (failedStatus(batch.status))
      return t("workflow.batch.failedCount", {
        count: batch.tasks.filter(retryableTask).length || 1,
      });
    if (batch.status === "awaiting_input") {
      const task = batch.tasks.find((task) => task.status === "awaiting_input");
      return t("workflow.batch.importReady", {
        count: task ? (preparedImportPreview(task.id)?.skills.length ?? 0) : 0,
      });
    }
    const rows = batch.kind === "import" ? importDetailRows(batch.tasks) : [];
    const skipped = rows.filter((row) => row.status === "skipped").length;
    return skipped
      ? t("workflow.batch.settled", { done: rows.length - skipped, skipped })
      : t("workflow.taskStatus.success");
  };
  const retry = (batch: TaskBatch) => {
    if (batch.report) {
      const resumable = batch.tasks.filter(
        (task) =>
          ["cancelled", "interrupted"].includes(task.status) &&
          task.steps.some((step) => step.command === CHECK_REPOSITORIES),
      );
      if (resumable.length)
        resumable.forEach((task) =>
          useTaskQueueStore.getState().retry(task.id),
        );
      else if (batch.reportError)
        void state.checkForUpdates(state.repositories ?? undefined);
      else {
        const repos = batch.report
          .filter((row) => row.needsCheck)
          .map((row) => row.repository);
        if (repos.length) void state.recheckRepositories(repos);
        else expand(batch.id);
      }
    } else
      batch.tasks
        .filter(retryableTask)
        .forEach((task) => useTaskQueueStore.getState().retry(task.id));
  };
  const action = (batch: TaskBatch) => {
    if (batch.report !== undefined && !activeStatus(batch.status)) return null;
    const active = batch.tasks.filter(isTaskActive);
    if (activeStatus(batch.status))
      return (
        <Button
          variant="ghost"
          disabled={!active.length || batch.status === "stopping"}
          onClick={() =>
            active.forEach((task) =>
              useTaskQueueStore.getState().cancel(task.id),
            )
          }
        >
          <ActionIcon action="stop" />
          {t("workflow.stopTask")}
        </Button>
      );
    if (batch.status === "awaiting_input")
      return (
        <Button
          variant="ghost"
          aria-expanded={expanded.has(batch.id)}
          onClick={() => expanded.has(batch.id) ? collapse(batch.id) : expand(batch.id)}
        >
          {t(
            expanded.has(batch.id)
              ? "workflow.batch.collapse"
              : batch.kind === "import"
                ? "workflow.batch.chooseSkills"
                : "workflow.batch.review",
          )}
          <ChevronRight className={cn(expanded.has(batch.id) && "rotate-90")} />
        </Button>
      );
    if (failedStatus(batch.status) || batch.status === "cancelled") {
      const syncFailed =
        !batch.report?.some((row) => row.needsCheck) &&
        batch.report?.some((row) => row.error && !row.needsCheck);
      return (
        <Button
          variant="outline"
          title={t("workflow.batch.retryHint")}
          onClick={() => retry(batch)}
        >
          <RotateCcw />
          {t(
            syncFailed
              ? "workflow.batch.review"
              : batch.status === "cancelled"
                ? "workflow.batch.resume"
                : "workflow.batch.retryFailed",
          )}
        </Button>
      );
    }
    return null;
  };
  const clearHistory = () => {
    for (const batch of batches.filter(isHistory)) {
      if (batch.report !== undefined) state.clearCheckResults();
      else if (batch.legacy) useAppStatusStore.getState().resetStatus();
      else
        batch.tasks.forEach((task) =>
          useTaskQueueStore.getState().remove(task.id),
        );
    }
  };
  const changeTab = (value: typeof tab) => {
    setTab(value);

  };
  const visibleCount = tab === "current" ? currentCount : historyCount;
  return (
    <>
      <Button size="status" variant="ghost" onClick={() => state.setOpen(true)}>
        <ListTodo />
        {t("workflow.activityTitle")} · {t("workflow.activitySummary", counts)}
      </Button>
      <Dialog open={state.open} onOpenChange={state.setOpen}>
        <DialogContent
          keepMounted
          className="flex flex-col gap-0 overflow-hidden p-0"
          style={{
            width: "min(67.5rem, calc(100vw - 2rem))",
            height: "min(45rem, calc(100dvh - 2rem))",
            maxWidth: "calc(100vw - 2rem)",
          }}
        >
          <DialogDescription className="sr-only">
            {t("workflow.taskPanel.backgroundHint")}
          </DialogDescription>
          <header className="flex min-h-16 shrink-0 flex-wrap items-center gap-4 border-b border-border p-4 pr-12">
            <DialogTitle className="flex items-center gap-2">
              <ListTodo className="size-[1em]" />
              {t("workflow.activityTitle")}
            </DialogTitle>
            <div
              role="tablist"
              aria-label={t("workflow.activityTitle")}
              className="flex items-center gap-4"
            >
              {(["current", "history"] as const).map((value) => (
                <button
                  key={value}
                  type="button"
                  role="tab"
                  id={`batch-tab-${value}`}
                  aria-selected={tab === value}
                  tabIndex={tab === value ? 0 : -1}
                  aria-controls={`batch-panel-${value}`}
                  onClick={() => changeTab(value)}
                  onKeyDown={(event) => {
                    if (
                      ["ArrowLeft", "ArrowRight", "Home", "End"].includes(
                        event.key,
                      )
                    ) {
                      event.preventDefault();
                      const next =
                        event.key === "Home"
                          ? "current"
                          : event.key === "End"
                            ? "history"
                            : value === "current"
                              ? "history"
                              : "current";
                      changeTab(next);
                      document.getElementById(`batch-tab-${next}`)?.focus();
                    }
                  }}
                  className={cn(
                    "inline-flex items-center gap-2 rounded-sm text-sm font-semibold focus-visible:outline-2 focus-visible:outline-ring",
                    tab === value
                      ? "text-primary"
                      : "text-muted-foreground",
                  )}
                >
                  {t(`workflow.batch.${value}`)}
                  <span className="text-xs font-normal">
                    {value === "current" ? currentCount : historyCount}
                  </span>
                </button>
              ))}
            </div>

          </header>
          <div
            role="tabpanel"
            id={`batch-panel-${tab}`}
            aria-labelledby={`batch-tab-${tab}`}
            className="min-h-0 flex-1 overflow-y-auto [scrollbar-gutter:stable]"
          >
            {batches.map((batch) => {
              const open = expanded.has(batch.id);
              const hidden =
                tab === "history" ? !isHistory(batch) : isHistory(batch);
              const prepared = batch.tasks.find(
                (task) =>
                  task.status === "awaiting_input" &&
                  task.steps.some(
                    (step) => step.command === PREPARE_GITHUB_IMPORT,
                  ),
              );
              const p = progress(batch);
              const Icon = failedStatus(batch.status)
                ? CircleAlert
                : batch.kind === "import"
                  ? Download
                  : batch.kind === "refresh"
                    ? FolderSync
                    : RefreshCw;
              const batchTitle = title(batch);
              return (
                <article
                  key={batch.id}
                  hidden={hidden}
                  aria-label={[batchTitle, batch.repository]
                    .filter(Boolean)
                    .join(" ")}
                  className={cn(
                    "border-b border-border",
                    open && "border-l-2 border-l-primary",
                  )}
                >
                  <div
                    className={cn(
                      "flex items-center gap-2 px-4 py-3",
                      open && "bg-accent pl-3.5",
                    )}
                  >
                    <button
                      type="button"
                      aria-expanded={open}
                      aria-controls={`batch-details-${batch.id}`}
                      onClick={() => open ? collapse(batch.id) : expand(batch.id)}
                      className="flex min-w-0 flex-1 items-center gap-2 rounded-sm text-left text-sm focus-visible:outline-2 focus-visible:outline-ring"
                    >
                      <ChevronRight
                        className={cn(
                          "size-[1em] shrink-0 text-muted-foreground transition-transform",
                          open && "rotate-90",
                        )}
                      />
                      <Icon
                        className={cn(
                          "size-[1em] shrink-0",
                          failedStatus(batch.status) && "text-destructive",
                        )}
                      />
                      <span className="min-w-0">
                        <span className="flex flex-wrap items-center gap-x-2 gap-y-1">
                          <span
                            className="font-semibold"
                            title={
                              batch.kind === "refresh"
                                ? t("workflow.refreshRepositoryInfoHint")
                                : undefined
                            }
                          >
                            {batchTitle}
                          </span>
                          {batch.repository && (
                            <span
                              className="max-w-full truncate"
                              title={batch.repository}
                            >
                              {batch.repository}
                            </span>
                          )}
                          {scope(batch) && (
                            <span className="text-xs text-muted-foreground">
                              {scope(batch)}
                            </span>
                          )}
                        </span>
                        <span
                          className={cn(
                            "mt-1 block text-xs",
                            failedStatus(batch.status)
                              ? "text-destructive"
                              : "text-muted-foreground",
                          )}
                        >
                          {summary(batch)}
                        </span>
                      </span>
                    </button>
                    <div className="shrink-0">{action(batch)}</div>
                  </div>
                  {batch.status === "running" && (
                    <div
                      role="progressbar"
                      aria-label={batchTitle}
                      aria-valuemin={0}
                      aria-valuemax={Math.max(1, p.total)}
                      aria-valuenow={p.determinate ? p.done : undefined}
                      className="ml-14 mr-4 h-0.5 overflow-hidden bg-muted"
                    >
                      <div
                        className={cn(
                          "h-full bg-primary",
                          !p.determinate && "animate-pulse",
                        )}
                        style={{
                          width: p.determinate
                            ? `${Math.min(1, p.done / Math.max(1, p.total)) * 100}%`
                            : "33%",
                        }}
                      />
                    </div>
                  )}
                  <div
                    id={`batch-details-${batch.id}`}
                    hidden={!open}
                    className="border-t border-border px-4 pb-3 sm:pl-14"
                  >
                    {(open || visited.has(batch.id)) &&
                      (batch.report !== undefined ? (
                        <TaskRepositoryReview
                          key={state.reportGeneration ?? "legacy"}
                          batch={batch}
                          onLater={() => collapse(batch.id)}
                        />
                      ) : prepared ? (
                        <TaskImportReview
                          taskId={prepared.id}
                          onLater={() => collapse(batch.id)}
                        />
                      ) : (
                        <TaskBatchDetails batch={batch} />
                      ))}
                    {open &&
                      !activeStatus(batch.status) &&
                      batch.report === undefined &&
                      batch.status !== "awaiting_input" && (
                        <div className="flex justify-end">
                          <Button
                            variant="ghost"
                            aria-label={t("workflow.deleteTaskRecord", {
                              name: batchTitle,
                            })}
                            onClick={() => {
                              batch.tasks.forEach((task) =>
                                useTaskQueueStore.getState().remove(task.id),
                              );
                              if (batch.legacy)
                                useAppStatusStore.getState().resetStatus();
                            }}
                          >
                            <ActionIcon action="delete" />{t("workflow.deleteRecord")}
                          </Button>
                        </div>
                      )}
                  </div>
                </article>
              );
            })}
            {!visibleCount && (
              <p className="p-4 py-12 text-center text-sm text-muted-foreground">
                {t(
                  tab === "current"
                    ? "workflow.noTasks"
                    : "workflow.batch.noHistory",
                )}
              </p>
            )}
          </div>
          <footer className="flex shrink-0 flex-wrap items-center gap-4 border-t border-border p-4">
            <HintIcon text={t("workflow.taskPanel.backgroundHint")} />
            <div className="min-w-0 flex-1">
              <p className="text-sm font-semibold">{tab === "history" ? t("workflow.batch.history") : t("workflow.flow.selected", {count: choices.length})}</p>
              <p className="text-xs text-muted-foreground">{tab === "history" ? t("workflow.taskPanel.backgroundHint") : t("workflow.flow.remaining", {risk: riskCount, check: recheckCount})}</p>
              {submitError && <p role="alert" className="text-xs text-destructive">{t("workflow.batch.previewUnavailable")}</p>}
            </div>
            {tab === "history" && <Button variant="outline" disabled={!historyCount} onClick={clearHistory}><ActionIcon action="clear" />{t("workflow.clearFinished")}</Button>}
            {tab === "current" && <><Button variant="outline" disabled={!batches.some(batch => !isHistory(batch) && batch.status === "success")} onClick={() => setArchived(previous => new Set([...previous, ...batches.filter(batch => batch.status === "success").map(batch => batch.id)]))}><ActionIcon action="archive" />{t("workflow.flow.archive")}</Button>
            <Button disabled={!choices.length} onClick={() => {
              const count = state.applySnapshots(choices.map(row => ({repository: row.repository, snapshotId: row.preview!.snapshotId!, allowDeletions: !!selection.approved.includes(row.preview!.snapshotId!)})), state.reportGeneration);
              setSubmitError(count === 0);
            }}><ActionIcon action="update" />{t("workflow.flow.sync", {count: choices.length})}</Button></>}
          </footer>
        </DialogContent>
      </Dialog>
    </>
  );
}
