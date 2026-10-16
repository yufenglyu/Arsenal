import { create } from "zustand";
import { invoke } from "@/lib/tauri";
import i18n from "@/i18n";
import { useTaskQueueStore, waitForTask } from "./taskQueueStore";

export interface RepositoryReadme { name: string; language: "default" | "zh" | "en"; content: string }

// Reading stays in the store; callers own request lifetimes when switching rows.
export const useRepositoryDocumentStore = create<{
  revision: number;
  invalidate: () => void;
  readDocuments: (directory: string, skillId?: string) => Promise<RepositoryReadme[]>;
  generateNote: (directory: string, skillId?: string, language?: string) => Promise<string>;
  highlightedTags: string[];
  setHighlightedTags: (tags: string[]) => void;
  readReadme: (directory: string, skillId?: string) => Promise<string | null>;
}>((set) => ({
  revision: 0,
  invalidate: () => set(state => ({revision: state.revision + 1})),
  readDocuments: (directory, skillId) => invoke<RepositoryReadme[]>("read_repository_documents", {directory, ...(skillId ? {skillId} : {})}),
  generateNote: async (directory, skillId, language = "zh") => {
    const documents = await useRepositoryDocumentStore.getState().readDocuments(directory, skillId);
    const available = documents.filter(document => document.content.trim());
    const document = available.find(document => document.language === (language.startsWith("zh") ? "zh" : "en"))
      ?? available.find(document => document.language === "default") ?? available[0];
    if (!document) throw new Error(i18n.t("workflow.repositoryReadmeRequired"));
    const id = useTaskQueueStore.getState().enqueue({key:`ai:repository-notes:${directory}`,kind:"ai",label:i18n.t("workflow.generateRepositoryNote"),steps:[{command:"generate_repository_note",args:{content:document.content},label:i18n.t("workflow.generateRepositoryNote")}]});
    const task = await waitForTask(id);
    if (task.status !== "success") throw new Error(task.steps.find(step => step.error)?.error ?? i18n.t("workflow.operationFailed"));
    return String(task.steps[0].result ?? "");
  },
  highlightedTags: [],
  setHighlightedTags: (tags) => set({ highlightedTags: [...new Set(tags.map(tag => tag.toLowerCase()))] }),
  readReadme: (directory, skillId) => invoke<string | null>("read_repository_readme", { directory, ...(skillId ? { skillId } : {}) }),
}));
