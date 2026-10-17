import {useEffect, useState} from "react";
import {useTranslation} from "react-i18next";
import {toast} from "sonner";
import {Button} from "@/components/ui/button";
import {Input} from "@/components/ui/input";
import {Dialog, DialogContent, DialogHeader, DialogTitle} from "@/components/ui/dialog";
import {useRepositorySyncStore} from "@/stores/repositorySyncStore";
import {isTaskActive, useTaskQueueStore} from "@/stores/taskQueueStore";

export function RepositoryAddressButton({repository, open: controlledOpen, onOpenChange}: {repository:string;open?:boolean;onOpenChange?:(open:boolean)=>void}) {
  const {t} = useTranslation();
  const [localOpen,setLocalOpen] = useState(false);
  const open = controlledOpen ?? localOpen;
  const setOpen = onOpenChange ?? setLocalOpen;
  const [address,setAddress] = useState("");
  const [error,setError] = useState("");
  useEffect(()=>{if(open){setAddress(repository);setError("");}},[open,repository]);
  const [saving,setSaving] = useState(false);
  const state = useRepositorySyncStore();
  const active = useTaskQueueStore(s => s.tasks.some(isTaskActive));
  const busy = active || !!state.migratingRepository || state.isChecking || !!state.checkingRepository;
  async function save() {
    setSaving(true); setError("");
    try {
      await state.changeRepositoryAddress(repository,address.trim());
      toast.success(t("workflow.address.success")); setOpen(false);
    } catch (error) {
      const code = error instanceof Error ? error.message : String(error);
      setError(t(`workflow.address.errors.${code}`, {defaultValue:t("workflow.address.errors.unknown")}));
    } finally {setSaving(false);}
  }
  return <>
    {controlledOpen === undefined && <Button variant="outline" disabled={busy} onClick={()=>setOpen(true)}>{t("workflow.address.title")}</Button>}
    <Dialog open={open} onOpenChange={value=>{if(!saving)setOpen(value);}}><DialogContent>
      <DialogHeader><DialogTitle>{t("workflow.address.title")}</DialogTitle></DialogHeader>
      <p className="text-sm">{t("workflow.address.hint")}</p>
      <p className="break-all text-sm">{t("workflow.address.current")}: {repository}</p>
      <label className="space-y-2 text-sm"><span>{t("workflow.address.newAddress")}</span><Input value={address} disabled={saving} onChange={e=>setAddress(e.target.value)} placeholder="owner/repo"/></label>
      {error && <p role="alert" className="text-xs text-destructive">{error}</p>}
      <div className="flex justify-end gap-2"><Button variant="outline" disabled={saving} onClick={()=>setOpen(false)}>{t("common.cancel")}</Button><Button disabled={busy || saving || !address.trim()} onClick={()=>void save()}>{t(saving ? "workflow.address.saving" : "workflow.address.save")}</Button></div>
    </DialogContent></Dialog>
  </>;
}
