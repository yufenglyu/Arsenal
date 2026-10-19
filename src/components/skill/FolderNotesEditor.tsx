import { ActionIcon } from "@/components/ui/action-icon";
import { useEffect, useRef, useState } from "react";
import { useRepositoryDocumentStore } from "@/stores/repositoryDocumentStore";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { findFolderNote, useMetadataStore } from "@/stores/metadataStore";
import { Button } from "@/components/ui/button";
import { Textarea } from "@/components/ui/textarea";
export function FolderNotesEditor({ skillIds, directory }: { skillIds: string[]; directory?: string }) {
  const { t, i18n } = useTranslation();
  const error = useMetadataStore((s) => s.folderError);
  const folders = useMetadataStore((s) => s.folders);
  const note = findFolderNote(folders, skillIds);
  const identity = note?.id ?? skillIds.join("|");
  const [value, setValue] = useState(note?.notes ?? "");
  const [busy, setBusy] = useState(false);
  const [generating, setGenerating] = useState(false);
  const request = useRef(0);
  useEffect(() => () => { request.current += 1; }, [identity, directory]);
  async function generate() {
    if (!directory || generating) return;
    const current = ++request.current;
    setGenerating(true);
    try {
      const generated = await useRepositoryDocumentStore.getState().generateNote(directory, skillIds[0], i18n.language);
      if (current === request.current) setValue(generated);
    } catch (error) {
      if (current === request.current) toast.error(error instanceof Error ? error.message : t("workflow.operationFailed"));
    } finally {
      if (current === request.current) setGenerating(false);
    }
  }
  useEffect(() => {
    setValue(note?.notes ?? "");
  }, [identity, note?.notes]);
  async function save(notes: string) {
    setBusy(true);
    try {
      await useMetadataStore
        .getState()
        .saveFolder({ id: note?.id ?? crypto.randomUUID(), notes, skillIds });
      setValue(notes);
    } catch {
      toast.error(t("workflow.operationFailed"));
    } finally {
      setBusy(false);
    }
  }
  return (
    <section className="flex flex-col gap-3">
      {error && (
        <p className="text-xs text-destructive">
          {t("workflow.operationFailed")}
        </p>
      )}
      <label className="block text-xs text-muted-foreground">
        <span className="mb-3 flex items-center gap-2 text-sm font-semibold"><ActionIcon action="remark"/>{t("workflow.folderNotes")}</span>
        <Textarea
          className="mt-0"
          value={value}
          onChange={(e) => setValue(e.target.value)}
          disabled={busy || generating}
        />
      </label>
      <div className="flex justify-end gap-2">
        {directory && <Button variant="outline" disabled={busy || generating} aria-label={t("workflow.generateRepositoryNote")} onClick={() => void generate()}><ActionIcon action="ai"/>{t(generating ? "detail.explanationLoading" : "workflow.ai")}</Button>}
        <Button
          size="sm"
          variant="outline"
          disabled={busy || generating}
          onClick={() => void save(value)}
        ><ActionIcon action="save"/>
          {t("common.save")}
        </Button>
        <Button
          size="sm"
          variant="outline"
          disabled={busy || generating}
          onClick={() => void save("")}
        ><ActionIcon action="delete"/>
          {t("workflow.clear")}
        </Button>
      </div>
    </section>
  );
}
