import { beforeEach, describe, expect, it, vi } from "vitest";
import * as api from "./invoke";
import { findCommand, hideMainWindow, SLASH_COMMANDS } from "./commands";

const mockMainHide = vi.hoisted(() => vi.fn(async () => {}));
const mockSetLaunchNoticeWithAutoClear = vi.hoisted(() => vi.fn());

vi.mock("@tauri-apps/api/window", () => ({
  getCurrentWindow: vi.fn(() => ({ hide: mockMainHide })),
}));

vi.mock("./invoke", () => ({
  openSettings: vi.fn(async () => {}),
  rebuildIndex: vi.fn(async () => true),
  quitApp: vi.fn(async () => {}),
  notifyMainHidden: vi.fn(async () => {}),
}));

// search.ts はモジュールレベルで SolidJS リアクティブコード（createSignal 等）と
// requestAnimationFrame を使用するため、直接ロードするとテスト環境でエラーになる。
// commands.ts が setLaunchNoticeWithAutoClear を import するためここでモックが必要。
vi.mock("../stores/search", () => ({
  setLaunchNoticeWithAutoClear: mockSetLaunchNoticeWithAutoClear,
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
  });

  it("/o calls openSettings", async () => {
    const cmd = findCommand("/o");
    expect(cmd).toBeDefined();
    await cmd!.action();

    expect(api.openSettings).toHaveBeenCalledTimes(1);
    expect(mockMainHide).not.toHaveBeenCalled();
  });

  it("/o インデックス構築中エラー時に通知を表示する", async () => {
    vi.mocked(api.openSettings).mockRejectedValueOnce(new Error("indexing_in_progress"));
    const cmd = findCommand("/o");
    await cmd!.action();

    expect(mockSetLaunchNoticeWithAutoClear).toHaveBeenCalledTimes(1);
  });

  it("/o 予期せぬエラーは再スローする", async () => {
    vi.mocked(api.openSettings).mockRejectedValueOnce(new Error("unexpected_error"));
    const cmd = findCommand("/o");

    await expect(cmd!.action()).rejects.toThrow("unexpected_error");
  });

  it("/s hides main window before rebuildIndex", async () => {
    const order: string[] = [];
    mockMainHide.mockImplementation(async () => { order.push("hideMain"); });
    vi.mocked(api.rebuildIndex).mockImplementation(async () => {
      order.push("rebuildIndex");
      return true;
    });

    const cmd = findCommand("/s");
    expect(cmd).toBeDefined();
    await cmd!.action();

    expect(order).toEqual(["hideMain", "rebuildIndex"]);
    expect(api.rebuildIndex).toHaveBeenCalledTimes(1);
  });

  it("/q calls quitApp without hiding", async () => {
    const cmd = findCommand("/q");
    expect(cmd).toBeDefined();
    await cmd!.action();

    expect(mockMainHide).not.toHaveBeenCalled();
    expect(api.quitApp).toHaveBeenCalledTimes(1);
  });
});

describe("hideMainWindow", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  // #361: trim（notifyMainHidden 内の EmptyWorkingSet）は win.hide() 完了後に走らせる。
  // 可視中に trim するとレンダラがページを再 touch し working set 回収が削がれるため。
  it("hide 完了後に notifyMainHidden(trim) を呼ぶ", async () => {
    const order: string[] = [];
    mockMainHide.mockImplementation(async () => {
      order.push("hide");
    });
    vi.mocked(api.notifyMainHidden).mockImplementation(async () => {
      order.push("notify");
    });

    await hideMainWindow();

    expect(order).toEqual(["hide", "notify"]);
  });
});
