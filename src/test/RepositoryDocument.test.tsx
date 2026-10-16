import { act, fireEvent, render, screen } from "@testing-library/react";
import { expect, it, vi } from "vitest";
import { RepositoryDocument } from "@/components/skill/RepositoryDocument";
import { useRepositoryDocumentStore } from "@/stores/repositoryDocumentStore";

it("ignores an older repository response after switching rows", async () => {
  let finishFirst!: (content: import("@/stores/repositoryDocumentStore").RepositoryReadme[]) => void;
  const read = vi.spyOn(useRepositoryDocumentStore.getState(), "readDocuments")
    .mockImplementationOnce(() => new Promise(resolve => { finishFirst = resolve; }))
    .mockResolvedValueOnce([{name:"README.md",language:"default",content:"# Second repository"}]);
  try {
    const { rerender } = render(<RepositoryDocument directory="/first"/>);
    rerender(<RepositoryDocument directory="/second"/>);
    expect(await screen.findByRole("heading", {name:"Second repository"})).toBeInTheDocument();
    await act(async () => finishFirst([{name:"README.md",language:"default",content:"# First repository"}]));
    expect(screen.queryByRole("heading", {name:"First repository"})).not.toBeInTheDocument();
    expect(screen.getByRole("heading", {name:"Second repository"})).toBeInTheDocument();
  } finally { read.mockRestore(); }
});
it("shows missing and failed README states", async () => {
  const read = vi.spyOn(useRepositoryDocumentStore.getState(), "readDocuments").mockResolvedValueOnce([]).mockRejectedValueOnce(new Error("unreadable"));
  try {
    const { rerender } = render(<RepositoryDocument directory="/missing"/>);
    expect(await screen.findByText(/此仓库没有 README.md/)).toBeInTheDocument();
    rerender(<RepositoryDocument directory="/unreadable"/>);
    expect(await screen.findByRole("alert")).toHaveTextContent("读取仓库 README.md 失败");
  } finally { read.mockRestore(); }
});

it("renders README HTML layout while removing executable content", async () => {
  const read = vi.spyOn(useRepositoryDocumentStore.getState(),"readDocuments").mockResolvedValue([{name:"README.md",language:"default",content:
    '<div align="center">\n\n# HTML title\n\n<img src="badge.svg" alt="Badge" onerror="alert(1)"><br>\n\n</div>\n<details open><summary>More</summary>Details</details><script>alert(1)</script><iframe src="https://example.test"></iframe><a href="javascript:alert(1)" onclick="alert(1)">Unsafe link</a>'}]);
  try {
    const {container} = render(<RepositoryDocument directory="/repo" sourceUrl="https://github.com/example/repo/blob/main/README.md"/>);
    expect(await screen.findByRole("heading",{name:"HTML title"})).toBeInTheDocument();
    expect(container.querySelector('.markdown-body div[align="center"]')).toHaveStyle({textAlign:"center"});
    expect(container.querySelector("br")).not.toBeNull();
    expect(container.querySelector("details summary")).toHaveTextContent("More");
    expect(screen.getByRole("img",{name:"Badge"})).toHaveAttribute("src","https://raw.githubusercontent.com/example/repo/main/badge.svg");
    expect(container.querySelector("script,iframe,[onerror],[onclick]")).toBeNull();
    expect(screen.getByText("Unsafe link")).not.toHaveAttribute("href");
  } finally { read.mockRestore(); }
});
it("selects Chinese by default and switches among default, Chinese and English READMEs", async () => {
  const read = vi.spyOn(useRepositoryDocumentStore.getState(),"readDocuments").mockResolvedValue([
    {name:"README.md",language:"default",content:"# Default guide"},
    {name:"README.zh.md",language:"zh",content:"# 中文说明"},
    {name:"README.en.md",language:"en",content:"# English guide"},
  ]);
  try {
    render(<RepositoryDocument directory="/repo"/>);
    expect(await screen.findByRole("heading",{name:"中文说明"})).toBeInTheDocument();
    const selector = screen.getByRole("combobox",{name:"选择 README 文档"});
    expect(selector).toHaveValue("README.zh.md");
    expect(screen.getByRole("option",{name:"默认 · README.md"})).toBeInTheDocument();
    expect(screen.getByRole("option",{name:"中文 · README.zh.md"})).toBeInTheDocument();
    fireEvent.change(selector,{target:{value:"README.en.md"}});
    expect(screen.getByRole("heading",{name:"English guide"})).toBeInTheDocument();
    expect(screen.queryByRole("heading",{name:"中文说明"})).not.toBeInTheDocument();
    fireEvent.change(selector,{target:{value:"README.md"}});
    expect(screen.getByRole("heading",{name:"Default guide"})).toBeInTheDocument();
  } finally { read.mockRestore(); }
});
it("reloads an open document after updating a repository", async () => {
  const read = vi.spyOn(useRepositoryDocumentStore.getState(),"readDocuments")
    .mockResolvedValueOnce([{name:"README.md",language:"default",content:"# Before update"}])
    .mockResolvedValueOnce([{name:"README.md",language:"default",content:"# After update"}]);
  try {
    render(<RepositoryDocument directory="/repo"/>);
    expect(await screen.findByRole("heading",{name:"Before update"})).toBeInTheDocument();
    act(()=>useRepositoryDocumentStore.getState().invalidate());
    expect(await screen.findByRole("heading",{name:"After update"})).toBeInTheDocument();
  } finally { read.mockRestore(); }
});
