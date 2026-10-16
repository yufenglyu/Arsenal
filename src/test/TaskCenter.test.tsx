import {beforeEach, it, expect, vi} from "vitest";
import {render, screen, fireEvent, act, within} from "@testing-library/react";
import {TaskCenter} from "@/components/layout/TaskCenter";
import {useTaskQueueStore as queue, type BackgroundTask} from "@/stores/taskQueueStore";
import {useRepositorySyncStore as updates} from "@/stores/repositorySyncStore";
import {useAppStatusStore} from "@/stores/appStatusStore";
const task = (id: string, status: BackgroundTask["status"], kind: BackgroundTask["kind"] = "check"): BackgroundTask => ({id,key:id,label:id,kind,status,cancelRequested:false,createdAt:1,locks:[],steps:[{label:id,command:"test",args:{},status}]});
function openCenter() {fireEvent.click(screen.getByRole("button",{name:/^任务 ·/}));}
function selectTask(name: string) {fireEvent.click(within(screen.getByRole("complementary",{name:"任务列表"})).getByRole("button",{name:new RegExp(name)}));}
const report = () => ({repositories:[{repository:"example/repo",added:[],modified:[{skillId:"changed",name:"Changed",version:"v1"}],deleted:[],unchanged:[{skillId:"same",name:"Same"}]}]});
beforeEach(() => {
  queue.setState({tasks:[task("running","running"),task("queued","queued"),task("done","success")]});
  updates.setState({open:false,centerView:null,preview:null,error:null,ignored:[],checkedAt:{},updateErrors:{},isChecking:false,checkingRepository:null,reportCheckedAt:null,reportGeneration:null});
  useAppStatusStore.setState({task:null});
});
it("identifies the repository while import preparation has no skill list yet", () => {
  queue.setState({tasks:[{...task("prepare","running","import"),label:"导入 GitHub 技能",steps:[{command:"prepare_github_resource_import",label:"prepare",status:"running",args:{repoUrl:"https://github.com/example/research-skills/tree/main?private=never-display"}}]}]});
  render(<TaskCenter/>);openCenter();
  expect(screen.getByRole("heading",{name:"example/research-skills"})).toBeInTheDocument();
  expect(screen.getByText("当前仓库：example/research-skills")).toBeInTheDocument();
  expect(screen.queryByText(/never-display/)).not.toBeInTheDocument();
});
it("shows the live skill, file and real file progress while other selections wait", () => {
  queue.setState({tasks:[{...task("import","running","import"),importProgress:{phase:"writing",currentSkill:"skills/a/SKILL.md",currentPath:"scripts/run.py",completedFiles:3,totalFiles:8,completedBytes:5,totalBytes:12},steps:[{command:"import_github_repo_skills",label:"import",status:"running",args:{repoUrl:"example/skills",selections:[{sourcePath:"skills/a/SKILL.md"},{sourcePath:"skills/b/SKILL.md"}]}}]}]});
  render(<TaskCenter/>);openCenter();
  expect(screen.getByText("当前技能：skills/a/SKILL.md")).toBeInTheDocument();
  expect(screen.getByText("当前文件：scripts/run.py")).toBeInTheDocument();
  expect(screen.getByRole("progressbar")).toHaveAttribute("aria-valuenow","3");
  expect(screen.getByRole("progressbar")).toHaveAttribute("aria-valuemax","8");
  expect(within(screen.getByRole("row",{name:/skills\/b\/SKILL.md/})).getByText("等待中")).toBeInTheDocument();
});
it("uses two task-type tabs and removes Check all", () => {
  render(<TaskCenter/>);openCenter();
  expect(screen.getByRole("dialog",{name:"任务"})).toBeInTheDocument();
  expect(screen.getAllByRole("tab")).toHaveLength(2);
  expect(screen.getByRole("tab",{name:/技能更新/})).toHaveAttribute("aria-selected","true");
  expect(screen.queryByRole("button",{name:"检查全部"})).not.toBeInTheDocument();
});
it("stops only the current tab's work including orphaned running records", () => {
  queue.setState({tasks:[task("import","running","import"),task("running","running"),task("queued","queued")]});
  render(<TaskCenter/>);openCenter();fireEvent.click(screen.getByRole("tab",{name:/技能更新/}));
  fireEvent.click(screen.getByRole("button",{name:"停止全部"}));
  expect(queue.getState().tasks.find(t=>t.id==="queued")?.status).toBe("cancelled");
  expect(queue.getState().tasks.find(t=>t.id==="running")?.status).toBe("cancelled");
  expect(queue.getState().tasks.find(t=>t.id==="import")?.cancelRequested).toBe(false);
});
it("cleans finished groups in this tab while preserving active batch children and failures", () => {
  queue.setState({tasks:[{...task("a","success"),batchId:"batch"},{...task("b","running"),batchId:"batch"},task("old","success"),task("failed","failed"),task("cancelled","cancelled"),task("import","success","import")]});
  render(<TaskCenter/>);openCenter();fireEvent.click(screen.getByRole("tab",{name:/技能更新/}));
  fireEvent.click(screen.getByRole("button",{name:"清理已完成记录"}));
  expect(queue.getState().tasks.map(t=>t.id)).toEqual(["a","b","failed","import"]);
});
it("defaults to actionable changes and includes unchanged skills under All", () => {
  updates.setState({preview:report()});render(<TaskCenter/>);openCenter();
  expect(screen.queryByPlaceholderText("搜索技能或仓库...")).not.toBeInTheDocument();
  expect(within(screen.getByLabelText("筛选技能明细")).getAllByRole("button").at(-1)).toHaveTextContent("全部 2");
  expect(screen.getByRole("button",{name:"待处理 1"})).toHaveAttribute("aria-pressed","true");
  expect(screen.getByText("Changed")).toBeInTheDocument();expect(screen.queryByText("Same")).not.toBeInTheDocument();
  fireEvent.click(screen.getByRole("button",{name:"全部 2"}));
  expect(screen.getByText("Same")).toBeInTheDocument();expect(screen.getByRole("checkbox",{name:"Same"})).toBeDisabled();
});
it("keeps the chosen task when a report arrives", () => {
  render(<TaskCenter/>);openCenter();selectTask("running");act(()=>updates.setState({preview:report()}));
  expect(screen.getByRole("heading",{name:"running"})).toBeInTheDocument();expect(screen.getByText("Changed")).not.toBeVisible();
  selectTask("全仓库检查");expect(screen.getByText("Changed")).toBeVisible();
});
it("aggregates a batch until every child finishes", () => {
  queue.setState({tasks:[{...task("a","success"),batchId:"batch",batchLabel:"Batch"},{...task("b","running"),batchId:"batch",batchLabel:"Batch"},{...task("c","queued"),batchId:"batch",batchLabel:"Batch"}]});
  render(<TaskCenter/>);openCenter();expect(screen.getByRole("tab",{name:/技能更新 1 运行中/})).toBeInTheDocument();
  expect(screen.getByText("已完成 1 · 执行中 1 · 排队 1 · 失败 0")).toBeInTheDocument();
  act(()=>queue.setState(s=>({tasks:s.tasks.map(t=>({...t,status:"success"}))})));
  expect(screen.getByRole("tab",{name:/技能更新 0 运行中/})).toBeInTheDocument();expect(screen.getByRole("heading",{name:"Batch"})).toBeInTheDocument();
});
it("shows historical checks without arbitrary arguments", () => {
  queue.setState({tasks:[{...task("check","success"),steps:[{label:"重新检查",command:"preview_source_backed_resource_repository_updates",args:{repositories:["example/repo"],privateField:"hidden-test-value"},status:"success",result:report()}]}]});
  render(<TaskCenter/>);openCenter();expect(screen.getByText("待更新 1")).toBeInTheDocument();expect(screen.queryByText("hidden-test-value")).not.toBeInTheDocument();
});
it("keeps interrupted AI tasks accessible without double-counting report failures", () => {
  queue.setState({tasks:[task("ai","interrupted","ai"),{...task("check","partial"),steps:[{label:"check",command:"preview_source_backed_resource_repository_updates",args:{repositories:["example/repo"]},status:"partial"}]}]});
  updates.setState({checkedAt:{"example/repo":2},preview:{repositories:[{repository:"example/repo",added:[],modified:[],deleted:[],unchanged:[],error:"network unavailable"}]}});
  render(<TaskCenter/>);expect(screen.getByRole("button",{name:/^任务 ·/})).toHaveTextContent("待处理 2");openCenter();
  selectTask("ai");expect(screen.getByRole("button",{name:"重试"})).toBeInTheDocument();
});
it("preserves result choices across task and tab switches", () => {
  queue.setState({tasks:[task("import","running","import"),task("update","running","update")]});updates.setState({preview:report()});
  render(<TaskCenter/>);openCenter();fireEvent.click(screen.getByRole("checkbox",{name:"Changed"}));
  selectTask("update");selectTask("全仓库检查");expect(screen.getByRole("checkbox",{name:"Changed"})).toBeChecked();
  fireEvent.click(screen.getByRole("tab",{name:/技能导入/}));expect(screen.getByText("Changed")).not.toBeVisible();
  fireEvent.click(screen.getByRole("tab",{name:/技能更新/}));expect(screen.getByRole("checkbox",{name:"Changed"})).toBeChecked();
});
it("deletes failed records without changing running work", () => {
  queue.setState({tasks:[task("failed","failed"),task("running","running")]});render(<TaskCenter/>);openCenter();selectTask("failed");
  fireEvent.click(screen.getByRole("button",{name:"删除任务记录：failed"}));expect(queue.getState().tasks.map(t=>t.id)).toEqual(["running"]);expect(queue.getState().tasks[0].cancelRequested).toBe(false);
});
it("allows deleting stopping orphan records", () => {
  queue.setState({tasks:[{...task("old import","running","import"),cancelRequested:true}]});render(<TaskCenter/>);openCenter();
  fireEvent.click(screen.getByRole("button",{name:"删除任务记录：old import"}));expect(queue.getState().tasks).toHaveLength(0);
});
it("dismisses failed repository results", () => {
  queue.setState({tasks:[]});updates.setState({preview:{repositories:[{repository:"failed/repo",added:[],modified:[],deleted:[],unchanged:[],error:"network unavailable"}]}});render(<TaskCenter/>);openCenter();
  fireEvent.click(screen.getByRole("button",{name:"移除失败仓库结果：failed/repo"}));expect(updates.getState().preview?.repositories).toEqual([]);
});
it("filters import task status without a skill search field", () => {
  queue.setState({tasks:[{...task("Repository import","success","import"),steps:[{label:"import",command:"import_github_repo_skills",args:{privateField:"never-show"},status:"success",result:{importedSkills:[{skillName:"Imported A"},{skillName:"Imported B"}],skippedSkills:["Skipped C"]}}]},task("update","running","update")]});
  render(<TaskCenter/>);openCenter();expect(screen.getByText("Imported A")).toBeInTheDocument();expect(screen.queryByText("never-show")).not.toBeInTheDocument();
  expect(screen.queryByRole("textbox",{name:"搜索技能…"})).not.toBeInTheDocument();expect(screen.getByText("Imported B")).toBeInTheDocument();
  fireEvent.change(screen.getByRole("combobox",{name:"筛选任务状态"}),{target:{value:"active"}});expect(screen.queryByText("Imported B")).not.toBeInTheDocument();
});
it("requires confirmation before enqueueing source deletions", () => {
  const enqueue=vi.spyOn(queue.getState(),"enqueue");updates.setState({preview:{repositories:[{repository:"example/repo",added:[],modified:[],deleted:[{skillId:"removed",name:"Removed"}],unchanged:[]}]}});
  render(<TaskCenter/>);openCenter();fireEvent.click(screen.getByRole("checkbox",{name:"Removed"}));fireEvent.click(screen.getByRole("button",{name:"应用更新"}));
  expect(screen.getByRole("dialog",{name:"确认处理来源已删除的技能"})).toBeInTheDocument();expect(enqueue).not.toHaveBeenCalled();fireEvent.click(screen.getByRole("button",{name:"取消"}));expect(enqueue).not.toHaveBeenCalled();enqueue.mockRestore();
});
it("clears the live report but keeps unrelated import and update records", () => {
  queue.setState({tasks:[task("import","success","import"),task("update","running","update")]});updates.setState({preview:report()});render(<TaskCenter/>);openCenter();
  fireEvent.click(screen.getByRole("button",{name:"清空检查结果"}));expect(updates.getState().preview).toBeNull();expect(queue.getState().tasks.map(t=>t.id)).toEqual(["import","update"]);
});
it("opens pending update results when reopened from an import tab", () => {
  updates.setState({preview:report()});render(<TaskCenter/>);openCenter();
  fireEvent.click(screen.getByRole("tab",{name:/技能导入/}));
  act(()=>updates.getState().setOpen(false));
  act(()=>updates.getState().setOpen(true));
  expect(screen.getByRole("tab",{name:/技能更新/})).toHaveAttribute("aria-selected","true");
  expect(screen.getByText("Changed")).toBeVisible();
});

it("keeps applying and retrying updates inside the current check report", () => {
  queue.setState({tasks:[]});
  updates.setState({preview:report(),reportGeneration:"current-report"});
  const enqueue = vi.spyOn(queue.getState(),"enqueue").mockImplementation(input => {
    const id = `child-${queue.getState().tasks.length}`;
    queue.setState(state => ({tasks:[...state.tasks,{...input,id,locks:input.locks ?? [],createdAt:2,status:"running",cancelRequested:false}]}));
    return id;
  });
  render(<TaskCenter/>);openCenter();
  fireEvent.click(screen.getByRole("checkbox",{name:"Changed"}));
  fireEvent.click(screen.getByRole("button",{name:"应用更新"}));
  expect(queue.getState().tasks[0].parentReportGeneration).toBe("current-report");
  expect(within(screen.getByRole("complementary",{name:"任务列表"})).getAllByRole("button")).toHaveLength(2); // report + cleanup
  expect(screen.getByRole("tab",{name:/技能更新 1 运行中/})).toBeInTheDocument();
  expect(screen.getByText("已完成 0 · 执行中 1 · 排队 0 · 失败 0")).toBeVisible();
  expect(screen.getByRole("button",{name:"清空检查结果"})).toBeDisabled();
  act(() => {
    queue.setState(state => ({tasks:state.tasks.map(task => ({...task,status:"failed"}))}));
    updates.setState({updateErrors:{"example/repo:changed:v1":"network unavailable"}});
  });
  fireEvent.click(screen.getByRole("button",{name:"重试"}));
  expect(queue.getState().tasks[1].parentReportGeneration).toBe("current-report");
  expect(within(screen.getByRole("complementary",{name:"任务列表"})).getAllByRole("button")).toHaveLength(2);
  fireEvent.click(screen.getByRole("button",{name:"停止"}));
  expect(queue.getState().tasks[1].status).toBe("cancelled");
  enqueue.mockRestore();
});

it("shows repository rechecks as children and preserves unrelated tasks", () => {
  const child = {...task("retry","running"),parentReportGeneration:"current-report",steps:[{label:"check",command:"preview_source_backed_resource_repository_updates",args:{repositories:["example/repo"]},status:"running" as const}]};
  queue.setState({tasks:[child,task("unrelated","running")]});
  updates.setState({preview:report(),reportGeneration:"current-report"});
  render(<TaskCenter/>);openCenter();
  const list=screen.getByRole("complementary",{name:"任务列表"});
  expect(within(list).queryByRole("button",{name:/retry/})).not.toBeInTheDocument();
  expect(within(list).getByRole("button",{name:/全仓库检查/})).toHaveTextContent("执行中");
  expect(within(list).getByRole("button",{name:/unrelated/})).toBeInTheDocument();
  fireEvent.click(screen.getByRole("button",{name:"停止"}));
  expect(queue.getState().tasks[0].status).toBe("cancelled");
  expect(queue.getState().tasks[1].cancelRequested).toBe(false);
});
