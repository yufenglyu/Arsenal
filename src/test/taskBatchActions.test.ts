import { beforeEach, afterEach, it, expect, vi } from "vitest";
import {
  useTaskQueueStore as queue,
  type BackgroundTask,
} from "@/stores/taskQueueStore";
import { useRepositorySyncStore as updates } from "@/stores/repositorySyncStore";
import { submitPreparedImport } from "@/stores/importPreparationStore";
import {
  buildTaskBatches,
  reportRows,
  CHECK_REPOSITORIES,
} from "@/lib/taskBatches";
import type { GitHubRepoPreview, RepositorySyncPreview } from "@/types";
const repo: RepositorySyncPreview = {
  repository: "example/repo",
  snapshotId: "snapshot",
  added: [],
  modified: [],
  deleted: [{ skillId: "old", name: "Old" }],
  unchanged: [],
};
const task = (
  id: string,
  status: BackgroundTask["status"],
): BackgroundTask => ({
  id,
  key: id,
  kind: "check",
  status,
  label: id,
  steps: [],
  locks: [],
  createdAt: 1,
  cancelRequested: false,
});
beforeEach(() => {
  queue.setState({ tasks: [] });
  updates.setState({
    preview: { repositories: [repo] },
    reportGeneration: "generation",
    reportCheckedAt: 100,
    isChecking: false,
    checkingRepository: null,
    migratingRepository: null,
    error: null,
    updateErrors: {},
    repositories: null,
    requestedRepositories: null,
  });
});
afterEach(() => vi.restoreAllMocks());
it("keeps a repository's structural failure instead of the batch's partial-check summary", () => {
  const bad = {...repo,error:"IMPORT_SOURCE_MISSING",snapshotId:null};
  const good = {...repo,repository:"example/good",snapshotId:"good"};
  updates.setState({preview:{repositories:[bad,good]}});
  const failed = {...task("partial", "partial"), steps:[{command:CHECK_REPOSITORIES,label:"Check",args:{repositories:["example/repo","example/good"]},status:"partial" as const,error:"Partial batch summary",result:{repositories:[bad,good]}}]};
  const rows = reportRows(updates.getState(), [failed]);
  expect(rows.find(row=>row.repository==="example/repo")?.error).toBe("IMPORT_SOURCE_MISSING");
  expect(rows.find(row=>row.repository==="example/good")).toMatchObject({status:"success",error:undefined,needsCheck:false});
});
it("does not revive an old sync failure while rechecking its snapshot", () => {
  const failed = {...task("apply", "failed"), steps: [{command: "apply_repository_snapshot", label: "Apply", args: {snapshotId: "snapshot"}, status: "failed" as const, error: "LOCAL_IDENTITY_CHANGED"}]};
  const checking = {...task("recheck", "running"), createdAt: 2, steps: [{command: CHECK_REPOSITORIES, label: "Check", args: {repositories: ["example/repo"]}, status: "running" as const}]};
  updates.setState({updateErrors: {"mirror:snapshot": "LOCAL_IDENTITY_CHANGED"}});
  expect(reportRows(updates.getState(), [failed, checking])[0]).toMatchObject({status: "running", needsCheck: false, error: undefined});
});
it.each(["failed", "cancelled", "interrupted"] as const)("keeps settled %s checks retryable", status => {
  updates.setState({preview: null});
  const checked = {...task("check", status), steps: [{command: CHECK_REPOSITORIES, label: "Check", args: {repositories: ["example/repo"]}, status}]};
  expect(reportRows(updates.getState(), [checked])[0]).toMatchObject({status, needsCheck: true});
});
it.each(["queued", "running"] as const)("does not count %s checks as needing recheck", status => {
  updates.setState({preview: null});
  const checking = {...task("check", status), steps: [{command: CHECK_REPOSITORIES, label: "Check", args: {repositories: ["example/repo"]}, status}]};
  expect(reportRows(updates.getState(), [checking])[0]).toMatchObject({status, needsCheck: false});
});
it("finishes a checked no-change repository without retaining an apply snapshot",()=>{
  updates.setState({preview:{repositories:[{...repo,deleted:[],remoteRef:'a'.repeat(40),snapshotId:null}]}});
  expect(reportRows(updates.getState(),[])[0]).toMatchObject({status:'success',pending:false,needsCheck:false});
  expect(buildTaskBatches([],updates.getState(),null)[0].status).toBe('success');
});
it("counts a document-only application as updated rather than unchanged", () => {
  updates.setState({
    preview: { repositories: [{ ...repo, deleted: [], updated: [] }] },
  });
  const applied = {
    ...task("apply", "success"),
    steps: [
      {
        command: "apply_repository_snapshot",
        label: repo.repository,
        args: { snapshotId: repo.snapshotId },
        status: "success" as const,
      },
    ],
  };
  expect(reportRows(updates.getState(), [applied])[0]).toMatchObject({
    status: "success",
    pending: false,
    applied: true,
  });
});
it("fails closed on old generations, changed snapshots and unacknowledged deletion", () => {
  const enqueue = vi
    .spyOn(queue.getState(), "enqueue")
    .mockReturnValue("child");
  const apply = updates.getState().applySnapshots;
  expect(
    apply(
      [
        {
          repository: repo.repository,
          snapshotId: "snapshot",
          allowDeletions: true,
        },
      ],
      "old",
    ),
  ).toBe(0);
  expect(
    apply(
      [
        {
          repository: repo.repository,
          snapshotId: "old",
          allowDeletions: true,
        },
      ],
      "generation",
    ),
  ).toBe(0);
  expect(
    apply(
      [
        {
          repository: repo.repository,
          snapshotId: "snapshot",
          allowDeletions: false,
        },
      ],
      "generation",
    ),
  ).toBe(0);
  expect(enqueue).not.toHaveBeenCalled();
  expect(
    apply(
      [
        {
          repository: repo.repository,
          snapshotId: "snapshot",
          allowDeletions: true,
        },
      ],
      "generation",
    ),
  ).toBe(1);
  expect(enqueue.mock.calls[0][0]).toMatchObject({
    parentReportGeneration: "generation",
    locks: ["repo:example/repo"],
    steps: [
      {
        command: "apply_repository_snapshot",
        args: { snapshotId: "snapshot" },
      },
    ],
  });
});
it("requires consent for removed files inside a modified skill", () => {
  updates.setState({
    preview: {
      repositories: [
        {
          ...repo,
          deleted: [],
          modified: [
            {
              skillId: "a",
              name: "A",
              files: [{ path: "helper.py", status: "deleted" }],
            },
          ],
        },
      ],
    },
  });
  const enqueue = vi
    .spyOn(queue.getState(), "enqueue")
    .mockReturnValue("child");
  expect(
    updates.getState().applySnapshots(
      [
        {
          repository: repo.repository,
          snapshotId: "snapshot",
          allowDeletions: false,
        },
      ],
      "generation",
    ),
  ).toBe(0);
  expect(enqueue).not.toHaveBeenCalled();
});
it("blocks conflicting repository locks but permits sync during other checks", () => {
  const enqueue = vi
    .spyOn(queue.getState(), "enqueue")
    .mockReturnValue("child");
  queue.setState({
    tasks: [{ ...task("lock", "running"), locks: ["repo:*"] }],
  });
  const choices = [
    {
      repository: repo.repository,
      snapshotId: "snapshot",
      allowDeletions: true,
    },
  ];
  expect(updates.getState().applySnapshots(choices, "generation")).toBe(0);
  queue.setState({ tasks: [] });
  updates.setState({ isChecking: true });
  queue.setState({tasks: [{...task("other", "running"), locks: ["repo:other/repo"]}]});
  expect(updates.getState().applySnapshots(choices, "generation")).toBe(1);
  expect(enqueue).toHaveBeenCalledOnce();
});
it("does not enqueue repositories with unchanged or failed previews", () => {
  const enqueue = vi
    .spyOn(queue.getState(), "enqueue")
    .mockReturnValue("child");
  updates.setState({ preview: { repositories: [{ ...repo, deleted: [] }] } });
  const choices = [
    {
      repository: repo.repository,
      snapshotId: "snapshot",
      allowDeletions: true,
    },
  ];
  expect(updates.getState().applySnapshots(choices, "generation")).toBe(0);
  updates.setState({
    preview: { repositories: [{ ...repo, error: "network" }] },
  });
  expect(updates.getState().applySnapshots(choices, "generation")).toBe(0);
  expect(enqueue).not.toHaveBeenCalled();
});
it("keeps a legacy report identity and creation time when its first update is queued", () => {
  updates.setState({ reportGeneration: null });
  const before = buildTaskBatches([], updates.getState(), null)[0];
  updates.setState({ reportGeneration: "new" });
  const after = buildTaskBatches(
    [
      {
        ...task("apply", "running"),
        createdAt: 200,
        kind: "update",
        parentReportGeneration: "new",
      },
    ],
    updates.getState(),
    null,
  )[0];
  expect(after.id).toBe(before.id);
  expect(after.createdAt).toBe(before.createdAt);
});
it("a successful recheck supersedes a previous failed checkpoint", () => {
  const failed = {
    ...task("failed", "failed"),
    steps: [
      {
        command: CHECK_REPOSITORIES,
        label: repo.repository,
        args: { repositories: [repo.repository] },
        status: "failed" as const,
        error: "network",
      },
    ],
  };
  const success = {
    ...task("success", "success"),
    createdAt: 2,
    steps: [
      { ...failed.steps[0], status: "success" as const, error: undefined },
    ],
  };
  expect(reportRows(updates.getState(), [failed, success])[0]).toMatchObject({
    status: "success",
    needsCheck: false,
    error: undefined,
    pending: true,
  });
});
it("rejects stale import reviews and unknown paths without marking preparation complete", () => {
  const preview: GitHubRepoPreview = {
    repo: {
      owner: "example",
      repo: "skills",
      branch: "main",
      normalizedUrl: "",
    },
    skills: [
      {
        sourcePath: "a",
        skillId: "a",
        skillName: "A",
        rootDirectory: "",
        skillDirectoryName: "a",
        downloadUrl: "",
      },
    ],
  };
  queue.setState({
    tasks: [
      {
        ...task("prepare", "awaiting_input"),
        kind: "import",
        steps: [
          {
            command: "prepare_github_resource_import",
            label: "prepare",
            args: { repoUrl: "example/skills" },
            result: preview,
            status: "awaiting_input",
          },
        ],
      },
    ],
  });
  const enqueue = vi
    .spyOn(queue.getState(), "enqueue")
    .mockReturnValue("child");
  expect(
    submitPreparedImport("prepare", { ...preview }, [
      { sourcePath: "a", resolution: "overwrite" },
    ]),
  ).toBeNull();
  expect(
    submitPreparedImport("prepare", preview, [
      { sourcePath: "unknown", resolution: "overwrite" },
    ]),
  ).toBeNull();
  expect(
    submitPreparedImport("prepare", preview, [
      { sourcePath: "a", resolution: "skip" },
    ]),
  ).toBeNull();
  expect(enqueue).not.toHaveBeenCalled();
  expect(queue.getState().tasks[0].status).toBe("awaiting_input");
});

it("supersedes a failed sync after a newer check even when immutable snapshot IDs match", () => {
  const failed = {...task("failed-sync", "failed"),createdAt: 1, steps: [{command: "apply_repository_snapshot", label: "Apply", args: {snapshotId: "snapshot"}, status: "failed" as const, error: "LOCAL_IDENTITY_CHANGED"}]};
  const checked = {...task("new-check", "success"),createdAt: 2, steps: [{command: CHECK_REPOSITORIES, label: "Check", args: {repositories: [repo.repository]}, status: "success" as const}]};
  expect(reportRows(updates.getState(), [failed, checked])[0]).toMatchObject({status: "success", needsCheck: false, error: undefined});
});
