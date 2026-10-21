import {beforeEach, expect, it, vi} from "vitest";
import {fireEvent, render, screen, within} from "@testing-library/react";
import {TaskImportReview} from "@/components/layout/TaskImportReview";
import {preparedImportPreview, submitPreparedImport} from "@/stores/importPreparationStore";
import type {GitHubRepoPreview} from "@/types";

vi.mock("@/stores/importPreparationStore", () => ({
  preparedImportPreview: vi.fn(), submitPreparedImport: vi.fn(() => true),
}));
const preview:GitHubRepoPreview = {
  repo:{owner:"example",repo:"skills",branch:"main",normalizedUrl:"https://github.com/example/skills"},
  skills:[
    {sourcePath:"skills/shared/SKILL.md",skillId:"shared",skillName:"Shared",rootDirectory:"skills",skillDirectoryName:"shared",downloadUrl:"",recommended:true,variantCount:2,duplicatePaths:["distribution/skills/shared/SKILL.md"]},
    {sourcePath:"plugins/demo/skills/shared/SKILL.md",skillId:"shared-variant",skillName:"Shared",rootDirectory:"plugins",skillDirectoryName:"shared-variant",downloadUrl:"",recommended:false,variantCount:2},
  ],
};
beforeEach(() => {
  vi.clearAllMocks();
  vi.mocked(preparedImportPreview).mockReturnValue(preview);
});
it("defaults to one source variant and submits the alternative as skipped", () => {
  render(<TaskImportReview taskId="import" onLater={vi.fn()}/>);
  const variants=screen.getAllByRole("checkbox",{name:"Shared"});
  expect(variants[0]).toBeChecked();expect(variants[1]).not.toBeChecked();
  expect(screen.getByText("已合并 1 份相同副本")).toBeInTheDocument();
  fireEvent.click(screen.getByRole("button",{name:"导入 1 项技能"}));
  expect(submitPreparedImport).toHaveBeenCalledWith("import",preview,[
    {sourcePath:"skills/shared/SKILL.md",resolution:"overwrite"},
    {sourcePath:"plugins/demo/skills/shared/SKILL.md",resolution:"skip"},
  ]);
});
it("switches source variants without selecting both, including after select-all", () => {
  render(<TaskImportReview taskId="import" onLater={vi.fn()}/>);
  const label=screen.getByText("plugins/demo/skills/shared/SKILL.md").closest("label")!;
  fireEvent.click(within(label).getByRole("checkbox"));
  let variants=screen.getAllByRole("checkbox",{name:"Shared"});
  expect(variants[0]).not.toBeChecked();expect(variants[1]).toBeChecked();
  fireEvent.click(screen.getByRole("checkbox",{name:"全选"}));
  fireEvent.click(screen.getByRole("checkbox",{name:"全选"}));
  variants=screen.getAllByRole("checkbox",{name:"Shared"});
  expect(variants[0]).toBeChecked();expect(variants[1]).not.toBeChecked();
});

it("submits only the newly selected variant without an update target id",()=>{
  render(<TaskImportReview taskId="reimport" onLater={vi.fn()}/>);
  fireEvent.click(screen.getAllByRole("checkbox",{name:"Shared"})[1]);
  fireEvent.click(screen.getByRole("button",{name:"导入 1 项技能"}));
  expect(submitPreparedImport).toHaveBeenCalledWith("reimport",preview,[
    {sourcePath:"skills/shared/SKILL.md",resolution:"skip"},
    {sourcePath:"plugins/demo/skills/shared/SKILL.md",resolution:"overwrite"},
  ]);
});
