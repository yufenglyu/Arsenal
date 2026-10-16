import { ArrowLeft, Ban, Check, CircleCheck, Download, EyeOff, FileText, FolderOpen, History, Info, Languages, ListFilter, ListTodo, PackageMinus, PackagePlus, Pencil, Play, Plus, RefreshCw, RotateCcw, Save, ShieldAlert, Sparkles, Trash2, X } from "lucide-react";

// Shared symbols for actions across dialogs, toolbars and task controls.
const icons = {
  cancel: X, close: X, confirm: Check, save: Save, clear: Trash2, delete: Trash2,
  add: Plus, import: Download, install: PackagePlus, uninstall: PackageMinus,
  edit: Pencil, back: ArrowLeft, retry: RotateCcw, reset: RotateCcw,
  check: RefreshCw, update: RefreshCw, ignore: EyeOff, history: History,
  tasks: ListTodo, filter: ListFilter, error: ShieldAlert, success: CircleCheck,
  stop: Ban, active: Play, open: FolderOpen, language: Languages, ai: Sparkles, info: Info, document: FileText,
};
export type ActionIconName = keyof typeof icons;
export function ActionIcon({ action }: { action: ActionIconName }) {
  const Icon = icons[action];
  return <Icon className="size-[1em] shrink-0" aria-hidden="true" />;
}
