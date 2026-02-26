import { beforeEach, describe, expect, it, vi } from "vitest";
import * as api from "./invoke";
import { WebviewWindow } from "@tauri-apps/api/webviewWindow";
import { findCommand } from "./commands";

const mockMainHide = vi.hoisted(() => vi.fn(async () => {}));
const mockResultsHide = vi.hoisted(() => vi.fn(async () => {}));

vi.mock("@tauri-apps/api/window", () => ({
  getCurrentWindow: vi.fn(() => ({ hide: mockMainHide })),
}));

vi.mock("@tauri-apps/api/webviewWindow", () => ({
  WebviewWindow: {
    getByLabel: vi.fn(async () => ({ hide: mockResultsHide })),
  },
}));

vi.mock("./invoke", () => ({
  openAbout: vi.fn(async () => {}),
  openSettings: vi.fn(async () => {}),
  rebuildIndex: vi.fn(async () => true),
  quitApp: vi.fn(async () => {}),
}));

describe("slash command actions", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.mocked(WebviewWindow.getByLabel).mockResolvedValue({ hide: mockResultsHide } as never);
  });

  it("/a hides results window before openAbout", async () => {
    const order: string[] = [];
    mockResultsHide.mockImplementation(async () => { order.push("hideResults"); });
    vi.mocked(api.openAbout).mockImplementation(async () => {
      order.push("openAbout");
    });

    const cmd = findCommand("/a");
    expect(cmd).toBeDefined();
    await cmd!.action();

    expect(order).toEqual(["hideResults", "openAbout"]);
    expect(api.openAbout).toHaveBeenCalledTimes(1);
    expect(vi.mocked(WebviewWindow.getByLabel)).toHaveBeenCalledWith("results");
  });

  it("/o hides results window before openSettings", async () => {
    const order: string[] = [];
    mockResultsHide.mockImplementation(async () => { order.push("hideResults"); });
    vi.mocked(api.openSettings).mockImplementation(async () => {
      order.push("openSettings");
    });

    const cmd = findCommand("/o");
    expect(cmd).toBeDefined();
    await cmd!.action();

    expect(order).toEqual(["hideResults", "openSettings"]);
    expect(api.openSettings).toHaveBeenCalledTimes(1);
    expect(vi.mocked(WebviewWindow.getByLabel)).toHaveBeenCalledWith("results");
  });

  it("/s hides all windows before rebuildIndex", async () => {
    const order: string[] = [];
    mockMainHide.mockImplementation(async () => { order.push("hideMain"); });
    mockResultsHide.mockImplementation(async () => { order.push("hideResults"); });
    vi.mocked(api.rebuildIndex).mockImplementation(async () => {
      order.push("rebuildIndex");
      return true;
    });

    const cmd = findCommand("/s");
    expect(cmd).toBeDefined();
    await cmd!.action();

    expect(order).toEqual(["hideMain", "hideResults", "rebuildIndex"]);
    expect(api.rebuildIndex).toHaveBeenCalledTimes(1);
  });

  it("/q does not call hide and calls quitApp", async () => {
    const cmd = findCommand("/q");
    expect(cmd).toBeDefined();
    await cmd!.action();

    expect(mockMainHide).not.toHaveBeenCalled();
    expect(mockResultsHide).not.toHaveBeenCalled();
    expect(api.quitApp).toHaveBeenCalledTimes(1);
  });
});
