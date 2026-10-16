import { useTranslation } from "react-i18next";
import { History, RefreshCw, FolderSync, X } from "lucide-react";
import { useRepositorySyncStore } from "@/stores/repositorySyncStore";
import { Button } from "@/components/ui/button";
import { Dialog, DialogContent, DialogHeader, DialogTitle, DialogDescription, DialogFooter } from "@/components/ui/dialog";

export function RepositoryCheckConfirm({open,onOpenChange,repositories}: {
  open:boolean; onOpenChange:(open:boolean)=>void; repositories?:string[];
}) {
  const {t}=useTranslation();
  const state=useRepositorySyncStore();
  return <Dialog open={open} onOpenChange={onOpenChange}>
    <DialogContent className="sm:max-w-xl">
      <DialogHeader>
        <DialogTitle className="flex items-center gap-2"><RefreshCw className="size-4" aria-hidden="true"/>{t("workflow.confirmCheckTitle")}</DialogTitle>
        <DialogDescription>{t(repositories?.length ? "workflow.confirmScopedCheck" : "workflow.confirmFullCheck")}</DialogDescription>
      </DialogHeader>
      <DialogFooter className="flex-row flex-wrap">
        <Button variant="outline" disabled={!state.preview} onClick={()=>{onOpenChange(false);state.setOpen(true);}}><History aria-hidden="true"/>{t("workflow.viewLastCheck")}</Button>
        <Button disabled={state.isChecking || !!state.checkingRepository} onClick={()=>{onOpenChange(false);void state.checkForUpdates(repositories);}}><RefreshCw aria-hidden="true"/>{t("workflow.startCheck")}</Button>
        <Button variant="outline" disabled={state.isRefreshingRepositories} onClick={()=>{onOpenChange(false);void state.refreshRepositories(repositories);}}><FolderSync aria-hidden="true"/>{t("workflow.refreshRepository")}</Button>
        <Button variant="outline" onClick={()=>onOpenChange(false)}><X aria-hidden="true"/>{t("common.cancel")}</Button>
      </DialogFooter>
    </DialogContent>
  </Dialog>;
}
