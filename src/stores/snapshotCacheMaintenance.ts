import {invoke, isTauriRuntime} from "@/lib/tauri";
import {retainedSnapshotIds, useRepositorySyncStore as sync} from "./repositorySyncStore";
import {isTaskActive, useTaskQueueStore as queue} from "./taskQueueStore";

const DEBOUNCE_MS = 250;
const RETRY_MS = 5 * 60 * 1000;

// Start after persisted reports/tasks hydrate. Never infer an empty retention set
// from not-yet-loaded localStorage, and never sweep while checks publish results.
export function startSnapshotCacheMaintenance(): () => void {
  if (!isTauriRuntime()) return () => {};
  let stopped = false;
  let running = false;
  let dirty = false;
  let timer: ReturnType<typeof setTimeout> | undefined;
  const ready = () => sync.persist.hasHydrated() && queue.persist.hasHydrated();
  const busy = () => sync.getState().isChecking || !!sync.getState().checkingRepository ||
    !!sync.getState().migratingRepository || queue.getState().tasks.some(task => isTaskActive(task) &&
      task.steps.some(step => ["preview_source_backed_resource_repository_updates", "apply_repository_snapshot"].includes(step.command)));
  const schedule = () => {
    dirty = true;
    if (stopped || timer || running || !ready()) return;
    timer = setTimeout(() => { timer = undefined; void clean(); }, DEBOUNCE_MS);
  };
  const clean = async () => {
    if (stopped || running || !ready() || busy()) return;
    running = true;
    dirty = false;
    try {
      // Two-phase collection: only release IDs returned by this inventory. A new
      // snapshot created while the IPC waits cannot accidentally enter this sweep.
      const known = await invoke<string[]>("list_repository_snapshot_cache");
      if (stopped || !ready() || busy()) return;
      const retained = new Set(retainedSnapshotIds());
      const snapshotIds = known.filter(id => !retained.has(id));
      if (snapshotIds.length) await invoke("release_repository_snapshots", {snapshotIds});
    } catch {
      // Cleanup failure never fails a business task. Retry on a later lifecycle
      // event or the bounded timer, rather than looping on a failed IPC/write.
    } finally {
      running = false;
      if (dirty && !stopped) schedule();
    }
  };
  const subscriptions = [
    sync.subscribe((next, previous) => {
      if (next.preview !== previous.preview || next.isChecking !== previous.isChecking ||
        next.checkingRepository !== previous.checkingRepository || next.migratingRepository !== previous.migratingRepository) schedule();
    }),
    queue.subscribe((next, previous) => {
      if (next.tasks.length !== previous.tasks.length || next.tasks.some((task, index) =>
        task.id !== previous.tasks[index]?.id || task.status !== previous.tasks[index]?.status ||
        task.clearRequested !== previous.tasks[index]?.clearRequested)) schedule();
    }),
    sync.persist.onFinishHydration(schedule),
    queue.persist.onFinishHydration(schedule),
  ];
  const interval = setInterval(schedule, RETRY_MS);
  schedule();
  return () => {
    stopped = true;
    if (timer) clearTimeout(timer);
    clearInterval(interval);
    subscriptions.forEach(unsubscribe => unsubscribe());
  };
}
