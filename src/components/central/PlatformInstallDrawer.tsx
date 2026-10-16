import { ActionIcon } from "@/components/ui/action-icon";
import { useMemo, useState } from "react";
import { Dialog as DialogPrimitive } from "@base-ui/react/dialog";
import { XIcon } from "lucide-react";
import { useTranslation } from "react-i18next";

import {
  Dialog,
  DialogClose,
  DialogDescription,
  DialogOverlay,
  DialogPortal,
  DialogTitle,
} from "@/components/ui/dialog";
import { Button } from "@/components/ui/button";
import { SearchInput } from "@/components/ui/search-input";
import { PlatformIcon } from "@/components/platform/PlatformIcon";
import type { AgentWithStatus, SkillWithLinks } from "@/types";
import { isInstallTargetAgent } from "@/lib/agents";
import { cn } from "@/lib/utils";

type PlatformDrawerTab = "installed" | "all" | "shared";

interface PlatformInstallDrawerProps {
  open: boolean;
  skill: SkillWithLinks | null;
  agents: AgentWithStatus[];
  togglingAgentId: string | null;
  onOpenChange: (open: boolean) => void;
  onToggle: (skillId: string, agentId: string) => void;
  onOpenInstallDialog?: () => void;
}

export function PlatformInstallDrawer({
  open,
  skill,
  agents,
  togglingAgentId,
  onOpenChange,
  onToggle,
  onOpenInstallDialog,
}: PlatformInstallDrawerProps) {
  const { t } = useTranslation();
  const [query, setQuery] = useState("");
  const [activeTab, setActiveTab] = useState<PlatformDrawerTab>("installed");
  const titleId = "platform-install-drawer-title";

  const targetAgents = useMemo(
    () => agents.filter(isInstallTargetAgent),
    [agents]
  );
  const linkedAgentIds = useMemo(
    () => new Set(skill?.linked_agents ?? []),
    [skill?.linked_agents]
  );
  const readOnlyAgentIds = useMemo(
    () => new Set(skill?.read_only_agents ?? []),
    [skill?.read_only_agents]
  );

  const rows = useMemo(() => {
    const normalizedQuery = query.trim().toLowerCase();
    return targetAgents
      .filter((agent) => {
        const isLinked = linkedAgentIds.has(agent.id);
        const isReadOnly = readOnlyAgentIds.has(agent.id);
        if (activeTab === "installed" && !isLinked && !isReadOnly) return false;
        if (activeTab === "shared" && !isReadOnly) return false;
        if (!normalizedQuery) return true;
        return `${agent.display_name} ${agent.id}`.toLowerCase().includes(normalizedQuery);
      })
      .sort((a, b) => {
        const aInstalled = linkedAgentIds.has(a.id) || readOnlyAgentIds.has(a.id);
        const bInstalled = linkedAgentIds.has(b.id) || readOnlyAgentIds.has(b.id);
        if (aInstalled !== bInstalled) return aInstalled ? -1 : 1;
        return a.display_name.localeCompare(b.display_name);
      });
  }, [activeTab, linkedAgentIds, query, readOnlyAgentIds, targetAgents]);

  if (!skill) return null;

  const tabs: Array<{ value: PlatformDrawerTab; label: string }> = [
    { value: "installed", label: t("platformDrawer.tabInstalled") },
    { value: "all", label: t("sidebar.softwarePlatforms") },
    { value: "shared", label: t("platformDrawer.tabShared") },
  ];

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogPortal keepMounted={false}>
        <DialogOverlay className="bg-black/20" />
        <DialogPrimitive.Popup
          role="dialog"
          aria-modal="true"
          aria-labelledby={titleId}
          className={cn(
            "fixed inset-y-0 right-0 z-50 flex h-full w-screen flex-col bg-background shadow-2xl ring-1 ring-border outline-none",
            "sm:w-[min(520px,92vw)]"
          )}
        >
          <div className="flex min-h-0 flex-1 flex-col">
            <div className="shrink-0 border-b border-border p-4">
              <div className="flex items-start justify-between gap-3">
                <div className="min-w-0 space-y-1">
                  <DialogTitle id={titleId} className="truncate">
                    {t("platformDrawer.title", { name: skill.name })}
                  </DialogTitle>
                  {skill.description && (
                    <DialogDescription className="line-clamp-2 text-xs">
                      {skill.description}
                    </DialogDescription>
                  )}
                </div>
                <DialogClose
                  render={
                    <Button
                      type="button"
                      variant="ghost"
                      size="icon-sm"
                      aria-label={t("common.close")}
                    />
                  }
                >
                  <XIcon />
                </DialogClose>
              </div>
              <div className="mt-4 flex items-center gap-2">
                <SearchInput
                  value={query}
                  onValueChange={setQuery}
                  placeholder={t("platformDrawer.searchPlaceholder")}
                  containerClassName="flex-1"
                  className="h-9"
                />
                {onOpenInstallDialog && (
                  <Button type="button" variant="outline" size="sm" onClick={onOpenInstallDialog}><ActionIcon action="open"/>
                    {t("platformDrawer.openFull")}
                  </Button>
                )}
              </div>
              <div
                role="tablist"
                aria-label={t("platformDrawer.tabsLabel")}
                className="mt-3 flex rounded-xl bg-muted/40 p-1"
              >
                {tabs.map((tab) => (
                  <button
                    key={tab.value}
                    type="button"
                    role="tab"
                    aria-selected={activeTab === tab.value}
                    onClick={() => setActiveTab(tab.value)}
                    className={cn(
                      "h-7 flex-1 rounded-lg px-2 text-xs font-medium transition-colors",
                      "focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-1",
                      activeTab === tab.value
                        ? "bg-background/95 text-foreground shadow-sm"
                        : "text-muted-foreground hover:bg-background/60 hover:text-foreground"
                    )}
                  >
                    {tab.label}
                  </button>
                ))}
              </div>
            </div>

            <div className="min-h-0 flex-1 overflow-y-auto p-3">
              {rows.length === 0 ? (
                <div className="rounded-xl border border-dashed border-border p-6 text-center text-sm text-muted-foreground">
                  {t("platformDrawer.noPlatforms")}
                </div>
              ) : (
                <div className="space-y-2">
                  {rows.map((agent) => {
                    const isLinked = linkedAgentIds.has(agent.id);
                    const isReadOnly = readOnlyAgentIds.has(agent.id);
                    const isToggling = togglingAgentId === agent.id;
                    const statusLabel = isReadOnly
                      ? t("platformDrawer.statusShared")
                      : isLinked
                        ? t("platformDrawer.statusInstalled")
                        : t("platformDrawer.statusNotInstalled");

                    return (
                      <div
                        key={agent.id}
                        className="flex items-center gap-3 rounded-xl border border-border bg-card px-3 py-2.5"
                      >
                        <PlatformIcon agentId={agent.id} className="size-[1em] text-muted-foreground" size={14} />
                        <div className="min-w-0 flex-1">
                          <div className="truncate text-sm font-medium text-foreground">
                            {agent.display_name}
                          </div>
                          <div className="mt-0.5 flex items-center gap-2 text-xs text-muted-foreground">
                            <span>{statusLabel}</span>
                            {!agent.is_detected && <span>{t("installDialog.notDetected")}</span>}
                          </div>
                        </div>
                        {isReadOnly ? (
                          <Button
                            type="button"
                            variant="ghost"
                            size="sm"
                            disabled
                            aria-label={t("platformDrawer.sharedAria", {
                              platform: agent.display_name,
                            })}
                          ><ActionIcon action="install"/>
                            {t("installDialog.alwaysIncluded")}
                          </Button>
                        ) : (
                          <Button
                            type="button"
                            variant={isLinked ? "outline" : "default"}
                            size="sm"
                            disabled={isToggling}
                            aria-label={
                              isLinked
                                ? t("platformDrawer.uninstallAria", {
                                    skill: skill.name,
                                    platform: agent.display_name,
                                  })
                                : t("platformDrawer.installAria", {
                                    skill: skill.name,
                                    platform: agent.display_name,
                                  })
                            }
                            onClick={() => onToggle(skill.id, agent.id)}
                          ><ActionIcon action={isLinked ? "uninstall" : "install"}/>
                            {isLinked ? t("common.uninstall") : t("common.install")}
                          </Button>
                        )}
                      </div>
                    );
                  })}
                </div>
              )}
            </div>
          </div>
        </DialogPrimitive.Popup>
      </DialogPortal>
    </Dialog>
  );
}
