import { useGitHubImportStore } from "./githubImportStore";
import { useResourceLibraryStore } from "./resourceLibraryStore";
import {
  registerTaskExecutor,
  registerTaskInputRequirement,
  registerTaskResult,
  useTaskQueueStore,
} from "./taskQueueStore";
import type { GitHubRepoPreview, GitHubSkillImportSelection } from "@/types";
import i18n from "@/i18n";
import {importRepository} from "@/lib/taskPresentation";
import { toast } from "sonner";

import { invoke } from "@/lib/tauri";
export interface LocalImportPreviewItem {
  skillId: string;
  name: string;
  conflict: boolean;
}
export async function previewLocalImport(sourceDir: string) {
  return invoke<LocalImportPreviewItem[]>("preview_local_resource_skills", {
    sourceDir,
  });
}

export const PREPARE_GITHUB_IMPORT = "prepare_github_resource_import";
export function preparedImportPreview(taskId: string): GitHubRepoPreview | null {
  const task = useTaskQueueStore.getState().tasks.find(task => task.id === taskId);
  const result = task?.steps.find(step => step.command === PREPARE_GITHUB_IMPORT)?.result as GitHubRepoPreview | undefined;
  if (!result?.repo || typeof result.repo.owner !== "string" || typeof result.repo.repo !== "string" || !Array.isArray(result.skills)) return null;
  if (!result.skills.every(skill => skill && typeof skill.sourcePath === "string" && typeof skill.skillId === "string" && typeof skill.skillName === "string")) return null;
  return result;
}

// Validate against the reviewed preview and transition the existing batch only once.
export function submitPreparedImport(taskId: string, preview: GitHubRepoPreview, selections: GitHubSkillImportSelection[]): string | null {
  const queue = useTaskQueueStore.getState();
  const task = queue.tasks.find(task => task.id === taskId);
  const step = task?.steps.find(step => step.command === PREPARE_GITHUB_IMPORT);
  if (!task || task.status !== "awaiting_input" || task.clearRequested || step?.result !== preview || typeof step.args.repoUrl !== "string") return null;
  const paths = new Set(preview.skills.map(skill => skill.sourcePath));
  const chosen = selections.filter(item => item.resolution !== "skip");
  if (!chosen.length || new Set(selections.map(item => item.sourcePath)).size !== selections.length || selections.some(item => !paths.has(item.sourcePath) || !["skip","rename","overwrite"].includes(item.resolution))) return null;
  if (chosen.some(item => item.resolution === "rename" && (!item.renamedSkillId?.trim() || /[\\/]/.test(item.renamedSkillId) || [".",".."].includes(item.renamedSkillId.trim())))) return null;
  const batchId = task.batchId ?? task.id;
  const id = enqueueGitHubImport(step.args.repoUrl, preview, selections, batchId, step.args.reimport === true);
  if (!id) return null;
  queue.patch(task.id, {batchId, status:"success", skippedImportSkills:preview.skills.filter(skill => !chosen.some(item => item.sourcePath === skill.sourcePath)).map(skill => skill.skillName),
    steps:task.steps.map(item => ({...item,status:"success"}))});
  return id;
}
const pendingPreviews = new Map<string, Promise<GitHubRepoPreview>>();
useTaskQueueStore.subscribe((state) => {
  for (const requestId of pendingPreviews.keys()) {
    if (!state.tasks.some((task) => task.batchId === requestId &&
      (task.status === "queued" || task.status === "running"))) {
      pendingPreviews.delete(requestId);
    }
  }
});

export function enqueueGitHubImport(
  repoUrl: string,
  preview: GitHubRepoPreview,
  selections: GitHubSkillImportSelection[],
  batchId?: string,
  reimport = false,
) {
  const chosen = selections.filter((selection) => selection.resolution !== "skip").map(selection=>({
    sourcePath:selection.sourcePath,resolution:selection.resolution,
    ...(selection.resolution === "rename"?{renamedSkillId:selection.renamedSkillId?.trim()}:{}),
  }));
  if (!chosen.length) return null;
  const repository = `${preview.repo.owner}/${preview.repo.repo}`.toLowerCase();
  const importUrl = /^[a-f0-9]{40}$/i.test(preview.repo.branch ?? "")
    ? `https://github.com/${preview.repo.owner}/${preview.repo.repo}/tree/${preview.repo.branch}`
    : repoUrl;
  return useTaskQueueStore.getState().enqueue({
    key: `${reimport?"reimport":"import"}:${repository}:${JSON.stringify(chosen)}`,
    kind: "import",
    label: repository,
    ...(batchId ? { batchId, batchLabel: repository } : {}),
    locks: [
      `repo:${repository}`,
      ...chosen.map((selection) => `skill:${selection.renamedSkillId ??
        preview.skills.find((item) => item.sourcePath === selection.sourcePath)?.skillId ??
        selection.sourcePath}`),
    ],
    // The backend stages the whole selection transactionally from one archive.
    steps: [{
      command: reimport?"reimport_github_repo_skills":"import_github_repo_skills",
      label: repository,
      args: { repoUrl: importUrl, selections: chosen },
    }],
  });
}

// Reuse the preview already requested by the dialog. A retry after restart previews again.
export function continueGitHubImportInBackground(
  repoUrl: string,
  preview?: Promise<GitHubRepoPreview>,
  operationId?: string,
  reimport = false,
) {
  const key = `prepare-${reimport?"reimport":"import"}:${repoUrl.trim().toLowerCase()}`;
  const existing = useTaskQueueStore.getState().tasks.find((task) => task.key === key &&
    ["queued", "running", "awaiting_input"].includes(task.status) && !task.cancelRequested);
  if (existing) return existing.id;
  const requestId = crypto.randomUUID();
  const label = importRepository([{steps:[{command:PREPARE_GITHUB_IMPORT,args:{repoUrl},label:""}]}]) ?? i18n.t("workflow.githubImportTask");
  if (preview) pendingPreviews.set(requestId, preview);
  return useTaskQueueStore.getState().enqueue({
    key,
    kind: "import",
    label,
    batchId: requestId,
    batchLabel: label,
    steps: [{
      command: PREPARE_GITHUB_IMPORT,
      label: i18n.t("workflow.prepareImport"),
      args: { repoUrl, requestId, ...(reimport?{reimport:true}:{}), ...(operationId ? {operationId} : {}) },
    }],
  });
}

// Queue preparation first so duplicate drops do not start duplicate downloads.
export function enqueueGitHubUrlImport(repoUrl: string) {
  return continueGitHubImportInBackground(repoUrl, undefined, crypto.randomUUID());
}

registerTaskExecutor(PREPARE_GITHUB_IMPORT, async (args) => {
  const requestId = String(args.requestId);
  const pending = pendingPreviews.get(requestId);
  try {
    const preview = await (pending ?? useGitHubImportStore.getState().previewGitHubRepoImport(String(args.repoUrl), String(args.operationId)));
    return args.reimport ? {...preview, skills:preview.skills.map(skill=>({...skill,conflict:null}))} : preview;
  } finally {
    pendingPreviews.delete(requestId);
  }
});
registerTaskInputRequirement(PREPARE_GITHUB_IMPORT, (result) =>
  (result as GitHubRepoPreview).skills.some((skill) => !!skill.conflict || (skill.variantCount ?? 0) > 1));
registerTaskResult(PREPARE_GITHUB_IMPORT, (step, result) => {
  const queue = useTaskQueueStore.getState();
  const task = queue.tasks.find((task) => task.batchId === step.args.requestId);
  if (!task || task.cancelRequested || task.clearRequested) return;
  const preview = result as GitHubRepoPreview;
  const repository = `${preview.repo.owner}/${preview.repo.repo}`;
  queue.patch(task.id, { label: repository, batchLabel: repository });
  if (step.status === "awaiting_input") {
    toast.info(i18n.t("workflow.importNeedsReview"));
    return;
  }
  if (!preview.skills.length) {
    toast.info(i18n.t("workflow.noImportableSkills"));
    return;
  }
  enqueueGitHubImport(String(step.args.repoUrl), preview, preview.skills.map((skill) => ({
    sourcePath: skill.sourcePath,
    resolution: skill.recommended === false ? "skip" : "overwrite",
  })), task.batchId, step.args.reimport === true);
});
for (const command of ["import_github_repo_skills", "reimport_github_repo_skills", "add_local_resource_skills"]) {
  registerTaskResult(command, async (_step,result) => {
    if(command==="reimport_github_repo_skills") {
      const {useRepositorySyncStore} = await import("./repositorySyncStore");
      const repo=(result as {repo?:{owner?:string;repo?:string}})?.repo;
      if(repo?.owner&&repo.repo)useRepositorySyncStore.getState().invalidateRepositories([`${repo.owner}/${repo.repo}`]);
      const {refreshResourceViews} = await import("./resourceTrashStore");
      await refreshResourceViews();
      const {useCentralSkillsStore} = await import("./centralSkillsStore");
      const error = useCentralSkillsStore.getState().error;
      if (error) throw new Error(error);
    } else await useResourceLibraryStore.getState().loadResourceLibrary({fresh:true});
    const error = useResourceLibraryStore.getState().error;
    if (error) throw new Error(error);
  });
}
