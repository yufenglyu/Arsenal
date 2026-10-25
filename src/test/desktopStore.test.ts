import {beforeEach, describe, expect, it, vi} from "vitest";
import {confirm} from "@tauri-apps/plugin-dialog";
import {toast} from "sonner";
import {invoke, listen} from "@/lib/tauri";
import {startDesktopExitListener, useDesktopStore} from "@/stores/desktopStore";
import {useTaskQueueStore} from "@/stores/taskQueueStore";

vi.mock("@/lib/tauri", () => ({invoke: vi.fn(), listen: vi.fn()}));
vi.mock("@tauri-apps/plugin-dialog", () => ({confirm: vi.fn()}));
vi.mock("sonner", () => ({toast: {error: vi.fn()}}));
beforeEach(() => {
  vi.clearAllMocks();
  useDesktopStore.setState({exiting: false});
  useTaskQueueStore.setState({tasks: []});
  vi.mocked(invoke).mockResolvedValue(undefined);
});
const activeTask = () => useTaskQueueStore.setState({tasks: [{id: "running", key: "running", kind: "update", label: "Example", status: "running", cancelRequested: false, createdAt: 1, locks: [], steps: []}]});

describe("explicit tray exit", () => {
  it("exits an idle app without asking a second time", async () => {
    await useDesktopStore.getState().requestExit();
    expect(confirm).not.toHaveBeenCalled();
    expect(invoke).toHaveBeenCalledWith("exit_app");
  });
  it("keeps background tasks alive when exit is cancelled", async () => {
    activeTask();
    vi.mocked(confirm).mockResolvedValue(false);
    await useDesktopStore.getState().requestExit();
    expect(invoke).not.toHaveBeenCalled();
    expect(useTaskQueueStore.getState().tasks[0].status).toBe("running");
    expect(useDesktopStore.getState().exiting).toBe(false);
  });
  it("allows confirmed exit and ignores concurrent clicks", async () => {
    activeTask();
    let resolve!: (answer: boolean) => void;
    vi.mocked(confirm).mockReturnValue(new Promise(done => {resolve = done;}));
    const first = useDesktopStore.getState().requestExit();
    await useDesktopStore.getState().requestExit();
    expect(confirm).toHaveBeenCalledTimes(1);
    resolve(true);
    await first;
    expect(invoke).toHaveBeenCalledTimes(1);
  });
  it("reports failed exit and permits retry", async () => {
    vi.mocked(invoke).mockRejectedValueOnce(new Error("fixture"));
    await useDesktopStore.getState().requestExit();
    expect(toast.error).toHaveBeenCalled();
    await useDesktopStore.getState().requestExit();
    expect(invoke).toHaveBeenCalledTimes(2);
  });
  it("cleans up a listener that finishes attaching after unmount", async () => {
    const dispose = vi.fn();
    let resolve!: (unlisten: () => void) => void;
    vi.mocked(listen).mockReturnValue(new Promise(done => {resolve = done;}));
    const stop = startDesktopExitListener();
    stop();
    resolve(dispose);
    await Promise.resolve();
    expect(dispose).toHaveBeenCalledTimes(1);
    expect(invoke).not.toHaveBeenCalled();
  });
});
