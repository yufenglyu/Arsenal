import { ActionIcon } from "@/components/ui/action-icon";
import type { ReactNode } from "react";
import { Bold, Italic, Heading, List, Link, Code, Pencil, Eye, Save } from "lucide-react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";

const actions = [
  { key: "bold", Icon: Bold, before: "**", after: "**" },
  { key: "italic", Icon: Italic, before: "*", after: "*" },
  { key: "heading", Icon: Heading, before: "\n## ", after: "\n" },
  { key: "list", Icon: List, before: "\n- ", after: "\n" },
  { key: "link", Icon: Link, before: "[", after: "](https://example.com)" },
  { key: "code", Icon: Code, before: "\n```\n", after: "\n```\n" },
];

export function NotebookToolbar({ title, preview, unavailable = false, onTogglePreview, association, onSave, saveDisabled }: {
  title: ReactNode;
  preview: boolean;
  unavailable?: boolean;
  onTogglePreview: () => void;
  association: ReactNode;
  onSave: () => void;
  saveDisabled: boolean;
}) {
  const { t } = useTranslation();
  return <div className="flex shrink-0 items-center gap-2 border-b border-border px-4 py-2">
    <h1 className="flex min-w-0 flex-1 items-center gap-2 text-sm font-semibold"><ActionIcon action="notebook"/><span className="min-w-0 flex-1 truncate">{title}</span></h1>
    <div className="flex shrink-0 items-center gap-2">
    <Button variant="ghost" size="icon-sm" disabled={saveDisabled} onClick={onSave}
      title={t("g.save")} aria-label={t("g.save")}><Save /></Button>
    <Button variant="ghost" size="icon-sm" disabled={unavailable} aria-pressed={preview}
      title={t(preview ? "g.editNotebook" : "g.previewNotebook")} aria-label={t(preview ? "g.editNotebook" : "g.previewNotebook")}
      onClick={onTogglePreview}>{preview ? <Pencil /> : <Eye />}</Button>
    {association}
    </div>
  </div>;
}

export function NotebookFormattingToolbar({ unavailable=false, onInsert }: {unavailable?:boolean;onInsert:(before:string,after:string)=>void}) {
  const {t}=useTranslation();
  return <div className="flex flex-wrap items-center gap-2 border-b border-border pb-2">
    {actions.map(({key,Icon,before,after})=><Button key={key} variant="ghost" size="icon-sm" disabled={unavailable}
      title={t(`g.markdown.${key}`)} aria-label={t(`g.markdown.${key}`)} onClick={()=>onInsert(before,after)}><Icon/></Button>)}
  </div>;
}
