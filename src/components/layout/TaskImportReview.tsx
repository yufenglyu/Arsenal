import { ActionIcon } from "@/components/ui/action-icon";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { Check } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import {
  preparedImportPreview,
  submitPreparedImport,
} from "@/stores/importPreparationStore";
import type { DuplicateResolution, GitHubRepoPreview } from "@/types";

export function TaskImportReview({
  taskId,
  onLater,
}: {
  taskId: string;
  onLater: () => void;
}) {
  const { t } = useTranslation();
  const preview = preparedImportPreview(taskId);
  return preview ? (
    <ImportChoices
      key={`${taskId}:${preview.repo.branch}`}
      taskId={taskId}
      preview={preview}
      onLater={onLater}
    />
  ) : (
    <p role="alert" className="py-3 text-sm text-destructive">
      {t("workflow.batch.previewUnavailable")}
    </p>
  );
}
function ImportChoices({
  taskId,
  preview,
  onLater,
}: {
  taskId: string;
  preview: GitHubRepoPreview;
  onLater: () => void;
}) {
  const { t } = useTranslation();
  const [choices, setChoices] = useState(() =>
    Object.fromEntries(
      preview.skills.map((skill) => [
        skill.sourcePath,
        {
          selected: skill.recommended !== false,
          resolution: (skill.conflict
            ? "skip"
            : "overwrite") as DuplicateResolution,
          rename: skill.skillId,
        },
      ]),
    ),
  );
  const [error, setError] = useState(false);
  const selectableCount = preview.skills.filter(skill => skill.recommended !== false).length;
  const selected = preview.skills.filter(
    (skill) => choices[skill.sourcePath]?.selected,
  );
  const chosen = selected.filter(
    (skill) => choices[skill.sourcePath].resolution !== "skip",
  );
  const invalid = chosen.some((skill) => {
    const item = choices[skill.sourcePath];
    return (
      item.resolution === "rename" &&
      (!item.rename.trim() ||
        /[\\/]/.test(item.rename) ||
        [".", ".."].includes(item.rename.trim()))
    );
  });
  const patch = (path: string, change: Partial<(typeof choices)[string]>) =>
    setChoices(previous => {
      const next = {...previous};
      const skill = preview.skills.find(item => item.sourcePath === path);
      if (change.selected && skill && (skill.variantCount ?? 0) > 1) {
        for (const variant of preview.skills) if (variant.skillName.trim().toLowerCase() === skill.skillName.trim().toLowerCase()) next[variant.sourcePath] = {...next[variant.sourcePath], selected:false};
      }
      next[path] = {...next[path], ...change};
      return next;
    });
  const submit = () => {
    const result = submitPreparedImport(
      taskId,
      preview,
      preview.skills.map((skill) => ({
        sourcePath: skill.sourcePath,
        resolution: choices[skill.sourcePath].selected
          ? choices[skill.sourcePath].resolution
          : "skip",
        renamedSkillId: choices[skill.sourcePath].rename.trim(),
      })),
    );
    setError(!result);
  };
  return (
    <div className="space-y-3 py-3">
      <div className="flex flex-wrap items-center justify-between gap-2">
        <label className="inline-flex items-center gap-2 text-sm">
          <input
            type="checkbox"
            checked={
              !!preview.skills.length &&
              selected.length === selectableCount
            }
            ref={(input) => {
              if (input)
                input.indeterminate =
                  selected.length > 0 &&
                  selected.length < selectableCount;
            }}
            onChange={(event) => {
              const checked = event.target.checked;
              setChoices((previous) =>
                Object.fromEntries(
                  Object.entries(previous).map(([key, item]) => [
                    key,
                    { ...item, selected: checked && preview.skills.find(skill => skill.sourcePath === key)?.recommended !== false },
                  ]),
                ),
              );
            }}
          />
          {t("workflow.selectAll")}
        </label>
        <span className="text-xs text-muted-foreground">
          {t("workflow.batch.importSelection", {
            count: chosen.length,
            skipped: preview.skills.length - chosen.length,
          })}
        </span>
      </div>
      <div className="divide-y divide-border border-y border-border">
        {preview.skills.map((skill) => {
          const item = choices[skill.sourcePath];
          return (
            <div
              key={skill.sourcePath}
              className="flex flex-wrap items-center justify-between gap-2 py-3"
            >
              <label className="flex min-w-0 items-center gap-2 text-sm">
                <input
                  type="checkbox"
                  aria-label={skill.skillName}
                  checked={item.selected}
                  onChange={(event) =>
                    patch(skill.sourcePath, { selected: event.target.checked })
                  }
                />
                <span className="min-w-0">
                  <span className="block break-all">{skill.skillName}</span>
                  <span className="block truncate text-xs text-muted-foreground" title={skill.sourcePath}>{skill.sourcePath}</span>
                  {!!skill.duplicatePaths?.length && <span className="block text-xs text-muted-foreground" title={skill.duplicatePaths.join("\n")}>{t("githubImport.exactCopies",{count:skill.duplicatePaths.length})}</span>}
                  {(skill.variantCount ?? 0)>1 && <span className="block text-xs text-muted-foreground">{t("githubImport.sourceVariants",{count:skill.variantCount})}</span>}
                </span>
                {skill.conflict && (
                  <span className="text-xs text-muted-foreground">
                    {t("workflow.batch.nameConflict")}
                  </span>
                )}
              </label>
              {skill.conflict && (
                <div className="flex flex-wrap items-center gap-2">
                  <select
                    aria-label={t("workflow.batch.conflictFor", {
                      name: skill.skillName,
                    })}
                    disabled={!item.selected}
                    value={item.resolution}
                    onChange={(event) =>
                      patch(skill.sourcePath, {
                        resolution: event.target.value as DuplicateResolution,
                      })
                    }
                    className="h-8 rounded-md border border-border bg-background px-3 text-sm focus:border-ring focus:outline-none disabled:opacity-50"
                  >
                    {(["skip", "overwrite", "rename"] as const).map((value) => (
                      <option key={value} value={value}>
                        {t(`githubImport.duplicateResolution.${value}`)}
                      </option>
                    ))}
                  </select>
                  {item.resolution === "rename" && (
                    <Input
                      aria-label={t("workflow.batch.renameFor", {
                        name: skill.skillName,
                      })}
                      className="w-48"
                      disabled={!item.selected}
                      value={item.rename}
                      onChange={(event) =>
                        patch(skill.sourcePath, { rename: event.target.value })
                      }
                    />
                  )}
                </div>
              )}
            </div>
          );
        })}
      </div>
      {(error || invalid) && (
        <p role="alert" className="text-xs text-destructive">
          {t(
            error
              ? "workflow.batch.previewUnavailable"
              : "workflow.batch.invalidRename",
          )}
        </p>
      )}
      <div className="flex flex-wrap items-center justify-end gap-2">
        {!chosen.length && (
          <span role="status" className="mr-auto text-xs text-muted-foreground">
            {t("workflow.batch.chooseOne")}
          </span>
        )}
        <Button
          variant="outline"
          title={t("workflow.batch.draftRetained")}
          onClick={onLater}
        >
          <ActionIcon action="back" />{t("workflow.batch.later")}
        </Button>
        <Button disabled={!chosen.length || invalid} onClick={submit}>
          <Check />
          {t("workflow.batch.importConfirm", { count: chosen.length })}
        </Button>
      </div>
    </div>
  );
}
