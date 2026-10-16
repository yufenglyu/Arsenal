import type { KeyboardEvent as ReactKeyboardEvent } from "react";
import { matchesShortcutEvent } from "./shortcutKeys";
import { useShortcutStore } from "@/stores/shortcutStore";
export function matchesShortcut(event: ReactKeyboardEvent, _action: string, _config: Record<string, string>) {
  return matchesShortcutEvent(event.nativeEvent, useShortcutStore.getState().shortcuts.saveNotebook);
}
