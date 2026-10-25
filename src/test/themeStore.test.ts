import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
import { invoke } from "@tauri-apps/api/core";
import { useThemeStore, type ThemeMode } from "../stores/themeStore";

function resetStore() {
  useThemeStore.setState({ mode: "system", resolvedTheme: "dark" });
  try {
    localStorage.removeItem("arsenal-theme-mode");
    localStorage.removeItem("catppuccin-flavor");
    localStorage.removeItem("catppuccin-accent");
  } catch {
    // ignore unavailable storage
  }
  delete document.documentElement.dataset.theme;
  delete document.documentElement.dataset.themeMode;
  delete document.documentElement.dataset.accent;
}

describe("themeStore", () => {
  beforeEach(() => {
    resetStore();
    vi.clearAllMocks();
    vi.mocked(invoke).mockResolvedValue(null);
  });

  afterEach(() => {
    resetStore();
  });

  it("defaults to system mode before init", () => {
    expect(useThemeStore.getState().mode).toBe("system");
  });

  it("setMode applies light mode and persists it", () => {
    useThemeStore.getState().setMode("light");

    expect(useThemeStore.getState().mode).toBe("light");
    expect(useThemeStore.getState().resolvedTheme).toBe("light");
    expect(document.documentElement.dataset.theme).toBe("light");
    expect(document.documentElement.dataset.themeMode).toBe("light");
    expect(invoke).toHaveBeenCalledWith("set_setting", { key: "theme_mode", value: "light" });
  });

  it("setMode applies dark mode and persists it", () => {
    useThemeStore.getState().setMode("dark");

    expect(useThemeStore.getState().mode).toBe("dark");
    expect(useThemeStore.getState().resolvedTheme).toBe("dark");
    expect(document.documentElement.dataset.theme).toBe("dark");
    expect(document.documentElement.dataset.themeMode).toBe("dark");
    expect(invoke).toHaveBeenCalledWith("set_setting", { key: "theme_mode", value: "dark" });
  });

  it("system mode resolves from prefers-color-scheme", () => {
    const spy = vi.spyOn(window, "matchMedia");
    spy.mockReturnValue({ matches: true } as MediaQueryList);

    useThemeStore.getState().setMode("system");

    expect(useThemeStore.getState().mode).toBe("system");
    expect(useThemeStore.getState().resolvedTheme).toBe("dark");
    expect(document.documentElement.dataset.theme).toBe("dark");
    expect(document.documentElement.dataset.themeMode).toBe("system");
    spy.mockRestore();
  });

  it("cycleMode rotates system to light to dark to system", () => {
    const expected: ThemeMode[] = ["light", "dark", "system"];

    for (const mode of expected) {
      useThemeStore.getState().cycleMode();
      expect(useThemeStore.getState().mode).toBe(mode);
    }
  });

  it("init loads config mode and ignores browser storage", async () => {
    vi.mocked(invoke).mockResolvedValue("light");
    localStorage.setItem("arsenal-theme-mode", "light");
    localStorage.setItem("catppuccin-accent", "green");

    await useThemeStore.getState().init();

    expect(useThemeStore.getState().mode).toBe("light");
    expect(document.documentElement.dataset.theme).toBe("light");
    expect(document.documentElement.dataset.accent).toBeUndefined();
  });

  it("init falls back to system when config mode is invalid", async () => {
    vi.mocked(invoke).mockResolvedValue("mocha");
    localStorage.setItem("arsenal-theme-mode", "mocha");
    const spy = vi.spyOn(window, "matchMedia");
    spy.mockReturnValue({ matches: false } as MediaQueryList);

    await useThemeStore.getState().init();

    expect(useThemeStore.getState().mode).toBe("system");
    expect(useThemeStore.getState().resolvedTheme).toBe("light");
    expect(document.documentElement.dataset.theme).toBe("light");
    spy.mockRestore();
  });
});
