import type { ReactNode } from "react";
import { ChevronRight } from "lucide-react";

export function TaskDisclosure({
  title,
  children,
  small = false,
}: {
  title: ReactNode;
  children: ReactNode;
  small?: boolean;
}) {
  return (
    <details className="group/task-fold rounded-md border border-transparent px-3 py-2 open:border-border open:bg-muted/20">
      <summary
        className={`flex cursor-pointer list-none items-center gap-2 rounded-sm focus-visible:outline-2 focus-visible:outline-ring [&::-webkit-details-marker]:hidden ${small ? "text-xs text-muted-foreground" : "text-sm"}`}
      >
        {title}
        <ChevronRight className="ml-auto size-[1em] shrink-0 transition-transform group-open/task-fold:rotate-90" />
      </summary>
      <div className="mt-3">{children}</div>
    </details>
  );
}
