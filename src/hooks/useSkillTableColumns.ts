import { useCallback, useEffect, useState } from "react";

export type SkillTableKind = "skill" | "folder" | "tree" | "flat";

export const FIXED_SKILL_COLUMNS = ["index", "name"] as const;
export const DEFAULT_SKILL_COLUMNS = [
  "index",
  "name",
  "createdAt",
  "updatedAt",
  "installSummary",
] as const;
export const DEFAULT_FOLDER_COLUMNS = [
  "index",
  "name",
  "skillCount",
  "notebook",
  "createdAt",
  "updatedAt",
  "installSummary",
] as const;

const REMOVED_COLUMNS = new Set(["actions", "source", "notes", "tags"]);
const FIXED_COLUMNS = new Set<string>(FIXED_SKILL_COLUMNS);
const CHANGE_EVENT = "skills-manage:skill-table-columns";
const STORAGE_VERSION = "v5";

function storageKey(kind: SkillTableKind) {
  return `skills-manage.skillTableColumns.${kind}.${STORAGE_VERSION}`;
}

function defaultsFor(kind: SkillTableKind) {
  if (kind === "flat") return ["index", "name", "repository", "createdAt", "updatedAt"];
  return kind === "tree" ? ["index", "name", "skillCount", "notebook", "githubStars", "createdAt", "updatedAt"] : kind === "skill" ? DEFAULT_SKILL_COLUMNS : DEFAULT_FOLDER_COLUMNS;
}

function readColumns(kind: SkillTableKind) {
  if (typeof window === "undefined") return new Set<string>(defaultsFor(kind));
  const raw = window.localStorage.getItem(storageKey(kind));
  if (!raw) return new Set<string>(defaultsFor(kind));

  try {
    const parsed = JSON.parse(raw);
    if (!Array.isArray(parsed)) return new Set<string>(defaultsFor(kind));
    if (kind === "tree" || kind === "folder") {
      const migration = `${storageKey(kind)}.notebook`;
      if (!window.localStorage.getItem(migration)) {
        if (!parsed.includes("notebook")) parsed.push("notebook");
        window.localStorage.setItem(storageKey(kind), JSON.stringify(parsed));
        window.localStorage.setItem(migration, "1");
      }
    }
    return new Set<string>([...parsed.map(String).filter((column: string) => !REMOVED_COLUMNS.has(column)), ...FIXED_COLUMNS]);
  } catch {
    return new Set<string>(defaultsFor(kind));
  }
}

export function useSkillTableColumns(kind: SkillTableKind) {
  const [visibleColumns, setVisibleColumns] = useState(() => readColumns(kind));
  const [previousKind, setPreviousKind] = useState(kind);
  // Switch columns in the same render as the view, before layout and scroll restoration.
  if (previousKind !== kind) {
    setPreviousKind(kind);
    setVisibleColumns(readColumns(kind));
  }

  const persist = useCallback(
    (next: Set<string>) => {
      try {
        window.localStorage.setItem(storageKey(kind), JSON.stringify([...next]));
        if (kind === "tree" || kind === "folder") window.localStorage.setItem(`${storageKey(kind)}.notebook`, "1");
      } catch {
        // Keep in-memory columns if localStorage is unavailable.
      }
      window.dispatchEvent(
        new CustomEvent(CHANGE_EVENT, { detail: { kind, columns: [...next] } })
      );
    },
    [kind]
  );

  const toggleColumn = useCallback(
    (key: string) => {
      if (FIXED_COLUMNS.has(key) || REMOVED_COLUMNS.has(key)) return;
      setVisibleColumns((previous) => {
        const next = new Set(previous);
        if (next.has(key)) {
          next.delete(key);
        } else {
          next.add(key);
        }
        for (const fixed of FIXED_COLUMNS) {
          next.add(fixed);
        }
        persist(next);
        return next;
      });
    },
    [persist]
  );

  const resetColumns = useCallback(() => {
    const next = new Set<string>(defaultsFor(kind));
    setVisibleColumns(next);
    persist(next);
  }, [kind, persist]);

  useEffect(() => {
    function handleChange(event: Event) {
      if (!(event instanceof CustomEvent)) return;
      if (event.detail?.kind !== kind) return;
      const columns = Array.isArray(event.detail.columns) ? event.detail.columns : [];
      setVisibleColumns(new Set<string>([...columns.map(String).filter((column: string) => !REMOVED_COLUMNS.has(column)), ...FIXED_COLUMNS]));
    }

    function handleStorage(event: StorageEvent) {
      if (event.key === storageKey(kind)) {
        setVisibleColumns(readColumns(kind));
      }
    }

    window.addEventListener(CHANGE_EVENT, handleChange);
    window.addEventListener("storage", handleStorage);
    return () => {
      window.removeEventListener(CHANGE_EVENT, handleChange);
      window.removeEventListener("storage", handleStorage);
    };
  }, [kind]);

  return { visibleColumns, toggleColumn, resetColumns };
}
