import {beforeEach, expect, it, vi} from "vitest";
import {fireEvent, render, screen, waitFor, within} from "@testing-library/react";
import {ResourceTrashView} from "@/pages/ResourceTrashView";
import {useResourceTrashStore as store} from "@/stores/resourceTrashStore";
import {openRowActions} from "./rowActions";

const restore=vi.fn().mockResolvedValue(undefined),purge=vi.fn().mockResolvedValue(undefined);
beforeEach(()=>{
  vi.clearAllMocks();
  store.setState({entries:[{id:"one",name:"example/one",kind:"repository",repositories:["example/one"],skillCount:3,deletedAt:"2026-10-21T01:00:00Z",reason:"delete",phase:"active"},{id:"two",name:"Second skill",kind:"skill",repositories:["example/two"],skillCount:1,deletedAt:"2026-10-21T02:00:00Z",reason:"delete",phase:"active"}],loading:false,error:null,load:vi.fn().mockResolvedValue(undefined),restore,purge});
});
it("restores a row only after confirmation using the shared table",async()=>{
  render(<ResourceTrashView/>);
  expect(screen.getByRole("columnheader",{name:"移入时间"})).toBeInTheDocument();
  const row=screen.getByRole("button",{name:"example/one"}).closest("tr")!;
  fireEvent.click(openRowActions(row).getByRole("menuitem",{name:"恢复"}));
  expect(restore).not.toHaveBeenCalled();
  fireEvent.click(within(screen.getByRole("dialog")).getByRole("button",{name:"确认"}));
  await waitFor(()=>expect(restore).toHaveBeenCalledWith("one"));
});
it("supports confirmed emptying and filters without hiding the saved total",async()=>{
  render(<ResourceTrashView/>);
  fireEvent.change(screen.getByRole("textbox",{name:"搜索回收站"}),{target:{value:"example/one"}});
  expect(screen.queryByRole("button",{name:"Second skill"})).not.toBeInTheDocument();
  fireEvent.click(screen.getByRole("button",{name:"清空回收站"}));
  expect(purge).not.toHaveBeenCalled();
  const dialog=within(screen.getByRole("dialog"));expect(dialog.getByText("Second skill")).toBeInTheDocument();
  fireEvent.click(dialog.getByRole("button",{name:"确认"}));
  await waitFor(()=>expect(purge).toHaveBeenCalledTimes(2));
});
