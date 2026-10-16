import { useMemo } from "react";
import { useTranslation } from "react-i18next";

import { Checkbox } from "@/components/ui/checkbox";
import {
  CENTRAL_AGENT_ID,
  isCollectionInstallTargetAgent,
  isInstallTargetAgent,
} from "@/lib/agents";
import { isProjectAgentId } from "@/lib/projectTargets";
import type { AgentWithStatus } from "@/types";

interface InstallTargetListProps {
  agents: AgentWithStatus[];
  selectedAgentIds: Set<string>;
  onToggleAgent: (agentId: string, checked: boolean) => void;
  linkedAgentIds?: Set<string>;
  readOnlyAgentIds?: Set<string>;
  isCentral?: boolean;
  includeCentral?: boolean;
  emptyMessage?: string;
  ariaLabel: string;
}

function byDisplayName(a: AgentWithStatus, b: AgentWithStatus) {
  return a.display_name.localeCompare(b.display_name, undefined, {
    numeric: true,
    sensitivity: "base",
  });
}

export function InstallTargetList({
  agents,
  selectedAgentIds,
  onToggleAgent,
  linkedAgentIds = new Set(),
  readOnlyAgentIds = new Set(),
  isCentral = false,
  includeCentral = false,
  emptyMessage,
  ariaLabel,
}: InstallTargetListProps) {
  const { t } = useTranslation();
  const groupedTargets = useMemo(() => {
    const targets = agents.filter((agent) =>
      includeCentral ? isCollectionInstallTargetAgent(agent) : isInstallTargetAgent(agent)
    );
    return {
      software: targets
        .filter((agent) => !isProjectAgentId(agent.id) && agent.id !== CENTRAL_AGENT_ID && agent.is_detected)
        .sort(byDisplayName),
      projects: targets
        .filter((agent) => isProjectAgentId(agent.id))
        .sort(byDisplayName),
      central: includeCentral
        ? targets.filter((agent) => agent.id === CENTRAL_AGENT_ID)
        : [],
    };
  }, [agents, includeCentral]);

  const sections = [
    {
      key: "software",
      title: t("installDialog.softwarePlatforms"),
      agents: groupedTargets.software,
    },
    {
      key: "projects",
      title: t("installDialog.projectDirectories"),
      agents: groupedTargets.projects,
    },
    {
      key: "central",
      title: t("installDialog.centralSkills"),
      agents: groupedTargets.central,
    },
  ].filter((section) => section.agents.length > 0);

  if (sections.length === 0) {
    return (
      <p className="text-sm text-muted-foreground">
        {emptyMessage ?? t("installDialog.noPlatforms")}
      </p>
    );
  }

  return (
    <div className="space-y-4" role="group" aria-label={ariaLabel}>
      {sections.map((section) => (
        <section key={section.key} className="space-y-2">
          <div className="flex items-center justify-between gap-2">
            <h3 className="text-sm font-semibold uppercase tracking-wide text-muted-foreground">
              {section.title}
            </h3>
            <span className="text-xs text-muted-foreground">
              {t("installDialog.targetCount", { count: section.agents.length })}
            </span>
          </div>
          <div className="grid grid-cols-1 gap-x-4 gap-y-2 sm:grid-cols-2">
            {section.agents.map((agent) => {
              const isLinked = linkedAgentIds.has(agent.id);
              const isReadOnly = readOnlyAgentIds.has(agent.id);
              const isCentralTarget = agent.id === CENTRAL_AGENT_ID;
              const label = isCentralTarget
                ? t("installDialog.centralSkills")
                : agent.display_name;
              const isSharedPlatform = !!agent.shares_central_skills;
              const isSharedThroughCentral = isSharedPlatform && isCentral;
              const isDisabled = isReadOnly || isSharedThroughCentral;
              const isChecked = selectedAgentIds.has(agent.id);

              return (
                <div key={agent.id} className="flex items-center gap-2">
                  <Checkbox
                    checked={isChecked}
                    disabled={isDisabled}
                    onCheckedChange={(checked) => onToggleAgent(agent.id, !!checked)}
                    aria-label={label}
                  />
                  <span
                    className={
                      isDisabled
                        ? "min-w-0 flex-1 select-none truncate text-sm text-muted-foreground"
                        : "min-w-0 flex-1 cursor-pointer select-none truncate text-sm text-foreground"
                    }
                    onClick={() => {
                      if (!isDisabled) onToggleAgent(agent.id, !isChecked);
                    }}
                  >
                    {label}
                  </span>
                  {isSharedThroughCentral ? (
                    <span className="shrink-0 text-xs text-primary">
                      {t("installDialog.sharedViaCentral")}
                    </span>
                  ) : isSharedPlatform ? (
                    <span className="shrink-0 text-xs text-amber-600 dark:text-amber-400">
                      {t("installDialog.sharedPlatformCentralize")}
                    </span>
                  ) : isReadOnly ? (
                    <span className="shrink-0 text-xs text-muted-foreground">
                      {t("installDialog.sharedAvailable")}
                    </span>
                  ) : isLinked ? (
                    <span className="shrink-0 text-xs text-primary">
                      {t("installDialog.alreadyLinked")}
                    </span>
                  ) : null}
                  {!agent.is_detected && (
                    <span className="shrink-0 text-xs text-muted-foreground">
                      {t("installDialog.notDetected")}
                    </span>
                  )}
                </div>
              );
            })}
          </div>
        </section>
      ))}
    </div>
  );
}
