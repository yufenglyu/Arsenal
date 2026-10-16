import { Notice, Plugin, TFile, getFrontMatterInfo, parseYaml } from "obsidian";

const UUID = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i;
const MAX_NOTE = 4 * 1024 * 1024;

function identity(value: unknown): string | null {
  return typeof value === "string" && UUID.test(value) ? value.toLowerCase() : null;
}

function properties(text: string): Record<string, unknown> {
  if (text.length > MAX_NOTE) return {};
  const info = getFrontMatterInfo(text);
  if (!info.exists) return {};
  try {
    const data: unknown = parseYaml(info.frontmatter);
    return data && typeof data === "object" && !Array.isArray(data) ? data as Record<string, unknown> : {};
  } catch { return {}; }
}

/** Read-only integration: file names are never used as identity or modified by this plugin. */
export default class SkillsHubLink extends Plugin {
  private index: Map<string, TFile[]> | null = null;
  private generation = 0;
  private ready!: Promise<void>;

  onload(): void {
    this.ready = new Promise(resolve => this.app.workspace.onLayoutReady(resolve));
    const invalidate = () => { this.index = null; this.generation++; };
    this.registerEvent(this.app.vault.on("create", invalidate));
    this.registerEvent(this.app.vault.on("modify", invalidate));
    this.registerEvent(this.app.vault.on("delete", invalidate));
    this.registerEvent(this.app.vault.on("rename", invalidate));
    this.registerObsidianProtocolHandler("skillshub-note", params => {
      void this.openNote(params.id).catch(() => new Notice("SkillsHub: Unable to open the note / 无法打开笔记。"));
    });
    this.addCommand({
      id: "copy-stable-note-link", name: "Copy stable note link / 复制稳定笔记链接",
      callback: () => { void this.copyLink().catch(() => new Notice("SkillsHub: Unable to copy link / 无法复制链接。")); },
    });
    this.addCommand({
      id: "return-to-skillshub", name: "Open repository in SkillsHub / 返回 SkillsHub 仓库",
      callback: () => { void this.returnToSkillsHub().catch(() => new Notice("SkillsHub: Unable to read properties / 无法读取属性。")); },
    });
    this.addCommand({ id: "rebuild-note-index", name: "Rebuild note index / 重建笔记索引", callback: () => {
      invalidate();
      void this.buildIndex().then(() => new Notice("SkillsHub: Index rebuilt / 索引已重建。"))
        .catch(() => new Notice("SkillsHub: Unable to index this vault / 无法为此库建立索引。"));
    } });
  }

  private async buildIndex(): Promise<Map<string, TFile[]>> {
    await this.ready;
    for (let attempt = 0; attempt < 3; attempt++) {
      const generation = this.generation;
      const files = this.app.vault.getMarkdownFiles();
      if (files.length > 20000) throw new Error("Index limit");
      const index = new Map<string, TFile[]>();
      for (const file of files) {
        if (file.stat.size > MAX_NOTE) continue;
        // Read file front matter rather than trusting an asynchronously updated metadata cache.
        const id = identity(properties(await this.app.vault.cachedRead(file)).skillshub_note_id);
        if (id) index.set(id, [...(index.get(id) ?? []), file]);
      }
      if (generation === this.generation) { this.index = index; return index; }
    }
    throw new Error("Vault changed during indexing");
  }

  private async openNote(value: unknown): Promise<void> {
    const id = identity(value);
    if (!id) { new Notice("SkillsHub: Invalid note ID / 笔记 ID 无效。"); return; }
    const index = this.index ?? await this.buildIndex();
    const files = index.get(id) ?? [];
    if (files.length !== 1) {
      new Notice(files.length > 1
        ? "SkillsHub: Duplicate note ID. Resolve the copied notes first / 笔记 ID 重复，请先处理副本。"
        : "SkillsHub: Note not found. Open its vault and enable SkillsHub Link / 未找到笔记，请打开所属库并启用 SkillsHub Link。");
      return;
    }
    const file = files[0];
    if (this.app.vault.getAbstractFileByPath(file.path) !== file ||
        identity(properties(await this.app.vault.cachedRead(file)).skillshub_note_id) !== id) {
      this.index = null;
      new Notice("SkillsHub: Note changed. Try the link again / 笔记已变化，请重试链接。");
      return;
    }
    await this.app.workspace.getLeaf(false).openFile(file);
  }

  private async activeProperties(): Promise<Record<string, unknown> | null> {
    const file = this.app.workspace.getActiveFile();
    if (!file || file.extension !== "md") { new Notice("SkillsHub: Open a linked Markdown note first / 请先打开已关联笔记。"); return null; }
    return properties(await this.app.vault.cachedRead(file));
  }

  private async copyLink(): Promise<void> {
    const data = await this.activeProperties();
    if (!data) return;
    const id = identity(data.skillshub_note_id);
    if (!id) { new Notice("SkillsHub: Open this note in SkillsHub to initialize its ID / 请先在 SkillsHub 中打开笔记以初始化 ID。"); return; }
    await navigator.clipboard.writeText(`obsidian://skillshub-note?id=${id}`);
    new Notice("SkillsHub: Stable link copied / 稳定链接已复制。");
  }

  private async returnToSkillsHub(): Promise<void> {
    const data = await this.activeProperties();
    if (!data) return;
    const id = identity(data.skillshub_repository_id);
    if (!id) { new Notice("SkillsHub: Repository ID missing / 缺少仓库 ID。"); return; }
    // Construct a known navigation URI; never execute arbitrary front matter URLs.
    window.open(`skillshub://repository/${id}?tab=notebook`);
  }
}
