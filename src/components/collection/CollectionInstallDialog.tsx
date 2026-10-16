import { ActionIcon } from "@/components/ui/action-icon";
import { useState, useEffect } from "react";
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
import { InstallTargetList } from "@/components/central/InstallTargetList";
import { AgentWithStatus, CollectionBatchInstallResult } from "@/types";
import { CENTRAL_AGENT_ID, isCollectionInstallTargetAgent } from "@/lib/agents";

// ─── Props ────────────────────────────────────────────────────────────────────

interface CollectionInstallDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  collectionName: string;
  skillCount: number;
  agents: AgentWithStatus[];
  isCentral?: boolean;
  onInstall: (agentIds: string[]) => Promise<CollectionBatchInstallResult>;
}

// ─── CollectionInstallDialog ──────────────────────────────────────────────────

export function CollectionInstallDialog({
  open,
  onOpenChange,
  collectionName,
  skillCount,
  agents,
  isCentral = false,
  onInstall,
}: CollectionInstallDialogProps) {
  const { t } = useTranslation();
  const targetAgents = agents.filter(isCollectionInstallTargetAgent);

  const [selectedAgentIds, setSelectedAgentIds] = useState<Set<string>>(new Set());
  const [isLoading, setIsLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [result, setResult] = useState<CollectionBatchInstallResult | null>(null);

  // Reset when dialog opens. Install targets always start empty so the user
  // explicitly chooses software platforms, project directories, and Shared Hub.
  useEffect(() => {
    if (open) {
      setSelectedAgentIds(new Set());
      setError(null);
      setResult(null);
    }
  }, [open]);

  function handleToggle(agentId: string, checked: boolean) {
    setSelectedAgentIds((prev) => {
      const next = new Set(prev);
      if (checked) next.add(agentId);
      else next.delete(agentId);
      return next;
    });
  }

  function getSelectedInstallableAgentIds() {
    return Array.from(selectedAgentIds).filter((agentId) => {
      const agent = targetAgents.find((candidate) => candidate.id === agentId);
      if (!agent) return false;
      return !(agent.shares_central_skills && isCentral);
    });
  }

  async function handleInstall() {
    const agentIds = getSelectedInstallableAgentIds();
    if (agentIds.length === 0) {
      setError(t("batchInstall.selectPlatform"));
      return;
    }

    setIsLoading(true);
    setError(null);
    try {
      const installResult = await onInstall(agentIds);
      setResult(installResult);
      if (installResult.failed.length === 0) {
        onOpenChange(false);
      }
    } catch (err) {
      setError(String(err));
    } finally {
      setIsLoading(false);
    }
  }

  const selectedInstallableCount = getSelectedInstallableAgentIds().length;
  const hasSharedSelection = targetAgents.some(
    (agent) =>
      (agent.shares_central_skills || agent.id === CENTRAL_AGENT_ID) &&
      selectedAgentIds.has(agent.id)
  );

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-2xl">
        <DialogHeader>
          <DialogTitle>{t("batchInstall.title", { name: collectionName })}</DialogTitle>
          <DialogClose />
        </DialogHeader>

        <DialogBody className="space-y-5">
          <DialogDescription>
            {t("batchInstall.desc", { count: skillCount })}
          </DialogDescription>

          <InstallTargetList
            agents={targetAgents}
            selectedAgentIds={selectedAgentIds}
            onToggleAgent={handleToggle}
            isCentral={isCentral}
            includeCentral
            emptyMessage={t("batchInstall.noPlatforms")}
            ariaLabel={t("batchInstall.selectPlatforms")}
          />

          {hasSharedSelection ? (
            <p className="text-xs text-amber-600 dark:text-amber-400">
              {t("installDialog.sharedPlatformHint")}
            </p>
          ) : null}

          {result && result.failed.length > 0 && (
            <div className="space-y-1">
              <p className="text-xs text-amber-600 dark:text-amber-400 font-medium">
                {t("batchInstall.succeeded", {
                  succeeded: result.succeeded.length,
                  failed: result.failed.length,
                })}
              </p>
              <ul className="text-xs text-muted-foreground space-y-0.5">
                {result.failed.map((f) => (
                  <li key={f.agent_id} className="text-destructive">
                    {f.agent_id}: {f.error}
                  </li>
                ))}
              </ul>
              <Button
                variant="outline"
                size="sm"
                onClick={() => onOpenChange(false)}
                className="mt-2"
              ><ActionIcon action="cancel"/>
                {t("batchInstall.close")}
              </Button>
            </div>
          )}

          {error && (
            <p className="text-xs text-destructive" role="alert">
              {error}
            </p>
          )}
        </DialogBody>

        {!result && (
          <DialogFooter>
            <Button
              variant="outline"
              onClick={() => onOpenChange(false)}
              disabled={isLoading}
            ><ActionIcon action="cancel"/>
              {t("batchInstall.cancel")}
            </Button>
            <Button
              onClick={handleInstall}
              disabled={isLoading || selectedInstallableCount === 0}
            >
              {isLoading ? (
                <>
                  <Loader2 className="size-4 animate-spin" />
                  {t("batchInstall.installing")}
                </>
              ) : (
                <><ActionIcon action="install"/>{t("batchInstall.install", { count: selectedInstallableCount })}</>
              )}
            </Button>
          </DialogFooter>
        )}
      </DialogContent>
    </Dialog>
  );
}
