import { ActionIcon } from "@/components/ui/action-icon";
import { previewLocalImport, enqueueGitHubImport, continueGitHubImportInBackground } from "@/stores/importPreparationStore";
import { useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { open } from "@tauri-apps/plugin-dialog";
import { FolderOpen, PackagePlus } from "lucide-react";
import { toast } from "sonner";
import { useGitHubImportStore } from "@/stores/githubImportStore";
import { taskErrorMessage, useTaskQueueStore } from "@/stores/taskQueueStore";
import type { GitHubRepoPreview, GitHubSkillImportSelection } from "@/types";
import { GitHubRepoImportWizard } from "@/components/github-import/GitHubRepoImportWizard";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogFooter,
} from "@/components/ui/dialog";

export function AddSkillsDialog({
  open: visible,
  onOpenChange,
  preparedTaskId,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  preparedTaskId?: string;
}) {
  const { t } = useTranslation();
  const [error, setError] = useState<string | null>(null);
  const preparedTask = useTaskQueueStore.getState().tasks.find(task => task.id === preparedTaskId);
  const [repo, setRepo] = useState(String(preparedTask?.steps[0]?.args.repoUrl ?? ""));
  const [importPreview, setImportPreview] = useState<GitHubRepoPreview | null>((preparedTask?.steps[0]?.result as GitHubRepoPreview | undefined) ?? null);
  const pendingGitHub = useRef<{repoUrl:string; preview:Promise<GitHubRepoPreview>; operationId:string} | null>(null);
  const [path, setPath] = useState("");
  const [overwrite, setOverwrite] = useState(false);
  const [wizard, setWizard] = useState(!!preparedTask);
  const [preparing, setPreparing] = useState(false);
  const request = useRef(0);
  const store = useGitHubImportStore();
  function close() {
    request.current++;
    if (pendingGitHub.current) store.cancelGitHubOperation(pendingGitHub.current.operationId);
    pendingGitHub.current = null;
    setPreparing(false);
    setError(null);
    setWizard(false);
    onOpenChange(false);
  }
  function detachGitHubImport() {
    if (pendingGitHub.current) {
      continueGitHubImportInBackground(pendingGitHub.current.repoUrl, pendingGitHub.current.preview, pendingGitHub.current.operationId);
      pendingGitHub.current = null;
      toast.info(t("workflow.importContinuesInBackground"));
    }
    close();
  }
  function queueGitHub(selections: GitHubSkillImportSelection[], preview = importPreview ?? store.githubImport.preview) {
    if (!preview || !selections.some(selection => selection.resolution !== "skip")) return;
    if (preparedTaskId) {
      const task = useTaskQueueStore.getState().tasks.find(task => task.id === preparedTaskId);
      if (!task || task.status !== "awaiting_input") return;
      useTaskQueueStore.getState().patch(task.id, {status:"success",steps:task.steps.map(step => ({...step,status:"success"}))});
    }
    enqueueGitHubImport(repo.trim(), preview, selections, preparedTask?.batchId);
    close();
  }
  async function add(source: "github" | "local") {
    const token = ++request.current;
    setPreparing(true);
    setError(null);
    try {
      if (source === "github") {
        const repoUrl = repo.trim();
        const operationId = crypto.randomUUID();
        const previewRequest = store.previewGitHubRepoImport(repoUrl, operationId);
        pendingGitHub.current = {repoUrl,preview:previewRequest,operationId};
        const preview = await previewRequest;
        if (token !== request.current) return;
        pendingGitHub.current = null;
        setImportPreview(preview);
        if (!preview.skills.length) {
          toast.info(t("workflow.noImportableSkills"));
          return;
        }
        if (preview.skills.some((skill) => skill.conflict)) {
          setWizard(true);
          return;
        }
        queueGitHub(
          preview.skills.map((skill) => ({
            sourcePath: skill.sourcePath,
            resolution: "overwrite",
          })),
          preview,
        );
      } else {
        const items = await previewLocalImport(path.trim());
        if (token !== request.current) return;
        const selected = items.filter((item) => !item.conflict || overwrite);
        if (!selected.length) {
          toast.info(t("workflow.noImportableSkills"));
          return;
        }
        useTaskQueueStore
          .getState()
          .enqueue({
            key: `local:${path.trim().toLowerCase()}:${selected.map((s) => s.skillId).join(",")}`,
            kind: "import",
            label: t("resource.localAddTitle"),
            locks: [
              "local-import",
              ...selected.map((s) => `skill:${s.skillId}`),
            ],
            steps: selected.map((item) => ({
              command: "add_local_resource_skills",
              label: item.name,
              args: {
                input: {
                  sourceDir: path.trim(),
                  overwrite,
                  selectedSkillIds: [item.skillId],
                },
              },
            })),
          });
        close();
      }
    } catch (cause) {
      if (token === request.current) {
        const message = taskErrorMessage(cause, "import");
        setError(message);
        toast.error(message);
      }
    } finally {
      if (token === request.current) {pendingGitHub.current = null; setPreparing(false);}
    }
  }
  return (
    <>
      <Dialog
        open={visible && !wizard}
        onOpenChange={(value, details) => {
          if (!value) {
            if (details.reason === "outside-press" && pendingGitHub.current) detachGitHubImport();
            else close();
          }
        }}
      >
        <DialogContent className="sm:max-w-xl">
          <DialogHeader>
            <DialogTitle className="flex items-center gap-2"><PackagePlus className="size-4" aria-hidden="true"/>{t("resource.addSkills")}</DialogTitle>
          </DialogHeader>
          <div className="grid grid-cols-[4rem_minmax(0,1fr)_auto] items-center gap-x-3 gap-y-4 py-2">
            <label htmlFor="add-github">GitHub</label>
            <Input
              id="add-github"
              value={repo}
              disabled={preparing}
              onChange={(e) => setRepo(e.target.value)}
              placeholder="owner/repo"
            />
            <Button
              disabled={preparing || !repo.trim()}
              onClick={() => void add("github")}
            ><ActionIcon action="import"/>
              {t("common.import")}
            </Button>
            <label htmlFor="add-local">{t("workflow.local")}</label>
            <div className="relative min-w-0">
              <Input
                id="add-local"
                className="pr-10"
                value={path}
                disabled={preparing}
                onChange={(e) => setPath(e.target.value)}
                placeholder={t("workflow.localFolderPlaceholder")}
              />
              <Button
                className="absolute right-1 top-1/2 -translate-y-1/2"
                size="icon-sm"
                variant="ghost"
                title={t("common.browse")}
                aria-label={t("common.browse")}
                disabled={preparing}
                onClick={async () => {
                  try {
                    const result = await open({
                      directory: true,
                      multiple: false,
                    });
                    if (typeof result === "string") setPath(result);
                  } catch {
                    toast.error(t("workflow.operationFailed"));
                  }
                }}
              >
                <FolderOpen className="size-4" />
              </Button>
            </div>
            <Button
              disabled={preparing || !path.trim()}
              onClick={() => void add("local")}
            ><ActionIcon action="add"/>
              {t("common.add")}
            </Button>
            <label className="col-start-2 col-span-2 flex items-center gap-2 text-xs text-muted-foreground">
              <input
                type="checkbox"
                checked={overwrite}
                disabled={preparing}
                onChange={(e) => setOverwrite(e.target.checked)}
              />
              {t("workflow.overwriteLocal")}
            </label>
          </div>
          {error && <p role="alert" className="text-sm text-destructive">{error}</p>}
          <DialogFooter>
            <Button variant="outline" onClick={close}><ActionIcon action="cancel"/>
              {t("common.cancel")}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
      <GitHubRepoImportWizard
        open={visible && wizard}
        onOpenChange={(value, details) => {
          if (!value) {
            if (details?.reason === "outside-press" && !preparedTaskId && importPreview) {
              continueGitHubImportInBackground(repo.trim(), Promise.resolve(importPreview));
              toast.info(t("workflow.importNeedsReview"));
            }
            close();
          }
        }}
        repoUrl={repo}
        onRepoUrlChange={setRepo}
        preview={importPreview ?? store.githubImport.preview}
        previewError={store.githubImport.error}
        isPreviewLoading={store.githubImport.isPreviewLoading}
        isImporting={false}
        importResult={null}
        onPreview={async () => {const preview = await store.previewGitHubRepoImport(repo); setImportPreview(preview); return preview;}}
        onReset={store.resetGitHubImport}
        launcherLabel={t("resource.addSkills")}
        onImport={selections => queueGitHub(selections)}
      />
    </>
  );
}
