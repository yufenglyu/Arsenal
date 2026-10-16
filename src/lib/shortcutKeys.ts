export const SHORTCUT_STORAGE_KEY = "skills-manage.shortcuts.v1";

export type ShortcutActionId =
  | "saveNotebook"
  | "expandSection"
  | "collapseSection"
  | "focusPageSearch"
  | "globalSearch"
  | "toggleSidebar"
  | "toggleSkillViewMode"
  | "refreshSkillList"
  | "goResources"
  | "goCollections"
  | "goCentral"
  | "goSettings";

export interface ShortcutDefinition {
  id: ShortcutActionId;
  defaultCombo: string;
  labelKey: string;
  descriptionKey: string;
}

export const SHORTCUT_DEFINITIONS: ShortcutDefinition[] = [
  { id: "saveNotebook", defaultCombo: "mod+s", labelKey: "g.save", descriptionKey: "g.notebook" },
  { id: "focusPageSearch", defaultCombo: "mod+f", labelKey: "settings.shortcuts.focusPageSearch", descriptionKey: "settings.shortcuts.focusPageSearchDesc" },
  { id: "expandSection", defaultCombo: "plus", labelKey: "settings.shortcuts.expandSection", descriptionKey: "settings.shortcuts.expandSectionDesc" },
  { id: "collapseSection", defaultCombo: "-", labelKey: "settings.shortcuts.collapseSection", descriptionKey: "settings.shortcuts.collapseSectionDesc" },
  {
    id: "globalSearch",
    defaultCombo: "mod+k",
    labelKey: "settings.shortcuts.globalSearch",
    descriptionKey: "settings.shortcuts.globalSearchDesc",
  },
  {
    id: "toggleSidebar",
    defaultCombo: "mod+b",
    labelKey: "settings.shortcuts.toggleSidebar",
    descriptionKey: "settings.shortcuts.toggleSidebarDesc",
  },
  {
    id: "toggleSkillViewMode",
    defaultCombo: "f3",
    labelKey: "settings.shortcuts.toggleSkillViewMode",
    descriptionKey: "settings.shortcuts.toggleSkillViewModeDesc",
  },
  {
    id: "refreshSkillList",
    defaultCombo: "f5",
    labelKey: "settings.shortcuts.refreshSkillList",
    descriptionKey: "settings.shortcuts.refreshSkillListDesc",
  },
  {
    id: "goResources",
    defaultCombo: "alt+1",
    labelKey: "settings.shortcuts.goResources",
    descriptionKey: "settings.shortcuts.goResourcesDesc",
  },
  {
    id: "goCollections",
    defaultCombo: "alt+2",
    labelKey: "settings.shortcuts.goCollections",
    descriptionKey: "settings.shortcuts.goCollectionsDesc",
  },
  {
    id: "goCentral",
    defaultCombo: "alt+3",
    labelKey: "settings.shortcuts.goCentral",
    descriptionKey: "settings.shortcuts.goCentralDesc",
  },
  {
    id: "goSettings",
    defaultCombo: "alt+,",
    labelKey: "settings.shortcuts.goSettings",
    descriptionKey: "settings.shortcuts.goSettingsDesc",
  },
];

export const DEFAULT_SHORTCUTS: Record<ShortcutActionId, string> =
  Object.fromEntries(
    SHORTCUT_DEFINITIONS.map((definition) => [
      definition.id,
      definition.defaultCombo,
    ])
  ) as Record<ShortcutActionId, string>;

const MODIFIER_KEYS = new Set(["mod", "ctrl", "meta", "shift", "alt"]);

function normalizeComboPart(part: string) {
  const trimmed = part.trim().toLowerCase();
  if (trimmed === "cmd" || trimmed === "command") return "meta";
  if (trimmed === "control") return "ctrl";
  if (trimmed === "option") return "alt";
  return trimmed;
}

function normalizeMainKey(key: string) {
  const trimmed = key.trim().toLowerCase();
  if (trimmed === "+") return "plus";
  if (trimmed === " ") return "space";
  if (trimmed === "esc") return "escape";
  if (trimmed === "return") return "enter";
  if (trimmed === "arrowup") return "up";
  if (trimmed === "arrowdown") return "down";
  if (trimmed === "arrowleft") return "left";
  if (trimmed === "arrowright") return "right";
  return trimmed;
}

export function normalizeShortcutCombo(combo: string) {
  if (combo.trim() === "+") return "plus";
  const parts = combo
    .split("+")
    .map(normalizeComboPart)
    .filter(Boolean);
  const modifiers = parts.filter((part) => MODIFIER_KEYS.has(part));
  const mainKey = parts.find((part) => !MODIFIER_KEYS.has(part));
  if (!mainKey) return "";

  const orderedModifiers = [
    modifiers.includes("mod") ? "mod" : null,
    modifiers.includes("ctrl") ? "ctrl" : null,
    modifiers.includes("meta") ? "meta" : null,
    modifiers.includes("alt") ? "alt" : null,
    modifiers.includes("shift") ? "shift" : null,
  ].filter((part): part is string => !!part);
  return [...new Set(orderedModifiers), normalizeMainKey(mainKey)].join("+");
}

export function shortcutEventToCombo(event: KeyboardEvent) {
  const key = normalizeMainKey(event.key);
  if (!key || ["control", "meta", "shift", "alt"].includes(key)) return "";
  const modifiers = [
    event.ctrlKey ? "ctrl" : null,
    event.metaKey ? "meta" : null,
    event.altKey ? "alt" : null,
    event.shiftKey ? "shift" : null,
  ].filter((part): part is string => !!part);
  return normalizeShortcutCombo([...modifiers, key].join("+"));
}

export function formatShortcutCombo(combo: string) {
  const normalized = normalizeShortcutCombo(combo);
  if (!normalized) return "";
  const isMac =
    typeof navigator !== "undefined" &&
    navigator.platform.toUpperCase().includes("MAC");
  return normalized
    .split("+")
    .map((part) => {
      if (part === "mod") return isMac ? "⌘" : "Ctrl";
      if (part === "ctrl") return "Ctrl";
      if (part === "meta") return isMac ? "⌘" : "Meta";
      if (part === "alt") return isMac ? "⌥" : "Alt";
      if (part === "shift") return isMac ? "⇧" : "Shift";
      if (part === "plus") return "+";
      if (part === "space") return "Space";
      if (part.length === 1) return part.toUpperCase();
      return part.charAt(0).toUpperCase() + part.slice(1);
    })
    .join(isMac ? "" : "+");
}

function targetIsEditable(target: EventTarget | null) {
  if (!(target instanceof HTMLElement)) return false;
  const tagName = target.tagName.toLowerCase();
  return (
    tagName === "input" ||
    tagName === "textarea" ||
    tagName === "select" ||
    target.isContentEditable
  );
}

export function matchesShortcutEvent(event: KeyboardEvent, combo: string) {
  const normalized = normalizeShortcutCombo(combo);
  if (!normalized) return false;
  const parts = normalized.split("+");
  const mainKey = parts.find((part) => !MODIFIER_KEYS.has(part));
  if (!mainKey) return false;

  const isMac =
    typeof navigator !== "undefined" &&
    navigator.platform.toUpperCase().includes("MAC");
  const needsMod = parts.includes("mod");
  const ctrlExpected = parts.includes("ctrl") || (needsMod && !isMac);
  const metaExpected = parts.includes("meta") || (needsMod && isMac);

  return (
    event.ctrlKey === ctrlExpected &&
    event.metaKey === metaExpected &&
    event.altKey === parts.includes("alt") &&
    (mainKey === "plus" || event.shiftKey === parts.includes("shift")) &&
    normalizeMainKey(event.key) === mainKey
  );
}

export function shouldIgnoreShortcutTarget(target: EventTarget | null) {
  return targetIsEditable(target);
}
