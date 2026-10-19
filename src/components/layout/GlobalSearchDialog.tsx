import { useCallback, useDeferredValue, useEffect, useMemo, useState } from "react";
import { useNavigate } from "react-router-dom";
import { useTranslation } from "react-i18next";
import {
  Blocks,
  Layers,
} from "lucide-react";

import {
  Command,
  CommandDialog,
  CommandEmpty,
  CommandGroup,
  CommandInput,
  CommandItem,
  CommandList,
} from "@/components/ui/command";
import { useCentralSkillsStore } from "@/stores/centralSkillsStore";
import { useCollectionStore } from "@/stores/collectionStore";
import { usePlatformStore } from "@/stores/platformStore";
import { useConfiguredHotkey } from "@/hooks/useConfiguredHotkey";
import { PlatformIcon } from "@/components/platform/PlatformIcon";
import { formatPathForDisplay } from "@/lib/path";
import { buildSearchText, normalizeSearchQuery, scoreSearchMatch } from "@/lib/search";
import { isEnabledInstallTargetAgent } from "@/lib/agents";

interface GlobalSearchDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
}

type SearchItem = {
  id: string;
  label: string;
  description?: string;
  groupKey: "central" | "collections" | "platforms";
  groupLabel: string;
  icon: React.ReactNode;
  searchText: string;
  labelText: string;
  descriptionText: string;
  onSelect: () => void;
};

export function GlobalSearchDialog({
  open,
  onOpenChange,
}: GlobalSearchDialogProps) {
  const navigate = useNavigate();
  const { t } = useTranslation();

  // Data sources
  const centralSkills = useCentralSkillsStore((s) => s.skills);
  const collections = useCollectionStore((s) => s.collections);
  const agents = usePlatformStore((s) => s.agents);
  const [query, setQuery] = useState("");
  const deferredQuery = useDeferredValue(query);
  const normalizedQuery = useMemo(
    () => normalizeSearchQuery(deferredQuery),
    [deferredQuery]
  );

  const close = useCallback(() => onOpenChange(false), [onOpenChange]);
  const groupMeta = useMemo(
    () => [
      {
        key: "central" as const,
        label: t("globalSearch.centralSkills"),
        initialLimit: 8,
      },
      {
        key: "collections" as const,
        label: t("globalSearch.collections"),
        initialLimit: 8,
      },
      {
        key: "platforms" as const,
        label: t("globalSearch.platforms"),
        initialLimit: 10,
      },
    ],
    [t]
  );

  useEffect(() => {
    if (!open) {
      setQuery("");
    }
  }, [open]);

  // Build flat search items
  const items = useMemo<SearchItem[]>(() => {
    if (!open) return [];

    const result: SearchItem[] = [];

    // Shared Hub
    for (const skill of centralSkills) {
      const labelText = skill.name.toLowerCase();
      const descriptionText = (skill.description ?? "").toLowerCase();
      result.push({
        id: `central-${skill.id}`,
        label: skill.name,
        description: skill.description,
        groupKey: "central",
        groupLabel: t("globalSearch.centralSkills"),
        icon: <Blocks className="size-4 shrink-0 text-primary/70" />,
        searchText: buildSearchText([
          skill.name,
          skill.description,
          skill.notes,
          ...(skill.tags ?? []),
          skill.source_author,
          skill.source_repo,
        ]),
        labelText,
        descriptionText,
        onSelect: () => {
          close();
          navigate(`/skill/${skill.id}`);
        },
      });
    }

    // Skill Bundles
    for (const col of collections) {
      result.push({
        id: `collection-${col.id}`,
        label: col.name,
        description: col.description,
        groupKey: "collections",
        groupLabel: t("globalSearch.collections"),
        icon: <Layers className="size-4 shrink-0 text-primary/70" />,
        searchText: buildSearchText([col.name, col.description]),
        labelText: col.name.toLowerCase(),
        descriptionText: (col.description ?? "").toLowerCase(),
        onSelect: () => {
          close();
          useCollectionStore.getState().selectGroup(col.id);
          navigate("/resources");
        },
      });
    }

    // Platform Views
    const platformAgents = agents.filter(isEnabledInstallTargetAgent);
    for (const agent of platformAgents) {
      const displayPath = formatPathForDisplay(agent.global_skills_dir);
      result.push({
        id: `platform-${agent.id}`,
        label: agent.display_name,
        description: displayPath,
        groupKey: "platforms",
        groupLabel: t("globalSearch.platforms"),
        icon: (
          <PlatformIcon agentId={agent.id} className="size-4 text-primary/70" />
        ),
        searchText: buildSearchText([agent.display_name, agent.global_skills_dir]),
        labelText: agent.display_name.toLowerCase(),
        descriptionText: displayPath.toLowerCase(),
        onSelect: () => {
          close();
          navigate(`/platform/${agent.id}`);
        },
      });
    }

    return result;
  }, [
    centralSkills,
    collections,
    agents,
    navigate,
    close,
    open,
    t,
  ]);

  const visibleGroups = useMemo(() => {
    if (!open) return [];

    if (!normalizedQuery) {
      return groupMeta
        .map((group) => ({
          key: group.key,
          heading: group.label,
          items: items
            .filter((item) => item.groupKey === group.key)
            .slice(0, group.initialLimit),
        }))
        .filter((group) => group.items.length > 0);
    }

    const matchedItems = items
      .map((item) => ({
        item,
        score: scoreSearchMatch(
          normalizedQuery,
          item.labelText,
          item.descriptionText,
          item.searchText
        ),
      }))
      .filter((entry) => Number.isFinite(entry.score))
      .sort((left, right) => {
        if (left.score !== right.score) {
          return left.score - right.score;
        }
        return left.item.label.localeCompare(right.item.label);
      })
      .slice(0, 50)
      .map((entry) => entry.item);

    return groupMeta
      .map((group) => ({
        key: group.key,
        heading: group.label,
        items: matchedItems.filter((item) => item.groupKey === group.key),
      }))
      .filter((group) => group.items.length > 0);
  }, [groupMeta, items, normalizedQuery, open]);

  useConfiguredHotkey("globalSearch", () => onOpenChange(!open), {
    allowInEditable: true,
  });

  return (
    <CommandDialog
      open={open}
      onOpenChange={onOpenChange}
      title={t("globalSearch.title")}
      description={t("globalSearch.description")}
      className="sm:max-w-lg"
      showCloseButton={false}
    >
      <Command shouldFilter={false}>
        <CommandInput
          placeholder={t("globalSearch.placeholder")}
          value={query}
          onValueChange={setQuery}
          clearLabel={t("common.clearSearch")}
        />
        <CommandList>
          <CommandEmpty>{t("globalSearch.noResults")}</CommandEmpty>
          {visibleGroups.map((group) => (
            <CommandGroup key={group.key} heading={group.heading}>
              {group.items.map((item) => (
                <CommandItem
                  key={item.id}
                  value={`${item.label} ${item.description ?? ""}`}
                  onSelect={item.onSelect}
                >
                  {item.icon}
                  <div className="flex flex-col min-w-0">
                    <span className="truncate text-sm">{item.label}</span>
                    {item.description && (
                      <span className="truncate text-xs text-muted-foreground">
                        {item.description}
                      </span>
                    )}
                  </div>
                </CommandItem>
              ))}
            </CommandGroup>
          ))}
        </CommandList>
      </Command>
    </CommandDialog>
  );
}
