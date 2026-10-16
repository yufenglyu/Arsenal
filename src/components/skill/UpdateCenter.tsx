import { ActionIcon } from "@/components/ui/action-icon";
import { RepositoryCheckConfirm } from "./RepositoryCheckConfirm";
import {repositoryUpdateRows, updateCategories as categories, updateItemKey, type UpdateCategory as Category} from "@/lib/repositoryUpdateRows";
import { useMemo, useState, type ReactNode } from "react";
import { ChevronRight, Info } from "lucide-react";
import { useTranslation } from "react-i18next";
import { getReportTaskGeneration, useRepositorySyncStore } from "@/stores/repositorySyncStore";
import {
  useTaskQueueStore,
  isTaskActive,
  taskErrorMessage,
} from "@/stores/taskQueueStore";
import { Button } from "@/components/ui/button";
import {cn} from "@/lib/utils";
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
export function UpdateCenter({ embedded = false, taskLayout = false }: { embedded?: boolean; taskLayout?: boolean } = {}) {
  const { t, i18n } = useTranslation();
  const [confirmCheck, setConfirmCheck] = useState(false);
  const state = useRepositorySyncStore();
  const tasks = useTaskQueueStore((s) => s.tasks);
  const reportTasks = tasks.filter(task => !!state.reportGeneration && task.parentReportGeneration === state.reportGeneration && !task.clearRequested);
  const activeReportTasks = reportTasks.filter(isTaskActive);
  const stoppingReport = activeReportTasks.length > 0 && activeReportTasks.every(task => task.cancelRequested);
  const [expanded, setExpanded] = useState<Record<string, boolean>>({});
  const [retryChoices, setRetryChoices] = useState<Record<string, boolean>>({});
  const [choices, setChoices] = useState<Record<string, boolean>>({});
  const [filter, setFilter] = useState<Category | "ignored" | "failed" | "updateFailed" | "attention" | "failures" | null>(taskLayout ? "attention" : null);
  const [pairs,setPairs] = useState<Record<string,string>>({});
  const rows = useMemo(() => repositoryUpdateRows(state.preview?.repositories ?? [],pairs),[state.preview,pairs]);
  const [confirmItems, setConfirmItems] = useState<typeof rows>([]);
  const busy = (key: string) =>
    tasks.some((task) => task.key === key && isTaskActive(task));
  const failed = (key: string) => {
    if (state.updateErrors[key]) return state.updateErrors[key];
    const latest = [...tasks].reverse().find(task => task.key === key);
    const repo = latest?.steps[0]?.args.repository;
    if (!latest || !["partial", "failed"].includes(latest.status)) return undefined;
    if (typeof repo === "string" && latest.createdAt <= (state.checkedAt[repo.toLowerCase()] ?? 0)) return undefined;
    return latest.steps.find(step => step.error)?.error;
  };
  const ignored = (key: string) => state.ignored.includes(key);
  const selectable = (row: (typeof rows)[number]) => !row.error && !busy(row.key) && (!taskLayout || !["unchanged", "updated"].includes(row.category));
  const applicable = (row: (typeof rows)[number]) => selectable(row) && row.category !== "unchanged" && row.category !== "updated" && !ignored(row.key) && (!row.candidates.length || !!row.replacement);
  const checked = (row: (typeof rows)[number]) => selectable(row) &&
    (choices[row.pairKey] ?? (!taskLayout && applicable(row) && ["added", "modified"].includes(row.category)));
  const visible = rows.filter((row) =>
    filter === "ignored"
      ? ignored(row.key)
      : !ignored(row.key) && (filter === "updateFailed" || filter === "failures" ? !!failed(row.key) : filter === "attention" ? !["unchanged", "updated"].includes(row.category) : (!filter || row.category === filter)),
  );
  const selected = visible.filter(checked);
  const failedRepositories = (state.preview?.repositories ?? []).filter(repo => !!repo.error);
  const visibleFailures = filter === null || filter === "failed" || filter === "failures" || filter === "attention" ? failedRepositories : [];
  const selectedFailures = visibleFailures.filter(repo => retryChoices[repo.repository]);
  const selectedRepositories = [...new Set([...selected.map(row => row.repo), ...selectedFailures.map(repo => repo.repository)])];
  const updatesToApply = selected.filter(applicable);
  const replacementsToApply = updatesToApply.filter(row => !!row.replacement);
  const regularUpdatesToApply = updatesToApply.filter(row => !row.replacement);
  const failedRows = visible.filter(row => failed(row.key) && applicable(row));
  const selectedFailedRows = selected.filter(row => failed(row.key) && applicable(row));
  const updatesToRetry = selected.length ? selectedFailedRows : failedRows;
  const repositoryChecking = (repository: string) => tasks.some(task => isTaskActive(task) && task.steps.some(step =>
    step.command === "preview_source_backed_resource_repository_updates" &&
    (!Array.isArray(step.args.repositories) || step.args.repositories.some(repo => String(repo).toLowerCase() === repository.toLowerCase()))));
  const checking = state.isChecking || !!state.checkingRepository;
  const repositoriesToRecheck = selectedRepositories.filter(repo => !repositoryChecking(repo));
  const retryableFailures = (selectedFailures.length ? selectedFailures : visibleFailures).filter(repo => !repositoryChecking(repo.repository));
  const selectableCount = visible.filter(selectable).length + visibleFailures.length;
  const selectedCount = selected.length + selectedFailures.length;
  const allSelected = selectableCount > 0 && selectedCount === selectableCount;
  const partiallySelected = selectedCount > 0 && !allSelected;
  function toggleAll(checked: boolean) {
    setRetryChoices(previous => ({...previous,...Object.fromEntries(visibleFailures.map(repo=>[repo.repository,checked]))}));
    setChoices(previous=>({...previous,...Object.fromEntries(visible.filter(selectable).map(row=>[row.pairKey,checked]))}));
  }
  function apply(items = regularUpdatesToApply) {
    const batchId = crypto.randomUUID();
    const parentReportGeneration = getReportTaskGeneration();
    for (const row of items) {
      useTaskQueueStore.getState().enqueue({
        key: row.key,
        batchId,
        parentReportGeneration,
        batchLabel: t("workflow.updateBatch", {count: items.length}),
        kind: "update",
        label: row.item.name,
        locks: [`repo:${row.repo.toLowerCase()}`, `skill:${row.item.skillId}`],
        steps: [
          {
            label: row.item.name,
            command: "apply_repository_update_item",
            args: {
              batchId,
              repository: row.repo,
              skillId: row.item.skillId,
              version: row.item.version ?? "",
              action: row.replacement ? "replace" : row.category,
              ...(row.replacement ? {replacementSkillId:row.replacement.skillId,replacementVersion:row.replacement.version} : {}),
            },
          },
        ],
      });
    }
    setChoices(previous => ({...previous, ...Object.fromEntries(items.map(row => [row.pairKey, false]))}));
  }
  function requestApply(items = regularUpdatesToApply) {
    if (items.some(row => row.category === "deleted")) setConfirmItems(items);
    else apply(items);
  }
  if (taskLayout) {
    const attention = rows.filter(row => !ignored(row.key) && !["unchanged", "updated"].includes(row.category));
    const filters = [
      {key:"attention",label:t("workflow.taskPanel.filter.attention"),count:attention.length + failedRepositories.length},
      {key:"failures",label:t("workflow.taskPanel.filter.failed"),count:rows.filter(row => !ignored(row.key) && !!failed(row.key)).length + failedRepositories.length},
      {key:"updated",label:t("workflow.updated"),count:rows.filter(row => row.category === "updated" && !ignored(row.key)).length},
      {key:"unchanged",label:t("workflow.changeType.unchanged"),count:rows.filter(row => row.category === "unchanged" && !ignored(row.key)).length},
      {key:"ignored",label:t("workflow.ignored"),count:rows.filter(row => ignored(row.key)).length},
      {key:null,label:t("workflow.allUpdates"),count:rows.filter(row => !ignored(row.key)).length + failedRepositories.length},
    ] as const;
    return <div className="flex min-h-0 flex-1 flex-col">
      <div className="shrink-0 px-4 py-4">
        <div className="flex flex-wrap items-start justify-between gap-2"><div className="min-w-0 flex-1"><h3 className="text-sm font-semibold">{t(state.repositories?.length ? "workflow.taskPanel.scopedCheck" : "workflow.taskPanel.fullCheck")}</h3><p className="mt-1 text-xs text-muted-foreground">{t("workflow.repositoryCheckTime")} · {state.reportCheckedAt ? new Date(state.reportCheckedAt).toLocaleString(i18n.language) : t("workflow.checkTimeUnknown")}</p></div>
          <div className="flex items-center gap-2">
            {activeReportTasks.length > 0 && <Button variant="outline" disabled={stoppingReport} onClick={() => activeReportTasks.forEach(task => useTaskQueueStore.getState().cancel(task.id))}><ActionIcon action="stop"/>{t("workflow.stopTask")}</Button>}
            <Button size="sm" variant="ghost" title={t("workflow.clearCheckResultsHint")} disabled={checking || activeReportTasks.length > 0 || tasks.some(task => isTaskActive(task) && task.kind === "check")} onClick={state.clearCheckResults}><ActionIcon action="delete"/>{t("workflow.clearCheckResults")}</Button>
          </div>
        </div>
        <p className="mt-3 text-xs text-muted-foreground">{t("workflow.taskPanel.resultSummary",{skills:rows.length,changes:attention.length,failures:failedRepositories.length})}</p>
        {activeReportTasks.length > 0 && <p role="status" className="mt-3 text-xs text-muted-foreground">{t("workflow.batchProgress",{completed:reportTasks.filter(task => task.status === "success").length,running:activeReportTasks.filter(task => task.status === "running").length,queued:activeReportTasks.filter(task => task.status === "queued").length,failed:reportTasks.filter(task => ["failed","partial","interrupted"].includes(task.status)).length})}</p>}
        {stoppingReport && <p className="mt-3 text-xs text-muted-foreground">{t("workflow.stopAfterStep")}</p>}
      </div>
      <div className="shrink-0 space-y-3 px-4 pb-3">
        <div className="flex flex-wrap gap-1" aria-label={t("workflow.taskPanel.detailFilter")}>{filters.map(item => <Button key={String(item.key)} size="sm" variant="ghost" aria-pressed={filter === item.key} className={cn("text-xs",filter === item.key && "bg-primary/10 font-semibold text-primary")} onClick={() => setFilter(item.key)}>{item.label} {item.count}</Button>)}</div>
        {attention.some(row => row.category === "deleted") && <p className="text-xs text-muted-foreground">{t("workflow.taskPanel.deletionHint")}</p>}
      </div>
      <div className="min-h-0 flex-1 overflow-auto px-4 pb-4">
        {state.error && <div className="mb-3 flex items-center gap-2 text-xs"><p className="text-destructive">{taskErrorMessage(state.error)}</p><Button size="sm" variant="ghost" onClick={() => state.dismissFailedCheck()}>{t("workflow.removeCheckResult")}</Button></div>}
        <table className="w-full min-w-[28rem] table-fixed text-left text-sm">
          <thead className="sticky top-0 z-10 bg-popover text-muted-foreground"><tr><th className="w-7 border-b py-2"><span className="sr-only">{t("workflow.selectAll")}</span></th><th className="border-b py-2 font-medium">{t("workflow.taskPanel.skillRepository")}</th><th className="w-24 border-b py-2 font-medium">{t("workflow.taskPanel.change")}</th><th className="w-24 border-b py-2 font-medium">{t("workflow.taskPanel.status")}</th><th className="w-28 border-b py-2 font-medium">{t("workflow.taskPanel.actions")}</th></tr></thead>
          <tbody>
            {visibleFailures.map(repo => <tr key={`failed:${repo.repository}`}><td className="border-b border-border/70 py-3 align-top"><input type="checkbox" aria-label={repo.repository} checked={!!retryChoices[repo.repository]} onChange={event => setRetryChoices(previous => ({...previous,[repo.repository]:event.target.checked}))}/></td><td className="border-b border-border/70 py-3 pr-3"><span className="break-all font-medium">{repo.repository}</span><p className="mt-1 text-destructive">{taskErrorMessage(repo.error)}</p></td><td className="border-b border-border/70 py-3 align-top text-muted-foreground">{t("workflow.taskPanel.repository")}</td><td className="border-b border-border/70 py-3 align-top text-destructive">{repositoryChecking(repo.repository) ? t("workflow.taskStatus.running") : t("workflow.checkFailed")}</td><td className="border-b border-border/70 py-3 align-top"><div className="flex flex-wrap gap-1"><Button size="sm" variant="ghost" disabled={checking || repositoryChecking(repo.repository)} onClick={() => void state.recheckRepository(repo.repository)}>{t("workflow.recheck")}</Button><Button size="sm" variant="ghost" aria-label={t("workflow.taskPanel.dismissRepositoryFailure",{name:repo.repository})} disabled={checking || repositoryChecking(repo.repository)} onClick={() => state.dismissFailedCheck(repo.repository)}><ActionIcon action="delete"/></Button></div></td></tr>)}
            {visible.map(row => <tr key={row.key}>
              <td className="border-b border-border/70 py-3 align-top"><input type="checkbox" aria-label={row.item.name} checked={checked(row)} disabled={!selectable(row)} onChange={event => setChoices(previous => ({...previous,[row.pairKey]:event.target.checked}))}/></td>
              <td className="border-b border-border/70 py-3 pr-3"><span className="break-all font-medium">{row.item.name}</span><p className="mt-1 break-all text-muted-foreground">{row.repo}</p>
                {failed(row.key) && <p className="mt-1 text-destructive">{taskErrorMessage(failed(row.key),"update")}</p>}
                {row.candidates.length > 0 && <div className="mt-2"><label className="text-muted-foreground">{t("workflow.reimportSource")}<select aria-label={t("workflow.replacementFor",{name:row.item.name})} className="mt-1 w-full rounded-md border border-border bg-popover p-1" disabled={busy(row.key)} value={row.replacement ? updateItemKey(row.repo,row.replacement) : ""} onChange={event => setPairs(previous => ({...previous,[row.pairKey]:event.target.value}))}>
                  <option value="">{t("workflow.chooseReplacement")}</option>{row.candidates.map(candidate => {const key=updateItemKey(row.repo,candidate);return <option key={key} value={key} disabled={rows.some(other => other.pairKey !== row.pairKey && other.repo === row.repo && other.replacement === candidate)}>{candidate.name} · {candidate.sourcePath}</option>;})}
                </select></label></div>}
                {!!row.item.files?.length && <details className="mt-2 text-muted-foreground"><summary className="cursor-pointer">{t("workflow.files",{count:row.item.files.length})}</summary>{row.item.files.map(file => <p className="break-all" key={file.path}>{t(`workflow.change.${file.status}`)} · {file.path}</p>)}</details>}
              </td>
              <td className="border-b border-border/70 py-3 align-top text-muted-foreground">{t(row.category === "updated" ? "workflow.updated" : `workflow.changeType.${row.category}`)}</td>
              <td className={cn("border-b border-border/70 py-3 align-top", failed(row.key) ? "text-destructive" : "text-muted-foreground")}>{busy(row.key) ? t(tasks.find(task => task.key === row.key && isTaskActive(task))?.cancelRequested ? "workflow.stopping" : `workflow.taskStatus.${tasks.find(task => task.key === row.key && isTaskActive(task))?.status}`) : ignored(row.key) ? t("workflow.ignored") : failed(row.key) ? t("workflow.updateFailed") : t(row.category === "updated" ? "workflow.updated" : row.category === "unchanged" ? "workflow.changeType.unchanged" : row.category === "deleted" ? "workflow.pendingDecision" : "workflow.pendingUpdate")}</td>
              <td className="border-b border-border/70 py-3 align-top">{applicable(row) && <Button size="sm" variant="ghost" className="text-primary" onClick={() => requestApply([row])}>{t(failed(row.key) ? "workflow.retryTask" : row.replacement ? "workflow.replaceDeletedCompact" : row.category === "deleted" ? "workflow.removeLocal" : "workflow.updateOne")}</Button>}{filter === "ignored" && <Button size="sm" variant="ghost" onClick={() => state.setIgnored(state.ignored.filter(key => key !== row.key))}>{t("workflow.restoreIgnored")}</Button>}</td>
            </tr>)}
          </tbody>
        </table>
        {!visible.length && !visibleFailures.length && <p className="py-10 text-center text-xs text-muted-foreground">{t(state.preview ? "workflow.noFilteredCheckResults" : "workflow.noPendingUpdates")}</p>}
      </div>
      <div className="flex min-h-14 shrink-0 flex-wrap items-center gap-2 border-t border-border px-4 py-2">
        <label className="mr-auto flex items-center gap-2 text-xs text-muted-foreground"><input type="checkbox" aria-label={t("workflow.selectAll")} checked={allSelected} aria-checked={partiallySelected ? "mixed" : allSelected} ref={node => {if(node)node.indeterminate=partiallySelected;}} disabled={!selectableCount} onChange={event => toggleAll(event.target.checked)}/>{t("workflow.selectedCompact",{count:selectedCount})}</label>
        <Button size="sm" variant="outline" disabled={!updatesToApply.length} onClick={() => state.setIgnored([...new Set([...state.ignored,...updatesToApply.map(row => row.key)])])}><ActionIcon action="ignore"/>{t("workflow.ignore")}</Button>
        <Button size="sm" variant="outline" disabled={state.isChecking || !repositoriesToRecheck.length} onClick={() => void state.recheckRepositories(repositoriesToRecheck)}><ActionIcon action="check"/>{t("workflow.recheckSelected")}</Button>
        {!!replacementsToApply.length && <Button size="sm" variant="outline" onClick={() => requestApply(replacementsToApply)}>{t("workflow.replaceDeletedCompact")}</Button>}
        <Button size="sm" disabled={!regularUpdatesToApply.length} onClick={() => requestApply()}><ActionIcon action="update"/>{t("workflow.apply")}</Button>
      </div>
      <Dialog open={confirmItems.length > 0} onOpenChange={open => {if(!open)setConfirmItems([]);}}><DialogContent><DialogHeader><DialogTitle>{t("workflow.taskPanel.confirmDeletion")}</DialogTitle></DialogHeader><p className="text-sm text-muted-foreground">{t("workflow.taskPanel.confirmDeletionHint")}</p><ul className="max-h-48 overflow-auto text-sm">{confirmItems.filter(row => row.category === "deleted").map(row => <li key={row.key} className="py-1">{row.item.name} · {row.repo}</li>)}</ul><div className="flex justify-end gap-2"><Button variant="outline" onClick={() => setConfirmItems([])}>{t("common.cancel")}</Button><Button onClick={() => {const keys=new Set(confirmItems.map(row => row.key));apply(rows.filter(row => keys.has(row.key) && applicable(row)));setConfirmItems([]);}}>{t("workflow.taskPanel.confirmProcess")}</Button></div></DialogContent></Dialog>
    </div>;
  }
  return (
    <>
    <RepositoryCheckConfirm open={confirmCheck} onOpenChange={setConfirmCheck} repositories={state.repositories ?? undefined}/>
    <UpdateFrame embedded={embedded}>
        <div className="flex flex-wrap items-center justify-between gap-2">
          <p className="text-xs text-muted-foreground">{t(state.repositories?.length ? "workflow.scopedCheckTime" : "workflow.fullCheckTime")} · {state.reportCheckedAt ? new Date(state.reportCheckedAt).toLocaleString(i18n.language) : t("workflow.checkTimeUnknown")}</p>
          <Button size="sm" variant="ghost" title={t("workflow.clearCheckResultsHint")} disabled={checking || tasks.some(task => isTaskActive(task) && task.steps.some(step => step.command === "preview_source_backed_resource_repository_updates")) || (!state.preview && !state.error)} onClick={state.clearCheckResults}><ActionIcon action="delete"/>{t("workflow.clearCheckResults")}</Button>
        </div>
        <div className="flex flex-wrap gap-2">
          <Button size="sm" variant={filter === null ? "default" : "outline"} aria-pressed={filter === null} onClick={() => setFilter(null)}><ActionIcon action="filter"/>
            {t("workflow.allUpdates")} {rows.filter(row => !ignored(row.key)).length + failedRepositories.length}
          </Button>
          {categories.map((category) => (
            <Button
              key={category}
              size="sm"
              variant={filter === category ? "default" : "outline"}
              aria-pressed={filter === category}
              onClick={() => setFilter(filter === category ? null : category)}
            ><ActionIcon action={category === "added" ? "add" : category === "deleted" ? "delete" : category === "modified" ? "update" : "success"}/>
              {t(category === "updated" ? "workflow.updated" : `workflow.${embedded ? "changeType" : "change"}.${category}`)}{" "}
              {
                rows.filter(
                  (row) => row.category === category && !ignored(row.key),
                ).length
              }
            </Button>
          ))}
          <Button size="sm" variant={filter === "failed" ? "default" : "outline"} aria-pressed={filter === "failed"} onClick={() => setFilter("failed")}><ActionIcon action="error"/>
            {t("workflow.checkFailed")} {failedRepositories.length}
          </Button>
          <Button size="sm" variant={filter === "updateFailed" ? "default" : "outline"} aria-pressed={filter === "updateFailed"} onClick={() => setFilter("updateFailed")}><ActionIcon action="error"/>
            {t("workflow.updateFailed")} {rows.filter(row => !ignored(row.key) && !!failed(row.key)).length}
          </Button>
          <Button
            size="sm"
            aria-pressed={filter === "ignored"}
            variant={filter === "ignored" ? "default" : "outline"}
            onClick={() => setFilter(filter === "ignored" ? null : "ignored")}
          ><ActionIcon action="ignore"/>
            {t("workflow.ignored")}
          </Button>
        </div>
        <div className="min-h-0 flex-1 space-y-3 overflow-auto">
          {!visible.length && !visibleFailures.length && <p className="py-8 text-center text-sm text-muted-foreground">{t(state.preview ? "workflow.noFilteredCheckResults" : "workflow.noPendingUpdates")}</p>}
          {state.error && (
            <div className="flex items-center gap-2"><p className="text-destructive">{t("workflow.operationFailed")}</p><Button size="sm" variant="ghost" onClick={() => state.dismissFailedCheck()}>{t("workflow.removeCheckResult")}</Button></div>
          )}
          {(state.preview?.repositories ?? []).map((repo) => {
            const items = visible.filter((row) => row.repo === repo.repository);
            if (repo.error ? !visibleFailures.includes(repo) : !items.length) return null;
            return (
              <section key={repo.repository} className="rounded border p-3">
                <div className="flex items-center gap-2 font-medium">
                  <input
                    type="checkbox"
                    aria-label={repo.repository}
                    checked={
                      repo.error ? !!retryChoices[repo.repository] : items.some(selectable) &&
                      items.filter(selectable).every(checked)
                    }
                    disabled={!repo.error && !items.some(selectable)}
                    onChange={(e) =>
                      repo.error ? setRetryChoices(previous => ({...previous, [repo.repository]: e.target.checked})) : setChoices((previous) => ({
                        ...previous,
                        ...Object.fromEntries(
                          items
                            .filter(selectable)
                            .map((row) => [row.pairKey, e.target.checked]),
                        ),
                      }))
                    }
                  />
                  <button
                    type="button"
                    className="flex min-w-0 flex-1 items-center gap-2 text-left"
                    aria-label={repo.repository}
                    aria-expanded={!!expanded[repo.repository]}
                    onClick={() =>
                      setExpanded((previous) => ({
                        ...previous,
                        [repo.repository]: !previous[repo.repository],
                      }))
                    }
                  >
                    <ChevronRight
                      className={`size-4 shrink-0 transition-transform ${expanded[repo.repository] ? "rotate-90" : ""}`}
                    />
                    <span className="truncate">{repo.repository}</span>
                    <span className="ml-auto shrink-0 text-xs font-normal text-muted-foreground">
                      {repo.error ? t("workflow.checkFailed") : t("workflow.selected", {
                        count: items.filter(checked).length,
                        total: items.length,
                      })}
                    </span>
                    {items.some(row => failed(row.key)) && <span className="shrink-0 text-xs text-destructive">{t("workflow.updateFailed")} {items.filter(row => failed(row.key)).length}</span>}
                  </button>
                </div>
                {state.checkedAt[repo.repository.toLowerCase()] && <p className="mt-1 text-xs text-muted-foreground">{t("workflow.repositoryCheckTime")} · {new Date(state.checkedAt[repo.repository.toLowerCase()]).toLocaleString(i18n.language)}</p>}
                {repo.error && (
                  <p className="text-sm text-destructive">
                    {taskErrorMessage(repo.error)}{" "}

                  </p>
                )}
                {embedded && repo.error && <Button size="sm" variant="outline" className="mt-2" disabled={repositoryChecking(repo.repository)} onClick={() => void state.recheckRepository(repo.repository)}><ActionIcon action="retry"/>{t("workflow.recheck")}</Button>}
                {embedded && repo.error && <Button size="sm" variant="ghost" className="mt-2" disabled={checking || repositoryChecking(repo.repository)} onClick={() => state.dismissFailedCheck(repo.repository)}><ActionIcon action="delete"/>{t("workflow.removeCheckResult")}</Button>}
                {expanded[repo.repository] &&
                  items.map((row) => (
                    <div
                      key={row.key}
                      className="mt-2 rounded border border-border bg-background p-2 text-sm"
                    >
                      <div className="flex items-center gap-2">
                        <input
                          type="checkbox"
                          aria-label={row.item.name}
                          checked={checked(row)}
                          disabled={!selectable(row)}
                          onChange={(e) =>
                            setChoices((previous) => ({
                              ...previous,
                              [row.pairKey]: e.target.checked,
                            }))
                          }
                        />
                        <span>{row.item.name}</span>
                        <span className="ml-auto text-xs">
                          {t(row.category === "updated" ? "workflow.updated" : `workflow.${embedded ? "changeType" : "change"}.${row.category}`)}
                        </span>
                        <span className="text-xs text-muted-foreground">{busy(row.key)
                          ? t(tasks.find(task => task.key === row.key && isTaskActive(task))?.cancelRequested ? "workflow.stopping" : `workflow.taskStatus.${tasks.find(task => task.key === row.key && isTaskActive(task))?.status}`)
                          : !failed(row.key) ? t(row.category === "updated" ? "workflow.taskStatus.success" : row.category === "unchanged" ? "workflow.change.unchanged" : row.category === "deleted" ? "workflow.pendingDecision" : "workflow.pendingUpdate") : ""}</span>
                        {failed(row.key) && !busy(row.key) && <span className="text-xs text-destructive">{t("workflow.updateFailed")}</span>}
                        {embedded && applicable(row) && <Button size="sm" variant="outline" onClick={() => apply([row])}>{t(failed(row.key) ? "workflow.retryTask" : row.replacement ? "workflow.replaceDeletedCompact" : row.category === "deleted" ? "workflow.removeLocal" : "workflow.updateOne")}</Button>}
                        {embedded && applicable(row) && <Button size="sm" variant="ghost" onClick={() => state.setIgnored([...new Set([...state.ignored,row.key])])}>{t("workflow.ignoreOne")}</Button>}
                        {filter === "ignored" && (
                          <Button
                            size="sm"
                            variant="ghost"
                            onClick={() =>
                              state.setIgnored(
                                state.ignored.filter((key) => key !== row.key),
                              )
                            }
                          ><ActionIcon action="ignore"/>
                            {t("workflow.restoreIgnored")}
                          </Button>
                        )}
                      </div>
                      {row.candidates.length > 0 && <div className="mt-2 space-y-1">
                        <p className="flex items-center gap-1 text-xs">{t("workflow.reimportSource")}
                          <button type="button" className="rounded text-muted-foreground focus-visible:ring-2 focus-visible:ring-ring" title={t("workflow.replaceDeletedHint")} aria-label={t("workflow.replaceDeletedHint")}><Info className="size-4"/></button>
                        </p>
                        <select aria-label={t("workflow.replacementFor",{name:row.item.name})} className="w-full rounded border bg-background p-1" disabled={busy(row.key)}
                          value={row.replacement ? updateItemKey(row.repo,row.replacement) : ""}
                          onChange={event=>setPairs(previous=>({...previous,[row.pairKey]:event.target.value}))}>
                          <option value="">{t("workflow.chooseReplacement")}</option>
                          {row.candidates.map(candidate=>{const key=updateItemKey(row.repo,candidate);return <option key={key} value={key} disabled={rows.some(other=>other.pairKey!==row.pairKey && other.repo===row.repo && other.replacement===candidate)}>{candidate.name} · {candidate.sourcePath}</option>;})}
                        </select>
                      </div>}
                      {row.category === "deleted" && !row.candidates.length && (
                        <p className="text-xs text-muted-foreground">
                          {t("workflow.remoteDeleted")}
                        </p>
                      )}
                      {failed(row.key) && (
                        <div className="flex flex-wrap items-center gap-2 text-xs text-destructive">
                          {
                            failed(row.key)
                          }

                        </div>
                      )}
                      {!!row.item.files?.length && (
                        <details className="mt-1 text-xs">
                          <summary className="cursor-pointer">
                            {t("workflow.files", {
                              count: row.item.files.length,
                            })}
                          </summary>
                          {row.item.files.map((file) => (
                            <p key={file.path}>
                              {t(`workflow.change.${file.status}`)} ·{" "}
                              {file.path}
                            </p>
                          ))}
                        </details>
                      )}
                    </div>
                  ))}
              </section>
            );
          })}
        </div>
        <div className="flex flex-wrap items-center gap-x-4 gap-y-3 border-t pt-3">
          <label className="mr-auto flex items-center gap-2 text-xs">
            <input type="checkbox" aria-label={t("workflow.selectAll")} checked={allSelected}
              aria-checked={partiallySelected ? "mixed" : allSelected}
              ref={node=>{if(node)node.indeterminate=partiallySelected;}}
              disabled={!selectableCount}
              onChange={event=>toggleAll(event.target.checked)} />
            <span>{t("workflow.selectedCompact", {count:selectedCount})}</span>
          </label>
          <div className="ml-auto flex flex-wrap items-center justify-end gap-2">
          <Button
            size="sm"
            variant="outline"
            disabled={state.isChecking || !repositoriesToRecheck.length}
            onClick={() => void state.recheckRepositories(repositoriesToRecheck)}
          ><ActionIcon action="check"/>
            {t("workflow.recheckSelected")}
          </Button>
          {visibleFailures.length > 0 && <Button size="sm" variant="outline" disabled={state.isChecking || !retryableFailures.length} onClick={()=>void state.recheckRepositories(retryableFailures.map(repo=>repo.repository))}><ActionIcon action="retry"/>{t("workflow.retry")}</Button>}
          {!embedded && <Button size="sm" variant="outline" disabled={checking} onClick={()=>setConfirmCheck(true)}><ActionIcon action="check"/>{t("workflow.checkAllCompact")}</Button>}
          </div>
          <div className="flex flex-wrap items-center justify-end gap-2 border-l pl-4">
          {visible.some(row => failed(row.key)) && <Button size="sm" variant="outline" disabled={!updatesToRetry.length} onClick={() => apply(updatesToRetry)}><ActionIcon action="retry"/>{t("workflow.retryUpdates")}</Button>}
          <Button size="sm" variant="outline" disabled={!updatesToApply.length} onClick={()=>state.setIgnored([...new Set([...state.ignored,...updatesToApply.map(row=>row.key)])])}><ActionIcon action="ignore"/>{t("workflow.ignore")}</Button>
          {visible.some(row => row.candidates.length > 0) && <Button size="sm" variant="outline" title={t("workflow.replaceDeleted")} disabled={!replacementsToApply.length} onClick={()=>apply(replacementsToApply)}><ActionIcon action="delete"/>{t("workflow.replaceDeletedCompact")}</Button>}
          <Button size="sm" disabled={!regularUpdatesToApply.length} onClick={()=>apply()}><ActionIcon action="update"/>
            {t("workflow.apply")}
          </Button>
          </div>
        </div>
    </UpdateFrame>
    </>
  );
}

function UpdateFrame({embedded, children}: {embedded: boolean; children: ReactNode}) {
  const {t} = useTranslation();
  const state = useRepositorySyncStore();
  if (embedded) return <div className="flex min-h-0 flex-1 flex-col gap-3">{children}</div>;
  return <Dialog open={state.open} onOpenChange={state.setOpen}>
    <DialogContent className="flex h-[min(46rem,calc(100dvh-2rem))] w-[min(56rem,calc(100vw-2rem))] max-w-none flex-col overflow-hidden resize">
      <DialogHeader><DialogTitle>{t("workflow.updateStatus")}</DialogTitle></DialogHeader>
      {children}
    </DialogContent>
  </Dialog>;
}
