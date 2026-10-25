import { useEffect, useRef, useState, type ReactNode } from "react";
import { createPortal } from "react-dom";
import { useTranslation } from "react-i18next";
import { Tags, FilterX } from "lucide-react";

/** Render the current page's filters above the sidebar footer. */
export function SidebarTagFilter({
  children,
  onClear,
  hasSelection = false,
}: {
  children: ReactNode;
  onClear?: () => void;
  hasSelection?: boolean;
}) {
  const { t } = useTranslation();
  const [target, setTarget] = useState<HTMLElement | null>(null);
  const [open, setOpen] = useState(() => {
    try {
      return localStorage.getItem("arsenal.tags.expanded") !== "false";
    } catch {
      return true;
    }
  });
  function toggleOpen() {
    const next = !open;
    setOpen(next);
    try {
      localStorage.setItem("arsenal.tags.expanded", String(next));
    } catch {
      /* Keep the current session state. */
    }
  }
  const [height, setHeight] = useState(() => {
    try {
      const saved = Number(localStorage.getItem("arsenal.tags.height"));
      return Number.isFinite(saved) && saved >= 60 ? saved : 128;
    } catch {
      return 128;
    }
  });
  const drag = useRef<{ y: number; height: number } | null>(null);
  const body = useRef<HTMLDivElement>(null);
  function resize(next: number) {
    const value = Math.round(
      Math.max(60, Math.min(window.innerHeight * 0.6, next)),
    );
    setHeight(value);
    try {
      localStorage.setItem("arsenal.tags.height", String(value));
    } catch {
      /* Keep in-memory height. */
    }
  }
  useEffect(() => {
    setTarget(document.getElementById("sidebar-tag-filter"));
  }, []);
  const content = (
    <section className="relative border-t border-sidebar-border px-3 py-2">
      {open && (
        <div
          role="separator"
          aria-orientation="horizontal"
          aria-label={t("central.resizeTags")}
          aria-valuenow={height}
          aria-valuemin={60}
          tabIndex={0}
          className="absolute inset-x-0 -top-1 h-2 cursor-row-resize touch-none hover:bg-primary/20 focus-visible:bg-primary/20 focus-visible:outline-none"
          onPointerDown={(event) => {
            event.preventDefault();
            drag.current = {
              y: event.clientY,
              height: body.current?.getBoundingClientRect().height ?? height,
            };
            event.currentTarget.setPointerCapture(event.pointerId);
          }}
          onPointerMove={(event) => {
            if (
              drag.current &&
              event.currentTarget.hasPointerCapture(event.pointerId)
            )
              resize(drag.current.height + drag.current.y - event.clientY);
          }}
          onPointerUp={(event) => {
            drag.current = null;
            if (event.currentTarget.hasPointerCapture(event.pointerId))
              event.currentTarget.releasePointerCapture(event.pointerId);
          }}
          onLostPointerCapture={() => {
            drag.current = null;
          }}
          onKeyDown={(event) => {
            if (event.key === "ArrowUp" || event.key === "ArrowDown") {
              event.preventDefault();
              resize(
                (body.current?.getBoundingClientRect().height ?? height) +
                  (event.key === "ArrowUp" ? 16 : -16),
              );
            }
          }}
        />
      )}
      <div className="flex items-center justify-between gap-2">
        <button
          type="button"
          aria-expanded={open}
          onClick={event => event.currentTarget.focus()}
          onKeyDown={event => {
            if (event.key === "Enter" || event.key === " ") {
              event.preventDefault();
              toggleOpen();
            }
          }}
          onDoubleClick={toggleOpen}
          className="flex min-w-0 flex-1 items-center gap-2 py-1 text-sm font-semibold text-muted-foreground focus:bg-primary/10 focus:text-primary"
        >
          <Tags className="size-4" />
          {t("central.tagFilter")}
        </button>
        {open && onClear && (
          <button
            type="button"
            aria-label={t("workflow.clearFilter")}
            title={t("workflow.clearFilter")}
            disabled={!hasSelection}
            onClick={onClear}
            className="flex size-6 shrink-0 items-center justify-center rounded text-muted-foreground hover:bg-muted hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring disabled:opacity-30 disabled:pointer-events-none"
          >
            <FilterX className="size-4" />
          </button>
        )}
      </div>
      {open && (
        <div
          ref={body}
          style={{ height, maxHeight: "60vh" }}
          className="overflow-y-auto py-2"
        >
          {children}
        </div>
      )}
    </section>
  );
  return target ? createPortal(content, target) : content;
}
