import { enqueueGitHubUrlImport } from "@/stores/importPreparationStore";
import { beforeEach, it, expect, vi } from "vitest";
import {
  act,
  render,
  screen,
  fireEvent,
  waitFor,
} from "@testing-library/react";
import { MemoryRouter } from "react-router-dom";
const { invoke } = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock("@/lib/tauri", () => ({ invoke, isTauriRuntime: () => true }));
vi.mock("@/components/github-import/GitHubRepoImportWizard", async () => {
  const { Dialog, DialogContent, DialogTitle } = await import("@/components/ui/dialog");
  return {
  GitHubRepoImportWizard: ({
    open,
    onImport,
  }: {
    open: boolean;
    onImport: (selections: unknown[]) => void;
  }) =>
    open ? (
      <Dialog open><DialogContent><DialogTitle>Import preview</DialogTitle><button
        onClick={() =>
          onImport([
            { sourcePath: "skills/a", resolution: "overwrite" },
            { sourcePath: "skills/b", resolution: "overwrite" },
          ])
        }
      >
        confirm selection
      </button></DialogContent></Dialog>
    ) : null,
  };
});
import {TaskCenter} from "@/components/layout/TaskCenter";
import {useRepositorySyncStore as updates} from "@/stores/repositorySyncStore";
import { AddSkillsDialog } from "@/components/skill/AddSkillsDialog";
import {
  useTaskQueueStore as queue,
  waitForTask,
} from "@/stores/taskQueueStore";
import { useResourceLibraryStore } from "@/stores/resourceLibraryStore";
beforeEach(() => {
  queue.setState({ tasks: [] });
  updates.setState({open:false,centerView:null,preview:null,error:null,isChecking:false,checkingRepository:null});
  invoke.mockReset();
  vi.spyOn(
    useResourceLibraryStore.getState(),
    "loadResourceLibrary",
  ).mockResolvedValue();
});
it("validates local skills first and queues selected skills without waiting for import", async () => {
  const close = vi.fn();
  let finish!: () => void;
  invoke.mockImplementation((command) =>
    command === "preview_local_resource_skills"
      ? Promise.resolve([
          { skillId: "a", name: "A", conflict: false },
          { skillId: "b", name: "B", conflict: true },
        ])
      : new Promise<void>((resolve) => {
          finish = resolve;
        }),
  );
  render(
    <MemoryRouter>
      <AddSkillsDialog open onOpenChange={close} />
    </MemoryRouter>,
  );
  fireEvent.change(screen.getByLabelText("本地"), {
    target: { value: "/test/skills" },
  });
  fireEvent.click(screen.getByRole("button", { name: "添加" }));
  await waitFor(() => expect(close).toHaveBeenCalledWith(false));
  expect(invoke).toHaveBeenCalledWith("add_local_resource_skills", {
    input: {
      sourceDir: "/test/skills",
      overwrite: false,
      selectedSkillIds: ["a"],
    },
  });
  finish();
  await waitForTask(queue.getState().tasks[0].id);
  expect(
    useResourceLibraryStore.getState().loadResourceLibrary,
  ).toHaveBeenCalled();
});
it("queues GitHub selections together and closes immediately", async () => {
  const close = vi.fn();
  invoke.mockImplementation((command) =>
    Promise.resolve(
      command === "preview_github_repo_import"
        ? {
            repo: { owner: "owner", repo: "repo" },
            skills: [
              { sourcePath: "skills/a", skillId: "a" },
              { sourcePath: "skills/b", skillId: "b" },
            ],
          }
        : { importedSkills: [] },
    ),
  );
  render(
    <MemoryRouter>
      <AddSkillsDialog open onOpenChange={close} />
    </MemoryRouter>,
  );
  fireEvent.change(screen.getByLabelText("GitHub"), {
    target: { value: "owner/repo" },
  });
  fireEvent.click(screen.getByRole("button", { name: "导入" }));
  await waitFor(() => expect(close).toHaveBeenCalledWith(false));
  expect(
    screen.queryByRole("button", { name: "confirm selection" }),
  ).not.toBeInTheDocument();
  await waitFor(() => expect(queue.getState().tasks[0].status).toBe("success"));
  expect(queue.getState().tasks[0].steps).toHaveLength(1);
});

it("imports 100 skills with one repository operation and one library refresh", async () => {
  const revision = "1234567890123456789012345678901234567890";
  const skills = Array.from({length:100}, (_, index) => ({sourcePath:`skills/skill-${index}`,skillId:`skill-${index}`}));
  invoke.mockImplementation(command => Promise.resolve(command === "preview_github_repo_import"
    ? {repo:{owner:"example",repo:"large",branch:revision},skills} : {importedSkills:[]}));
  render(<MemoryRouter><AddSkillsDialog open onOpenChange={vi.fn()}/></MemoryRouter>);
  fireEvent.change(screen.getByLabelText("GitHub"), {target:{value:"example/large"}});
  fireEvent.click(screen.getByRole("button", {name:"导入"}));
  await waitFor(() => expect(queue.getState().tasks[0]?.status).toBe("success"));
  expect(invoke.mock.calls.filter(([command]) => command === "import_github_repo_skills")).toHaveLength(1);
  expect(invoke).toHaveBeenCalledWith("import_github_repo_skills", {
    operationId:expect.any(String),
    repoUrl:`https://github.com/example/large/tree/${revision}`,
    selections:skills.map(skill => ({sourcePath:skill.sourcePath,resolution:"overwrite"})),
  });
  expect(useResourceLibraryStore.getState().loadResourceLibrary).toHaveBeenCalledTimes(1);
});

it("shows only two add entries and cancellation stops a pending preview from importing", async () => {
  let finish!: (items: unknown[]) => void;
  invoke.mockImplementation(
    () =>
      new Promise((resolve) => {
        finish = resolve;
      }),
  );
  const close = vi.fn();
  render(
    <MemoryRouter>
      <AddSkillsDialog open onOpenChange={close} />
    </MemoryRouter>,
  );
  expect(screen.getByRole("button", { name: "添加" })).toBeInTheDocument();
  expect(screen.getByRole("button", { name: "导入" })).toBeInTheDocument();
  expect(screen.queryByText("手动创建")).not.toBeInTheDocument();
  expect(screen.queryByText("选择技能")).not.toBeInTheDocument();
  fireEvent.change(screen.getByLabelText("本地"), {
    target: { value: "/test/skills" },
  });
  fireEvent.click(screen.getByRole("button", { name: "添加" }));
  fireEvent.click(screen.getByRole("button", { name: "取消" }));
  await act(async () => finish([{ skillId: "a", name: "A", conflict: false }]));
  expect(close).toHaveBeenCalledWith(false);
  expect(queue.getState().tasks).toHaveLength(0);
});

it("shows a safe actionable preview error and keeps input available for retry",async()=>{
  invoke.mockRejectedValue("Invalid GitHub repository URL.");
  const close=vi.fn();
  render(<MemoryRouter><AddSkillsDialog open onOpenChange={close}/></MemoryRouter>);
  fireEvent.change(screen.getByLabelText("GitHub"),{target:{value:"invalid"}});
  fireEvent.click(screen.getByRole("button",{name:"导入"}));
  expect(await screen.findByRole("alert")).toHaveTextContent("owner/repo");
  expect(screen.getByRole("button",{name:"导入"})).toBeEnabled();
  expect(close).not.toHaveBeenCalled();
  expect(queue.getState().tasks).toHaveLength(0);
});

it("continues an unfinished GitHub import in the queue after an outside click and unmount", async () => {
  let finish!: (preview: unknown) => void;
  invoke.mockImplementation(command => command === "preview_github_repo_import"
    ? new Promise(resolve => {finish = resolve;}) : Promise.resolve({importedSkills:[]}));
  const close = vi.fn();
  const view = render(<MemoryRouter><AddSkillsDialog open onOpenChange={close}/></MemoryRouter>);
  fireEvent.change(screen.getByLabelText("GitHub"), {target:{value:"owner/repo"}});
  fireEvent.click(screen.getByRole("button", {name:"导入"}));
  const backdrop = document.querySelector('[data-slot="dialog-overlay"]')!;
  fireEvent.mouseDown(backdrop); fireEvent.mouseUp(backdrop); fireEvent.click(backdrop);
  await waitFor(() => expect(close).toHaveBeenCalledWith(false));
  expect(queue.getState().tasks).toHaveLength(1);
  view.unmount();
  await act(async () => finish({repo:{owner:"owner",repo:"repo"},skills:[{sourcePath:"skills/a",skillId:"a",skillName:"A"}]}));
  await waitFor(() => expect(invoke).toHaveBeenCalledWith("import_github_repo_skills", {operationId:expect.any(String),repoUrl:"owner/repo",selections:[{sourcePath:"skills/a",resolution:"overwrite"}]}));
  expect(invoke.mock.calls.filter(([command]) => command === "preview_github_repo_import")).toHaveLength(1);
  await Promise.all(queue.getState().tasks.map(task => waitForTask(task.id)));
});

function dismissOutside() {
  const backdrop = document.querySelector('[data-slot="dialog-overlay"]')!;
  fireEvent.mouseDown(backdrop); fireEvent.mouseUp(backdrop); fireEvent.click(backdrop);
}
const githubPreview = (conflict = false) => ({repo:{owner:"owner",repo:"repo"},skills:[{sourcePath:"skills/a",skillId:"a",skillName:"A",conflict:conflict ? {existingSkillId:"a"} : null},{sourcePath:"skills/b",skillId:"b",skillName:"B"}]});
it("keeps conflicts pending and resumes them from the global task center", async () => {
  let finish!: (preview: unknown) => void;
  invoke.mockImplementation(command => command === "preview_github_repo_import" ? new Promise(resolve => {finish = resolve;}) : Promise.resolve({importedSkills:[]}));
  const view = render(<MemoryRouter><AddSkillsDialog open onOpenChange={vi.fn()}/></MemoryRouter>);
  fireEvent.change(screen.getByLabelText("GitHub"), {target:{value:"owner/repo"}});
  fireEvent.click(screen.getByRole("button", {name:"导入"}));
  dismissOutside(); view.unmount();
  await act(async () => finish(githubPreview(true)));
  await waitFor(() => expect(queue.getState().tasks[0].status).toBe("awaiting_input"));
  expect(invoke.mock.calls.filter(([command]) => command === "import_github_repo_skills")).toHaveLength(0);
  render(<MemoryRouter><TaskCenter/></MemoryRouter>);
  fireEvent.click(screen.getByRole("button", {name:/任务 ·/}));
  fireEvent.click(screen.getByRole("button", {name:"选择技能"}));
  expect(screen.getAllByRole("dialog")).toHaveLength(1);
  fireEvent.change(screen.getByRole("combobox", {name:"同名处理：A"}), {target:{value:"overwrite"}});
  fireEvent.click(screen.getByRole("button", {name:"导入 2 项技能"}));
  await waitFor(() => expect(queue.getState().tasks.every(task => task.status === "success")).toBe(true));
  expect(queue.getState().tasks).toHaveLength(2);
  expect(new Set(queue.getState().tasks.map(task => task.batchId)).size).toBe(1);
  expect(invoke.mock.calls.filter(([command]) => command === "preview_github_repo_import")).toHaveLength(1);
});
it("still cancels an unfinished GitHub preview when the Cancel button is pressed", async () => {
  let finish!: (preview: unknown) => void;
  invoke.mockImplementation(() => new Promise(resolve => {finish = resolve;}));
  render(<MemoryRouter><AddSkillsDialog open onOpenChange={vi.fn()}/></MemoryRouter>);
  fireEvent.change(screen.getByLabelText("GitHub"), {target:{value:"owner/repo"}});
  fireEvent.click(screen.getByRole("button", {name:"导入"}));
  fireEvent.click(screen.getByRole("button", {name:"取消"}));
  await act(async () => finish(githubPreview()));
  expect(queue.getState().tasks).toHaveLength(0);
});
it("does not import after the background preparation is stopped", async () => {
  let finish!: (preview: unknown) => void;
  invoke.mockImplementation(command => command === "preview_github_repo_import" ? new Promise(resolve => {finish = resolve;}) : Promise.resolve({importedSkills:[]}));
  const view = render(<MemoryRouter><AddSkillsDialog open onOpenChange={vi.fn()}/></MemoryRouter>);
  fireEvent.change(screen.getByLabelText("GitHub"), {target:{value:"owner/repo"}});
  fireEvent.click(screen.getByRole("button", {name:"导入"}));
  dismissOutside(); view.unmount();
  queue.getState().cancel(queue.getState().tasks[0].id);
  await act(async () => finish(githubPreview()));
  await waitFor(() => expect(queue.getState().tasks[0].status).toBe("cancelled"));
  expect(invoke.mock.calls.filter(([command]) => command === "import_github_repo_skills")).toHaveLength(0);
});
it("records a detached preview failure and retries with a new preview", async () => {
  let fail!: (reason: unknown) => void;
  let previews = 0;
  invoke.mockImplementation(command => command === "preview_github_repo_import" ? ++previews === 1 ? new Promise((_resolve,reject) => {fail = reject;}) : Promise.resolve(githubPreview()) : Promise.resolve({importedSkills:[]}));
  const view = render(<MemoryRouter><AddSkillsDialog open onOpenChange={vi.fn()}/></MemoryRouter>);
  fireEvent.change(screen.getByLabelText("GitHub"), {target:{value:"owner/repo"}});
  fireEvent.click(screen.getByRole("button", {name:"导入"}));
  dismissOutside(); view.unmount();
  await act(async () => fail(new Error("network unavailable")));
  await waitFor(() => expect(queue.getState().tasks[0].status).toBe("failed"));
  act(() => queue.getState().retry(queue.getState().tasks[0].id));
  await waitFor(() => expect(queue.getState().tasks.length).toBe(2));
  await Promise.all(queue.getState().tasks.map(task => waitForTask(task.id)));
  expect(previews).toBe(2);
  expect(queue.getState().tasks.every(task => task.status === "success")).toBe(true);
});

it("deduplicates dropped URLs before preview and leaves conflicts awaiting review", async () => {
  invoke.mockResolvedValue({repo:{owner:"owner",repo:"repo"},skills:[{sourcePath:"skills/a",skillId:"a",conflict:{existingSkillId:"a"}}]});
  const first = enqueueGitHubUrlImport("https://github.com/owner/repo");
  const duplicate = enqueueGitHubUrlImport("https://github.com/owner/repo");
  expect(first).toBe(duplicate);
  await waitFor(() => expect(queue.getState().tasks[0].status).toBe("awaiting_input"));
  expect(invoke.mock.calls.filter(([command]) => command === "preview_github_repo_import")).toHaveLength(1);
  expect(invoke).not.toHaveBeenCalledWith("import_github_repo_skills", expect.anything());
});
it("automatically imports dropped URLs without conflicts", async () => {
  invoke.mockImplementation(command => Promise.resolve(command === "preview_github_repo_import"
    ? {repo:{owner:"owner",repo:"repo",branch:"a".repeat(40)},skills:[{sourcePath:"skills/a",skillId:"a"}]}
    : {importedSkills:[],skippedSkills:[]}));
  enqueueGitHubUrlImport("https://github.com/owner/repo");
  await waitFor(() => expect(invoke).toHaveBeenCalledWith("import_github_repo_skills", expect.objectContaining({
    repoUrl:`https://github.com/owner/repo/tree/${"a".repeat(40)}`,selections:[{sourcePath:"skills/a",resolution:"overwrite"}],
  })));
  for (const task of queue.getState().tasks) await waitForTask(task.id);
});
