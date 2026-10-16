import { ActionIcon } from "@/components/ui/action-icon";
import { useState, useEffect } from "react";
import { FolderOpen, Loader2 } from "lucide-react";
import { useTranslation } from "react-i18next";
import { open as openDialog } from "@tauri-apps/plugin-dialog";

import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogDescription,
  DialogBody,
  DialogFooter,
  DialogClose,
} from "@/components/ui/dialog";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { ScanDirectory } from "@/types";
import { projectDirectoryName } from "@/lib/projectTargets";
import { formatPathForDisplay, normalizePathForInputDisplay } from "@/lib/path";

interface AddDirectoryDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  directory?: ScanDirectory | null;
  onAdd: (path: string, label: string) => Promise<void>;
  onEdit?: (path: string, nextPath: string, label: string) => Promise<void>;
}

function folderNameFromPath(path: string) {
  const parts = path.split(/[\\/]/).filter(Boolean);
  return parts[parts.length - 1] ?? "";
}

export function AddDirectoryDialog({
  open,
  onOpenChange,
  directory = null,
  onAdd,
  onEdit,
}: AddDirectoryDialogProps) {
  const { t } = useTranslation();
  const isEditMode = directory !== null;
  const [name, setName] = useState("");
  const [path, setPath] = useState("");
  const [isSubmitting, setIsSubmitting] = useState(false);
  const [nameError, setNameError] = useState<string | null>(null);
  const [validationError, setValidationError] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (open) {
      setName(directory ? projectDirectoryName(directory) : "");
      setPath(directory ? formatPathForDisplay(directory.path) : "");
      setNameError(null);
      setValidationError(null);
      setError(null);
    }
  }, [open, directory]);

  async function handleSubmit() {
    const trimmedName = name.trim();
    const trimmedPath = path.trim();
    let hasError = false;
    if (!trimmedName) {
      setNameError(t("addDir.nameRequired"));
      hasError = true;
    } else {
      setNameError(null);
    }
    if (!trimmedPath) {
      setValidationError(t("addDir.pathRequired"));
      hasError = true;
    } else {
      setValidationError(null);
    }
    if (hasError) return;

    setIsSubmitting(true);
    setError(null);

    try {
      if (isEditMode && directory && onEdit) {
        await onEdit(directory.path, trimmedPath, trimmedName);
      } else {
        await onAdd(trimmedPath, trimmedName);
      }
      onOpenChange(false);
    } catch (err) {
      setError(String(err));
    } finally {
      setIsSubmitting(false);
    }
  }

  function handleKeyDown(e: React.KeyboardEvent) {
    if (e.key === "Enter" && !isSubmitting) {
      void handleSubmit();
    }
  }

  async function handleBrowse() {
    const selected = await openDialog({
      directory: true,
      multiple: false,
      title: t("addDir.browseTitle"),
    });
    if (typeof selected !== "string") return;
    const nextPath = normalizePathForInputDisplay(selected);
    setPath(nextPath);
    setValidationError(null);
    if (!name.trim()) {
      setName(folderNameFromPath(nextPath));
      setNameError(null);
    }
  }

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>{isEditMode ? t("addDir.editTitle") : t("addDir.title")}</DialogTitle>
          <DialogClose />
        </DialogHeader>

        <DialogBody className="space-y-4">
          <DialogDescription>
            {isEditMode ? t("addDir.editDesc") : t("addDir.desc")}
          </DialogDescription>

          <div className="space-y-1.5">
            <label htmlFor="dir-name" className="text-sm font-medium">
              {t("addDir.nameLabel")} <span className="text-destructive">*</span>
            </label>
            <Input
              id="dir-name"
              placeholder={t("addDir.namePlaceholder")}
              value={name}
              onChange={(e) => {
                setName(e.target.value);
                if (nameError) setNameError(null);
              }}
              onKeyDown={handleKeyDown}
              disabled={isSubmitting}
              autoFocus
            />
            {nameError && (
              <p className="text-xs text-destructive" role="alert">
                {nameError}
              </p>
            )}
          </div>

          <div className="space-y-1.5">
            <label htmlFor="dir-path" className="text-sm font-medium">
              {t("addDir.pathLabel")} <span className="text-destructive">*</span>
            </label>
            <div className="flex gap-2">
              <Input
                id="dir-path"
                placeholder={t("addDir.pathPlaceholder")}
                value={path}
                onChange={(e) => {
                  setPath(e.target.value);
                  if (validationError) setValidationError(null);
                }}
                onKeyDown={handleKeyDown}
                disabled={isSubmitting}
              />
              <Button
                type="button"
                variant="outline"
                onClick={() => void handleBrowse()}
                disabled={isSubmitting}
                className="shrink-0"
              >
                <FolderOpen className="size-4" />
                {t("addDir.browse")}
              </Button>
            </div>
            {validationError && (
              <p className="text-xs text-destructive" role="alert">
                {validationError}
              </p>
            )}
          </div>

          {error && (
            <p className="text-xs text-destructive" role="alert">
              {error}
            </p>
          )}
        </DialogBody>

        <DialogFooter>
          <Button
            variant="outline"
            onClick={() => onOpenChange(false)}
            disabled={isSubmitting}
          ><ActionIcon action="cancel"/>
            {t("addDir.cancel")}
          </Button>
          <Button onClick={() => void handleSubmit()} disabled={isSubmitting}>
            {isSubmitting ? (
              <>
                <Loader2 className="size-4 animate-spin" />
                {isEditMode ? t("addDir.saving") : t("addDir.adding")}
              </>
            ) : (
              isEditMode ? <><ActionIcon action="save"/>{t("addDir.save")}</> : <><ActionIcon action="add"/>{t("addDir.add")}</>
            )}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
