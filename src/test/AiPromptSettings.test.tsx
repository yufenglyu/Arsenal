import { beforeEach, describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { AiPromptSettings } from "@/components/settings/AiPromptSettings";
import { DEFAULT_NOTE_PROMPT, DEFAULT_REPOSITORY_NOTE_PROMPT, DEFAULT_TAGS_PROMPT } from "@/stores/aiPromptStore";
import { invoke } from "@/lib/tauri";
vi.mock("@/lib/tauri", () => ({ invoke: vi.fn(), isTauriRuntime: () => true }));

describe("AI prompt settings", () => {
  beforeEach(() => { vi.mocked(invoke).mockReset(); });
  it("loads saved prompts, edits and saves both config keys", async () => {
    vi.mocked(invoke).mockImplementation(async (command, args) => command === "get_setting" ? ((args as { key: string }).key === "ai_note_prompt" ? "备注要求" : "标签要求") : undefined);
    render(<AiPromptSettings />);
    await waitFor(() => expect(screen.getByLabelText("AI 备注提示词")).toHaveValue("备注要求"));
    expect(screen.getByLabelText("AI 标签提示词")).toHaveValue("标签要求");
    fireEvent.change(screen.getByLabelText("AI 备注提示词"), { target: { value: "只写一句中文" } });
    fireEvent.change(screen.getByLabelText("AI 仓库备注提示词"), {target:{value:"概括仓库用途"}});
    fireEvent.click(screen.getByRole("button", { name: "保存" }));
    await waitFor(() => expect(invoke).toHaveBeenCalledWith("set_setting", { key: "ai_note_prompt", value: "只写一句中文" }));
    await waitFor(() => expect(invoke).toHaveBeenCalledWith("set_setting", { key: "ai_tags_prompt", value: "标签要求" }));
    await waitFor(() => expect(invoke).toHaveBeenCalledWith("set_setting", {key:"ai_repository_note_prompt",value:"概括仓库用途"}));
  });
  it("restores defaults as an editable draft before saving", async () => {
    vi.mocked(invoke).mockResolvedValue("自定义");
    render(<AiPromptSettings />);
    await waitFor(() => expect(screen.getByRole("button", { name: "恢复默认" })).toBeEnabled());
    fireEvent.click(screen.getByRole("button", { name: "恢复默认" }));
    expect(screen.getByLabelText("AI 备注提示词")).toHaveValue(DEFAULT_NOTE_PROMPT);
    expect(screen.getByLabelText("AI 标签提示词")).toHaveValue(DEFAULT_TAGS_PROMPT);
    expect(screen.getByLabelText("AI 仓库备注提示词")).toHaveValue(DEFAULT_REPOSITORY_NOTE_PROMPT);
    expect(vi.mocked(invoke).mock.calls.every(([command]) => command === "get_setting")).toBe(true);
  });
});
