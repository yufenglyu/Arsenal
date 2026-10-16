import {beforeEach, expect, it, vi} from "vitest";
import {fireEvent, render, screen, waitFor} from "@testing-library/react";
import {FolderNotesEditor} from "@/components/skill/FolderNotesEditor";
import {useRepositoryDocumentStore as documents} from "@/stores/repositoryDocumentStore";
import {useMetadataStore as metadata} from "@/stores/metadataStore";
import {registerTaskExecutor, useTaskQueueStore} from "@/stores/taskQueueStore";

beforeEach(() => {vi.restoreAllMocks();metadata.setState({folders:[],folderError:false});useTaskQueueStore.setState({tasks:[]});});

it("generates from the preferred README and falls back to the default document", async () => {
  const generate=vi.fn().mockResolvedValue("Repository note");
  registerTaskExecutor("generate_repository_note",generate);
  vi.spyOn(documents.getState(),"readDocuments").mockResolvedValue([{name:"README.md",language:"default",content:"Default text"},{name:"README_zh.md",language:"zh",content:"中文内容"}]);
  expect(await documents.getState().generateNote("/fixture/repo","skill","zh")).toBe("Repository note");
  expect(generate).toHaveBeenLastCalledWith({content:"中文内容"});
  await documents.getState().generateNote("/fixture/repo","skill","en");
  expect(generate).toHaveBeenLastCalledWith({content:"Default text"});
});

it("does not call AI when README files are missing or empty", async () => {
  const generate=vi.fn();registerTaskExecutor("generate_repository_note",generate);
  vi.spyOn(documents.getState(),"readDocuments").mockResolvedValue([{name:"README.md",language:"default",content:"  "}]);
  await expect(documents.getState().generateNote("/fixture/repo")).rejects.toThrow(/README/);
  expect(generate).not.toHaveBeenCalled();
});

it("fills an editable draft and saves only after Save is clicked", async () => {
  vi.spyOn(documents.getState(),"generateNote").mockResolvedValue("Generated repository note");
  const save=vi.spyOn(metadata.getState(),"saveFolder").mockResolvedValue();
  render(<FolderNotesEditor directory="/fixture/repo" skillIds={["skill"]}/>);
  fireEvent.click(screen.getByRole("button",{name:"基于 README 生成仓库备注"}));
  await waitFor(() => expect(screen.getByRole("textbox")).toHaveValue("Generated repository note"));
  expect(save).not.toHaveBeenCalled();
  fireEvent.click(screen.getByRole("button",{name:"保存"}));
  await waitFor(() => expect(save).toHaveBeenCalledWith(expect.objectContaining({notes:"Generated repository note",skillIds:["skill"]})));
});
