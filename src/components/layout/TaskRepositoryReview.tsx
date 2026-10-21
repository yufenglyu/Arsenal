import { ActionIcon } from "@/components/ui/action-icon";
import {repositoryRisk, repositoryLocked, repositorySelectable, selectedRepositories, useTaskReviewStore} from "@/stores/taskReviewStore";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { RefreshCw } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { useRepositorySyncStore } from "@/stores/repositorySyncStore";
import {
  isTaskActive,
  taskErrorMessage,
  useTaskQueueStore,
} from "@/stores/taskQueueStore";
import {
  type RepositoryTaskRow,
  type TaskBatch,
} from "@/lib/taskBatches";
import type { RepositorySyncPreview } from "@/types";
import { TaskDisclosure } from "./TaskDisclosure";

function Changes({ repo }: { repo: RepositorySyncPreview }) {
  const { t } = useTranslation();
  const paired = new Set(repo.added.filter(item => repo.deleted.some(old => old.name === item.name)).map(item => item.name));
  return (
    <div className="space-y-3">
      {[...paired].map(name => <TaskDisclosure key={name} title={t("workflow.flow.sameName", {name})}>
        <p className="mb-3 text-xs text-muted-foreground">{t("workflow.flow.sameNameHint")}</p>
        {(["added", "deleted"] as const).flatMap(category => repo[category].filter(item => item.name === name).map(item => <div key={`${category}:${item.skillId}`} className="py-2"><p className={`text-sm ${category === "deleted" ? "text-destructive" : ""}`}>{t(`workflow.change.${category}`)}</p><p className="break-all font-mono text-xs text-muted-foreground">{item.sourcePath ?? item.skillId}</p></div>))}
      </TaskDisclosure>)}
      {(["added", "modified", "deleted", "updated"] as const).flatMap(
        (category) =>
          (repo[category] ?? []).filter(item => !(["added", "deleted"].includes(category) && paired.has(item.name))).map((item) => (
            <TaskDisclosure
              key={`${category}:${item.skillId}:${item.version}`}
              title={
                <>
                  <span
                    className={
                      category === "deleted"
                        ? "text-destructive"
                        : "text-muted-foreground"
                    }
                  >
                    {t(
                      category === "updated"
                        ? "workflow.updated"
                        : `workflow.change.${category}`,
                    )}
                  </span>
                  <span className="min-w-0 break-all">{item.name}</span>
                </>
              }
            >
              {item.files?.length ? (
                item.files.map((file) => (
                  <p key={file.path} className="flex gap-2 py-1 text-xs">
                    <span
                      className={
                        file.status === "deleted"
                          ? "text-destructive"
                          : "text-muted-foreground"
                      }
                    >
                      {t(`workflow.change.${file.status}`)}
                    </span>
                    <span className="min-w-0 break-all font-mono">
                      {file.path}
                    </span>
                  </p>
                ))
              ) : (
                <p className="break-all text-xs text-muted-foreground">
                  {item.sourcePath ?? item.name}
                </p>
              )}
            </TaskDisclosure>
          )),
      )}
      {!!repo.documentChanges?.length && (
        <TaskDisclosure title={t("workflow.mirror.documents")}>
          {repo.documentChanges.map((file) => (
            <p key={file.path} className="flex gap-2 py-1 text-xs">
              <span
                className={
                  file.status === "deleted"
                    ? "text-destructive"
                    : "text-muted-foreground"
                }
              >
                {t(`workflow.change.${file.status}`)}
              </span>
              <span className="min-w-0 break-all font-mono">{file.path}</span>
            </p>
          ))}
        </TaskDisclosure>
      )}
      {!!repo.unchanged.length && (
        <TaskDisclosure
          small
          title={t("workflow.batch.unchangedCount", {
            count: repo.unchanged.length,
          })}
        >
          {repo.unchanged.map((item) => (
            <p
              key={`${item.skillId}:${item.version}`}
              className="break-all py-1 text-sm"
            >
              {item.name}
            </p>
          ))}
        </TaskDisclosure>
      )}
    </div>
  );
}

export function TaskRepositoryReview({ batch }: { batch: TaskBatch; onLater?: () => void }) {
  const { t, i18n } = useTranslation();
  const state = useRepositorySyncStore();
  const tasks = useTaskQueueStore(s => s.tasks);
  const selection = useTaskReviewStore();
  const [search, setSearch] = useState("");
  const [filter, setFilter] = useState<"attention" | "ready" | "risk" | "check" | "done" | "all">("attention");
  const rows = batch.report ?? [];
  const eligible = (row: RepositoryTaskRow) => repositorySelectable(row, tasks, state.migratingRepository);
  const approved = (row: RepositoryTaskRow) => !repositoryRisk(row) || selection.approved.includes(row.preview!.snapshotId!);
  const ready = rows.filter(row => eligible(row) && approved(row));
  const risks = rows.filter(row => eligible(row) && !approved(row));
  const recheck = rows.filter(row => row.needsCheck);
  const choices = selectedRepositories(rows, tasks, state.migratingRepository, selection);
  const completed = (row: RepositoryTaskRow) => !row.pending && !row.needsCheck && !row.error && row.status === "success";
  const matching = rows.filter(row => row.repository.toLowerCase().includes(search.trim().toLowerCase()));
  const visible = matching.filter(row => filter === "all" ||
    (filter === "ready" ? ready.includes(row) : filter === "risk" ? risks.includes(row) : filter === "check" ? recheck.includes(row) : filter === "done" ? completed(row) : !completed(row) && !ready.includes(row)));
  const counts = {attention: rows.length - ready.length - rows.filter(completed).length, ready: ready.length, risk: risks.length, check: recheck.length, done: rows.filter(completed).length, all: rows.length};
  return <div className="space-y-3 py-3">
    <div className="flex flex-wrap items-center justify-between gap-2">
      <span className="text-xs text-muted-foreground">{t("workflow.flow.selection", {count: choices.length, total: ready.length})}</span>
      {!!recheck.length && <Button variant="outline" onClick={() => void state.recheckRepositories(recheck.filter(row => !repositoryLocked(row, tasks)).map(row => row.repository))}><RefreshCw />{t("workflow.flow.recheck", {count: recheck.length})}</Button>}
    </div>
    {!!ready.length && <label className="flex items-center gap-2 text-sm"><input type="checkbox" checked={choices.length === ready.length} onChange={event => ready.forEach(row => selection.select(row.preview!.snapshotId!, event.target.checked))} />{t("workflow.flow.selectReady")}</label>}
    <div className="flex flex-wrap gap-4" aria-label={t("workflow.flow.filters")}>
      {(["attention", "ready", "risk", "check", "done", "all"] as const).map(value => <button type="button" key={value} aria-pressed={filter === value} onClick={() => setFilter(value)} className={`text-sm rounded-sm focus-visible:outline-2 focus-visible:outline-ring ${filter === value ? "font-semibold text-primary" : "text-muted-foreground"}`}>{t(`workflow.flow.${value}`)} {counts[value]}</button>)}
    </div>
    {rows.length > 8 && <Input aria-label={t("workflow.batch.searchRepos")} placeholder={t("workflow.batch.searchRepos")} value={search} onChange={event => setSearch(event.target.value)} />}
    {batch.reportError && <p role="alert" className="text-xs text-destructive">{taskErrorMessage(batch.reportError)}</p>}
    {visible.map(row => {
      const repo = row.preview;
      const running = repositoryLocked(row, tasks);
      const owner = tasks.find(task => isTaskActive(task) && task.locks.includes(`repo:${row.repository.toLowerCase()}`));
      const risk = repositoryRisk(row);
      const accepted = !!repo?.snapshotId && selection.approved.includes(repo.snapshotId);
      const selectable = eligible(row) && approved(row);
      return <section key={row.repository} className="rounded-md border border-border p-3" aria-label={row.repository}>
        <div className="flex items-center gap-2">
          {selectable && <input type="checkbox" aria-label={row.repository} checked={!selection.excluded.includes(repo!.snapshotId!)} onChange={event => selection.select(repo!.snapshotId!, event.target.checked)} />}
          <span className="min-w-0 flex-1 break-all text-sm">{row.repository}</span>
          <span className={`text-xs ${row.error ? "text-destructive" : "text-muted-foreground"}`}>{running && owner?.cancelRequested ? t("workflow.stopping") : running ? t(`workflow.taskStatus.${owner?.status === "queued" ? "queued" : "running"}`) : row.needsCheck ? t("workflow.batch.needsCheck") : completed(row) ? t(row.applied ? "workflow.updated" : "workflow.changeType.unchanged") : risk && !accepted ? t("workflow.flow.risk") : t("workflow.flow.ready")}</span>
          {running && owner ? <Button variant="ghost" disabled={owner.cancelRequested} onClick={() => useTaskQueueStore.getState().cancel(owner.id)}><ActionIcon action="stop" />{t("workflow.stopTask")}</Button> : row.needsCheck ? <Button variant="outline" disabled={state.migratingRepository?.toLowerCase() === row.repository.toLowerCase()} onClick={() => void state.recheckRepository(row.repository)}><RefreshCw />{t("workflow.recheck")}</Button> : null}
          <Button variant="ghost" size="icon" title={t("workflow.deleteTaskRecord",{name:row.repository})} aria-label={t("workflow.deleteTaskRecord",{name:row.repository})} onClick={()=>state.dismissRepositoryTask(row.repository)}><ActionIcon action="delete"/></Button>
          {accepted && selectable && <Button variant="ghost" onClick={() => selection.forget([repo!.snapshotId!])}><ActionIcon action="back" />{t("workflow.flow.withdraw")}</Button>}
        </div>
        {row.error && <p className="mt-1 text-xs text-destructive">{taskErrorMessage(row.error, "update")}</p>}
        {repo && (row.pending || repo.unchanged.length > 0 || !!repo.updated?.length) && <TaskDisclosure title={t(row.needsCheck ? "workflow.flow.lastResult" : risk && !accepted ? "workflow.flow.reviewRisk" : "workflow.flow.viewChanges")}>
          {row.needsCheck && <p className="mb-3 text-xs text-muted-foreground">{t("workflow.batch.previewUnavailable")}</p>}
          <Changes repo={repo} />
          {eligible(row) && risk && !accepted && <div className="mt-3 flex items-center justify-between gap-4"><p className="text-xs text-destructive">{t(`workflow.flow.${risk}`)}</p><Button onClick={() => selection.approve(repo.snapshotId!)}><ActionIcon action="confirm" />{t(risk === "deletions" ? "workflow.flow.allowDeletion" : "workflow.flow.allowOverwrite")}</Button></div>}
        </TaskDisclosure>}
      </section>;
    })}
    {!visible.length && <p className="py-4 text-sm text-muted-foreground">{t("workflow.flow.empty")}</p>}
    <TaskDisclosure small title={t("workflow.batch.log")}><p className="text-xs text-muted-foreground">{state.reportCheckedAt ? new Date(state.reportCheckedAt).toLocaleString(i18n.language) : t("workflow.checkTimeUnknown")}</p></TaskDisclosure>
  </div>;
}
