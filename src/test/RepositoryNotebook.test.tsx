import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { NotebookEditor } from "@/components/notebook/NotebookEditor";
import { NotebookMarkdown } from "@/components/notebook/NotebookMarkdown";
import { useNotebookStore } from "@/stores/notebookStore";
import type { NotebookFile, Repository } from "@/types/notebook";
import { normalizeNotebookFolder } from "@/lib/notebookNames";
import defaultTemplate from "@/data/obsidianTemplate.json";
import { NotebookSettings } from "@/components/notebook/NotebookSettings";

const initial = useNotebookStore.getState();
function repo(id: number, notebook = "saved"): Repository {
  return { id, sourceKey: `github:example/project${id}`, repositoryUid: `repository-${id}`, fullName: `example/project${id}`, url: "", description: "", tags: [], stars: null, skillIds: ["skill"], notebook, obsidian: null };
}
beforeEach(() => { useNotebookStore.setState(initial, true); });
afterEach(() => { vi.useRealTimers(); });

describe("repository notebooks", () => {
  it("shows Arsenal defaults instead of an old saved template and loads the new template setting", async () => {
    useNotebookStore.setState({config: {obsidian_property_template: "obsolete custom template"}, loadConfig: async () => {}});
    const {container} = render(<NotebookSettings />);
    const field = container.querySelector<HTMLTextAreaElement>("#notebook-template")!;
    expect(field).toHaveValue(defaultTemplate.template);
    const template = "---\narsenal_repository: {{fullName}}\n---\n";
    act(() => useNotebookStore.setState({config: {arsenal_property_template: template}}));
    await waitFor(() => expect(field).toHaveValue(template));
  });
  it("uses Arsenal folders and property identifiers", () => {
    expect(normalizeNotebookFolder()).toBe("Arsenal");
    expect(normalizeNotebookFolder(" ")).toBe("Arsenal");
    expect(normalizeNotebookFolder("Attachments/Research")).toBe("Attachments/Research");
    expect(defaultTemplate.template).toContain("arsenal_note_id: {{noteId}}");
    expect(defaultTemplate.template).toContain("arsenal_uri: {{arsenalUri}}");
    expect(defaultTemplate.template).toContain("arsenal_repository_id: {{repositoryUid}}");
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
    row.obsidian = { bindingVersion: 2, noteId: "note", vaultKey: "default", relativePath: "notes/a.md", obsidianUri: "obsidian://arsenal-note?id=note" };
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
