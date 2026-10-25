import { ActionIcon } from "@/components/ui/action-icon";
import {repositoryRisk, repositoryLocked, repositorySelectable, selectedRepositories, useTaskReviewStore} from "@/stores/taskReviewStore";
import { useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { RefreshCw } from "lucide-react";
import { Button } from "@/components/ui/button";
import { HintIcon } from "@/components/ui/hint-icon";
import { useRepositorySyncStore } from "@/stores/repositorySyncStore";
import {
  isTaskActive,
  taskErrorMessage,
  useTaskQueueStore,
} from "@/stores/taskQueueStore";
import {
  repositoryHasDeletions,
  type RepositoryTaskRow,
  type TaskBatch,
} from "@/lib/taskBatches";
import type { RepositorySyncPreview } from "@/types";
import { TaskDisclosure } from "./TaskDisclosure";

// Keep the risk inventory tied to this repository's preview. Additions alone
// are routine changes; same-name replacements remain together for path review.
function RiskChanges({repo}: {repo: RepositorySyncPreview}) {
  const {t} = useTranslation();
  const localRisk = repo.localModified || (repo.localModified == null && repo.modified.length > 0);
  const riskyModified = repo.modified.filter(item => localRisk || item.files?.some(file => file.status === "deleted"));
  const riskyAdded = repo.added.filter(item => repo.deleted.some(old => old.name === item.name));
  const riskyDocuments = (repo.documentChanges ?? []).filter(file => file.status === "deleted" || (localRisk && file.status === "modified"));
  const risk = {...repo, added: riskyAdded, modified: riskyModified, unchanged: [], updated: [], documentChanges: riskyDocuments};
  const other = {...repo, added: repo.added.filter(item => !riskyAdded.includes(item)), modified: repo.modified.filter(item => !riskyModified.includes(item)), deleted: [], documentChanges: (repo.documentChanges ?? []).filter(file => !riskyDocuments.includes(file))};
  return <div className="space-y-3">
    <Changes repo={risk} />
    {!!(other.added.length + other.modified.length + other.unchanged.length + (other.updated?.length ?? 0) + other.documentChanges.length) && <TaskDisclosure title={t("workflow.flow.otherChanges")}><Changes repo={other} /></TaskDisclosure>}
  </div>;
}

function RiskSummary({repo}: {repo: RepositorySyncPreview}) {
  const {t} = useTranslation();
  const localRisk = repo.localModified || (repo.localModified == null && repo.modified.length > 0);
  const hasDeletions = repositoryHasDeletions(repo);
  const summary = [hasDeletions && t("workflow.flow.deletionRisk"), localRisk && t("workflow.flow.overwriteRisk")].filter(Boolean).join(" · ");
  const explanation = [t("workflow.flow.riskRepository", {repository: repo.repository}), hasDeletions && t("workflow.flow.deletions"), localRisk && t("workflow.flow.localChanges")].filter(Boolean).join("\n");
  return <div className="flex min-w-0 items-center gap-2">
    <p className={`min-w-0 text-xs ${hasDeletions ? "text-destructive" : "text-foreground"}`}>{summary}</p>
    <HintIcon text={explanation} className="text-xs [&_svg]:size-[1em]" />
  </div>;
}

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
                  <span className="min-w-0 break-all">
                    <span className="block">{item.name}</span>
                    {item.sourcePath && <span className="block font-mono text-xs text-muted-foreground">{item.sourcePath}</span>}
                  </span>
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
              ) : !item.sourcePath ? (
                <p className="break-all text-xs text-muted-foreground">
                  {item.name}
                </p>
              ) : null}
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
  const { t } = useTranslation();
  const state = useRepositorySyncStore();
  const tasks = useTaskQueueStore(s => s.tasks);
  const selection = useTaskReviewStore();
  const listRef = useRef<HTMLDivElement>(null);
  const [expandedRepositories, setExpandedRepositories] = useState<string[]>([]);
  const setRepositoryExpanded = (repository: string, expanded: boolean) => setExpandedRepositories(previous => expanded ? [...new Set([...previous, repository])] : previous.filter(name => name !== repository));
  const [filter, setFilter] = useState<"attention" | "ready" | "risk" | "check" | "done" | "all">("attention");
  const rows = batch.report ?? [];
  const eligible = (row: RepositoryTaskRow) => repositorySelectable(row, tasks, state.migratingRepository);
  const approved = (row: RepositoryTaskRow) => !repositoryRisk(row) || selection.approved.includes(row.preview!.snapshotId!);
  const ready = rows.filter(row => eligible(row) && approved(row));
  const risks = rows.filter(row => eligible(row) && !approved(row));
  const recheck = rows.filter(row => row.needsCheck);
  const choices = selectedRepositories(rows, tasks, state.migratingRepository, selection);
  const completed = (row: RepositoryTaskRow) => !row.pending && !row.needsCheck && !row.error && row.status === "success";
  const attention = rows.filter(row => !completed(row) && (!ready.includes(row) || !!repositoryRisk(row)));
  const visible = rows.filter(row => filter === "all" ||
    (filter === "ready" ? ready.includes(row) : filter === "risk" ? risks.includes(row) : filter === "check" ? recheck.includes(row) : filter === "done" ? completed(row) : attention.includes(row)));
  const counts = {attention: attention.length, ready: ready.length, risk: risks.length, check: recheck.length, done: rows.filter(completed).length, all: rows.length};
  const changeFilter = (value: typeof filter) => {
    setFilter(value);
    const list = listRef.current;
    const panel = list?.closest<HTMLElement>('[role="tabpanel"]');
    if (list && panel) panel.scrollTo?.({top: Math.max(0, panel.scrollTop + list.getBoundingClientRect().top - panel.getBoundingClientRect().top - 16)});
  };
  return <div className="grid grid-cols-[10rem_minmax(0,1fr)] items-start gap-4 py-4">
    <aside className="sticky top-4 max-h-[calc(100dvh-14rem)] min-w-0 space-y-4 overflow-y-auto overscroll-contain border-r border-border pl-5 pr-4" aria-label={t("workflow.flow.filters")}>
      <div className="space-y-2">
        <label title={!ready.length ? t("workflow.flow.noReady") : undefined} className="grid grid-cols-[1rem_minmax(0,1fr)] items-center gap-2 text-sm"><input className="justify-self-start" type="checkbox" aria-label={t("workflow.flow.selectReady")} disabled={!ready.length} ref={node => {if (node) node.indeterminate = choices.length > 0 && choices.length < ready.length;}} checked={!!ready.length && choices.length === ready.length} onChange={event => ready.forEach(row => selection.select(row.preview!.snapshotId!, event.target.checked))} /><span className="whitespace-nowrap">{t("workflow.flow.selectReadyShort")}</span></label>
      </div>
      <div className="flex flex-col gap-1">
        {(["attention", "ready", "risk", "check", "done", "all"] as const).map(value => <button type="button" key={value} aria-pressed={filter === value} onClick={() => changeFilter(value)} className={`flex w-full items-center justify-between gap-2 rounded-sm py-2 pl-6 pr-2 text-left text-sm focus-visible:outline-2 focus-visible:outline-ring ${filter === value ? "font-semibold text-primary" : "text-muted-foreground hover:text-foreground"}`}><span>{t(`workflow.flow.${value}`)}</span><span className="text-xs font-normal">{counts[value]}</span></button>)}
      </div>
    </aside>
    <div ref={listRef} className="min-w-0 space-y-3">
    {batch.reportError && <p role="alert" className="text-xs text-destructive">{taskErrorMessage(batch.reportError)}</p>}
    {visible.map(row => {
      const repo = row.preview;
      const running = repositoryLocked(row, tasks);
      const owner = tasks.find(task => isTaskActive(task) && task.locks.includes(`repo:${row.repository.toLowerCase()}`));
      const risk = repositoryRisk(row);
      const accepted = !!repo?.snapshotId && selection.approved.includes(repo.snapshotId);
      const selectable = eligible(row) && approved(row);
      const hasDetails = !!repo && (row.pending || repo.unchanged.length > 0 || !!repo.updated?.length);
      const summary = repo ? ([
        ["workflow.change.added", repo.added.length],
        ["workflow.change.modified", repo.modified.length],
        ["workflow.change.deleted", repo.deleted.length],
        ["workflow.flow.documents", repo.documentChanges?.length ?? 0],
        ["workflow.changeType.unchanged", row.pending ? 0 : repo.unchanged.length],
        ["workflow.updated", repo.updated?.length ?? 0],
      ] as const).filter(([, count]) => count > 0) : [];
      const summaryText = row.error ? taskErrorMessage(row.error, "update") : !running && !row.needsCheck ? summary.map(([key, count]) => `${t(key)} ${count}`).join(" · ") : "";
      const statusHint = risk && !accepted && !running && !row.needsCheck ? t(`workflow.flow.${risk}Summary`) : undefined;
      const header = <>
          {selectable && <input type="checkbox" aria-label={row.repository} checked={!selection.excluded.includes(repo!.snapshotId!)} onChange={event => selection.select(repo!.snapshotId!, event.target.checked)} />}
          <span className="min-w-0 flex-1 truncate text-sm sm:max-w-64 lg:max-w-72" title={row.repository}>{row.repository}</span>
          <span className={`min-w-0 flex-1 text-xs text-muted-foreground ${row.error ? "break-words whitespace-normal" : "truncate"}`} title={summaryText}>{summaryText}</span>
          <span title={statusHint} className={`shrink-0 whitespace-nowrap text-xs ${row.error ? "text-destructive" : "text-muted-foreground"}`}>{running && owner?.cancelRequested ? t("workflow.stopping") : running ? t(`workflow.taskStatus.${owner?.status === "queued" ? "queued" : "running"}`) : row.needsCheck ? t("workflow.batch.needsCheck") : completed(row) ? t(row.applied ? "workflow.updated" : "workflow.changeType.unchanged") : risk ? t(accepted ? "workflow.flow.confirmed" : "workflow.flow.risk") : t("workflow.flow.ready")}</span>
      </>;
      const actions = <div className="flex w-18 shrink-0 items-center justify-end gap-2">
          {running && owner ? <Button variant="ghost" size="icon" title={t("workflow.stopTask")} aria-label={t("workflow.stopTask")} disabled={owner.cancelRequested} onClick={() => useTaskQueueStore.getState().cancel(owner.id)}><ActionIcon action="stop" /></Button> : row.needsCheck ? <Button variant="ghost" size="icon" title={t("workflow.recheck")} aria-label={t("workflow.recheck")} disabled={state.migratingRepository?.toLowerCase() === row.repository.toLowerCase()} onClick={() => void state.recheckRepository(row.repository)}><RefreshCw /></Button> : null}
          <Button variant="ghost" size="icon" title={t("workflow.deleteTaskRecord",{name:row.repository})} aria-label={t("workflow.deleteTaskRecord",{name:row.repository})} onClick={()=>state.dismissRepositoryTask(row.repository)}><ActionIcon action="delete"/></Button>
          {accepted && selectable && <Button variant="ghost" size="icon" title={t("workflow.flow.withdraw")} aria-label={t("workflow.flow.withdraw")} onClick={() => selection.forget([repo!.snapshotId!])}><ActionIcon action="back" /></Button>}
      </div>;
      return <section key={row.repository} className="rounded-md border border-border p-3" aria-label={row.repository}>
        {hasDetails ? <TaskDisclosure header={header} actions={actions} title={t("workflow.flow.details")} expanded={expandedRepositories.includes(row.repository)} onExpandedChange={expanded => setRepositoryExpanded(row.repository, expanded)}>
          <div className="mb-3 flex items-center justify-between gap-2">
            <div className="flex min-w-0 flex-wrap items-center gap-x-4 gap-y-1">
              <p className="shrink-0 text-sm font-semibold">{t(row.needsCheck ? "workflow.flow.lastResult" : risk && !accepted ? "workflow.flow.reviewRisk" : "workflow.flow.viewChanges")}</p>
              {risk && !accepted && !row.needsCheck && <RiskSummary repo={repo!} />}
            </div>
            {eligible(row) && risk && !accepted && <Button variant="ghost" size="icon" title={t(risk === "deletions" ? "workflow.flow.allowDeletion" : "workflow.flow.allowOverwrite")} aria-label={t(risk === "deletions" ? "workflow.flow.allowDeletion" : "workflow.flow.allowOverwrite")} onClick={() => {selection.approve(repo!.snapshotId!); setRepositoryExpanded(row.repository, false);}}><ActionIcon action="confirm" /></Button>}
          </div>
          {row.needsCheck && <p className="mb-3 text-xs text-muted-foreground">{t("workflow.batch.previewUnavailable")}</p>}
          {risk && !accepted && !row.needsCheck ? <RiskChanges repo={repo!} /> : <Changes repo={repo!} />}
        </TaskDisclosure> : <div className="flex flex-wrap items-center gap-2"><div className="flex min-w-0 flex-1 flex-wrap items-center gap-x-4 gap-y-2">{header}</div><span className="size-8 shrink-0" aria-hidden="true" />{actions}</div>}
      </section>;
    })}
    {!visible.length && <p className="py-4 text-sm text-muted-foreground">{t("workflow.flow.empty")}</p>}

    </div>
  </div>;
}
