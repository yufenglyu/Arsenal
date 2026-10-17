import {useState} from "react";
import {useTranslation} from "react-i18next";
import {ChevronRight, FolderSync} from "lucide-react";
import {Button} from "@/components/ui/button";
import {Dialog, DialogContent, DialogHeader, DialogTitle} from "@/components/ui/dialog";
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
  const [confirmation, setConfirmation] = useState<string[]>([]);
  const [confirmCheck, setConfirmCheck] = useState(false);
  const reportTasks = tasks.filter(task=>!!state.reportGeneration && task.parentReportGeneration===state.reportGeneration && !task.clearRequested);
  const activeReportTasks = reportTasks.filter(isTaskActive);
  const currentSkills = useResourceLibraryStore(s=>s.skills);
  const repos = state.preview?.repositories ?? [];
  const coverage = state.preview?.coverage;
  const libraryChanged = !!coverage && (coverage.length !== currentSkills.length || coverage.some(item=>!currentSkills.some(skill=>skill.id===item.skillId)));
  const pending = (repo:RepositorySyncPreview) => hasRepositoryChanges(repo);
  const active = (repo:RepositorySyncPreview) => tasks.some(task => isTaskActive(task) && task.locks.includes(`repo:${repo.repository.toLowerCase()}`));
  const canApply = (repo:RepositorySyncPreview) => !!repo.snapshotId && !repo.error && !active(repo);
  const choices = repos.filter(repo => selected[repo.repository] && canApply(repo));
  const busy = state.isChecking || !!state.checkingRepository;
  const apply = () => {
    const batchId = crypto.randomUUID();
    const parentReportGeneration = getReportTaskGeneration();
    for (const repo of repos.filter(repo => repo.snapshotId && confirmation.includes(repo.snapshotId) && canApply(repo))) {
      useTaskQueueStore.getState().enqueue({key:`mirror:${repo.snapshotId}`,batchId,parentReportGeneration,
        batchLabel:t("workflow.mirror.apply"),kind:"update",label:repo.repository,locks:[`repo:${repo.repository.toLowerCase()}`],
        steps:[{command:"apply_repository_snapshot",label:repo.repository,args:{snapshotId:repo.snapshotId}}]});
    }
    setConfirmation([]); setSelected({});
  };
  const content = <div className="flex min-h-0 flex-1 flex-col">
    <div className="shrink-0 space-y-3 p-4">
      <div className="flex items-center justify-between gap-2"><h3 className="inline-flex items-center gap-2 text-sm font-semibold"><FolderSync className="size-[1em] shrink-0"/>{t("workflow.mirror.title")}</h3>
        <Button variant="ghost" disabled={busy || !!activeReportTasks.length} onClick={state.clearCheckResults}>{t("workflow.clearCheckResults")}</Button></div>
      <p className="text-xs text-muted-foreground">{t("workflow.mirror.hint")}</p>
      {!!activeReportTasks.length && <div className="flex items-center gap-2"><p role="status" className="text-xs text-muted-foreground">{t("workflow.batchProgress",{completed:reportTasks.filter(task=>task.status==="success").length,running:activeReportTasks.filter(task=>task.status==="running").length,queued:activeReportTasks.filter(task=>task.status==="queued").length,failed:reportTasks.filter(task=>["failed","partial","interrupted"].includes(task.status)).length})}</p><Button variant="outline" disabled={activeReportTasks.every(task=>task.cancelRequested)} onClick={()=>activeReportTasks.forEach(task=>useTaskQueueStore.getState().cancel(task.id))}>{t("workflow.stopTask")}</Button></div>}
      {libraryChanged && <p className="text-xs text-muted-foreground">{t("workflow.mirror.libraryChanged")}</p>}
      <p className="text-xs text-muted-foreground">{t("workflow.repositoryCheckTime")} · {state.reportCheckedAt ? new Date(state.reportCheckedAt).toLocaleString(i18n.language) : t("workflow.checkTimeUnknown")}</p>
      {coverage ? <details className="text-xs"><summary className="cursor-pointer">{t("workflow.mirror.coverage",{total:coverage.length,included:coverage.filter(c=>c.status!=="excluded").length,excluded:coverage.filter(c=>c.status==="excluded").length,completed:coverage.filter(c=>c.status==="completed").length,failed:coverage.filter(c=>c.status==="failed").length,pending:coverage.filter(c=>c.status==="pending").length,added:repos.reduce((n,r)=>n+r.added.length,0)})}</summary>
        <ul className="mt-3 max-h-48 overflow-auto">{coverage.map(item=><li key={item.skillId} className="py-1">{item.name} · {item.repository ?? t("workflow.mirror.local")} · {t(`workflow.mirror.coverageStatus.${item.reason ?? item.status}`)}</li>)}</ul></details>
        : state.preview && <p className="text-xs">{t("workflow.mirror.expired")}</p>}
      {state.error && <p role="alert" className="text-xs text-destructive">{taskErrorMessage(state.error)}</p>}
    </div>
    <div className="min-h-0 flex-1 space-y-4 overflow-auto px-4 pb-4">
      {repos.map(repo => {
        const rows = repositoryUpdateRows([repo], {});
        const task = [...tasks].reverse().find(task => task.key === `mirror:${repo.snapshotId}`);
        const error = state.updateErrors[`mirror:${repo.snapshotId}`];
        return <section key={repo.repository} className="rounded-md border border-border p-3">
          <div className="flex flex-wrap items-center gap-2">
            <input type="checkbox" aria-label={repo.repository} checked={!!selected[repo.repository]} disabled={!canApply(repo)} onChange={e=>setSelected(prev=>({...prev,[repo.repository]:e.target.checked}))}/>
            <span className="min-w-0 flex-1 break-all text-sm font-semibold">{repo.repository}</span>
            <span className="text-xs text-muted-foreground">{active(repo) ? t(`workflow.taskStatus.${task?.status ?? "running"}`) : error ? t("workflow.updateFailed") : repo.error ? t("workflow.checkFailed") : pending(repo) ? t("workflow.pendingUpdate") : t("workflow.changeType.unchanged")}</span>
            <Button variant="outline" disabled={busy || active(repo)} onClick={()=>void state.recheckRepository(repo.repository)}>{t("workflow.recheck")}</Button>
            <Button disabled={!canApply(repo)} onClick={()=>setConfirmation([repo.snapshotId!])}>{t(error ? "workflow.mirror.retry" : "workflow.mirror.apply")}</Button>
          </div>
          {repo.remoteRef && <p className="mt-3 truncate font-mono text-xs text-muted-foreground" title={repo.remoteRef}>{repo.remoteRef}</p>}
          {(repo.error || error) && <p role="alert" className="mt-3 text-xs text-destructive">{taskErrorMessage(repo.error || error, "update")}</p>}
          {!repo.snapshotId && !repo.error && <p className="mt-3 text-xs">{t("workflow.mirror.expired")}</p>}
          {!!repo.documentChanges?.length && <div className="mt-3 text-sm"><p className="font-semibold">{t("workflow.mirror.documents")}</p>{repo.documentChanges.map(file=><p key={file.path} className="mt-2 break-all">{t(`workflow.change.${file.status}`)} · {file.path}</p>)}</div>}
          {!!rows.length && <details open className="group mt-3"><summary className="flex cursor-pointer list-none items-center gap-2 text-sm font-semibold [&::-webkit-details-marker]:hidden">{t("workflow.mirror.details",{count:rows.length})}<ChevronRight className="ml-auto size-[1em] shrink-0 group-open:rotate-90"/></summary>
            <table className="mt-3 w-full table-fixed text-left text-sm"><thead><tr><th className="border-b py-2 font-normal">{t("workflow.taskPanel.skillRepository")}</th><th className="w-24 border-b py-2 font-normal">{t("workflow.taskPanel.change")}</th></tr></thead><tbody>{rows.map(row=><tr key={row.key}><td className="border-b py-2 pr-3"><p className="break-all">{row.item.name}</p>{row.item.files?.map(file=><p key={file.path} className="mt-1 break-all text-xs text-muted-foreground">{t(`workflow.change.${file.status}`)} · {file.path}</p>)}</td><td className="border-b py-2 align-top">{t(row.category==="updated" ? "workflow.updated" : `workflow.changeType.${row.category}`)}</td></tr>)}</tbody></table>
          </details>}
        </section>;
      })}
      {!repos.length && <p className="py-4 text-xs text-muted-foreground">{t("workflow.noPendingUpdates")}</p>}
    </div>
    <div className="flex shrink-0 flex-wrap items-center gap-2 border-t p-4">
      <label className="mr-auto inline-flex items-center gap-2 text-xs"><input type="checkbox" aria-label={t("workflow.selectAll")} checked={repos.some(canApply) && repos.filter(canApply).every(r=>selected[r.repository])} onChange={e=>setSelected(Object.fromEntries(repos.filter(canApply).map(r=>[r.repository,e.target.checked])))}/>{t("workflow.selectedCompact",{count:choices.length})}</label>
      <Button variant="outline" disabled={busy || tasks.some(isTaskActive)} onClick={()=>setConfirmCheck(true)}>{t("workflow.recheck")}</Button>
      <Button disabled={!choices.length} onClick={()=>setConfirmation(choices.map(repo=>repo.snapshotId!))}>{t("workflow.mirror.applySelected")}</Button>
    </div>
    <RepositoryCheckConfirm open={confirmCheck} onOpenChange={setConfirmCheck}/>
    <Dialog open={!!confirmation.length} onOpenChange={open=>{if(!open)setConfirmation([]);}}><DialogContent><DialogHeader><DialogTitle>{t("workflow.mirror.confirmTitle")}</DialogTitle></DialogHeader><p className="text-sm">{t("workflow.mirror.confirmHint")}</p><ul className="max-h-48 overflow-auto text-sm">{repos.filter(r=>r.snapshotId && confirmation.includes(r.snapshotId)).map(r=><li className="py-1" key={r.repository}>{r.repository}</li>)}</ul><div className="flex justify-end gap-2"><Button variant="outline" onClick={()=>setConfirmation([])}>{t("common.cancel")}</Button><Button onClick={apply}>{t("workflow.mirror.confirm")}</Button></div></DialogContent></Dialog>
  </div>;
  if (embedded || taskLayout) return content;
  return <Dialog open={state.open} onOpenChange={state.setOpen}><DialogContent className="flex max-h-[85vh] flex-col"><DialogHeader><DialogTitle>{t("workflow.mirror.title")}</DialogTitle></DialogHeader>{content}</DialogContent></Dialog>;
}
