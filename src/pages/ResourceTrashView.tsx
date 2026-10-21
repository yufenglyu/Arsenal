import {useContext, useEffect, useLayoutEffect, useMemo, useRef, useState} from "react";
import {useTranslation} from "react-i18next";
import {RotateCw, Trash2} from "lucide-react";
import {StatusBarSlot} from "@/components/layout/StatusBarSlot";
import {useConfiguredHotkey} from "@/hooks/useConfiguredHotkey";
import {useBrowserStatusStore} from "@/stores/browserStatusStore";
import {useResourceTrashStore} from "@/stores/resourceTrashStore";
import {SkillBrowserTable, type FolderTableItem} from "@/components/skill/SkillBrowserTable";
import {SkillBrowserHeader} from "@/components/skill/SkillBrowserHeader";
import {Button} from "@/components/ui/button";
import {SearchInput} from "@/components/ui/search-input";
import {BatchSkillActionDialog, type BatchAction, type BatchEntry} from "@/components/skill/BatchSkillActionDialog";

export function ResourceTrashView(){
  const {t}=useTranslation();const {entries,loading,error,load,restore,purge}=useResourceTrashStore();
  const [query,setQuery]=useState("");
  const statusSlot=useContext(StatusBarSlot);
  const list=useRef<HTMLDivElement>(null);
  const [scrollbarWidth,setScrollbarWidth]=useState(0);
  const [request,setRequest]=useState<{action:BatchAction;entries:BatchEntry[]}|null>(null);
  useEffect(()=>{void load();},[load]);
  const rows=useMemo(()=>entries.filter(entry=>entry.name.toLowerCase().includes(query.trim().toLowerCase())),[entries,query]);
  const actions=(ids:string[])=>entries.filter(entry=>ids.includes(entry.id)).map(entry=>({key:entry.id,name:entry.name,operations:{restore:()=>restore(entry.id),purge:()=>purge([entry.id])}}));
  useConfiguredHotkey("refreshSkillList",()=>{if(!loading&&!document.querySelector('[role="dialog"]'))void load(true);},{allowInEditable:true});
  useLayoutEffect(()=>{
    const element=list.current;if(!element)return;
    const header=element.querySelector("thead");
    const measure=()=>{
      setScrollbarWidth(Math.max(0,element.offsetWidth-element.clientWidth));
      element.style.setProperty("--skill-table-header-height",`${header?header.getBoundingClientRect().height+2:0}px`);
    };
    measure();if(typeof ResizeObserver==="undefined")return;
    const observer=new ResizeObserver(measure);observer.observe(element);if(header)observer.observe(header);
    return()=>observer.disconnect();
  },[rows.length]);
  useEffect(()=>{if(!rows.length)useBrowserStatusStore.getState().setStats({path:"/trash",collections:false,groups:0,skills:0,selected:0,selectedGroups:0,installed:0});},[rows.length]);
  return <div className="flex h-full min-h-0 flex-col">
    <div className="flex h-11 shrink-0 items-center border-b border-border [&>header]:h-full [&>header]:flex-1 [&>header]:border-b-0" style={{paddingRight:scrollbarWidth}}><SkillBrowserHeader title={<h1>{t("trash.title")}</h1>} search={<SearchInput value={query} onValueChange={setQuery} placeholder={t("trash.search")} aria-label={t("trash.search")}/>}
      actions={<><Button variant="ghost" size="icon" aria-label={t("common.refresh")} disabled={loading} onClick={()=>void load(true)}><RotateCw/></Button>
        <Button variant="destructive" size="icon" title={t("trash.empty")} aria-label={t("trash.empty")} disabled={!entries.length} onClick={()=>setRequest({action:"purge",entries:actions(entries.map(entry=>entry.id))})}><Trash2/></Button></>}/></div>
    {error&&<p role="alert" className="px-4 pt-4 text-sm text-destructive">{error}</p>}
    <div ref={list} className="skill-browser-list min-h-0 flex-1 overflow-y-auto overflow-x-hidden [overflow-anchor:none]">
      {rows.length?<SkillBrowserTable kind="folder" tree statusPath="/trash" stickyHeaderTop="0px" className="rounded-none border-0 shadow-none" visibleColumns={new Set(["index","name","skillCount","updatedAt"])} columnLabels={{updatedAt:t("trash.deletedAt")}}
        folders={rows.map((entry):FolderTableItem=>({key:entry.id,name:entry.name,path:"",skillCount:entry.skillCount,updatedAt:entry.deletedAt,onOpen:()=>{},expandable:false,
          batchOperations:actions([entry.id])[0]?.operations,restore:()=>restore(entry.id),purge:()=>purge([entry.id]),
        }))}/>:<p className="p-8 text-center text-sm text-muted-foreground">{loading?t("common.loading"):t(query?"browser.noMatches":"trash.emptyState")}</p>}
    </div>
    <div ref={statusSlot} className="shrink-0" data-testid="browser-status-slot"/>
    {request&&<BatchSkillActionDialog {...request} onClose={()=>setRequest(null)}/>}
  </div>;
}
