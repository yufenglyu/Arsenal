import type { KeyboardEvent } from "react";
import { matchesShortcutEvent, shouldIgnoreShortcutTarget } from "@/lib/shortcutKeys";
import { useShortcutStore } from "@/stores/shortcutStore";

export function useExpansionShortcuts(expand: () => void, collapse: () => void, arrowKeys = false) {
  const shortcuts = useShortcutStore(state => state.shortcuts);
  return (event: KeyboardEvent<HTMLElement>) => {
    if (event.nativeEvent.isComposing || shouldIgnoreShortcutTarget(event.target)) return;
    const plainArrow = arrowKeys && !event.altKey && !event.ctrlKey && !event.metaKey && !event.shiftKey;
    const action = plainArrow && event.key === "ArrowRight" ? expand
      : plainArrow && event.key === "ArrowLeft" ? collapse
      : matchesShortcutEvent(event.nativeEvent, shortcuts.expandSection) || (shortcuts.expandSection === "plus" && event.key === "=" && !event.ctrlKey && !event.metaKey && !event.altKey && !event.shiftKey) ? expand
      : matchesShortcutEvent(event.nativeEvent, shortcuts.collapseSection) ? collapse : null;
    if (!action) return;
    event.preventDefault();
    event.stopPropagation();
    action();
  };
}
