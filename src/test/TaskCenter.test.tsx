import {useTaskReviewStore} from "@/stores/taskReviewStore";
import { beforeEach, afterEach, it, expect, vi } from "vitest";
import { render, screen, fireEvent, act, within } from "@testing-library/react";
import { TaskCenter } from "@/components/layout/TaskCenter";
import {
  useTaskQueueStore as queue,
  type BackgroundTask,
} from "@/stores/taskQueueStore";
import { useRepositorySyncStore as updates } from "@/stores/repositorySyncStore";
import { useAppStatusStore } from "@/stores/appStatusStore";
import { CHECK_REPOSITORIES } from "@/lib/taskBatches";
import type { GitHubRepoPreview, RepositorySyncPreview } from "@/types";
const task = (
  id: string,
  status: BackgroundTask["status"],
  kind: BackgroundTask["kind"] = "check",
): BackgroundTask => ({
  id,
  key: id,
  label: id,
  kind,
  status,
  cancelRequested: false,
  createdAt: 1,
  locks: [],
  steps: [{ label: id, command: "test", args: {}, status }],
});
const repo = (
  patch: Partial<RepositorySyncPreview> = {},
): RepositorySyncPreview => ({
  repository: "example/repo",
  snapshotId: "snapshot-one",
  localModified: false,
  added: [],
  modified: [
    {
      skillId: "changed",
      name: "Changed",
      files: [{ path: "SKILL.md", status: "modified" }],
    },
  ],
  deleted: [],
  unchanged: [{ skillId: "same", name: "Same" }],
  ...patch,
});
const open = () =>
  fireEvent.click(screen.getByRole("button", { name: /^任务 ·/ }));
const row = (name: string) => screen.getByRole("article", { name });
const expand = (name: string) =>
  fireEvent.click(within(row(name)).getAllByRole("button")[0]);
const history = () =>
  fireEvent.click(screen.getByRole("tab", { name: /历史记录/ }));
const current = () =>
  fireEvent.click(screen.getByRole("tab", { name: /当前任务/ }));
function mockEnqueue() {
  return vi.spyOn(queue.getState(), "enqueue").mockImplementation((input) => {
    const id = `child-${queue.getState().tasks.length}`;
    queue.setState((s) => ({
      tasks: [
        ...s.tasks,
        {
          ...input,
          id,
          status: "queued",
          locks: input.locks ?? [],
          cancelRequested: false,
          createdAt: 2,
        },
      ],
    }));
    return id;
  });
}
const preview: GitHubRepoPreview = {
  repo: {
    owner: "example",
    repo: "skills",
    branch: "main",
    normalizedUrl: "https://github.com/example/skills",
  },
  skills: [
    {
      sourcePath: "a/SKILL.md",
      skillId: "a",
      skillName: "Alpha",
      rootDirectory: "",
      skillDirectoryName: "a",
      downloadUrl: "",
      conflict: {
        existingSkillId: "a",
        existingName: "Alpha",
        proposedSkillId: "a",
        proposedName: "Alpha",
      },
    },
    {
      sourcePath: "b/SKILL.md",
      skillId: "b",
      skillName: "Beta",
      rootDirectory: "",
      skillDirectoryName: "b",
      downloadUrl: "",
    },
  ],
};
const prepared = (): BackgroundTask => ({
  ...task("prepare", "awaiting_input", "import"),
  batchId: "import-batch",
  steps: [
    {
      command: "prepare_github_resource_import",
      label: "prepare",
      args: {
        repoUrl: "https://github.com/example/skills?private=hidden-value",
      },
      status: "awaiting_input",
      result: preview,
    },
  ],
});
beforeEach(() => {
  useTaskReviewStore.setState({excluded: [], approved: []});
  queue.setState({ tasks: [] });
  updates.setState({
    open: false,
    centerView: null,
    preview: null,
    error: null,
    ignored: [],
    checkedAt: {},
    updateErrors: {},
    isChecking: false,
    checkingRepository: null,
    migratingRepository: null,
    reportCheckedAt: null,
    reportGeneration: null,
    dismissedRepositories: [],
    repositories: null,
    requestedRepositories: null,
  });
  useAppStatusStore.setState({ task: null });
});
afterEach(() => vi.restoreAllMocks());
it("excludes AI work from all task counts and history without deleting it", () => {
  queue.setState({
    tasks: [
      task("AI remark", "running", "ai"),
      task("AI explanation", "failed", "ai"),
      task("AI done", "success", "ai"),
      task("refresh", "running"),
    ],
  });
  render(<TaskCenter />);
  expect(screen.getByRole("button", { name: /^任务 ·/ })).toHaveTextContent(
    "运行 1",
  );
  open();
  expect(
    screen.queryByText(/AI remark|AI explanation|AI done/),
  ).not.toBeInTheDocument();
  expect(screen.getByRole("tab", { name: "历史记录 0" })).toBeInTheDocument();
  expect(queue.getState().tasks).toHaveLength(4);
});
it("uses current/history tabs with keyboard navigation", () => {
  render(<TaskCenter />);
  open();
  const tab = screen.getByRole("tab", { name: "当前任务 0" });
  fireEvent.keyDown(tab, { key: "ArrowRight" });
  expect(screen.getByRole("tab", { name: "历史记录 0" })).toHaveAttribute(
    "aria-selected",
    "true",
  );
  expect(screen.getByText("暂无历史记录")).toBeVisible();
});
it("keeps completed batches until explicitly archived", () => {
  queue.setState({
    tasks: [
      { ...task("a", "success"), batchId: "batch", batchLabel: "Batch" },
      { ...task("b", "running"), batchId: "batch", batchLabel: "Batch" },
      task("other", "running"),
    ],
  });
  render(<TaskCenter />);
  open();
  expand("Batch");
  const before = screen
    .getAllByRole("article")
    .map((el) => el.getAttribute("aria-label"));
  act(() =>
    queue.setState((s) => ({
      tasks: s.tasks.map((item) =>
        item.batchId === "batch"
          ? {
              ...item,
              status: "success",
              steps: item.steps.map((step) => ({ ...step, status: "success" })),
            }
          : item,
      ),
    })),
  );
  expect(
    screen.getAllByRole("article").map((el) => el.getAttribute("aria-label")),
  ).toEqual(before);
  expand("Batch");
  fireEvent.click(screen.getByRole("button", {name: "归档已完成"}));
  expect(
    screen.queryByRole("article", { name: "Batch" }),
  ).not.toBeInTheDocument();
  history();
  expect(row("Batch")).toBeVisible();
  expand("Batch");
  expect(row("Batch")).toBeVisible();
});
it("clears history without removing AI, failures, stopped work or successful active-batch children", () => {
  queue.setState({
    tasks: [
      { ...task("a", "success"), batchId: "batch" },
      { ...task("b", "running"), batchId: "batch" },
      task("done", "success"),
      task("failed", "failed"),
      task("stopped", "cancelled"),
      task("AI", "success", "ai"),
    ],
  });
  render(<TaskCenter />);
  open();
  history();
  fireEvent.click(screen.getByRole("button", { name: "清理已完成记录" }));
  expect(queue.getState().tasks.map((item) => item.id)).toEqual([
    "a",
    "b",
    "failed",
    "stopped",
    "AI",
  ]);
});
it("retries only failed children in a large batch", () => {
  const retry = vi
    .spyOn(queue.getState(), "retry")
    .mockImplementation(() => {});
  queue.setState({
    tasks: Array.from({ length: 104 }, (_, i) => ({
      ...task(`repo-${i}`, i < 100 ? "success" : "failed"),
      batchId: "batch",
      batchLabel: "Batch",
    })),
  });
  render(<TaskCenter />);
  open();
  fireEvent.click(screen.getByRole("button", { name: "重试失败项" }));
  expect(retry.mock.calls.map(([id]) => id)).toEqual([
    "repo-100",
    "repo-101",
    "repo-102",
    "repo-103",
  ]);
});
it("stops only the chosen batch", () => {
  queue.setState({
    tasks: [
      { ...task("a", "running"), batchId: "batch", batchLabel: "Batch" },
      { ...task("b", "queued"), batchId: "batch", batchLabel: "Batch" },
      task("other", "running"),
      task("AI", "running", "ai"),
    ],
  });
  render(<TaskCenter />);
  open();
  fireEvent.click(within(row("Batch")).getByRole("button", { name: "停止" }));
  expect(
    queue
      .getState()
      .tasks.filter((item) => item.cancelRequested)
      .map((item) => item.id),
  ).toEqual(["a", "b"]);
});
it("identifies full updates and refresh scope explicitly, including single repository refresh", () => {
  updates.setState({
    preview: { repositories: [repo()] },
    reportGeneration: "g",
  });
  queue.setState({
    tasks: [
      {
        ...task("refresh", "running"),
        scope: { mode: "selected", repositories: ["example/one"] },
        steps: [
          {
            command: "refresh_repository",
            label: "example/one",
            args: { repository: "example/one" },
            status: "running",
          },
        ],
      },
    ],
  });
  render(<TaskCenter />);
  open();
  expect(row("更新技能仓库")).toBeVisible();
  expect(row("刷新仓库 example/one")).toBeVisible();
  expect(screen.getByText("全部 1 个仓库")).toBeVisible();
});
it("does not replace an expanded import when a report arrives", () => {
  queue.setState({ tasks: [prepared()] });
  render(<TaskCenter />);
  open();
  expand("导入技能 example/skills");
  act(() => updates.setState({ preview: { repositories: [repo()] } }));
  expect(screen.getByRole("checkbox", { name: "Beta" })).toBeVisible();
  expect(
    screen.queryByRole("checkbox", { name: "example/repo" }),
  ).not.toBeInTheDocument();
});
it("keeps import choices inline across collapse and tab changes and queues only chosen skills", () => {
  const enqueue = mockEnqueue();
  queue.setState({ tasks: [prepared()] });
  render(<TaskCenter />);
  open();
  expand("导入技能 example/skills");
  expect(screen.getAllByRole("dialog")).toHaveLength(1);
  expect(screen.queryByText(/hidden-value/)).not.toBeInTheDocument();
  fireEvent.click(screen.getByRole("checkbox", { name: "全选" }));
  expect(screen.getByRole("button", { name: "导入 0 项技能" })).toBeDisabled();
  fireEvent.click(screen.getByRole("checkbox", { name: "Beta" }));
  fireEvent.click(screen.getByRole("button", { name: "稍后处理" }));
  history();
  current();
  expand("导入技能 example/skills");
  expect(screen.getByRole("checkbox", { name: "Beta" })).toBeChecked();
  expect(screen.getByRole("checkbox", { name: /Alpha/ })).not.toBeChecked();
  fireEvent.click(screen.getByRole("button", { name: "导入 1 项技能" }));
  expect(enqueue).toHaveBeenCalledOnce();
  expect(enqueue.mock.calls[0][0]).toMatchObject({
    batchId: "import-batch",
    steps: [
      {
        args: {
          selections: [{ sourcePath: "b/SKILL.md", resolution: "overwrite" }],
        },
      },
    ],
  });
  expect(queue.getState().tasks[0]).toMatchObject({
    status: "success",
    skippedImportSkills: ["Alpha"],
  });
  expect(screen.getAllByRole("article")).toHaveLength(1);
});
it("preserves conflict rename and prevents invalid names", () => {
  queue.setState({ tasks: [prepared()] });
  render(<TaskCenter />);
  open();
  expand("导入技能 example/skills");
  fireEvent.change(screen.getByRole("combobox", { name: "同名处理：Alpha" }), {
    target: { value: "rename" },
  });
  fireEvent.change(screen.getByRole("textbox", { name: "重命名：Alpha" }), {
    target: { value: "../bad" },
  });
  expect(screen.getByRole("button", { name: "导入 2 项技能" })).toBeDisabled();
});
it("shows file progress and retains detailed file/skill information in logs", () => {
  queue.setState({
    tasks: [
      {
        ...task("import", "running", "import"),
        importProgress: {
          phase: "writing",
          currentSkill: "a/SKILL.md",
          currentPath: "scripts/run.py",
          completedFiles: 3,
          totalFiles: 8,
          completedBytes: 5,
          totalBytes: 12,
        },
        steps: [
          {
            command: "import_github_repo_skills",
            label: "import",
            status: "running",
            args: {
              repoUrl: "example/skills",
              selections: [
                { sourcePath: "a/SKILL.md" },
                { sourcePath: "b/SKILL.md" },
              ],
            },
          },
        ],
      },
    ],
  });
  render(<TaskCenter />);
  open();
  expand("导入技能 example/skills");
  expect(screen.getByRole("progressbar")).toHaveAttribute("aria-valuenow", "3");
  expect(screen.getByRole("progressbar")).toHaveAttribute("aria-valuemax", "8");
  fireEvent.click(screen.getByText("日志"));
  expect(screen.getByText("当前文件：scripts/run.py")).toBeVisible();
  expect(screen.getByText("b/SKILL.md")).toBeVisible();
});
it("requires inline deletion confirmation and invalidates it when the snapshot changes", () => {
  const enqueue = mockEnqueue();
  updates.setState({
    reportGeneration: "g",
    preview: {
      repositories: [
        repo({ deleted: [{ skillId: "removed", name: "Removed" }] }),
      ],
    },
  });
  render(<TaskCenter />);
  open();
  expect(screen.getAllByRole("dialog")).toHaveLength(1);
  const submit = () =>
    screen.getByRole("button", { name: /同步已选 [01] 个仓库/ });
  expect(submit()).toBeDisabled();
  fireEvent.click(screen.getByText("核对风险"));
  fireEvent.click(screen.getByRole("button", {name: "允许删除并选中"}));
  expect(submit()).toBeEnabled();
  act(() =>
    updates.setState({
      preview: {
        repositories: [
          repo({
            snapshotId: "snapshot-two",
            deleted: [{ skillId: "new", name: "New deletion" }],
          }),
        ],
      },
    }),
  );
  expect(submit()).toBeDisabled();
  fireEvent.click(screen.getByText("核对风险"));
  fireEvent.click(screen.getByRole("button", {name: "允许删除并选中"}));
  fireEvent.click(submit());
  expect(enqueue).toHaveBeenCalledOnce();
  expect(enqueue.mock.calls[0][0]).toMatchObject({
    parentReportGeneration: "g",
    steps: [
      {
        command: "apply_repository_snapshot",
        args: { snapshotId: "snapshot-two" },
      },
    ],
  });
  expect(screen.getAllByRole("article")).toHaveLength(1);
});
it("requires confirmation for deleted files even when no skill is removed", () => {
  updates.setState({
    preview: {
      repositories: [
        repo({ documentChanges: [{ path: "README.md", status: "deleted" }] }),
      ],
    },
  });
  render(<TaskCenter />);
  open();
  fireEvent.click(screen.getByText("核对风险"));
  expect(screen.getByRole("button", {name: "允许删除并选中"})).toBeVisible();
  expect(screen.queryByRole("checkbox", { name: /确认删除/ })).not.toBeInTheDocument();
  expect(
    screen.getByRole("button", { name: /同步已选 [01] 个仓库/ }),
  ).toBeDisabled();
});
it("keeps report identity throughout checking, review, application and retry", () => {
  queue.setState({
    tasks: [
      {
        ...task("check", "running"),
        parentReportGeneration: "g",
        scope: { mode: "all", repositories: ["example/repo"] },
        steps: [
          {
            command: CHECK_REPOSITORIES,
            label: "example/repo",
            args: { repositories: ["example/repo"] },
            status: "running",
          },
        ],
      },
    ],
  });
  updates.setState({ reportGeneration: "g", isChecking: true });
  render(<TaskCenter />);
  open();
  expand("更新技能仓库");
  act(() => {
    queue.setState((s) => ({
      tasks: s.tasks.map((item) => ({
        ...item,
        status: "success",
        steps: item.steps.map((step) => ({ ...step, status: "success" })),
      })),
    }));
    updates.setState({
      preview: { repositories: [repo()] },
      isChecking: false,
    });
  });
  expect(screen.getAllByRole("article")).toHaveLength(1);
  const enqueue = mockEnqueue();
  fireEvent.click(screen.getByRole("button", { name: /同步已选 [01] 个仓库/ }));
  expect(enqueue.mock.calls[0][0].parentReportGeneration).toBe("g");
  expect(screen.getAllByRole("article")).toHaveLength(1);
});
it("reports failed repository checks without leaking raw provider errors", () => {
  updates.setState({
    preview: {
      repositories: [
        repo({ snapshotId: null, error: "private-unknown-error" }),
      ],
    },
  });
  render(<TaskCenter />);
  open();
  expect(screen.queryByText(/private-unknown-error/)).not.toBeInTheDocument();
  expect(screen.getByRole("button", { name: "重新检查" })).toBeVisible();
  expect(
    screen.queryByRole("button", { name: /确认更新/ }),
  ).not.toBeInTheDocument();
});

it("selects routine repositories in bulk and excludes unconfirmed risk and failed rows", () => {
  updates.setState({reportGeneration: "bulk", preview: {repositories: [
    ...Array.from({length: 98}, (_, i) => repo({repository: `example/regular-${i}`, snapshotId: `regular-${i}`})),
    repo({repository: "example/risky", snapshotId: "risk", deleted: [{skillId: "old", name: "Old", sourcePath: "old/SKILL.md"}]}),
    repo({repository: "example/failure", snapshotId: null, error: "NETWORK_ERROR"}),
  ]}});
  const enqueue = mockEnqueue();
  render(<TaskCenter />); open();
  expect(screen.getByRole("button", {name: "同步已选 98 个仓库"})).toBeEnabled();
  expect(screen.queryByText("example/regular-0")).not.toBeInTheDocument();
  fireEvent.click(screen.getByRole("button", {name: "同步已选 98 个仓库"}));
  expect(enqueue).toHaveBeenCalledTimes(98);
  expect(enqueue.mock.calls.every(([input]) => input.steps[0].args.snapshotId !== "risk")).toBe(true);
});
it("keeps other repository rechecks and sync available while one repository is checking", () => {
  const recheck = vi.spyOn(updates.getState(), "recheckRepository").mockResolvedValue();
  updates.setState({checkingRepository: "example/first", preview: {repositories: [repo(), repo({repository: "example/second", error: "NETWORK_ERROR", snapshotId: null}), repo({repository: "example/third", error: "NETWORK_ERROR", snapshotId: null})]}});
  render(<TaskCenter />); open();
  fireEvent.click(within(screen.getByRole("region", {name: "example/second"})).getByRole("button", {name: "重新检查"}));
  fireEvent.click(within(screen.getByRole("region", {name: "example/third"})).getByRole("button", {name: "重新检查"}));
  expect(recheck.mock.calls.map(([name]) => name)).toEqual(["example/second", "example/third"]);
  expect(screen.getByRole("button", {name: "同步已选 1 个仓库"})).toBeEnabled();
});
it("gives expired sync failures a single recheck action and no selectable checkbox", () => {
  updates.setState({preview: {repositories: [repo()]}, updateErrors: {"mirror:snapshot-one": "LOCAL_IDENTITY_CHANGED"}});
  render(<TaskCenter />); open();
  const repository = within(screen.getByRole("region", {name: "example/repo"}));
  expect(repository.getAllByRole("button", {name: "重新检查"})).toHaveLength(1);
  expect(repository.queryByRole("checkbox")).not.toBeInTheDocument();
  expect(screen.getByRole("button", {name: "同步已选 0 个仓库"})).toBeDisabled();
});
it("keeps multiple batches expanded without resetting import choices", () => {
  queue.setState({tasks: [prepared()]});
  updates.setState({preview: {repositories: [repo({deleted: [{skillId: "old", name: "Old"}]})]}});
  render(<TaskCenter />); open();
  expand("导入技能 example/skills");
  expect(screen.getByText("核对风险")).toBeVisible();
  expect(screen.getByRole("combobox", {name: "同名处理：Alpha"})).toBeVisible();
});

it("groups expanded review details and removes the unrelated address action",()=>{
  updates.setState({preview:{repositories:[repo({localModified:true})]},reportGeneration:"review"});
  render(<TaskCenter/>);open();
  const section=screen.getByRole("region",{name:"example/repo"});
  expect(section).toHaveClass("rounded-md","border");
  const disclosure=within(section).getByText("核对风险").closest("details")!;
  expect(disclosure).toHaveClass("open:border-border","open:bg-muted/20");
  expect(screen.queryByRole("button",{name:"修改仓库地址"})).not.toBeInTheDocument();
});
it("deletes an import awaiting selection without submitting it",()=>{
  queue.setState({tasks:[prepared()]});render(<TaskCenter/>);open();
  const batch=screen.getByRole("article");
  fireEvent.click(within(batch).getByRole("button",{name:/删除任务记录/}));
  expect(queue.getState().tasks).toHaveLength(0);
  expect(screen.queryByRole("article")).not.toBeInTheDocument();
});
it("deletes one pending repository without hiding the other repository",()=>{
  updates.setState({preview:{repositories:[repo({localModified:true}),repo({repository:"example/other",snapshotId:"other",localModified:true})]},reportGeneration:"delete-one"});
  render(<TaskCenter/>);open();
  fireEvent.click(screen.getByRole("button",{name:"删除任务记录：example/repo"}));
  expect(updates.getState().preview?.repositories.map(row=>row.repository)).toEqual(["example/other"]);
  expect(screen.queryByRole("region",{name:"example/repo"})).not.toBeInTheDocument();
  expect(screen.getByRole("region",{name:"example/other"})).toBeInTheDocument();
});
