import { useSkillDetailStore } from "./skillDetailStore";
import { useCollectionStore } from "./collectionStore";
import { create } from "zustand";
import { invoke, isTauriRuntime } from "@/lib/tauri";
import { useResourceLibraryStore } from "./resourceLibraryStore";
import { useCentralSkillsStore } from "./centralSkillsStore";

export interface EditableMetadata {
  notes: string | null;
  tags: string[];
}
export interface FolderNote {
  id: string;
  notes: string;
  skillIds: string[];
}
interface MetadataState {
  folders: FolderNote[];
  folderError: boolean;
  revision: number;
  loadFolders: () => Promise<void>;
  saveFolder: (note: FolderNote) => Promise<void>;
  publish: (id: string, metadata: EditableMetadata) => void;
  changeTag: (
    tag: string,
    ids: string[] | null,
    remove: boolean,
  ) => Promise<void>;
  renameTag: (oldTag: string, newTag: string, merge: boolean) => Promise<void>;
  countTag: (tag: string) => Promise<number>;
}
export const useMetadataStore = create<MetadataState>((set, get) => ({
  folders: [],
  folderError: false,
  revision: 0,
  loadFolders: async () => {
    try {
      if (isTauriRuntime())
        set({
          folders: (await invoke<FolderNote[]>("get_folder_notes")) ?? [],
          folderError: false,
        });
    } catch {
      set({ folderError: true });
    }
  },
  saveFolder: async (note) => {
    if (isTauriRuntime()) await invoke("save_folder_note", { note });
    set((s) => ({
      folders: [...s.folders.filter((n) => n.id !== note.id), note],
    }));
  },
  publish: (id, metadata) => {
    useSkillDetailStore.setState((s) => ({
      detail: s.detail?.id === id ? { ...s.detail, ...metadata } : s.detail,
    }));
    useCollectionStore.setState((s) => ({
      currentDetail: s.currentDetail
        ? {
            ...s.currentDetail,
            skills: s.currentDetail.skills.map((skill) =>
              skill.id === id ? { ...skill, ...metadata } : skill,
            ),
          }
        : null,
    }));
    useResourceLibraryStore.setState((s) => ({
      skills: s.skills.map((skill) =>
        skill.id === id ? { ...skill, ...metadata } : skill,
      ),
    }));
    useCentralSkillsStore.setState((s) => ({
      skills: s.skills.map((skill) =>
        skill.id === id ? { ...skill, ...metadata } : skill,
      ),
    }));
    set((s) => ({ revision: s.revision + 1 }));
  },
  countTag: async (tag) =>
    isTauriRuntime()
      ? invoke<number>("count_skill_tag", { tag })
      : new Set(
          [
            ...useResourceLibraryStore.getState().skills,
            ...useCentralSkillsStore.getState().skills,
          ]
            .filter((s) =>
              s.tags?.some((t) => t.toLowerCase() === tag.toLowerCase()),
            )
            .map((s) => s.id),
        ).size,
  renameTag: async (oldTag, newTag, merge) => {
    const rows = await invoke<Array<EditableMetadata & {id:string}>>("rename_skill_tag", {oldTag,newTag:newTag.trim(),merge});
    rows.forEach(row => get().publish(row.id,row));
    window.dispatchEvent(new CustomEvent("arsenal-tag-renamed", {detail:{oldTag:oldTag.toLowerCase(),newTag:newTag.trim().toLowerCase()}}));
  },
  changeTag: async (tag, ids, remove) => {
    if (isTauriRuntime()) {
      const rows = await invoke<Array<EditableMetadata & { id: string }>>(
        "change_skill_tag",
        { tag, skillIds: ids, remove },
      );
      rows.forEach((row) => get().publish(row.id, row));
    } else {
      const skills = new Map(
        [
          ...useResourceLibraryStore.getState().skills,
          ...useCentralSkillsStore.getState().skills,
        ].map((s) => [s.id, s]),
      );
      for (const skill of skills.values()) {
        if (ids && !ids.includes(skill.id)) continue;
        const has = (skill.tags ?? []).some(
          (t) => t.toLowerCase() === tag.toLowerCase(),
        );
        if (has === !remove) continue;
        const tags = (skill.tags ?? []).filter(
          (t) => t.toLowerCase() !== tag.toLowerCase(),
        );
        if (!remove) tags.push(tag);
        get().publish(skill.id, { notes: skill.notes ?? null, tags });
      }
    }
  },
}));

export function findFolderNote(notes: FolderNote[], skillIds: string[]) {
  return notes.find((note) =>
    note.skillIds.some((id) => skillIds.includes(id)),
  );
}
