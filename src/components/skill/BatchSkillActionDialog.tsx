import { ActionIcon } from "@/components/ui/action-icon";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { Dialog, DialogContent, DialogHeader, DialogTitle, DialogDescription, DialogFooter } from "@/components/ui/dialog";
import { Button } from "@/components/ui/button";
import { InstallTargetList } from "@/components/central/InstallTargetList";
import type { AgentWithStatus } from "@/types";

export type BatchAction = "install" | "update" | "delete" | "uninstall" | "reimport" | "restore" | "purge";
export type BatchOperations = Partial<Record<BatchAction, (targets: string[]) => Promise<unknown>>>;
export interface BatchEntry { key: string; name: string; operations?: BatchOperations; agents?: AgentWithStatus[] }
export function BatchSkillActionDialog({action, entries, onClose}: {action:BatchAction; entries:BatchEntry[]; onClose:()=>void}) {
  const {t} = useTranslation();
  const [targets, setTargets] = useState<Set<string>>(new Set());
  const [running, setRunning] = useState(false);
  const [done, setDone] = useState(false);
  const [results, setResults] = useState<{name:string; error?:string}[]>([]);
  const eligible = entries.filter(entry=>entry.operations?.[action]);
  const agents = [...new Map(entries.flatMap(entry=>entry.agents ?? []).map(agent=>[agent.id,agent])).values()];
  async function apply() {
    if(running || done) return;
    setRunning(true);
    for (const entry of eligible) {
      try {
        const result = await entry.operations![action]!([...targets]);
        if(result && typeof result === "object" && "failed" in result && Array.isArray(result.failed) && result.failed.length) throw new Error(t("browser.batchPartialFailure",{count:result.failed.length}));
        setResults(previous=>[...previous,{name:entry.name}]);
      } catch(error) { setResults(previous=>[...previous,{name:entry.name,error:String(error)}]); }
    }
    setRunning(false); setDone(true);
  }
  return <Dialog open onOpenChange={open=>{if(!open && !running)onClose();}}><DialogContent>
    <DialogHeader><DialogTitle>{t(`browser.batch.${action}`)}</DialogTitle><DialogDescription>{t("browser.batchSelection",{count:eligible.length, skipped:entries.length-eligible.length})}{action === "delete" || action === "uninstall" ? ` ${t("browser.batchRemovalWarning")}` : ["reimport","restore","purge"].includes(action) ? ` ${t(`trash.${action}Warning`)}` : ""}</DialogDescription></DialogHeader>
    <div className="max-h-[50vh] space-y-3 overflow-auto">
      <ul className="space-y-1 text-sm">{eligible.map(entry=><li key={entry.key} className="break-all">{entry.name}</li>)}</ul>
      {action === "install" && <InstallTargetList agents={agents} selectedAgentIds={targets} ariaLabel={t("resource.installToTargetsAction")} onToggleAgent={(id,checked)=>setTargets(previous=>{const next=new Set(previous);if(checked)next.add(id);else next.delete(id);return next;})}/>}
      {done&&action==="reimport"&&<p className="text-xs text-muted-foreground">{t("trash.reimportQueued")}</p>}
      {(running || done) && <p className="text-xs text-muted-foreground">{results.length} / {eligible.length}</p>}
      {results.filter(result=>result.error).map((result,index)=><p key={index} className="text-xs text-destructive">{result.name}: {result.error}</p>)}
    </div>
    <DialogFooter className="flex-row justify-end"><Button variant={action==="purge"?"destructive":"default"} onClick={()=>void apply()} disabled={running || done || !eligible.length || (action==="install"&&!targets.size)}><ActionIcon action="confirm"/>{t("common.confirm")}</Button><Button variant="outline" onClick={onClose} disabled={running}><ActionIcon action="cancel"/>{t(done ? "common.close":"common.cancel")}</Button></DialogFooter>
  </DialogContent></Dialog>;
}
