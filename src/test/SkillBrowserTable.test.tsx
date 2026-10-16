import { openRowActions } from "./rowActions";
import { act, fireEvent, render, screen, within, waitFor } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";

import { SkillBrowserTable } from "@/components/skill/SkillBrowserTable";

describe("SkillBrowserTable", () => {
  it("renders selected skill columns and actions", () => {
    const onDetail = vi.fn();
    render(
      <SkillBrowserTable
        kind="skill"
        visibleColumns={new Set(["name", "source", "notes", "actions"])}
        skills={[
          {
            rowKey: "one",
            name: "api-skill",
            description: "Design APIs",
            notes: "Important internal note",
            sourceRepo: "owner/repo",
            onDetail,
          },
        ]}
      />
    );

    expect(screen.getByRole("columnheader", { name: "序号" })).toBeInTheDocument();
    expect(screen.getByRole("columnheader", { name: "序号" })).toBeInTheDocument();
    expect(screen.getByRole("columnheader", { name: "名称" })).toBeInTheDocument();
    expect(screen.queryByRole("columnheader", { name: "创建时间" })).not.toBeInTheDocument();
    expect(screen.queryByText("owner/repo")).not.toBeInTheDocument();
    expect(screen.queryByText("Important internal note")).not.toBeInTheDocument();
    expect(screen.queryByText("Design APIs")).not.toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "查看 api-skill 的详情" }));
    expect(onDetail).toHaveBeenCalled();
  });

  it("shows a leftmost index column for skills and folders", () => {
    const { rerender } = render(
      <SkillBrowserTable
        kind="skill"
        visibleColumns={new Set(["name", "actions"])}
        skills={[
          { rowKey: "one", name: "api-skill" },
          { rowKey: "two", name: "cli-skill" },
        ]}
      />
    );

    expect(
      screen.getAllByRole("columnheader").map((header) => header.getAttribute("aria-label"))
    ).toEqual(["序号", "名称"]);
    expect(screen.getByRole("cell", { name: "1" })).toBeInTheDocument();
    expect(screen.getByRole("cell", { name: "2" })).toBeInTheDocument();

    rerender(
      <SkillBrowserTable
        kind="folder"
        visibleColumns={new Set(["name", "actions"])}
        folders={[
          {
            key: "owner/repo",
            name: "owner/repo",
            path: "D:/Skills/owner/repo",
            skillCount: 3,
            onOpen: vi.fn(),
          },
          {
            key: "other/repo",
            name: "other/repo",
            path: "D:/Skills/other/repo",
            skillCount: 1,
            onOpen: vi.fn(),
          },
        ]}
      />
    );

    expect(
      screen.getAllByRole("columnheader").map((header) => header.getAttribute("aria-label"))
    ).toEqual(["序号", "名称"]);
    expect(screen.getByRole("cell", { name: "1" })).toBeInTheDocument();
    expect(screen.getByRole("cell", { name: "2" })).toBeInTheDocument();
  });

  it("sorts from sortable column headers and keeps the actions header left aligned", () => {
    const onSortChange = vi.fn();
    render(
      <SkillBrowserTable
        kind="skill"
        visibleColumns={new Set(["name", "source", "createdAt", "updatedAt", "actions"])}
        sortField="name"
        sortDirection="asc"
        onSortChange={onSortChange}
        skills={[
          {
            rowKey: "one",
            name: "api-skill",
            sourceRepo: "owner/repo",
          },
        ]}
      />
    );

    fireEvent.click(screen.getByRole("button", { name: "创建时间" }));
    expect(onSortChange).toHaveBeenCalledWith("createdAt", "asc");
    expect(screen.getByRole("button", { name: "创建时间" }).querySelector("svg")).toHaveClass("lucide-arrow-up-down");

    fireEvent.click(screen.getByRole("button", { name: "名称，升序排序" }));
    expect(onSortChange).toHaveBeenCalledWith("name", "desc");
    expect(screen.getByRole("button", { name: "名称，升序排序" }).querySelector("svg")).toHaveClass("lucide-arrow-up");

    expect(screen.queryByRole("columnheader", { name: "操作" })).not.toBeInTheDocument();
  });

  it("opens column settings from a header context menu", () => {
    const onToggleColumn = vi.fn();
    render(
      <SkillBrowserTable
        kind="folder"
        visibleColumns={new Set(["name", "skillCount", "actions"])}
        onToggleColumn={onToggleColumn}
        onResetColumns={vi.fn()}
        folders={[
          {
            key: "owner/repo",
            name: "owner/repo",
            path: "D:/Skills/owner/repo",
            skillCount: 3,
            onOpen: vi.fn(),
          },
        ]}
      />
    );

    fireEvent.contextMenu(screen.getByRole("columnheader", { name: "名称" }));
    expect(screen.queryByRole("checkbox", { name: "路径" })).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole("checkbox", { name: "技能数" }));

    expect(onToggleColumn).toHaveBeenCalledWith("skillCount");
  });

  it("does not render the folder path column in directory views", () => {
    render(
      <SkillBrowserTable
        kind="folder"
        visibleColumns={new Set(["name", "path", "skillCount"])}
        folders={[
          {
            key: "owner/repo",
            name: "addyosmani/agent-skills",
            path: "D:\\Data\\Codes\\AI\\Skills\\addyosmani\\agent-skills",
            skillCount: 24,
            onOpen: vi.fn(),
          },
        ]}
      />
    );

    expect(screen.queryByRole("columnheader", { name: "路径" })).not.toBeInTheDocument();
    expect(screen.queryByText("D:\\Data\\Codes\\AI\\Skills\\addyosmani\\agent-skills")).not.toBeInTheDocument();
    expect(screen.getByText("24")).toBeInTheDocument();
  });

  it("keeps table headers sticky while the page scrolls", () => {
    render(
      <SkillBrowserTable
        kind="skill"
        visibleColumns={new Set(["name", "source", "actions"])}
        skills={[{ rowKey: "one", name: "api-skill", sourceRepo: "owner/repo" }]}
      />
    );

    expect(screen.getByRole("columnheader", { name: "名称" }).closest("thead")).toHaveClass(
      "sticky",
      "top-[var(--skill-table-sticky-top)]"
    );
  });

  it("shows only aggregate counts in the install summary column", () => {
    const onToggle = vi.fn();
    render(
      <SkillBrowserTable
        kind="skill"
        visibleColumns={new Set(["name", "installSummary"])}
        skills={[
          {
            rowKey: "one",
            name: "api-skill",
            platformIcons: {
              agents: [
                {
                  id: "claude-code",
                  display_name: "Claude Code",
                  global_skills_dir: "C:/Users/alice/.claude/skills",
                  is_detected: true,
                  is_builtin: true,
                  is_enabled: true,
                },
              ],
              linkedAgents: ["claude-code"],
              readOnlyAgents: [],
              skillId: "skill-1",
              onToggle,
              togglingAgentId: null,
            },
          },
        ]}
      />
    );

    expect(screen.getByText("独立安装：平台 1 · 项目 0")).toBeInTheDocument();
    expect(screen.queryByText(/共享中心：平台/)).not.toBeInTheDocument();
    expect(
      screen.queryByRole("button", { name: "切换 api-skill 在 Claude Code 的链接状态" })
    ).not.toBeInTheDocument();
    expect(onToggle).not.toHaveBeenCalled();
  });

  it("splits install summary into platforms, projects, and shared targets", () => {
    render(
      <SkillBrowserTable
        kind="skill"
        visibleColumns={new Set(["name", "installSummary"])}
        skills={[
          {
            rowKey: "one",
            name: "api-skill",
            installAgents: [
              {
                id: "claude-code",
                display_name: "Claude Code",
                global_skills_dir: "C:/Users/alice/.claude/skills",
                is_detected: true,
                is_builtin: true,
                is_enabled: true,
              },
              {
                id: "project:1",
                display_name: "Demo",
                global_skills_dir: "C:/Projects/Demo/.agents/skills",
                is_detected: true,
                is_builtin: false,
                is_enabled: true,
              },
              {
                id: "hermes",
                display_name: "Hermes",
                global_skills_dir: "C:/Users/alice/.agents/skills",
                is_detected: true,
                is_builtin: true,
                is_enabled: true,
                shares_central_skills: true,
              },
            ],
            installLinkedAgentIds: ["claude-code", "project:1"],
            installReadOnlyAgentIds: ["hermes"],
          },
        ]}
      />
    );

    expect(screen.getByText("独立安装：平台 1 · 项目 1")).toBeInTheDocument();
    expect(screen.queryByText(/共享中心：平台/)).not.toBeInTheDocument();
    expect(screen.getByLabelText(/平台：Claude Code/)).toHaveAttribute(
      "title",
      expect.stringContaining("项目：Demo")
    );
  });

  it("keeps platform source details out of the installation summary column", () => {
    render(
      <SkillBrowserTable
        kind="skill"
        visibleColumns={new Set(["name", "installSummary"])}
        skills={[
          {
            rowKey: "one",
            name: "api-skill",
            sourceType: "symlink",
            sourceLocation: "central",
          },
        ]}
      />
    );

    expect(screen.getByText("未安装")).toBeInTheDocument();
    expect(screen.queryByText(/共享中心：平台/)).not.toBeInTheDocument();
    expect(screen.queryByText("共享中心")).not.toBeInTheDocument();
    expect(screen.queryByText("符号链接")).not.toBeInTheDocument();
  });

  it("renders the four resource actions with paired plus and minus icons", () => {
    render(
      <SkillBrowserTable
        kind="skill"
        visibleColumns={new Set(["name", "actions"])}
        skills={[
          {
            rowKey: "available",
            name: "available",
            isCentral: false,
            onInstallToCentral: vi.fn(),
            installToCentralLabel: "加入共享中心",
            onInstallTo: vi.fn(),
            installToLabel: "安装",
            onUpdateFromSource: vi.fn(),
            updateFromSourceLabel: "更新",
            onDeleteFromCentral: vi.fn(),
            deleteFromCentralLabel: "删除",
          },
          {
            rowKey: "installed",
            name: "installed",
            isCentral: true,
            onRemoveFromCentral: vi.fn(),
            removeFromCentralLabel: "从共享中心移除",
            onUninstallFromPlatform: vi.fn(),
            uninstallFromLabel: "卸载",
            onUpdateFromSource: vi.fn(),
            updateFromSourceLabel: "更新",
            onDeleteFromCentral: vi.fn(),
            deleteFromCentralLabel: "删除",
          },
        ]}
      />
    );

    const availableRow = screen.getByRole("row", { name: /available/ });
    expect(openRowActions(availableRow).getByRole("menuitem", { name: "加入共享中心" })).toBeInTheDocument();
    expect(openRowActions(availableRow).getByRole("menuitem", { name: "安装" })).toBeInTheDocument();
    expect(screen.getByRole("menu").querySelector(".lucide-plus")).toBeInTheDocument();
    expect(screen.getByRole("menu").querySelector(".lucide-package-plus")).toBeInTheDocument();

    const installedRow = screen.getByRole("row", { name: /installed/ });
    expect(openRowActions(installedRow).getByRole("menuitem", { name: "从共享中心移除" })).toBeInTheDocument();
    expect(openRowActions(installedRow).getByRole("menuitem", { name: "卸载" })).toBeInTheDocument();
    expect(screen.getByRole("menu").querySelector(".lucide-minus")).toBeInTheDocument();
    expect(screen.getByRole("menu").querySelector(".lucide-package-minus")).toBeInTheDocument();
  });

  it("resizes a column from the header resize handle", () => {
    render(
      <SkillBrowserTable
        kind="skill"
        visibleColumns={new Set(["name", "source", "actions"])}
        skills={[{ rowKey: "one", name: "api-skill", sourceRepo: "owner/repo" }]}
      />
    );

    const handle = screen.getByRole("separator", { name: "调整 名称 列宽" });
    fireEvent.pointerDown(handle, { clientX: 100 });
    act(() => {
      document.dispatchEvent(new PointerEvent("pointermove", { clientX: 160 }));
      document.dispatchEvent(new PointerEvent("pointerup"));
    });

    const nameColumn = document.querySelectorAll("colgroup col")[1];
    expect(nameColumn).toHaveStyle({
      width: "444px",
    });

    fireEvent.doubleClick(handle);
    const fittedWidth = Number.parseInt(nameColumn.getAttribute("style")?.match(/width:\s*(\d+)/)?.[1] ?? "0", 10);
    expect(fittedWidth).toBeGreaterThanOrEqual(80);
    expect(fittedWidth).toBeLessThan(200);
  });

  it("lets the index column be resized below the default column minimum", () => {
    window.localStorage.removeItem("skills-manage.skillTableColumnWidths.skill");
    render(
      <SkillBrowserTable
        kind="skill"
        visibleColumns={new Set(["name", "actions"])}
        skills={[{ rowKey: "one", name: "api-skill" }]}
      />
    );

    const indexColumn = document.querySelector("colgroup col");
    expect(indexColumn).toHaveStyle({ width: "40px" });

    const handle = screen.getByRole("separator", { name: "调整 序号 列宽" });
    fireEvent.pointerDown(handle, { clientX: 100 });
    act(() => {
      document.dispatchEvent(new PointerEvent("pointermove", { clientX: 70 }));
      document.dispatchEvent(new PointerEvent("pointerup"));
    });

    expect(indexColumn).toHaveStyle({ width: "32px" });
  });

  it("renders folder rows and invokes folder actions", () => {
    const onOpen = vi.fn();
    const onDelete = vi.fn();
    render(
      <SkillBrowserTable
        kind="folder"
        visibleColumns={new Set(["name", "skillCount", "actions"])}
        folders={[
          {
            key: "owner/repo",
            name: "owner/repo",
            path: "D:/Skills/owner/repo",
            skillCount: 3,
            onOpen,
            onDelete,
            deleteLabel: "删除目录 owner/repo",
          },
        ]}
      />
    );

    fireEvent.click(screen.getByRole("button", { name: "owner/repo" }));
    expect(onOpen).toHaveBeenCalled();

    fireEvent.click(openRowActions(screen.getByRole("row", {name:/owner\/repo/})).getByRole("menuitem", { name: "删除目录 owner/repo" }));
    expect(onDelete).toHaveBeenCalled();
  });

  it("uses paired folder install and uninstall actions", () => {
    render(
      <SkillBrowserTable
        kind="folder"
        visibleColumns={new Set(["name", "actions"])}
        folders={[
          {
            key: "owner/repo",
            name: "owner/repo",
            path: "D:/Skills/owner/repo",
            skillCount: 3,
            onOpen: vi.fn(),
            onUninstall: vi.fn(),
            uninstallLabel: "卸载目录",
            onInstall: vi.fn(),
            installLabel: "安装到平台/项目",
            onUpdate: vi.fn(),
            updateLabel: "更新目录",
            onDelete: vi.fn(),
            deleteLabel: "删除目录",
          },
        ]}
      />
    );

    const row = screen.getByRole("row", { name: /owner\/repo/ });
    const actionLabels = openRowActions(row)
      .getAllByRole("menuitem")
      .map((button) => button.getAttribute("aria-label"))
      .filter(Boolean);

    expect(actionLabels).toEqual([
      "卸载目录",
      "更新目录",
      "删除目录",
    ]);
    expect(screen.getByRole("menu").querySelector(".lucide-package-minus")).toBeInTheDocument();
    expect(screen.getByRole("menu").querySelector(".lucide-package-plus")).not.toBeInTheDocument();
  });

  it("renders folder created and updated columns as sortable headers", () => {
    render(
      <SkillBrowserTable
        kind="folder"
        visibleColumns={new Set(["name", "createdAt", "updatedAt", "installSummary", "actions"])}
        sortField="name"
        sortDirection="asc"
        onSortChange={vi.fn()}
        folders={[
          {
            key: "owner/repo",
            name: "owner/repo",
            path: "D:/Skills/owner/repo",
            skillCount: 3,
            createdAt: "2026-07-14T00:00:00Z",
            updatedAt: "2026-08-14T00:00:00Z",
            onOpen: vi.fn(),
          },
        ]}
      />
    );

    expect(screen.getByRole("button", { name: "创建时间" }).querySelector("svg")).toHaveClass("lucide-arrow-up-down");
    expect(screen.getByRole("button", { name: "更新时间" }).querySelector("svg")).toHaveClass("lucide-arrow-up-down");
    expect(screen.getByText("2026-07-14")).toBeInTheDocument();
    expect(screen.getByText("2026-08-14")).toBeInTheDocument();
    expect(
      screen.getAllByRole("columnheader").map((header) => header.getAttribute("aria-label"))
    ).toEqual(["序号", "名称", "创建时间", "更新时间", "安装统计"]);
  });

  it("splits folder install summary into platforms, projects, and shared targets", () => {
    render(
      <SkillBrowserTable
        kind="folder"
        visibleColumns={new Set(["name", "installSummary"])}
        folders={[
          {
            key: "owner/repo",
            name: "owner/repo",
            path: "D:/Skills/owner/repo",
            skillCount: 3,
            installSummaryMembers: [
              { is_central: true, shared_agents: ["hermes"] },
              { is_central: false, linked_agents: ["claude-code", "project:1"] },
            ],
            installAgents: [
              {
                id: "claude-code",
                display_name: "Claude Code",
                global_skills_dir: "C:/Users/alice/.claude/skills",
                is_detected: true,
                is_builtin: true,
                is_enabled: true,
              },
              {
                id: "project:1",
                display_name: "Demo",
                global_skills_dir: "C:/Projects/Demo/.agents/skills",
                is_detected: true,
                is_builtin: false,
                is_enabled: true,
              },
              {
                id: "hermes",
                display_name: "Hermes",
                global_skills_dir: "C:/Users/alice/.agents/skills",
                is_detected: true,
                is_builtin: true,
                is_enabled: true,
                shares_central_skills: true,
              },
            ],
            installLinkedAgentIds: ["claude-code", "project:1"],
            installReadOnlyAgentIds: ["hermes"],
            onOpen: vi.fn(),
          },
        ]}
      />
    );

    expect(screen.getByText("独立安装：平台 1 · 项目 1")).toBeInTheDocument();
    expect(screen.getByText("共享中心：平台 2 · 项目 1")).toBeInTheDocument();
    expect(screen.getByLabelText(/平台：Claude Code/)).toHaveAttribute(
      "title",
      expect.stringContaining("项目：Demo")
    );
  });
});

it("shows only shared platform/project counts for a member with additional manual installs", () => {
  render(<SkillBrowserTable kind="skill" visibleColumns={new Set(["name", "installSummary"])} skills={[{
    rowKey: "member", name: "member", isCentral: true,
    installAgents: ["shared-platform", "project:1"].map(id => ({ id, display_name: id, global_skills_dir: "demo", is_detected: true, is_builtin: false, is_enabled: true })),
    installSummaryMembers: [{ is_central: true, linked_agents: ["manual-platform"], shared_agents: ["shared-platform", "project:1"] }],
  }]} />);
  expect(screen.getByText("共享中心：平台 1 · 项目 1")).toBeInTheDocument();
  expect(screen.queryByText(/独立安装：平台/)).not.toBeInTheDocument();
});


it("shows repository stars including zero and leaves unknown sources blank", () => {
  render(<SkillBrowserTable kind="folder" showGithubStars visibleColumns={new Set(["name"])} folders={[
    { key: "github", name: "Example/Repo", path: "/repo", skillCount: 3, githubStars: 1234, onOpen: vi.fn() },
    { key: "zero", name: "Example/New", path: "/new", skillCount: 1, githubStars: 0, onOpen: vi.fn() },
    { key: "local", name: "Local", path: "/local", skillCount: 1, onOpen: vi.fn() },
  ]} />);
  expect(screen.getByRole("columnheader", { name: "星标数" })).toBeInTheDocument();
  expect(screen.getByRole("cell", { name: (1234).toLocaleString() })).toBeInTheDocument();
  expect(screen.getByRole("cell", { name: "0" })).toBeInTheDocument();
  const localRow = screen.getByRole("button", { name: "Local" }).closest("tr")!;
  expect(within(localRow).getAllByRole("cell").at(-1)).toHaveTextContent(/^$/);

});

  it.each(["skill", "folder"] as const)("omits search from %s actions", (kind) => {
    const action = vi.fn();
    render(<SkillBrowserTable kind={kind} visibleColumns={new Set(["name", "source", "actions"])}
      skills={[{ name: "test", sourceRepo: "Owner/Repo", isReadOnly: true, onUpdateFromSource: action, onSearch: action, onDeleteFromCentral: action }]}
      folders={[{ key: "test", name: "test", path: "test", skillCount: 1, onOpen: action, onUpdate: action, onSearch: action, onDelete: action }]} />);
    expect(openRowActions(screen.getAllByRole("row").at(-1)!).queryByRole("menuitem", { name: "搜索技能（GitHub / Google）" })).not.toBeInTheDocument();
    expect(screen.queryByText("只读")).not.toBeInTheDocument();
  });

it("reorders headers and cells by dragging, remembers order and keeps resize separate", () => {
  const props = {kind:"skill" as const, visibleColumns:new Set(["name","source","createdAt"]), skills:[{rowKey:"demo",name:"Demo",sourceRepo:"Example/Tools",createdAt:"2026-09-01"}]};
  const {unmount} = render(<SkillBrowserTable {...props}/>);
  const dataTransfer = {setData:vi.fn(), effectAllowed:"", dropEffect:""};
  const source = screen.getByRole("columnheader",{name:"创建时间"});
  const name = screen.getByRole("columnheader",{name:"名称"});
  fireEvent.dragStart(source,{dataTransfer});
  fireEvent.dragOver(name,{dataTransfer,clientX:0});
  fireEvent.drop(name,{dataTransfer,clientX:0});
  expect(screen.getAllByRole("columnheader").map(header=>header.getAttribute("aria-label"))).toEqual(["序号","创建时间","名称"]);
  expect(within(screen.getAllByRole("row")[1]).getAllByRole("cell")[1]).toHaveTextContent("2026-09-01");
  unmount();
  render(<SkillBrowserTable {...props}/>);
  expect(screen.getAllByRole("columnheader")[1]).toHaveAttribute("aria-label","创建时间");
  fireEvent.pointerDown(screen.getByRole("separator",{name:"调整 名称 列宽"}),{clientX:100});
  dataTransfer.setData.mockClear();
  fireEvent.dragStart(screen.getByRole("columnheader",{name:"名称"}),{dataTransfer});
  expect(dataTransfer.setData).not.toHaveBeenCalled();
  fireEvent.pointerUp(document);
});

it("excludes retired columns from the visibility menu and old saved preferences", () => {
  render(<SkillBrowserTable kind="folder" tree visibleColumns={new Set(["name","source","notes","tags"])} onToggleColumn={vi.fn()} folders={[]} />);
  expect(screen.getAllByRole("columnheader")).toHaveLength(2);
  fireEvent.contextMenu(screen.getByRole("columnheader",{name:"名称"}));
  const menu=screen.getByRole("menu");
  for (const name of ["仓库","备注","标签"]) expect(within(menu).queryByText(name)).not.toBeInTheDocument();
});

it("supports Ctrl and Shift selection without opening details and preserves selection on right click", () => {
  const detail=vi.fn();
  render(<SkillBrowserTable kind="skill" visibleColumns={new Set(["name"])} skills={["A","B","C"].map(name=>({rowKey:name,name,onDetail:detail,batchOperations:{update:async()=>{}}}))}/>);
  const row=(name:string)=>screen.getByRole("row",{name:new RegExp(name)});
  const firstButton = within(row("A")).getByRole("button");
  firstButton.focus();
  fireEvent.click(firstButton);
  expect(row("A")).toHaveFocus();
  fireEvent.click(within(row("C")).getByRole("button"),{ctrlKey:true});
  expect(row("C")).toHaveFocus();
  expect(detail).toHaveBeenCalledTimes(1);
  expect(row("A")).toHaveAttribute("aria-selected","true");
  expect(row("C")).toHaveAttribute("aria-selected","true");
  openRowActions(row("C"));
  expect(screen.getByText("已选 2 行")).toBeInTheDocument();
  fireEvent.keyDown(screen.getByRole("menu"),{key:"Escape"});
  fireEvent.click(row("A"));
  firstButton.focus();
  fireEvent.click(within(row("C")).getByRole("button"),{shiftKey:true});
  expect(row("C")).toHaveFocus();
  expect(firstButton).not.toHaveFocus();
  expect(row("B")).toHaveAttribute("aria-selected","true");
  openRowActions(row("B"));
  expect(screen.getByText("已选 3 行")).toBeInTheDocument();
});

it("deduplicates selected folders and children and confirms before batch deletion", async () => {
  const first=vi.fn().mockResolvedValue(undefined), second=vi.fn().mockRejectedValue(new Error("failed second"));
  const children=[{rowKey:"a",name:"A",onDetail:vi.fn(),batchOperations:{delete:first}},{rowKey:"b",name:"B",onDetail:vi.fn(),batchOperations:{delete:second}}];
  render(<SkillBrowserTable tree kind="folder" visibleColumns={new Set(["name"])} folders={[{key:"folder",name:"Folder",path:"",skillCount:2,onOpen:vi.fn(),expanded:true,children}]}/>);
  fireEvent.click(screen.getByRole("button",{name:"Folder"}));
  fireEvent.click(screen.getByRole("button",{name:"查看 A 的详情"}),{ctrlKey:true});
  fireEvent.click(openRowActions(screen.getByRole("button",{name:"Folder"}).closest("tr")!).getByRole("menuitem",{name:"删除"}));
  expect(first).not.toHaveBeenCalled();
  const dialog=screen.getByRole("dialog");
  expect(within(dialog).getByText(/将处理 2 项/)).toBeInTheDocument();
  fireEvent.click(within(dialog).getByRole("button",{name:"确认"}));
  await waitFor(()=>expect(within(dialog).getByText(/failed second/)).toBeInTheDocument());
  expect(first).toHaveBeenCalledTimes(1);expect(second).toHaveBeenCalledTimes(1);
});

it("keeps the overview index column narrow regardless of saved column widths", () => {
  render(<SkillBrowserTable compactList kind="skill" visibleColumns={new Set(["name"])} skills={[{name:"A"}]}/>);
  const table=screen.getByRole("table");
  expect(table).toHaveStyle({width:"100%"});
  expect(table.querySelector("col")).toHaveStyle({width:"44px"});
});

it.each(["skill", "folder"] as const)("keeps selected %s rows highlighted under the mouse and prevents Shift text selection", kind => {
 const skills=["A","B"].map(name=>({rowKey:name,name,onDetail:vi.fn()}));
 const folders=["A","B"].map(name=>({key:name,name,path:"",skillCount:1,onOpen:vi.fn()}));
 render(<SkillBrowserTable kind={kind} tree visibleColumns={new Set(["name"])} skills={skills} folders={folders}/>);
 const rows=screen.getAllByRole("row").slice(1);
 fireEvent.click(rows[0]);
 fireEvent.click(rows[1],{ctrlKey:true});
 for(const row of rows) {
   expect(row).toHaveClass("bg-primary/10", "select-none");
   expect(row).not.toHaveClass("hover:bg-muted/25");
 }
 const down=new MouseEvent("mousedown",{bubbles:true,cancelable:true,shiftKey:true,button:0});
 rows[1].dispatchEvent(down);
 expect(down.defaultPrevented).toBe(true);
 fireEvent.click(rows[1],{ctrlKey:true});
 expect(rows[1]).toHaveClass("hover:bg-muted/25");
});
