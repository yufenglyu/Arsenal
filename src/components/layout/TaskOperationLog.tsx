import { useTranslation } from "react-i18next";
import { CHECK_REPOSITORIES, type TaskBatch } from "@/lib/taskBatches";
import { importDetailRows } from "@/lib/taskPresentation";
import { taskErrorMessage } from "@/stores/taskQueueStore";
import type { RepositorySyncPreview } from "@/types";

// The persisted task queue is the shared operation record. Read only documented
// presentation fields; never serialize arbitrary command arguments or results.
export function TaskOperationLog({ batches }: { batches: TaskBatch[] }) {
  const { t, i18n } = useTranslation();
  const tasks = [...new Map(batches.flatMap(batch => batch.tasks).map(task => [task.id, task])).values()]
    .sort((a, b) => b.createdAt - a.createdAt);
  const checkedRepositories = new Set<string>();
  const time = (value: number) => new Date(value).toLocaleString(i18n.language);
  const statusText = (status: string, error?: string | null) => error ? taskErrorMessage(error)
    : t(status === "imported" ? "status.importedLabel" : status === "skipped" ? "status.skippedLabel"
      : status === "ready" ? "workflow.taskPanel.readyToImport" : `workflow.taskStatus.${status}`);
  const summary = (repo: RepositorySyncPreview) => t("workflow.flow.checkSummary", {
    added: repo.added?.length ?? 0, modified: repo.modified?.length ?? 0,
    deleted: repo.deleted?.length ?? 0, unchanged: repo.unchanged?.length ?? 0,
    documents: repo.documentChanges?.length ?? 0,
  });
  const entries = tasks.flatMap(task => {
    const progress = task.importProgress;
    const warnings = task.steps.flatMap((step, index) => step.warning ? [<p role="alert" key={`${task.id}:${index}:warning`} className="break-all text-destructive">{task.label} · {step.warning}</p>] : []);
    if (task.kind === "import") return [
      <div key={task.id} className="space-y-2 border-b border-border pb-3">
        <p className="break-all"><span className="text-muted-foreground">{time(task.createdAt)} · </span>{task.label} · {t("workflow.batch.importTitle")} · {statusText(task.status, task.steps.find(step => step.error)?.error)}</p>
        {progress?.currentSkill && <p className="break-all text-muted-foreground">{t("workflow.taskPanel.currentSkill", {name: progress.currentSkill})}</p>}
        {progress?.currentPath && <p className="break-all text-muted-foreground">{t("workflow.taskPanel.currentFile", {path: progress.currentPath})}</p>}
        {importDetailRows([task]).map(row => <p key={row.key} className="break-all">{row.name} · {statusText(row.status, row.error)}</p>)}
        {warnings}
      </div>,
    ];
    const taskEntries = task.steps.flatMap((step, index) => {
      const status = step.status ?? task.status;
      if (step.command !== CHECK_REPOSITORIES) return [
        <p key={`${task.id}:${index}`} className="break-all">
          <span className="text-muted-foreground">{time(task.createdAt)} · </span>
          {step.label} · {t(step.command === "apply_repository_snapshot" || step.command === "apply_repository_update_item"
            ? "workflow.flow.syncAction" : "workflow.refreshRepositoryInfo")} · {statusText(status, step.error)}
        </p>,
      ];
      const result = step.result as {repositories?: RepositorySyncPreview[]} | undefined;
      const results = Array.isArray(result?.repositories) ? result.repositories : [];
      const scope = Array.isArray(step.args.repositories) ? step.args.repositories : step.args.plannedRepositories;
      const names = [...new Map([
        ...(Array.isArray(scope) ? scope.filter((name): name is string => typeof name === "string") : []),
        ...results.flatMap(repo => typeof repo.repository === "string" ? [repo.repository] : []),
      ].map(name => [name.toLowerCase(), name])).values()];
      if (!names.length) return [<p key={`${task.id}:${index}`} className="break-all"><span className="text-muted-foreground">{time(task.createdAt)} · </span>{t("workflow.flow.checkAction")} · {statusText(status, step.error)}</p>];
      return names.map(name => {
        checkedRepositories.add(name.toLowerCase());
        const checked = results.find(repo => repo.repository.toLowerCase() === name.toLowerCase());
        const error = checked ? checked.error : step.error;
        const settled = !["queued", "running"].includes(status);
        const outcome = checked && settled ? checked.error ? "failed" : "success" : status;
        return <p key={`${task.id}:${index}:${name}`} className="break-all">
          <span className="text-muted-foreground">{time(task.createdAt)} · </span>
          {name} · {t("workflow.flow.checkAction")} · {error ? taskErrorMessage(error) : t(outcome === "running" ? "workflow.batch.checking" : `workflow.taskStatus.${outcome}`)}
          {checked && !error && <> · {summary(checked)}</>}
        </p>;
      });
    });
    return [...taskEntries, ...warnings];
  });
  // Older persisted previews may predate queue records. Keep their safe summary
  // visible without duplicating repositories already represented by check steps.
  for (const batch of batches) {
    for (const row of batch.report ?? []) {
      if (checkedRepositories.has(row.repository.toLowerCase())) continue;
      entries.push(<p key={`preview:${row.repository}`} className="break-all">
        <span className="text-muted-foreground">{time(batch.createdAt)} · </span>{row.repository} · {t("workflow.flow.checkAction")} · {statusText(row.status, row.error)}
        {row.preview && !row.error && <> · {summary(row.preview)}</>}
      </p>);
    }
    if (batch.legacy) entries.push(<div key={batch.id} className="space-y-2">
      <p>{batch.label} · {statusText(batch.legacy.status === "error" ? "failed" : batch.legacy.status, batch.legacy.error)}</p>
      {batch.legacy.items?.map((item, index) => <p key={index} className="break-all">{item.name} · {item.repository} · {t(item.status === "failed" ? "workflow.taskStatus.failed" : item.status === "updated" ? "workflow.updated" : "status.skippedLabel")}</p>)}
    </div>);
  }
  return <section aria-label={t("workflow.batch.log")} className="space-y-3 p-4 text-xs">
    {entries.length ? entries : <p className="text-muted-foreground">{t("workflow.batch.noLogs")}</p>}
  </section>;
}
