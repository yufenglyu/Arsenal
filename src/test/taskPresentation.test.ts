import {expect,it} from "vitest";
import {groupStatus,importDetailRows,matchesTaskFilter} from "@/lib/taskPresentation";
import type {BackgroundTask} from "@/stores/taskQueueStore";
const task=(id:string,status:BackgroundTask["status"]):BackgroundTask=>({id,key:id,label:id,kind:"import",status,cancelRequested:false,createdAt:0,locks:[],steps:[]});
it("retains preparation failures even when no preview was returned",()=>{
 const failed={...task("prepare","failed"),steps:[{command:"prepare_github_resource_import",args:{},label:"prepare",status:"failed" as const,error:"timeout"}]};
 expect(importDetailRows([failed])).toEqual([{key:"prepare:0",name:"prepare",status:"failed",error:"timeout"}]);
});
it("keeps mixed batch failures actionable instead of reporting the first successful child",()=>{
 expect(groupStatus([task("one","success"),task("two","failed")])).toBe("partial");
 expect(groupStatus([task("one","success"),task("two","queued")])).toBe("queued");
 expect(matchesTaskFilter("partial","finished")).toBe(false);
 expect(matchesTaskFilter("awaiting_input","attention")).toBe(true);
});
it("does not duplicate preparation rows after the same batch starts importing",()=>{
 const prepared={...task("prepare","success"),steps:[{command:"prepare_github_resource_import",args:{},label:"prepare",result:{skills:[{name:"A"}]}}]};
 const imported={...task("import","success"),steps:[{command:"import_github_repo_skills",args:{},label:"import",result:{importedSkills:[{skillName:"A"}],skippedSkills:["B"]}}]};
 expect(importDetailRows([prepared,imported]).map(row=>[row.name,row.status])).toEqual([["A","imported"],["B","skipped"]]);
});
it("shows conflicts without rendering unrelated payload fields",()=>{
 const prepared={...task("prepare","awaiting_input"),steps:[{command:"prepare_github_resource_import",args:{unrelated:"not-displayed"},label:"prepare",result:{skills:[{name:"A",conflict:{}},{name:"B"}],unrelated:"not-displayed"}}]};
 expect(importDetailRows([prepared]).map(row=>row.name)).toEqual(["A","B"]);
 expect(importDetailRows([prepared])[0].status).toBe("awaiting_input");
 expect(importDetailRows([prepared])[1].status).toBe("ready");
});
