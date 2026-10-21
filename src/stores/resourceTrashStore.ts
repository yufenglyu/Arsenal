import {create} from "zustand";
import {invoke, isTauriRuntime} from "@/lib/tauri";
import {useResourceLibraryStore} from "./resourceLibraryStore";
import {useCentralSkillsStore} from "./centralSkillsStore";
import {useCollectionStore} from "./collectionStore";
import {useMetadataStore} from "./metadataStore";
import {useNotebookStore} from "./notebookStore";
import {taskErrorMessage} from "./taskQueueStore";

export interface ResourceTrashEntry {
  id:string; name:string; kind:"skill"|"repository"; repositories:string[];
  skillCount:number; deletedAt:string; reason:"delete"|"reimport"; phase:string;
}
interface State {
  entries:ResourceTrashEntry[]; loading:boolean; error:string|null;
  load:(fresh?:boolean)=>Promise<void>; restore:(id:string)=>Promise<void>; purge:(ids:string[])=>Promise<void>;
}
export async function refreshResourceViews() {
  await Promise.all([
    useResourceLibraryStore.getState().loadResourceLibrary({fresh:true}),
    useCentralSkillsStore.getState().loadCentralSkills(),
    useCollectionStore.getState().loadGroupDetails(),
    useMetadataStore.getState().loadFolders(), useNotebookStore.getState().load(),
  ]);
  await useResourceTrashStore.getState().load(true);
}
let pending:Promise<void>|null=null;
export const useResourceTrashStore=create<State>((set,get)=>({
  entries:[],loading:false,error:null,
  load:(fresh)=>{
    if(pending)return fresh?pending.then(()=>get().load()):pending;
    pending=(async()=>{
      set({loading:true,error:null});
      try {const entries=isTauriRuntime()?await invoke<ResourceTrashEntry[]>("list_resource_trash"):[];set({entries:Array.isArray(entries)?entries:[]});}
      catch(error){set({error:taskErrorMessage(error)});}
      finally{set({loading:false});}
    })().finally(()=>{pending=null;});return pending;
  },
  restore:async id=>{
    try {
      const repositories=get().entries.find(entry=>entry.id===id)?.repositories??[];
      await invoke("restore_resource_trash",{id});
      const {useRepositorySyncStore}=await import("./repositorySyncStore");
      useRepositorySyncStore.getState().invalidateRepositories(repositories);
      await refreshResourceViews();
    }
    catch(error){set({error:taskErrorMessage(error)});throw new Error(taskErrorMessage(error));}
  },
  purge:async ids=>{
    try {await invoke("purge_resource_trash",{ids});await get().load();}
    catch(error){set({error:taskErrorMessage(error)});throw new Error(taskErrorMessage(error));}
  },
}));
