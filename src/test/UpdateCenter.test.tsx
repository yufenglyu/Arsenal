import i18n from "@/i18n";
import {beforeEach, it, expect, vi} from "vitest";
import {render, screen, fireEvent, waitFor, act} from "@testing-library/react";
const {invoke} = vi.hoisted(()=>({invoke:vi.fn()}));
vi.mock("@/lib/tauri",()=>({invoke,isTauriRuntime:()=>true}));
import {UpdateCenter} from "@/components/skill/UpdateCenter";
import {useRepositorySyncStore as updates} from "@/stores/repositorySyncStore";
import {useTaskQueueStore as queue, waitForTask} from "@/stores/taskQueueStore";
import {useResourceLibraryStore} from "@/stores/resourceLibraryStore";
import {useCentralSkillsStore} from "@/stores/centralSkillsStore";
const repository={repository:"owner/repo",snapshotId:"snapshot-one",remoteRef:"a".repeat(40),
  added:[{skillId:"new",name:"New",version:"v1"}], modified:[{skillId:"changed",name:"Changed",version:"v1"}],
  deleted:[{skillId:"gone",name:"Gone",version:"deleted"}],unchanged:[{skillId:"same",name:"Same"}],
  documentChanges:[{path:"README.md",status:"modified" as const}]};
beforeEach(async()=>{
  vi.restoreAllMocks();
  await i18n.changeLanguage("zh"); invoke.mockReset();invoke.mockResolvedValue(undefined);
  queue.setState({tasks:[]});
  vi.spyOn(useResourceLibraryStore.getState(),"loadResourceLibrary").mockResolvedValue();
  vi.spyOn(useCentralSkillsStore.getState(),"loadCentralSkills").mockResolvedValue();
  updates.setState({preview:{repositories:[structuredClone(repository)],coverage:[{skillId:"changed",name:"Changed",repository:"owner/repo",status:"completed"},{skillId:"local",name:"Local",status:"excluded",reason:"local"}]},
    open:true,migratingRepository:null,ignored:[],isChecking:false,checkingRepository:null,error:null,checkedAt:{},updateErrors:{},reportGeneration:"report-one"});
});
async function confirm() {fireEvent.click(screen.getByRole("button",{name:"同步整仓"}));fireEvent.click(screen.getByRole("button",{name:"确认同步"}));await act(async()=>{await Promise.all(queue.getState().tasks.map(t=>waitForTask(t.id)));});}
it("applies one immutable repository task including additions, deletions and documents",async()=>{
  render(<UpdateCenter embedded/>);await confirm();
  expect(invoke).toHaveBeenCalledTimes(1);expect(invoke).toHaveBeenCalledWith("apply_repository_snapshot",{snapshotId:"snapshot-one"});
  const report=updates.getState().preview!.repositories[0];expect(report.added).toEqual([]);expect(report.deleted).toEqual([]);expect(report.documentChanges).toEqual([]);expect(report.updated).toHaveLength(3);
  expect(queue.getState().tasks[0]).toMatchObject({locks:["repo:owner/repo"],parentReportGeneration:"report-one"});
});
it("never hides additions behind deletion candidates",()=>{
 render(<UpdateCenter taskLayout/>);expect(screen.getByText("New")).toBeInTheDocument();expect(screen.getByText("Gone")).toBeInTheDocument();expect(screen.queryByRole("combobox")).not.toBeInTheDocument();
});
it("counts local coverage separately from remote additions and document rows",()=>{
 render(<UpdateCenter embedded/>);expect(screen.getByText(/检查时本地 2 · 纳入 1 · 未纳入 1/)).toBeInTheDocument();expect(screen.getByText(/远端新增 1/)).toBeInTheDocument();
});
it("requires rechecking legacy reports",()=>{
 updates.setState({preview:{repositories:[{...repository,snapshotId:null}]}});render(<UpdateCenter embedded/>);expect(screen.getByRole("button",{name:"同步整仓"})).toBeDisabled();expect(screen.getAllByText(/旧检查结果无法/).length).toBeGreaterThan(0);
});
it("a failed repository remains visible and cannot be applied",()=>{
 updates.setState({preview:{repositories:[{...repository,snapshotId:null,error:"network unavailable"}]}});render(<UpdateCenter embedded/>);expect(screen.getByRole("button",{name:"同步整仓"})).toBeDisabled();expect(screen.getByRole("alert")).toBeInTheDocument();
});
it("retries the same snapshot after write failure",async()=>{
 invoke.mockRejectedValueOnce(new Error("WRITE_FAILED"));render(<UpdateCenter embedded/>);await confirm();
 expect(screen.getByRole("button",{name:"重试整仓同步"})).toBeInTheDocument();
 fireEvent.click(screen.getByRole("button",{name:"重试整仓同步"}));fireEvent.click(screen.getByRole("button",{name:"确认同步"}));
 await waitFor(()=>expect(invoke).toHaveBeenCalledTimes(2));await act(async()=>{await Promise.all(queue.getState().tasks.map(t=>waitForTask(t.id)));});
 expect(invoke.mock.calls.map(c=>c[1])).toEqual([{snapshotId:"snapshot-one"},{snapshotId:"snapshot-one"}]);
});
it("allows document-only and installation-only synchronization",async()=>{
 updates.setState({preview:{repositories:[{...repository,added:[],modified:[],deleted:[]}]}});render(<UpdateCenter embedded/>);await confirm();expect(invoke).toHaveBeenCalledWith("apply_repository_snapshot",{snapshotId:"snapshot-one"});
});
it("does not transfer confirmation to a new snapshot after a recheck",()=>{
 render(<UpdateCenter embedded/>);fireEvent.click(screen.getByRole("button",{name:"同步整仓"}));
 act(()=>updates.setState({preview:{repositories:[{...repository,snapshotId:"different"}]}}));fireEvent.click(screen.getByRole("button",{name:"确认同步"}));expect(invoke).not.toHaveBeenCalled();
});

it("migrates the repository, invalidates its preview and rechecks the new address",async()=>{
 const {useNotebookStore} = await import("@/stores/notebookStore");
 vi.spyOn(useNotebookStore.getState(),"load").mockResolvedValue();
 const recheck = vi.spyOn(updates.getState(),"recheckRepository").mockResolvedValue();
 invoke.mockResolvedValue("new-owner/new-repo");
 render(<UpdateCenter embedded/>);
 fireEvent.click(screen.getByRole("button",{name:"修改仓库地址"}));
 fireEvent.change(screen.getByLabelText("新仓库地址"),{target:{value:"https://github.com/new-owner/new-repo"}});
 fireEvent.click(screen.getByRole("button",{name:"验证并迁移"}));
 await waitFor(()=>expect(recheck).toHaveBeenCalledWith("new-owner/new-repo"));
 expect(invoke).toHaveBeenCalledWith("change_repository_address",{repository:"owner/repo",newAddress:"https://github.com/new-owner/new-repo"});
 expect(updates.getState().preview?.repositories[0]).toMatchObject({repository:"new-owner/new-repo",snapshotId:null,deleted:[],added:[]});
 expect(screen.getByRole("button",{name:"同步整仓"})).toBeDisabled();
});
it("keeps the original preview and displays validation failures",async()=>{
 invoke.mockRejectedValue("addressValidation");
 render(<UpdateCenter embedded/>);
 fireEvent.click(screen.getByRole("button",{name:"修改仓库地址"}));
 fireEvent.change(screen.getByLabelText("新仓库地址"),{target:{value:"new/repo"}});
 fireEvent.click(screen.getByRole("button",{name:"验证并迁移"}));
 expect(await screen.findByRole("alert")).toHaveTextContent("原来源未修改");
 expect(updates.getState().preview?.repositories[0].snapshotId).toBe("snapshot-one");
});
it("disables migration and synchronization while an address migration is running",()=>{
 updates.setState({migratingRepository:"owner/repo"});
 render(<UpdateCenter embedded/>);
 expect(screen.getByRole("button",{name:"修改仓库地址"})).toBeDisabled();
 expect(screen.getByRole("button",{name:"同步整仓"})).toBeDisabled();
});

it("adds the migrated repository to an unrelated existing report without losing that report",async()=>{
 const {useNotebookStore} = await import("@/stores/notebookStore");
 vi.spyOn(useNotebookStore.getState(),"load").mockResolvedValue();
 vi.spyOn(updates.getState(),"recheckRepository").mockResolvedValue();
 invoke.mockResolvedValue("new/repo");
 await updates.getState().changeRepositoryAddress("another/old","new/repo");
 expect(updates.getState().preview?.repositories.map(row=>row.repository)).toEqual(["owner/repo","new/repo"]);
 expect(updates.getState().preview?.repositories[0].snapshotId).toBe("snapshot-one");
 expect(updates.getState().preview?.repositories[1].snapshotId).toBeNull();
});
