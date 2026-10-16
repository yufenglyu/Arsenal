import { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { ExternalLink, FolderOpen, Link2, Plus, Pencil, Copy, FileCode, RefreshCw, Unlink } from "lucide-react";
import { Menu } from "@base-ui/react/menu";
import { defaultNotebookName, notebookTitle, normalizeNotebookFolder } from "@/lib/notebookNames";
import { NotebookToolbar, NotebookFormattingToolbar } from "./NotebookToolbar";
import ReactMarkdown from "react-markdown";
import { mathRemarkPlugins, mathRehypePlugins } from "@/lib/markdownMath";
import remarkGfm from "remark-gfm";
import { Textarea } from "@/components/ui/textarea";
import { Input } from "@/components/ui/input";
import { Button } from "@/components/ui/button";
import { useNotebookStore } from "@/stores/notebookStore";
import type { NotebookFile, Repository } from "@/types/notebook";
import { Modal } from "./Modal";
import { matchesShortcut } from "@/lib/notebookShortcut";

function LocalImage({id,src,alt}:{id:number;src?:string;alt?:string}) {
  const [url,setUrl]=useState<string>();
  useEffect(()=>{let active=true;setUrl(undefined);if(src&&!/^https?:\/\//i.test(src))void useNotebookStore.getState().notebookAsset(id,src).then(value=>{if(active)setUrl(value??undefined);});return()=>{active=false;};},[id,src]);
  const remote=src&&/^https?:\/\//i.test(src);
  return <img src={remote?src:url} alt={alt??""} loading="lazy" referrerPolicy="no-referrer"/>;
}

function LocalMarkdown({repository,body}:{repository:Repository;body:string}) {
  const store=useNotebookStore();
  return <div className="notebook-markdown"><ReactMarkdown remarkPlugins={[remarkGfm, ...mathRemarkPlugins]} rehypePlugins={mathRehypePlugins}
    urlTransform={value=>/^(?:https?:\/\/|#)/i.test(value)||!/^([a-z][a-z\d+.-]*:|\/\/)/i.test(value)?value:""}
    components={{
      img:({src,alt})=><LocalImage id={repository.id} src={src} alt={alt}/>,
      a:({href,children})=><a href={href} onClick={e=>{if(!href||href.startsWith("#"))return;e.preventDefault();if(/^https?:\/\//i.test(href))void store.visit(href);else void store.notebookAsset(repository.id,href,true);}}>{children}</a>,
    }}>{body}</ReactMarkdown></div>;
}

export function NotebookLinkDialog({repository,initialBody,onClose,onLinked,create:initialCreate=false}:{repository:Repository;initialBody?:string;onClose:()=>void;onLinked?:(file:NotebookFile)=>void;create?:boolean}) {
  const {t}=useTranslation();const store=useNotebookStore();
  const [create,setCreate]=useState(initialCreate);
  const folder=normalizeNotebookFolder(store.config.obsidian_note_folder);
  const [path,setPath]=useState(`${folder}/${defaultNotebookName(repository.fullName)}.md`);
  const [inspection,setInspection]=useState<NotebookFile|null>(null);
  const [busy,setBusy]=useState(false);
  const [propertyPreview,setPropertyPreview]=useState<string|null>(null);
  const previewProperties=store.previewNotebookProperties;
  useEffect(()=>{let active=true;void previewProperties(repository.id).then(value=>{if(active)setPropertyPreview(value);});return()=>{active=false;};},[repository.id,previewProperties]);
  async function choose(){const selected=await store.chooseNotebook();if(!selected)return;const file=await store.inspectNotebook(selected);if(file){setPath(file.relativePath);setInspection(file);setCreate(false);}}
  async function bind(){setBusy(true);try{const file=await store.bindNotebook(repository.id,path,create,inspection?.revision??null,create?initialBody:undefined);if(file){onLinked?.(file);onClose();}}finally{setBusy(false);}}
  return <Modal title={t("g.obsidian.link")} icon={<Link2/>} onClose={onClose}>
    <div className="space-y-4">
      <p className="text-xs text-muted-foreground">{t("g.obsidian.linkHint")}</p>
      <div className="flex gap-2"><Button size="sm" variant={create?"default":"outline"} onClick={()=>{setCreate(true);setInspection(null);}}><Plus/>{t("g.obsidian.create")}</Button><Button size="sm" variant="outline" onClick={()=>void choose()}><FolderOpen/>{t("g.obsidian.chooseExisting")}</Button></div>
      <label className="block text-sm">{t("g.obsidian.relativePath")}<Input className="notebook-input mt-2 w-full" value={path} readOnly={!create} onChange={e=>setPath(e.target.value)}/></label>
      <div className="rounded-lg border border-border p-3">
        <p className="mb-3 text-sm font-semibold">{t("g.obsidian.propertyPreview")}</p>
        <pre className="whitespace-pre-wrap break-all text-xs">{propertyPreview ?? "—"}</pre>
      </div>
      {inspection&&<details><summary className="cursor-pointer text-sm">{t("g.obsidian.existingPreview")}</summary><pre className="mt-3 max-h-48 overflow-auto whitespace-pre-wrap text-xs">{inspection.properties}{"\n"}{inspection.body}</pre></details>}
      <div className="flex justify-end gap-2"><Button variant="outline" onClick={onClose}>{t("g.cancel")}</Button><Button disabled={busy||!path.trim()||(!create&&!inspection)} onClick={()=>void bind()}><Link2/>{t("g.obsidian.confirmLink")}</Button></div>
    </div>
  </Modal>;
}

type Draft = {body:string;baseline:NotebookFile};
const linkedDrafts=new Map<string,Draft>();

export function ObsidianNotebook({repository:r}:{repository:Repository}) {
  const {t}=useTranslation();const store=useNotebookStore();
  const key=r.repositoryUid??String(r.id);
  const draft=linkedDrafts.get(key);
  const [value,setValue]=useState(draft?.body??"");const valueRef=useRef(value);
  const [file,setFile]=useState<NotebookFile|null>(draft?.baseline??null);const baseline=useRef(file);
  const [external,setExternal]=useState<NotebookFile|null>(null);
  const [preview,setPreview]=useState(true);const [busy,setBusy]=useState(false);const busyRef=useRef(false);
  const [unavailable,setUnavailable]=useState(false);const [linking,setLinking]=useState(false);
  const [renaming,setRenaming]=useState(false);const [name,setName]=useState("");
  const [opening,setOpening]=useState(false);const [legacy,setLegacy]=useState(false);const [unlinking,setUnlinking]=useState(false);
  const request=useRef(0);
  const editor=useRef<HTMLTextAreaElement>(null);
  const dirty=!!file&&value!==file.body;
  function apply(next:NotebookFile){baseline.current=next;valueRef.current=next.body;setFile(next);setValue(next.body);setExternal(null);setUnavailable(false);linkedDrafts.delete(key);}
  function change(next:string){valueRef.current=next;setValue(next);if(baseline.current)linkedDrafts.set(key,{body:next,baseline:baseline.current});}
  function insert(before:string,after:string){
    const input=editor.current;if(!input)return;
    const start=input.selectionStart,end=input.selectionEnd,selected=value.slice(start,end)||t("g.markdownText");
    change(value.slice(0,start)+before+selected+after+value.slice(end));
    requestAnimationFrame(()=>{input.focus();input.setSelectionRange(start+before.length,start+before.length+selected.length);});
  }
  useEffect(()=>{
    let disposed=false;let readFailed=false;let unsubscribe:(()=>void)|undefined;let timer:ReturnType<typeof setTimeout>|undefined;
    const refresh=async(quiet=true)=>{
      if(busyRef.current)return;
      const sequence=++request.current;
      const next=await useNotebookStore.getState().readNotebook(r.id,quiet&&readFailed);
      if(disposed||sequence!==request.current||busyRef.current)return;
      readFailed=!next;setUnavailable(!next);if(!next)return;
      if(baseline.current&&valueRef.current!==baseline.current.body){
        if(next.revision!==baseline.current.revision)setExternal(next);
        else {setExternal(null);if(next.relativePath!==baseline.current.relativePath){baseline.current=next;setFile(next);linkedDrafts.set(key,{body:valueRef.current,baseline:next});}}
      }
      else {baseline.current=next;valueRef.current=next.body;setFile(next);setValue(next.body);setExternal(null);linkedDrafts.delete(key);}
    };
    const schedule=()=>{clearTimeout(timer);timer=setTimeout(()=>void refresh(),250);};
    void refresh(false);
    void useNotebookStore.getState().watchNotebook(schedule).then(stop=>{if(disposed)stop();else unsubscribe=stop;});
    window.addEventListener("focus",schedule);
    const interval=setInterval(schedule,3000);
    return()=>{disposed=true;unsubscribe?.();clearTimeout(timer);clearInterval(interval);window.removeEventListener("focus",schedule);};
  },[r.id,key,store.config.obsidian_vault_path]);
  async function save(updateProperties=false){
    if(!baseline.current||busyRef.current||unavailable)return false;
    busyRef.current=true;setBusy(true);request.current++;
    const body=valueRef.current;
    try{
      const next=await store.saveNotebook(r.id,body,baseline.current.revision,updateProperties);
      if(next){if(valueRef.current===body)apply(next);else {baseline.current=next;setFile(next);linkedDrafts.set(key,{body:valueRef.current,baseline:next});}return true;}
      const latest=await store.readNotebook(r.id,true);if(latest)setExternal(latest);return false;
    }finally{busyRef.current=false;setBusy(false);}
  }
  async function unlink(keepBody:boolean){if(busy)return;setBusy(true);try{if(await store.unlinkNotebook(r.id,keepBody,file?.revision??null)){linkedDrafts.delete(key);setUnlinking(false);}}finally{setBusy(false);}}
  async function rename(){
    if(!file||busyRef.current||dirty||unavailable||external||!name.trim())return;
    if(name.trim()===notebookTitle(file.relativePath)){setRenaming(false);return;}
    busyRef.current=true;setBusy(true);request.current++;
    try{const next=await store.renameNotebook(r.id,name,file.revision);if(next){apply(next);setRenaming(false);}}
    finally{busyRef.current=false;setBusy(false);}
  }
  const open=()=>{if(dirty)setOpening(true);else void store.openObsidian(r.id);};
  const displayPath=external?.relativePath??file?.relativePath??r.obsidian?.relativePath??"";
  const beginRename=()=>{setName(notebookTitle(file?.relativePath??displayPath));setRenaming(true);};
  return <section className="flex h-full min-h-0 flex-col" aria-label={t("g.notebook")} onKeyDown={e=>{if(!e.repeat&&matchesShortcut(e,"saveNotebook",useNotebookStore.getState().config)){e.preventDefault();e.stopPropagation();void save();}}}>
    <NotebookToolbar title={renaming?<Input className="w-full min-w-0" aria-label={t("g.obsidian.noteName")} autoFocus
      value={name} disabled={busy} onChange={e=>setName(e.target.value)} onBlur={()=>void rename()}
      onKeyDown={e=>{e.stopPropagation();if(e.key==="Enter"||matchesShortcut(e,"saveNotebook",useNotebookStore.getState().config)){e.preventDefault();void rename();}if(e.key==="Escape"){e.preventDefault();setRenaming(false);}}}/>
      :<button type="button" className="block w-full truncate text-left font-semibold" disabled={busy||dirty||unavailable||!file||!!external}
        title={t("g.obsidian.inlineRenameHint",{path:displayPath})} aria-label={t("g.obsidian.rename")} onClick={beginRename}>{notebookTitle(displayPath)||defaultNotebookName(r.fullName)}</button>}
      onSave={()=>void save()} saveDisabled={busy||renaming||!dirty||unavailable||!!external} preview={preview} unavailable={!file||unavailable||renaming} onTogglePreview={()=>setPreview(!preview)}
      association={<Menu.Root>
        <Menu.Trigger render={<Button size="icon-sm" variant="ghost"/>} title={t("g.obsidian.manageLink")} aria-label={t("g.obsidian.manageLink")}><Link2/></Menu.Trigger>
        <Menu.Portal><Menu.Positioner align="end" sideOffset={8} className="z-50">
          <Menu.Popup className="w-64 max-w-[calc(100vw-16px)] rounded-lg border border-border bg-popover p-1 text-sm text-popover-foreground shadow-lg outline-none">
            <Menu.Item className="notebook-menu-item" onClick={open}><ExternalLink/>{t("g.obsidian.open")}</Menu.Item>
            <Menu.Item className="notebook-menu-item" disabled={busy||dirty||unavailable||!file||!!external} onClick={()=>setTimeout(beginRename,0)}><Pencil/>{t("g.obsidian.rename")}</Menu.Item>
            <Menu.Item className="notebook-menu-item" disabled={busy||dirty||unavailable||!file||!!external} onClick={()=>void save(true)}><RefreshCw/>{t("g.obsidian.updateProperties")}</Menu.Item>
            <Menu.Item className="notebook-menu-item" disabled={busy||unavailable||!file} onClick={()=>void store.copyNotebookLink(r.id)}><Copy/>{t("g.obsidian.copyStableLink")}</Menu.Item>
            <Menu.Item className="notebook-menu-item" disabled={busy||dirty} onClick={()=>setLinking(true)}><Link2/>{t("g.obsidian.relocate")}</Menu.Item>
            <Menu.Item className="notebook-menu-item" disabled={busy||dirty} onClick={()=>setUnlinking(true)}><Unlink/>{t("g.obsidian.unlink")}</Menu.Item>
          </Menu.Popup>
        </Menu.Positioner></Menu.Portal>
      </Menu.Root>}/>
    <div className="flex min-h-0 flex-1 flex-col gap-4 overflow-auto p-4">
    {!preview&&<NotebookFormattingToolbar unavailable={busy||unavailable} onInsert={insert}/>}
    {unavailable&&<p className="text-xs text-muted-foreground">{t("g.obsidian.unavailable")}</p>}
    {external&&<div className="space-y-3 rounded-lg border border-border p-3">
      <p className="text-sm font-semibold">{t("g.obsidian.conflict")}</p>
      <details><summary className="cursor-pointer text-sm">{t("g.obsidian.compare")}</summary><div className="mt-3 grid gap-3"><label>{t("g.obsidian.draft")}<Textarea readOnly className="notebook-input mt-2 min-h-32 w-full" value={value}/></label><label>{t("g.obsidian.external")}<Textarea readOnly className="notebook-input mt-2 min-h-32 w-full" value={external.body}/></label></div></details>
      <div className="flex flex-wrap gap-2"><Button size="sm" variant="outline" onClick={()=>{if(window.confirm(t("g.obsidian.discardDraft")))apply(external);}}>{t("g.obsidian.useExternal")}</Button><Button size="sm" variant="outline" onClick={()=>setLinking(true)}>{t("g.obsidian.saveCopy")}</Button></div>
    </div>}
    {file?.properties&&<details className="notebook-properties rounded-lg border border-border p-3"><summary className="flex cursor-pointer items-center gap-2 text-sm"><FileCode/>{t("g.obsidian.properties")}</summary><pre className="mt-3 whitespace-pre-wrap break-all font-mono text-xs">{"---\n"}{file.properties.trimEnd()}{"\n---"}</pre></details>}
    {r.notebook&&<div className="flex flex-wrap items-center gap-2 text-xs"><span>{t("g.obsidian.legacyRetained")}</span><Button size="sm" variant="outline" onClick={()=>setLegacy(true)}>{t("g.obsidian.viewLegacy")}</Button><Button size="sm" variant="outline" disabled={busy||unavailable||dirty} onClick={()=>{if(window.confirm(t("g.obsidian.clearLegacyConfirm")))void store.clearLegacyNotebook(r.id,r.notebook??"");}}>{t("g.obsidian.clearLegacy")}</Button></div>}
    {preview?<div className="min-h-64 flex-1">{value?<LocalMarkdown repository={r} body={value}/>:<p className="text-xs text-muted-foreground">{t("g.emptyNotebook")}</p>}</div>:<Textarea ref={editor} className="notebook-input min-h-64 flex-1 resize-y font-mono leading-relaxed" aria-label={t("g.notebookContent")} value={value} onChange={e=>change(e.target.value)} disabled={unavailable}/>}
    <span className="text-xs text-muted-foreground">{dirty?t("g.unsaved"):""}</span>
    </div>
    {linking&&<NotebookLinkDialog key={key} repository={r} initialBody={value} create={!!external} onClose={()=>setLinking(false)} onLinked={next=>{linkedDrafts.delete(key);apply(next);}}/>}
    {legacy&&<Modal title={t("g.obsidian.viewLegacy")} onClose={()=>setLegacy(false)}><pre className="whitespace-pre-wrap text-sm">{r.notebook}</pre></Modal>}
    {unlinking&&<Modal title={t("g.obsidian.unlink")} onClose={()=>setUnlinking(false)}><p className="mb-4 text-sm">{t("g.obsidian.unlinkHint")}</p><div className="flex flex-wrap justify-end gap-2"><Button variant="outline" onClick={()=>setUnlinking(false)}>{t("g.cancel")}</Button><Button variant="outline" disabled={busy} onClick={()=>void unlink(false)}>{t("g.obsidian.unlinkOnly")}</Button><Button disabled={busy||unavailable||!file} onClick={()=>void unlink(true)}>{t("g.obsidian.keepInternal")}</Button></div></Modal>}
    {opening&&<Modal title={t("g.obsidian.open")} onClose={()=>setOpening(false)}><p className="mb-4 text-sm">{t("g.obsidian.openDirty")}</p><div className="flex flex-wrap justify-end gap-2"><Button variant="outline" onClick={()=>setOpening(false)}>{t("g.cancel")}</Button><Button variant="outline" onClick={()=>{setOpening(false);void store.openObsidian(r.id);}}>{t("g.obsidian.openWithoutSaving")}</Button><Button disabled={busy||!!external} onClick={()=>void save().then(ok=>{if(ok){setOpening(false);void store.openObsidian(r.id);}})}>{t("g.obsidian.saveAndOpen")}</Button></div></Modal>}
  </section>;
}
