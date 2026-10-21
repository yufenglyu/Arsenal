import { Popover } from "@base-ui/react/popover";
import { Filter } from "lucide-react";
import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import { allSearchScopes, type SearchScope } from "@/lib/skillFilters";

export function SearchScopes({ value, onChange }: {
  value: SearchScope[];
  onChange: (value: SearchScope[]) => void;
}) {
  const { t } = useTranslation();
  const [open, setOpen] = useState(false);
  useEffect(() => {
    const close = () => setOpen(false);
    window.addEventListener("blur", close);
    return () => window.removeEventListener("blur", close);
  }, []);
  return <Popover.Root open={open} onOpenChange={setOpen}>
    <Popover.Trigger render={<Button variant="ghost" size="icon" />}
      aria-label={t("workflow.searchScope")} title={t("workflow.searchScope")}>
      <Filter aria-hidden="true" />
    </Popover.Trigger>
    <Popover.Portal>
      <Popover.Positioner align="end" sideOffset={4} className="z-[60]">
        <Popover.Popup initialFocus={false} aria-label={t("workflow.searchScope")}
          className="min-w-40 select-none rounded-md border border-border bg-popover p-3 text-sm text-popover-foreground shadow-md outline-none">
          {allSearchScopes.map(scope => <label key={scope} className="flex items-center gap-2 p-1"
            // Keep the native text-selection hit test away from a changing popup.
            // The label click still toggles its checkbox, and keyboard focus is unaffected.
            onMouseDown={event => { if (event.button === 0) event.preventDefault(); }}>
            <input type="checkbox" checked={value.includes(scope)}
              disabled={value.length === 1 && value.includes(scope)}
              onChange={() => onChange(value.includes(scope) ? value.filter(s => s !== scope) : [...value, scope])} />
            {t(`workflow.scope.${scope}`)}
          </label>)}
        </Popover.Popup>
      </Popover.Positioner>
    </Popover.Portal>
  </Popover.Root>;
}
