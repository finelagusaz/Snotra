import { beforeEach, describe, expect, it, vi } from "vitest";
import * as api from "./invoke";
import { WebviewWindow } from "@tauri-apps/api/webviewWindow";
import { findCommand, SLASH_COMMANDS } from "./commands";

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
  openSettings: vi.fn(async () => {}),
  rebuildIndex: vi.fn(async () => true),
  quitApp: vi.fn(async () => {}),
}));

describe("SLASH_COMMANDS", () => {
  it("4つのコマンドを持つ", () => {
    expect(SLASH_COMMANDS).toHaveLength(4);
  });

  it("コマンド名が /r /o /s /q の順で定義されている", () => {
    expect(SLASH_COMMANDS.map((c) => c.command)).toEqual(["/r", "/o", "/s", "/q"]);
  });

  it("各エントリの label は command と同一", () => {
    for (const c of SLASH_COMMANDS) {
      expect(c.label).toBe(c.command);
    }
  });

  it("各エントリの description は空でない", () => {
    for (const c of SLASH_COMMANDS) {
      expect(c.description.trim().length).toBeGreaterThan(0);
    }
  });
});

describe("findCommand", () => {
  it("完全一致で見つかる", () => {
    expect(findCommand("/o")).toBeDefined();
    expect(findCommand("/o")!.command).toBe("/o");
  });

  it("前後の空白をトリムして一致する", () => {
    expect(findCommand("  /s  ")).toBeDefined();
    expect(findCommand("  /s  ")!.command).toBe("/s");
  });

  it("存在しないコマンドは undefined を返す", () => {
    expect(findCommand("/x")).toBeUndefined();
  });

  it("空文字は undefined を返す", () => {
    expect(findCommand("")).toBeUndefined();
  });

  it("部分一致は undefined を返す（prefix '/o ' 等）", () => {
    expect(findCommand("/o extra")).toBeUndefined();
  });

  it("大文字小文字は区別する（/O は見つからない）", () => {
    expect(findCommand("/O")).toBeUndefined();
  });
});

describe("slash command actions", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.mocked(WebviewWindow.getByLabel).mockResolvedValue({ hide: mockResultsHide } as never);
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
