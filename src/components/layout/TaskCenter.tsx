import {ActionIcon} from "@/components/ui/action-icon";
import {useEffect, useRef, useState} from "react";
import {Download, RefreshCw} from "lucide-react";
import {useTranslation} from "react-i18next";
import {useTaskQueueStore, isTaskActive, taskErrorMessage, type TaskStep, type BackgroundTask} from "@/stores/taskQueueStore";
import {useRepositorySyncStore} from "@/stores/repositorySyncStore";
import {useAppStatusStore} from "@/stores/appStatusStore";
import {activityGroups, activityAttention, pendingUpdateCount, needsAttention} from "@/lib/activityCenter";
import {groupStatus, importDetailRows, importRepository, matchesTaskFilter, taskPriority, taskTab, type TaskFilter, type TaskTab} from "@/lib/taskPresentation";
import {cn} from "@/lib/utils";
import {UpdateCenter} from "@/components/skill/UpdateCenter";
import {AddSkillsDialog} from "@/components/skill/AddSkillsDialog";
import {PREPARE_GITHUB_IMPORT} from "@/stores/importPreparationStore";
import {isTauriRuntime} from "@/lib/tauri";
import {Button} from "@/components/ui/button";
import {Dialog, DialogContent, DialogHeader, DialogTitle} from "@/components/ui/dialog";
// Only render the known check-report fields; never expose arbitrary IPC args/results.
function RepositoryCheckDetails({step}: {step: TaskStep}) {
  const {t} = useTranslation();
  const scope = Array.isArray(step.args.repositories) ? step.args.repositories.filter((repo): repo is string => typeof repo === "string") : [];
  const result = step.result;
  const repositories = result && typeof result === "object" && "repositories" in result && Array.isArray(result.repositories)
    ? result.repositories.filter((repo): repo is import("@/types").RepositorySyncPreview =>
      repo && typeof repo === "object" && typeof repo.repository === "string" &&
      ["added", "modified", "deleted", "unchanged"].every(category => Array.isArray(repo[category]))) : [];
  const categories = ["added", "modified", "deleted", "unchanged"] as const;
  const successful = repositories.filter(repo => !repo.error);
  return <div className="mt-2 space-y-2">
    <p className="text-muted-foreground">{t("workflow.checkScope")} · {scope.length ? scope.join(", ") : t("workflow.allSourceRepositories")}</p>
    {repositories.length ? <>
      <p>{t("workflow.checkResultSummary", {repositories: repositories.length,
        skills: successful.reduce((sum, repo) => sum + categories.reduce((count, category) => count + repo[category].length, 0), 0),
        failures: repositories.length - successful.length})}</p>
      <ul className="divide-y divide-border">
        {repositories.map(repo => <li key={repo.repository} className="py-2">
          <p className="font-medium">{repo.repository}</p>
          {repo.error ? <p className="mt-1 text-destructive">{t("workflow.checkFailed")} · {taskErrorMessage(repo.error)}</p> : <>
            <div className="mt-1 flex flex-wrap gap-x-4 gap-y-1 text-muted-foreground">
              {categories.map(category => <span key={category}>{t(`workflow.change.${category}`)} {repo[category].length}</span>)}
            </div>
            {!!(repo.added.length + repo.modified.length + repo.deleted.length) && <details className="mt-1">
              <summary className="cursor-pointer text-muted-foreground">{t("workflow.changedSkills")}</summary>
              {categories.filter(category => category !== "unchanged").flatMap(category => repo[category].filter(item => typeof item.name === "string").map(item =>
                <p key={`${category}:${item.skillId}`} className="py-0.5">{t(`workflow.change.${category}`)} · {item.name}</p>))}
            </details>}
          </>}
        </li>)}
      </ul>
    </> : <p className="text-muted-foreground">{t(step.status === "running" || step.status === "queued" ? "workflow.checkDetailsPending" : "workflow.checkDetailsUnavailable")}</p>}
  </div>;
}


function TaskDetails({items}: {items: BackgroundTask[]}) {
  const {t} = useTranslation();
  const [filter, setFilter] = useState("all");
  const imports = items[0]?.kind === "import";
  const rows = importDetailRows(items).filter(row =>
    (filter === "all" || filter === "failed" && ["failed", "partial", "interrupted"].includes(row.status) ||
      filter === "attention" && row.status === "awaiting_input" || filter === "finished" && ["imported", "skipped", "success"].includes(row.status)));
  return <>
    {imports && <div className="shrink-0 space-y-3 px-4 pb-3">
      <div className="flex flex-wrap gap-1" aria-label={t("workflow.taskPanel.detailFilter")}>
        {(["all", "attention", "failed", "finished"] as const).map(value => <Button key={value} size="sm" variant="ghost" aria-pressed={filter === value}
          className={cn("text-xs", filter === value && "bg-primary/10 font-semibold text-primary")} onClick={() => setFilter(value)}>{t(`workflow.taskPanel.importFilter.${value}`)}</Button>)}
      </div>
    </div>}
    <div className="min-h-0 flex-1 overflow-auto px-4 pb-4">
      {imports ? <table className="w-full table-fixed text-left text-sm">
        <thead className="sticky top-0 bg-popover text-muted-foreground"><tr><th className="border-b py-2 font-medium">{t("skillBrowser.columns.name")}</th><th className="w-28 border-b py-2 font-medium">{t("workflow.taskPanel.status")}</th></tr></thead>
        <tbody>{rows.map(row => <tr key={row.key}><td className="border-b border-border/70 py-3 pr-3"><span className="break-all">{row.name}</span>{row.error && <p className="mt-1 text-destructive">{taskErrorMessage(row.error, "import")}</p>}</td>
          <td className={cn("border-b border-border/70 py-3 align-top", ["partial", "failed", "interrupted"].includes(row.status) ? "text-destructive" : "text-muted-foreground")}>{t(row.status === "imported" ? "status.importedLabel" : row.status === "skipped" ? "status.skippedLabel" : row.status === "ready" ? "workflow.taskPanel.readyToImport" : `workflow.taskStatus.${row.status}`)}</td></tr>)}</tbody>
      </table> : items.map(task => <section key={task.id} className="border-b border-border py-3 text-sm">
        <p className="mb-2 font-medium">{task.label} · {t(`workflow.taskStatus.${task.status}`)}</p>
        {task.steps.map((step, index) => <div key={index} className="mb-2">
          <p>{step.label} · {t(`workflow.taskStatus.${step.status ?? "queued"}`)}</p>
          {step.error && <p className="mt-1 text-destructive">{taskErrorMessage(step.error, task.kind)}</p>}
          {step.command === "preview_source_backed_resource_repository_updates" && <RepositoryCheckDetails step={step}/>}
          {task.kind === "ai" && step.status === "success" && <p className="mt-2 select-text whitespace-pre-wrap">{typeof step.result === "string" ? step.result : Array.isArray(step.result) && step.result.every(item => typeof item === "string") ? step.result.join(", ") : ""}</p>}
        </div>)}
      </section>)}
      {imports && !rows.length && <p className="py-8 text-center text-xs text-muted-foreground">{t(items.some(isTaskActive) ? "workflow.taskPanel.importPreparing" : "workflow.noFilteredCheckResults")}</p>}
    </div>
  </>;
}

export function TaskCenter() {
  const {t, i18n} = useTranslation();
  const state = useRepositorySyncStore();
  const tasks = useTaskQueueStore(s => s.tasks).filter(task => !task.clearRequested);
  const importResult = useAppStatusStore(s => s.task);
  const [chosenTab, setTab] = useState<TaskTab | null>(null);
  const [filters, setFilters] = useState<Record<TaskTab, TaskFilter>>({import:"all", update:"all"});
  const [selection, setSelection] = useState<Record<TaskTab, string | null>>({import:null, update:null});
  const [reviewTaskId, setReviewTaskId] = useState<string | null>(null);
  const wasOpen = useRef(false);
  const {count: attentionCount} = activityAttention(tasks, state.preview, state.checkedAt, state.error);
  const updatePending = pendingUpdateCount(state.preview, state.ignored, tasks);
  const pending = updatePending + attentionCount + (state.error ? 1 : 0);
  const hasReport = !!state.preview || !!state.error;
  const tab = chosenTab ?? (state.centerView === "history" && importResult?.kind === "import" ? "import"
    : updatePending || hasReport ? "update" : tasks.some(task => task.kind === "import") ? "import" : "update");
  const reportTasks = hasReport && state.reportGeneration ? tasks.filter(task => task.parentReportGeneration === state.reportGeneration && !task.clearRequested) : [];
  const reportActive = reportTasks.filter(isTaskActive);
  const groups = activityGroups(tasks.filter(task => !reportTasks.includes(task)));
  // The newest completed check is represented by the live report, not a duplicate row.
  const latestCheckId = groups.find(group => group.items.every(task => task.kind === "check" && task.steps.some(step => step.command === "preview_source_backed_resource_repository_updates")))?.id;
  const entries = groups.filter(group => !(hasReport && group.id === latestCheckId && !group.items.some(isTaskActive))).map(group => ({
    id:group.id, label:group.items[0].kind === "import" ? importRepository(group.items) ?? group.label : group.label, tab:taskTab(group.items[0]), status:groupStatus(group.items), createdAt:group.items[0].createdAt, items:group.items,
  }));
  if (hasReport) entries.push({id:"report",label:t(state.repositories?.length ? "workflow.taskPanel.scopedCheck" : "workflow.taskPanel.fullCheck"),tab:"update",
    status:reportActive.length ? groupStatus(reportActive) : updatePending || state.error ? "awaiting_input" : "success",createdAt:state.reportCheckedAt ?? 0,items:[]});
  if (importResult?.kind && importResult.status !== "idle" && !tasks.some(task => task.id === importResult.id)) entries.push({
    id:`status:${importResult.id}`,label:importResult.label,tab:importResult.kind,
    status:importResult.status === "running" ? "running" : importResult.status === "error" ? "partial" : "success",
    createdAt:Date.parse(importResult.startedAt ?? "") || 0,items:[],
  });
  const visible = entries.filter(entry => entry.tab === tab && matchesTaskFilter(entry.status, filters[tab]))
    .sort((a,b) => taskPriority(a.status) - taskPriority(b.status) || b.createdAt - a.createdAt);
  const current = visible.find(entry => entry.id === selection[tab]) ?? visible.find(entry => entry.id === "report") ?? visible[0];
  const active = current?.items.filter(isTaskActive) ?? [];
  const stopping = active.length > 0 && active.every(task => task.cancelRequested);
  const reviewTask = current?.items.find(task => task.status === "awaiting_input" && task.steps.some(step => step.command === PREPARE_GITHUB_IMPORT));
  const completed = current?.items.reduce((sum, task) => sum + task.steps.filter(step => step.status === "success").length, 0) ?? 0;
  const total = current?.items.reduce((sum, task) => sum + task.steps.length, 0) ?? 0;
  const importRows = current?.tab === "import" ? importDetailRows(current.items) : [];
  const repository = current?.tab === "import" ? importRepository(current.items) : null;
  const importingTask = active.find(task => task.status === "running" && task.steps.some(step => step.command === "import_github_repo_skills" && step.status === "running"));
  const importProgress = importingTask?.importProgress;
  const importStage = importingTask ? importProgress?.phase ?? "preparing" : "checking";
  const fileProgress = importProgress?.phase === "writing" && importProgress.totalFiles > 0 ? importProgress : null;
  const progressTotal = fileProgress?.totalFiles ?? Math.max(1,total);
  const progressCompleted = fileProgress?.completedFiles ?? completed;
  const determinate = !!fileProgress || total > 1 || completed > 0;
  const currentId = current?.id;
  useEffect(() => {
    if (!state.open) return;
    if (chosenTab === null) setTab(tab);
    if (currentId && !selection[tab]) setSelection(previous => ({...previous,[tab]:currentId}));
  }, [state.open, chosenTab, tab, currentId, selection]);
  useEffect(() => {
    if (!state.open) {wasOpen.current=false;return;}
    if (wasOpen.current) return;
    wasOpen.current=true;
    if (state.centerView === "pending" && hasReport) {
      setTab("update");
      setFilters(previous => ({...previous,update:"all"}));
      setSelection(previous => ({...previous,update:"report"}));
    }
  }, [state.open, state.centerView, hasReport]);
  const statusId = importResult?.id;
  const statusKind = importResult?.kind;
  useEffect(() => {
    if (state.open && state.centerView === "history" && statusId && statusKind) {
      setTab(statusKind);
      setFilters(previous => ({...previous,[statusKind]:"all"}));
      setSelection(previous => ({...previous,[statusKind]:`status:${statusId}`}));
    }
  }, [state.open, state.centerView, statusId, statusKind]);
  const statusLabel = (status: ReturnType<typeof groupStatus>) => t(status === "stopping" ? "workflow.stopping" : status === "awaiting_input" ? "workflow.taskPanel.filter.attention" : `workflow.taskStatus.${status}`);
  const statusColor = (status: ReturnType<typeof groupStatus>) => matchesTaskFilter(status, "failed") ? "text-destructive"
    : matchesTaskFilter(status, "active") || status === "awaiting_input" ? "text-primary" : "text-muted-foreground";
  function cleanFinished() {
    const candidates = entries.filter(entry => entry.tab === tab && ["success", "cancelled"].includes(entry.status));
    candidates.forEach(entry => entry.items.forEach(task => useTaskQueueStore.getState().remove(task.id)));
    if (candidates.some(entry => entry.id.startsWith("status:"))) useAppStatusStore.getState().resetStatus();
  }
  useEffect(() => {
    const before = (event: BeforeUnloadEvent) => {
      if (useTaskQueueStore.getState().tasks.some(isTaskActive)) {
        event.preventDefault();
        event.returnValue = "";
      }
    };
    window.addEventListener("beforeunload", before);
    let dispose: (() => void) | undefined;
    let stopped = false;
    if (isTauriRuntime())
      void import("@tauri-apps/api/window")
        .then(async ({ getCurrentWindow }) => {
          const unlisten = await getCurrentWindow().onCloseRequested(
            async (event) => {
              if (!useTaskQueueStore.getState().tasks.some(isTaskActive))
                return;
              event.preventDefault();
              const { confirm } = await import("@tauri-apps/plugin-dialog");
              if (await confirm(t("workflow.exitTasks"), { kind: "warning" }))
                await getCurrentWindow().destroy();
            },
          );
          if (stopped) unlisten();
          else dispose = unlisten;
        })
        .catch(() => {
          /* No native window is available in browser previews. */
        });
    return () => {
      stopped = true;
      dispose?.();
      window.removeEventListener("beforeunload", before);
    };
  }, [t]);



  return <>
    <Button size="status" variant="ghost" onClick={() => state.setOpen(true)}><ActionIcon action="tasks"/>{t("workflow.activityTitle")} · {t("workflow.activitySummary", {running:tasks.filter(isTaskActive).length,pending})}</Button>
    <Dialog open={state.open} onOpenChange={state.setOpen}>
      <DialogContent className="flex resize flex-col gap-0 overflow-hidden p-0" style={{width:"min(72rem, calc(100vw - 2rem))",height:"min(46rem, calc(100dvh - 2rem))",minWidth:"min(40rem, calc(100vw - 2rem))",minHeight:"min(28rem, calc(100dvh - 2rem))",maxWidth:"calc(100vw - 2rem)",maxHeight:"calc(100dvh - 2rem)"}}>
        <DialogHeader className="shrink-0 px-4 pt-4 pb-3 pr-12"><div className="flex items-center gap-1"><DialogTitle className="flex items-center gap-2"><ActionIcon action="tasks"/>{t("workflow.activityTitle")}</DialogTitle><Button type="button" size="icon-sm" variant="ghost" className="text-muted-foreground" title={t("workflow.taskPanel.backgroundHint")} aria-label={t("workflow.taskPanel.backgroundHint")}><ActionIcon action="info"/></Button></div></DialogHeader>
        <div role="tablist" aria-label={t("workflow.taskPanel.taskType")} className="flex shrink-0 gap-2 border-b border-border px-4 pb-3">
          {(["import", "update"] as const).map(value => <Button key={value} role="tab" aria-selected={tab === value} tabIndex={tab === value ? 0 : -1} aria-controls={`task-panel-${value}`} id={`task-tab-${value}`} size="sm" variant="ghost"
            onKeyDown={event => {if (["ArrowLeft", "ArrowRight", "Home", "End"].includes(event.key)) {event.preventDefault();const next=event.key === "Home" ? "import" : event.key === "End" ? "update" : value === "import" ? "update" : "import";setTab(next);document.getElementById(`task-tab-${next}`)?.focus({preventScroll:true});}}}
            className={cn(tab === value && "bg-primary/10 font-semibold text-primary")} onClick={() => setTab(value)}>
            {value === "import" ? <Download className="size-4"/> : <RefreshCw className="size-4"/>}{t(`workflow.taskPanel.tabs.${value}`)}
            <span className="text-xs font-normal">{t("workflow.taskPanel.runningCount",{count:entries.filter(entry => entry.tab === value && matchesTaskFilter(entry.status,"active")).length})}</span>
          </Button>)}
        </div>
        <div className="flex min-h-0 flex-1" role="tabpanel" id={`task-panel-${tab}`} aria-labelledby={`task-tab-${tab}`}>
          <aside className="flex w-56 shrink-0 flex-col border-r border-border bg-muted/20 sm:w-64" aria-label={t("workflow.taskPanel.taskList")}>
            <div className="shrink-0 p-3"><select className="h-8 w-full rounded-md border border-border bg-popover px-2 text-xs" aria-label={t("workflow.taskPanel.filterTasks")} value={filters[tab]} onChange={event => setFilters(previous => ({...previous,[tab]:event.target.value as TaskFilter}))}>
              {(["all", "active", "attention", "failed", "finished"] as const).map(value => <option key={value} value={value}>{t(`workflow.taskPanel.filter.${value}`)} {entries.filter(entry => entry.tab === tab && matchesTaskFilter(entry.status,value)).length}</option>)}
            </select></div>
            <div className="min-h-0 flex-1 space-y-1 overflow-auto px-2 pb-3">
              {visible.map(entry => <button key={entry.id} type="button" aria-pressed={current?.id === entry.id} onClick={() => setSelection(previous => ({...previous,[tab]:entry.id}))}
                className={cn("w-full rounded-md border border-transparent p-3 text-left hover:bg-muted/60 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring",current?.id === entry.id && "border-primary/20 bg-primary/10")}>
                <span className="mb-1.5 flex items-center justify-between gap-2 text-xs"><span className={statusColor(entry.status)}>{statusLabel(entry.status)}</span><span className="shrink-0 text-muted-foreground">{entry.createdAt ? new Date(entry.createdAt).toLocaleTimeString(i18n.language,{hour:"2-digit",minute:"2-digit"}) : "—"}</span></span>
                <span className="block truncate text-sm font-semibold" title={entry.label}>{entry.label}</span>
                <span className="mt-1 block text-xs text-muted-foreground">{entry.id === "report" ? t("workflow.taskPanel.pendingChanges",{count:updatePending}) : entry.items.length ? t("workflow.taskSteps",{completed:entry.items.reduce((sum,task) => sum+task.steps.filter(step => step.status === "success").length,0),total:entry.items.reduce((sum,task) => sum+task.steps.length,0)}) : t("workflow.taskPanel.skillDetails")}</span>
              </button>)}
              {!visible.length && <p className="px-3 py-10 text-center text-xs text-muted-foreground">{t("workflow.noTasks")}</p>}
            </div>
            <div className="flex h-14 shrink-0 items-center border-t border-border px-3"><Button size="sm" variant="ghost" className="text-muted-foreground" disabled={!entries.some(entry => entry.tab === tab && entry.id !== "report" && ["success", "cancelled"].includes(entry.status))} onClick={cleanFinished}><ActionIcon action="clear"/>{t("workflow.clearFinished")}</Button></div>
          </aside>
          <section className="flex min-h-0 min-w-0 flex-1 flex-col" aria-label={t("workflow.taskPanel.skillDetails")}>
            {hasReport && <div hidden={current?.id !== "report"} className={current?.id === "report" ? "flex min-h-0 flex-1 flex-col" : "hidden"}><UpdateCenter key={state.reportGeneration ?? "initial"} embedded taskLayout/></div>}
            {current?.items.length ? <>
              <div className="shrink-0 px-4 py-4">
                <div className="flex flex-wrap items-start justify-between gap-3"><div className="min-w-0 flex-1"><h3 className="break-all text-sm font-semibold">{current.label}</h3><p className="mt-1 text-xs text-muted-foreground">{t(`workflow.taskKind.${current.items[0].kind}`)} · {new Date(current.createdAt).toLocaleString(i18n.language)} · <span className={statusColor(current.status)}>{statusLabel(current.status)}</span></p></div>
                  <div className="flex flex-wrap items-center gap-1">
                    {reviewTask && <Button size="sm" onClick={() => setReviewTaskId(reviewTask.id)}>{t("workflow.resumeImport")}</Button>}
                    {active.length > 0 ? <Button size="sm" variant="outline" disabled={stopping} onClick={() => active.forEach(task => useTaskQueueStore.getState().cancel(task.id))}><ActionIcon action="stop"/>{t("workflow.stopTask")}</Button>
                      : current.items.some(task => needsAttention(task) && task.status !== "awaiting_input" || task.status === "cancelled") && <Button size="sm" variant="outline" onClick={() => current.items.filter(task => needsAttention(task) && task.status !== "awaiting_input" || task.status === "cancelled").forEach(task => useTaskQueueStore.getState().retry(task.id))}><ActionIcon action="retry"/>{t("workflow.retryTask")}</Button>}
                    {(!active.length || stopping) && <Button size="sm" variant="ghost" aria-label={t("workflow.deleteTaskRecord",{name:current.label})} onClick={() => current.items.forEach(task => useTaskQueueStore.getState().remove(task.id))}><ActionIcon action="delete"/>{t("workflow.deleteRecord")}</Button>}
                  </div>
                </div>
                <p className="mt-3 text-xs text-muted-foreground">{importRows.length ? t("workflow.taskPanel.importSummary",{total:importRows.length,imported:importRows.filter(row => row.status === "imported" || row.status === "success").length,skipped:importRows.filter(row => row.status === "skipped").length}) : t("workflow.taskSteps",{completed,total})}</p>
                {current.tab === "import" && active.length > 0 && (repository || importingTask) && <div role="status" aria-live="polite" className="mt-3 space-y-1 text-xs">
                  {repository && <p>{t("workflow.taskPanel.currentRepository",{repository})}</p>}
                  <p className="text-primary">{t(active.some(task => task.status === "running") ? `workflow.taskPanel.importStage.${importStage}` : "workflow.taskStatus.queued")}</p>
                  {importProgress?.currentSkill && <p className="break-all font-medium">{t("workflow.taskPanel.currentSkill",{name:importProgress.currentSkill})}</p>}
                  {importProgress?.currentPath && <p className="break-all text-muted-foreground">{t("workflow.taskPanel.currentFile",{path:importProgress.currentPath})}</p>}
                  {!!importProgress?.totalFiles && <p className="text-muted-foreground">{t("workflow.taskPanel.fileProgress",{completed:importProgress.completedFiles,total:importProgress.totalFiles})}</p>}
                </div>}
                {current.items.length > 1 && <p className="mt-2 text-xs text-muted-foreground">{t("workflow.batchProgress",{completed:current.items.filter(task => task.status === "success").length,running:current.items.filter(task => task.status === "running").length,queued:current.items.filter(task => task.status === "queued").length,failed:current.items.filter(task => needsAttention(task) && task.status !== "awaiting_input").length})}</p>}
                {active.length > 0 && <div role="progressbar" aria-label={current.label} aria-valuemin={0} aria-valuenow={determinate ? progressCompleted : undefined} aria-valuemax={progressTotal} className="mt-3 h-1 overflow-hidden rounded-full bg-muted"><div className={cn("h-full bg-primary",!determinate && "animate-pulse")} style={{width:!determinate ? "33%" : `${Math.min(1,progressCompleted/progressTotal)*100}%`}}/></div>}
                {stopping && <p className="mt-3 text-xs text-muted-foreground">{t("workflow.stopAfterStep")}</p>}
              </div>
              <TaskDetails key={current.id} items={current.items}/>
              <div className="flex min-h-14 shrink-0 flex-wrap items-center justify-between gap-2 border-t border-border px-4 py-2 text-xs text-muted-foreground"><span>{t("workflow.taskPanel.recordsHint")}</span>
                <Button size="sm" variant="outline" disabled={!tasks.some(task => taskTab(task) === tab && isTaskActive(task) && !task.cancelRequested)} onClick={() => tasks.filter(task => taskTab(task) === tab && isTaskActive(task)).forEach(task => useTaskQueueStore.getState().cancel(task.id))}><ActionIcon action="stop"/>{t("workflow.stopAllTasks")}</Button>
              </div>
            </> : current?.id.startsWith("status:") && importResult ? <>
              <div className="flex shrink-0 items-center justify-between gap-2 px-4 py-4"><h3 className="font-semibold">{importResult.label}</h3>{importResult.status !== "running" && <Button size="sm" variant="ghost" onClick={() => useAppStatusStore.getState().resetStatus()}>{t("workflow.deleteRecord")}</Button>}</div>
              <div className="min-h-0 flex-1 overflow-auto px-4">{importResult.items?.map((item,index) => <div key={index} className="flex flex-wrap items-center gap-2 border-b border-border py-3 text-xs"><span>{item.name}</span><span className="text-muted-foreground">{item.repository}</span><span className="ml-auto">{t(item.status === "updated" ? importResult.kind === "import" ? "status.importedLabel" : "workflow.updated" : item.status === "failed" ? "workflow.taskStatus.failed" : "status.skippedLabel")}</span>{item.status === "failed" && importResult.onRetryFailedItem && <Button size="sm" variant="outline" onClick={() => importResult.onRetryFailedItem?.(item)}>{t("status.retryFailedItem")}</Button>}</div>)}</div>
            </> : current?.id !== "report" && <p className="m-auto px-4 text-center text-sm text-muted-foreground">{t("workflow.taskPanel.selectTask")}</p>}
          </section>
        </div>
      </DialogContent>
    </Dialog>
    {reviewTaskId && <AddSkillsDialog key={reviewTaskId} preparedTaskId={reviewTaskId} open onOpenChange={open => {if(!open)setReviewTaskId(null);}}/>}
  </>;
}
