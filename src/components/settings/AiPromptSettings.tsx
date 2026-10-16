import { Save, RotateCcw } from "lucide-react";
import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import { Textarea } from "@/components/ui/textarea";
import { DEFAULT_NOTE_PROMPT, DEFAULT_REPOSITORY_NOTE_PROMPT, DEFAULT_TAGS_PROMPT, useAiPromptStore } from "@/stores/aiPromptStore";

export function AiPromptSettings() {
  const { t } = useTranslation();
  const load = useAiPromptStore(s => s.load);
  const save = useAiPromptStore(s => s.save);
  const [note, setNote] = useState(DEFAULT_NOTE_PROMPT);
  const [repositoryNote, setRepositoryNote] = useState(DEFAULT_REPOSITORY_NOTE_PROMPT);
  const [tags, setTags] = useState(DEFAULT_TAGS_PROMPT);
  const [ready, setReady] = useState(false);
  const [saving, setSaving] = useState(false);
  useEffect(() => {
    let active = true;
    load().then(values => {
      if (active) { setNote(values.note); setTags(values.tags); setRepositoryNote(values.repositoryNote); setReady(true); }
    }).catch(() => { if (active) toast.error(t("settings.configLoadFailed")); });
    return () => { active = false; };
  }, [load, t]);
  async function handleSave() {
    setSaving(true);
    try { await save(note, tags, repositoryNote); toast.success(t("settings.aiPromptsSaved")); }
    catch { toast.error(t("settings.configSaveFailed")); }
    finally { setSaving(false); }
  }
  return <div className="space-y-3">
    <div className="space-y-2">
      <div className="text-sm font-medium">{t("settings.aiNotePrompt")}</div>
      <Textarea aria-label={t("settings.aiNotePrompt")} id="ai-note-prompt" className="min-h-28 text-sm" value={note} onChange={e => setNote(e.target.value)} disabled={!ready || saving} />
    </div>
    <div className="space-y-2">
      <div className="text-sm font-medium">{t("settings.aiTagsPrompt")}</div>
      <Textarea aria-label={t("settings.aiTagsPrompt")} id="ai-tags-prompt" className="min-h-24 text-sm" value={tags} onChange={e => setTags(e.target.value)} disabled={!ready || saving} />
    </div>

    <div className="space-y-2">
      <div className="text-sm font-medium">{t("settings.aiRepositoryNotePrompt")}</div>
      <Textarea aria-label={t("settings.aiRepositoryNotePrompt")} id="ai-repository-note-prompt" className="min-h-28 text-sm" value={repositoryNote} onChange={e => setRepositoryNote(e.target.value)} disabled={!ready || saving} />
    </div>
    <div className="flex flex-wrap justify-end gap-2">
      <Button variant="outline" size="sm" disabled={!ready || saving} onClick={() => { setNote(DEFAULT_NOTE_PROMPT); setTags(DEFAULT_TAGS_PROMPT); setRepositoryNote(DEFAULT_REPOSITORY_NOTE_PROMPT); }}><RotateCcw className="size-4" />{t("settings.aiPromptsReset")}</Button>
      <Button size="sm" disabled={!ready || saving} onClick={() => void handleSave()}><Save className="size-4" />{t(saving ? "detail.savingMetadata" : "settings.aiPromptsSave")}</Button>
    </div>
  </div>;
}
