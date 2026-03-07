import { afterEach, beforeEach, describe, expect, it, vi, type Mock } from "vitest";

// ── ブラウザ API スタブ（Node 環境に存在しない・vi.hoisted でモジュールロード前に実行） ──
vi.hoisted(() => {
  globalThis.requestAnimationFrame = ((cb: Function) => setTimeout(cb, 0)) as typeof requestAnimationFrame;
  globalThis.cancelAnimationFrame = ((id: number) => clearTimeout(id)) as typeof cancelAnimationFrame;
});

// ── Tauri 依存をモック（search.ts のモジュールロード前に宣言） ──────────────

vi.mock("@tauri-apps/api/event", () => ({
  emit: vi.fn(async () => {}),
  listen: vi.fn(async () => () => {}),
}));

vi.mock("../lib/invoke", () => ({
  getMatchingTools: vi.fn(async () => []),
  search: vi.fn(async () => []),
  getHistoryResults: vi.fn(async () => []),
  launchItem: vi.fn(async () => ({ status: "ok", code: 0, message: null })),
  launchWithTool: vi.fn(async () => ({ status: "ok", code: 0, message: null })),
  listFolder: vi.fn(async () => []),
  getIndexingState: vi.fn(async () => false),
  recordFolderExpansion: vi.fn(async () => {}),
}));

// ── モック確立後にインポート ─────────────────────────────────────────────────

import * as eventApi from "@tauri-apps/api/event";
import * as api from "../lib/invoke";
import type { OpenerTool, SearchResult } from "../lib/types";
import {
  results,
  selected,
  query,
  setQuery,
  setSelected,
  setFolderFilter,
  folderFilter,
  activateSelected,
  enterToolSelection,
  exitToolSelection,
  moveSelectionDown,
  refreshResults,
  resetForShow,
  toolSelectionState,
} from "../stores/search";
import { setToolSelectionState } from "../stores/tool-selection";

// ── テスト定数 ────────────────────────────────────────────────────────────────

const FILE_RESULT: SearchResult = {
  name: "file.txt",
  path: "C:\\dir\\file.txt",
  isFolder: false,
  isError: false,
};

const TOOL_1: OpenerTool = { name: "Tool One", exe: "C:\\one.exe", args: "" };
const TOOL_2: OpenerTool = { name: "Tool Two", exe: "C:\\two.exe", args: "" };

// ── セットアップ ──────────────────────────────────────────────────────────────

beforeEach(() => {
  // debounce タイマーを制御し、テスト間でタイマーが漏れるのを防ぐ
  vi.useFakeTimers();
  vi.clearAllMocks();
  // search.ts の debounced refresh で呼ばれた場合に空を返す
  vi.mocked(api.search).mockResolvedValue([]);

  // シグナルをリセット
  setToolSelectionState(null);
  setQuery("");
  setSelected(0);
  setFolderFilter("");
});

afterEach(() => {
  vi.clearAllTimers();
  vi.useRealTimers();
});

// ── enterToolSelection ────────────────────────────────────────────────────────

describe("enterToolSelection", () => {
  it("ツール0件: toolSelectionState は変化せず activateSelected にフォールバック", async () => {
    vi.mocked(api.getMatchingTools).mockResolvedValue([]);

    await enterToolSelection(FILE_RESULT);

    expect(toolSelectionState()).toBeNull();
  });

  it("ツール1件: toolSelectionState は変化せず activateSelected にフォールバック", async () => {
    vi.mocked(api.getMatchingTools).mockResolvedValue([TOOL_1]);

    await enterToolSelection(FILE_RESULT);

    expect(toolSelectionState()).toBeNull();
  });

  it("ツール2件: toolSelectionState がセットされ results にツールが入る", async () => {
    vi.mocked(api.getMatchingTools).mockResolvedValue([TOOL_1, TOOL_2]);

    await enterToolSelection(FILE_RESULT);

    const frame = toolSelectionState();
    expect(frame).not.toBeNull();
    expect(frame!.targetPath).toBe(FILE_RESULT.path);
    expect(frame!.targetIsFolder).toBe(FILE_RESULT.isFolder);
    expect(frame!.tools).toEqual([TOOL_1, TOOL_2]);

    expect(results()).toHaveLength(2);
    expect(results()[0].name).toBe("Tool One");
    expect(results()[0].path).toBe("C:\\one.exe");
    expect(results()[1].name).toBe("Tool Two");
    expect(selected()).toBe(0);
  });

  it("ツール2件: savedQuery にその時点の query が保存される", async () => {
    vi.mocked(api.getMatchingTools).mockResolvedValue([TOOL_1, TOOL_2]);
    setQuery("my query");

    await enterToolSelection(FILE_RESULT);

    expect(toolSelectionState()!.savedQuery).toBe("my query");
  });

  it("ツール2件: savedFolderFilter にその時点の folderFilter が保存される", async () => {
    vi.mocked(api.getMatchingTools).mockResolvedValue([TOOL_1, TOOL_2]);
    setFolderFilter("*.log");

    await enterToolSelection(FILE_RESULT);

    expect(toolSelectionState()!.savedFolderFilter).toBe("*.log");
  });
});

// ── exitToolSelection ─────────────────────────────────────────────────────────

describe("exitToolSelection", () => {
  it("toolSelectionState が null のとき false を返す（ガード）", () => {
    expect(exitToolSelection()).toBe(false);
  });

  it("toolSelectionState が設定されているとき true を返し toolSelectionState を null にする", () => {
    setToolSelectionState({
      targetPath: "C:\\foo.txt",
      targetIsFolder: false,
      tools: [TOOL_1, TOOL_2],
      savedResults: [],
      savedSelected: 0,
      savedQuery: "",
      savedFolderFilter: "",
    });

    const ret = exitToolSelection();

    expect(ret).toBe(true);
    expect(toolSelectionState()).toBeNull();
  });

  it("savedResults と savedSelected が復元される", () => {
    const savedResults: SearchResult[] = [FILE_RESULT];
    setToolSelectionState({
      targetPath: "C:\\foo.txt",
      targetIsFolder: false,
      tools: [TOOL_1, TOOL_2],
      savedResults,
      savedSelected: 0,
      savedQuery: "",
      savedFolderFilter: "",
    });

    exitToolSelection();

    expect(results()).toEqual(savedResults);
    expect(selected()).toBe(0);
  });

  it("savedFolderFilter が復元される（C1: フォルダ展開中の Escape 復帰）", () => {
    setToolSelectionState({
      targetPath: "C:\\dir\\sub",
      targetIsFolder: true,
      tools: [TOOL_1, TOOL_2],
      savedResults: [],
      savedSelected: 0,
      savedQuery: "",
      savedFolderFilter: "*.log",
    });

    exitToolSelection();

    expect(folderFilter()).toBe("*.log");
  });
});

// ── resetForShow ─────────────────────────────────────────────────────────────

describe("resetForShow", () => {
  it("ツール選択状態を null にリセットする", () => {
    setToolSelectionState({
      targetPath: "C:\\foo.txt",
      targetIsFolder: false,
      tools: [TOOL_1, TOOL_2],
      savedResults: [],
      savedSelected: 0,
      savedQuery: "before",
      savedFolderFilter: "",
    });

    resetForShow();

    expect(toolSelectionState()).toBeNull();
  });

  it("query / folderFilter / selected もリセットされる", () => {
    setQuery("hello");
    setFolderFilter("*.log");
    setSelected(2);

    resetForShow();

    expect(query()).toBe("");
    expect(folderFilter()).toBe("");
    expect(selected()).toBe(0);
  });

  it("ツール選択中の再表示でも同様にリセットされる", () => {
    setToolSelectionState({
      targetPath: "C:\\bar.exe",
      targetIsFolder: false,
      tools: [TOOL_1, TOOL_2],
      savedResults: [FILE_RESULT],
      savedSelected: 1,
      savedQuery: "bar",
      savedFolderFilter: "*.txt",
    });
    setQuery("bar");
    setFolderFilter("*.txt");
    setSelected(1);

    resetForShow();

    expect(toolSelectionState()).toBeNull();
    expect(query()).toBe("");
    expect(folderFilter()).toBe("");
    expect(selected()).toBe(0);
  });

  it("クリーン状態では結果イベントを emit しない", async () => {
    // 初期状態: query="", folderState=null, toolSelectionState=null
    const emitMock = eventApi.emit as Mock;
    emitMock.mockClear();

    resetForShow();

    // runRefresh() がスキップされるため IPC は発生しない
    await vi.runAllTimersAsync();
    const resultsCalls = emitMock.mock.calls.filter((args) =>
      args[0] === "results-data-changed" ||
      args[0] === "results-selection-changed" ||
      args[0] === "results-visibility-changed"
    );
    expect(resultsCalls).toHaveLength(0);
  });

  it("クエリが非空なら結果イベントを emit する", async () => {
    setQuery("hello");
    const emitMock = eventApi.emit as Mock;
    emitMock.mockClear();

    resetForShow();

    // runRefresh() が走るため結果イベントが発生する
    await vi.runAllTimersAsync();
    const resultsCalls = emitMock.mock.calls.filter((args) =>
      args[0] === "results-data-changed" ||
      args[0] === "results-visibility-changed"
    );
    expect(resultsCalls.length).toBeGreaterThan(0);
  });
});

// ── activateSelected — ツール選択委譲 ────────────────────────────────────────

describe("activateSelected — ツール選択委譲", () => {
  it("ツール選択中は launchWithTool を呼ぶ", async () => {
    setToolSelectionState({
      targetPath: "C:\\doc.pdf",
      targetIsFolder: false,
      tools: [TOOL_1],
      savedResults: [],
      savedSelected: 0,
      savedQuery: "doc",
      savedFolderFilter: "",
    });
    // selected() === 0 → TOOL_1 が起動対象

    await activateSelected();

    expect(api.launchWithTool).toHaveBeenCalledOnce();
    expect(api.launchWithTool).toHaveBeenCalledWith(
      "C:\\doc.pdf",
      "doc",
      TOOL_1.exe,
      TOOL_1.args,
    );
  });

  it("ツール選択中は launchItem を呼ばない", async () => {
    setToolSelectionState({
      targetPath: "C:\\doc.pdf",
      targetIsFolder: false,
      tools: [TOOL_1],
      savedResults: [],
      savedSelected: 0,
      savedQuery: "doc",
      savedFolderFilter: "",
    });

    await activateSelected();

    expect(api.launchItem).not.toHaveBeenCalled();
  });
});

// ── launchWithSelectedTool 成功後のクリーンアップ ─────────────────────────────

describe("launchWithSelectedTool 成功後のクリーンアップ", () => {
  it("toolSelectionState / results / selected / folderFilter がクリアされる", async () => {
    setToolSelectionState({
      targetPath: "C:\\img.png",
      targetIsFolder: false,
      tools: [TOOL_1, TOOL_2],
      savedResults: [FILE_RESULT],
      savedSelected: 1,
      savedQuery: "img",
      savedFolderFilter: "*.png",
    });
    setFolderFilter("*.png");
    setSelected(1);

    await activateSelected();

    expect(toolSelectionState()).toBeNull();
    expect(results()).toEqual([]);
    expect(selected()).toBe(0);
    expect(folderFilter()).toBe("");
  });
});

// ── refreshResults ガード (C3) ───────────────────────────────────────────────

describe("refreshResults ガード (C3)", () => {
  it("ツール選択中は api.search を呼ばない", async () => {
    setToolSelectionState({
      targetPath: "C:\\foo.txt",
      targetIsFolder: false,
      tools: [TOOL_1, TOOL_2],
      savedResults: [],
      savedSelected: 0,
      savedQuery: "test",
      savedFolderFilter: "",
    });
    // query を "test" にしても refreshResults が早期リターンするはず
    setQuery("test");

    await refreshResults();

    expect(api.search).not.toHaveBeenCalled();
  });

  it("ツール選択中は api.listFolder も呼ばない（フォルダ展開中の C3）", async () => {
    setToolSelectionState({
      targetPath: "C:\\dir\\sub",
      targetIsFolder: true,
      tools: [TOOL_1, TOOL_2],
      savedResults: [],
      savedSelected: 0,
      savedQuery: "",
      savedFolderFilter: "",
    });

    await refreshResults();

    expect(api.listFolder).not.toHaveBeenCalled();
  });
});

// ── selection-only IPC 軽量化 (#162) ──────────────────────────────────────────

describe("selection-only IPC (#162)", () => {
  it("moveSelectionDown は results-selection-changed のみ emit し results-data-changed は emit しない", async () => {
    // 結果を2件セットアップ
    const items: SearchResult[] = [
      { name: "a.txt", path: "C:\\a.txt", isFolder: false, isError: false },
      { name: "b.txt", path: "C:\\b.txt", isFolder: false, isError: false },
    ];
    vi.mocked(api.search).mockResolvedValue(items);
    setQuery("test");
    await vi.runAllTimersAsync();

    const emitMock = eventApi.emit as Mock;
    emitMock.mockClear();

    moveSelectionDown();

    const dataCalls = emitMock.mock.calls.filter((args) => args[0] === "results-data-changed");
    const selectionCalls = emitMock.mock.calls.filter((args) => args[0] === "results-selection-changed");

    expect(dataCalls).toHaveLength(0);
    expect(selectionCalls).toHaveLength(1);
    expect(selectionCalls[0][1]).toMatchObject({ selected: 1 });
  });
});
