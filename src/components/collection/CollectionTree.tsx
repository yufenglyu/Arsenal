import {useResourceTrashStore} from "@/stores/resourceTrashStore";
import {splitResourceLibrarySkillsByFolder} from "@/lib/skillFolders";
import {useEffect, useLayoutEffect, useMemo, useRef, useState, type DragEvent} from "react";
import {useNavigate,useLocation} from "react-router-dom";
import {useTranslation} from "react-i18next";
import {ChevronRight, Folder, Inbox, Plus, Minus, FolderPlus, Pencil, FileDown, Trash2, Check, ListTree} from "lucide-react";
import {toast} from "sonner";
import {useCollectionStore} from "@/stores/collectionStore";
import {useResourceLibraryStore} from "@/stores/resourceLibraryStore";
import {useExpansionShortcuts} from "@/hooks/useExpansionShortcuts";
import {collectionMembers, collectionScope, COLLECTION_DRAG, SKILLS_DRAG, REPOSITORIES_DRAG, COLLECTION_SOURCE_DRAG, UNFILED_COLLECTION} from "@/lib/collectionGroups";
import {Button} from "@/components/ui/button";
import {Input} from "@/components/ui/input";
import {Dialog, DialogContent, DialogHeader, DialogTitle, DialogBody, DialogFooter} from "@/components/ui/dialog";
import {cn} from "@/lib/utils";
import type {Collection} from "@/types";
import {shouldIgnoreShortcutTarget} from "@/lib/shortcutKeys";
import {SkillRepositoryIcon} from "@/components/skill/SkillActionIcons";
import {save} from "@tauri-apps/plugin-dialog";

export function CollectionTree({count, onExport, onExpandedChange}: {count?: number; onExport?: () => void; onExpandedChange?: (expanded: boolean) => void}) {
  const {t}=useTranslation(); const navigate=useNavigate(); const {pathname}=useLocation(); const store=useCollectionStore(s=>s);
  const skills=useResourceLibraryStore(s=>s.skills);
  const libraryDir=useResourceLibraryStore(s=>s.resourceLibraryDir);
  const trashEntries=useResourceTrashStore(s=>s.entries);const loadTrash=useResourceTrashStore(s=>s.load);
  const trashRevision=skills.map(skill=>skill.id).join("\0");
  useEffect(()=>{void loadTrash(true);},[trashRevision,loadTrash]);
  const trashRepositories=new Set(trashEntries.flatMap(entry=>entry.repositories)).size;
  const trashSkills=trashEntries.reduce((total,entry)=>total+entry.skillCount,0);
  const repositoryBySkill=useMemo(()=>{
    const keys=new Map<string,string>();
    const folders=splitResourceLibrarySkillsByFolder(skills,libraryDir).groups;
    for(const folder of folders)for(const skill of folder.skills)keys.set(skill.id,skill.source_repo?`source:${skill.source_repo.toLowerCase()}`:`folder:${folder.relativePath}`);
    for(const skill of skills)if(skill.source_repo)keys.set(skill.id,`source:${skill.source_repo.toLowerCase()}`);
    return keys;
  },[skills,libraryDir]);
  const tree=useRef<HTMLDivElement>(null); const menuRef=useRef<HTMLDivElement>(null);
  const [closed,setClosed]=useState<Set<string>>(()=>{try{return new Set(JSON.parse(localStorage.getItem('arsenal.groups.closed')??'[]'));}catch{return new Set();}});
  const [rootOpen,setRootOpen]=useState(()=>{try{return localStorage.getItem('arsenal.groups.open')!=='false';}catch{return true;}});
  useEffect(()=>{try{localStorage.setItem('arsenal.groups.open',String(rootOpen));}catch{/* Keep session state. */}},[rootOpen]);
  const [controlHeld,setControlHeld]=useState(false);
  const [menu,setMenu]=useState<{item:Collection|null;x:number;y:number}|null>(null);
  const [edit,setEdit]=useState<{action:'create'|'rename'|'delete';item?:Collection;parentId?:string|null}|null>(null);
  const [name,setName]=useState('');const [busy,setBusy]=useState(false);
  const [drop,setDrop]=useState<{id:string;zone:'before'|'inside'|'after'}|null>(null);
  const {collections,currentDetail,loadGroupDetails}=store;
  const groupError=t('groups.operationFailed');
  useEffect(()=>{void loadGroupDetails().catch(()=>toast.error(groupError));},[collections,currentDetail,loadGroupDetails,groupError]);
  useEffect(()=>{try{localStorage.setItem('arsenal.groups.closed',JSON.stringify([...closed]));}catch{/* Keep session state. */}},[closed]);
  useEffect(()=>{
    const down=(e:KeyboardEvent)=>{if(e.key==='Control'||e.key==='Meta')setControlHeld(true);};
    const up=(e:KeyboardEvent)=>{if(!e.ctrlKey&&!e.metaKey)setControlHeld(false);};const clear=()=>setControlHeld(false);
    window.addEventListener('keydown',down);window.addEventListener('keyup',up);window.addEventListener('blur',clear);
    return()=>{window.removeEventListener('keydown',down);window.removeEventListener('keyup',up);window.removeEventListener('blur',clear);};
  },[]);
  useEffect(()=>{if(!menu)return;const close=()=>setMenu(null);window.addEventListener('click',close);return()=>window.removeEventListener('click',close);},[menu]);
  useLayoutEffect(()=>{if(!menu||!menuRef.current)return;const el=menuRef.current;const bounds=el.getBoundingClientRect();el.style.left=`${Math.max(4,Math.min(menu.x,window.innerWidth-bounds.width-4))}px`;el.style.top=`${Math.max(4,Math.min(menu.y,window.innerHeight-bounds.height-4))}px`;el.querySelector<HTMLButtonElement>('button')?.focus();},[menu]);
  const expandAll=()=>{setRootOpen(true);setClosed(new Set());};
  const collapseAll=()=>{setClosed(new Set(store.collections.map(item=>item.id)));tree.current?.querySelector<HTMLButtonElement>('button')?.focus();};
  const expansionKeys=useExpansionShortcuts(expandAll,collapseAll);
  useEffect(()=>{
    const handler=(event:KeyboardEvent)=>{
      const target=event.target instanceof HTMLElement?event.target:null;
      if(pathname!=='/resources'||!target?.closest('[data-testid="skill-browser-workspace"]')||target.closest('[role="dialog"],[role="menu"],[role="separator"]')||shouldIgnoreShortcutTarget(target)||event.defaultPrevented||event.isComposing)return;
      if(!store.activeCollectionId||store.activeCollectionId===UNFILED_COLLECTION||event.ctrlKey||event.metaKey||event.shiftKey||event.altKey)return;
      if(event.key==='ArrowLeft'||event.key==='ArrowRight'){event.preventDefault();event.stopPropagation();setClosed(old=>{const next=new Set(old);if(event.key==='ArrowRight')next.delete(store.activeCollectionId!);else next.add(store.activeCollectionId!);return next;});}
    };
    window.addEventListener('keydown',handler,true);return()=>window.removeEventListener('keydown',handler,true);
  },[collections,store.activeCollectionId,pathname]);
  const memberGroups=new Set(controlHeld&&store.selectedSkillIds.length===1?store.groupDetails.filter(item=>item.skills.some(skill=>skill.id===store.selectedSkillIds[0])).map(item=>item.id):[]);
  const revealed=new Set<string>();
  for(const id of memberGroups){let parent=store.collections.find(item=>item.id===id)?.parent_id;while(parent&&!revealed.has(parent)){revealed.add(parent);parent=store.collections.find(item=>item.id===parent)?.parent_id;}}
  const rootExpanded=rootOpen||memberGroups.size>0;
  useEffect(()=>{onExpandedChange?.(rootExpanded);},[rootExpanded,onExpandedChange]);
  const assigned=new Set(store.groupDetails.flatMap(item=>item.skills.map(skill=>skill.id)));
  function choose(id:string|null){store.selectGroup(id);navigate('/resources');}
  function open(action:'create'|'rename'|'delete',item?:Collection,parentId?:string|null){setName(action==='rename'?item?.name??'':'');setEdit({action,item,parentId});setMenu(null);}
  async function change(request:Parameters<typeof store.changeGroup>[0]) {try{await store.changeGroup(request);return true;}catch{toast.error(t('groups.operationFailed'));return false;}}
  async function submit(){if(!edit)return;setBusy(true);try{if(await change({action:edit.action,id:edit.item?.id,name:name.trim(),parentId:edit.parentId}))setEdit(null);}finally{setBusy(false);}}
  async function exportGroup(item:Collection){
    setMenu(null);
    try{const path=await save({title:t('groups.export'),defaultPath:'skill-group.csv',filters:[{name:'CSV',extensions:['csv']}]});if(!path)return;
      const ids=[...collectionMembers(store.groupDetails,collectionScope(collections,item.id,store.includeDescendants))];
      await useResourceLibraryStore.getState().exportDirectoryList(path,ids);
    }catch{toast.error(t('groups.operationFailed'));}
  }
  function acceptDrop(e:DragEvent,id:string,zone:'before'|'inside'|'after') {
    e.preventDefault();e.stopPropagation();setDrop(null);
    const dragged=e.dataTransfer.getData(COLLECTION_DRAG);
    if(dragged){
      if(dragged===id)return;
      const target=store.collections.find(item=>item.id===id);
      const siblings=store.collections.filter(item=>(item.parent_id??null)===(target?.parent_id??null)&&item.id!==dragged);
      const next=siblings[siblings.findIndex(item=>item.id===id)+1];
      void change({action:'move',id:dragged,parentId:zone==='inside'?(id||null):target?.parent_id??null,beforeId:zone==='before'?id:zone==='after'?next?.id:null});
    }else if(id){
      try{const ids:unknown=JSON.parse(e.dataTransfer.getData(SKILLS_DRAG));if(!Array.isArray(ids)||!ids.length||!ids.every(value=>typeof value==='string'))return;
        let skillIds:string[]=ids;
        let repositories:string[]|undefined;
        const stable=e.dataTransfer.getData(REPOSITORIES_DRAG);
        if(stable){
          const payload:unknown=JSON.parse(stable);
          if(!payload||typeof payload!=='object'||!('repositories' in payload)||!('skillIds' in payload))return;
          const repos=payload.repositories;const members=payload.skillIds;
          if(!Array.isArray(repos)||!repos.length||!repos.every(value=>typeof value==='string'&&value.trim())||!Array.isArray(members)||!members.every(value=>typeof value==='string'))return;
          repositories=repos;skillIds=members;
        }
        const source=e.dataTransfer.getData(COLLECTION_SOURCE_DRAG);
        if(e.shiftKey&&source===id)return;
        void change({action:e.shiftKey&&source?'transfer':'assign',id,skillIds,...(repositories?{repositories}:{}),sourceCollectionId:e.shiftKey&&source?source:undefined});
      }catch{/* Ignore unrelated drag data. */}
    }
    setClosed(old=>new Set([...old].filter(value=>value!==id)));
  }
  function statistics(members?:Set<string>,total?:number) {
    const selected=members?skills.filter(skill=>members.has(skill.id)):skills;
    const repositories=new Set(selected.flatMap(skill=>{const key=repositoryBySkill.get(skill.id);return key?[key]:[];})).size;
    const skillCount=total??selected.length;
    return <span data-repository-count className="shrink-0 whitespace-nowrap text-xs font-normal tabular-nums" title={t('groups.countSummary',{repositories,skills:skillCount})}>{repositories}/{skillCount}</span>;
  }
  function render(parentId:string|null,depth:number):React.ReactNode {
    return store.collections.filter(item=>(item.parent_id??null)===parentId).map(item=>{
      const hasChildren=store.collections.some(child=>child.parent_id===item.id);
      const expanded=!closed.has(item.id)||revealed.has(item.id);
      const members=collectionMembers(store.groupDetails,collectionScope(store.collections,item.id,store.includeDescendants));
      return <div role="treeitem" key={item.id} data-group-id={item.id} aria-label={item.name} aria-selected={store.activeCollectionId===item.id} aria-expanded={hasChildren?expanded:undefined}>
        <div data-group-row draggable onDragStart={e=>{e.stopPropagation();e.dataTransfer.setData(COLLECTION_DRAG,item.id);e.dataTransfer.effectAllowed='move';}} onDragEnd={()=>setDrop(null)}
          onDragOver={e=>{if(!e.dataTransfer.types.some(type=>[COLLECTION_DRAG,SKILLS_DRAG].includes(type)))return;e.preventDefault();e.stopPropagation();const members=e.dataTransfer.types.includes(SKILLS_DRAG);e.dataTransfer.dropEffect=members&&!e.shiftKey?'copy':'move';const box=e.currentTarget.getBoundingClientRect();const ratio=(e.clientY-box.top)/box.height;setDrop({id:item.id,zone:members?'inside':ratio<.25?'before':ratio>.75?'after':'inside'});}}
          onDragLeave={e=>{if(!(e.relatedTarget instanceof Node)||!e.currentTarget.contains(e.relatedTarget))setDrop(null);}}
          onDrop={e=>acceptDrop(e,item.id,drop?.id===item.id?drop.zone:'inside')}
          onContextMenu={e=>{e.preventDefault();setMenu({item,x:e.clientX,y:e.clientY});}}
          className={cn('flex items-center rounded-md border-y-2 border-transparent py-1 text-sm',store.activeCollectionId===item.id?'bg-primary/15 text-primary':'hover:bg-primary/10',memberGroups.has(item.id)&&'ring-1 ring-inset ring-primary bg-primary/20',drop?.id===item.id&&(drop.zone==='before'?'border-t-primary':drop.zone==='after'?'border-b-primary':'bg-primary/20'))} style={{paddingLeft:4+depth*16}}>
          <button tabIndex={-1} disabled={!hasChildren} className="inline-flex size-5 shrink-0 items-center justify-center" aria-label={t(expanded?'groups.collapse':'groups.expand',{name:item.name})} onClick={()=>setClosed(old=>{const next=new Set(old);if(next.has(item.id))next.delete(item.id);else next.add(item.id);return next;})}>{hasChildren&&<ChevronRight className={cn('size-3.5',expanded&&'rotate-90')}/>}</button>
          <button data-group-select className="inline-flex min-w-0 flex-1 items-center gap-2 pr-2 text-left" onClick={e=>{e.currentTarget.focus();choose(item.id);}}><Folder className="size-3.5 shrink-0"/><span className="min-w-0 flex-1 truncate">{item.name}</span>{statistics(members)}</button>
        </div>{hasChildren&&expanded&&<div role="group">{render(item.id,depth+1)}</div>}
      </div>;
    });
  }
  return <>
    <div ref={tree} role="tree" aria-label={t('sidebar.resourceLibrary')} className="py-1" onKeyDown={e=>{
      expansionKeys(e);if(e.defaultPrevented||e.nativeEvent.isComposing||(e.target as HTMLElement).closest('input,textarea,[role=menu]'))return;
      const node=(e.target as HTMLElement).closest<HTMLElement>('[data-group-id]');const id=node?.dataset.groupId;
      if(e.key==='ContextMenu'||(e.shiftKey&&e.key==='F10')){e.preventDefault();const rect=(e.target as HTMLElement).getBoundingClientRect();setMenu({item:store.collections.find(item=>item.id===id)??null,x:rect.left,y:rect.bottom});return;}
      if(e.ctrlKey||e.metaKey||e.altKey||e.shiftKey)return;
      if(e.key==='ArrowDown'||e.key==='ArrowUp'){e.preventDefault();const rows=Array.from(tree.current!.querySelectorAll<HTMLButtonElement>('[data-group-select]'));const index=rows.indexOf(e.target as HTMLButtonElement);const next=rows[Math.max(0,Math.min(rows.length-1,index+(e.key==='ArrowDown'?1:-1)))];next?.click();next?.scrollIntoView?.({block:'nearest'});}
      if(e.key==='ArrowLeft'||e.key==='ArrowRight'){e.preventDefault();if(id)setClosed(old=>{const next=new Set(old);if(e.key==='ArrowRight')next.delete(id);else next.add(id);return next;});else setRootOpen(e.key==='ArrowRight');}
    }}>
      <div className="flex items-center" onDragOver={e=>{if(e.dataTransfer.types.includes(COLLECTION_DRAG))e.preventDefault();}} onDrop={e=>acceptDrop(e,'','inside')} onContextMenu={e=>{e.preventDefault();setMenu({item:null,x:e.clientX,y:e.clientY});}}>
        <button data-group-select aria-expanded={rootExpanded} aria-label={t('sidebar.resourceLibrary')} aria-current={(pathname==='/resources'||pathname==='/')&&!store.activeCollectionId?'page':undefined} className={cn('flex min-w-0 flex-1 items-center gap-2 rounded-md pl-1 pr-2 py-1 text-sm font-semibold', (pathname==='/resources'||pathname==='/')&&!store.activeCollectionId?'bg-hover-bg text-white':'hover:bg-primary/10')} onClick={e=>{e.currentTarget.focus();choose(null);}} onDoubleClick={()=>setRootOpen(!rootExpanded)} onKeyDown={e=>{if(e.key==="Enter"||e.key===" "){e.preventDefault();e.stopPropagation();setRootOpen(!rootExpanded);choose(null);}}}><SkillRepositoryIcon className="size-3.5 shrink-0"/><span className="min-w-0 flex-1 truncate text-left">{t('sidebar.resourceLibrary')}</span>{statistics(undefined,count)}</button>
      </div>
      {rootExpanded&&<div>{render(null,0)}<button data-group-select className={cn('flex w-full items-center gap-2 rounded-md pl-6 pr-2 py-1 text-sm',store.activeCollectionId===UNFILED_COLLECTION&&'bg-primary/15 text-primary')} onClick={e=>{e.currentTarget.focus();choose(UNFILED_COLLECTION);}}><Inbox className="size-3.5"/><span className="flex-1 text-left">{t('groups.unfiled')}</span>{statistics(new Set(skills.filter(skill=>!assigned.has(skill.id)).map(skill=>skill.id)))}</button><button data-group-select className={cn('flex w-full items-center gap-2 rounded-md pl-6 pr-2 py-1 text-sm',pathname==='/trash'&&'bg-primary/15 text-primary')} onClick={()=>{store.selectGroup(null);navigate('/trash');}}><Trash2 className="size-3.5 shrink-0"/><span className="flex-1 text-left">{t('trash.title')}</span><span className="shrink-0 text-xs font-normal tabular-nums" title={t('groups.countSummary',{repositories:trashRepositories,skills:trashSkills})}>{trashRepositories}/{trashSkills}</span></button></div>}
    </div>
    {menu&&<div ref={menuRef} role="menu" className="fixed z-50 min-w-44 rounded-md border border-border bg-popover p-1 shadow-md" style={{left:menu.x,top:menu.y}} onClick={e=>e.stopPropagation()} onKeyDown={e=>{if(e.key==='Escape'){setMenu(null);tree.current?.querySelector<HTMLButtonElement>('button')?.focus();}if(e.key==='ArrowDown'||e.key==='ArrowUp'){e.preventDefault();const buttons=Array.from(e.currentTarget.querySelectorAll<HTMLButtonElement>('button:not(:disabled)'));const i=buttons.indexOf(document.activeElement as HTMLButtonElement);buttons[(i+(e.key==='ArrowDown'?1:-1)+buttons.length)%buttons.length]?.focus();}}}>
      <div role="group" aria-label={t('groups.menu.organize')}>
        <button role="menuitem" className="group-menu-item" onClick={()=>open('create',undefined,menu.item?.id??null)}><FolderPlus/>{t(menu.item?'groups.createChild':'groups.create')}</button>
        {menu.item&&<button role="menuitem" className="group-menu-item" onClick={()=>open('rename',menu.item!)}><Pencil/>{t('groups.rename')}</button>}
      </div>
      {(menu.item||onExport)&&<><div role="separator" className="my-1 border-t border-border"/><div role="group" aria-label={t('groups.menu.export')}>
        <button role="menuitem" className="group-menu-item" onClick={()=>{if(menu.item)void exportGroup(menu.item);else{setMenu(null);onExport?.();}}}><FileDown/>{t(menu.item?'groups.export':'resource.exportDirectoryList')}</button>
      </div></>}
      <div role="separator" className="my-1 border-t border-border"/>
      <div role="group" aria-label={t('groups.menu.view')}>
        <button role="menuitemcheckbox" aria-checked={store.includeDescendants} className="group-menu-item" onClick={()=>{try{localStorage.setItem('arsenal.groups.includeDescendants',String(!store.includeDescendants));}catch{/* Keep session state. */}useCollectionStore.setState({includeDescendants:!store.includeDescendants});setMenu(null);}}><ListTree/>{t('groups.includeChildren')}<Check className={cn('ml-auto',!store.includeDescendants&&'invisible')}/></button>
        <button role="menuitem" className="group-menu-item" onClick={()=>{expandAll();setMenu(null);}}><Plus/>{t('groups.expandAll')}</button>
        <button role="menuitem" className="group-menu-item" onClick={()=>{collapseAll();setMenu(null);}}><Minus/>{t('groups.collapseAll')}</button>
      </div>
      {menu.item&&<><div role="separator" className="my-1 border-t border-border"/><div role="group" aria-label={t('groups.menu.danger')}>
        <button role="menuitem" className="group-menu-item text-destructive" onClick={()=>open('delete',menu.item!)}><Trash2/>{t('groups.delete')}</button>
      </div></>}
    </div>}
    <Dialog open={!!edit} onOpenChange={open=>{if(!busy&&!open)setEdit(null);}}><DialogContent><DialogHeader><DialogTitle>{t(`groups.${edit?.action??'create'}`)}</DialogTitle></DialogHeader><DialogBody>{edit?.action==='delete'?<p className="text-sm">{t('groups.deleteConfirm',{name:edit.item?.name})}</p>:<Input autoFocus aria-label={t('groups.name')} value={name} maxLength={100} onChange={e=>setName(e.target.value)} onKeyDown={e=>{if(e.key==='Enter'&&!e.nativeEvent.isComposing&&name.trim()&&!busy)void submit();}}/>}</DialogBody><DialogFooter><Button variant="outline" disabled={busy} onClick={()=>setEdit(null)}>{t('common.cancel')}</Button><Button variant={edit?.action==='delete'?'destructive':'default'} disabled={busy||(edit?.action!=='delete'&&!name.trim())} onClick={()=>void submit()}>{t('common.confirm')}</Button></DialogFooter></DialogContent></Dialog>
  </>;
}
