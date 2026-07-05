import { beforeEach, describe, expect, it, vi } from "vitest";

// ── Tauri invoke モック ────────────────────────────────────────────────────────
// 目的: 各ラッパー関数が「正しいコマンド名 + 正しい引数キー」で invoke を呼ぶことを
// 固定し、Rust 側 #[tauri::command] とのドリフト（改名・引数キー変更）を検知する。
const { mockInvoke } = vi.hoisted(() => ({
  mockInvoke: vi.fn(async (): Promise<unknown> => undefined),
}));
vi.mock("@tauri-apps/api/core", () => ({ invoke: mockInvoke }));
vi.mock("./trace", () => ({ trace: vi.fn() }));

// ── モック確立後にインポート ──────────────────────────────────────────────────
import * as api from "./invoke";

beforeEach(() => {
  mockInvoke.mockClear();
  mockInvoke.mockResolvedValue(undefined);
});

// ── IPC 契約テーブル ──────────────────────────────────────────────────────────
// command は Rust 側の関数名（snake_case）、args のキーは Rust 側引数の camelCase 版
// （Tauri v2 は camelCase → snake_case を自動変換する）。
// Rust 側でコマンドを改名・引数変更したら、このテーブルの期待値も同時に更新すること。
const CONTRACT: Array<{
  fn: string;
  call: () => Promise<unknown>;
  command: string;
  args: Record<string, unknown> | undefined;
}> = [
  {
    fn: "search",
    call: () => api.search("firefox"),
    command: "search",
    args: { query: "firefox" },
  },
  {
    fn: "getHistoryResults",
    call: () => api.getHistoryResults(),
    command: "get_history_results",
    args: undefined,
  },
  {
    fn: "launchItem",
    call: () => api.launchItem("C:\\app.exe", "app"),
    command: "launch_item",
    args: { path: "C:\\app.exe", query: "app" },
  },
  {
    fn: "listFolder",
    call: () => api.listFolder("C:\\dir", "*.txt"),
    command: "list_folder",
    args: { dir: "C:\\dir", filter: "*.txt" },
  },
  {
    fn: "getBootstrapPayload",
    call: () => api.getBootstrapPayload(),
    command: "get_bootstrap_payload",
    args: undefined,
  },
  {
    fn: "getIconsBatch",
    call: () => api.getIconsBatch(["C:\\a.exe", "C:\\b.exe"]),
    command: "get_icons_batch",
    args: { paths: ["C:\\a.exe", "C:\\b.exe"] },
  },
  {
    fn: "openSettings",
    call: () => api.openSettings(),
    command: "open_settings",
    args: undefined,
  },
  {
    fn: "saveSearchPlacement",
    call: () => api.saveSearchPlacement(),
    command: "save_search_placement",
    args: undefined,
  },
  {
    fn: "notifyMainShown",
    call: () => api.notifyMainShown(),
    command: "notify_main_shown",
    args: undefined,
  },
  {
    fn: "notifyMainHidden",
    call: () => api.notifyMainHidden(),
    command: "notify_main_hidden",
    args: undefined,
  },
  {
    fn: "getIndexingState",
    call: () => api.getIndexingState(),
    command: "get_indexing_state",
    args: undefined,
  },
  {
    fn: "rebuildIndex",
    call: () => api.rebuildIndex(),
    command: "rebuild_index",
    args: undefined,
  },
  {
    fn: "quitApp",
    call: () => api.quitApp(),
    command: "quit_app",
    args: undefined,
  },
  {
    fn: "recordFolderExpansion",
    call: () => api.recordFolderExpansion("C:\\dir"),
    command: "record_folder_expansion",
    args: { path: "C:\\dir" },
  },
  {
    fn: "getMatchingTools",
    call: () => api.getMatchingTools("C:\\file.txt", false),
    command: "get_matching_tools",
    args: { path: "C:\\file.txt", isFolder: false },
  },
  {
    fn: "launchWithTool",
    call: () => api.launchWithTool("C:\\f.txt", "f", "C:\\tool.exe", "-a"),
    command: "launch_with_tool",
    args: { path: "C:\\f.txt", query: "f", toolExe: "C:\\tool.exe", toolArgs: "-a" },
  },
  {
    fn: "getInstantCommands",
    call: () => api.getInstantCommands("goo"),
    command: "get_instant_commands",
    args: { prefixInput: "goo" },
  },
  {
    fn: "executeInstantCommand",
    call: () => api.executeInstantCommand("google", "@google rust"),
    command: "execute_instant_command",
    args: { name: "google", query: "@google rust" },
  },
];

describe("invoke.ts IPC 契約（コマンド名 + 引数キー）", () => {
  for (const c of CONTRACT) {
    it(`${c.fn} → invoke("${c.command}")`, async () => {
      await c.call();

      expect(mockInvoke).toHaveBeenCalledTimes(1);
      if (c.args === undefined) {
        expect(mockInvoke).toHaveBeenCalledWith(c.command, undefined);
      } else {
        expect(mockInvoke).toHaveBeenCalledWith(c.command, c.args);
      }
    });
  }

  it("契約テーブルは invoke.ts の公開ラッパーを網羅する（追加漏れ検知）", async () => {
    // invoke.ts に新しいラッパーを追加したら CONTRACT にも行を追加すること。
    const exported = Object.entries(api)
      .filter(([, v]) => typeof v === "function")
      .map(([k]) => k)
      .sort();
    const covered = CONTRACT.map((c) => c.fn).sort();
    expect(exported).toEqual(covered);
  });

  it("invoke の解決値がそのまま返る（search）", async () => {
    const items = [{ name: "a", path: "C:\\a", isFolder: false, isError: false }];
    mockInvoke.mockResolvedValueOnce(items);

    await expect(api.search("a")).resolves.toBe(items);
  });

  it("invoke の reject がそのまま伝播する", async () => {
    mockInvoke.mockRejectedValueOnce(new Error("ipc down"));

    await expect(api.search("a")).rejects.toThrow("ipc down");
  });
});
