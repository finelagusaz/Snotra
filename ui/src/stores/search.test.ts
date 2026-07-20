import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

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
  getInstantCommands: vi.fn(async () => []),
  executeInstantCommand: vi.fn(async () => ({ status: "ok", code: 0, message: null })),
}));

// perf 計測を spy 化（requestId 相関の検証用。node 環境では実体は no-op だが呼び出し引数は観測できる）
vi.mock("../lib/perf", () => ({
  perfMarkInput: vi.fn(),
  perfStartSearch: vi.fn(),
  perfMarkSearchDone: vi.fn(),
  perfCancelSearch: vi.fn(),
  perfMarkRenderDone: vi.fn(),
}));

// ── モック確立後にインポート ─────────────────────────────────────────────────

import * as tauriEvent from "@tauri-apps/api/event";
import * as api from "../lib/invoke";
import type { InstantCommand, OpenerTool, SearchResult } from "../lib/types";
import {
  results,
  selected,
  query,
  setQuery,
  dispatchQueryInput,
  setSelected,
  setFolderFilter,
  folderFilter,
  activateSelected,
  activateSelectedByIndex,
  enterToolSelection,
  exitToolSelection,
  enterFolderExpansion,
  exitFolderExpansion,
  moveSelectionDown,
  refreshResults,
  resetForShow,
  shouldShowResults,
  viewKind,
  interpKind,
  allowsFolderNav,
  toolSelectionState,
  launchNotice,
  clearLaunchNotice,
  setHotkeyFailureNotice,
  indexing,
  initIndexingState,
  setInstantCommandPrefix,
  getSearchGeneration,
} from "../stores/search";
import { setToolSelectionState } from "../stores/tool-selection";
import { folderState, setFolderState } from "../stores/folder";
import { getInstantCommandItems } from "../stores/instantCommand";
import { perfStartSearch, perfMarkSearchDone } from "../lib/perf";

// ── テスト定数 ────────────────────────────────────────────────────────────────

const FILE_RESULT: SearchResult = {
  name: "file.txt",
  path: "C:\\dir\\file.txt",
  isFolder: false,
  isError: false,
};

const TOOL_1: OpenerTool = { name: "Tool One", exe: "C:\\one.exe", args: "" };
const TOOL_2: OpenerTool = { name: "Tool Two", exe: "C:\\two.exe", args: "" };

const CMD_GOOGLE: InstantCommand = { name: "google", display: "https://google.com?q={query}", description: "Google 検索" };
const CMD_CLIP: InstantCommand = { name: "clip", display: "echo {clip}", description: "" };

// ── セットアップ ──────────────────────────────────────────────────────────────

beforeEach(() => {
  // debounce タイマーを制御し、テスト間でタイマーが漏れるのを防ぐ
  vi.useFakeTimers();
  vi.clearAllMocks();
  // search.ts の debounced refresh で呼ばれた場合に空を返す
  vi.mocked(api.search).mockResolvedValue([]);

  // シグナルをリセット
  setToolSelectionState(null);
  setFolderState(null);
  setQuery("");
  setSelected(0);
  setFolderFilter("");
  setInstantCommandPrefix("@");
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

  it("ツール2件: launchQuery にその時点の query が保存される", async () => {
    vi.mocked(api.getMatchingTools).mockResolvedValue([TOOL_1, TOOL_2]);
    setQuery("my query");

    await enterToolSelection(FILE_RESULT);

    expect(toolSelectionState()!.launchQuery).toBe("my query");
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
      kind: "tool",
      targetPath: "C:\\foo.txt",
      targetIsFolder: false,
      tools: [TOOL_1, TOOL_2],
      savedResults: [],
      savedSelected: 0,
      launchQuery: "",
      savedFolderFilter: "",
    });

    const ret = exitToolSelection();

    expect(ret).toBe(true);
    expect(toolSelectionState()).toBeNull();
  });

  it("savedResults と savedSelected が復元される", () => {
    const savedResults: SearchResult[] = [FILE_RESULT];
    setToolSelectionState({
      kind: "tool",
      targetPath: "C:\\foo.txt",
      targetIsFolder: false,
      tools: [TOOL_1, TOOL_2],
      savedResults,
      savedSelected: 0,
      launchQuery: "",
      savedFolderFilter: "",
    });

    exitToolSelection();

    expect(results()).toEqual(savedResults);
    expect(selected()).toBe(0);
  });

  it("savedFolderFilter が復元される（C1: フォルダ展開中の Escape 復帰）", () => {
    setToolSelectionState({
      kind: "tool",
      targetPath: "C:\\dir\\sub",
      targetIsFolder: true,
      tools: [TOOL_1, TOOL_2],
      savedResults: [],
      savedSelected: 0,
      launchQuery: "",
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
      kind: "tool",
      targetPath: "C:\\foo.txt",
      targetIsFolder: false,
      tools: [TOOL_1, TOOL_2],
      savedResults: [],
      savedSelected: 0,
      launchQuery: "before",
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
      kind: "tool",
      targetPath: "C:\\bar.exe",
      targetIsFolder: false,
      tools: [TOOL_1, TOOL_2],
      savedResults: [FILE_RESULT],
      savedSelected: 1,
      launchQuery: "bar",
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

  it("クリーン状態では api.search を呼ばない", async () => {
    // 初期状態: query="", folderState=null, toolSelectionState=null
    vi.mocked(api.search).mockClear();

    resetForShow();

    // runRefresh() がスキップされるため search は呼ばれない
    await vi.runAllTimersAsync();
    expect(api.search).not.toHaveBeenCalled();
  });

  it("クエリが非空でも resetForShow 後はクエリが空になり search は呼ばれない", async () => {
    setQuery("hello");
    vi.mocked(api.search).mockClear();

    resetForShow();

    expect(query()).toBe("");
    await vi.runAllTimersAsync();
    expect(api.search).not.toHaveBeenCalled();
  });
});

// ── activateSelected — ツール選択委譲 ────────────────────────────────────────

describe("activateSelected — ツール選択委譲", () => {
  it("ツール選択中は launchWithTool を呼ぶ", async () => {
    setToolSelectionState({
      kind: "tool",
      targetPath: "C:\\doc.pdf",
      targetIsFolder: false,
      tools: [TOOL_1],
      savedResults: [],
      savedSelected: 0,
      launchQuery: "doc",
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
      kind: "tool",
      targetPath: "C:\\doc.pdf",
      targetIsFolder: false,
      tools: [TOOL_1],
      savedResults: [],
      savedSelected: 0,
      launchQuery: "doc",
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
      kind: "tool",
      targetPath: "C:\\img.png",
      targetIsFolder: false,
      tools: [TOOL_1, TOOL_2],
      savedResults: [FILE_RESULT],
      savedSelected: 1,
      launchQuery: "img",
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
      kind: "tool",
      targetPath: "C:\\foo.txt",
      targetIsFolder: false,
      tools: [TOOL_1, TOOL_2],
      savedResults: [],
      savedSelected: 0,
      launchQuery: "test",
      savedFolderFilter: "",
    });
    // query を "test" にしても refreshResults が早期リターンするはず
    setQuery("test");

    // 早期 return（searchLane.run() の前）なので world 世代は進まない（#534 の核心不変条件）
    const genBefore = getSearchGeneration();
    await refreshResults();

    expect(api.search).not.toHaveBeenCalled();
    expect(getSearchGeneration()).toBe(genBefore);
  });

  it("ツール選択中は api.listFolder も呼ばない（フォルダ展開中の C3）", async () => {
    setToolSelectionState({
      kind: "tool",
      targetPath: "C:\\dir\\sub",
      targetIsFolder: true,
      tools: [TOOL_1, TOOL_2],
      savedResults: [],
      savedSelected: 0,
      launchQuery: "",
      savedFolderFilter: "",
    });

    await refreshResults();

    expect(api.listFolder).not.toHaveBeenCalled();
  });
});

// ── selection シグナル更新 ───────────────────────────────────────────────────

describe("selection シグナル更新", () => {
  it("moveSelectionDown は selected シグナルを更新する", async () => {
    // 結果を2件セットアップ
    const items: SearchResult[] = [
      { name: "a.txt", path: "C:\\a.txt", isFolder: false, isError: false },
      { name: "b.txt", path: "C:\\b.txt", isFolder: false, isError: false },
    ];
    vi.mocked(api.search).mockResolvedValue(items);
    dispatchQueryInput("test");
    await vi.runAllTimersAsync();

    expect(selected()).toBe(0);
    moveSelectionDown();
    expect(selected()).toBe(1);
  });
});

// ── setHotkeyFailureNotice ────────────────────────────────────────────────────

describe("setHotkeyFailureNotice", () => {
  beforeEach(() => {
    clearLaunchNotice();
  });

  it("呼び出し直後に launchNotice が設定される", () => {
    setHotkeyFailureNotice("ホットキー (Alt+Q) の登録に失敗しました");
    expect(launchNotice()).toBe("ホットキー (Alt+Q) の登録に失敗しました");
  });

  it("5000ms 後に launchNotice が自動クリアされる", () => {
    setHotkeyFailureNotice("テスト通知");
    expect(launchNotice()).not.toBeNull();
    vi.advanceTimersByTime(4999);
    expect(launchNotice()).not.toBeNull();
    vi.advanceTimersByTime(1);
    expect(launchNotice()).toBeNull();
  });

  it("resetForShow() で即座にクリアされる", () => {
    setHotkeyFailureNotice("テスト通知");
    expect(launchNotice()).not.toBeNull();
    resetForShow();
    expect(launchNotice()).toBeNull();
  });

  it("連続呼び出しで前のタイマーがキャンセルされ最後の通知だけが残る", () => {
    setHotkeyFailureNotice("1回目");
    vi.advanceTimersByTime(3000);
    setHotkeyFailureNotice("2回目");
    // 3000ms 追加（1回目から合計 6000ms）しても 2回目は 5000ms 未満のため残る
    vi.advanceTimersByTime(3000);
    expect(launchNotice()).toBe("2回目");
    // 2回目のタイマーが切れるまで待つ（2回目から 5000ms）
    vi.advanceTimersByTime(2000);
    expect(launchNotice()).toBeNull();
  });
});

// ── initIndexingState / indexing signal ──────────────────────────────────────

describe("initIndexingState", () => {
  // 各テストで listen コールバックをキャプチャするための Map
  let eventCallbacks: Map<string, () => void>;

  beforeEach(() => {
    eventCallbacks = new Map();
    vi.mocked(tauriEvent.listen).mockImplementation(
      async (eventName, callback) => {
        eventCallbacks.set(eventName, callback as () => void);
        return () => {};
      },
    );
  });

  it("indexing-started イベントを受信すると indexing() が true になる", async () => {
    await initIndexingState();
    expect(indexing()).toBe(false); // getIndexingState モックは false を返す

    eventCallbacks.get("indexing-started")?.();

    expect(indexing()).toBe(true);
  });

  it("indexing-started → indexing-complete のシーケンスで false に戻る", async () => {
    await initIndexingState();

    eventCallbacks.get("indexing-started")?.();
    expect(indexing()).toBe(true);

    eventCallbacks.get("indexing-complete")?.();
    expect(indexing()).toBe(false);
  });

  it("cleanup 関数を呼ぶと indexing-started の unlisten が実行される", async () => {
    const unlistenStarted = vi.fn<() => void>();
    vi.mocked(tauriEvent.listen).mockImplementation(
      async (eventName, callback) => {
        eventCallbacks.set(eventName, callback as () => void);
        return eventName === "indexing-started" ? unlistenStarted : vi.fn<() => void>();
      },
    );

    const cleanup = await initIndexingState();
    cleanup();

    expect(unlistenStarted).toHaveBeenCalledOnce();
  });
});

// ── instant モード（interpKind 純粋導出） ──────────────────────────────────────

describe("instant モード", () => {
  it("@prefix でインスタントコマンドモードに入る", async () => {
    vi.mocked(api.getInstantCommands).mockResolvedValue([CMD_GOOGLE, CMD_CLIP]);

    dispatchQueryInput("@goo");
    await vi.runAllTimersAsync();

    expect(interpKind()).toBe("instant");
    expect(results()).toHaveLength(2);
    expect(results()[0].name).toBe("google");
  });

  it("コマンド名だけでフィルタリングされる（スペース後はクエリ部分）", async () => {
    vi.mocked(api.getInstantCommands).mockResolvedValue([CMD_GOOGLE]);

    dispatchQueryInput("@google SolidJS tutorial");
    await vi.runAllTimersAsync();

    // getInstantCommands は "google" でフィルタされる（スペース以降は除外）
    expect(api.getInstantCommands).toHaveBeenCalledWith("google");
  });

  it("プレフィックスを消すとモード解除", async () => {
    vi.mocked(api.getInstantCommands).mockResolvedValue([CMD_GOOGLE]);
    setQuery("@goo");
    await vi.runAllTimersAsync();
    expect(interpKind()).toBe("instant");

    setQuery("goo");
    await vi.runAllTimersAsync();
    expect(interpKind()).toBe("plain");
  });

  it("resetForShow でモード解除", async () => {
    vi.mocked(api.getInstantCommands).mockResolvedValue([CMD_GOOGLE]);
    setQuery("@goo");
    await vi.runAllTimersAsync();
    expect(interpKind()).toBe("instant");

    resetForShow();
    expect(interpKind()).toBe("plain");
  });
});

// ── executeInstantCommandSelected（activateSelected 経由）────────────────────

describe("executeInstantCommandSelected", () => {
  beforeEach(async () => {
    vi.mocked(api.getInstantCommands).mockResolvedValue([CMD_GOOGLE, CMD_CLIP]);
    dispatchQueryInput("@google SolidJS");
    await vi.runAllTimersAsync();
    // instant モード（interpKind = "instant"）, results に 2 件入っている状態
  });

  it("成功: クエリクリア + モード解除", async () => {
    vi.mocked(api.executeInstantCommand).mockResolvedValue({
      status: "ok",
      code: 0,
      message: null,
    });

    vi.mocked(api.search).mockClear();
    const ok = await activateSelected();

    expect(ok).toBe(true);
    expect(api.executeInstantCommand).toHaveBeenCalledWith("google", "SolidJS");
    expect(interpKind()).toBe("plain");
    expect(query()).toBe("");
    // instant 成功で query を空へ戻したとき、余計な検索を発火しない（旧 suppress の意図・#537 回帰ガード）。
    // raw setQuery("") は dispatchQueryInput を経由しないため plain 検索は起動しない。
    await vi.runAllTimersAsync();
    expect(api.search).not.toHaveBeenCalled();
  });

  it("失敗: 候補が復元される", async () => {
    vi.mocked(api.executeInstantCommand).mockResolvedValue({
      status: "failed",
      code: 1,
      message: "command not found",
    });
    const resultsBefore = results();
    const selectedBefore = selected();

    const ok = await activateSelected();

    expect(ok).toBe(false);
    expect(interpKind()).toBe("instant");
    expect(results()).toEqual(resultsBefore);
    expect(selected()).toBe(selectedBefore);
  });

  it("activateSelectedByIndex でインデックス指定実行", async () => {
    vi.mocked(api.executeInstantCommand).mockResolvedValue({
      status: "ok",
      code: 0,
      message: null,
    });

    await activateSelectedByIndex(1);

    // index 1 = CMD_CLIP
    expect(api.executeInstantCommand).toHaveBeenCalledWith("clip", "SolidJS");
  });
});

// ── refreshResults ガード — インスタントコマンドモード ────────────────────────

describe("refreshResults ガード — インスタントコマンドモード", () => {
  it("インスタントコマンドモード中は api.search を呼ばない", async () => {
    vi.mocked(api.getInstantCommands).mockResolvedValue([CMD_GOOGLE]);
    setQuery("@goo");
    await vi.runAllTimersAsync();
    expect(interpKind()).toBe("instant");
    vi.mocked(api.search).mockClear();

    // 早期 return（searchLane.run() の前）なので world 世代は進まない（#534 の核心不変条件）
    const genBefore = getSearchGeneration();
    await refreshResults();

    expect(api.search).not.toHaveBeenCalled();
    expect(getSearchGeneration()).toBe(genBefore);
  });
});

// ── shouldShowResults ─────────────────────────────────────────────────────────

describe("shouldShowResults", () => {
  it("結果なし → false", () => {
    expect(shouldShowResults()).toBe(false);
  });

  it("結果あり + indexing=false → true", async () => {
    vi.mocked(api.search).mockResolvedValue([FILE_RESULT]);
    dispatchQueryInput("file");
    await vi.runAllTimersAsync();

    expect(shouldShowResults()).toBe(true);
  });

  it("結果あり + indexing=true → false（通常検索）", async () => {
    // indexing を true にする
    let setIndexingTrue: (() => void) | undefined;
    vi.mocked(tauriEvent.listen).mockImplementation(
      async (eventName, callback) => {
        if (eventName === "indexing-started") setIndexingTrue = callback as () => void;
        return () => {};
      },
    );
    await initIndexingState();
    setIndexingTrue?.();
    expect(indexing()).toBe(true);

    // 結果を直接セットするために search をモック
    vi.mocked(api.search).mockResolvedValue([FILE_RESULT]);
    dispatchQueryInput("file");
    await vi.runAllTimersAsync();

    expect(shouldShowResults()).toBe(false);
  });

  it("結果あり + indexing=true + instant → true", async () => {
    let setIndexingTrue: (() => void) | undefined;
    vi.mocked(tauriEvent.listen).mockImplementation(
      async (eventName, callback) => {
        if (eventName === "indexing-started") setIndexingTrue = callback as () => void;
        return () => {};
      },
    );
    await initIndexingState();
    setIndexingTrue?.();

    vi.mocked(api.getInstantCommands).mockResolvedValue([CMD_GOOGLE]);
    dispatchQueryInput("@goo");
    await vi.runAllTimersAsync();

    expect(indexing()).toBe(true);
    expect(interpKind()).toBe("instant");
    expect(results().length).toBeGreaterThan(0);
    expect(shouldShowResults()).toBe(true);
  });

  it("結果あり + indexing=true + folderState → true", async () => {
    let setIndexingTrue: (() => void) | undefined;
    vi.mocked(tauriEvent.listen).mockImplementation(
      async (eventName, callback) => {
        if (eventName === "indexing-started") setIndexingTrue = callback as () => void;
        return () => {};
      },
    );
    await initIndexingState();
    setIndexingTrue?.();

    // folderState を設定して結果をセット
    setFolderState(FOLDER_FRAME);
    vi.mocked(api.listFolder).mockResolvedValue([FILE_RESULT]);
    await refreshResults();
    await vi.runAllTimersAsync();

    expect(indexing()).toBe(true);
    expect(shouldShowResults()).toBe(true);
  });

  it("結果あり + indexing=true + toolSelectionState → true（tool 枝の明示化）", async () => {
    let setIndexingTrue: (() => void) | undefined;
    vi.mocked(tauriEvent.listen).mockImplementation(
      async (eventName, callback) => {
        if (eventName === "indexing-started") setIndexingTrue = callback as () => void;
        return () => {};
      },
    );
    await initIndexingState();
    setIndexingTrue?.();
    expect(indexing()).toBe(true);

    // ツール選択中は results にツール一覧が入る（folderState は null）。
    // tool×indexing は通常 UI では到達不能だが、tool 枝が「常に表示」であることを単体で固定する。
    vi.mocked(api.getMatchingTools).mockResolvedValue([TOOL_1, TOOL_2]);
    await enterToolSelection(FILE_RESULT);

    expect(toolSelectionState()).not.toBeNull();
    expect(results().length).toBe(2);
    expect(shouldShowResults()).toBe(true);
  });
});

// ── viewKind（軸1: ビューフレーム射影） ───────────────────────────────────────

const FOLDER_FRAME = {
  kind: "folder" as const,
  currentDir: "C:\\test",
  savedResults: [] as SearchResult[],
  savedSelected: 0,
  restoreQuery: "",
};

const TOOL_FRAME = {
  kind: "tool" as const,
  targetPath: "C:\\foo.txt",
  targetIsFolder: false,
  tools: [TOOL_1, TOOL_2],
  savedResults: [] as SearchResult[],
  savedSelected: 0,
  launchQuery: "",
  savedFolderFilter: "",
};

describe("viewKind", () => {
  it("通常時は 'results'", () => {
    expect(viewKind()).toBe("results");
  });

  it("folderState セット時は 'folder'", () => {
    setFolderState(FOLDER_FRAME);
    expect(viewKind()).toBe("folder");
  });

  it("toolSelectionState セット時は 'tool'", () => {
    setToolSelectionState(TOOL_FRAME);
    expect(viewKind()).toBe("tool");
  });

  it("tool が folder の上に積まれた場合は 'tool' を優先（SPEC §18.5 直交・優先度）", () => {
    setFolderState(FOLDER_FRAME);
    setToolSelectionState(TOOL_FRAME);
    expect(viewKind()).toBe("tool");
  });
});

// ── interpKind（軸2: 入力の意味・results 限定で非 plain） ──────────────────────

describe("interpKind", () => {
  it("通常クエリは 'plain'", () => {
    setQuery("hello");
    expect(interpKind()).toBe("plain");
  });

  it("'/' プレフィックス（非コマンド）は 'command'", () => {
    setQuery("/xyz");
    expect(interpKind()).toBe("command");
  });

  it("'@' プレフィックスでインスタントコマンドモードなら 'instant'", async () => {
    vi.mocked(api.getInstantCommands).mockResolvedValue([CMD_GOOGLE]);
    setQuery("@goo");
    await vi.runAllTimersAsync();

    expect(interpKind()).toBe("instant");
  });

  it("viewKind が results 以外なら常に 'plain'（folder 中の '/' でも plain）", () => {
    setFolderState(FOLDER_FRAME);
    setQuery("/xyz");
    expect(interpKind()).toBe("plain");
  });

  it("純粋導出: latch を介さず query から同期的に 'instant' を導出する", () => {
    // runAllTimersAsync 不要 — interpKind は query+prefix の純粋関数（持続シグナル非依存）。
    setQuery("@goo");
    expect(interpKind()).toBe("instant");
    setQuery("goo");
    expect(interpKind()).toBe("plain");
  });

  it("空 prefix では instant 化しない（prefix && ガード）", () => {
    setInstantCommandPrefix("");
    setQuery("@goo");
    expect(interpKind()).toBe("plain");
  });
});

// ── allowsFolderNav（Phase 2: 複合条件の一本化） ──────────────────────────────

describe("allowsFolderNav", () => {
  it("通常モードでは true", () => {
    expect(allowsFolderNav()).toBe(true);
  });

  it("folder モード中は true（フォルダ内でのさらなる展開・離脱は許可）", () => {
    setFolderState(FOLDER_FRAME);
    expect(allowsFolderNav()).toBe(true);
  });

  it("tool 選択中は false", () => {
    setToolSelectionState(TOOL_FRAME);
    expect(allowsFolderNav()).toBe(false);
  });

  it("instant コマンドモード中は false", () => {
    setQuery("@goo");
    expect(allowsFolderNav()).toBe(false);
  });

  it("tool が folder の上に積まれている場合も false（tool 優先）", () => {
    setFolderState(FOLDER_FRAME);
    setToolSelectionState(TOOL_FRAME);
    expect(allowsFolderNav()).toBe(false);
  });
});

// ── supersede（latestRun primitive への集約・#534）────────────────────────────
// 検索語は query() から読むが、query effect + debounce のタイミングは他 describe の
// 同期 setQuery が残す状態に依存して不安定なため、エクスポート済み refreshResults() を
// 直接呼んで in-flight を作る（世代跨ぎの supersede は latestRun.test.ts が単体で担保済み）。

describe("supersede（world 世代跨ぎ・モード遷移）", () => {
  it("検索 in-flight 中の enterToolSelection が古い検索結果を無効化する", async () => {
    // 先行 describe が indexing=true を漏らすため false に戻す（indexing 中は検索がガードされる）
    vi.mocked(api.getIndexingState).mockResolvedValue(false);
    await initIndexingState();
    expect(indexing()).toBe(false);

    const resolvers: Array<(v: SearchResult[]) => void> = [];
    vi.mocked(api.search).mockImplementation(
      () => new Promise<SearchResult[]>((r) => resolvers.push(r)),
    );
    vi.mocked(api.getMatchingTools).mockResolvedValue([TOOL_1, TOOL_2]);

    setQuery("q");
    const p = refreshResults(); // 直接 refresh → run(gen=N) → api.search("q") deferred
    expect(resolvers.length).toBeGreaterThan(0);

    await enterToolSelection(FILE_RESULT); // world 世代を invalidate → results=tools
    expect(toolSelectionState()).not.toBeNull();
    expect(results()).toHaveLength(2);

    // 溜まった古い検索を全部解決 → いずれも stale で results を上書きしない
    resolvers.forEach((r) =>
      r([{ name: "stale", path: "C:\\s", isFolder: false, isError: false }]),
    );
    await p;
    await vi.runAllTimersAsync();

    expect(results()).toHaveLength(2);
    expect(results()[0].name).toBe("Tool One");
  });
});

// ── flush スコープ（instant を待受対象にしない・#534 Step 5c-1 の回帰網）─────────

describe("flush スコープ（instant fetch を activation の待受に載せない）", () => {
  it("instant IPC in-flight のまま非 instant へ遷移しても activation は instant IPC を待たない", async () => {
    // instant fetch を「解決しない」promise にする（待受に載ると activation がハングする）
    vi.mocked(api.getInstantCommands).mockImplementation(
      () => new Promise<InstantCommand[]>(() => {}),
    );

    setInstantCommandPrefix("@");
    dispatchQueryInput("@g");
    await vi.runAllTimersAsync(); // 30ms デバウンス発火 → instant IPC in-flight（never resolve）
    expect(interpKind()).toBe("instant");

    // 非 instant（command）へ遷移。`/x` は slash_noop で runRefresh を呼ばないため
    // single-slot の refreshInFlight を上書きしない。instant fetch が誤って flush 追跡に
    // 載っていれば、never-resolve な instant IPC が slot に残り activation がハングする。
    // （plain "x" は後続 refresh が slot を上書き→クリアし回帰を隠すため使わない・code-reviewer 指摘）
    dispatchQueryInput("/x");
    await vi.runAllTimersAsync();
    expect(interpKind()).toBe("command");

    // activation が instant IPC を待つと fake timer 下で解決せずハングする。
    // 解決すれば「flush が instant を待受対象にしていない」ことの証拠。
    const activated = await activateSelected();
    expect(activated).toBe(false); // 結果空で起動対象なし。だがハングせず false を返す
  });
});

// ── debounce adapter（#536 Phase 2・OwnedTimer 載せ替えの等価性を store 越しに直接固定）─
// 既存テストは runAllTimersAsync で一括 flush し leading/trailing を区別しない。ここでは 50ms
// 境界をまたいで「leading 即時発火」「burst で trailing 1 回・最後の query」を固定する（codex P1）。

describe("debounce adapter（leading/trailing 直接固定）", () => {
  // effect（microtask）は走らせるが 50ms trailing は進めない flush。
  const settleEffect = () => vi.advanceTimersByTimeAsync(0);

  beforeEach(async () => {
    // indexing リークを false に戻す（indexing 中は refreshResults が api.search を呼ばない）。
    vi.mocked(api.getIndexingState).mockResolvedValue(false);
    await initIndexingState();
    // 先行 describe の同期 setQuery が残す保留 timer を排出し、isPending() を false に揃える。
    await vi.runAllTimersAsync();
    vi.mocked(api.search).mockResolvedValue([]);
    vi.mocked(api.search).mockClear();
  });

  it("leading edge: 50ms 経過前に api.search が即発火し、50ms 後に trailing で再発火する", async () => {
    dispatchQueryInput("file");
    await settleEffect(); // dispatch + leading の runRefresh（<50ms なので trailing 未発火）
    expect(api.search).toHaveBeenCalledTimes(1); // leading のみ
    expect(api.search).toHaveBeenLastCalledWith("file");

    await vi.advanceTimersByTimeAsync(50); // trailing 発火
    expect(api.search).toHaveBeenCalledTimes(2);
    expect(api.search).toHaveBeenLastCalledWith("file");
  });

  it("burst（50ms 未満の連続入力）は leading 1 回 + trailing 1 回（最後の query）", async () => {
    dispatchQueryInput("f");
    await settleEffect(); // leading "f"、trailing 保留
    dispatchQueryInput("fi");
    await settleEffect(); // isPending() true → leading 発火せず re-arm
    dispatchQueryInput("fil");
    await settleEffect(); // re-arm（timer リセット）
    expect(api.search).toHaveBeenCalledTimes(1); // leading "f" のみ
    expect(api.search).toHaveBeenLastCalledWith("f");

    await vi.advanceTimersByTimeAsync(50); // trailing（最後の query）
    expect(api.search).toHaveBeenCalledTimes(2);
    expect(api.search).toHaveBeenLastCalledWith("fil");
  });
});

// ── instant debounce adapter（#536 Phase 3・items 即時クリア副作用 + 最新 filterName の直接固定）─
// instant は leading なし・trailing 30ms。arm に混ぜない「候補一覧の即時クリア」副作用（IPC 応答前の
// Enter で古いコマンド誤起動を防ぐ）と、burst で最後の filterName だけが 1 回 IPC を発行することを固定する。

describe("instant debounce adapter（items クリア副作用と最新 filterName）", () => {
  const settleEffect = () => vi.advanceTimersByTimeAsync(0);

  beforeEach(() => {
    setInstantCommandPrefix("@");
    vi.mocked(api.getInstantCommands).mockResolvedValue([CMD_GOOGLE, CMD_CLIP]);
  });

  it("再入力時、候補一覧は 30ms 前（IPC 応答前）に即クリアされる（arm と別関心事）", async () => {
    dispatchQueryInput("@goo");
    await vi.runAllTimersAsync(); // 30ms 発火 → 候補取得
    expect(getInstantCommandItems().length).toBeGreaterThan(0);

    // 別 filterName で再入力: scheduleInstantCommandFetch が同期で items=[] にし 30ms を arm。
    dispatchQueryInput("@cl");
    await settleEffect(); // dispatch 実行（<30ms・IPC 未発火）
    expect(getInstantCommandItems()).toEqual([]); // 古い候補が残らない（誤起動防止の要）
  });

  it("burst（30ms 未満の連続入力）は leading なしで、最後の filterName で 1 回だけ IPC 取得", async () => {
    // 呼び出し履歴はグローバル beforeEach の clearAllMocks() が各テスト前にクリア済み。
    dispatchQueryInput("@g");
    await settleEffect(); // arm "g"（leading なし＝即時 IPC は発行しない）
    dispatchQueryInput("@go");
    await settleEffect(); // re-arm "go"
    dispatchQueryInput("@goo");
    await settleEffect(); // re-arm "goo"
    expect(api.getInstantCommands).not.toHaveBeenCalled(); // 30ms 前は未発火（leading なし）

    await vi.advanceTimersByTimeAsync(30); // trailing 発火
    expect(api.getInstantCommands).toHaveBeenCalledTimes(1);
    expect(api.getInstantCommands).toHaveBeenCalledWith("goo"); // 最後の filterName のみ
  });
});

// ── executeInstantCommandSelected rollback の disturbed() 判定（#534 Step 5c-4, #539）────

describe("executeInstantCommandSelected rollback（world 世代が進んだら復元しない）", () => {
  it("await 中にモード遷移で world 世代が進むと、失敗しても候補を復元しない（disturbed）", async () => {
    vi.mocked(api.getInstantCommands).mockResolvedValue([CMD_GOOGLE, CMD_CLIP]);
    dispatchQueryInput("@google X");
    await vi.runAllTimersAsync();
    expect(interpKind()).toBe("instant");
    expect(results()).toHaveLength(2); // instant 候補が入っている

    // executeInstantCommand を deferred にして await 中に介入できるようにする
    let settleExec!: (v: Awaited<ReturnType<typeof api.executeInstantCommand>>) => void;
    vi.mocked(api.executeInstantCommand).mockImplementation(
      () => new Promise((resolve) => (settleExec = resolve)),
    );

    const p = activateSelected(); // withLaunchLifecycle が invalidate 直後に launchGen 捕捉 → executeInstantCommand 待ち
    await Promise.resolve();

    // await 中に world 世代をさらに進める（モード遷移）→ current() が launchGen を超える＝disturbed
    vi.mocked(api.getMatchingTools).mockResolvedValue([TOOL_1, TOOL_2]);
    await enterToolSelection(FILE_RESULT);
    expect(toolSelectionState()).not.toBeNull();

    // 失敗させる → onFailure: disturbed() が true なので候補リストを復元しない
    settleExec({ status: "failed", code: 1, message: "nope" });
    const ok = await p;

    expect(ok).toBe(false);
    // 復元されず、await 中に遷移したツール選択の状態が保たれる（instant 候補に巻き戻らない）
    expect(toolSelectionState()).not.toBeNull();
    expect(results()[0].name).toBe("Tool One");
  });
});

// ── 起動レーン mutex（activationLane・二重起動拒否・入れ子非ブロック・#535）──────

describe("起動レーン mutex（activationLane）", () => {
  it("起動 in-flight 中の 2 回目の activate は false で弾かれる（launch は 1 回）", async () => {
    // 先行 describe が indexing=true を漏らすため false に戻す
    vi.mocked(api.getIndexingState).mockResolvedValue(false);
    await initIndexingState();

    // ツール選択モードで検証する。launchWithSelectedTool は results() ではなく frame.tools
    // （toolSelectionState）を読むため、withLaunchLifecycle の clearResults() 後も 2 本目が起動
    // 対象を見つけられる＝mutex の有無を判別できる（通常モードは clearResults() で 2 本目が空 results
    // により無条件 false になり、mutex が壊れていても launchItem が増えず判別不能）。
    vi.mocked(api.getMatchingTools).mockResolvedValue([TOOL_1, TOOL_2]);
    await enterToolSelection(FILE_RESULT);
    expect(toolSelectionState()).not.toBeNull();
    setSelected(0);

    // launchWithTool を deferred にして 1 本目を in-flight のまま滞留させる
    let settleLaunch!: (v: Awaited<ReturnType<typeof api.launchWithTool>>) => void;
    vi.mocked(api.launchWithTool).mockImplementation(
      () => new Promise((resolve) => (settleLaunch = resolve)),
    );

    const p1 = activateSelected(); // tool 起動 in-flight（launchWithTool 待ちで滞留）
    await vi.runAllTimersAsync(); // 1 本目が launchWithTool の await に到達するまで進める
    expect(api.launchWithTool).toHaveBeenCalledOnce();
    expect(toolSelectionState()).not.toBeNull(); // onSuccess 未実行＝frame は生きている

    // 2 本目: activationLane が in-flight を検知して task を起動せず undefined → ?? false
    const r2 = await activateSelected();
    expect(r2).toBe(false);
    expect(api.launchWithTool).toHaveBeenCalledOnce(); // 2 本目は launch を呼ばない（mutex が阻止）

    // 1 本目を解放 → true（解放後は次の起動が可能）
    settleLaunch({ status: "ok", code: 0, message: null });
    expect(await p1).toBe(true);
  });

  it("入れ子経路（modal → tool 起動）が自己ブロックしない", async () => {
    vi.mocked(api.getIndexingState).mockResolvedValue(false);
    await initIndexingState();

    // clearAllMocks は実装を復元しないため、先行テストが仕込んだ deferred launchWithTool を
    // resolved へ戻す（never-resolve のまま待つとタイムアウトする）。
    vi.mocked(api.launchWithTool).mockResolvedValue({ status: "ok", code: 0, message: null });

    // ツール選択モードへ（tools=2）
    vi.mocked(api.getMatchingTools).mockResolvedValue([TOOL_1, TOOL_2]);
    await enterToolSelection(FILE_RESULT);
    expect(toolSelectionState()).not.toBeNull();
    expect(results()).toHaveLength(2);
    setSelected(0); // TOOL_1 を選択

    // activateSelected → tryModalActivate（lane の前）→ launchWithSelectedTool → activationLane。
    // 外側 activateSelected は modal 非 null で early return するため lane を二重取得せず、
    // 内側 launchWithSelectedTool が lane を取れる（自己ブロックしない）。
    const ok = await activateSelected();

    expect(ok).toBe(true);
    expect(api.launchWithTool).toHaveBeenCalledOnce();
    expect(api.launchWithTool).toHaveBeenCalledWith(
      FILE_RESULT.path,
      "",
      TOOL_1.exe,
      TOOL_1.args,
    );
  });
});

// ── perf requestId 相関（#534 Step 5c-5）──────────────────────────────────────

describe("perf requestId 相関", () => {
  it("perfStartSearch と perfMarkSearchDone は同一 requestId、getSearchGeneration() も一致", async () => {
    // 先行 describe が漏らす indexing=true を false に戻す（indexing 中は perfStartSearch("query") 前で return）
    vi.mocked(api.getIndexingState).mockResolvedValue(false);
    await initIndexingState();
    expect(indexing()).toBe(false);

    vi.mocked(api.search).mockResolvedValue([FILE_RESULT]);

    setQuery("file");
    await refreshResults(); // 直接 refresh（この位置では query effect が不安定なため）
    await vi.runAllTimersAsync(); // effect 由来の追随検索も流し切る

    const startQueryCalls = vi.mocked(perfStartSearch).mock.calls.filter((c) => c[1] === "query");
    const doneCalls = vi.mocked(perfMarkSearchDone).mock.calls;
    expect(startQueryCalls.length).toBeGreaterThan(0);
    expect(doneCalls.length).toBeGreaterThan(0);

    // 完了した最新検索の requestId は perfStartSearch にも現れ、getSearchGeneration()（= ResultsSection の
    // perfMarkRenderDone 源）とも一致する。世代更新位置がずれるとこの相関が崩れる。
    const lastDoneId = doneCalls.at(-1)![0];
    expect(startQueryCalls.some((c) => c[0] === lastDoneId)).toBe(true);
    expect(getSearchGeneration()).toBe(lastDoneId);
  });
});

// ── 経路分離（raw setQuery は検索を起動しない・#537）──────────────────────────
// suppressNextQueryEffectRefresh 撤廃の直接証明。query effect を廃し dispatchQueryInput を唯一の
// 検索起動起点にしたため、プログラム的リセット経路（resetForShow・instant 成功等）が使う raw setQuery は
// 検索を起動しない。ユーザー入力経路（dispatchQueryInput）だけが起動する、という経路分離を固定する。

describe("経路分離（raw setQuery は検索を起動しない・#537）", () => {
  beforeEach(async () => {
    // 先行 describe が漏らす indexing=true を false に戻す（indexing 中は refreshResults が search を呼ばない）。
    vi.mocked(api.getIndexingState).mockResolvedValue(false);
    await initIndexingState();
    await vi.runAllTimersAsync();
    vi.mocked(api.search).mockResolvedValue([]);
    vi.mocked(api.search).mockClear();
  });

  it("raw setQuery は dispatch を経由せず api.search を起動しない / dispatchQueryInput は起動する", async () => {
    // プログラム的リセット経路の代理: raw setQuery（旧 suppress で守っていた「effect を黙らせる」対象）。
    setQuery("hello");
    await vi.runAllTimersAsync();
    expect(api.search).not.toHaveBeenCalled();

    // 対照: ユーザー入力経路は検索を起動する（唯一の起動起点であることの確認）。
    dispatchQueryInput("hello");
    await vi.runAllTimersAsync();
    expect(api.search).toHaveBeenCalledWith("hello");
  });
});

// ── ViewStack push/pop（2 段スタック復元・#538）────────────────────────────────
// results → folder → tool の 2 段スタックを push/pop し、pop 時の復元順序・folderFilter 復帰
// （tool→folder）・restoreQuery 復帰（folder→results）を公開 API 経由で固定する。exitToolSelection/
// exitFolderExpansion が委譲する popView の統一規律を特性化する（挙動不変ゆえ常に緑であるべき）。

describe("ViewStack push/pop（2 段スタック復元・#538）", () => {
  beforeEach(() => {
    // folder モードの検索（listFolder）と tool 解決（getMatchingTools）を固定
    vi.mocked(api.listFolder).mockResolvedValue([FILE_RESULT]);
    vi.mocked(api.getMatchingTools).mockResolvedValue([TOOL_1, TOOL_2]);
  });

  it("results→folder→tool を push し、pop で folderFilter → restoreQuery の順に復元する", async () => {
    setQuery("orig");

    // push folder: restoreQuery に離脱時復元用の query を捕捉（enterFolderExpansion は query を変えない）
    enterFolderExpansion("C:\\dir");
    await vi.runAllTimersAsync();
    expect(viewKind()).toBe("folder");
    expect(folderState()!.restoreQuery).toBe("orig");

    // folder 内でフィルタを設定
    setFolderFilter("*.log");

    // push tool（folder の上に直交して積む）: savedFolderFilter に下段 folder の filter を、
    // launchQuery に起動用 query を捕捉する（folder 中 query は不変ゆえ "orig"）
    await enterToolSelection(FILE_RESULT);
    expect(viewKind()).toBe("tool");
    expect(toolSelectionState()!.savedFolderFilter).toBe("*.log");
    expect(toolSelectionState()!.launchQuery).toBe("orig");

    // pop tool → folder へ戻る: folderFilter 復帰・folderState 残存（下段は残る）
    expect(exitToolSelection()).toBe(true);
    expect(viewKind()).toBe("folder");
    expect(folderFilter()).toBe("*.log");
    expect(folderState()).not.toBeNull();

    // pop folder → results へ戻る: query 復帰（restoreQuery）・folderState 消滅
    expect(exitFolderExpansion()).toBe(true);
    expect(viewKind()).toBe("results");
    expect(query()).toBe("orig");
    expect(folderState()).toBeNull();
  });

  it("pop 順序は tool → folder（Escape 短絡 exitToolSelection() || exitFolderExpansion() で内側優先）", async () => {
    setQuery("orig");
    enterFolderExpansion("C:\\dir");
    await vi.runAllTimersAsync();
    setFolderFilter("*.log");
    await enterToolSelection(FILE_RESULT);

    // Escape 相当の短絡: tool が頂点にある間は exitToolSelection が消費し、folder は残る
    const handledFirst = exitToolSelection() || exitFolderExpansion();
    expect(handledFirst).toBe(true);
    expect(viewKind()).toBe("folder"); // tool だけ pop・folder 残存

    // 次の Escape: folder を pop して results へ
    const handledSecond = exitToolSelection() || exitFolderExpansion();
    expect(handledSecond).toBe(true);
    expect(viewKind()).toBe("results");
  });

  it("exit ガード: スタックが空なら両 exit は false（Escape はウィンドウ非表示へフォールスルー）", () => {
    // 頂点なし（results）: どちらの exit も自スロット不在で false を返す
    expect(exitToolSelection()).toBe(false);
    expect(exitFolderExpansion()).toBe(false);
  });
});
