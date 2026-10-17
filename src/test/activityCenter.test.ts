import {it, expect} from "vitest";
import {activityGroups, activityAttention, pendingUpdateCount} from "@/lib/activityCenter";
import type {BackgroundTask} from "@/stores/taskQueueStore";
const task = (id:string, status:BackgroundTask["status"], key=id):BackgroundTask => ({id,key,status,label:id,kind:"update",createdAt:1,cancelRequested:false,locks:[],steps:[{command:"test",args:{},label:id}]});
it("does not count updated or unchanged results as pending work", () => {
  const report = {repositories:[{repository:"example/repo",added:[{skillId:"new",name:"New"}],modified:[],deleted:[],
    unchanged:[{skillId:"same",name:"Same"}],updated:[{skillId:"done",name:"Done"}]}]};
  expect(pendingUpdateCount(report,[],[])).toBe(1);
});
it("counts only the latest attempt and groups related failures", () => {
  const tasks = [task("old","failed","same"),{...task("latest","failed","same"),batchId:"batch"},{...task("other","failed"),batchId:"batch"}];
  expect(activityAttention(tasks,null,{},null)).toMatchObject({count:1});
  expect(activityAttention(tasks,null,{},null).attention.map(t => t.id)).toEqual(["latest","other"]);
  expect(activityGroups(tasks)).toHaveLength(2);
});
it("does not count an active whole-repository mirror as another pending update",()=>{
 const preview={repositories:[{repository:"example/repo",snapshotId:"snap",added:[{skillId:"new",name:"New"}],modified:[],deleted:[{skillId:"old",name:"Old"}],unchanged:[]}]};
 const active:BackgroundTask={id:"mirror",key:"mirror:snap",kind:"update",label:"Mirror",status:"running",createdAt:1,cancelRequested:false,locks:[],steps:[]};
 expect(pendingUpdateCount(preview,[],[active])).toBe(0);
 expect(pendingUpdateCount(preview,[],[])).toBe(1);
});
it("counts a failed check once even when the global error and task coexist", () => {
  const failed = {...task("check","failed"),kind:"check" as const,steps:[{command:"preview_source_backed_resource_repository_updates",args:{},label:"check"}]};
  expect(activityAttention([failed],null,{},"network unavailable")).toMatchObject({attention:[],count:0});
});

it("does not bring a dismissed full-check failure back as a pending task", () => {
  const report = {repositories:[{repository:"failed/repo",added:[],modified:[],deleted:[],unchanged:[],error:"network unavailable"}]};
  const failed = {...task("check","partial"),kind:"check" as const,steps:[{command:"preview_source_backed_resource_repository_updates",args:{repositories:null},label:"check",result:report}]};
  expect(activityAttention([failed],{repositories:[]},{"failed/repo":2},null)).toMatchObject({attention:[],count:0});
});
