import {useNotebookStore} from "./notebookStore";
import {invoke, isTauriRuntime} from "@/lib/tauri";
import { useRepositoryDocumentStore } from "./repositoryDocumentStore";
import {activityAttention, pendingUpdateCount} from "@/lib/activityCenter";
import {
  registerTaskExecutor,
  registerTaskResult,
  registerTaskFailure,
  registerTaskCompletion,
  isTaskActive,
  useTaskQueueStore,
  waitForTask,
} from "./taskQueueStore";
import { useCentralSkillsStore } from "./centralSkillsStore";
import i18n from "@/i18n";
import { toast } from "sonner";
import { create } from "zustand";
import { persist, createJSONStorage } from "zustand/middleware";
import { useResourceLibraryStore } from "@/stores/resourceLibraryStore";
import type {
  RepositorySyncPreview,
  RepositorySyncPreviewReport,
} from "@/types";

export function hasRepositoryChanges(item: RepositorySyncPreview): boolean {
  return (
    item.added.length > 0 || item.modified.length > 0 || item.deleted.length > 0 || (item.documentChanges?.length ?? 0) > 0
  );
}

interface RepositorySyncState {
  migratingRepository: string | null;
  changeRepositoryAddress: (repository: string, newAddress: string) => Promise<void>;
  updateErrors: Record<string, string>;
  isRefreshingRepositories: boolean;
  refreshRepositories: (repositories?: string[]) => Promise<void>;
  checkedAt: Record<string, number>;
  reportCheckedAt: number | null;
  reportGeneration: string | null;
  ignored: string[];
  setIgnored: (keys: string[]) => void;
  preview: RepositorySyncPreviewReport | null;
  isChecking: boolean;
  applied: boolean;
  appliedRepositories: string[];
  checkingRepository: string | null;
  recheckRepository: (repository: string) => Promise<void>;
  recheckRepositories: (repositories: string[]) => Promise<void>;
  markRepositoryApplied: (repository: string) => void;
  markApplied: () => void;
  requestedRepositories: string[] | null;
  error: string | null;
  checkForUpdates: (repositories?: string[]) => Promise<void>;
  open: boolean;
  centerView: "pending" | "active" | "history" | null;
  setCenterView: (view: "pending" | "active" | "history") => void;
  includeAdded: boolean;
  removeDeleted: boolean;
  repositories: string[] | null;
  setPreview: (preview: RepositorySyncPreviewReport | null) => void;
  clearCheckResults: () => void;
  dismissFailedCheck: (repository?: string) => void;
  setOpen: (open: boolean) => void;
  setIncludeAdded: (includeAdded: boolean) => void;
  setRemoveDeleted: (removeDeleted: boolean) => void;
  setRepositories: (repositories: string[] | null) => void;
}

class CheckPaused extends Error {}
let previewRequest: Promise<void> | null = null;
const activeRechecks = new Set<string>();

export const useRepositorySyncStore = create<RepositorySyncState>()(
  persist(
    (set, get) => ({
      migratingRepository: null,
      changeRepositoryAddress: async (repository, newAddress) => {
        if (get().migratingRepository || get().isChecking || get().checkingRepository || useTaskQueueStore.getState().tasks.some(isTaskActive)) throw new Error("addressBusy");
        set({migratingRepository: repository});
        try {
          const target = await invoke<string>("change_repository_address", {repository, newAddress});
          const pending: RepositorySyncPreview = {repository:target.toLowerCase(),snapshotId:null,currentRef:null,remoteRef:null,added:[],modified:[],deleted:[],unchanged:[],updated:[],documentChanges:[],error:null};
          // A committed migration must invalidate the visible preview before any reload.
          set(state => ({preview: state.preview ? {...state.preview,
            coverage: state.preview.coverage?.map(row => row.repository?.toLowerCase() === repository.toLowerCase() ? {...row, repository:target.toLowerCase(),status:"pending"} : row),
            repositories: state.preview.repositories.filter(row => row.repository.toLowerCase() !== repository.toLowerCase() && row.repository.toLowerCase() !== target.toLowerCase()).concat(pending),
          } : null, error:null,
            repositories:state.repositories?.map(row => row.toLowerCase() === repository.toLowerCase() ? target : row) ?? null,
            requestedRepositories:state.requestedRepositories?.map(row => row.toLowerCase() === repository.toLowerCase() ? target : row) ?? null,
            checkedAt:Object.fromEntries(Object.entries(state.checkedAt).filter(([row])=>row.toLowerCase() !== repository.toLowerCase())),
            applied:false, appliedRepositories:state.appliedRepositories.filter(row => row.toLowerCase() !== repository.toLowerCase())}));
          await useResourceLibraryStore.getState().loadResourceLibrary();
          await useNotebookStore.getState().load();
          await get().recheckRepository(target);
        } catch (error) {
          const code = error instanceof Error ? error.message : String(error);
          if (code === "addressRecovery") set(state=>({preview:state.preview ? {...state.preview,repositories:state.preview.repositories.map(row=>row.repository.toLowerCase()===repository.toLowerCase() ? {...row,snapshotId:null,error:code} : row)} : null}));
          throw error;
        } finally { set({migratingRepository:null}); }
      },
      updateErrors: {},
      isRefreshingRepositories: false,
      refreshRepositories: async (repositories) => {
        if (get().isRefreshingRepositories) return;
        set({ isRefreshingRepositories: true });
        try {
          const library = useResourceLibraryStore.getState();
          if (library.error) throw new Error(library.error);
          const scope = repositories?.length ? new Set(repositories.map(repo => repo.toLowerCase())) : null;
          const repos = [...new Set(library.skills.flatMap(skill => {
            const repo = skill.source_repo?.trim().toLowerCase();
            return repo && /^[^/\s]+\/[^/\s]+$/.test(repo) && (!scope || scope.has(repo)) ? [repo] : [];
          }))];
          const batchId = crypto.randomUUID();
          const ids = repos.map(repository => useTaskQueueStore.getState().enqueue({
            batchId, batchLabel: i18n.t("workflow.refreshRepository"),
            key: `repository-metadata:${repository}`, kind: "check", label: `${i18n.t("workflow.refreshRepository")} · ${repository}`,
            locks: [`repo:${repository}`],
            steps: [{ command: "refresh_repository", args: { repository }, label: repository }],
          }));
          await Promise.all(ids.map(waitForTask));
        } catch {
          toast.error(i18n.t("workflow.operationFailed"));
        } finally { set({ isRefreshingRepositories: false }); }
      },
      checkedAt: {},
      reportCheckedAt: null,
      reportGeneration: null,
      ignored: [],
      setIgnored: (ignored) => set({ ignored }),
      preview: null,
      isChecking: false,
      applied: false,
      appliedRepositories: [],
      checkingRepository: null,
      markRepositoryApplied: (repository) =>
        set((state) => {
          const appliedRepositories = [
            ...new Set([...state.appliedRepositories, repository]),
          ];
          return {
            appliedRepositories,
            applied:
              !!state.preview?.repositories.length &&
              state.preview.repositories.every(
                (item) =>
                  (!item.error && !hasRepositoryChanges(item)) ||
                  appliedRepositories.includes(item.repository),
              ),
          };
        }),
      recheckRepository: (repository) => get().recheckRepositories([repository]),
      recheckRepositories: async (repositories) => {
        if (get().isChecking || !repositories.length) return;
        const requested = [...new Set(repositories.map(repo => repo.toLowerCase()))].filter(repo => !activeRechecks.has(repo));
        if (!requested.length) return;
        requested.forEach(repo => activeRechecks.add(repo));
        set({ checkingRepository: [...activeRechecks][0] });
        try {
          const report = await checkInBackground(requested, getReportTaskGeneration());
          set((state) => ({
            preview: state.preview ? {
              ...state.preview,
              coverage: mergeCoverage(state.preview.coverage, report.coverage),
              repositories: state.preview.repositories.map(item => {
                if (!requested.includes(item.repository.toLowerCase())) return item;
                const replacement = report.repositories.find(repo => repo.repository.toLowerCase() === item.repository.toLowerCase());
                return replacement ? { ...replacement, repository: item.repository } : { ...item, error: i18n.t("workflow.operationFailed") };
              }),
            } : report,
            applied: false,
            appliedRepositories: state.appliedRepositories.filter(item => !requested.includes(item.toLowerCase())),
          }));
        } catch (error) {
          set((state) => ({
            preview: state.preview ? {
              ...state.preview,
              repositories: state.preview.repositories.map(item => requested.includes(item.repository.toLowerCase()) ? { ...item, error: String(error) } : item),
            } : null,
          }));
        } finally {
          requested.forEach(repo => activeRechecks.delete(repo));
          set({ checkingRepository: [...activeRechecks][0] ?? null });
        }
      },
      markApplied: () =>
        set((state) => ({
          applied: true,
          appliedRepositories:
            state.preview?.repositories.map((item) => item.repository) ?? [],
        })),
      requestedRepositories: null,
      error: null,
      checkForUpdates: (repositories) => {
        if (previewRequest) return previewRequest;
        if (get().checkingRepository || get().migratingRepository) return Promise.resolve();
        const existing = useTaskQueueStore.getState().tasks.find(task =>
          !!get().reportGeneration && task.parentReportGeneration === get().reportGeneration && task.steps.length > 0 &&
          task.steps.every(step => step.command === "preview_source_backed_resource_repository_updates") &&
          (isTaskActive(task) || ["cancelled", "interrupted"].includes(task.status)));
        if (existing) {
          if (!isTaskActive(existing)) useTaskQueueStore.getState().retry(existing.id);
          set({isChecking:true, error:null});
          previewRequest = waitForTask(existing.id).then(() => {}).finally(() => {
            set({isChecking:false}); previewRequest = null;
          });
          return previewRequest;
        }
        if (!repositories?.length) {
          releaseSnapshots(get().preview?.repositories ?? []);
          useTaskQueueStore.getState().clearCheckHistory();
          set({preview: null, checkedAt: {}, reportCheckedAt: null, updateErrors: {},
            reportGeneration: crypto.randomUUID(), repositories: null, applied: false,
            appliedRepositories: [], includeAdded: true, removeDeleted: false});
        }
        set({
          isChecking: true,
          error: null,
          requestedRepositories: repositories ?? null,
        });
        previewRequest = (async () => {
          try {
            if (!repositories?.length) {
              await useResourceLibraryStore.getState().loadResourceLibrary();
              const scanError=useResourceLibraryStore.getState().error;
              if (scanError) throw new Error(scanError);
            }
            const preview = await checkInBackground(repositories);
            toast.info(i18n.t("workflow.checkFinished", {count: preview.repositories.reduce<number>((sum, repo) => sum + repo.added.length + repo.modified.length + repo.deleted.length, 0), failures: preview.repositories.filter(repo => repo.error).length}), {action:{label:i18n.t("workflow.viewResults"),onClick:() => get().setOpen(true)}});
            set({
              preview,
              reportCheckedAt: Date.now(),
              includeAdded: true,
              removeDeleted: false,
              repositories: repositories ?? null,
              applied: false,
              appliedRepositories: [],
            });
          } catch (error) {
            if (error instanceof CheckPaused) return;
            set({ error: String(error) });
            toast.error(i18n.t("workflow.checkFailedNotification"), {action:{label:i18n.t("workflow.viewResults"),onClick:() => get().setOpen(true)}});
          } finally {
            set({ isChecking: false });
          }
        })().finally(() => {
          previewRequest = null;
        });
        return previewRequest;
      },
      open: false,
      centerView: null,
      setCenterView: (centerView) => set({centerView, open: true}),
      includeAdded: true,
      removeDeleted: false,
      repositories: null,
      setPreview: (preview) => set({ preview }),
      clearCheckResults: () => {
        if (get().isChecking || get().checkingRepository || useTaskQueueStore.getState().tasks.some(task =>
          isTaskActive(task) && (task.parentReportGeneration === get().reportGeneration || task.steps.some(step => step.command === "preview_source_backed_resource_repository_updates")))) return;
        releaseSnapshots(get().preview?.repositories ?? []);
        const generation = get().reportGeneration;
        for (const task of useTaskQueueStore.getState().tasks) {
          if (generation && task.parentReportGeneration === generation) useTaskQueueStore.getState().remove(task.id);
        }
        useTaskQueueStore.getState().clearCheckHistory();
        set({preview:null, reportCheckedAt:null, checkedAt:{}, updateErrors:{}, error:null,
          reportGeneration:crypto.randomUUID(), repositories:null, requestedRepositories:null,
          applied:false, appliedRepositories:[]});
      },
      dismissFailedCheck: (repository) => {
        if (repository) {
          if (get().isChecking || activeRechecks.has(repository.toLowerCase())) return;
          set(state => ({preview: state.preview ? {repositories: state.preview.repositories.filter(repo =>
            repo.repository.toLowerCase() !== repository.toLowerCase() || !repo.error)} : null}));
        } else {
          set({error: null});
          for (const task of useTaskQueueStore.getState().tasks) {
            if (["failed", "partial", "interrupted"].includes(task.status) && task.steps.length &&
              task.steps.every(step => step.command === "preview_source_backed_resource_repository_updates" && !step.result))
              useTaskQueueStore.getState().remove(task.id);
          }
        }
      },
      setOpen: (open) => {
        const state = get();
        const tasks = useTaskQueueStore.getState().tasks.filter(task => !task.clearRequested);
        const pending = pendingUpdateCount(state.preview, state.ignored, tasks) + activityAttention(tasks, state.preview, state.checkedAt, state.error).count + (state.error ? 1 : 0);
        set({open, centerView: open ? pending ? "pending" : tasks.some(task => task.status === "running" || task.status === "queued") ? "active" : "history" : null});
      },
      setIncludeAdded: (includeAdded) => set({ includeAdded }),
      setRemoveDeleted: (removeDeleted) => set({ removeDeleted }),
      setRepositories: (repositories) => set({ repositories }),
    }),
    {
      name: "skillshub.repository-update-preview.v1",
      storage: createJSONStorage(() => localStorage),
      merge: (persisted, current) => {
        const saved = persisted as Partial<RepositorySyncState> | undefined;
        const repositories = saved?.preview?.repositories;
        // Remove only the exact preview seeded by the withdrawn screenshot build.
        const screenshotPreview = repositories?.length === 2 &&
          repositories.some(repo => repo.repository === "demo/engineering" &&
            repo.added.some(item => item.skillId === "new" && item.version === "demo-v2") &&
            repo.modified.some(item => item.skillId === "react-patterns" && item.version === "demo-v2")) &&
          repositories.some(repo => repo.repository === "demo/research" &&
            repo.modified.some(item => item.skillId === "research-notes" && item.version === "demo-v2"));
        return { ...current, ...saved, ...(screenshotPreview ? {
          preview: null, repositories: null, applied: false, appliedRepositories: [],
        } : {}) };
      },
      partialize: (state) => ({
        updateErrors: state.updateErrors,
        checkedAt: state.checkedAt,
        reportCheckedAt: state.reportCheckedAt,
        reportGeneration: state.reportGeneration,
        ignored: state.ignored,
        preview: state.preview,
        repositories: state.repositories,
        includeAdded: state.includeAdded,
        removeDeleted: state.removeDeleted,
        applied: state.applied,
        appliedRepositories: state.appliedRepositories,
      }),
    },
  ),
);

// Persist the parent identity even for reports saved before task ownership existed.
export function getReportTaskGeneration(): string {
  const state = useRepositorySyncStore.getState();
  if (state.reportGeneration) return state.reportGeneration;
  const generation = crypto.randomUUID();
  useRepositorySyncStore.setState({reportGeneration: generation});
  return generation;
}

async function checkInBackground(
  repositories?: string[],
  parentReportGeneration?: string,
): Promise<RepositorySyncPreviewReport> {
  const reportGeneration = getReportTaskGeneration();
  const planned = repositories?.length ? repositories : isTauriRuntime()
    ? await invoke<string[]>("plan_repository_update_check")
    : [...new Set(useResourceLibraryStore.getState().skills.flatMap(skill => skill.source_repo ? [skill.source_repo.toLowerCase()] : []))];
  const scopes = planned.length ? planned.map(repository => [repository]) : [null];
  const id = useTaskQueueStore
    .getState()
    .enqueue({
      parentReportGeneration: parentReportGeneration ?? reportGeneration ?? undefined,
      key: `check:${repositories?.slice().sort().join(",") ?? "all"}:${reportGeneration ?? "legacy"}`,
      kind: "check",
      locks: repositories?.length ? repositories.map(repo => `repo:${repo.toLowerCase()}`) : ["repo:*"],
      label: repositories?.length ? i18n.t("workflow.checkRepositories", {count:repositories.length}) : i18n.t("workflow.allSourceRepositories"),
      steps: scopes.map(scope => ({
        label: scope?.[0] ?? i18n.t("workflow.recheck"),
        command: "preview_source_backed_resource_repository_updates",
        args: { repositories: scope, reportGeneration, plannedRepositories: planned },
      })),
    });
  const task = await waitForTask(id);
  if (task.status === "cancelled" || task.status === "interrupted") throw new CheckPaused();
  if (task.status !== "success" && task.status !== "partial")
    throw new Error(task.steps.find(step => step.error)?.error ?? i18n.t("workflow.operationFailed"));
  return task.steps.reduce<RepositorySyncPreviewReport>((report, step) => {
    const result = step.result as RepositorySyncPreviewReport | undefined;
    return result ? {repositories: [...report.repositories, ...result.repositories],
      coverage: mergeCoverage(report.coverage, result.coverage)} : report;
  }, {repositories: []});
}
registerTaskResult("apply_repository_update_item", async (step) => {
  useRepositorySyncStore.setState((state) => {
    if (!state.preview) return {};
    const action = step.args.action;
    if (action !== "added" && action !== "modified" && action !== "deleted" && action !== "replace") return {};
    const preview = {...state.preview, repositories:state.preview.repositories.map(repo => {
      if (repo.repository.toLowerCase() !== String(step.args.repository).toLowerCase()) return repo;
      const matches = (item: (typeof repo.added)[number]) => item.skillId === step.args.skillId && (item.version ?? "") === step.args.version;
      if (action === "replace") {
        const replacement=repo.added.find(item=>item.skillId===step.args.replacementSkillId && item.version===step.args.replacementVersion);
        if (!replacement || !repo.deleted.some(matches)) return repo;
        return {...repo,deleted:repo.deleted.filter(item=>!matches(item)),added:repo.added.filter(item=>item!==replacement),
          updated:[...(repo.updated ?? []).filter(item=>item.skillId!==step.args.skillId),{...replacement,skillId:String(step.args.skillId),files:[]}]};
      }
      const completed = repo[action].filter(matches);
      return {...repo, [action]:repo[action].filter(item => !matches(item)),
        updated: [...(repo.updated ?? []).filter(item => !completed.some(done => done.skillId === item.skillId)), ...completed.map(item => ({...item, files:[]}))]};
    })};
    const updateErrors = {...state.updateErrors};
    delete updateErrors[updateStepKey(step.args)];
    return {preview, updateErrors, applied:preview.repositories.every(repo => !repo.error && !hasRepositoryChanges(repo))};
  });
});

registerTaskCompletion("apply_repository_update_item", async finished => {
  if (finished.batchId && useTaskQueueStore.getState().tasks.some(task =>
    task.batchId === finished.batchId && isTaskActive(task))) return;
  await Promise.all([
    useResourceLibraryStore.getState().loadResourceLibrary(),
    useCentralSkillsStore.getState().loadCentralSkills(),
  ]);
});

function updateStepKey(args: Record<string, unknown>): string {
  const key = `${String(args.repository).toLowerCase()}:${args.skillId}:${args.version ?? ""}`;
  return args.action === "replace" ? `${key}:replace:${String(args.repository).toLowerCase()}:${args.replacementSkillId}:${args.replacementVersion ?? ""}` : key;
}

registerTaskFailure("apply_repository_update_item", step => {
  // Store only the sanitized task error, separately from disposable task history.
  const key = updateStepKey(step.args);
  useRepositorySyncStore.setState(state => ({updateErrors: {...state.updateErrors, [key]: step.error ?? i18n.t("workflow.operationFailed")}}));
});

registerTaskExecutor(
  "preview_source_backed_resource_repository_updates",
  (args) =>
    useResourceLibraryStore
      .getState()
      .previewRepositorySync(args.repositories as string[] | undefined),
);

registerTaskResult("update_source_backed_resource_skill", async () => {
  await useResourceLibraryStore.getState().loadResourceLibrary();
});

for (const command of ["refresh_repository_stars", "refresh_repository"]) registerTaskResult(command, (step, result) => {
  const repository = String(step.args.repository).toLowerCase();
  if (typeof result !== "number") return;
  if (command === "refresh_repository") useRepositoryDocumentStore.getState().invalidate();
  useResourceLibraryStore.setState(state => ({skills: state.skills.map(skill => skill.source_repo?.toLowerCase() === repository ? {...skill, github_stars: result} : skill)}));
  useCentralSkillsStore.setState(state => ({skills: state.skills.map(skill => skill.source_repo?.toLowerCase() === repository ? {...skill, github_stars: result} : skill)}));
});

registerTaskResult(
  "preview_source_backed_resource_repository_updates",
  (step, result) => {
    // A check started before the latest full refresh cannot restore its old report.
    if ((step.args.reportGeneration ?? null) !== useRepositorySyncStore.getState().reportGeneration) return;
    const rawReport = result as RepositorySyncPreviewReport;
    if (!rawReport?.repositories) return;
    const planned = step.args.plannedRepositories as string[] | undefined;
    if (planned?.length && rawReport.coverage) rawReport.coverage = rawReport.coverage.map(row =>
      row.reason === "outside_scope" && row.repository && planned.includes(row.repository.toLowerCase())
        ? {...row, status: "pending", reason: undefined} : row);
    const scope = Array.isArray(step.args.repositories) ? new Set((step.args.repositories as string[]).map(repo => repo.toLowerCase())) : null;
    releaseSnapshots((useRepositorySyncStore.getState().preview?.repositories ?? []).filter(old => rawReport.repositories.some(next=>next.repository.toLowerCase()===old.repository.toLowerCase() && next.snapshotId!==old.snapshotId)));
    const report = {...rawReport, repositories:rawReport.repositories.filter(repo => !scope || scope.has(repo.repository.toLowerCase()))};
    useRepositorySyncStore.setState((state) => ({
      updateErrors: Object.fromEntries(Object.entries(state.updateErrors).filter(([key]) =>
        !report.repositories.some(repo => !repo.error && key.startsWith(`${repo.repository.toLowerCase()}:`)))),
      reportCheckedAt: state.reportCheckedAt ?? Date.now(),
      checkedAt: {...state.checkedAt, ...Object.fromEntries(report.repositories.map(repo => [repo.repository.toLowerCase(), Date.now()]))},
      preview:
        step.args.repositories && state.preview
          ? {
              ...state.preview,
              coverage: mergeCoverage(state.preview.coverage, report.coverage),
              repositories: [
                ...state.preview.repositories.map(
                  (repo) =>
                    report.repositories.find(
                      (r) =>
                        r.repository.toLowerCase() ===
                        repo.repository.toLowerCase(),
                    ) ?? repo,
                ),
                ...report.repositories.filter(
                  (repo) =>
                    !state.preview!.repositories.some(
                      (r) =>
                        r.repository.toLowerCase() ===
                        repo.repository.toLowerCase(),
                    ),
                ),
              ],
            }
          : report,
    }));
  },
);

function mergeCoverage(previous: RepositorySyncPreviewReport["coverage"], incoming: RepositorySyncPreviewReport["coverage"]) {
  if (!previous) return incoming;
  // A partial recheck changes outcomes, never the frozen denominator.
  return previous.map(item => incoming?.find(next => next.skillId === item.skillId && next.reason !== "outside_scope" && next.status !== "pending") ?? item);
}
registerTaskResult("apply_repository_snapshot", async step => {
  useRepositorySyncStore.setState(state => {
    if (!state.preview) return {};
    const preview = {...state.preview, repositories:state.preview.repositories.map(repo => repo.snapshotId === step.args.snapshotId
      ? {...repo, updated:[...(repo.updated ?? []), ...repo.added, ...repo.modified, ...repo.deleted], added:[], modified:[], deleted:[], documentChanges:[]}
      : repo)};
    const updateErrors = {...state.updateErrors};
    delete updateErrors[`mirror:${step.args.snapshotId}`];
    return {preview, updateErrors, applied:preview.repositories.every(repo => !repo.error && !hasRepositoryChanges(repo))};
  });
  useRepositoryDocumentStore.getState().invalidate();
});
registerTaskFailure("apply_repository_snapshot", step => {
  useRepositorySyncStore.setState(state => ({updateErrors:{...state.updateErrors,[`mirror:${step.args.snapshotId}`]:step.error ?? i18n.t("workflow.operationFailed")}}));
});
registerTaskCompletion("apply_repository_snapshot", async () => {
  await Promise.all([useResourceLibraryStore.getState().loadResourceLibrary(),useCentralSkillsStore.getState().loadCentralSkills()]);
});

function releaseSnapshots(repositories: RepositorySyncPreview[]) {
  if (!isTauriRuntime()) return;
  const tasks = useTaskQueueStore.getState().tasks;
  const snapshotIds = repositories.flatMap(repo => repo.snapshotId && !tasks.some(task => isTaskActive(task) && task.steps.some(step=>step.args.snapshotId===repo.snapshotId)) ? [repo.snapshotId] : []);
  if (snapshotIds.length) void invoke("release_repository_snapshots", {snapshotIds}).catch(()=>{/* A retained cache is safe; never fail a check because cleanup failed. */});
}
