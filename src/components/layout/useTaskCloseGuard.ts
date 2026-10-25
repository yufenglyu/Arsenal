import { useEffect } from "react";
import { startDesktopExitListener } from "@/stores/desktopStore";
import { isTaskActive, useTaskQueueStore } from "@/stores/taskQueueStore";
import { isTauriRuntime } from "@/lib/tauri";

export function useTaskCloseGuard() {
  useEffect(() => {
    // Native close hides to tray; only explicit Quit asks about active tasks.
    if (isTauriRuntime()) return startDesktopExitListener();
    const before = (event: BeforeUnloadEvent) => {
      if (useTaskQueueStore.getState().tasks.some(isTaskActive)) {
        event.preventDefault();
        event.returnValue = "";
      }
    };
    window.addEventListener("beforeunload", before);
    return () => {
      window.removeEventListener("beforeunload", before);
    };
  }, []);
}
