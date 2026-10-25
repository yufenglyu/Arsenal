import { useId, useState, type ReactNode } from "react";
import { ChevronRight } from "lucide-react";
import { Button } from "@/components/ui/button";

export function TaskDisclosure({
  title,
  children,
  small = false,
  header,
  actions,
  expanded: controlledExpanded,
  onExpandedChange,
}: {
  title: ReactNode;
  children: ReactNode;
  small?: boolean;
  header?: ReactNode;
  actions?: ReactNode;
  expanded?: boolean;
  onExpandedChange?: (expanded: boolean) => void;
}) {
  const [localExpanded, setExpanded] = useState(false);
  const expanded = controlledExpanded ?? localExpanded;
  const panelId = useId();
  if (header) return (
    <div>
      <div className="flex flex-wrap items-center gap-2">
        <div className="flex min-w-0 flex-1 flex-wrap items-center gap-x-4 gap-y-2">{header}</div>
        <Button
          variant="ghost"
          size="icon"
          title={typeof title === "string" ? title : undefined}
          aria-expanded={expanded}
          aria-controls={panelId}
          onClick={() => {if (controlledExpanded === undefined) setExpanded(!expanded); onExpandedChange?.(!expanded);}}
        >
          <span className="sr-only">{title}</span>
          <ChevronRight className={`transition-transform ${expanded ? "rotate-90" : ""}`} />
        </Button>
        {actions}
      </div>
      <div id={panelId} hidden={!expanded} className="mt-3 rounded-md border border-border bg-muted/20 p-3">{children}</div>
    </div>
  );
  return (
    <details open={controlledExpanded} onToggle={onExpandedChange ? event => onExpandedChange(event.currentTarget.open) : undefined} className="group/task-fold rounded-md border border-transparent px-3 py-2 open:border-border open:bg-muted/20">
      <summary
        className={`flex cursor-pointer list-none items-center gap-2 rounded-sm font-normal focus-visible:outline-2 focus-visible:outline-ring [&::-webkit-details-marker]:hidden [&_svg]:size-[1em] [&_svg]:shrink-0 ${small ? "text-xs text-muted-foreground" : "text-sm"}`}
      >
        {title}
        <ChevronRight className="ml-auto size-[1em] shrink-0 transition-transform group-open/task-fold:rotate-90" />
      </summary>
      <div className="mt-3">{children}</div>
    </details>
  );
}
