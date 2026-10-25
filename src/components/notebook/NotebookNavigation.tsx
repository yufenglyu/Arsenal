import { useEffect } from "react";
import { useNavigate } from "react-router-dom";
import { useNotebookStore } from "@/stores/notebookStore";
import { useResourceLibraryStore } from "@/stores/resourceLibraryStore";
import { useAppStatusStore } from "@/stores/appStatusStore";
import i18n from "@/i18n";

export function NotebookNavigation() {
  const navigate = useNavigate();
  useEffect(() => {
    let disposed = false;
    let stop: (() => void) | undefined;
    const consume = async (uid: string) => {
      await Promise.all([useNotebookStore.getState().load(), useResourceLibraryStore.getState().loadResourceLibrary()]);
      if (disposed) return;
      const row = useNotebookStore.getState().rows.find(r => r.repositoryUid === uid);
      const skill = useResourceLibraryStore.getState().skills.find(s => row?.skillIds.includes(s.id));
      if (!skill) {
        useAppStatusStore.getState().startTask({ id: "notebook-navigation", label: i18n.t("g.notebook") });
        useAppStatusStore.getState().failTask({ error: i18n.t("g.errors.noteRepositoryMissing") });
        return;
      }
      localStorage.setItem("arsenal.browser.tab", JSON.stringify("notebook"));
      window.dispatchEvent(new CustomEvent("arsenal-open-notebook"));
      navigate(`/resources?${new URLSearchParams({ locate: skill.id, view: "folders" })}`);
    };
    void useNotebookStore.getState().notebookNavigation(consume).then(unlisten => { if (disposed) unlisten(); else stop = unlisten; });
    return () => { disposed = true; stop?.(); };
  }, [navigate]);
  return null;
}
