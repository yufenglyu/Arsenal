import {TaskDetailFilters, type DetailFilter} from "@/components/layout/TaskDetailFilters";
import {cn} from "@/lib/utils";
import {ActionIcon} from "@/components/ui/action-icon";
import {HintIcon} from "@/components/ui/hint-icon";
import {RepositoryAddressButton} from "./RepositoryAddressButton";
import {useState} from "react";
import {useTranslation} from "react-i18next";
import {ChevronRight, FolderSync} from "lucide-react";
import {Button} from "@/components/ui/button";
import {Dialog, DialogContent, DialogHeader, DialogTitle, DialogDescription, DialogFooter} from "@/components/ui/dialog";
import {RepositoryCheckConfirm} from "./RepositoryCheckConfirm";
import {getReportTaskGeneration, hasRepositoryChanges, useRepositorySyncStore} from "@/stores/repositorySyncStore";
import {isTaskActive, taskErrorMessage, useTaskQueueStore} from "@/stores/taskQueueStore";
import {repositoryUpdateRows} from "@/lib/repositoryUpdateRows";
import {useResourceLibraryStore} from "@/stores/resourceLibraryStore";
import type {RepositorySyncPreview} from "@/types";

export function UpdateCenter({embedded = false, taskLayout = false}: {embedded?:boolean;taskLayout?:boolean} = {}) {
  const {t, i18n} = useTranslation();
  const state = useRepositorySyncStore();
  const tasks = useTaskQueueStore(s => s.tasks);
  const [selected, setSelected] = useState<Record<string,boolean>>({});
  const [confirmation, setConfirmation] = useState<{repository:string; snapshotId:string | null; action:"check" | "apply"}[]>([]);
  const [filter, setFilter] = useState<DetailFilter>("all");
  const [search, setSearch] = useState("");
  const [confirmCheck, setConfirmCheck] = useState(false);
  const reportTasks = tasks.filter(task=>!!state.reportGeneration && task.parentReportGeneration===state.reportGeneration && !task.clearRequested);
  const activeReportTasks = reportTasks.filter(isTaskActive);
  const checkTasks = reportTasks.filter(task => task.steps.every(step => step.command === "preview_source_backed_resource_repository_updates"));
  const checkSteps = checkTasks.flatMap(task => task.steps);
  const pausedChecks = checkTasks.filter(task => ["cancelled", "interrupted"].includes(task.status));
  const currentSkills = useResourceLibraryStore(s=>s.skills);
  const repos = state.preview?.repositories ?? [];
  const coverage = state.preview?.coverage;
  const libraryChanged = !!coverage && (coverage.length !== currentSkills.length || coverage.some(item=>!currentSkills.some(skill=>skill.id===item.skillId)));
  const pending = (repo:RepositorySyncPreview) => hasRepositoryChanges(repo);
  const active = (repo:RepositorySyncPreview) => tasks.some(task => isTaskActive(task) && task.locks.includes(`repo:${repo.repository.toLowerCase()}`));
  const canApply = (repo:RepositorySyncPreview) => !!repo.snapshotId && !repo.error && !state.isChecking && !state.checkingRepository && !state.migratingRepository && !active(repo);
  const latestTask = (repo:RepositorySyncPreview) => [...reportTasks].reverse().find(task => task.key === `mirror:${repo.snapshotId}`);
  const failed = (repo:RepositorySyncPreview) => !!(repo.error || state.updateErrors[`mirror:${repo.snapshotId}`] || ["failed", "partial", "interrupted"].includes(latestTask(repo)?.status ?? ""));
  const needsCheck = (repo:RepositorySyncPreview) => !!repo.error || !repo.snapshotId;
  const matches = (repo:RepositorySyncPreview, value:DetailFilter) => {
    if (value === "all") return true;
    if (active(repo)) return false;
    if (value === "failed") return failed(repo);
    if (value === "attention") return failed(repo) || needsCheck(repo) || pending(repo);
    return !failed(repo) && !needsCheck(repo) && !pending(repo) && (!!repo.updated?.length || latestTask(repo)?.status === "success");
  };
  const counts = Object.fromEntries((["all", "attention", "failed", "finished"] as const).map(value => [value, repos.filter(repo => matches(repo, value)).length])) as Record<DetailFilter,number>;
  const visibleRepos = repos.filter(repo => matches(repo, filter) && repo.repository.toLocaleLowerCase().includes(search.trim().toLocaleLowerCase()));
  const canProcess = (repo:RepositorySyncPreview) => !state.isChecking && !state.checkingRepository && !state.migratingRepository && !active(repo) && matches(repo, "attention");
  const choices = visibleRepos.filter(repo => selected[repo.repository] && canProcess(repo));
  const requestProcessing = (repositories:RepositorySyncPreview[]) => setConfirmation(repositories.map(repo => ({repository:repo.repository, snapshotId:repo.snapshotId ?? null, action:needsCheck(repo) ? "check" : "apply"})));
  const busy = state.isChecking || !!state.checkingRepository || !!state.migratingRepository;
  const apply = () => {
    const batchId = crypto.randomUUID();
    const parentReportGeneration = getReportTaskGeneration();
    const rechecks:string[] = [];
    for (const choice of confirmation) {
      const repo = repos.find(repo => repo.repository === choice.repository && (repo.snapshotId ?? null) === choice.snapshotId);
      if (!repo || busy || active(repo)) continue;
      if (choice.action === "check") {
        if (needsCheck(repo)) rechecks.push(repo.repository);
        continue;
      }
      if (!canApply(repo)) continue;
      useTaskQueueStore.getState().enqueue({key:`mirror:${repo.snapshotId}`,batchId,parentReportGeneration,
        batchLabel:t("workflow.mirror.apply"),kind:"update",label:repo.repository,locks:[`repo:${repo.repository.toLowerCase()}`],
        steps:[{command:"apply_repository_snapshot",label:repo.repository,args:{snapshotId:repo.snapshotId}}]});
    }
    if (rechecks.length) void state.recheckRepositories(rechecks);
    setConfirmation([]); setSelected({});
  };
  const content = <div className="flex min-h-0 flex-1 flex-col">
    <div className="shrink-0 space-y-3 p-4">
      <div className="flex items-center justify-between gap-2"><h3 className="inline-flex items-center gap-2 text-sm font-semibold"><FolderSync className="size-[1em] shrink-0"/>{t("workflow.mirror.title")}<HintIcon text={t("workflow.mirror.hint")}/></h3>
        <Button variant="ghost" disabled={busy || !!activeReportTasks.length} onClick={state.clearCheckResults}>{t("workflow.clearCheckResults")}</Button></div>
      {!!activeReportTasks.length && <div className="flex items-center gap-2"><p role="status" className="text-xs text-muted-foreground">{t("workflow.batchProgress",{completed:reportTasks.filter(task=>task.status==="success").length,running:activeReportTasks.filter(task=>task.status==="running").length,queued:activeReportTasks.filter(task=>task.status==="queued").length,failed:reportTasks.filter(task=>["failed","partial","interrupted"].includes(task.status)).length})}</p><Button variant="outline" disabled={activeReportTasks.every(task=>task.cancelRequested)} onClick={()=>activeReportTasks.forEach(task=>useTaskQueueStore.getState().cancel(task.id))}>{t(activeReportTasks.every(task => checkTasks.includes(task)) ? "workflow.pauseCheck" : "workflow.stopTask")}</Button></div>}
      {!!checkTasks.length && <div className="flex items-center gap-2">
        <p className="text-xs text-muted-foreground">{t("workflow.taskSteps", {completed:checkSteps.filter(step => step.status === "success").length,total:checkSteps.length})}<HintIcon text={t("workflow.checkCheckpointHint")}/></p>
        {!activeReportTasks.length && !!pausedChecks.length && <Button variant="outline" onClick={() => void state.checkForUpdates()}>{t("workflow.continueCheck")}</Button>}
      </div>}
      {libraryChanged && <p className="text-xs text-muted-foreground">{t("workflow.mirror.libraryChanged")}</p>}
      <details className="group text-xs">
        <summary className="flex cursor-pointer list-none items-center gap-2 text-muted-foreground [&::-webkit-details-marker]:hidden">{t("workflow.taskPanel.reportDetails")}<ChevronRight className="size-[1em] shrink-0 group-open:rotate-90"/></summary>
        <div className="mt-3 space-y-3">
      <p className="text-xs text-muted-foreground">{t("workflow.repositoryCheckTime")} · {state.reportCheckedAt ? new Date(state.reportCheckedAt).toLocaleString(i18n.language) : t("workflow.checkTimeUnknown")}</p>
      {coverage ? <div className="text-xs"><p>{t("workflow.mirror.coverage",{total:coverage.length,included:coverage.filter(c=>c.status!=="excluded").length,excluded:coverage.filter(c=>c.status==="excluded").length,completed:coverage.filter(c=>c.status==="completed").length,failed:coverage.filter(c=>c.status==="failed").length,pending:coverage.filter(c=>c.status==="pending").length,added:repos.reduce((n,r)=>n+r.added.length,0)})}</p>
        <ul className="mt-3 max-h-48 overflow-auto">{coverage.map(item=><li key={item.skillId} className="py-1">{item.name} · {item.repository ?? t("workflow.mirror.local")} · {t(`workflow.mirror.coverageStatus.${item.reason ?? item.status}`)}</li>)}</ul></div>
        : null}
        </div>
      </details>
      {!coverage && state.preview && <p className="text-xs">{t("workflow.mirror.expired")}</p>}
      {state.error && <p role="alert" className="text-xs text-destructive">{taskErrorMessage(state.error)}</p>}
      <TaskDetailFilters value={filter} counts={counts} search={search}
        onChange={value=>{setFilter(value);setSelected({});}} onSearch={value=>{setSearch(value);setSelected({});}}/>
    </div>
    <div className="min-h-0 flex-1 space-y-4 overflow-auto px-4 pb-4">
      {visibleRepos.map(repo => {
        const rows = repositoryUpdateRows([repo], {});
        const task = [...tasks].reverse().find(task => task.key === `mirror:${repo.snapshotId}`);
        const error = state.updateErrors[`mirror:${repo.snapshotId}`] || (failed(repo) && !repo.error ? latestTask(repo)?.steps.find(step=>step.error)?.error : undefined);
        return <section key={repo.repository} className="rounded-md border border-border p-3">
          <div className="flex flex-wrap items-center gap-2">
            <input type="checkbox" aria-label={repo.repository} checked={!!selected[repo.repository]} disabled={!canProcess(repo)} onChange={e=>setSelected(prev=>({...prev,[repo.repository]:e.target.checked}))}/>
            <span className="min-w-0 flex-1 break-all text-sm font-semibold">{repo.repository}</span>
            <span className={cn("text-xs", active(repo) ? "text-primary" : failed(repo) ? "text-destructive" : "text-muted-foreground")}>{active(repo) ? t(`workflow.taskStatus.${task?.status ?? "running"}`) : error || failed(repo) && !repo.error ? t("workflow.updateFailed") : repo.error ? t("workflow.checkFailed") : pending(repo) ? t("workflow.pendingUpdate") : matches(repo,"finished") ? t("workflow.taskStatus.success") : t("workflow.changeType.unchanged")}</span>
            {needsCheck(repo) ? <Button variant="outline" disabled={busy || active(repo)} onClick={()=>void state.recheckRepository(repo.repository)}><ActionIcon action="check"/>{t("workflow.recheck")}</Button> : <>
              <Button variant="ghost" size="icon" title={t("workflow.recheck")} aria-label={t("workflow.recheck")} disabled={busy || active(repo)} onClick={()=>void state.recheckRepository(repo.repository)}><ActionIcon action="check"/></Button>
              <Button disabled={!canApply(repo)} onClick={()=>requestProcessing([repo])}>{t(failed(repo) ? "workflow.mirror.retry" : "workflow.mirror.apply")}</Button>
            </>}
          </div>
          {(repo.error || error) && <p role="alert" className="mt-3 text-xs text-destructive">{taskErrorMessage(repo.error || error, "update")}</p>}
          {!repo.snapshotId && !repo.error && <p className="mt-3 text-xs">{t("workflow.mirror.expired")}</p>}
          {(!!rows.length || !!repo.documentChanges?.length || !!repo.error) && <details className="group mt-3"><summary className="flex cursor-pointer list-none items-center gap-2 text-xs text-muted-foreground [&::-webkit-details-marker]:hidden">{t("workflow.mirror.details",{count:rows.length})}<ChevronRight className="ml-auto size-[1em] shrink-0 group-open:rotate-90"/></summary>
            <div className="mt-3"><RepositoryAddressButton repository={repo.repository}/></div>
            {repo.remoteRef && <p className="mt-3 truncate font-mono text-xs text-muted-foreground" title={repo.remoteRef}>{repo.remoteRef}</p>}
            {!!repo.documentChanges?.length && <div className="mt-3 text-sm"><p className="font-semibold">{t("workflow.mirror.documents")}</p>{repo.documentChanges.map(file=><p key={file.path} className="mt-2 break-all">{t(`workflow.change.${file.status}`)} · {file.path}</p>)}</div>}
            <table className="mt-3 w-full table-fixed text-left text-sm"><thead><tr><th className="border-b py-2 font-normal">{t("workflow.taskPanel.skillRepository")}</th><th className="w-24 border-b py-2 font-normal">{t("workflow.taskPanel.change")}</th></tr></thead><tbody>{rows.map(row=><tr key={row.key}><td className="border-b py-2 pr-3"><p className="break-all">{row.item.name}</p>{row.item.files?.map(file=><p key={file.path} className="mt-1 break-all text-xs text-muted-foreground">{t(`workflow.change.${file.status}`)} · {file.path}</p>)}</td><td className="border-b py-2 align-top">{t(row.category==="updated" ? "workflow.updated" : `workflow.changeType.${row.category}`)}</td></tr>)}</tbody></table>
          </details>}
        </section>;
      })}
      {!visibleRepos.length && <p className="py-4 text-xs text-muted-foreground">{t(repos.length ? "workflow.noFilteredCheckResults" : "workflow.noPendingUpdates")}</p>}
    </div>
    <div className="flex shrink-0 flex-wrap items-center gap-2 border-t p-4">
      <label className="mr-auto inline-flex items-center gap-2 text-xs"><input type="checkbox" ref={input=>{if(input)input.indeterminate=choices.length>0 && choices.length<visibleRepos.filter(canProcess).length;}} aria-label={t("workflow.selectAll")} title={t("workflow.taskPanel.selectVisible")} disabled={!visibleRepos.some(canProcess)} checked={visibleRepos.some(canProcess) && visibleRepos.filter(canProcess).every(r=>selected[r.repository])} onChange={e=>setSelected(Object.fromEntries(visibleRepos.filter(canProcess).map(r=>[r.repository,e.target.checked])))}/>{t("workflow.taskPanel.selectVisible")}<span>{t("workflow.selectedCompact",{count:choices.length})}</span></label>
      <Button variant="outline" disabled={busy || tasks.some(isTaskActive)} onClick={()=>setConfirmCheck(true)}>{t("workflow.recheck")}</Button>
      <Button disabled={!choices.length} onClick={()=>requestProcessing(choices)}>{t(filter === "failed" ? "workflow.taskPanel.retrySelected" : "workflow.taskPanel.processSelected")}</Button>
    </div>
    <RepositoryCheckConfirm open={confirmCheck} onOpenChange={setConfirmCheck}/>
    <Dialog open={!!confirmation.length} onOpenChange={open=>{if(!open)setConfirmation([]);}}>
      <DialogContent className="sm:max-w-xl">
        <DialogHeader><DialogTitle><FolderSync/>{t(confirmation.some(choice=>choice.action==="apply") ? "workflow.mirror.confirmTitle" : "workflow.taskPanel.processTitle")}</DialogTitle></DialogHeader>
        <DialogDescription className="text-foreground leading-relaxed">{confirmation.some(choice=>choice.action==="apply") && t("workflow.mirror.confirmHint")}{confirmation.some(choice=>choice.action==="check") && <span className="mt-3 block">{t("workflow.taskPanel.recheckBeforeSync")}</span>}</DialogDescription>
        <ul className="max-h-48 overflow-auto rounded-md border border-border divide-y divide-border text-sm">
          {confirmation.map(r=><li className="flex items-center gap-2 px-3 py-3" key={r.repository}><FolderSync className="size-[1em] shrink-0 text-muted-foreground"/><span className="min-w-0 break-all">{r.repository}</span></li>)}
        </ul>
        <DialogFooter><Button variant="outline" onClick={()=>setConfirmation([])}>{t("common.cancel")}</Button><Button onClick={apply}><FolderSync/>{t(confirmation.some(choice=>choice.action==="apply") ? "workflow.mirror.confirm" : "common.confirm")}</Button></DialogFooter>
      </DialogContent>
    </Dialog>
  </div>;
  if (embedded || taskLayout) return content;
  return <Dialog open={state.open} onOpenChange={state.setOpen}><DialogContent className="flex max-h-[85vh] flex-col gap-0 p-0 sm:max-w-3xl"><DialogHeader className="m-0"><DialogTitle><FolderSync/>{t("workflow.mirror.title")}</DialogTitle></DialogHeader>{content}</DialogContent></Dialog>;
}
