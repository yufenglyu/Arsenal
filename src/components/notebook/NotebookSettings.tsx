import { HintIcon } from "@/components/ui/hint-icon";
import { ActionIcon } from "@/components/ui/action-icon";
import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { ChevronRight, FolderOpen, ExternalLink, Link } from "lucide-react";
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
  const [folder, setFolder] = useState("Arsenal");
  const [template, setTemplate] = useState(defaultTemplate.template);
  const [busy, setBusy] = useState(false);
  useEffect(() => { void useNotebookStore.getState().loadConfig(); }, []);
  useEffect(() => { setVault(config.obsidian_vault_path ?? ""); setFolder(config.obsidian_note_folder ?? "Arsenal"); setTemplate(config.obsidian_property_template || defaultTemplate.template); }, [config]);
  return (
    <Card>
      <details className="group/notebook-settings">
        <summary className="cursor-pointer list-none [&::-webkit-details-marker]:hidden">
          <CardHeader>
            <div className="flex items-center gap-2">
              <ActionIcon action="notebook"/>
              <CardTitle role="heading" aria-level={2} className="flex items-center gap-2">
                {t("g.notebook")}
                <ChevronRight className="size-[1em] shrink-0 group-open/notebook-settings:rotate-90" />
              </CardTitle>
              <HintIcon text={`${t("notebookSettings.hint")}\n\n${t("notebookSettings.backup")}`} />
            </div>
          </CardHeader>
        </summary>
        <CardContent className="space-y-4 pt-4">
          <div className="space-y-2">
            <label htmlFor="notebook-vault" className="block text-sm">{t("notebookSettings.vault")}</label>
            <div className="flex flex-col gap-2 sm:flex-row sm:items-center">
              <Input id="notebook-vault" className="min-w-0 flex-1" value={vault} disabled={busy} onChange={e => setVault(e.target.value)} />
              <Button variant="outline" className="shrink-0 sm:min-w-20" disabled={busy}
                onClick={() => void useNotebookStore.getState().chooseVault().then(path => { if (path) setVault(path); })}>
                <FolderOpen className="size-[1em]" />{t("common.browse")}
              </Button>
              <Button variant="outline" className="shrink-0 sm:min-w-20" disabled={busy || !vault.trim()}
                onClick={() => void useNotebookStore.getState().openVault(vault)}>
                <ExternalLink className="size-[1em]" />{t("settings.openPath")}
              </Button>
            </div>
          </div>
          <div className="space-y-2">
            <div className="flex items-center gap-1">
              <label htmlFor="notebook-folder" className="text-sm">{t("notebookSettings.folder")}</label>
              <HintIcon text={t("notebookSettings.folderHint")} className="text-xs [&_svg]:size-[1em]" />
            </div>
            <Input id="notebook-folder" value={folder} disabled={busy} onChange={e => setFolder(e.target.value)} />
          </div>
          <div className="space-y-2">
            <div className="flex items-center gap-1">
              <label htmlFor="notebook-template" className="text-sm">{t("notebookSettings.template")}</label>
              <HintIcon text={t("notebookSettings.templateIdentity")} className="text-xs [&_svg]:size-[1em]" />
            </div>
            <Textarea id="notebook-template" className="min-h-64 font-mono" value={template} disabled={busy} onChange={e => setTemplate(e.target.value)} />
          </div>
          <div className="flex flex-wrap justify-end gap-2">
            <Button variant="outline" disabled={busy} onClick={() => setTemplate(defaultTemplate.template)}>
              <ActionIcon action="reset" />{t("notebookSettings.reset")}
            </Button>
            <Button variant="outline" disabled={busy} onClick={() => void useNotebookStore.getState().registerProtocol()}>
              <Link className="size-[1em]" />{t("notebookSettings.repair")}
            </Button>
            <Button disabled={busy} onClick={() => { setBusy(true); void useNotebookStore.getState().saveConfig(vault, folder, template).finally(() => setBusy(false)); }}>
              <ActionIcon action="save" />{t("g.save")}
            </Button>
          </div>
        </CardContent>
      </details>
    </Card>
  );
}
