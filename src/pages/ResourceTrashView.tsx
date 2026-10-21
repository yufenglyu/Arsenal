import {useEffect, useMemo, useState} from "react";
import {useTranslation} from "react-i18next";
import {RotateCw, Trash2} from "lucide-react";
import {useResourceTrashStore} from "@/stores/resourceTrashStore";
import {type FolderTableItem} from "@/components/skill/SkillBrowserTable";
import {SkillBrowserWorkspace} from "@/components/skill/SkillBrowserWorkspace";
import {MetadataRow} from "@/components/skill/SkillDetailView";
import {SkillBrowserHeader} from "@/components/skill/SkillBrowserHeader";
import {Button} from "@/components/ui/button";
import {SearchInput} from "@/components/ui/search-input";
import {BatchSkillActionDialog, type BatchAction, type BatchEntry} from "@/components/skill/BatchSkillActionDialog";

export function ResourceTrashView(){
  const {t,i18n}=useTranslation();const {entries,loading,error,load,restore,purge}=useResourceTrashStore();
  const [query,setQuery]=useState("");
  const [request,setRequest]=useState<{action:BatchAction;entries:BatchEntry[]}|null>(null);
  useEffect(()=>{void load();},[load]);
  const rows=useMemo(()=>entries.filter(entry=>entry.name.toLowerCase().includes(query.trim().toLowerCase())),[entries,query]);
  const actions=(ids:string[])=>entries.filter(entry=>ids.includes(entry.id)).map(entry=>({key:entry.id,name:entry.name,operations:{restore:()=>restore(entry.id),purge:()=>purge([entry.id])}}));
  const folders=rows.map((entry):FolderTableItem=>({
    key:entry.id,name:entry.name,path:"",skillCount:entry.skillCount,updatedAt:entry.deletedAt,onOpen:()=>{},expandable:false,overviewOnly:true,
    batchOperations:actions([entry.id])[0]?.operations,restore:()=>restore(entry.id),purge:()=>purge([entry.id]),
    metadata:<>
      <MetadataRow label={t("detail.sourceRepo")} value={entry.repositories.join(", ")||"—"}/>
      <MetadataRow label={t("skillBrowser.columns.skillCount")} value={String(entry.skillCount)}/>
      <MetadataRow label={t("trash.deletedAt")} value={new Date(entry.deletedAt).toLocaleString(i18n.language)}/>
    </>,
  }));
  return <div className="flex h-full min-h-0 flex-col">
    {error&&<p role="alert" className="px-4 pt-4 text-sm text-destructive">{error}</p>}
    <SkillBrowserWorkspace storageKey="trash" allowFlatView={false} loading={loading} folders={folders}
      visibleColumns={new Set(["index","name","skillCount","updatedAt"])} columnLabels={{updatedAt:t("trash.deletedAt")}}
      emptyMessage={t(query?"browser.noMatches":"trash.emptyState")} onRefresh={()=>load(true)}
      toolbar={()=> <SkillBrowserHeader title={<h1>{t("trash.title")}</h1>} search={<SearchInput value={query} onValueChange={setQuery} placeholder={t("trash.search")} aria-label={t("trash.search")}/>}
        actions={<><Button variant="ghost" size="icon" aria-label={t("common.refresh")} disabled={loading} onClick={()=>void load(true)}><RotateCw/></Button>
          <Button variant="ghost" size="icon" title={t("trash.empty")} aria-label={t("trash.empty")} disabled={!entries.length} onClick={()=>setRequest({action:"purge",entries:actions(entries.map(entry=>entry.id))})}><Trash2/></Button></>}/>}/>
    {request&&<BatchSkillActionDialog {...request} onClose={()=>setRequest(null)}/>}
  </div>;
}
