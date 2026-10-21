import { useState } from "react";
import userEvent from "@testing-library/user-event";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { expect, it } from "vitest";
import { SearchScopes } from "@/components/skill/SearchScopes";
import { allSearchScopes, type SearchScope } from "@/lib/skillFilters";

function Harness() {
  const [value, setValue] = useState<SearchScope[]>(allSearchScopes);
  return <><SearchScopes value={value} onChange={setValue}/><button>outside</button></>;
}
async function setup() {
  const user = userEvent.setup();
  render(<Harness/>);
  const trigger = screen.getByRole("button", {name:"搜索范围"});
  await user.click(trigger);
  return {user, trigger, scope: await screen.findByLabelText("技能名称")};
}
it("keeps the popup open while toggling a label without native mouse text selection", async () => {
  const {user, scope} = await setup();
  const press = new MouseEvent("mousedown", {bubbles:true, cancelable:true});
  fireEvent(scope.closest("label")!, press);
  expect(press.defaultPrevented).toBe(true);
  await user.click(scope.closest("label")!);
  expect(scope).not.toBeChecked();
  expect(scope).toBeVisible();
  await user.click(scope.closest("label")!);
  expect(scope).toBeChecked();
  await user.click(screen.getByText("outside"));
  await waitFor(() => expect(scope).not.toBeInTheDocument());
});
it("closes on window blur", async () => {
  const {scope} = await setup();
  fireEvent.blur(window);
  await waitFor(() => expect(scope).not.toBeInTheDocument());
});
it("supports keyboard toggling, keeps at least one scope, and returns focus on Escape", async () => {
  const {user, trigger, scope} = await setup();
  scope.focus();
  await user.keyboard(" ");
  expect(scope).not.toBeChecked();
  await user.click(screen.getByLabelText("仓库名称"));
  await user.click(screen.getByLabelText("描述"));
  expect(screen.getByLabelText("备注")).toBeDisabled();
  await user.keyboard("{Escape}");
  await waitFor(() => expect(scope).not.toBeInTheDocument());
  expect(trigger).toHaveFocus();
});
