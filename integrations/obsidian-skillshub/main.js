"use strict";
var __defProp = Object.defineProperty;
var __getOwnPropDesc = Object.getOwnPropertyDescriptor;
var __getOwnPropNames = Object.getOwnPropertyNames;
var __hasOwnProp = Object.prototype.hasOwnProperty;
var __export = (target, all) => {
  for (var name in all)
    __defProp(target, name, { get: all[name], enumerable: true });
};
var __copyProps = (to, from, except, desc) => {
  if (from && typeof from === "object" || typeof from === "function") {
    for (let key of __getOwnPropNames(from))
      if (!__hasOwnProp.call(to, key) && key !== except)
        __defProp(to, key, { get: () => from[key], enumerable: !(desc = __getOwnPropDesc(from, key)) || desc.enumerable });
  }
  return to;
};
var __toCommonJS = (mod) => __copyProps(__defProp({}, "__esModule", { value: true }), mod);

// main.ts
var main_exports = {};
__export(main_exports, {
  default: () => SkillsHubLink
});
module.exports = __toCommonJS(main_exports);
var import_obsidian = require("obsidian");
var UUID = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i;
var MAX_NOTE = 4 * 1024 * 1024;
function identity(value) {
  return typeof value === "string" && UUID.test(value) ? value.toLowerCase() : null;
}
function properties(text) {
  if (text.length > MAX_NOTE) return {};
  const info = (0, import_obsidian.getFrontMatterInfo)(text);
  if (!info.exists) return {};
  try {
    const data = (0, import_obsidian.parseYaml)(info.frontmatter);
    return data && typeof data === "object" && !Array.isArray(data) ? data : {};
  } catch {
    return {};
  }
}
var SkillsHubLink = class extends import_obsidian.Plugin {
  constructor() {
    super(...arguments);
    this.index = null;
    this.generation = 0;
  }
  onload() {
    this.ready = new Promise((resolve) => this.app.workspace.onLayoutReady(resolve));
    const invalidate = () => {
      this.index = null;
      this.generation++;
    };
    this.registerEvent(this.app.vault.on("create", invalidate));
    this.registerEvent(this.app.vault.on("modify", invalidate));
    this.registerEvent(this.app.vault.on("delete", invalidate));
    this.registerEvent(this.app.vault.on("rename", invalidate));
    this.registerObsidianProtocolHandler("skillshub-note", (params) => {
      void this.openNote(params.id).catch(() => new import_obsidian.Notice("SkillsHub: Unable to open the note / \u65E0\u6CD5\u6253\u5F00\u7B14\u8BB0\u3002"));
    });
    this.addCommand({
      id: "copy-stable-note-link",
      name: "Copy stable note link / \u590D\u5236\u7A33\u5B9A\u7B14\u8BB0\u94FE\u63A5",
      callback: () => {
        void this.copyLink().catch(() => new import_obsidian.Notice("SkillsHub: Unable to copy link / \u65E0\u6CD5\u590D\u5236\u94FE\u63A5\u3002"));
      }
    });
    this.addCommand({
      id: "return-to-skillshub",
      name: "Open repository in SkillsHub / \u8FD4\u56DE SkillsHub \u4ED3\u5E93",
      callback: () => {
        void this.returnToSkillsHub().catch(() => new import_obsidian.Notice("SkillsHub: Unable to read properties / \u65E0\u6CD5\u8BFB\u53D6\u5C5E\u6027\u3002"));
      }
    });
    this.addCommand({ id: "rebuild-note-index", name: "Rebuild note index / \u91CD\u5EFA\u7B14\u8BB0\u7D22\u5F15", callback: () => {
      invalidate();
      void this.buildIndex().then(() => new import_obsidian.Notice("SkillsHub: Index rebuilt / \u7D22\u5F15\u5DF2\u91CD\u5EFA\u3002")).catch(() => new import_obsidian.Notice("SkillsHub: Unable to index this vault / \u65E0\u6CD5\u4E3A\u6B64\u5E93\u5EFA\u7ACB\u7D22\u5F15\u3002"));
    } });
  }
  async buildIndex() {
    await this.ready;
    for (let attempt = 0; attempt < 3; attempt++) {
      const generation = this.generation;
      const files = this.app.vault.getMarkdownFiles();
      if (files.length > 2e4) throw new Error("Index limit");
      const index = /* @__PURE__ */ new Map();
      for (const file of files) {
        if (file.stat.size > MAX_NOTE) continue;
        const id = identity(properties(await this.app.vault.cachedRead(file)).skillshub_note_id);
        if (id) index.set(id, [...index.get(id) ?? [], file]);
      }
      if (generation === this.generation) {
        this.index = index;
        return index;
      }
    }
    throw new Error("Vault changed during indexing");
  }
  async openNote(value) {
    const id = identity(value);
    if (!id) {
      new import_obsidian.Notice("SkillsHub: Invalid note ID / \u7B14\u8BB0 ID \u65E0\u6548\u3002");
      return;
    }
    const index = this.index ?? await this.buildIndex();
    const files = index.get(id) ?? [];
    if (files.length !== 1) {
      new import_obsidian.Notice(files.length > 1 ? "SkillsHub: Duplicate note ID. Resolve the copied notes first / \u7B14\u8BB0 ID \u91CD\u590D\uFF0C\u8BF7\u5148\u5904\u7406\u526F\u672C\u3002" : "SkillsHub: Note not found. Open its vault and enable SkillsHub Link / \u672A\u627E\u5230\u7B14\u8BB0\uFF0C\u8BF7\u6253\u5F00\u6240\u5C5E\u5E93\u5E76\u542F\u7528 SkillsHub Link\u3002");
      return;
    }
    const file = files[0];
    if (this.app.vault.getAbstractFileByPath(file.path) !== file || identity(properties(await this.app.vault.cachedRead(file)).skillshub_note_id) !== id) {
      this.index = null;
      new import_obsidian.Notice("SkillsHub: Note changed. Try the link again / \u7B14\u8BB0\u5DF2\u53D8\u5316\uFF0C\u8BF7\u91CD\u8BD5\u94FE\u63A5\u3002");
      return;
    }
    await this.app.workspace.getLeaf(false).openFile(file);
  }
  async activeProperties() {
    const file = this.app.workspace.getActiveFile();
    if (!file || file.extension !== "md") {
      new import_obsidian.Notice("SkillsHub: Open a linked Markdown note first / \u8BF7\u5148\u6253\u5F00\u5DF2\u5173\u8054\u7B14\u8BB0\u3002");
      return null;
    }
    return properties(await this.app.vault.cachedRead(file));
  }
  async copyLink() {
    const data = await this.activeProperties();
    if (!data) return;
    const id = identity(data.skillshub_note_id);
    if (!id) {
      new import_obsidian.Notice("SkillsHub: Open this note in SkillsHub to initialize its ID / \u8BF7\u5148\u5728 SkillsHub \u4E2D\u6253\u5F00\u7B14\u8BB0\u4EE5\u521D\u59CB\u5316 ID\u3002");
      return;
    }
    await navigator.clipboard.writeText(`obsidian://skillshub-note?id=${id}`);
    new import_obsidian.Notice("SkillsHub: Stable link copied / \u7A33\u5B9A\u94FE\u63A5\u5DF2\u590D\u5236\u3002");
  }
  async returnToSkillsHub() {
    const data = await this.activeProperties();
    if (!data) return;
    const id = identity(data.skillshub_repository_id);
    if (!id) {
      new import_obsidian.Notice("SkillsHub: Repository ID missing / \u7F3A\u5C11\u4ED3\u5E93 ID\u3002");
      return;
    }
    window.open(`skillshub://repository/${id}?tab=notebook`);
  }
};
