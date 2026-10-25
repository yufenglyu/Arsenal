import { useRepositoryDocumentStore } from "@/stores/repositoryDocumentStore";
import { ActionIcon } from "@/components/ui/action-icon";
import { Pencil, Trash2 } from "lucide-react";
import { Input } from "@/components/ui/input";
import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { useMetadataStore } from "@/stores/metadataStore";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogBody,
  DialogHeader,
  DialogTitle,
  DialogFooter,
} from "@/components/ui/dialog";
import { cn } from "@/lib/utils";

export function TagFilters({
  tags,
  selected,
  onChange,
}: {
  tags: { key: string; label: string }[];
  selected: string[];
  onChange: (tags: string[]) => void;
}) {
  const { t } = useTranslation();
  const highlighted = useRepositoryDocumentStore(state => state.highlightedTags);
  const [menu, setMenu] = useState<{
    tag: string;
    x: number;
    y: number;
  } | null>(null);
  const [confirmation, setConfirmation] = useState<{
    tag: string;
    count: number;
  } | null>(null);
  const [dropTarget, setDropTarget] = useState<{
    key: string;
    remove: boolean;
  } | null>(null);
  useEffect(() => {
    const clear = () => setDropTarget(null);
    window.addEventListener("dragend", clear);
    window.addEventListener("drop", clear);
    window.addEventListener("blur", clear);
    return () => {
      window.removeEventListener("dragend", clear);
      window.removeEventListener("drop", clear);
      window.removeEventListener("blur", clear);
    };
  }, []);
  const [editing, setEditing] = useState<string | null>(null);
  const [newName, setNewName] = useState("");
  const [mergeConfirmed, setMergeConfirmed] = useState(false);
  const [globalCollision, setGlobalCollision] = useState(false);
  const collision = globalCollision || !!editing && editing.toLowerCase() !== newName.trim().toLowerCase() && tags.some(tag => tag.key === newName.trim().toLowerCase());
  useEffect(() => {
    const renamed = (event: Event) => {
      const {oldTag,newTag} = (event as CustomEvent<{oldTag:string;newTag:string}>).detail;
      if (selected.includes(oldTag)) onChange([...new Set(selected.map(tag => tag === oldTag ? newTag : tag))]);
    };
    window.addEventListener("arsenal-tag-renamed", renamed);
    return () => window.removeEventListener("arsenal-tag-renamed", renamed);
  }, [selected,onChange]);
  const [busy, setBusy] = useState(false);
  useEffect(() => {
    if (!menu) return;
    const close = () => setMenu(null);
    const key = (event: KeyboardEvent) => {
      if (event.key === "Escape") close();
    };
    window.addEventListener("pointerdown", close);
    window.addEventListener("keydown", key);
    return () => {
      window.removeEventListener("pointerdown", close);
      window.removeEventListener("keydown", key);
    };
  }, [menu]);
  async function drop(event: React.DragEvent, tag: string) {
    event.preventDefault();
    setDropTarget(null);
    try {
      const ids: unknown = JSON.parse(
        event.dataTransfer.getData("application/arsenal-skills"),
      );
      if (
        !Array.isArray(ids) ||
        !ids.length ||
        !ids.every((id) => typeof id === "string")
      )
        return;
      await useMetadataStore
        .getState()
        .changeTag(tag, ids, Boolean(event.shiftKey));
    } catch {
      toast.error(t("workflow.operationFailed"));
    }
  }
  return (
    <>
      <div
        role="group"
        aria-label={t("central.tagFilter")}
        className="flex flex-wrap items-center gap-1"
      >
        {tags.map((tag) => (
          <button
            key={tag.key}
            type="button"
            aria-pressed={selected.includes(tag.key)}
            data-highlighted={highlighted.includes(tag.key) || undefined}
            data-drop-target={
              dropTarget?.key === tag.key
                ? dropTarget.remove
                  ? "remove"
                  : "add"
                : undefined
            }
            onClick={(event) =>
              onChange(
                event.ctrlKey || event.metaKey
                  ? selected.includes(tag.key)
                    ? selected.filter((key) => key !== tag.key)
                    : [...selected, tag.key]
                  : selected.includes(tag.key)
                    ? selected.filter((key) => key !== tag.key)
                    : [tag.key],
              )
            }
            onContextMenu={(event) => {
              event.preventDefault();
              setMenu({ tag: tag.label, x: event.clientX, y: event.clientY });
            }}
            onDragOver={(event) => {
              if (
                event.dataTransfer.types.includes(
                  "application/arsenal-skills",
                )
              ) {
                event.preventDefault();
                setDropTarget({
                  key: tag.key,
                  remove: Boolean(event.shiftKey),
                });
                event.dataTransfer.dropEffect = event.shiftKey
                  ? "move"
                  : "copy";
              }
            }}
            onDragLeave={() =>
              setDropTarget((current) =>
                current?.key === tag.key ? null : current,
              )
            }
            onDrop={(event) => void drop(event, tag.label)}
            className={cn(
              "rounded bg-transparent px-1.5 py-0.5 text-xs text-foreground transition-[transform,color,box-shadow] duration-150 motion-reduce:transition-none focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring",
              dropTarget?.key === tag.key
                ? dropTarget.remove
                  ? "relative z-10 scale-110 text-destructive ring-2 ring-destructive shadow-sm"
                  : "relative z-10 scale-110 text-foreground ring-2 ring-primary shadow-sm"
                : selected.includes(tag.key) || highlighted.includes(tag.key)
                  ? "font-semibold text-primary"
                  : "hover:text-primary",
            )}
          >
            {tag.label}
          </button>
        ))}
      </div>
      {menu && (
        <div
          role="menu"
          className="fixed z-50 rounded border bg-popover p-1 shadow-lg"
          style={{
            left: Math.min(menu.x, window.innerWidth - 180),
            top: Math.min(menu.y, window.innerHeight - 100),
          }}
          onPointerDown={(e) => e.stopPropagation()}
        >
          <button role="menuitem" className="flex w-full items-center gap-2 rounded px-3 py-1.5 text-sm hover:bg-muted" onClick={() => {setEditing(menu.tag);setNewName(menu.tag);setGlobalCollision(false);setMergeConfirmed(false);setMenu(null);}}><Pencil className="size-4"/>{t("workflow.renameTag")}</button>
          <button
            autoFocus
            role="menuitem"
            className="flex w-full items-center gap-2 rounded px-3 py-1.5 text-sm text-destructive hover:bg-muted"
            onClick={async () => {
              try {
                const count = await useMetadataStore
                  .getState()
                  .countTag(menu.tag);
                setConfirmation({ tag: menu.tag, count });
                setMenu(null);
              } catch {
                toast.error(t("workflow.operationFailed"));
              }
            }}
          >
            <Trash2 className="size-4"/>{t("common.delete")}
          </button>
        </div>
      )}
      <Dialog open={editing !== null} onOpenChange={open => {if(!open && !busy)setEditing(null);}}>
        <DialogContent><DialogHeader><DialogTitle>{t("workflow.renameTag")}</DialogTitle></DialogHeader>
          <DialogBody className="space-y-3"><Input aria-label={t("workflow.tagName")} value={newName} maxLength={100} onChange={event=>{setNewName(event.target.value);setGlobalCollision(false);setMergeConfirmed(false);}}/>
          {collision && <label className="flex items-center gap-2 text-sm"><input type="checkbox" checked={mergeConfirmed} onChange={event=>setMergeConfirmed(event.target.checked)}/>{t("workflow.mergeTag")}</label>}</DialogBody>
          <DialogFooter><Button variant="outline" disabled={busy} onClick={()=>setEditing(null)}><ActionIcon action="cancel"/>{t("common.cancel")}</Button><Button disabled={busy || !newName.trim() || (collision && !mergeConfirmed)} onClick={async()=>{if(!editing)return;setBusy(true);try{if (!mergeConfirmed && editing.toLowerCase() !== newName.trim().toLowerCase() && await useMetadataStore.getState().countTag(newName.trim()) > 0) {setGlobalCollision(true);return;} await useMetadataStore.getState().renameTag(editing,newName,mergeConfirmed);setEditing(null);}catch{toast.error(t("workflow.operationFailed"));}finally{setBusy(false);}}}><ActionIcon action="save"/>{t("common.save")}</Button></DialogFooter>
        </DialogContent>
      </Dialog>
      <Dialog
        open={!!confirmation}
        onOpenChange={(open) => {
          if (!open && !busy) setConfirmation(null);
        }}
      >
        <DialogContent>
          <DialogHeader>
            <DialogTitle>
              {t("workflow.deleteTag", {
                tag: confirmation?.tag,
                count: confirmation?.count,
              })}
            </DialogTitle>
          </DialogHeader>
          <DialogFooter>
            <Button
              variant="outline"
              disabled={busy}
              onClick={() => setConfirmation(null)}
            ><ActionIcon action="cancel"/>
              {t("common.cancel")}
            </Button>
            <Button
              variant="destructive"
              disabled={busy}
              onClick={async () => {
                if (!confirmation) return;
                setBusy(true);
                try {
                  await useMetadataStore
                    .getState()
                    .changeTag(confirmation.tag, null, true);
                  onChange(
                    selected.filter(
                      (key) => key !== confirmation.tag.toLowerCase(),
                    ),
                  );
                  setConfirmation(null);
                } catch {
                  toast.error(t("workflow.operationFailed"));
                } finally {
                  setBusy(false);
                }
              }}
            ><ActionIcon action="delete"/>
              {t("common.delete")}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </>
  );
}
