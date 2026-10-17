import {it, expect} from "vitest";
import {repositoryUpdateRows} from "@/lib/repositoryUpdateRows";
const added = {skillId:"new", name:"New", sourcePath:"skills/new/SKILL.md", version:"v1"};
const deleted = {skillId:"old", name:"Old", version:"deleted"};
const repo = {repository:"owner/repo", added:[added], deleted:[deleted], modified:[], unchanged:[]};
it("keeps every remote skill visible and never infers a rename from one deletion and addition", () => {
  const rows = repositoryUpdateRows([repo], {});
  expect(rows).toHaveLength(2);
  expect(rows.find(row => row.category === "added")?.item).toEqual(added);
  expect(rows.find(row => row.category === "deleted")?.replacement).toBeUndefined();
});
it("does not hide additions when several skills disappeared", () => {
  const rows = repositoryUpdateRows([{...repo, deleted:[deleted, {...deleted, skillId:"second"}]}], {});
  expect(rows.filter(row => row.category === "added")).toHaveLength(1);
});
