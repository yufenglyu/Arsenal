import { Columns3, RotateCcw } from "lucide-react";
import { useState } from "react";
import { useTranslation } from "react-i18next";

import { optionsForSkillTable } from "@/components/skill/skillColumnOptions";
import { Button } from "@/components/ui/button";
import { FIXED_SKILL_COLUMNS, type SkillTableKind } from "@/hooks/useSkillTableColumns";
import { cn } from "@/lib/utils";

const fixedColumnKeys = new Set<string>(FIXED_SKILL_COLUMNS);

interface SkillColumnSettingsProps {
  kind: SkillTableKind;
  visibleColumns: Set<string>;
  onToggle: (key: string) => void;
  onReset: () => void;
  className?: string;
}

export function SkillColumnSettings({
  kind,
  visibleColumns,
  onToggle,
  onReset,
  className,
}: SkillColumnSettingsProps) {
  const { t } = useTranslation();
  const [open, setOpen] = useState(false);

  return (
    <div className={cn("relative", className)}>
      <Button
        type="button"
        variant="ghost"
        size="icon-xs"
        aria-label={t("skillBrowser.columnSettings")}
        title={t("skillBrowser.columnSettings")}
        aria-expanded={open}
        onClick={() => setOpen((value) => !value)}
      >
        <Columns3 className="size-4" />
      </Button>

      {open ? (
        <div
          role="menu"
          aria-label={t("skillBrowser.columnSettings")}
          className="absolute bottom-8 right-0 z-50 w-56 rounded-xl border border-border bg-popover p-2 text-popover-foreground shadow-lg"
        >
          <div className="mb-1 px-2 text-xs font-medium text-muted-foreground">
            {t("skillBrowser.visibleColumns")}
          </div>
          <div className="space-y-1">
            {optionsForSkillTable(kind).map((option) => {
              const fixed = option.fixed || fixedColumnKeys.has(option.key);
              return (
                <label
                  key={option.key}
                  className={cn(
                    "flex items-center gap-2 rounded-lg px-2 py-1.5 text-xs",
                    fixed ? "text-muted-foreground" : "cursor-pointer hover:bg-muted"
                  )}
                >
                  <input
                    type="checkbox"
                    aria-label={t(option.labelKey)}
                    checked={visibleColumns.has(option.key)}
                    disabled={fixed}
                    onChange={() => onToggle(option.key)}
                    className="size-3.5 accent-primary"
                  />
                  <span>{t(option.labelKey)}</span>
                  {fixed ? (
                    <span className="ml-auto text-[0.68rem] text-muted-foreground">
                      {t("skillBrowser.fixedColumn")}
                    </span>
                  ) : null}
                </label>
              );
            })}
          </div>
          <div className="mt-2 border-t border-border pt-2">
            <Button
              type="button"
              variant="ghost"
              size="xs"
              className="w-full justify-start"
              onClick={onReset}
            >
              <RotateCcw className="size-4" />
              {t("skillBrowser.resetColumns")}
            </Button>
          </div>
        </div>
      ) : null}
    </div>
  );
}
