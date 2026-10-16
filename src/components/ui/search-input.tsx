import { Search, X } from "lucide-react";
import * as React from "react";
import { useTranslation } from "react-i18next";

import { Input } from "@/components/ui/input";
import { cn } from "@/lib/utils";

interface SearchInputProps
  extends Omit<React.ComponentProps<typeof Input>, "value" | "onChange"> {
  value: string;
  onValueChange: (value: string) => void;
  clearLabel?: string;
  trailing?: React.ReactNode;
  containerClassName?: string;
  iconClassName?: string;
  clearButtonClassName?: string;
}

export const SearchInput = React.forwardRef<HTMLInputElement, SearchInputProps>(
  (
    {
      value,
      onValueChange,
      clearLabel,
      trailing,
      containerClassName,
      className,
      iconClassName,
      clearButtonClassName,
      onKeyDown,
      ...props
    },
    ref
  ) => {
    const { t } = useTranslation();
    const resolvedClearLabel = clearLabel ?? t("common.clearSearch");

    return (
      <div className={cn("relative", containerClassName)}>
        <Search
          className={cn(
            "pointer-events-none absolute left-2.5 top-1/2 size-4 -translate-y-1/2 text-muted-foreground",
            iconClassName
          )}
        />
        <Input
          ref={ref}
          data-page-search="true"
          value={value}
          onChange={(event) => onValueChange(event.target.value)}
          onKeyDown={(event) => {
            onKeyDown?.(event);
            if (event.defaultPrevented || event.nativeEvent.isComposing || event.key !== "Escape") return;
            event.preventDefault();
            event.stopPropagation();
            onValueChange("");
            event.currentTarget.blur();
          }}
          className={cn("bg-muted/40 pl-8", trailing ? (value ? "pr-18" : "pr-10") : value ? "pr-9" : "pr-3", className)}
          {...props}
        />
        {value.length > 0 && (
          <button
            type="button"
            onClick={() => onValueChange("")}
            aria-label={resolvedClearLabel}
            title={resolvedClearLabel}
            className={cn(
              "absolute right-1.5 top-1/2 inline-flex size-6 -translate-y-1/2 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-muted hover:text-foreground",
              trailing && "right-10",
              clearButtonClassName
            )}
          >
            <X className="size-4" />
          </button>
        )}
        {trailing && <div className="absolute right-1 top-1/2 z-30 -translate-y-1/2">{trailing}</div>}
      </div>
    );
  }
);

SearchInput.displayName = "SearchInput";
