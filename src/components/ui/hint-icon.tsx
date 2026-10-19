import { CircleHelp } from "lucide-react";
import { cn } from "@/lib/utils";

export function HintIcon({ text, className }: { text: string; className?: string }) {
  return (
    <span
      tabIndex={0}
      title={text}
      aria-label={text}
      className={cn(
        "inline-flex size-5 shrink-0 items-center justify-center rounded-full text-muted-foreground transition-colors hover:bg-muted/70 hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring",
        className
      )}
    >
      <CircleHelp className="size-4" />
    </span>
  );
}
