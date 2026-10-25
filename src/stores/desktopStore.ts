import { create } from "zustand";
import { confirm } from "@tauri-apps/plugin-dialog";
import { toast } from "sonner";
import i18n from "@/i18n";
import { invoke, listen } from "@/lib/tauri";
import { isTaskActive, useTaskQueueStore } from "./taskQueueStore";

export const useDesktopStore = create<{exiting: boolean; requestExit: () => Promise<void>}>((set, get) => ({
  exiting: false,
  requestExit: async () => {
    if (get().exiting) return;
    set({exiting: true});
    try {
      if (useTaskQueueStore.getState().tasks.some(isTaskActive) &&
          !await confirm(i18n.t("workflow.exitTasks"), {kind: "warning"})) return;
      await invoke("exit_app");
    } catch {
      toast.error(i18n.t("desktop.exitFailed"));
    } finally {
      set({exiting: false});
    }
  },
}));

export function startDesktopExitListener(): () => void {
  let stopped = false;
  let unlisten: (() => void) | undefined;
  void listen("arsenal-exit-requested", () => {void useDesktopStore.getState().requestExit();})
    .then(dispose => {if (stopped) dispose(); else unlisten = dispose;})
    .catch(() => {/* Browser previews have no desktop event bridge. */});
  return () => {stopped = true; unlisten?.();};
}
