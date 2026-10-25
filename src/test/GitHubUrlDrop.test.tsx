import { createEvent, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, expect, it, vi } from "vitest";
import { GitHubUrlDrop } from "@/components/layout/GitHubUrlDrop";
import { acceptsGitHubUrlDrag, droppedGitHubUrls, droppedShortcutGitHubUrls } from "@/lib/githubUrlDrop";
import { enqueueGitHubUrlImport } from "@/stores/importPreparationStore";
import { toast } from "sonner";
vi.mock("@/stores/importPreparationStore", () => ({ enqueueGitHubUrlImport: vi.fn() }));
vi.mock("sonner", () => ({ toast: {info: vi.fn()} }));
const transfer = (values: Record<string,string | undefined>) => ({types:Object.keys(values), getData:(type:string) => values[type] ?? "", dropEffect:"none"});
beforeEach(() => vi.clearAllMocks());
it("parses browser URI lists, drops comments and duplicates, and preserves pinned paths", () => {
  expect(droppedGitHubUrls(transfer({"text/uri-list":"# Comment\r\nhttps://github.com/Example/Skills/?utm_source=browser#readme\r\nhttps://github.com/example/skills\r\nhttps://github.com/example/other/tree/main"})))
    .toEqual(["https://github.com/example/skills","https://github.com/example/other/tree/main"]);
});
it("supports Firefox URL/title pairs and a plain address-bar URL", () => {
  expect(droppedGitHubUrls(transfer({"text/x-moz-url":"https://github.com/example/skills\nhttps://github.com/title/not-a-link"})))
    .toEqual(["https://github.com/example/skills"]);
  expect(droppedGitHubUrls(transfer({"text/plain":"http://github.com/example/skills"})))
    .toEqual(["https://github.com/example/skills"]);
});
it.each(["https://github.com/owner", "https://github.com.evil.test/a/b", "https://example.test/a/b", "file:///a/b", "https://example:example@github.com/a/b", "Ordinary text"])("ignores unsupported input %s", value => {
  expect(droppedGitHubUrls(transfer({"text/plain":value}))).toEqual([]);
});
it("queues each dropped repository once and prevents navigation", () => {
  render(<GitHubUrlDrop/>);
  const dataTransfer = transfer({"text/uri-list":"https://github.com/example/skills\nhttps://github.com/example/skills\nhttps://github.com/example/other"});
  fireEvent.dragEnter(window,{dataTransfer});
  expect(screen.getByRole("status")).toHaveTextContent("松开以导入 GitHub 仓库");
  const over = createEvent.dragOver(window,{dataTransfer});
  fireEvent(window,over);
  expect(over.defaultPrevented).toBe(true);
  expect(dataTransfer.dropEffect).toBe("copy");
  const drop = createEvent.drop(window,{dataTransfer});
  fireEvent(window,drop);
  expect(drop.defaultPrevented).toBe(true);
  expect(enqueueGitHubUrlImport).toHaveBeenCalledTimes(2);
  expect(enqueueGitHubUrlImport).toHaveBeenNthCalledWith(1,"https://github.com/example/skills");
  expect(screen.queryByRole("status")).not.toBeInTheDocument();
});
it("ignores internal skill, column and local file drags", () => {
  render(<GitHubUrlDrop/>);
  for (const values of [{"application/arsenal-skills":"[]"}]) {
    const dataTransfer = transfer(values);
    expect(acceptsGitHubUrlDrag(dataTransfer)).toBe(false);
    const drop = createEvent.drop(window,{dataTransfer});
    fireEvent(window,drop);
    expect(drop.defaultPrevented).toBe(false);
  }
  const dataTransfer = transfer({"text/plain":"https://github.com/example/skills"});
  fireEvent.dragStart(window,{dataTransfer});
  fireEvent.dragEnter(window,{dataTransfer});
  expect(screen.queryByRole("status")).not.toBeInTheDocument();
  const drop = createEvent.drop(window,{dataTransfer});
  fireEvent(window,drop);
  expect(drop.defaultPrevented).toBe(false);
  expect(enqueueGitHubUrlImport).not.toHaveBeenCalled();
});
it("blocks unsupported web links while preserving ordinary editable text drops", () => {
  render(<GitHubUrlDrop/>);
  const link = createEvent.drop(window,{dataTransfer:transfer({"text/plain":"https://example.test/a/b"})});
  fireEvent(window,link);
  expect(link.defaultPrevented).toBe(true);
  expect(toast.info).toHaveBeenCalledWith("请拖入有效的 GitHub 仓库网址。");
  const text = createEvent.drop(window,{dataTransfer:transfer({"text/plain":"Notes"})});
  fireEvent(window,text);
  expect(text.defaultPrevented).toBe(false);
  expect(enqueueGitHubUrlImport).not.toHaveBeenCalled();
});
it("clears the hint when leaving and removes global listeners on unmount", () => {
  const {unmount} = render(<GitHubUrlDrop/>);
  const dataTransfer = transfer({"text/plain":"https://github.com/example/skills"});
  fireEvent.dragEnter(window,{dataTransfer});
  fireEvent.dragEnter(window,{dataTransfer});
  fireEvent.dragLeave(window,{dataTransfer});
  expect(screen.getByRole("status")).toBeInTheDocument();
  fireEvent.dragLeave(window,{dataTransfer});
  expect(screen.queryByRole("status")).not.toBeInTheDocument();
  unmount();
  fireEvent.drop(window,{dataTransfer});
  expect(enqueueGitHubUrlImport).not.toHaveBeenCalled();
});

it("accepts a browser URL advertised together with a virtual shortcut file", () => {
  render(<GitHubUrlDrop/>);
  const dataTransfer = transfer({Files:"", "text/uri-list":"https://github.com/example/skills", "text/plain":"https://github.com/example/skills"});
  fireEvent.dragEnter(window,{dataTransfer});
  expect(screen.getByRole("status")).toHaveTextContent("松开以导入 GitHub 仓库");
  const drop = createEvent.drop(window,{dataTransfer});
  fireEvent(window,drop);
  expect(drop.defaultPrevented).toBe(true);
  expect(enqueueGitHubUrlImport).toHaveBeenCalledWith("https://github.com/example/skills");
});

it("does not trigger import for ordinary local files", () => {
  render(<GitHubUrlDrop/>);
  const drop = createEvent.drop(window,{dataTransfer:{...transfer({Files:""}),files:[new File(["Notes"],"notes.txt")]}});
  fireEvent(window,drop);
  expect(drop.defaultPrevented).toBe(false);
  expect(enqueueGitHubUrlImport).not.toHaveBeenCalled();
});
it("preserves link-only drag operations", () => {
  render(<GitHubUrlDrop/>);
  const dataTransfer = {...transfer({"text/plain":"https://github.com/example/skills"}),effectAllowed:"link"};
  fireEvent.dragOver(window,{dataTransfer});
  expect(dataTransfer.dropEffect).toBe("link");
});
it("supports displayed address-bar URLs without a scheme", () => {
  expect(droppedGitHubUrls(transfer({"text/plain":"github.com/example/skills"}))).toEqual(["https://github.com/example/skills"]);
});
it("imports a Windows URL shortcut advertised only as Files", async () => {
  render(<GitHubUrlDrop/>);
  const bytes = new TextEncoder().encode("[InternetShortcut]\r\nURL=https://github.com/example/skills\r\n");
  const file = new File([bytes],"skills.url");
  // jsdom does not implement Blob.arrayBuffer; the browser uses the real method.
  Object.defineProperty(file,"arrayBuffer",{value:async()=>bytes.buffer});
  const dataTransfer = {...transfer({Files:""}),files:[file]};
  fireEvent.dragEnter(window,{dataTransfer});
  expect(screen.getByRole("status")).toBeInTheDocument();
  const drop = createEvent.drop(window,{dataTransfer});
  fireEvent(window,drop);
  expect(drop.defaultPrevented).toBe(true);
  await waitFor(()=>expect(enqueueGitHubUrlImport).toHaveBeenCalledWith("https://github.com/example/skills"));
});
it("decodes UTF-16 Windows shortcuts and ignores oversized or unrelated files", async () => {
  const text="[InternetShortcut]\r\nURL=https://github.com/example/skills\r\n";
  const bytes=new Uint8Array(2+text.length*2);bytes.set([0xff,0xfe]);
  const view=new DataView(bytes.buffer);for(let index=0;index<text.length;index++)view.setUint16(2+index*2,text.charCodeAt(index),true);
  const file=new File([bytes],"skills.url");
  Object.defineProperty(file,"arrayBuffer",{value:async()=>bytes.buffer});
  expect(await droppedShortcutGitHubUrls([file,new File(["x".repeat(65537)],"large.url"),new File([text],"notes.txt")])).toEqual(["https://github.com/example/skills"]);
});
