import {createPortal} from "react-dom";
import {StatusBarSlot} from "./StatusBarSlot";
import { GitHubUrlDrop } from "./GitHubUrlDrop";
import { focusPageSearch } from "@/lib/focusPageSearch";
import { SettingsView } from "@/pages/SettingsView";
import { useEffect, useRef, useState } from "react";
import { Outlet, useLocation, useNavigate } from "react-router-dom";
import { Sidebar } from "./Sidebar";
import { AppStatusBar } from "./AppStatusBar";
import { GlobalSearchDialog } from "./GlobalSearchDialog";
import { useSidebarStore } from "@/stores/sidebarStore";
import { useConfiguredHotkey } from "@/hooks/useConfiguredHotkey";
import { usePlatformStore } from "@/stores/platformStore";

/**
 * Top-level app shell: sidebar + scrollable main content area.
 * Triggers the initial platform scan on mount.
 */
export function AppShell({ settingsOpen = false, onCloseSettings }: {settingsOpen?: boolean; onCloseSettings?: () => void}) {
  const [statusSlot, setStatusSlot] = useState<HTMLDivElement | null>(null);
  const [isSearchOpen, setIsSearchOpen] = useState(false);
  const mainRef = useRef<HTMLElement | null>(null);
  const { pathname } = useLocation();
  const navigate = useNavigate();

  const initialize = usePlatformStore((s) => s.initialize);
  const toggleSidebar = useSidebarStore((s) => s.toggleExpanded);

  useEffect(() => {
    initialize();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  useEffect(() => {
    if (!mainRef.current) return;
    mainRef.current.scrollTop = 0;
  }, [pathname]);

  useEffect(() => {
    function preventDefaultContextMenu(event: MouseEvent) {
      event.preventDefault();
    }
    function preventDefaultPrintShortcut(event: KeyboardEvent) {
      if (
        (event.ctrlKey || event.metaKey) &&
        !event.altKey &&
        !event.shiftKey &&
        event.key.toLowerCase() === "p"
      ) {
        event.preventDefault();
        event.stopPropagation();
      }
    }

    window.addEventListener("contextmenu", preventDefaultContextMenu);
    window.addEventListener("keydown", preventDefaultPrintShortcut, true);
    return () => {
      window.removeEventListener("contextmenu", preventDefaultContextMenu);
      window.removeEventListener("keydown", preventDefaultPrintShortcut, true);
    };
  }, []);

  useConfiguredHotkey("focusPageSearch", () => focusPageSearch(mainRef.current), {allowInEditable: true});
  useConfiguredHotkey("toggleSidebar", toggleSidebar);
  useConfiguredHotkey("goResources", () => navigate("/resources"));
  useConfiguredHotkey("goCollections", () => {navigate("/resources");requestAnimationFrame(()=>document.querySelector<HTMLButtonElement>('[role="tree"] button')?.focus());});
  useConfiguredHotkey("goCentral", () => navigate("/central"));
  useConfiguredHotkey("goSettings", () => navigate("/settings"));

  return (
    <div className="flex h-screen bg-background text-foreground overflow-hidden">
      <GitHubUrlDrop />
      <Sidebar settingsOpen={settingsOpen} />
      <StatusBarSlot.Provider value={setStatusSlot}><div className="flex min-h-0 min-w-0 flex-1 flex-col">
        <main ref={mainRef} className="flex-1 min-h-0 min-w-0 overflow-hidden">
          <div className="h-full" hidden={settingsOpen}><Outlet /></div>
          {settingsOpen && <SettingsView onClose={onCloseSettings} />}
        </main>
        {statusSlot && !settingsOpen ? createPortal(<AppStatusBar />, statusSlot) : <AppStatusBar settingsOpen={settingsOpen} />}
      </div></StatusBarSlot.Provider>
      <GlobalSearchDialog
        open={isSearchOpen}
        onOpenChange={setIsSearchOpen}
      />
    </div>
  );
}
