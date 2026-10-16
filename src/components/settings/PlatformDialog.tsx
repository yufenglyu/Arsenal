import { ActionIcon } from "@/components/ui/action-icon";
import { usePlatformIconStore } from "@/stores/platformIconStore";
import { PlatformIcon } from "@/components/platform/PlatformIcon";
import { useState, useEffect, useMemo } from "react";
import { Loader2 } from "lucide-react";
import { useTranslation } from "react-i18next";

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
import { AgentWithStatus } from "@/types";
import { deriveHomeDir, formatPathForDisplay, joinPathForDisplay } from "@/lib/path";
import { usePlatformStore } from "@/stores/platformStore";
// ─── Props ────────────────────────────────────────────────────────────────────

interface PlatformDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  /** Pass a platform to edit it; null for create mode. */
  platform: AgentWithStatus | null;
  onAdd?: (id: string | undefined, displayName: string, globalSkillsDir: string) => Promise<string | void>;
  onEdit?: (id: string, displayName: string, globalSkillsDir: string) => Promise<string | void>;
}

// ─── PlatformDialog ───────────────────────────────────────────────────────────

export function PlatformDialog({
  open,
  onOpenChange,
  platform,
  onAdd,
  onEdit,
}: PlatformDialogProps) {
  const { t } = useTranslation();
  const agents = usePlatformStore((state) => state.agents);
  const isEditMode = platform !== null;
  const homeDir = useMemo(() => {
    const candidates = [
      platform?.global_skills_dir,
      agents.find((agent) => agent.id === "central")?.global_skills_dir,
      ...agents.map((agent) => agent.global_skills_dir),
    ].filter((candidate): candidate is string => Boolean(candidate));

    return candidates
      .map((candidate) => deriveHomeDir(candidate))
      .find((candidate): candidate is string => Boolean(candidate));
  }, [agents, platform]);

  const [savedId, setSavedId] = useState<string | null>(null);
  const [iconDraft, setIconDraft] = useState<string | null | undefined>(undefined);
  const [displayName, setDisplayName] = useState("");
  const [platformId, setPlatformId] = useState("");
  const [globalSkillsDir, setGlobalSkillsDir] = useState("");
  const [idManuallyEdited, setIdManuallyEdited] = useState(false);
  const [dirManuallyEdited, setDirManuallyEdited] = useState(false);
  const [isSubmitting, setIsSubmitting] = useState(false);
  const [nameError, setNameError] = useState<string | null>(null);
  const [idError, setIdError] = useState<string | null>(null);
  const [dirError, setDirError] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  // Reset form when dialog opens.
  useEffect(() => {
    if (open) {
      setSavedId(null);
      setIconDraft(undefined);
      setDisplayName(platform?.display_name ?? "");
      setPlatformId(platform?.id ?? "");
      setGlobalSkillsDir(platform ? formatPathForDisplay(platform.global_skills_dir) : "");
      setIdManuallyEdited(isEditMode);
      setDirManuallyEdited(isEditMode);
      setNameError(null);
      setIdError(null);
      setDirError(null);
      setError(null);
    }
  }, [open, platform, isEditMode]);

  function slugifyPlatformId(value: string) {
    return value
      .trim()
      .toLowerCase()
      .replace(/[^a-z0-9]+/g, "-")
      .replace(/^-+|-+$/g, "");
  }

  async function handleSubmit() {
    const trimmedName = displayName.trim();
    const trimmedId = platformId.trim();
    const trimmedDir = globalSkillsDir.trim();

    let hasError = false;
    if (!trimmedName) {
      setNameError(t("platformDialog.nameRequired"));
      hasError = true;
    } else {
      setNameError(null);
    }
    if (trimmedId && !/^[a-z0-9][a-z0-9-]*[a-z0-9]$|^[a-z0-9]$/.test(trimmedId)) {
      setIdError(t("platformDialog.idInvalid"));
      hasError = true;
    } else if (isEditMode && !trimmedId) {
      setIdError(t("platformDialog.idRequired"));
      hasError = true;
    } else {
      setIdError(null);
    }
    if (!trimmedDir) {
      setDirError(t("platformDialog.dirRequired"));
      hasError = true;
    } else {
      setDirError(null);
    }

    if (hasError) return;

    setIsSubmitting(true);
    setError(null);

    try {
      let effectiveId = savedId;
      if (!effectiveId) {
        const returnedId = isEditMode && onEdit ? await onEdit(trimmedId, trimmedName, trimmedDir) : await onAdd?.(trimmedId || undefined, trimmedName, trimmedDir);
        effectiveId = returnedId || trimmedId || null;
        setSavedId(effectiveId);
      }
      if (iconDraft !== undefined) {
        if (!effectiveId) throw new Error(t("platformDialog.iconSaveFailed"));
        await usePlatformIconStore.getState().save(effectiveId, iconDraft);
      } else if (platform && effectiveId !== platform.id) {
        await usePlatformIconStore.getState().load();
      }
      onOpenChange(false);
    } catch (err) {
      setError(String(err));
    } finally {
      setIsSubmitting(false);
    }
  }

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>
            {isEditMode
                ? t("platformDialog.editTitle")
                : t("platformDialog.addTitle")}
          </DialogTitle>
          <DialogClose />
        </DialogHeader>

        <DialogBody className="space-y-4">
          <DialogDescription>
            {isEditMode
              ? t("platformDialog.editDesc")
              : t("platformDialog.addDesc")}
          </DialogDescription>

          <div className="space-y-2">
            <label htmlFor="platform-icon" className="text-sm font-medium">{t("platformDialog.iconLabel")}</label>
            <div className="flex items-center gap-3">
              <div className="flex size-10 items-center justify-center rounded-lg border bg-muted/30">
                {iconDraft ? <img src={iconDraft} alt="" className="size-7 object-contain"/> : iconDraft === null ? <span>—</span> : <PlatformIcon agentId={platform?.id ?? platformId} brand size={28}/>}
              </div>
              <Input id="platform-icon" type="file" accept="image/png,image/svg+xml,image/webp" disabled={isSubmitting} onChange={event => {
                const file = event.target.files?.[0];
                if (!file) return;
                if (file.size > 2 * 1024 * 1024) { setError(t("platformDialog.iconTooLarge")); return; }
                const reader = new FileReader();
                reader.onload = () => { setIconDraft(String(reader.result)); setError(null); };
                reader.onerror = () => setError(t("platformDialog.iconSaveFailed"));
                reader.readAsDataURL(file);
              }}/>
              <Button variant="outline" disabled={isSubmitting} onClick={() => setIconDraft(null)}><ActionIcon action="reset"/>{t("platformDialog.resetIcon")}</Button>
            </div>
            <p className="text-xs text-muted-foreground">{t("platformDialog.iconHint")}</p>
          </div>
          {/* Display name field */}
          <div className="space-y-1.5">
            <label htmlFor="platform-name" className="text-sm font-medium">
              {t("platformDialog.nameLabel")} <span className="text-destructive">*</span>
            </label>
            <Input
              id="platform-name"
              placeholder={t("platformDialog.namePlaceholder")}
              value={displayName}
              onChange={(e) => {
                const name = e.target.value;
                setDisplayName(name);
                if (nameError) setNameError(null);
                if (!idManuallyEdited && !isEditMode) {
                  setPlatformId(slugifyPlatformId(name));
                  if (idError) setIdError(null);
                }
                // Auto-generate path from name if user hasn't manually edited it
                if (!dirManuallyEdited && !isEditMode) {
                  const slug = slugifyPlatformId(name);
                  setGlobalSkillsDir(
                    slug
                      ? homeDir
                        ? joinPathForDisplay(homeDir, `.${slug}/skills/`)
                        : `~/.${slug}/skills/`
                      : ""
                  );
                }
              }}
              disabled={isSubmitting || savedId !== null}
              autoFocus
            />
            {nameError && (
              <p className="text-xs text-destructive" role="alert">
                {nameError}
              </p>
            )}
          </div>

          <div className="space-y-1.5">
            <label htmlFor="platform-id" className="text-sm font-medium">
              {t("platformDialog.idLabel")}
              {isEditMode ? <span className="text-destructive"> *</span> : null}
            </label>
            <Input
              id="platform-id"
              placeholder={t("platformDialog.idPlaceholder")}
              value={platformId}
              onChange={(event) => {
                setPlatformId(slugifyPlatformId(event.target.value));
                setIdManuallyEdited(true);
                if (idError) setIdError(null);
              }}
              disabled={isSubmitting || savedId !== null}
            />
            {idError ? (
              <p className="text-xs text-destructive" role="alert">
                {idError}
              </p>
            ) : (
              <p className="text-xs text-muted-foreground">
                {t("platformDialog.idHint")}
              </p>
            )}
          </div>

          {/* Global skills dir field */}
          <div className="space-y-1.5">
            <label htmlFor="platform-dir" className="text-sm font-medium">
              {t("platformDialog.dirLabel")} <span className="text-destructive">*</span>
            </label>
            <Input
              id="platform-dir"
              placeholder={t("platformDialog.dirPlaceholder")}
              value={globalSkillsDir}
              onChange={(e) => {
                setGlobalSkillsDir(e.target.value);
                setDirManuallyEdited(true);
                if (dirError) setDirError(null);
              }}
              disabled={isSubmitting || savedId !== null}
            />
            {dirError && (
              <p className="text-xs text-destructive" role="alert">
                {dirError}
              </p>
            )}
            {!dirError && !isEditMode && (
              <p className="text-xs text-muted-foreground">
                {dirManuallyEdited
                  ? (t("platformDialog.dirManualHint") || "Path manually set. Edit Platform Name won't change it.")
                  : (t("platformDialog.dirAutoHint") || "Auto-generated from Platform Name. You can edit it freely.")}
              </p>
            )}
          </div>

          {/* Backend error */}
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
            {t("platformDialog.cancel")}
          </Button>
          <Button onClick={handleSubmit} disabled={isSubmitting}>
            {isSubmitting ? (
              <>
                <Loader2 className="size-4 animate-spin" />
                {isEditMode ? t("platformDialog.saving") : t("platformDialog.adding")}
              </>
            ) : isEditMode ? (
              <><ActionIcon action="save"/>{t("platformDialog.save")}</>
            ) : (
              <><ActionIcon action="add"/>{t("platformDialog.add")}</>
            )}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
