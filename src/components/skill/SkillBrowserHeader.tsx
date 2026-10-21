import type { ReactNode } from "react";

/** Compact toolbar inside the list pane, aligned with the inspector tabs. */
export function SkillBrowserHeader({ title, search, actions, viewControl }: { title: ReactNode; search: ReactNode; actions?: ReactNode; viewControl?: ReactNode }) {
  return <header className="flex h-11 min-w-0 shrink-0 items-center gap-2 border-b border-border pl-2">
    <div className="flex min-w-0 flex-1 items-center gap-1 overflow-x-auto [scrollbar-width:none] [&>div]:flex [&>div]:items-center [&>div]:gap-1 [&_h1]:sr-only">{title}{viewControl}{actions}</div>
    <div className="ml-auto w-1/2 min-w-0 max-w-xl shrink-0 [&>div]:w-full [&>div]:min-w-0 [&>div]:max-w-none">{search}</div>
  </header>;
}
