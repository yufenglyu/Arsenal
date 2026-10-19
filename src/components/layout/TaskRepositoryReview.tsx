import { useState } from "react";
import { useTranslation } from "react-i18next";
import { Check, RefreshCw } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { RepositoryAddressButton } from "@/components/skill/RepositoryAddressButton";
import { useRepositorySyncStore } from "@/stores/repositorySyncStore";
import {
  isTaskActive,
  taskErrorMessage,
  useTaskQueueStore,
} from "@/stores/taskQueueStore";
import {
  repositoryHasDeletions,
  type RepositoryTaskRow,
  type TaskBatch,
} from "@/lib/taskBatches";
import type { RepositorySyncPreview } from "@/types";
import { TaskDisclosure } from "./TaskDisclosure";

function Changes({ repo }: { repo: RepositorySyncPreview }) {
  const { t } = useTranslation();
  return (
    <div className="space-y-3">
      {(["added", "modified", "deleted", "updated"] as const).flatMap(
        (category) =>
          (repo[category] ?? []).map((item) => (
            <TaskDisclosure
              key={`${category}:${item.skillId}:${item.version}`}
              title={
                <>
                  <span
                    className={
                      category === "deleted"
                        ? "text-destructive"
                        : "text-muted-foreground"
                    }
                  >
                    {t(
                      category === "updated"
                        ? "workflow.updated"
                        : `workflow.change.${category}`,
                    )}
                  </span>
                  <span className="min-w-0 break-all">{item.name}</span>
                </>
              }
            >
              {item.files?.length ? (
                item.files.map((file) => (
                  <p key={file.path} className="flex gap-2 py-1 text-xs">
                    <span
                      className={
                        file.status === "deleted"
                          ? "text-destructive"
                          : "text-muted-foreground"
                      }
                    >
                      {t(`workflow.change.${file.status}`)}
                    </span>
                    <span className="min-w-0 break-all font-mono">
                      {file.path}
                    </span>
                  </p>
                ))
              ) : (
                <p className="break-all text-xs text-muted-foreground">
                  {item.sourcePath ?? item.name}
                </p>
              )}
            </TaskDisclosure>
          )),
      )}
      {!!repo.documentChanges?.length && (
        <TaskDisclosure title={t("workflow.mirror.documents")}>
          {repo.documentChanges.map((file) => (
            <p key={file.path} className="flex gap-2 py-1 text-xs">
              <span
                className={
                  file.status === "deleted"
                    ? "text-destructive"
                    : "text-muted-foreground"
                }
              >
                {t(`workflow.change.${file.status}`)}
              </span>
              <span className="min-w-0 break-all font-mono">{file.path}</span>
            </p>
          ))}
        </TaskDisclosure>
      )}
      {!!repo.unchanged.length && (
        <TaskDisclosure
          small
          title={t("workflow.batch.unchangedCount", {
            count: repo.unchanged.length,
          })}
        >
          {repo.unchanged.map((item) => (
            <p
              key={`${item.skillId}:${item.version}`}
              className="break-all py-1 text-sm"
            >
              {item.name}
            </p>
          ))}
        </TaskDisclosure>
      )}
      <RepositoryAddressButton repository={repo.repository} />
    </div>
  );
}

export function TaskRepositoryReview({
  batch,
  onLater,
}: {
  batch: TaskBatch;
  onLater: () => void;
}) {
  const { t, i18n } = useTranslation();
  const state = useRepositorySyncStore();
  const tasks = useTaskQueueStore((s) => s.tasks);
  const [excluded, setExcluded] = useState<Set<string>>(new Set());
  const [acknowledged, setAcknowledged] = useState("");
  const [search, setSearch] = useState("");
  const [submitError, setSubmitError] = useState(false);
  const rows = batch.report ?? [];
  const busy =
    state.isChecking ||
    !!state.checkingRepository ||
    !!state.migratingRepository;
  const locked = (row: RepositoryTaskRow) =>
    tasks.some(
      (task) =>
        isTaskActive(task) &&
        task.locks.some(
          (lock) =>
            lock === "repo:*" ||
            lock === `repo:${row.repository.toLowerCase()}`,
        ),
    );
  const actionable = (row: RepositoryTaskRow) =>
    row.pending &&
    !row.needsCheck &&
    !!row.preview?.snapshotId &&
    !row.error &&
    !locked(row) &&
    !busy;
  // A failed sync can be reviewed and re-submitted; failed checks must be repeated.
  const canSelect = (row: RepositoryTaskRow) =>
    actionable({ ...row, error: row.needsCheck ? row.error : undefined });
  const candidates = rows.filter(canSelect);
  const choices = candidates.filter(
    (row) => !excluded.has(row.preview!.snapshotId!),
  );
  const deletionChoices = choices.filter((row) =>
    repositoryHasDeletions(row.preview!),
  );
  const signature = JSON.stringify(
    choices.map((row) => [row.repository, row.preview!.snapshotId]).sort(),
  );
  const hasDeletion = deletionChoices.length > 0;
  const visible = rows.filter((row) =>
    row.repository.toLowerCase().includes(search.trim().toLowerCase()),
  );
  const attention = visible.filter(
    (row) =>
      row.pending || row.needsCheck || row.error || row.status !== "success",
  );
  const settled = visible.filter((row) => !attention.includes(row));
  const submit = () => {
    const applied = state.applySnapshots(
      choices.map((row) => ({
        repository: row.repository,
        snapshotId: row.preview!.snapshotId!,
        allowDeletions: !hasDeletion || acknowledged === signature,
      })),
      state.reportGeneration,
    );
    setSubmitError(applied === 0);
    setAcknowledged("");
  };
  const renderRow = (row: RepositoryTaskRow) => {
    const repo = row.preview;
    const running = locked(row);
    const status = row.error
      ? taskErrorMessage(row.error, "update")
      : running
        ? t(
            `workflow.taskStatus.${row.status === "queued" ? "queued" : "running"}`,
          )
        : row.needsCheck
          ? t("workflow.batch.needsCheck")
          : row.pending
            ? [
                repo &&
                repo.added.length + repo.modified.length + repo.deleted.length >
                  0
                  ? t("workflow.batch.changes", {
                      added: repo.added.length,
                      modified: repo.modified.length,
                      deleted: repo.deleted.length,
                    })
                  : null,
                repo?.documentChanges?.length
                  ? t("workflow.batch.documentsChanged", {
                      count: repo.documentChanges.length,
                    })
                  : null,
              ]
                .filter(Boolean)
                .join(" · ")
            : t(
                row.applied
                  ? "workflow.updated"
                  : "workflow.changeType.unchanged",
              );
    return (
      <section key={row.repository} className="border-b border-border py-3">
        <div className="flex items-start gap-2">
          {row.pending && !row.needsCheck && (
            <input
              type="checkbox"
              className="mt-1 shrink-0"
              aria-label={row.repository}
              checked={!excluded.has(repo!.snapshotId!)}
              disabled={!canSelect(row)}
              onChange={(event) => {
                const checked = event.target.checked;
                setExcluded((previous) => {
                  const next = new Set(previous);
                  if (checked) next.delete(repo!.snapshotId!);
                  else next.add(repo!.snapshotId!);
                  return next;
                });
              }}
            />
          )}
          <div className="min-w-0 flex-1">
            {repo ? (
              <TaskDisclosure
                title={
                  <>
                    <span className="min-w-0 flex-1 break-all">
                      {row.repository}
                    </span>
                    <span
                      className={`shrink-0 text-xs ${row.error ? "text-destructive" : "text-muted-foreground"}`}
                    >
                      {status}
                    </span>
                  </>
                }
              >
                <Changes repo={repo} />
              </TaskDisclosure>
            ) : (
              <div className="flex flex-wrap justify-between gap-2 text-sm">
                <span className="break-all">{row.repository}</span>
                <span
                  className={
                    row.error ? "text-destructive" : "text-muted-foreground"
                  }
                >
                  {status}
                </span>
              </div>
            )}
          </div>
          {row.needsCheck && (
            <Button
              variant="outline"
              disabled={busy || running}
              onClick={() => void state.recheckRepository(row.repository)}
            >
              <RefreshCw />
              {t("workflow.recheck")}
            </Button>
          )}
        </div>
      </section>
    );
  };
  return (
    <div>
      {rows.length > 8 && (
        <Input
          type="search"
          className="my-3"
          aria-label={t("workflow.batch.searchRepos")}
          placeholder={t("workflow.batch.searchRepos")}
          value={search}
          onChange={(event) => setSearch(event.target.value)}
        />
      )}
      {batch.reportError && (
        <p role="alert" className="py-3 text-sm text-destructive">
          {taskErrorMessage(batch.reportError)}
        </p>
      )}
      {attention.map(renderRow)}
      {!!settled.length && (
        <TaskDisclosure
          title={
            <>
              <Check />
              {t("workflow.batch.completedCount", { count: settled.length })}
            </>
          }
        >
          {settled.map(renderRow)}
        </TaskDisclosure>
      )}
      {!!candidates.length && (
        <div className="space-y-3 py-3">
          <label className="flex items-center gap-2 text-sm">
            <input
              type="checkbox"
              checked={choices.length === candidates.length}
              ref={(input) => {
                if (input)
                  input.indeterminate =
                    choices.length > 0 && choices.length < candidates.length;
              }}
              onChange={(event) => {
                const checked = event.target.checked;
                setExcluded((previous) => {
                  const next = new Set(previous);
                  for (const row of candidates) {
                    if (checked) next.delete(row.preview!.snapshotId!);
                    else next.add(row.preview!.snapshotId!);
                  }
                  return next;
                });
              }}
            />
            {t("workflow.batch.selectUpdates", { count: choices.length })}
          </label>
          {hasDeletion && (
            <label className="flex items-start gap-2 text-sm">
              <input
                className="mt-1"
                type="checkbox"
                checked={acknowledged === signature}
                onChange={(event) =>
                  setAcknowledged(event.target.checked ? signature : "")
                }
              />
              <span
                title={deletionChoices.map((row) => row.repository).join(", ")}
              >
                {t("workflow.batch.confirmDeletions", {
                  count: deletionChoices.length,
                })}
              </span>
            </label>
          )}
          {submitError && (
            <p role="alert" className="text-xs text-destructive">
              {t("workflow.batch.previewUnavailable")}
            </p>
          )}
          <div className="flex flex-wrap justify-end gap-2">
            <Button
              variant="outline"
              title={t("workflow.batch.draftRetained")}
              onClick={onLater}
            >
              {t("workflow.batch.later")}
            </Button>
            <Button
              disabled={
                !choices.length || (hasDeletion && acknowledged !== signature)
              }
              onClick={submit}
            >
              <Check />
              {t("workflow.batch.updateConfirm", { count: choices.length })}
            </Button>
          </div>
        </div>
      )}
      <TaskDisclosure small title={t("workflow.batch.log")}>
        <p>
          {state.reportCheckedAt
            ? new Date(state.reportCheckedAt).toLocaleString(i18n.language)
            : t("workflow.checkTimeUnknown")}
        </p>
        {!!state.preview?.coverage?.length && (
          <div className="mt-3">
            {state.preview.coverage.map((item) => (
              <p key={item.skillId} className="break-all py-1">
                {item.name} ·{" "}
                {t(
                  `workflow.mirror.coverageStatus.${item.reason ?? item.status}`,
                )}
              </p>
            ))}
          </div>
        )}
      </TaskDisclosure>
    </div>
  );
}
