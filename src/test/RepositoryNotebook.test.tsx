import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { NotebookEditor } from "@/components/notebook/NotebookEditor";
import { NotebookMarkdown } from "@/components/notebook/NotebookMarkdown";
import { useNotebookStore } from "@/stores/notebookStore";
import type { NotebookFile, Repository } from "@/types/notebook";
import { normalizeNotebookFolder } from "@/lib/notebookNames";
import defaultTemplate from "@/data/obsidianTemplate.json";

const initial = useNotebookStore.getState();
function repo(id: number, notebook = "saved"): Repository {
  return { id, sourceKey: `github:example/project${id}`, repositoryUid: `repository-${id}`, fullName: `example/project${id}`, url: "", description: "", tags: [], stars: null, skillIds: ["skill"], notebook, obsidian: null };
}
beforeEach(() => { useNotebookStore.setState(initial, true); });
afterEach(() => { vi.useRealTimers(); });

describe("repository notebooks", () => {
  it("uses Arsenal folders while preserving existing notebook property identifiers", () => {
    expect(normalizeNotebookFolder()).toBe("Arsenal");
    expect(normalizeNotebookFolder(" ")).toBe("Arsenal");
    expect(normalizeNotebookFolder("Attachments/Research")).toBe("Attachments/Research");
    expect(defaultTemplate.template).toContain("skillshub_note_id: {{noteId}}");
    expect(defaultTemplate.template).toContain("skillshub_uri: {{skillshubUri}}");
    expect(defaultTemplate.template).not.toMatch(/arsenal_(?:note|repository|uri)/);
  });
  it("previews GFM tables and math locally without rendering raw HTML", () => {
    const { container } = render(<NotebookMarkdown content={'| Key | Value |\n|---|---|\n| x | y |\n\n$E=mc^2$\n\n<script>alert(1)</script>\n\n```\n$literal$\n```'} />);
    expect(screen.getByRole("table")).toBeInTheDocument();
    expect(container.querySelector(".katex")).toBeInTheDocument();
    expect(container.querySelector("script")).toBeNull();
    expect(container.querySelector("pre")).toHaveTextContent("$literal$");
  });
  it("retains separate unsaved drafts when switching repositories", () => {
    const a = repo(10001), b = repo(10002, "other");
    const { rerender } = render(<NotebookEditor repository={a}/>);
    fireEvent.click(screen.getByRole("button", { name: "编辑笔记" }));
    fireEvent.change(screen.getByRole("textbox"), { target: { value: "first draft" } });
    rerender(<NotebookEditor repository={b}/>);
    expect(screen.queryByText("first draft")).toBeNull();
    expect(screen.getByText("other")).toBeInTheDocument();
    rerender(<NotebookEditor repository={a}/>);
    expect(screen.getByText("first draft")).toBeInTheDocument();
  });
  it("keeps the draft and Save enabled after a failed internal save", async () => {
    const saveField = vi.fn().mockResolvedValue(false);
    useNotebookStore.setState({ saveField });
    render(<NotebookEditor repository={repo(10003)}/>);
    fireEvent.click(screen.getByRole("button", { name: "编辑笔记" }));
    fireEvent.change(screen.getByRole("textbox"), { target: { value: "preserve me" } });
    fireEvent.click(screen.getByRole("button", { name: "保存" }));
    await waitFor(() => expect(saveField).toHaveBeenCalledWith(10003, "notebook", "preserve me", "saved"));
    expect(screen.getByRole("textbox")).toHaveValue("preserve me");
    await waitFor(() => expect(screen.getByRole("button", { name: "保存" })).toBeEnabled());
  });
  it("retains a linked draft and blocks Save when an external revision arrives", async () => {
    const row = repo(10004, "");
    row.obsidian = { bindingVersion: 2, noteId: "note", vaultKey: "default", relativePath: "notes/a.md", obsidianUri: "obsidian://skillshub-note?id=note" };
    let notify = () => {};
    let file: NotebookFile = { noteId: "note", body: "original", revision: "r1", properties: "", relativePath: "notes/a.md" };
    useNotebookStore.setState({ readNotebook: vi.fn(async () => file), watchNotebook: async onChange => { notify = onChange; return () => {}; } });
    render(<NotebookEditor repository={row}/>);
    await screen.findByText("original");
    fireEvent.click(screen.getByRole("button", { name: "编辑笔记" }));
    fireEvent.change(screen.getByRole("textbox"), { target: { value: "my draft" } });
    file = { ...file, body: "external edit", revision: "r2" };
    act(() => { notify(); });
    await waitFor(() => expect(screen.getByRole("button", { name: "保存" })).toBeDisabled());
    expect(screen.getByRole("textbox", { name: "笔记 Markdown 内容" })).toHaveValue("my draft");
  });
});
