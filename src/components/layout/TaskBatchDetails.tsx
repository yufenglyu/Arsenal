import { useState } from "react";
import { useTranslation } from "react-i18next";
import { Check, RotateCcw } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { importDetailRows } from "@/lib/taskPresentation";
import {
  CHECK_REPOSITORIES,
  retryableTask,
  type TaskBatch,
} from "@/lib/taskBatches";
import { taskErrorMessage, useTaskQueueStore } from "@/stores/taskQueueStore";
import { TaskDisclosure } from "./TaskDisclosure";

export function TaskBatchDetails({ batch }: { batch: TaskBatch }) {
  const { t, i18n } = useTranslation();
  const [search, setSearch] = useState("");
  const rows =
    batch.kind === "import"
      ? importDetailRows(batch.tasks)
      : batch.tasks.flatMap((task) =>
          task.steps.map((step, index) => ({
            key: `${task.id}:${index}`,
            name: step.label,
            status: step.status ?? task.status,
            error: step.error,
          })),
        );
  const matches = (name: string) =>
    name.toLocaleLowerCase().includes(search.trim().toLocaleLowerCase());
  const settled = (status: string) =>
    ["success", "imported", "skipped"].includes(status);
  const visible = rows.filter((row) => matches(row.name));
  const unfinished = visible.filter((row) => !settled(row.status));
  const completed = visible.filter((row) => settled(row.status));
  const renderRows = (items: typeof rows) =>
    items.map((row) => {
      const owner = batch.tasks.find((task) => row.key === `${task.id}:0`);
      return (
        <div
          key={row.key}
          className="flex flex-wrap items-center justify-between gap-2 border-b border-border py-3 text-sm"
        >
          <span className="min-w-0 flex-1 break-all">{row.name}</span>
          <span
            className={row.error ? "text-destructive" : "text-muted-foreground"}
          >
            {row.error
              ? taskErrorMessage(
                  row.error,
                  batch.kind === "refresh" ? "check" : batch.kind,
                )
              : t(
                  row.status === "imported"
                    ? "status.importedLabel"
                    : row.status === "skipped"
                      ? "status.skippedLabel"
                      : row.status === "ready"
                        ? "workflow.taskPanel.readyToImport"
                        : `workflow.taskStatus.${row.status}`,
                )}
          </span>
          {owner && batch.tasks.length > 1 && retryableTask(owner) && (
            <Button
              variant="ghost"
              aria-label={t("workflow.taskPanel.retryTaskNamed", {
                name: owner.label,
              })}
              onClick={() => useTaskQueueStore.getState().retry(owner.id)}
            >
              <RotateCcw />
              {t("workflow.retryTask")}
            </Button>
          )}
        </div>
      );
    });
  const skipped = completed.filter((row) => row.status === "skipped").length;
  return (
    <div>
      {rows.length > 8 && (
        <Input
          type="search"
          className="my-3"
          aria-label={t("workflow.batch.searchItems")}
          placeholder={t("workflow.batch.searchItems")}
          value={search}
          onChange={(event) => setSearch(event.target.value)}
        />
      )}
      {renderRows(unfinished)}
      {!!completed.length && (
        <TaskDisclosure
          title={
            <>
              <Check />
              {t(
                skipped
                  ? "workflow.batch.settled"
                  : "workflow.batch.completedCount",
                {
                  count: completed.length,
                  done: completed.length - skipped,
                  skipped,
                },
              )}
            </>
          }
        >
          {renderRows(completed)}
        </TaskDisclosure>
      )}
      {batch.legacy?.items?.map((item, index) => (
        <div
          key={index}
          className="flex flex-wrap items-center gap-2 border-b border-border py-3 text-sm"
        >
          <span className="break-all">{item.name}</span>
          {item.repository && (
            <span className="break-all text-muted-foreground">
              {item.repository}
            </span>
          )}
          <span className="ml-auto text-muted-foreground">
            {t(
              item.status === "failed"
                ? "workflow.taskStatus.failed"
                : item.status === "updated"
                  ? "workflow.updated"
                  : "status.skippedLabel",
            )}
          </span>
          {item.status === "failed" && batch.legacy?.onRetryFailedItem && (
            <Button
              variant="outline"
              onClick={() => batch.legacy?.onRetryFailedItem?.(item)}
            >
              {t("workflow.retryTask")}
            </Button>
          )}
        </div>
      ))}
      <TaskDisclosure small title={t("workflow.batch.log")}>
        <p>
          {batch.createdAt
            ? new Date(batch.createdAt).toLocaleString(i18n.language)
            : t("workflow.checkTimeUnknown")}
        </p>
        {batch.tasks.flatMap((task) =>
          task.importProgress
            ? [
                <div key={task.id} className="mt-3 space-y-2">
                  {task.importProgress.currentSkill && (
                    <p className="break-all">
                      {t("workflow.taskPanel.currentSkill", {
                        name: task.importProgress.currentSkill,
                      })}
                    </p>
                  )}
                  {task.importProgress.currentPath && (
                    <p className="break-all">
                      {t("workflow.taskPanel.currentFile", {
                        path: task.importProgress.currentPath,
                      })}
                    </p>
                  )}
                </div>,
              ]
            : [],
        )}
        {batch.tasks.flatMap((task) =>
          task.steps
            .filter((step) => step.command === CHECK_REPOSITORIES)
            .flatMap((step, index) => {
              const result = step.result as
                | { repositories?: { repository?: string; error?: string }[] }
                | undefined;
              return Array.isArray(result?.repositories)
                ? result.repositories
                    .filter((repo) => typeof repo.repository === "string")
                    .map((repo) => (
                      <p
                        className="mt-2 break-all"
                        key={`${task.id}:${index}:${repo.repository}`}
                      >
                        {repo.repository} ·{" "}
                        {repo.error
                          ? taskErrorMessage(repo.error)
                          : t("workflow.taskStatus.success")}
                      </p>
                    ))
                : [];
            }),
        )}
      </TaskDisclosure>
    </div>
  );
}
