import { useRepositoryDocumentStore as documents } from "@/stores/repositoryDocumentStore";
import { beforeEach, expect, it, vi } from "vitest";
const { invoke } = vi.hoisted(() => ({invoke: vi.fn()}));
vi.mock("@/lib/tauri", () => ({invoke, isTauriRuntime: () => true}));
import { useRepositorySyncStore as sync } from "@/stores/repositorySyncStore";
import { useResourceLibraryStore as library } from "@/stores/resourceLibraryStore";
import { useTaskQueueStore as queue } from "@/stores/taskQueueStore";
import type { SkillWithLinks } from "@/types";
const skill = (id: string, repo: string): SkillWithLinks => ({id, name:id, source_repo:repo, github_stars:7, file_path:"/skills/SKILL.md", is_central:false, scanned_at:"", linked_agents:[], read_only_agents:[]});
beforeEach(() => {
  vi.restoreAllMocks(); invoke.mockReset(); queue.setState({tasks:[]});
  sync.setState({isRefreshingRepositories:false, reportCheckedAt:123, preview:{repositories:[]}});
  library.setState({skills:[skill("a","Owner/Repo"),skill("b","owner/repo"),skill("c","other/repo")],error:null});
});
it("updates unique selected repositories without scanning skills or replacing update results", async () => {
  invoke.mockResolvedValue(0);
  const scan=vi.spyOn(library.getState(),"loadResourceLibrary");
  const revision=documents.getState().revision;
  await sync.getState().refreshRepositories(["owner/repo"]);
  expect(invoke).toHaveBeenCalledExactlyOnceWith("refresh_repository", {repository:"owner/repo"});
  expect(scan).not.toHaveBeenCalled();
  expect(documents.getState().revision).toBe(revision+1);
  expect(library.getState().skills.map(item=>item.github_stars)).toEqual([0,0,7]);
  expect(sync.getState().preview).toEqual({repositories:[]});
  expect(sync.getState().reportCheckedAt).toBe(123);
  expect(sync.getState().isRefreshingRepositories).toBe(false);
});
it("retains failed star counts and reports the failure in background tasks", async () => {
  invoke.mockRejectedValue(new Error("network unavailable"));
  await sync.getState().refreshRepositories(["owner/repo"]);
  expect(library.getState().skills.map(item=>item.github_stars)).toEqual([7,7,7]);
  expect(queue.getState().tasks[0].status).toBe("failed");
  expect(sync.getState().isRefreshingRepositories).toBe(false);
});
