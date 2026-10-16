export interface ObsidianBinding {
  bindingVersion: number;
  noteId?: string;
  vaultKey: string;
  relativePath: string;
  obsidianUri: string;
}
export interface NotebookFile {
  noteId?: string;
  body: string;
  revision: string;
  properties: string;
  relativePath: string;
}
export interface Repository {
  id: number;
  sourceKey: string;
  repositoryUid: string;
  fullName: string;
  url: string;
  description: string;
  tags: string[];
  stars: number | null;
  skillIds: string[];
  notebook: string;
  obsidian: ObsidianBinding | null;
}
