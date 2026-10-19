import { beforeEach, describe, expect, it, vi } from "vitest";
import { useRepositorySyncStore } from "@/stores/repositorySyncStore";
import { useResourceLibraryStore } from "@/stores/resourceLibraryStore";
import {useTaskQueueStore as queue, waitForTask, type BackgroundTask} from "@/stores/taskQueueStore";
import type { RepositorySyncPreviewReport } from "@/types";

describe("background repository preview", () => {
  it("clears check results and records while preserving ignored versions and unrelated tasks", () => {
    const check:BackgroundTask = {id:"check",key:"check",kind:"check",label:"Check",status:"success",createdAt:1,cancelRequested:false,locks:[],steps:[{command:"preview_source_backed_resource_repository_updates",label:"Check",args:{}}]};
    const importTask:BackgroundTask = {...check,id:"import",key:"import",kind:"import",status:"running",steps:[{command:"import_github_repo_skills",label:"Import",args:{}}]};
    queue.setState({tasks:[check,importTask]});
    useRepositorySyncStore.setState({preview:{repositories:[]},reportCheckedAt:1,checkedAt:{"example/repo":1},ignored:["keep-version"],reportGeneration:"old"});
    useRepositorySyncStore.getState().clearCheckResults();
    expect(useRepositorySyncStore.getState()).toMatchObject({preview:null,reportCheckedAt:null,checkedAt:{},ignored:["keep-version"]});
    expect(useRepositorySyncStore.getState().reportGeneration).not.toBe("old");
    expect(queue.getState().tasks).toEqual([importTask]);
    expect(importTask.cancelRequested).toBe(false);
  });
  it("does not clear a report while a repository check is running", () => {
    queue.setState({tasks:[]});
    useRepositorySyncStore.setState({preview:{repositories:[]},isChecking:true,reportGeneration:"active"});
    useRepositorySyncStore.getState().clearCheckResults();
    expect(useRepositorySyncStore.getState().preview).not.toBeNull();
    expect(useRepositorySyncStore.getState().reportGeneration).toBe("active");
  });
  it("keeps active child updates and clears only this report's finished children", () => {
    const child:BackgroundTask = {id:"child",key:"child",kind:"update",label:"Child",status:"running",createdAt:1,cancelRequested:false,locks:[],parentReportGeneration:"current",steps:[{command:"apply_repository_update_item",label:"Apply",args:{}}]};
    const unrelated = {...child,id:"unrelated",parentReportGeneration:undefined,status:"success" as const};
    queue.setState({tasks:[child,unrelated]});
    useRepositorySyncStore.setState({preview:{repositories:[]},reportGeneration:"current",isChecking:false,checkingRepository:null});
    useRepositorySyncStore.getState().clearCheckResults();
    expect(useRepositorySyncStore.getState().preview).not.toBeNull();
    queue.setState({tasks:[{...child,status:"failed"},unrelated]});
    useRepositorySyncStore.getState().clearCheckResults();
    expect(useRepositorySyncStore.getState().preview).toBeNull();
    expect(queue.getState().tasks).toEqual([unrelated]);
  });
  it("scans before a full check and preserves its report time on a repository retry", async () => {
    const order:string[]=[];
    useResourceLibraryStore.setState({error:null});
    vi.spyOn(useResourceLibraryStore.getState(),"loadResourceLibrary").mockImplementation(async()=>{order.push("scan");});
    vi.spyOn(useResourceLibraryStore.getState(),"previewRepositorySync").mockImplementation(async()=>{order.push("check");return {repositories:[{repository:"example/repo",added:[],modified:[],deleted:[],unchanged:[]}]};});
    await useRepositorySyncStore.getState().checkForUpdates();
    expect(order).toEqual(["scan","check"]);
    expect(useRepositorySyncStore.getState().reportCheckedAt).toEqual(expect.any(Number));
    useRepositorySyncStore.setState({reportCheckedAt:1000});
    await useRepositorySyncStore.getState().recheckRepository("example/repo");
    expect(queue.getState().tasks.at(-1)?.parentReportGeneration).toBe(useRepositorySyncStore.getState().reportGeneration);
    expect(useRepositorySyncStore.getState().reportCheckedAt).toBe(1000);
    expect(useRepositorySyncStore.getState().checkedAt["example/repo"]).toBeGreaterThan(1000);
    expect(order).toEqual(["scan","check","check"]);
  });
  beforeEach(() => {
    vi.restoreAllMocks();
    vi.mocked((window as unknown as {__TAURI_INTERNALS__: {invoke: ReturnType<typeof vi.fn>}}).__TAURI_INTERNALS__.invoke).mockResolvedValue([]);
    localStorage.clear();
    useRepositorySyncStore.setState({ preview: null, reportGeneration:null, open: false, isChecking: false, error: null,
      applied: false, appliedRepositories: [], checkingRepository: null, repositories: null, requestedRepositories: null, includeAdded: true, removeDeleted: false });
  });

  it.each([undefined, []])("clears previous reports and check history as soon as a full check starts (%s)", async (scope) => {
    const old = {repository:"old/repo",added:[{skillId:"old",name:"Old"}],modified:[],deleted:[],unchanged:[]};
    const oldTask:BackgroundTask = {id:"old-check",key:"old-check",kind:"check",label:"Old check",status:"failed",createdAt:1,cancelRequested:false,locks:[],steps:[{label:"check",command:"preview_source_backed_resource_repository_updates",args:{}}]};
    queue.setState({tasks:[oldTask,{...oldTask,id:"ai",key:"ai",kind:"ai",steps:[{label:"AI",command:"explain_skill",args:{}}]}]});
    useRepositorySyncStore.setState({preview:{repositories:[old]},reportCheckedAt:1,checkedAt:{"old/repo":1},updateErrors:{old:"failed"},ignored:["keep-version"],applied:true,appliedRepositories:["old/repo"]});
    let finishScan!: () => void;
    vi.spyOn(useResourceLibraryStore.getState(),"loadResourceLibrary").mockImplementation(() => new Promise(resolve => {finishScan = resolve;}));
    useResourceLibraryStore.setState({error:null});
    vi.spyOn(useResourceLibraryStore.getState(),"previewRepositorySync").mockRejectedValue(new Error("network unavailable"));
    const request = useRepositorySyncStore.getState().checkForUpdates(scope);
    expect(useRepositorySyncStore.getState()).toMatchObject({preview:null,reportCheckedAt:null,checkedAt:{},updateErrors:{},applied:false,appliedRepositories:[],ignored:["keep-version"],isChecking:true});
    expect(queue.getState().tasks.map(task => task.id)).toEqual(["ai"]);
    finishScan(); await request;
    expect(useRepositorySyncStore.getState()).toMatchObject({preview:null,isChecking:false,error:expect.any(String)});
  });

  it("does not restore a superseded running check when its result arrives late", async () => {
    const oldReport = {repositories:[{repository:"old/repo",added:[],modified:[],deleted:[],unchanged:[]}]};
    const newReport = {repositories:[{repository:"new/repo",added:[],modified:[],deleted:[],unchanged:[]}]};
    useRepositorySyncStore.setState({reportGeneration:"old",preview:oldReport});
    useResourceLibraryStore.setState({error:null});
    vi.spyOn(useResourceLibraryStore.getState(),"loadResourceLibrary").mockResolvedValue();
    let finishOld!: (report:RepositorySyncPreviewReport) => void;
    vi.spyOn(useResourceLibraryStore.getState(),"previewRepositorySync").mockImplementationOnce(() => new Promise(resolve => {finishOld = resolve;})).mockResolvedValueOnce(newReport);
    const id = queue.getState().enqueue({key:"old-check",kind:"check",label:"Old check",steps:[{command:"preview_source_backed_resource_repository_updates",label:"check",args:{repositories:null,reportGeneration:"old"}}]});
    const oldFinished = waitForTask(id);
    await useRepositorySyncStore.getState().checkForUpdates();
    expect(useRepositorySyncStore.getState().preview).toEqual(newReport);
    expect(queue.getState().tasks.find(task => task.id === id)?.clearRequested).toBe(true);
    finishOld(oldReport); await oldFinished;
    expect(useRepositorySyncStore.getState().preview).toEqual(newReport);
    expect(queue.getState().tasks.some(task => task.id === id)).toBe(false);
  });

  it("continues without a mounted page and coalesces repeat clicks", async () => {
    let finish!: (report: RepositorySyncPreviewReport) => void;
    const check = vi.spyOn(useResourceLibraryStore.getState(), "previewRepositorySync")
      .mockImplementation(() => new Promise((resolve) => { finish = resolve; }));
    const apply = vi.spyOn(useResourceLibraryStore.getState(), "syncSourceBackedSkills");
    const request = useRepositorySyncStore.getState().checkForUpdates(["example/skills"]);
    useRepositorySyncStore.getState().setOpen(false);
    expect(useRepositorySyncStore.getState().isChecking).toBe(true);
    expect(useRepositorySyncStore.getState().checkForUpdates()).toBe(request);
    finish({ repositories: [] });
    await request;
    expect(check).toHaveBeenCalledTimes(1);
    expect(apply).not.toHaveBeenCalled();
    expect(useRepositorySyncStore.getState()).toMatchObject({
      isChecking: false, open: false, preview: { repositories: [] }, repositories: ["example/skills"],
    });
  });

  it("retains failures for status-bar retry and accepts a later successful check", async () => {
    const check = vi.spyOn(useResourceLibraryStore.getState(), "previewRepositorySync")
      .mockRejectedValueOnce(new Error("network unavailable"))
      .mockResolvedValueOnce({ repositories: [] });
    await useRepositorySyncStore.getState().checkForUpdates();
    expect(useRepositorySyncStore.getState()).toMatchObject({ isChecking: false, error: expect.any(String) });
    await useRepositorySyncStore.getState().checkForUpdates();
    expect(check).toHaveBeenCalledTimes(2);
    expect(useRepositorySyncStore.getState()).toMatchObject({ error: null, open: false });
  });

  it("restores the report and choices without reopening a dialog or resuming a spinner", async () => {
    useRepositorySyncStore.setState({ preview: { repositories: [] }, repositories: ["example/skills"],
      includeAdded: false, removeDeleted: true, applied: true, open: true, isChecking: true });
    const key = "skillshub.repository-update-preview.v1";
    const saved = localStorage.getItem(key)!;
    expect(JSON.parse(saved).state).not.toHaveProperty("open");
    expect(JSON.parse(saved).state).not.toHaveProperty("isChecking");
    useRepositorySyncStore.setState({ preview: null, repositories: null, includeAdded: true,
      removeDeleted: false, applied: false, open: false, isChecking: false });
    localStorage.setItem(key, saved);
    await useRepositorySyncStore.persist.rehydrate();
    expect(useRepositorySyncStore.getState()).toMatchObject({ preview: { repositories: [] },
      repositories: ["example/skills"], includeAdded: false, removeDeleted: true, applied: true,
      open: false, isChecking: false });
  });

  it("keeps the saved report on failure and replaces its scope and choices only on success", async () => {
    useRepositorySyncStore.setState({ preview: { repositories: [] }, repositories: ["old/skills"],
      includeAdded: false, removeDeleted: true, applied: true });
    vi.spyOn(useResourceLibraryStore.getState(), "previewRepositorySync")
      .mockRejectedValueOnce(new Error("offline"))
      .mockResolvedValueOnce({ repositories: [] });
    await useRepositorySyncStore.getState().checkForUpdates(["new/skills"]);
    expect(useRepositorySyncStore.getState()).toMatchObject({ preview: { repositories: [] },
      repositories: ["old/skills"], requestedRepositories: ["new/skills"], applied: true });
    await useRepositorySyncStore.getState().checkForUpdates(["new/skills"]);
    expect(useRepositorySyncStore.getState()).toMatchObject({ repositories: ["new/skills"],
      includeAdded: true, removeDeleted: false, applied: false });
  });
  it("rechecks one repository without losing other results or options", async () => {
    const first = {repository:"one/repo",added:[],modified:[],deleted:[],unchanged:[]};
    const second = {...first,repository:"two/repo"};
    useRepositorySyncStore.setState({preview:{repositories:[first,second]},includeAdded:false,removeDeleted:true});
    useRepositorySyncStore.getState().markApplied();
    const check = vi.spyOn(useResourceLibraryStore.getState(), "previewRepositorySync").mockResolvedValue({repositories:[{...first,modified:[{skillId:"x",name:"Changed"}]}]});
    await useRepositorySyncStore.getState().recheckRepository("one/repo");
    expect(check).toHaveBeenCalledWith(["one/repo"], []);
    expect(useRepositorySyncStore.getState().preview?.repositories[1]).toEqual(second);
    expect(useRepositorySyncStore.getState()).toMatchObject({applied:false,appliedRepositories:["two/repo"],includeAdded:false,removeDeleted:true,checkingRepository:null});
    useRepositorySyncStore.getState().markRepositoryApplied("one/repo");
    expect(useRepositorySyncStore.getState().applied).toBe(true);
  });

  it("retains other items when a single recheck fails", async () => {
    const item = {repository:"one/repo",added:[],modified:[],deleted:[],unchanged:[]};
    useRepositorySyncStore.setState({preview:{repositories:[item,{...item,repository:"two/repo"}]}});
    vi.spyOn(useResourceLibraryStore.getState(), "previewRepositorySync").mockRejectedValue(new Error("offline"));
    await useRepositorySyncStore.getState().recheckRepository("one/repo");
    expect(useRepositorySyncStore.getState().preview?.repositories[0].error).toEqual(expect.any(String));
    expect(useRepositorySyncStore.getState().preview?.repositories[1].error).toBeUndefined();
  });

});

it("removes the withdrawn screenshot preview without changing real data or ignored versions", async () => {
  const repo = (repository: string) => ({repository, added: [], modified: [], deleted: [], unchanged: []});
  const persisted = {
    ignored: ["real/repo:skill:version"],
    preview: {repositories: [
      {...repo("demo/engineering"),added:[{skillId:"new",name:"performance-review",version:"demo-v2"}],modified:[{skillId:"react-patterns",name:"react-patterns",version:"demo-v2"}]},
      {...repo("demo/research"),modified:[{skillId:"research-notes",name:"research-notes",version:"demo-v2"}]},
    ]},
  };
  localStorage.setItem("skillshub.repository-update-preview.v1",JSON.stringify({state:persisted,version:0}));
  await useRepositorySyncStore.persist.rehydrate();
  expect(useRepositorySyncStore.getState().preview).toBeNull();
  expect(useRepositorySyncStore.getState().ignored).toEqual(persisted.ignored);
  const realPreview={repositories:[repo("demo/engineering"),repo("demo/research")]};
  localStorage.setItem("skillshub.repository-update-preview.v1",JSON.stringify({state:{preview:realPreview},version:0}));
  await useRepositorySyncStore.persist.rehydrate();
  expect(useRepositorySyncStore.getState().preview).toEqual(realPreview);
});

it("does not let scoped check results replace repositories outside its scope",async()=>{
  const first={repository:"one/repo",added:[],modified:[],deleted:[],unchanged:[]};
  const second={...first,repository:"two/repo"};
  useRepositorySyncStore.setState({isChecking:false,checkingRepository:null,preview:{repositories:[first,second]}});
  vi.spyOn(useResourceLibraryStore.getState(),"previewRepositorySync").mockResolvedValue({repositories:[first,{...second,error:"outside scope"}]});
  await useRepositorySyncStore.getState().recheckRepository("one/repo");
  expect(useRepositorySyncStore.getState().preview?.repositories[1]).toEqual(second);
});

it("runs scoped rechecks independently and coalesces duplicate repositories", async () => {
  const empty = (repository: string) => ({repository, added: [], modified: [], deleted: [], unchanged: []});
  useRepositorySyncStore.setState({isChecking: false, checkingRepository: null, preview: {repositories: [empty("one/repo"), empty("two/repo")]}});
  const finishes = new Map<string, (report: RepositorySyncPreviewReport) => void>();
  const check = vi.spyOn(useResourceLibraryStore.getState(), "previewRepositorySync").mockImplementation(repositories =>
    new Promise(resolve => finishes.set(repositories![0], resolve)));
  const one = useRepositorySyncStore.getState().recheckRepositories(["one/repo"]);
  const two = useRepositorySyncStore.getState().recheckRepositories(["two/repo"]);
  await useRepositorySyncStore.getState().recheckRepositories(["one/repo"]);
  await vi.waitFor(() => expect(finishes.size).toBe(2));
  expect(check).toHaveBeenCalledTimes(2);
  finishes.get("one/repo")!({repositories: [empty("one/repo")]});
  await one;
  expect(useRepositorySyncStore.getState().checkingRepository).toBe("two/repo");
  finishes.get("two/repo")!({repositories: [empty("two/repo")]});
  await two;
  expect(useRepositorySyncStore.getState().checkingRepository).toBeNull();
  expect(useRepositorySyncStore.getState().preview?.repositories).toHaveLength(2);
});

it("dismisses only failed repository results and preserves valid changes", () => {
  const repo = {repository:"good/repo",added:[{skillId:"new",name:"New"}],modified:[],deleted:[],unchanged:[]};
  useRepositorySyncStore.setState({isChecking:false,preview:{repositories:[repo,{...repo,repository:"failed/repo",error:"network unavailable"}]}});
  useRepositorySyncStore.getState().dismissFailedCheck("good/repo");
  expect(useRepositorySyncStore.getState().preview?.repositories).toHaveLength(2);
  useRepositorySyncStore.getState().dismissFailedCheck("FAILED/repo");
  expect(useRepositorySyncStore.getState().preview?.repositories).toEqual([repo]);
});

it("keeps the frozen coverage denominator when a repository is rechecked", async () => {
  queue.setState({tasks:[]});
  const old:RepositorySyncPreviewReport={coverage:[{skillId:"one",name:"One",repository:"a/repo",status:"failed"},{skillId:"two",name:"Two",repository:"b/repo",status:"completed"}],repositories:[{repository:"a/repo",added:[],modified:[],deleted:[],unchanged:[],error:"network"},{repository:"b/repo",added:[],modified:[],deleted:[],unchanged:[]}]};
  useRepositorySyncStore.setState({preview:old,isChecking:false,checkingRepository:null,reportGeneration:"coverage-report"});
  vi.spyOn(useResourceLibraryStore.getState(),"previewRepositorySync").mockResolvedValue({coverage:[{skillId:"one",name:"One",repository:"a/repo",status:"completed"},{skillId:"three",name:"New local",status:"excluded",reason:"local"}],repositories:[{...old.repositories[0],error:null,snapshotId:"new"}]});
  await useRepositorySyncStore.getState().recheckRepository("a/repo");
  expect(useRepositorySyncStore.getState().preview?.coverage).toEqual([{...old.coverage![0],status:"completed"},old.coverage![1]]);
});


describe("repository check checkpoints", () => {
  const report = (repository:string):RepositorySyncPreviewReport => ({repositories:[{repository,added:[],modified:[],deleted:[],unchanged:[]}]});
  it("pauses between repositories and resumes without checking completed repositories again", async () => {
    vi.restoreAllMocks(); queue.setState({tasks:[]});
    useRepositorySyncStore.setState({reportGeneration:"checkpoint",preview:null,isChecking:false,checkingRepository:null,migratingRepository:null});
    let finish!: (value:RepositorySyncPreviewReport) => void;
    const check = vi.spyOn(useResourceLibraryStore.getState(),"previewRepositorySync")
      .mockImplementationOnce(() => new Promise(resolve => {finish=resolve;}))
      .mockResolvedValueOnce(report("example/two"));
    const pending = useRepositorySyncStore.getState().checkForUpdates(["example/one","example/two"]);
    const task = queue.getState().tasks.at(-1)!;
    expect(task.steps).toHaveLength(2);
    queue.getState().cancel(task.id);
    finish(report("example/one")); await pending;
    expect(check).toHaveBeenCalledTimes(1);
    expect(queue.getState().tasks.at(-1)?.steps[0].status).toBe("success");
    expect(useRepositorySyncStore.getState().error).toBeNull();
    await useRepositorySyncStore.getState().checkForUpdates();
    expect(check.mock.calls.map(call=>call[0])).toEqual([["example/one"],["example/two"]]);
    expect(useRepositorySyncStore.getState().preview?.repositories.map(repo=>repo.repository)).toEqual(["example/one","example/two"]);
  });
  it("rehydrates an interrupted check and continues only its unfinished repository steps", async () => {
    vi.restoreAllMocks(); queue.setState({tasks:[]});
    useRepositorySyncStore.setState({reportGeneration:"restart",preview:report("example/one"),isChecking:false,checkingRepository:null,migratingRepository:null,error:null});
    const step = (repository:string) => ({command:"preview_source_backed_resource_repository_updates",label:repository,args:{repositories:[repository],reportGeneration:"restart"}});
    const saved:BackgroundTask = {id:"restart",key:"restart",label:"Check",kind:"check",locks:["repo:*"],createdAt:1,cancelRequested:false,status:"running",parentReportGeneration:"restart",steps:[
      {...step("example/one"),status:"success",result:report("example/one")},
      {...step("example/two"),status:"running"}, {...step("example/three"),status:"queued"}]};
    localStorage.setItem("skillshub.background-tasks.v1",JSON.stringify({state:{tasks:[saved]},version:0}));
    await queue.persist.rehydrate();
    expect(queue.getState().tasks[0].status).toBe("interrupted");
    const check = vi.spyOn(useResourceLibraryStore.getState(),"previewRepositorySync").mockImplementation(async repos=>report(repos![0]));
    await useRepositorySyncStore.getState().checkForUpdates();
    expect(check.mock.calls.map(call=>call[0])).toEqual([["example/two"],["example/three"]]);
    expect(useRepositorySyncStore.getState().preview?.repositories).toHaveLength(3);
    expect(queue.getState().tasks[0].status).toBe("success");
  });
});
