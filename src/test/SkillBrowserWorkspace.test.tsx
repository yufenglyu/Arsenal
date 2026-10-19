vi.mock("@/components/notebook/RepositoryNotebook", () => ({ RepositoryNotebook: ({skillId}: {skillId: string}) => <div data-testid="repository-notebook">{skillId}</div> }));
import { useRepositoryDocumentStore } from "@/stores/repositoryDocumentStore";
import { TagFilters } from "@/components/skill/TagFilters";
import { type ReactElement, type ReactNode } from "react";
import { MemoryRouter } from "react-router-dom";
import { openRowActions } from "./rowActions";
import { fireEvent, render as testingRender, screen, within, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { SkillBrowserWorkspace } from "@/components/skill/SkillBrowserWorkspace";
import { SkillBrowserHeader } from "@/components/skill/SkillBrowserHeader";
import { useShortcutStore } from "@/stores/shortcutStore";
import { DEFAULT_SHORTCUTS } from "@/lib/shortcutKeys";
import type { FolderTableItem, SkillTableItem } from "@/components/skill/SkillBrowserTable";
vi.mock("@/components/skill/SkillDetailView", async (importOriginal) => ({ ...await importOriginal<typeof import("@/components/skill/SkillDetailView")>(), SkillDetailView: ({skillId,agentId,rowId,inspectorTab,installationSummary}:{skillId:string;agentId?:string;rowId?:string;inspectorTab:string;installationSummary?:ReactNode}) => <div data-testid="inspector">{[skillId,agentId,rowId,inspectorTab].join("|")}{inspectorTab === "overview" && installationSummary}</div> }));
const render = (ui: ReactElement) => ui.type === MemoryRouter ? testingRender(ui) : testingRender(ui, {wrapper: MemoryRouter});
const remove = vi.fn();
const skills: SkillTableItem[] = [
  {rowKey:"one",detailRequest:{skillId:"one"},name:"First skill",sourceRepo:"Example/Tools",onRemove:remove},
  {rowKey:"two",detailRequest:{skillId:"two"},name:"Second skill",sourceRepo:"Example/Tools"},
];
const folders: FolderTableItem[] = [{key:"repo",name:"Example/Tools",path:"/demo/tools",skillCount:2,skillKeys:["one","two"],githubStars:20,onOpen:vi.fn()}];
const props={storageKey:"test",skills,folders,showGithubStars:true};
beforeEach(()=>{localStorage.clear();vi.clearAllMocks()});
describe("tree browser workspace",()=>{
 it("restores each view's scroll position after the shorter view clamps scrolling",()=>{
  render(<SkillBrowserWorkspace {...props}/>);
  const scroll=screen.getByRole("region",{name:"技能列表"}).children[1] as HTMLElement;
  scroll.scrollTop=120;
  fireEvent.click(screen.getByRole("button",{name:"平铺所有技能"}));
  expect(scroll.scrollTop).toBe(120);
  scroll.scrollTop=4000;
  fireEvent.click(screen.getByRole("button",{name:"按仓库分组"}));
  expect(scroll.scrollTop).toBe(120);
  scroll.scrollTop=80;
  fireEvent.click(screen.getByRole("button",{name:"平铺所有技能"}));
  expect(scroll.scrollTop).toBe(4000);
  fireEvent.click(screen.getByRole("button",{name:"按仓库分组"}));
  expect(scroll.scrollTop).toBe(80);
 });
 it.each(["resources", "collections", "central", "platform", "project"])("places the %s view switch after Open and handles F3/F5", scope => {
  useShortcutStore.setState({shortcuts:{...DEFAULT_SHORTCUTS}});
  const refresh=vi.fn();
  const toolbar=(viewControl: React.ReactNode) => <SkillBrowserHeader title={<button>Open directory</button>} viewControl={viewControl} actions={<button>Add</button>} search={<input aria-label="search"/>}/>;
  const {rerender}=render(<SkillBrowserWorkspace {...props} storageKey={scope} toolbar={toolbar} onRefresh={refresh}/>);
  const toolbarButtons=screen.getByRole("button",{name:"Open directory"}).closest("header")!.querySelectorAll("button");
  expect([...toolbarButtons].map(button=>button.getAttribute("aria-label") ?? button.textContent)).toEqual(["Open directory","平铺所有技能","Add"]);
  expect(screen.queryByRole("button",{name:"全部展开"})).not.toBeInTheDocument();
  const viewEvent=new KeyboardEvent("keydown",{key:"F3",bubbles:true,cancelable:true});
  fireEvent(screen.getByRole("textbox",{name:"search"}),viewEvent);
  expect(viewEvent.defaultPrevented).toBe(true);
  expect(screen.getByRole("columnheader",{name:"仓库"})).toBeInTheDocument();
  fireEvent.keyDown(window,{key:"F5"});
  expect(refresh).toHaveBeenCalledOnce();
  rerender(<SkillBrowserWorkspace {...props} storageKey={scope} toolbar={toolbar} onRefresh={refresh} loading/>);
  fireEvent.keyDown(window,{key:"F5"});
  expect(refresh).toHaveBeenCalledOnce();
  fireEvent.keyDown(window,{key:"F3"});
  expect(screen.queryByRole("columnheader",{name:"仓库"})).not.toBeInTheDocument();
  fireEvent.keyDown(screen.getByRole("region",{name:"技能列表"}),{key:"+"});
  expect(screen.getByRole("button",{name:"查看 First skill 的详情"})).toBeInTheDocument();
 });
 it("switches to all skills with repository columns and persists the view",()=>{
  const {unmount}=render(<SkillBrowserWorkspace {...props} allowFlatView/>);
  fireEvent.click(screen.getByRole("button",{name:"平铺所有技能"}));
  expect(screen.getByRole("columnheader",{name:"仓库"})).toBeInTheDocument();
  expect(screen.getByRole("button",{name:"查看 First skill 的详情"})).toBeInTheDocument();
  expect(screen.getByRole("button",{name:"查看 Second skill 的详情"})).toBeInTheDocument();
  expect(screen.queryByRole("button",{name:"全部展开"})).not.toBeInTheDocument();
  fireEvent.click(screen.getByRole("button",{name:"查看 First skill 的详情"}));
  expect(screen.getByTestId("inspector")).toHaveTextContent("one");
  unmount();render(<SkillBrowserWorkspace {...props} allowFlatView/>);
  expect(screen.getByRole("columnheader",{name:"仓库"})).toBeInTheDocument();
  fireEvent.click(screen.getByRole("button",{name:"按仓库分组"}));
  expect(screen.queryByRole("columnheader",{name:"仓库"})).not.toBeInTheDocument();
 });
 it("keeps selection separate from expansion and renders one shared table",()=>{
  render(<SkillBrowserWorkspace {...props}/>);
  expect(screen.getAllByRole("table")).toHaveLength(1);
  expect(screen.queryByText("First skill")).not.toBeInTheDocument();
  fireEvent.click(screen.getByRole("button",{name:"Example/Tools"}));
  expect(screen.getByRole("heading",{name:"Example/Tools"})).toBeInTheDocument();
  expect(screen.queryByTestId("inspector")).not.toBeInTheDocument();
  fireEvent.click(screen.getByRole("button",{name:"展开 Example/Tools"}));
  fireEvent.click(screen.getAllByRole("button",{name:"查看 First skill 的详情"})[0]);
  expect(screen.getByTestId("inspector")).toHaveTextContent("one|||overview");
  expect(screen.getAllByRole("columnheader").map(h=>h.getAttribute("aria-label"))).toEqual(["序号","名称","技能数","笔记","星标数","创建时间","更新时间"]);
 });
 it("selects skill rows from non-interactive cells",()=>{
  render(<SkillBrowserWorkspace {...props}/>);fireEvent.keyDown(screen.getByRole("region",{name:"技能列表"}),{key:"+"});
  const row=screen.getByRole("button",{name:"查看 First skill 的详情"}).closest("tr")!;
  fireEvent.click(within(row).getAllByRole("cell")[0]);expect(screen.getByTestId("inspector")).toHaveTextContent("one");
 });
 it("row actions do not change the current selection",()=>{
  render(<SkillBrowserWorkspace {...props}/>);fireEvent.keyDown(screen.getByRole("region",{name:"技能列表"}),{key:"+"});
  fireEvent.click(screen.getByRole("button",{name:"查看 Second skill 的详情"}));
  const row=screen.getByRole("button",{name:"查看 First skill 的详情"}).closest("tr")!;
  fireEvent.click(openRowActions(row).getByRole("menuitem", { name: "删除"}));
  expect(screen.getByTestId("inspector")).toHaveTextContent("two");
 });
 it("search expands matching rows without overwriting saved expansion",()=>{
  const {rerender}=render(<SkillBrowserWorkspace {...props}/>);
  rerender(<SkillBrowserWorkspace {...props} skills={[skills[1]]} searchActive/>);
  expect(screen.getByRole("button",{name:"查看 Second skill 的详情"})).toBeInTheDocument();
  expect(screen.queryByRole("button",{name:"查看 First skill 的详情"})).not.toBeInTheDocument();
  rerender(<SkillBrowserWorkspace {...props}/>);
  expect(screen.queryByRole("button",{name:"查看 Second skill 的详情"})).not.toBeInTheDocument();
 });
 it("retains expanded repositories after remount",()=>{
  const {unmount}=render(<SkillBrowserWorkspace {...props}/>);fireEvent.keyDown(screen.getByRole("region",{name:"技能列表"}),{key:"+"});unmount();
  render(<SkillBrowserWorkspace {...props}/>);expect(screen.getByRole("button",{name:"查看 First skill 的详情"})).toBeInTheDocument();
 });
 it("keeps the active document tab when selecting another skill",()=>{
  render(<SkillBrowserWorkspace {...props}/>);fireEvent.keyDown(screen.getByRole("region",{name:"技能列表"}),{key:"+"});fireEvent.click(screen.getByRole("button",{name:"查看 First skill 的详情"}));fireEvent.click(screen.getByRole("tab",{name:"文档"}));fireEvent.click(screen.getByRole("button",{name:"查看 Second skill 的详情"}));expect(screen.getByTestId("inspector")).toHaveTextContent("two|||document");
 });
 it("clears stale detail when the selected skill is removed or filtered out",()=>{
  const {rerender}=render(<SkillBrowserWorkspace {...props}/>);fireEvent.keyDown(screen.getByRole("region",{name:"技能列表"}),{key:"+"});fireEvent.click(screen.getByRole("button",{name:"查看 First skill 的详情"}));rerender(<SkillBrowserWorkspace {...props} skills={[skills[1]]}/>);expect(screen.queryByTestId("inspector")).not.toBeInTheDocument();
 });
 it("passes platform row identity to the embedded detail",()=>{
  const duplicates=skills.map((s,i)=>({...s,name:"Same name",detailRequest:{skillId:"same",agentId:"cursor",rowId:`row-${i}`}}));
  render(<SkillBrowserWorkspace {...props} skills={duplicates} agentId="cursor"/>);fireEvent.keyDown(screen.getByRole("region",{name:"技能列表"}),{key:"+"});fireEvent.click(screen.getAllByRole("button",{name:"查看 Same name 的详情"})[1]);expect(screen.getByTestId("inspector")).toHaveTextContent("same|cursor|row-1|overview");
 });
 it("repository location highlights expand and select the target skill",()=>{
  render(<SkillBrowserWorkspace {...props} skills={[{...skills[0],highlighted:true},skills[1]]}/>);expect(screen.getByTestId("inspector")).toHaveTextContent("one");expect(screen.getByRole("button",{name:"查看 First skill 的详情"})).toBeInTheDocument();
 });
 it("supports hiding and restoring the preview and keyboard resizing",()=>{
  render(<SkillBrowserWorkspace {...props}/>);const splitter=screen.getByRole("separator",{name:"调整列表与预览宽度"});fireEvent.keyDown(splitter,{key:"ArrowLeft"});expect(splitter).toHaveAttribute("aria-valuenow","47");fireEvent.click(screen.getByRole("button",{name:"收起预览"}));expect(screen.queryByRole("tablist")).not.toBeInTheDocument();fireEvent.click(screen.getByRole("button",{name:"显示预览"}));expect(screen.getByRole("tablist")).toBeInTheDocument();
 });
 it("numeric group sorting keeps unknown stars last",()=>{
  const more=[...folders,{...folders[0],key:"large",name:"Large",githubStars:100,skillKeys:[]},{...folders[0],key:"unknown",name:"Unknown",githubStars:null,skillKeys:[]}];
  render(<SkillBrowserWorkspace {...props} folders={more} sortField="githubStars" sortDirection="desc"/>);const rows=screen.getAllByRole("row").slice(1);expect(rows[0]).toHaveTextContent("Large");expect(rows[2]).toHaveTextContent("Unknown");
 });
 it("does not render any modal drawer",()=>{render(<SkillBrowserWorkspace {...props}/>);fireEvent.keyDown(screen.getByRole("region",{name:"技能列表"}),{key:"+"});fireEvent.click(screen.getByRole("button",{name:"查看 First skill 的详情"}));expect(screen.queryByRole("dialog")).not.toBeInTheDocument()});
 it("expands and collapses the focused table with plus/minus without consuming text input", () => {
  render(<SkillBrowserWorkspace {...props} toolbar={<input aria-label="filter" />} />);
  const list = screen.getByRole("region", {name:"技能列表"});
  fireEvent.keyDown(list, {key:"+", shiftKey:true});
  expect(screen.getByRole("button",{name:"查看 First skill 的详情"})).toBeInTheDocument();
  fireEvent.keyDown(screen.getByLabelText("filter"), {key:"-"});
  expect(screen.getByRole("button",{name:"查看 First skill 的详情"})).toBeInTheDocument();
  fireEvent.keyDown(list, {key:"-"});
  expect(screen.queryByRole("button",{name:"查看 First skill 的详情"})).not.toBeInTheDocument();
  fireEvent.keyDown(list, {key:"+", code:"NumpadAdd"});
  expect(screen.getByRole("button",{name:"查看 First skill 的详情"})).toBeInTheDocument();
 });
 it("shows folder metadata above a separate skill list without scrolling on selection", () => {
  const scroll = vi.fn();
  const original = HTMLElement.prototype.scrollIntoView;
  HTMLElement.prototype.scrollIntoView = scroll;
  try {
    render(<SkillBrowserWorkspace {...props} folders={[{...folders[0], createdAt:"2026-09-01T00:00:00Z", updatedAt:"2026-09-02T00:00:00Z"}]} />);
    fireEvent.click(screen.getByRole("button",{name:"Example/Tools"}));
    const metadata = screen.getByRole("region",{name:"基本信息"});
    expect(within(metadata).getByRole("link",{name:"Example/Tools"})).toHaveAttribute("href","https://github.com/Example/Tools");
    expect(within(metadata).getByText("/demo/tools")).toBeInTheDocument();
    const list = screen.getByRole("region",{name:"技能清单"});
    expect(within(list).getAllByRole("columnheader").map(header => header.getAttribute("aria-label"))).toEqual(["序号", "名称"]);
    expect(within(list).getByRole("cell", {name:"1"})).toBeInTheDocument();
    expect(metadata.compareDocumentPosition(list) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    expect(scroll).not.toHaveBeenCalled();
  } finally { HTMLElement.prototype.scrollIntoView = original; }
 });
 it("opens icon actions with the keyboard, preserves confirmation, and closes on Escape", async () => {
  render(<SkillBrowserWorkspace {...props} />);
  fireEvent.keyDown(screen.getByRole("region",{name:"技能列表"}),{key:"+"});
  const row=screen.getByRole("button",{name:"查看 First skill 的详情"}).closest("tr")!;
  fireEvent.keyDown(row,{key:"F10",shiftKey:true});
  const menu=screen.getByRole("menu",{name:"First skill"});
  const action=within(menu).getByRole("menuitem",{name:"删除"});
  expect(action.querySelector("svg")).toBeInTheDocument();
  fireEvent.click(action);expect(remove).not.toHaveBeenCalled();
  fireEvent.click(within(menu).getByRole("menuitem",{name:"确认删除"}));
  expect(remove).toHaveBeenCalledOnce();
  await waitFor(() => expect(screen.queryByRole("menu")).not.toBeInTheDocument());
  fireEvent.contextMenu(row);
  fireEvent.keyDown(screen.getByRole("menu"),{key:"Escape"});
  expect(screen.queryByRole("menu")).not.toBeInTheDocument();expect(row).toHaveFocus();
 });
 it("keeps unavailable folder update disabled and dismisses on outside click", () => {
  render(<SkillBrowserWorkspace {...props} folders={[{...folders[0],updateLabel:"更新"}]} />);
  const menu=openRowActions(screen.getByRole("button",{name:"Example/Tools"}).closest("tr")!);
  expect(menu.getByRole("menuitem",{name:"更新"})).toBeDisabled();
  fireEvent.pointerDown(document.body);
  expect(screen.queryByRole("menu")).not.toBeInTheDocument();
 });
});

it("uses detected enabled targets in overview installation statistics and links to their pages", () => {
 const targets = [{id:"cursor",display_name:"Cursor",global_skills_dir:"demo",is_detected:true,is_enabled:true,is_builtin:true}, {id:"missing",display_name:"Missing",global_skills_dir:"demo",is_detected:false,is_enabled:true,is_builtin:true}, {id:"disabled",display_name:"Disabled",global_skills_dir:"demo",is_detected:true,is_enabled:false,is_builtin:true}];
 render(<MemoryRouter><SkillBrowserWorkspace {...props} skills={[{...skills[0],isCentral:true,installAgents:targets}]}/></MemoryRouter>);
 fireEvent.keyDown(screen.getByRole("region",{name:"技能列表"}),{key:"+"});
 fireEvent.click(screen.getByRole("button",{name:"查看 First skill 的详情"}));
 expect(screen.queryByRole("tab",{name:"安装"})).not.toBeInTheDocument();
 expect(screen.getByRole("region",{name:"安装统计"})).toBeVisible();
 expect(screen.getByRole("link",{name:/Cursor/})).toHaveAttribute("href","/platform/cursor");
 expect(screen.queryByRole("link",{name:/Missing|Disabled/})).not.toBeInTheDocument();
});

it("expands only the focused folder with Right and collapses it with Left", () => {
 render(<SkillBrowserWorkspace {...props} folders={[...folders,{key:"other",name:"Other",path:"",skillCount:0,onOpen:vi.fn()}]}/>);
 const list=within(screen.getByRole("region",{name:"技能列表"}));
 const button=screen.getByRole("button",{name:"Example/Tools"});
 const row=button.closest("tr")!;
 fireEvent.click(row);
 expect(row).toHaveFocus();
 fireEvent.keyDown(row,{key:"ArrowRight"});
 expect(list.getByRole("button",{name:"查看 First skill 的详情"})).toBeInTheDocument();
 expect(screen.getByRole("button",{name:"展开 Other"})).toBeInTheDocument();
 fireEvent.keyDown(row,{key:"ArrowRight"});
 expect(list.getByRole("button",{name:"查看 First skill 的详情"})).toBeInTheDocument();
 fireEvent.keyDown(row,{key:"ArrowLeft"});
 expect(list.queryByRole("button",{name:"查看 First skill 的详情"})).not.toBeInTheDocument();
 // Clicking the name leaves browser focus on the button; arrow navigation must move it to the row.
 fireEvent.click(button);
 button.focus();
 fireEvent.keyDown(button,{key:"ArrowRight"});
 expect(row).toHaveFocus();
 expect(button).not.toHaveFocus();
 expect(row).toHaveAttribute("aria-selected","true");
 expect(list.getByRole("button",{name:"查看 First skill 的详情"})).toBeInTheDocument();
 button.focus();
 fireEvent.keyDown(button,{key:"ArrowLeft"});
 expect(row).toHaveFocus();
 expect(list.queryByRole("button",{name:"查看 First skill 的详情"})).not.toBeInTheDocument();
});

it("moves selection, focus and details through visible rows with Up and Down", () => {
 render(<SkillBrowserWorkspace {...props}/>);
 const list=within(screen.getByRole("region",{name:"技能列表"}));
 const folder=list.getByRole("button",{name:"Example/Tools"}).closest("tr")!;
 fireEvent.click(folder);
 fireEvent.keyDown(folder,{key:"ArrowRight"});
 fireEvent.keyDown(folder,{key:"ArrowDown"});
 const first=list.getByRole("button",{name:"查看 First skill 的详情"}).closest("tr")!;
 expect(first).toHaveFocus();
 expect(first).toHaveAttribute("aria-selected","true");
 expect(screen.getByTestId("inspector")).toHaveTextContent("one");
 fireEvent.keyDown(first,{key:"ArrowDown"});
 const second=list.getByRole("button",{name:"查看 Second skill 的详情"}).closest("tr")!;
 expect(second).toHaveFocus();
 expect(screen.getByTestId("inspector")).toHaveTextContent("two");
 fireEvent.keyDown(second,{key:"ArrowDown"});
 expect(second).toHaveFocus();
 fireEvent.keyDown(second,{key:"ArrowUp"});
 expect(first).toHaveFocus();
 fireEvent.keyDown(first,{key:"ArrowUp"});
 expect(folder).toHaveFocus();
 fireEvent.keyDown(folder,{key:"ArrowLeft"});
 fireEvent.keyDown(folder,{key:"ArrowDown"});
 expect(folder).toHaveFocus();
 expect(list.queryByRole("button",{name:"查看 First skill 的详情"})).not.toBeInTheDocument();
});

it("uses Home and End within the selected row's folder or across folder rows", () => {
 const otherSkill: SkillTableItem = {rowKey:"other-skill", name:"Other skill", detailRequest:{skillId:"other-skill"}};
 render(<SkillBrowserWorkspace {...props} skills={[...skills,otherSkill]} folders={[
  {key:"before",name:"A Before",path:"",skillCount:0,onOpen:vi.fn()},
  ...folders,
  {key:"after",name:"Z After",path:"",skillCount:1,skillKeys:["other-skill"],onOpen:vi.fn()},
 ]}/>);
 fireEvent.keyDown(screen.getByRole("region",{name:"技能列表"}),{key:"+"});
 const list=within(screen.getByRole("region",{name:"技能列表"}));
 const first=list.getByRole("button",{name:"查看 First skill 的详情"}).closest("tr")!;
 const second=list.getByRole("button",{name:"查看 Second skill 的详情"}).closest("tr")!;
 fireEvent.click(second);
 fireEvent.keyDown(second,{key:"Home"});
 expect(first).toHaveFocus();
 expect(first).toHaveAttribute("aria-selected","true");
 expect(screen.getByTestId("inspector")).toHaveTextContent("one");
 fireEvent.keyDown(first,{key:"End"});
 expect(second).toHaveFocus();
 expect(second).toHaveAttribute("aria-selected","true");
 expect(screen.getByTestId("inspector")).toHaveTextContent("two");
 fireEvent.keyDown(second,{key:"End"});
 expect(second).toHaveFocus();
 const middle=list.getByRole("button",{name:"Example/Tools"}).closest("tr")!;
 const before=list.getByRole("button",{name:"A Before"}).closest("tr")!;
 const after=list.getByRole("button",{name:"Z After"}).closest("tr")!;
 fireEvent.click(middle);
 fireEvent.keyDown(middle,{key:"Home"});
 expect(before).toHaveFocus();
 expect(before).toHaveAttribute("aria-selected","true");
 expect(screen.getByRole("heading",{name:"A Before"})).toBeInTheDocument();
 fireEvent.keyDown(before,{key:"End"});
 expect(after).toHaveFocus();
 expect(after).toHaveAttribute("aria-selected","true");
 expect(screen.getByRole("heading",{name:"Z After"})).toBeInTheDocument();
 const single=list.getByRole("button",{name:"查看 Other skill 的详情"}).closest("tr")!;
 fireEvent.click(single);
 fireEvent.keyDown(single,{key:"Home"});
 expect(single).toHaveFocus();
 fireEvent.keyDown(single,{key:"End"});
 expect(single).toHaveFocus();
});

it("does not scroll back to an old located folder when selecting a later folder's skill", () => {
 const scroll = vi.fn();
 const original = Object.getOwnPropertyDescriptor(HTMLElement.prototype, "scrollIntoView");
 Object.defineProperty(HTMLElement.prototype,"scrollIntoView",{configurable:true,value:scroll});
 try {
  const before: FolderTableItem = {key:"before",name:"A Before",path:"",skillCount:1,skillKeys:["before-skill"],highlighted:true,onOpen:vi.fn()};
  render(<SkillBrowserWorkspace {...props} folders={[before,...folders]} skills={[
   {rowKey:"before-skill",name:"Earlier skill",detailRequest:{skillId:"before-skill"}},...skills,
  ]}/>);
  expect(scroll).toHaveBeenCalled();
  fireEvent.keyDown(screen.getByRole("region",{name:"技能列表"}),{key:"+"});
  scroll.mockClear();
  const list=within(screen.getByRole("region",{name:"技能列表"}));
  fireEvent.click(list.getByRole("button",{name:"查看 Second skill 的详情"}));
  expect(screen.getByTestId("inspector")).toHaveTextContent("two");
  expect(list.getByRole("button",{name:"查看 Second skill 的详情"}).closest("tr")).toHaveAttribute("aria-selected","true");
  expect(list.getByRole("button",{name:"查看 Earlier skill 的详情"})).toBeInTheDocument();
  expect(scroll).not.toHaveBeenCalled();
 } finally {
  if (original) Object.defineProperty(HTMLElement.prototype,"scrollIntoView",original);
  else Reflect.deleteProperty(HTMLElement.prototype,"scrollIntoView");
 }
});

it("collapses a skill's parent and moves selection, focus and preview to that folder on Left", () => {
 render(<SkillBrowserWorkspace {...props} folders={[{key:"before",name:"Other",path:"",skillCount:0,onOpen:vi.fn()},...folders]}/>);
 const list=within(screen.getByRole("region",{name:"技能列表"}));
 fireEvent.click(list.getByRole("button",{name:"展开 Example/Tools"}));
 const skill=list.getByRole("button",{name:"查看 Second skill 的详情"});
 fireEvent.click(skill);
 fireEvent.keyDown(skill,{key:"ArrowLeft"});
 const parent=list.getByRole("button",{name:"Example/Tools"}).closest("tr")!;
 expect(parent).toHaveFocus();
 expect(parent).toHaveAttribute("aria-selected","true");
 expect(list.getByRole("button",{name:"展开 Example/Tools"})).toHaveAttribute("aria-expanded","false");
 expect(list.queryByRole("button",{name:"查看 Second skill 的详情"})).not.toBeInTheDocument();
 expect(screen.getByRole("heading",{name:"Example/Tools"})).toBeInTheDocument();
 expect(screen.queryByTestId("inspector")).not.toBeInTheDocument();
});

it("shows a repository README and retains the document tab between repository and skill selections", async () => {
  const read = vi.spyOn(useRepositoryDocumentStore.getState(), "readDocuments").mockResolvedValue([{name:"README.md",language:"default",content:"# Repository guide"}]);
  try {
    render(<SkillBrowserWorkspace {...props}/>);
    fireEvent.click(screen.getByRole("button", {name:"Example/Tools"}));
    fireEvent.click(screen.getByRole("tab", {name:"文档"}));
    expect(await screen.findByRole("heading", {name:"Repository guide"})).toBeInTheDocument();
    expect(read).toHaveBeenCalledWith("/demo/tools", "one");
    expect(screen.queryByRole("region", {name:"技能清单"})).not.toBeInTheDocument();
    fireEvent.keyDown(screen.getByRole("region", {name:"技能列表"}), {key:"+"});
    fireEvent.click(screen.getByRole("button", {name:"查看 First skill 的详情"}));
    expect(screen.getByTestId("inspector")).toHaveTextContent("one|||document");
    fireEvent.click(screen.getByRole("button", {name:"Example/Tools"}));
    expect(await screen.findByRole("heading", {name:"Repository guide"})).toBeInTheDocument();
  } finally { read.mockRestore(); }
});
it("highlights the selected repository tags without changing filters and clears them on unmount", () => {
  const change = vi.fn();
  const {unmount} = render(<><TagFilters tags={[{key:"a",label:"A"},{key:"b",label:"B"}]} selected={[]} onChange={change}/>
    <SkillBrowserWorkspace {...props} folders={[{...folders[0],tags:["A"]},{...folders[0],key:"other",name:"Other",tags:["B"],skillKeys:[]}]}/></>);
  fireEvent.click(screen.getByRole("button",{name:"Example/Tools"}));
  expect(screen.getByRole("button",{name:"A"})).toHaveAttribute("data-highlighted","true");
  expect(screen.queryByRole("region",{name:"技能标签"})).not.toBeInTheDocument();
  expect(screen.getByRole("button",{name:"A"})).toHaveAttribute("aria-pressed","false");
  fireEvent.click(screen.getByRole("button",{name:"Other"}));
  expect(screen.getByRole("button",{name:"A"})).not.toHaveAttribute("data-highlighted");
  expect(screen.getByRole("button",{name:"B"})).toHaveAttribute("data-highlighted","true");
  expect(change).not.toHaveBeenCalled();
  unmount();
  expect(useRepositoryDocumentStore.getState().highlightedTags).toEqual([]);
});

it.each(["repositories", "skills", "mixed"])("highlights the union of %s selections and updates it when deselecting", mode => {
  const taggedSkills = [
    {...skills[0], tags:[{key:"a",label:"A"},{key:"shared",label:"Shared"}]},
    {...skills[1], tags:[{key:"b",label:"B"},{key:"shared",label:"Shared"}]},
  ];
  const taggedFolders = [
    {...folders[0],tags:["A","Shared"],skillKeys:["one"]},
    {...folders[0],key:"other",name:"Other",tags:["B","Shared"],skillKeys:["two"]},
  ];
  render(<><TagFilters tags={[{key:"a",label:"A"},{key:"b",label:"B"},{key:"shared",label:"Shared"}]} selected={[]} onChange={vi.fn()}/>
    <SkillBrowserWorkspace {...props} skills={taggedSkills} folders={taggedFolders}/></>);
  if (mode !== "repositories") fireEvent.keyDown(screen.getByRole("region",{name:"技能列表"}),{key:"+"});
  const first = mode === "skills" ? "查看 First skill 的详情" : "Example/Tools";
  const second = mode === "repositories" ? "Other" : "查看 Second skill 的详情";
  fireEvent.click(screen.getAllByRole("button",{name:first})[0]);
  fireEvent.click(screen.getAllByRole("button",{name:second})[0],{ctrlKey:true});
  for (const tag of ["A","B","Shared"]) expect(screen.getByRole("button",{name:tag})).toHaveAttribute("data-highlighted","true");
  fireEvent.click(screen.getAllByRole("button",{name:first})[0],{ctrlKey:true});
  expect(screen.getByRole("button",{name:"A"})).not.toHaveAttribute("data-highlighted");
  for (const tag of ["B","Shared"]) expect(screen.getByRole("button",{name:tag})).toHaveAttribute("data-highlighted","true");
  fireEvent.click(screen.getAllByRole("button",{name:second})[0],{metaKey:true});
  for (const tag of ["A","B","Shared"]) expect(screen.getByRole("button",{name:tag})).not.toHaveAttribute("data-highlighted");
});
it("updates range selection tags after selected skills are filtered out", () => {
  const taggedSkills = skills.map((skill,index) => ({...skill,tags:[{key:String(index),label:String(index)}]}));
  const {rerender} = render(<SkillBrowserWorkspace {...props} skills={taggedSkills}/>);
  fireEvent.keyDown(screen.getByRole("region",{name:"技能列表"}),{key:"+"});
  fireEvent.click(screen.getByRole("button",{name:"查看 First skill 的详情"}));
  fireEvent.click(screen.getByRole("button",{name:"查看 Second skill 的详情"}),{shiftKey:true});
  expect(useRepositoryDocumentStore.getState().highlightedTags).toEqual(["0","1"]);
  rerender(<SkillBrowserWorkspace {...props} skills={[taggedSkills[1]]}/>);
  expect(useRepositoryDocumentStore.getState().highlightedTags).toEqual(["1"]);
  rerender(<SkillBrowserWorkspace {...props} skills={[]} folders={[]}/>);
  expect(useRepositoryDocumentStore.getState().highlightedTags).toEqual([]);
});

it("updates the tag union when selecting and deselecting skills in the repository preview list", () => {
  const taggedSkills = skills.map((skill,index) => ({...skill,tags:[{key:String(index),label:String(index)}]}));
  render(<SkillBrowserWorkspace {...props} skills={taggedSkills}/>);
  fireEvent.click(screen.getByRole("button",{name:"Example/Tools"}));
  const list = within(screen.getByRole("region",{name:"技能清单"}));
  fireEvent.click(list.getByRole("button",{name:"查看 First skill 的详情"}),{ctrlKey:true});
  expect(useRepositoryDocumentStore.getState().highlightedTags).toEqual(["0"]);
  fireEvent.click(list.getByRole("button",{name:"查看 Second skill 的详情"}),{ctrlKey:true});
  expect(useRepositoryDocumentStore.getState().highlightedTags).toEqual(["0","1"]);
  fireEvent.click(list.getByRole("button",{name:"查看 First skill 的详情"}),{ctrlKey:true});
  expect(useRepositoryDocumentStore.getState().highlightedTags).toEqual(["1"]);
  fireEvent.click(list.getByRole("button",{name:"查看 Second skill 的详情"}),{ctrlKey:true});
  expect(useRepositoryDocumentStore.getState().highlightedTags).toEqual([]);
});

it("restores an obsolete install tab preference to overview and shows repository installation statistics", () => {
 localStorage.setItem("skillshub.browser.tab",JSON.stringify("install"));
 render(<SkillBrowserWorkspace {...props}/>);
 fireEvent.click(screen.getByRole("button",{name:"Example/Tools"}));
 expect(screen.getByRole("tab",{name:"概览"})).toHaveAttribute("aria-selected","true");
 expect(screen.queryByRole("tab",{name:"安装"})).not.toBeInTheDocument();
 expect(screen.getByRole("region",{name:"安装统计"})).toBeVisible();
 fireEvent.click(screen.getByRole("tab",{name:"文档"}));
 expect(screen.queryByRole("region",{name:"安装统计"})).not.toBeInTheDocument();
});

it("exposes the notebook tab only on repository rows", () => {
  render(<SkillBrowserWorkspace {...props}/>);
  fireEvent.click(screen.getByRole("button", {name: "Example/Tools"}));
  fireEvent.click(screen.getByRole("tab", {name: "笔记"}));
  expect(screen.getByTestId("repository-notebook")).toHaveTextContent("one");
  fireEvent.keyDown(screen.getByRole("region", {name: "技能列表"}), {key: "+"});
  fireEvent.click(screen.getByRole("button", {name: "查看 First skill 的详情"}));
  expect(screen.queryByRole("tab", {name: "笔记"})).not.toBeInTheDocument();
  expect(screen.getByTestId("inspector")).toHaveTextContent("overview");
});
it("keeps notebooks out of collection rows", () => {
  render(<SkillBrowserWorkspace {...props} collectionsMode/>);
  fireEvent.click(screen.getByRole("button", {name: "Example/Tools"}));
  expect(screen.queryByRole("tab", {name: "笔记"})).not.toBeInTheDocument();
  expect(screen.queryByRole("columnheader", {name: "笔记"})).not.toBeInTheDocument();
});

it("does not treat the loose local-skills grouping as a repository notebook", () => {
  render(<SkillBrowserWorkspace storageKey="local" skills={[{rowKey: "local", name: "Loose local skill"}]} />);
  fireEvent.click(screen.getByRole("button", {name: "本地技能"}));
  expect(screen.queryByRole("tab", {name: "笔记"})).not.toBeInTheDocument();
});

it('keeps overview, document and notebook available in a narrow collapsed rail',()=>{
 render(<SkillBrowserWorkspace {...props}/>);
 fireEvent.click(screen.getByRole('button',{name:'Example/Tools'}));
 fireEvent.click(screen.getByRole('button',{name:'收起预览'}));
 const rail=screen.getByRole('complementary',{name:'详情预览'});
 expect(rail).toHaveStyle({width:'40px'});
 expect(within(rail).getByRole('button',{name:'概览'})).toBeEnabled();
 expect(within(rail).getByRole('button',{name:'文档'})).toBeEnabled();
 expect(within(rail).getByRole('button',{name:'笔记'})).toBeEnabled();
 fireEvent.click(within(rail).getByRole('button',{name:'笔记'}));
 expect(screen.queryByRole('tablist')).not.toBeInTheDocument();
 fireEvent.click(within(rail).getByRole('button',{name:'显示预览'}));
 expect(screen.getByRole('tab',{name:'笔记'})).toHaveAttribute('aria-selected','true');
});

it('inherits GitHot range selection and select-all keyboard behavior',()=>{
 const selection=vi.fn();render(<SkillBrowserWorkspace {...props} forceFlat onSelectionChange={selection}/>);
 const first=screen.getByRole('button',{name:'查看 First skill 的详情'}).closest('tr')!;
 const second=screen.getByRole('button',{name:'查看 Second skill 的详情'}).closest('tr')!;
 fireEvent.click(first);fireEvent.keyDown(first,{key:'ArrowDown',shiftKey:true});
 expect(first).toHaveAttribute('aria-selected','true');expect(second).toHaveAttribute('aria-selected','true');expect(second).toHaveFocus();
 fireEvent.click(first);fireEvent.keyDown(first,{key:'a',ctrlKey:true});expect(second).toHaveAttribute('aria-selected','true');
 expect(selection).toHaveBeenLastCalledWith(['skill:one','skill:two']);
});

it('keeps preview collapsed on selection and across workspace remounts',()=>{
 const view=render(<SkillBrowserWorkspace {...props}/>);
 fireEvent.click(screen.getByRole('button',{name:'收起预览'}));
 fireEvent.click(screen.getByRole('button',{name:'Example/Tools'}));
 expect(screen.queryByRole('tablist')).not.toBeInTheDocument();
 view.unmount();
 render(<SkillBrowserWorkspace {...props} storageKey="other"/>);
 expect(screen.queryByRole('tablist')).not.toBeInTheDocument();
 expect(screen.getByRole('button',{name:'显示预览'})).toBeInTheDocument();
});

it('drags a collapsed repository as all of its skill members',()=>{
 const drag=vi.fn();render(<SkillBrowserWorkspace {...props} onDragSkills={drag}/>);
 const row=screen.getByRole('button',{name:'Example/Tools'}).closest('tr')!;
 fireEvent.dragStart(row,{dataTransfer:{setData:vi.fn()}});
 expect(drag).toHaveBeenCalledWith(expect.anything(),['one','two']);
});
it('detaches selected skill rows in one group operation',()=>{
 const detach=vi.fn();render(<SkillBrowserWorkspace {...props} forceFlat onDetachSkills={detach}/>);
 const row=screen.getByRole('button',{name:'查看 First skill 的详情'}).closest('tr')!;
 fireEvent.click(row);fireEvent.keyDown(row,{key:'a',ctrlKey:true});
 fireEvent.click(openRowActions(row).getByRole('menuitem',{name:'移出分组'}));
 expect(detach).toHaveBeenCalledWith(['one','two']);
});
