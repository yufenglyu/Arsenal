import { ActionIcon } from "@/components/ui/action-icon";
import { useTranslation } from "react-i18next";
import { Check } from "lucide-react";
import { Button } from "@/components/ui/button";
import { importDetailRows } from "@/lib/taskPresentation";
import {
  retryableTask,
  type TaskBatch,
} from "@/lib/taskBatches";
import { isTaskActive, taskErrorMessage, useTaskQueueStore } from "@/stores/taskQueueStore";
import { TaskDisclosure } from "./TaskDisclosure";

export function TaskBatchDetails({ batch }: { batch: TaskBatch }) {
  const { t } = useTranslation();
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
  const settled = (status: string) =>
    ["success", "imported", "skipped"].includes(status);
  const unfinished = rows.filter((row) => !settled(row.status));
  const completed = rows.filter((row) => settled(row.status));
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
            className={`text-xs ${row.error ? "text-destructive" : "text-muted-foreground"}`}
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
          {owner && isTaskActive(owner) && <Button variant="ghost" size="icon" title={t(owner.cancelRequested ? "workflow.stopping" : "workflow.stopTask")} aria-label={t(owner.cancelRequested ? "workflow.stopping" : "workflow.stopTask")} disabled={owner.cancelRequested} onClick={() => useTaskQueueStore.getState().cancel(owner.id)}><ActionIcon action="stop" /></Button>}
          {owner && batch.tasks.length > 1 && retryableTask(owner) && (
            <Button
              variant="ghost"
              size="icon"
              title={t("workflow.retryTask")}
              aria-label={t("workflow.taskPanel.retryTaskNamed", {
                name: owner.label,
              })}
              onClick={() => useTaskQueueStore.getState().retry(owner.id)}
            >
              <ActionIcon action="retry" />
            </Button>
          )}
        </div>
      );
    });
  const skipped = completed.filter((row) => row.status === "skipped").length;
  return (
    <div>
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
          <span className="ml-auto text-xs text-muted-foreground">
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
              variant="ghost"
              size="icon"
              title={t("workflow.retryTask")}
              aria-label={t("workflow.retryTask")}
              onClick={() => batch.legacy?.onRetryFailedItem?.(item)}
            >
              <ActionIcon action="retry" />
            </Button>
          )}
        </div>
      ))}

    </div>
  );
}
