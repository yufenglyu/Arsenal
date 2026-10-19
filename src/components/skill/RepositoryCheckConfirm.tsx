import { useTranslation } from "react-i18next";
import { RefreshCw, FolderSync, X } from "lucide-react";
import { useRepositorySyncStore } from "@/stores/repositorySyncStore";
import { Button } from "@/components/ui/button";
import { Dialog, DialogContent, DialogHeader, DialogTitle, DialogDescription, DialogFooter } from "@/components/ui/dialog";

export function RepositoryCheckConfirm({open,onOpenChange,repositories}: {
  open:boolean; onOpenChange:(open:boolean)=>void; repositories?:string[];
}) {
  const {t}=useTranslation();
  const state=useRepositorySyncStore();
  return <Dialog open={open} onOpenChange={onOpenChange}>
    <DialogContent className="sm:max-w-2xl">
      <DialogHeader>
        <DialogTitle className="flex items-center gap-2"><RefreshCw className="size-4" aria-hidden="true"/>{t("workflow.confirmCheckTitle")}</DialogTitle>
      </DialogHeader>
      <DialogDescription className="text-foreground leading-relaxed">{t(repositories?.length ? "workflow.confirmScopedCheck" : "workflow.confirmFullCheck")}</DialogDescription>
      {!!repositories?.length && <ul className="max-h-48 divide-y divide-border overflow-auto rounded-md border border-border">{repositories.map(repository=><li key={repository} className="break-all p-3">{repository}</li>)}</ul>}
      <DialogFooter className="flex-row flex-wrap items-center justify-end">
          <Button variant="outline" title={t("workflow.refreshRepositoryInfoHint")} disabled={state.isRefreshingRepositories} onClick={()=>{onOpenChange(false);void state.refreshRepositories(repositories);}}><FolderSync aria-hidden="true"/>{t("workflow.refreshRepositoryInfo")}</Button>
          <Button disabled={state.isChecking || !!state.checkingRepository} onClick={()=>{onOpenChange(false);void state.checkForUpdates(repositories);}}><RefreshCw aria-hidden="true"/>{t("workflow.startCheck")}</Button>
          <Button variant="outline" onClick={()=>onOpenChange(false)}><X aria-hidden="true"/>{t("common.cancel")}</Button>
      </DialogFooter>
    </DialogContent>
  </Dialog>;
}
