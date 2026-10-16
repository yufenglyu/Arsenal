import { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { Link2 } from "lucide-react";
import { toast } from "sonner";
import { acceptsGitHubUrlDrag, droppedGitHubUrls, droppedShortcutGitHubUrls } from "@/lib/githubUrlDrop";
import { enqueueGitHubUrlImport } from "@/stores/importPreparationStore";

/** Listen on the window so links can also be dropped over portals and dialogs. */
export function GitHubUrlDrop() {
  const { t } = useTranslation();
  const [active, setActive] = useState(false);
  const translate = useRef(t);
  translate.current = t;
  useEffect(() => {
    let internal = false;
    let depth = 0;
    const reset = () => { internal = false; depth = 0; setActive(false); };
    const start = () => { internal = true; };
    const accepts = (event: DragEvent) => !internal && event.dataTransfer && acceptsGitHubUrlDrag(event.dataTransfer);
    const enter = (event: DragEvent) => {
      if (!accepts(event)) return;
      depth++;
      setActive(true);
    };
    const over = (event: DragEvent) => {
      if (!accepts(event)) return;
      event.preventDefault();
      event.dataTransfer!.dropEffect = ["link", "linkMove"].includes(event.dataTransfer!.effectAllowed) ? "link" : "copy";
      setActive(true);
    };
    const leave = () => { depth = Math.max(0, depth - 1); if (!depth) setActive(false); };
    const drop = async (event: DragEvent) => {
      const accepted = accepts(event);
      reset();
      if (!accepted || !event.dataTransfer) return;
      let urls = droppedGitHubUrls(event.dataTransfer);
      const shortcuts = Array.from(event.dataTransfer.files ?? []).filter(file => file.name.toLowerCase().endsWith(".url"));
      if (!urls.length && shortcuts.length) {
        // Cancel default navigation before awaiting the file contents.
        event.preventDefault();
        event.stopPropagation();
        try { urls = await droppedShortcutGitHubUrls(shortcuts); }
        catch { toast.info(translate.current("workflow.githubDropUnsupported")); return; }
        if (!urls.length) { toast.info(translate.current("workflow.githubDropUnsupported")); return; }
      }
      // Keep ordinary text drops editable, but prevent browser link navigation.
      if (!urls.length) {
        if (Array.from(event.dataTransfer.types).some(type => ["text/uri-list", "text/x-moz-url"].includes(type))
          || /^https?:\/\//i.test(event.dataTransfer.getData("text/plain").trim())) {
          event.preventDefault();
          event.stopPropagation();
          toast.info(translate.current("workflow.githubDropUnsupported"));
        }
        return;
      }
      event.preventDefault();
      event.stopPropagation();
      for (const url of urls) enqueueGitHubUrlImport(url);
      toast.info(translate.current("workflow.githubDropQueued", { count: urls.length }));
    };
    window.addEventListener("dragstart", start, true);
    window.addEventListener("dragenter", enter, true);
    window.addEventListener("dragover", over, true);
    window.addEventListener("dragleave", leave, true);
    window.addEventListener("drop", drop, true);
    window.addEventListener("dragend", reset, true);
    window.addEventListener("blur", reset);
    return () => {
      window.removeEventListener("dragstart", start, true);
      window.removeEventListener("dragenter", enter, true);
      window.removeEventListener("dragover", over, true);
      window.removeEventListener("dragleave", leave, true);
      window.removeEventListener("drop", drop, true);
      window.removeEventListener("dragend", reset, true);
      window.removeEventListener("blur", reset);
    };
  }, []);
  return active ? <div className="pointer-events-none fixed inset-2 z-[60] flex items-center justify-center rounded-md border-2 border-dashed border-primary">
    <div role="status" className="flex items-center gap-2 rounded-md border border-border bg-background p-4 text-sm font-semibold shadow-lg">
      <Link2 className="size-[1em] shrink-0" aria-hidden="true"/>{t("workflow.githubDropHint")}
    </div>
  </div> : null;
}
