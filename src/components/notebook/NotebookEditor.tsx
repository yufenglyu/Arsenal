import { useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { Link2 } from "lucide-react";
import { Textarea } from "@/components/ui/textarea";
import { Button } from "@/components/ui/button";
import { useNotebookStore } from "@/stores/notebookStore";
import type { Repository } from "@/types/notebook";
import { NotebookMarkdown } from "./NotebookMarkdown";
import { NotebookToolbar, NotebookFormattingToolbar } from "./NotebookToolbar";
import { defaultNotebookName } from "@/lib/notebookNames";
import { matchesShortcut } from "@/lib/notebookShortcut";
import { NotebookLinkDialog, ObsidianNotebook } from "./ObsidianNotebook";

// Keep unsaved edits during repository/tab switches, only for this app session.
const drafts = new Map<number, {body: string; baseline: string}>();

export function NotebookEditor({ repository }: { repository: Repository }) {
  return repository.obsidian ? <ObsidianNotebook key={repository.id} repository={repository}/> : <InternalNotebookEditor key={repository.id} repository={repository}/>;
}
function InternalNotebookEditor({ repository }: { repository: Repository }) {
  const { t } = useTranslation();
  const saveField = useNotebookStore(s => s.saveField);
  const [value, setValue] = useState(() => drafts.get(repository.id)?.body ?? repository.notebook ?? "");
  const [saved, setSaved] = useState(drafts.get(repository.id)?.baseline ?? repository.notebook ?? "");
  const [preview, setPreview] = useState(true);
  const [saving, setSaving] = useState(false);
  const [linking, setLinking] = useState(false);
  const configured = useNotebookStore(s=>!!s.config.obsidian_vault_path);
  const editor = useRef<HTMLTextAreaElement>(null);
  function change(next: string) {
    setValue(next);
    drafts.set(repository.id, {body: next, baseline: saved});
  }
  async function save() {
    if (saving) return;
    setSaving(true);
    try {
      if (await saveField(repository.id, "notebook", value, saved)) {
        setSaved(value);
        const current = drafts.get(repository.id);
        if (current?.body === value) drafts.delete(repository.id);
        else if (current) drafts.set(repository.id, {...current, baseline: value});
      }
    } finally { setSaving(false); }
  }
  function insert(before: string, after = "", placeholder = t("g.markdownText")) {
    const input = editor.current;
    if (!input) return;
    const start = input.selectionStart, end = input.selectionEnd;
    const selected = value.slice(start, end) || placeholder;
    change(value.slice(0, start) + before + selected + after + value.slice(end));
    requestAnimationFrame(() => {
      input.focus();
      input.setSelectionRange(start + before.length, start + before.length + selected.length);
    });
  }
  return <section className="flex h-full min-h-0 flex-col" aria-label={t("g.notebook")}
    onKeyDown={e => { if (!e.repeat && matchesShortcut(e, "saveNotebook", useNotebookStore.getState().config)) { e.preventDefault(); e.stopPropagation(); void save(); } }}>
    <NotebookToolbar title={defaultNotebookName(repository.fullName)} onSave={()=>void save()} saveDisabled={saving || value === saved} preview={preview} onTogglePreview={()=>setPreview(!preview)}
      association={<Button size="icon-sm" variant="ghost" disabled={!configured||saving} aria-label={t("g.obsidian.link")} title={t(configured?"g.obsidian.link":"g.obsidian.setupHint")} onClick={()=>setLinking(true)}><Link2/></Button>}/>
    <div className="flex min-h-0 flex-1 flex-col gap-4 overflow-auto p-4">
    {!preview&&<NotebookFormattingToolbar unavailable={saving} onInsert={insert}/>}
    {!configured&&<p className="text-xs text-muted-foreground">{t("g.obsidian.setupHint")}</p>}
    {preview ? <div className="min-h-64 flex-1">{value ? <NotebookMarkdown content={value} /> : <p className="text-xs text-muted-foreground">{t("g.emptyNotebook")}</p>}</div>
      : <Textarea ref={editor} className="notebook-input min-h-64 flex-1 resize-y font-mono leading-relaxed" aria-label={t("g.notebookContent")}
        placeholder={t("g.notebookHint")} value={value} onChange={e => change(e.target.value)} />}
    <div className="flex items-center justify-between gap-2">
      <span className="text-xs text-muted-foreground">{value !== saved ? t("g.unsaved") : ""}</span>
    </div>
    </div>
    {linking&&<NotebookLinkDialog repository={repository} initialBody={value} create onClose={()=>setLinking(false)} onLinked={()=>drafts.delete(repository.id)}/>}
  </section>;
}
