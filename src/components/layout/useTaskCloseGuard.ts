import { useEffect } from "react";
import { useTranslation } from "react-i18next";
import { isTaskActive, useTaskQueueStore } from "@/stores/taskQueueStore";
import { isTauriRuntime } from "@/lib/tauri";

export function useTaskCloseGuard() {
  const { t } = useTranslation();
  useEffect(() => {
    const before = (event: BeforeUnloadEvent) => {
      if (useTaskQueueStore.getState().tasks.some(isTaskActive)) {
        event.preventDefault();
        event.returnValue = "";
      }
    };
    window.addEventListener("beforeunload", before);
    let dispose: (() => void) | undefined;
    let stopped = false;
    if (isTauriRuntime())
      void import("@tauri-apps/api/window")
        .then(async ({ getCurrentWindow }) => {
          const unlisten = await getCurrentWindow().onCloseRequested(
            async (event) => {
              if (!useTaskQueueStore.getState().tasks.some(isTaskActive))
                return;
              event.preventDefault();
              const { confirm } = await import("@tauri-apps/plugin-dialog");
              if (await confirm(t("workflow.exitTasks"), { kind: "warning" }))
                await getCurrentWindow().destroy();
            },
          );
          if (stopped) unlisten();
          else dispose = unlisten;
        })
        .catch(() => {
          /* No native window is available in browser previews. */
        });
    return () => {
      stopped = true;
      dispose?.();
      window.removeEventListener("beforeunload", before);
    };
  }, [t]);
}
