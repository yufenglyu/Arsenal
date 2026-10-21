import {useEffect, useMemo, useState} from "react";
import {useTranslation} from "react-i18next";
import {RotateCw, Trash2, Undo2} from "lucide-react";
import {useResourceTrashStore} from "@/stores/resourceTrashStore";
import {SkillBrowserTable, type FolderTableItem} from "@/components/skill/SkillBrowserTable";
import {SkillBrowserHeader} from "@/components/skill/SkillBrowserHeader";
import {Button} from "@/components/ui/button";
import {SearchInput} from "@/components/ui/search-input";
import {BatchSkillActionDialog, type BatchAction, type BatchEntry} from "@/components/skill/BatchSkillActionDialog";

export function ResourceTrashView(){
  const {t}=useTranslation();const {entries,loading,error,load,restore,purge}=useResourceTrashStore();
  const [query,setQuery]=useState("");const [selected,setSelected]=useState<string[]>([]);
  const [request,setRequest]=useState<{action:BatchAction;entries:BatchEntry[]}|null>(null);
  useEffect(()=>{void load();},[load]);
  const rows=useMemo(()=>entries.filter(entry=>entry.name.toLowerCase().includes(query.trim().toLowerCase())),[entries,query]);
  const actions=(ids:string[])=>entries.filter(entry=>ids.includes(entry.id)).map(entry=>({key:entry.id,name:entry.name,operations:{restore:()=>restore(entry.id),purge:()=>purge([entry.id])}}));
  const selectedIds=selected.filter(key=>key.startsWith("folder:")).map(key=>key.slice(7));
  return <div className="flex h-full min-h-0 flex-col">
    <SkillBrowserHeader title={<h1>{t("trash.title")}</h1>} search={<SearchInput value={query} onValueChange={setQuery} placeholder={t("trash.search")} aria-label={t("trash.search")}/>}
      actions={<><Button variant="ghost" size="icon" aria-label={t("common.refresh")} disabled={loading} onClick={()=>void load()}><RotateCw/></Button>
        <Button variant="outline" disabled={!selectedIds.length} onClick={()=>setRequest({action:"restore",entries:actions(selectedIds)})}><Undo2/>{t("trash.restoreSelected")}</Button>
        <Button variant="destructive" disabled={!entries.length} onClick={()=>setRequest({action:"purge",entries:actions(entries.map(entry=>entry.id))})}><Trash2/>{t("trash.empty")}</Button></>}/>
    {error&&<p role="alert" className="px-4 pt-4 text-sm text-destructive">{error}</p>}
    <div className="min-h-0 flex-1 overflow-auto">
      {rows.length?<SkillBrowserTable kind="folder" visibleColumns={new Set(["index","name","skillCount","updatedAt"])} columnLabels={{updatedAt:t("trash.deletedAt")}}
        onSelectionChange={setSelected} folders={rows.map((entry):FolderTableItem=>({key:entry.id,name:entry.name,path:"",skillCount:entry.skillCount,updatedAt:entry.deletedAt,onOpen:()=>{},expandable:false,
          batchOperations:actions([entry.id])[0]?.operations,restore:()=>restore(entry.id),purge:()=>purge([entry.id]),
        }))}/>:<p className="p-4 text-center text-sm text-muted-foreground">{loading?t("common.loading"):t(query?"browser.noMatches":"trash.emptyState")}</p>}
    </div>
    <div className="border-t border-border px-4 py-2 text-xs text-muted-foreground">{t("trash.retention")}</div>
    {request&&<BatchSkillActionDialog {...request} onClose={()=>setRequest(null)}/>}
  </div>;
}
