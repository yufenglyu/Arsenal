import { it, expect } from "vitest";
import { useState } from "react";
import { render, screen, fireEvent } from "@testing-library/react";
import { SidebarTagFilter } from "@/components/layout/SidebarTagFilter";
function Harness() {
  const [selected, setSelected] = useState(true);
  return (
    <SidebarTagFilter
      hasSelection={selected}
      onClear={() => setSelected(false)}
    >
      <span>Tag content</span>
    </SidebarTagFilter>
  );
}
it("hides clear filters while collapsed and preserves the selection until cleared", () => {
  localStorage.removeItem("skillshub.tags.expanded");
  render(<Harness />);
  const clear = screen.getByRole("button", { name: "清除筛选" });
  expect(clear).toBeEnabled();
  fireEvent.click(screen.getByRole("button", { name: "标签" }));
  expect(screen.getByText("Tag content")).toBeVisible();
  expect(screen.getByRole("button", { name: "标签" })).toHaveFocus();
  fireEvent.doubleClick(screen.getByRole("button", { name: "标签" }));
  expect(screen.queryByText("Tag content")).not.toBeInTheDocument();
  expect(screen.queryByRole("button", { name: "清除筛选" })).not.toBeInTheDocument();
  expect(screen.getByRole("button", { name: "标签" })).toHaveAttribute(
    "aria-expanded",
    "false",
  );
  fireEvent.doubleClick(screen.getByRole("button", { name: "标签" }));
  const restoredClear = screen.getByRole("button", { name: "清除筛选" });
  expect(restoredClear).toBeEnabled();
  fireEvent.click(restoredClear);
  expect(restoredClear).toBeDisabled();
  const title = screen.getByRole("button", {name:"标签"});
  fireEvent.keyDown(title, {key:" "});
  expect(title).toHaveAttribute("aria-expanded", "false");
  fireEvent.click(title);
  expect(title).toHaveAttribute("aria-expanded", "false");
  fireEvent.keyDown(title, {key:"Enter"});
  expect(title).toHaveAttribute("aria-expanded", "true");
});
