import i18n from "@/i18n";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { waitFor } from "@testing-library/react";
const { invoke, listen } = vi.hoisted(() => ({ invoke: vi.fn(), listen: vi.fn(async (_event: string, _callback: (event: {payload: Record<string, unknown>}) => void) => vi.fn()) }));
vi.mock("@/lib/tauri", () => ({ invoke }));
vi.mock("@tauri-apps/api/event", () => ({listen}));
import {
  useTaskQueueStore as queue,
  waitForTask,
  taskErrorMessage,
  registerTaskInputRequirement,
  registerTaskResult,
} from "@/stores/taskQueueStore";
it("explains snapshot error codes returned as repository report strings",()=>{
 expect(taskErrorMessage("IMPORT_SOURCE_MISSING")).toBe(i18n.t("workflow.errors.IMPORT_SOURCE_MISSING"));
 expect(taskErrorMessage("REPOSITORY_NOT_IN_LIBRARY")).toBe(i18n.t("workflow.errors.REPOSITORY_NOT_IN_LIBRARY"));
 expect(taskErrorMessage("Repository is no longer in the resource library")).toBe(i18n.t("workflow.errors.REPOSITORY_NOT_IN_LIBRARY"));
 expect(taskErrorMessage("SNAPSHOT_CACHE_FULL")).toBe(i18n.t("workflow.errors.SNAPSHOT_CACHE_FULL"));
 expect(taskErrorMessage("UNSAFE_SNAPSHOT_PATH: unsupported managed file type")).toBe(i18n.t("workflow.errors.UNSAFE_SNAPSHOT_PATH"));
 expect(taskErrorMessage("TARGET_DIRECTORY_CONFLICT")).toBe(i18n.t("workflow.errors.TARGET_DIRECTORY_CONFLICT"));
 expect(taskErrorMessage("SNAPSHOT_LINK_UNAVAILABLE")).toBe(i18n.t("workflow.errors.SNAPSHOT_LINK_UNAVAILABLE"));
});
const input = (key: string, locks: string[] = []) => ({
  key,
  kind: "update" as const,
  label: key,
  locks,
  steps: [{ command: "test", args: { key }, label: key }],
});
describe("background queue", () => {
  it("routes live import progress by operation and cleans up listeners", async () => {
    const callbacks: Array<(event: {payload: Record<string, unknown>}) => void> = [];
    const stops: ReturnType<typeof vi.fn>[] = [];
    listen.mockImplementation(async (_event, callback) => {callbacks.push(callback); const stop=vi.fn();stops.push(stop);return stop;});
    const finishes: Array<() => void> = [];
    invoke.mockImplementation(() => new Promise<void>(resolve => finishes.push(resolve)));
    const ids = ["one", "two"].map(name => queue.getState().enqueue({...input(name),kind:"import",steps:[{command:"import_github_repo_skills",label:name,args:{repoUrl:`example/${name}`}}]}));
    await waitFor(() => expect(invoke).toHaveBeenCalledTimes(2));
    const operationId = invoke.mock.calls[0][1].operationId;
    const payload={operationId,phase:"writing",currentSkill:"skills/one/SKILL.md",currentPath:"SKILL.md",completedFiles:2,totalFiles:5,completedBytes:2,totalBytes:5};
    callbacks.forEach(callback => callback({payload}));
    await waitFor(() => expect(queue.getState().tasks.find(task => task.id === ids[0])?.importProgress).toMatchObject(payload));
    expect(queue.getState().tasks.find(task => task.id === ids[1])?.importProgress).toBeUndefined();
    expect(localStorage.getItem("skillshub.background-tasks.v1")).not.toContain('"currentSkill"');
    const waiters=ids.map(waitForTask);finishes.forEach(finish => finish());await Promise.all(waiters);
    stops.forEach(stop=>expect(stop).toHaveBeenCalledOnce());
  });
  it("sends cancellation to the backend import and resolves the task as cancelled", async () => {
    let rejectImport!: (error: Error) => void;
    invoke.mockImplementation((command) => command === "import_github_repo_skills"
      ? new Promise((_, reject) => {rejectImport = reject;})
      : Promise.resolve(rejectImport(new Error("Operation cancelled"))));
    const id = queue.getState().enqueue({...input("import"), kind:"import",
      steps:[{command:"import_github_repo_skills",args:{repoUrl:"example/repo"},label:"import"}]});
    await waitFor(() => expect(invoke).toHaveBeenCalledWith("import_github_repo_skills", expect.anything()));
    const operationId = invoke.mock.calls[0][1].operationId;
    const waiter = waitForTask(id);
    queue.getState().cancel(id);
    expect(invoke).toHaveBeenCalledWith("cancel_github_operation", {operationId});
    expect((await waiter).status).toBe("cancelled");
    expect(queue.getState().tasks[0].steps[0]).toMatchObject({status:"cancelled",error:undefined});
  });
  it("stops an orphaned running record immediately and allows deletion", () => {
    queue.setState({tasks: [{...input("orphan"), id: "orphan", locks: [], createdAt: 1,
      status: "running", cancelRequested: true}]});
    queue.getState().cancel("orphan");
    expect(queue.getState().tasks[0].status).toBe("cancelled");
    queue.getState().remove("orphan");
    expect(queue.getState().tasks).toHaveLength(0);
  });
  it("removes a stopping record from display while retaining locks until the write finishes", async () => {
    let finish!: () => void;
    invoke.mockImplementationOnce(() => new Promise<void>(resolve => {finish = resolve;})).mockResolvedValue(undefined);
    const id = queue.getState().enqueue(input("stopping", ["repo:one"]));
    const waiter = waitForTask(id);
    queue.getState().remove(id);
    expect(queue.getState().tasks.find(task => task.id === id)?.cancelRequested).toBe(true);
    expect(queue.getState().tasks.find(task => task.id === id)?.clearRequested).toBe(true);
    const next = queue.getState().enqueue(input("next", ["repo:one"]));
    expect(invoke).toHaveBeenCalledTimes(1);
    finish();
    expect((await waiter).status).toBe("cancelled");
    await waitForTask(next);
    expect(queue.getState().tasks.map(task => task.id)).toEqual([next]);
  });
  it("does not submit new work after a running preparation was deleted",async()=>{
    const command="test_deleted_preparation";const submit=vi.fn();
    registerTaskInputRequirement(command,()=>false);registerTaskResult(command,submit);
    let finish!:(value:unknown)=>void;
    invoke.mockImplementationOnce(()=>new Promise(resolve=>{finish=resolve;}));
    const id=queue.getState().enqueue({...input("preparing"),steps:[{command,label:"prepare",args:{}}]});
    const waiter=waitForTask(id);queue.getState().remove(id);finish({});
    expect((await waiter).status).toBe("cancelled");
    expect(submit).not.toHaveBeenCalled();
  });
  it("clears running tasks safely without releasing their target locks early", async () => {
    let finish!: () => void;
    invoke.mockImplementationOnce(() => new Promise<void>(resolve => {finish = resolve;})).mockResolvedValue(undefined);
    const active = queue.getState().enqueue(input("active", ["repo:one"]));
    const queued = queue.getState().enqueue(input("queued", ["repo:one"]));
    const activeWaiter = waitForTask(active);
    const queuedWaiter = waitForTask(queued);
    queue.getState().clearAll();
    expect((await queuedWaiter).status).toBe("cancelled");
    expect(queue.getState().tasks).toHaveLength(1);
    expect(queue.getState().tasks[0]).toMatchObject({id: active, clearRequested: true, cancelRequested: true});
    const next = queue.getState().enqueue(input("next", ["repo:one"]));
    expect(invoke).toHaveBeenCalledTimes(1);
    finish();
    expect((await activeWaiter).status).toBe("cancelled");
    await waitForTask(next);
    expect(queue.getState().tasks.map(task => task.id)).toEqual([next]);
  });
  beforeEach(() => {
    invoke.mockReset();
    queue.setState({ tasks: [] });
    localStorage.clear();
  });
  it("limits update concurrency, serializes shared targets and rejects duplicate submissions", async () => {
    const finishes: Array<() => void> = [];
    invoke.mockImplementation(
      () => new Promise<void>((resolve) => finishes.push(resolve)),
    );
    const a = queue.getState().enqueue(input("a", ["repo:one"]));
    const b = queue.getState().enqueue(input("b", ["repo:one"]));
    const c = queue.getState().enqueue(input("c", ["repo:two"]));
    expect(queue.getState().enqueue(input("a"))).toBe(a);
    await waitFor(() => expect(invoke).toHaveBeenCalledTimes(2));
    expect(queue.getState().tasks.find((t) => t.id === b)?.status).toBe(
      "queued",
    );
    finishes[0]();
    await waitForTask(a);
    await waitFor(() => expect(invoke).toHaveBeenCalledTimes(3));
    finishes[1]();
    finishes[2]();
    await Promise.all([waitForTask(b), waitForTask(c)]);
  });
  it("finishes a current step and stops before the next on cancellation", async () => {
    let finish!: () => void;
    invoke.mockImplementation(
      () =>
        new Promise<void>((resolve) => {
          finish = resolve;
        }),
    );
    const task = {
      ...input("cancel"),
      steps: [...input("one").steps, ...input("two").steps],
    };
    const id = queue.getState().enqueue(task);
    await waitFor(() => expect(invoke).toHaveBeenCalledTimes(1));
    queue.getState().cancel(id);
    finish();
    const result = await waitForTask(id);
    expect(result.status).toBe("cancelled");
    expect(result.steps[0].status).toBe("success");
    expect(invoke).toHaveBeenCalledTimes(1);
  });
  it("retries only failures and never exposes raw provider responses", async () => {
    invoke
      .mockResolvedValueOnce("done")
      .mockRejectedValueOnce("provider raw body");
    const id = queue
      .getState()
      .enqueue({
        ...input("retry"),
        steps: [...input("one").steps, ...input("two").steps],
      });
    const task = await waitForTask(id);
    expect(task.status).toBe("partial");
    expect(task.steps[1].error).not.toContain("provider raw body");
    invoke.mockResolvedValueOnce("done");
    queue.getState().retry(id);
    expect((await waitForTask(id)).status).toBe("success");
    expect(invoke).toHaveBeenCalledTimes(3);
  });
  it("keeps a committed write successful while exposing a sanitized refresh warning", async () => {
    registerTaskResult("committed_with_refresh_warning", async () => {throw new Error("File private-value in use (os error 32)");});
    invoke.mockResolvedValue(null);
    const id=queue.getState().enqueue({...input("warning"),steps:[{command:"committed_with_refresh_warning",label:"Write",args:{}}]});
    const task=await waitForTask(id);
    expect(task.status).toBe("success");
    expect(task.steps[0].warning).toContain(i18n.t("workflow.errors.fileBusy"));
    expect(task.steps[0].warning).not.toContain("private-value");
    queue.getState().retry(id);
    await waitForTask(id);
    expect(invoke).toHaveBeenCalledTimes(1);
  });
  it("retains check results and their refresh warning in the task record", async () => {
    invoke.mockResolvedValue({repositories:[],viewRefreshError:i18n.t("workflow.errors.fileBusy")});
    const id=queue.getState().enqueue({...input("checked-warning"),kind:"check",steps:[{command:"preview_source_backed_resource_repository_updates",label:"Check",args:{repositories:[]}}]});
    const task=await waitForTask(id);
    expect(task.status).toBe("success");
    expect(task.steps[0].result).toMatchObject({repositories:[]});
    expect(task.steps[0].warning).toContain(i18n.t("workflow.errors.fileBusy"));
  });
  it("restores unfinished tasks as interrupted without running them", async () => {
    const task = {
      ...input("restore"),
      id: "saved",
      locks: [],
      createdAt: 1,
      cancelRequested: false,
      status: "running",
      steps: [{ ...input("restore").steps[0], status: "running" }],
    };
    localStorage.setItem(
      "skillshub.background-tasks.v1",
      JSON.stringify({ state: { tasks: [task] }, version: 0 }),
    );
    await queue.persist.rehydrate();
    expect(queue.getState().tasks[0].status).toBe("interrupted");
    expect(invoke).not.toHaveBeenCalled();
  });
  it("keeps only the latest 100 completed tasks in memory", async () => {
    invoke.mockResolvedValue("done");
    for (let i = 0; i < 105; i++)
      await waitForTask(queue.getState().enqueue(input(`history-${i}`)));
    expect(queue.getState().tasks).toHaveLength(100);
    expect(queue.getState().tasks[0].key).toBe("history-5");
  });
});

it("distinguishes changed categories from changed remote contents",()=>{
  expect(taskErrorMessage("Preview changed: item category changed; check this repository again")).toBe(i18n.t("workflow.errors.categoryChanged"));
  expect(taskErrorMessage("Preview changed: remote content version changed; check this repository again")).toBe(i18n.t("workflow.errors.remoteVersionChanged"));
});
it("distinguishes GitHub rate limiting from credentials and AI authorization",()=>{
  expect(taskErrorMessage("GitHub rate limit was exceeded (HTTP 403)")).toBe(i18n.t("workflow.errors.rateLimit"));
  expect(taskErrorMessage("HTTP 429 Too Many Requests")).toBe(i18n.t("workflow.errors.rateLimit"));
  expect(taskErrorMessage("GitHub denied access (HTTP 401)")).toBe(i18n.t("workflow.errors.githubAuthorization"));
  expect(taskErrorMessage("HTTP 401", "ai")).toBe(i18n.t("workflow.errors.aiAuthorization"));
});

it("serializes a full check against updates but lets independent repositories run",async()=>{
  queue.setState({tasks:[]}); invoke.mockReset();
  const finishes:Array<()=>void>=[];
  invoke.mockImplementation(()=>new Promise<void>(resolve=>finishes.push(resolve)));
  const update=queue.getState().enqueue(input("write",["repo:one"]));
  const check=queue.getState().enqueue({...input("check",["repo:*"]),kind:"check"});
  expect(queue.getState().tasks.find(task=>task.id===check)?.status).toBe("queued");
  finishes[0](); await waitForTask(update);
  await waitFor(()=>expect(finishes).toHaveLength(2));
  finishes[1](); await waitForTask(check);
});

it("reports interrupted downloads as network failures even if a fallback mirror denies access", () => {
  expect(taskErrorMessage("Failed to inspect GitHub repository: github network connection failed; fallback mirror returned HTTP 403")).toBe(i18n.t("workflow.errors.network"));
  expect(taskErrorMessage("Failed to download archive: response body failed: network download interrupted; mirror returned HTTP 403")).toBe(i18n.t("workflow.errors.network"));
  expect(taskErrorMessage("Failed to read GitHub repository archive: error decoding response body", "import")).toBe(i18n.t("workflow.errors.network"));
  expect(taskErrorMessage("network request or download timed out")).toBe(i18n.t("workflow.errors.timeout"));
});

it("preserves safe download errors when task failures are handled a second time", () => {
  for (const key of ["network", "timeout", "githubAuthorization", "rateLimit"]) {
    const safe = i18n.t(`workflow.errors.${key}`);
    expect(taskErrorMessage(safe, "import")).toBe(safe);
    expect(taskErrorMessage(new Error(safe), "import")).toBe(safe);
  }
});
