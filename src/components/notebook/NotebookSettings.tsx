import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { BookOpen, ChevronRight, FolderOpen, Info } from "lucide-react";
import { useNotebookStore } from "@/stores/notebookStore";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Textarea } from "@/components/ui/textarea";
import { Card, CardHeader, CardTitle, CardContent } from "@/components/ui/card";
import defaultTemplate from "@/data/obsidianTemplate.json";

export function NotebookSettings() {
  const { t } = useTranslation();
  const config = useNotebookStore(s => s.config);
  const [vault, setVault] = useState("");
  const [folder, setFolder] = useState("SkillsHub");
  const [template, setTemplate] = useState(defaultTemplate.template);
  const [busy, setBusy] = useState(false);
  useEffect(() => { void useNotebookStore.getState().loadConfig(); }, []);
  useEffect(() => { setVault(config.obsidian_vault_path ?? ""); setFolder(config.obsidian_note_folder ?? "SkillsHub"); setTemplate(config.obsidian_property_template || defaultTemplate.template); }, [config]);
  return <Card><details className="group/notebook-settings"><summary className="cursor-pointer list-none [&::-webkit-details-marker]:hidden"><CardHeader><CardTitle className="flex items-center gap-2 text-sm font-semibold"><BookOpen className="size-[1em]"/>{t("g.notebook")}<ChevronRight className="size-[1em] group-open/notebook-settings:rotate-90"/><span tabIndex={0} title={t("notebookSettings.hint")} aria-label={t("notebookSettings.hint")}><Info className="size-[1em]"/></span></CardTitle></CardHeader></summary>
    <CardContent className="space-y-4">
      <label className="block space-y-2 text-sm"><span>{t("notebookSettings.vault")}</span><div className="flex gap-2"><Input value={vault} onChange={e => setVault(e.target.value)}/><Button variant="ghost" size="icon" title={t("notebookSettings.browse")} aria-label={t("notebookSettings.browse")} onClick={() => void useNotebookStore.getState().chooseVault().then(path => { if (path) setVault(path); })}><FolderOpen/></Button></div></label>
      <label className="block space-y-2 text-sm"><span>{t("notebookSettings.folder")}</span><Input value={folder} onChange={e => setFolder(e.target.value)}/></label>
      <label className="block space-y-2 text-sm"><span>{t("notebookSettings.template")}</span><Textarea className="min-h-64 font-mono" value={template} onChange={e => setTemplate(e.target.value)}/></label>
      <p className="text-xs text-muted-foreground">{t("notebookSettings.backup")}</p>
      <div className="flex flex-wrap gap-2"><Button variant="outline" onClick={() => setTemplate(defaultTemplate.template)}>{t("notebookSettings.reset")}</Button><Button variant="outline" onClick={() => void useNotebookStore.getState().registerProtocol()}>{t("notebookSettings.repair")}</Button><Button disabled={busy} onClick={() => { setBusy(true); void useNotebookStore.getState().saveConfig(vault, folder, template).finally(() => setBusy(false)); }}>{t("g.save")}</Button></div>
    </CardContent>
  </details></Card>;
}
