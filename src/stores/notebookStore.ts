import { openInFileManager } from "@/lib/openPath";
import { create } from "zustand";
import { invoke, listen, isTauriRuntime } from "@/lib/tauri";
import { open } from "@tauri-apps/plugin-dialog";
import { open as openUrl } from "@tauri-apps/plugin-shell";
import i18n from "@/i18n";
import type { NotebookFile, Repository } from "@/types/notebook";
import { useAppStatusStore } from "./appStatusStore";

function report(error: unknown) {
  const code = String(error);
  const message = i18n.t(`g.errors.${code}`, { defaultValue: i18n.t("g.obsidian.unavailable") });
  useAppStatusStore.getState().startTask({ id: "notebook", label: i18n.t("g.notebook") });
  useAppStatusStore.getState().failTask({ error: message });
}
async function task<T>(work: () => Promise<T>, quiet = false): Promise<T | null> {
  try { return await work(); } catch (error) { if (!quiet) report(error); return null; }
}
interface NotebookState {
  rows: Repository[];
  config: Record<string, string>;
  load: () => Promise<void>;
  loadConfig: () => Promise<void>;
  ensure: (directory: string, skillId: string) => Promise<Repository | null>;
  saveField: (id: number, field: "notebook", body: string, expectedBody: string) => Promise<boolean>;
  chooseNotebook: () => Promise<string | null>;
  inspectNotebook: (path: string) => Promise<NotebookFile | null>;
  previewNotebookProperties: (id: number) => Promise<string | null>;
  readNotebook: (id: number, quiet?: boolean) => Promise<NotebookFile | null>;
  bindNotebook: (id: number, path: string, create: boolean, expectedRevision: string | null, initialBody?: string) => Promise<NotebookFile | null>;
  saveNotebook: (id: number, body: string, expectedRevision: string, updateProperties?: boolean) => Promise<NotebookFile | null>;
  renameNotebook: (id: number, name: string, expectedRevision: string) => Promise<NotebookFile | null>;
  unlinkNotebook: (id: number, keepBody: boolean, expectedRevision: string | null) => Promise<boolean>;
  clearLegacyNotebook: (id: number, expectedLegacy: string) => Promise<boolean>;
  openObsidian: (id: number) => Promise<void>;
  copyNotebookLink: (id: number) => Promise<void>;
  notebookAsset: (id: number, link: string, open?: boolean) => Promise<string | null>;
  watchNotebook: (onChange: () => void) => Promise<() => void>;
  visit: (url: string) => Promise<void>;
  saveConfig: (vaultPath: string, folder: string, template: string) => Promise<boolean>;
  openVault: (path: string) => Promise<void>;
  chooseVault: () => Promise<string | null>;
  registerProtocol: () => Promise<void>;
  notebookNavigation: (navigate: (uid: string) => Promise<void>) => Promise<() => void>;
}
export const useNotebookStore = create<NotebookState>((set, get) => {
  async function mutate<T>(command: string, args: Record<string, unknown>) {
    const result = await task(() => invoke<T>(command, args));
    if (result !== null) await get().load();
    return result;
  }
  return {
    rows: [], config: {},
    load: async () => { if (!isTauriRuntime()) return; const rows = await task(() => invoke<Repository[]>("list_repository_notebooks")); if (rows) set({ rows }); },
    loadConfig: async () => { if (!isTauriRuntime()) return; const config = await task(() => invoke<Record<string, string>>("get_notebook_settings")); if (config) set({ config }); },
    ensure: async (directory, skillId) => {
      const row = await task(() => invoke<Repository>("ensure_repository_notebook", { directory, skillId }));
      if (row) set(s => ({ rows: [...s.rows.filter(r => r.id !== row.id), row] }));
      return row;
    },
    saveField: async (id, _field, body, expectedBody) => {
      return (await task(async () => { await invoke("save_internal_notebook", { id, body, expectedBody }); await get().load(); return true; })) === true;
    },
    chooseNotebook: async () => task(async () => { const path = await open({ multiple: false, filters: [{ name: "Markdown", extensions: ["md"] }] }); return typeof path === "string" ? path : null; }),
    openVault: async path => { await task(() => openInFileManager(path)); },
    chooseVault: async () => task(async () => { const path = await open({ directory: true, multiple: false }); return typeof path === "string" ? path : null; }),
    inspectNotebook: path => task(() => invoke<NotebookFile>("inspect_notebook", { path })),
    previewNotebookProperties: id => task(() => invoke<string>("preview_notebook_properties", { id })),
    readNotebook: (id, quiet = false) => task(() => invoke<NotebookFile>("read_notebook", { id }), quiet),
    bindNotebook: (id, path, create, expectedRevision, initialBody) => mutate<NotebookFile>("bind_notebook", { id, path, create, expectedRevision, initialBody: initialBody ?? null }),
    saveNotebook: (id, body, expectedRevision, updateProperties = false) => task(() => invoke<NotebookFile>("save_notebook", { id, body, expectedRevision, updateProperties })),
    renameNotebook: (id, name, expectedRevision) => mutate<NotebookFile>("rename_notebook", { id, name, expectedRevision }),
    unlinkNotebook: async (id, keepBody, expectedRevision) => (await task(async () => { await invoke("unlink_notebook", { id, keepBody, expectedRevision }); await get().load(); return true; })) === true,
    clearLegacyNotebook: async (id, expectedLegacy) => (await task(async () => { await invoke("clear_legacy_notebook", { id, expectedLegacy }); await get().load(); return true; })) === true,
    openObsidian: async id => { await task(() => invoke("open_obsidian", { id })); },
    copyNotebookLink: async id => { await task(async () => { const file = await get().readNotebook(id); if (file?.noteId) await navigator.clipboard.writeText(`obsidian://arsenal-note?id=${file.noteId}`); }); },
    notebookAsset: (id, link, open = false) => task(() => invoke<string>("notebook_asset", { id, link, open }), !open),
    watchNotebook: async onChange => {
      if (!isTauriRuntime()) return () => {};
      const stop = await task(() => listen("obsidian-changed", onChange));
      if (!stop) return () => {};
      await task(() => invoke("watch_notebook", { enabled: true }));
      return () => { stop(); void task(() => invoke("watch_notebook", { enabled: false }), true); };
    },
    visit: async url => { await task(() => openUrl(url)); },
    saveConfig: async (vaultPath, folder, template) => (await task(async () => { await invoke("save_notebook_settings", { vaultPath, folder, template }); await get().loadConfig(); return true; })) === true,
    registerProtocol: async () => { await task(() => invoke("register_arsenal_protocol")); },
    notebookNavigation: async navigate => {
      if (!isTauriRuntime()) return () => {};
      let active = true;
      const consume = async () => {
        const ids = await invoke<string[]>("take_notebook_navigation");
        if (active && ids?.length) await navigate(ids[ids.length - 1]);
      };
      const stop = await task(() => listen("notebook-navigation", () => { void task(consume); }));
      await task(consume);
      await task(async () => {
        const error = await invoke<string | null>("take_protocol_registration_error");
        if (active && error) report(error);
      });
      return () => { active = false; stop?.(); };
    },
  };
});
