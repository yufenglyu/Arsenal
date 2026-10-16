import { ActionIcon } from "@/components/ui/action-icon";
import { useBrowserStatusStore } from "@/stores/browserStatusStore";
import { usePlatformStore } from "@/stores/platformStore";
import { useCollectionStore } from "@/stores/collectionStore";
import { TaskCenter } from "./TaskCenter";
import { useLocation } from "react-router-dom";
import { useRepositorySyncStore } from "@/stores/repositorySyncStore";
import { AlertCircle, CheckCircle2, Circle, Loader2 } from "lucide-react";
import { useTranslation } from "react-i18next";

import { Button } from "@/components/ui/button";
import { useCentralSkillsStore } from "@/stores/centralSkillsStore";
import { useResourceLibraryStore } from "@/stores/resourceLibraryStore";
import { useAppStatusStore, type AppStatusTask } from "@/stores/appStatusStore";
import { cn } from "@/lib/utils";

function statusIcon(task: AppStatusTask | null) {
  if (!task) return <Circle className="size-4 fill-current text-muted-foreground" />;
  if (task.status === "running") {
    return <Loader2 className="size-4 animate-spin text-primary" />;
  }
  if (task.status === "success") {
    return <CheckCircle2 className="size-4 text-emerald-600" />;
  }
  if (task.status === "error") {
    return <AlertCircle className="size-4 text-destructive" />;
  }
  return <Circle className="size-4 fill-current text-muted-foreground" />;
}

export function AppStatusBar({settingsOpen = false}: {settingsOpen?: boolean} = {}) {
  const {pathname} = useLocation();
  const stats = useBrowserStatusStore(state => state.stats);
  const agents = usePlatformStore(state => state.agents);
  const counts = usePlatformStore(state => state.skillsByAgent);
  const collections = useCollectionStore(state => state.collections.length);
  const collectionSize = useCollectionStore(state => state.currentDetail?.skills.length ?? 0);

  const { t } = useTranslation();
  const setCenterView = useRepositorySyncStore(s => s.setCenterView);
  const task = useAppStatusStore((state) => state.task);
  const resourceSkills = useResourceLibraryStore((state) => state.skills?.length ?? 0);
  const centralSkills = useCentralSkillsStore((state) => state.skills?.length ?? 0);

  const isImport = task?.kind === "import";

  const activeStats = !settingsOpen && stats?.path === pathname ? stats : null;
  const agentId = pathname.startsWith("/platform/") ? decodeURIComponent(pathname.slice("/platform/".length)) : undefined;
  const agent = agents.find(agent => agent.id === agentId);
  const pageLabel = settingsOpen ? t("sidebar.settings") : agent?.display_name ?? (pathname === "/resources" ? t("sidebar.resourceLibrary") : pathname === "/central" ? t("sidebar.centralSkills") : pathname.startsWith("/collections") ? t("sidebar.collections") : pathname === "/settings" ? t("sidebar.settings") : t("status.ready"));
  const total = agentId ? counts[agentId] ?? 0 : pathname === "/resources" ? resourceSkills : pathname === "/central" ? centralSkills : activeStats?.collections ? collections : pathname.startsWith("/collections/") ? collectionSize : activeStats?.skills ?? 0;
  const contextDetail = activeStats ? [
    activeStats.collections ? t("status.collectionCounts", {visible:activeStats.groups,total}) : t("status.skillCounts", {visible:activeStats.skills,total}),
    activeStats.collections ? t("status.selectedCollections", {count:activeStats.selectedGroups}) : t("status.selectedSkills", {count:activeStats.selected}),
    !activeStats.collections && t("status.repositoryCounts", {count:activeStats.groups}),
    !activeStats.collections && t("status.installedSkills", {count:activeStats.installed}),
    agent && t(agent.shares_central_skills ? "status.sharedDirectory" : "status.independentDirectory"),
    activeStats.name,
  ].filter(Boolean).join(" · ") : "";
  const busyTask = task?.status === "running";
  const label = busyTask ? task.label : pageLabel;
  const detail = busyTask ? task.detail ?? "" : contextDetail;
  const statusTitle =
    task?.error && task.error !== detail ? `${label}: ${detail} (${task.error})` : `${label}: ${detail}`;
  const hasStats =
    task &&
    (typeof task.updatedCount === "number" ||
      typeof task.unchangedCount === "number" ||
      typeof task.deletedCount === "number" ||
      typeof task.skippedCount === "number" ||
      typeof task.failedCount === "number" ||
      (task.items?.length ?? 0) > 0);
  const currentCount = task?.currentCount ?? 0;
  const totalCount = task?.totalCount ?? 0;
  const showProgress = task?.status === "running" && totalCount > 0;
  const progressPercent = showProgress
    ? Math.min(100, Math.round((currentCount / totalCount) * 100))
    : 0;
  const progressLabel = showProgress
    ? t("status.resourceSourceProgressAria", {
        current: currentCount,
        total: totalCount,
        name: detail,
      })
    : detail;
  return (
    <>
      <footer
        className="flex h-10 shrink-0 items-center justify-between gap-3 border-t border-border bg-card/95 px-3 text-xs text-muted-foreground"
        aria-label={t("status.label")}
        title={statusTitle}
      >
        <div className="flex min-w-0 items-center gap-2">
          {statusIcon(busyTask ? task : null)}
          <span
            className={cn(
              "shrink-0 font-normal",
              busyTask && task?.status === "error" && "text-destructive",
              task?.status === "success" && "text-foreground",
              task?.status === "running" && "text-foreground"
            )}
          >
            {label}
          </span>
          <span className="truncate">{detail}</span>
        </div>
        <div className="flex shrink-0 items-center gap-2"><TaskCenter />
          {showProgress ? (
            <>
              <span className="tabular-nums text-foreground">
                {t("status.resourceSourceProgressCount", {
                  current: currentCount,
                  total: totalCount,
                })}
              </span>
              <div
                className="h-1.5 w-24 overflow-hidden rounded-full bg-muted"
                role="progressbar"
                aria-valuemin={0}
                aria-valuemax={totalCount}
                aria-valuenow={currentCount}
                aria-label={progressLabel}
              >
                <div
                  className="h-full rounded-full bg-primary transition-[width]"
                  style={{ width: `${progressPercent}%` }}
                />
              </div>
            </>
          ) : null}
          {hasStats ? (
            <Button
              type="button"
              variant="ghost"
              size="status"
              className="shrink-0 text-muted-foreground"
              onClick={() => setCenterView("history")}
              aria-label={t(isImport ? "status.viewImportStats" : "status.viewUpdateStats")}
            ><ActionIcon action="delete"/>
              {typeof task?.updatedCount === "number" ? (
                <span>{t(isImport ? "status.importedCount" : "status.updatedCount", { count: task.updatedCount })}</span>
              ) : null}
              {typeof task?.unchangedCount === "number" ? (
                <span>{t("status.unchangedCount", { count: task.unchangedCount })}</span>
              ) : null}
              {typeof task?.deletedCount === "number" ? (
                <span>{t("status.deletedCount", { count: task.deletedCount })}</span>
              ) : null}
              {typeof task?.skippedCount === "number" ? (
                <span>{t("status.skippedCount", { count: task.skippedCount })}</span>
              ) : null}
              {typeof task?.failedCount === "number" ? (
                <span className={task.failedCount > 0 ? "text-destructive" : undefined}>
                  {t(isImport ? "status.importFailedCount" : "status.failedCount", { count: task.failedCount })}
                </span>
              ) : null}
            </Button>
          ) : null}
        </div>
      </footer>
    </>
  );
}
