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
  setSelected,
  setFolderFilter,
  folderFilter,
  activateSelected,
  activateSelectedByIndex,
  enterToolSelection,
  exitToolSelection,
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
import { setFolderState } from "../stores/folder";
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

// ── selection シグナル更新 ───────────────────────────────────────────────────

describe("selection シグナル更新", () => {
  it("moveSelectionDown は selected シグナルを更新する", async () => {
    // 結果を2件セットアップ
    const items: SearchResult[] = [
      { name: "a.txt", path: "C:\\a.txt", isFolder: false, isError: false },
      { name: "b.txt", path: "C:\\b.txt", isFolder: false, isError: false },
    ];
    vi.mocked(api.search).mockResolvedValue(items);
    setQuery("test");
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

    setQuery("@goo");
    await vi.runAllTimersAsync();

    expect(interpKind()).toBe("instant");
    expect(results()).toHaveLength(2);
    expect(results()[0].name).toBe("google");
  });

  it("コマンド名だけでフィルタリングされる（スペース後はクエリ部分）", async () => {
    vi.mocked(api.getInstantCommands).mockResolvedValue([CMD_GOOGLE]);

    setQuery("@google SolidJS tutorial");
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
    setQuery("@google SolidJS");
    await vi.runAllTimersAsync();
    // instant モード（interpKind = "instant"）, results に 2 件入っている状態
  });

  it("成功: クエリクリア + モード解除", async () => {
    vi.mocked(api.executeInstantCommand).mockResolvedValue({
      status: "ok",
      code: 0,
      message: null,
    });

    const ok = await activateSelected();

    expect(ok).toBe(true);
    expect(api.executeInstantCommand).toHaveBeenCalledWith("google", "SolidJS");
    expect(interpKind()).toBe("plain");
    expect(query()).toBe("");
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

    await refreshResults();

    expect(api.search).not.toHaveBeenCalled();
  });
});

// ── shouldShowResults ─────────────────────────────────────────────────────────

describe("shouldShowResults", () => {
  it("結果なし → false", () => {
    expect(shouldShowResults()).toBe(false);
  });

  it("結果あり + indexing=false → true", async () => {
    vi.mocked(api.search).mockResolvedValue([FILE_RESULT]);
    setQuery("file");
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
    setQuery("file");
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
    setQuery("@goo");
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
  currentDir: "C:\\test",
  savedResults: [] as SearchResult[],
  savedSelected: 0,
  savedQuery: "",
};

const TOOL_FRAME = {
  targetPath: "C:\\foo.txt",
  targetIsFolder: false,
  tools: [TOOL_1, TOOL_2],
  savedResults: [] as SearchResult[],
  savedSelected: 0,
  savedQuery: "",
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
    setQuery("@g");
    await vi.runAllTimersAsync(); // 30ms デバウンス発火 → instant IPC in-flight（never resolve）
    expect(interpKind()).toBe("instant");

    // 非 instant（command）へ遷移。`/x` は slash_noop で runRefresh を呼ばないため
    // single-slot の refreshInFlight を上書きしない。instant fetch が誤って flush 追跡に
    // 載っていれば、never-resolve な instant IPC が slot に残り activation がハングする。
    // （plain "x" は後続 refresh が slot を上書き→クリアし回帰を隠すため使わない・code-reviewer 指摘）
    setQuery("/x");
    await vi.runAllTimersAsync();
    expect(interpKind()).toBe("command");

    // activation が instant IPC を待つと fake timer 下で解決せずハングする。
    // 解決すれば「flush が instant を待受対象にしていない」ことの証拠。
    const activated = await activateSelected();
    expect(activated).toBe(false); // 結果空で起動対象なし。だがハングせず false を返す
  });
});

// ── executeInstantCommandSelected rollback の preGen+1 判定（#534 Step 5c-4）────

describe("executeInstantCommandSelected rollback（world 世代が進んだら復元しない）", () => {
  it("await 中にモード遷移で world 世代が進むと、失敗しても候補を復元しない（preGen+1 不成立）", async () => {
    vi.mocked(api.getInstantCommands).mockResolvedValue([CMD_GOOGLE, CMD_CLIP]);
    setQuery("@google X");
    await vi.runAllTimersAsync();
    expect(interpKind()).toBe("instant");
    expect(results()).toHaveLength(2); // instant 候補が入っている

    // executeInstantCommand を deferred にして await 中に介入できるようにする
    let settleExec!: (v: Awaited<ReturnType<typeof api.executeInstantCommand>>) => void;
    vi.mocked(api.executeInstantCommand).mockImplementation(
      () => new Promise((resolve) => (settleExec = resolve)),
    );

    const p = activateSelected(); // preGen 捕捉 → withLaunchLifecycle が +1 → executeInstantCommand 待ち
    await Promise.resolve();

    // await 中に world 世代をさらに進める（モード遷移）→ current() が preGen+1 を超える
    vi.mocked(api.getMatchingTools).mockResolvedValue([TOOL_1, TOOL_2]);
    await enterToolSelection(FILE_RESULT);
    expect(toolSelectionState()).not.toBeNull();

    // 失敗させる → onFailure: current() !== preGen+1 なので候補リストを復元しない
    settleExec({ status: "failed", code: 1, message: "nope" });
    const ok = await p;

    expect(ok).toBe(false);
    // 復元されず、await 中に遷移したツール選択の状態が保たれる（instant 候補に巻き戻らない）
    expect(toolSelectionState()).not.toBeNull();
    expect(results()[0].name).toBe("Tool One");
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
