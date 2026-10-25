import { it, expect, vi } from "vitest";
import { useState } from "react";
import {
  render,
  screen,
  fireEvent,
  waitFor,
  within,
  createEvent,
} from "@testing-library/react";
import { TagFilters } from "@/components/skill/TagFilters";
import { useMetadataStore } from "@/stores/metadataStore";
function Harness() {
  const [selected, setSelected] = useState<string[]>([]);
  return (
    <TagFilters
      tags={[
        { key: "a", label: "A" },
        { key: "b", label: "B" },
      ]}
      selected={selected}
      onChange={setSelected}
    />
  );
}
it("supports Ctrl/Cmd additive selection and plain-click single selection", () => {
  render(<Harness />);
  const a = screen.getByRole("button", { name: "A" }),
    b = screen.getByRole("button", { name: "B" });
  fireEvent.click(a);
  fireEvent.click(b, { ctrlKey: true });
  expect(a).toHaveAttribute("aria-pressed", "true");
  expect(b).toHaveAttribute("aria-pressed", "true");
  fireEvent.click(a, { metaKey: true });
  expect(a).toHaveAttribute("aria-pressed", "false");
  fireEvent.click(a);
  expect(b).toHaveAttribute("aria-pressed", "false");
  fireEvent.click(a);
  expect(
    screen.queryByRole("button", { name: "清除筛选" }),
  ).not.toBeInTheDocument();
  expect(a).toHaveAttribute("aria-pressed", "false");
});
it("uses Shift only to remove the target tag from dragged skills", async () => {
  const change = vi
    .spyOn(useMetadataStore.getState(), "changeTag")
    .mockResolvedValue();
  render(<Harness />);
  for (const remove of [false, true]) {
    const event = createEvent.drop(screen.getByRole("button", { name: "A" }), {
      dataTransfer: { getData: () => JSON.stringify(["one", "two"]) },
    });
    Object.defineProperty(event, "shiftKey", { value: remove });
    fireEvent(screen.getByRole("button", { name: "A" }), event);
    await waitFor(() =>
      expect(change).toHaveBeenLastCalledWith("A", ["one", "two"], remove),
    );
  }
  change.mockRestore();
});
it("shows affected skill count before deleting a tag globally", async () => {
  const count = vi
    .spyOn(useMetadataStore.getState(), "countTag")
    .mockResolvedValue(7);
  const change = vi
    .spyOn(useMetadataStore.getState(), "changeTag")
    .mockResolvedValue();
  render(<Harness />);
  fireEvent.contextMenu(screen.getByRole("button", { name: "A" }));
  fireEvent.click(screen.getByRole("menuitem", { name: "删除" }));
  const dialog = await screen.findByRole("dialog");
  expect(dialog).toHaveTextContent("7 个技能");
  expect(change).not.toHaveBeenCalled();
  fireEvent.click(within(dialog).getByRole("button", { name: "删除" }));
  await waitFor(() => expect(change).toHaveBeenCalledWith("A", null, true));
  count.mockRestore();
  change.mockRestore();
});

// Windows native drop interception prevents HTML5 skill-to-tag events entirely.
import tauriConfig from "../../src-tauri/tauri.conf.json";
it("allows HTML5 drag and drop in the desktop window", () => {
  expect(tauriConfig.app.windows[0]).toHaveProperty("dragDropEnabled", false);
});

import { SkillBrowserTable } from "@/components/skill/SkillBrowserTable";
it("transfers selected skill IDs from table rows to the tag drop target", async () => {
  const change = vi
    .spyOn(useMetadataStore.getState(), "changeTag")
    .mockResolvedValue();
  render(
    <>
      <SkillBrowserTable
        kind="skill"
        visibleColumns={new Set(["name"])}
        skills={[
          {
            rowKey: "one",
            name: "One",
            detailRequest: { skillId: "one" },
            onDetail: vi.fn(),
          },
          { rowKey: "two", name: "Two", detailRequest: { skillId: "two" } },
        ]}
      />
      <Harness />
    </>,
  );
  const one = screen.getByText("One").closest("tr")!;
  const two = screen.getByText("Two").closest("tr")!;
  fireEvent.click(one);
  fireEvent.click(two, { ctrlKey: true });
  const data = new Map<string, string>();
  const transfer = {
    setData: (key: string, value: string) => data.set(key, value),
    getData: (key: string) => data.get(key) ?? "",
    types: ["application/arsenal-skills"],
    effectAllowed: "",
    dropEffect: "",
  };
  screen.getByText("One").focus();
  fireEvent.dragStart(one, { dataTransfer: transfer });
  expect(one).toHaveFocus();
  expect(JSON.parse(transfer.getData("application/arsenal-skills"))).toEqual([
    "one",
    "two",
  ]);
  const tag = screen.getByRole("button", { name: "A" });
  const over = createEvent.dragOver(tag, { dataTransfer: transfer });
  fireEvent(tag, over);
  expect(over.defaultPrevented).toBe(true);
  expect(tag).toHaveAttribute("data-drop-target", "add");
  fireEvent.dragLeave(tag);
  expect(tag).not.toHaveAttribute("data-drop-target");
  fireEvent(tag, over);
  fireEvent.drop(tag, { dataTransfer: transfer });
  expect(tag).not.toHaveAttribute("data-drop-target");
  await waitFor(() =>
    expect(change).toHaveBeenCalledWith("A", ["one", "two"], false),
  );
  change.mockRestore();
});

it("distinguishes removal hover and resets feedback when dragging is cancelled", () => {
  render(<Harness />);
  const a = screen.getByRole("button", { name: "A" });
  const b = screen.getByRole("button", { name: "B" });
  const transfer = { types: ["application/arsenal-skills"], dropEffect: "" };
  const over = createEvent.dragOver(a, { dataTransfer: transfer });
  Object.defineProperty(over, "shiftKey", { value: true });
  fireEvent(a, over);
  expect(a).toHaveAttribute("data-drop-target", "remove");
  fireEvent.dragOver(b, { dataTransfer: transfer });
  expect(a).not.toHaveAttribute("data-drop-target");
  expect(b).toHaveAttribute("data-drop-target", "add");
  fireEvent.dragEnd(window);
  expect(b).not.toHaveAttribute("data-drop-target");
});

it("renames from the icon menu and requires explicit merge on collision", async () => {
  const rename=vi.spyOn(useMetadataStore.getState(),"renameTag").mockResolvedValue();
  render(<Harness/>);
  fireEvent.contextMenu(screen.getByRole("button",{name:"A"}));
  const edit=screen.getByRole("menuitem",{name:"重命名标签"});
  expect(edit.querySelector("svg")).not.toBeNull();
  fireEvent.click(edit);
  const dialog=screen.getByRole("dialog",{name:"重命名标签"});
  fireEvent.change(within(dialog).getByRole("textbox"),{target:{value:"B"}});
  expect(within(dialog).getByRole("button",{name:"保存"})).toBeDisabled();
  fireEvent.click(within(dialog).getByRole("checkbox"));
  fireEvent.click(within(dialog).getByRole("button",{name:"保存"}));
  await waitFor(()=>expect(rename).toHaveBeenCalledWith("A","B",true));
  rename.mockRestore();
});
