// @vitest-environment jsdom
import { describe, expect, it, vi, beforeEach } from "vitest";
import { render, fireEvent } from "@solidjs/testing-library";

// ── ブラウザ API スタブ + モック関数宣言（vi.hoisted でモジュールロード前に実行） ──
// vi.mock ファクトリはファイル先頭にホイストされるため、参照する変数も vi.hoisted で宣言する。
const {
  mockQuery, mockResults, mockSelected, mockFolderState, mockFolderFilter,
  mockToolSelectionState, mockIndexing, mockLaunching, mockLaunchNotice,
  mockNoResults, mockInstantCommandMode, mockExitToolSelection, mockExitFolderExpansion,
  mockEnterToolSelection, mockActivateSelected, mockEnterFolderExpansion,
  mockNavigateFolderUp, mockMoveSelectionUp, mockMoveSelectionDown,
  mockSetQuery, mockSetFolderFilter, mockClearLaunchNotice,
  mockHideMainWindow, mockViewKind, mockInterpKind,
} = vi.hoisted(() => {
  globalThis.requestAnimationFrame = ((cb: Function) =>
    setTimeout(cb, 0)) as typeof requestAnimationFrame;
  globalThis.cancelAnimationFrame = ((id: number) =>
    clearTimeout(id)) as typeof cancelAnimationFrame;
  globalThis.performance ??= { now: () => 0 } as Performance;

  return {
    mockQuery: vi.fn(() => ""),
    mockResults: vi.fn((): any[] => []),
    mockSelected: vi.fn(() => 0),
    mockFolderState: vi.fn((): any => null),
    mockFolderFilter: vi.fn(() => ""),
    mockToolSelectionState: vi.fn((): any => null),
    mockIndexing: vi.fn(() => false),
    mockLaunching: vi.fn(() => false),
    mockLaunchNotice: vi.fn((): string | null => null),
    mockNoResults: vi.fn(() => false),
    mockInstantCommandMode: vi.fn(() => false),
    mockExitToolSelection: vi.fn(() => false),
    mockExitFolderExpansion: vi.fn(() => false),
    mockEnterToolSelection: vi.fn(async () => false),
    mockActivateSelected: vi.fn(async () => false),
    mockEnterFolderExpansion: vi.fn(),
    mockNavigateFolderUp: vi.fn(),
    mockMoveSelectionUp: vi.fn(),
    mockMoveSelectionDown: vi.fn(),
    mockSetQuery: vi.fn(),
    mockSetFolderFilter: vi.fn(),
    mockClearLaunchNotice: vi.fn(),
    mockHideMainWindow: vi.fn(),
    // 軸メモは下位シグナルモックから導出する（実装と同じ射影）。
    // 既存テストは mockToolSelectionState 等を set するだけで両軸が追従する。
    mockViewKind: vi.fn((): "results" | "folder" | "tool" =>
      mockToolSelectionState() ? "tool" : mockFolderState() ? "folder" : "results"),
    mockInterpKind: vi.fn((): "plain" | "command" | "instant" => {
      if (mockToolSelectionState() || mockFolderState()) return "plain";
      if (mockInstantCommandMode()) return "instant";
      if (mockQuery().trimStart().startsWith("/")) return "command";
      return "plain";
    }),
  };
});

// ── stores/search モック ──
// SearchWindow はストア関数の「呼び分け」を担当するため、全関数をモック化して
// 「どのキー × どの状態 → どの関数が呼ばれるか」をテストする。
vi.mock("../stores/search", () => ({
  query: mockQuery,
  setQuery: mockSetQuery,
  results: mockResults,
  selected: mockSelected,
  folderState: mockFolderState,
  folderFilter: mockFolderFilter,
  setFolderFilter: mockSetFolderFilter,
  moveSelectionUp: mockMoveSelectionUp,
  moveSelectionDown: mockMoveSelectionDown,
  exitFolderExpansion: mockExitFolderExpansion,
  navigateFolderUp: mockNavigateFolderUp,
  enterFolderExpansion: mockEnterFolderExpansion,
  activateSelected: mockActivateSelected,
  indexing: mockIndexing,
  launching: mockLaunching,
  launchNotice: mockLaunchNotice,
  clearLaunchNotice: mockClearLaunchNotice,
  enterToolSelection: mockEnterToolSelection,
  exitToolSelection: mockExitToolSelection,
  toolSelectionState: mockToolSelectionState,
  noResults: mockNoResults,
  instantCommandMode: mockInstantCommandMode,
  viewKind: mockViewKind,
  interpKind: mockInterpKind,
}));

// ── 外部依存モック ──
vi.mock("../lib/commands", () => ({
  hideMainWindow: mockHideMainWindow,
}));

vi.mock("../lib/folderNav", () => ({
  computeParentDir: vi.fn(() => null),
}));
vi.mock("../lib/perf", () => ({ perfMarkInput: vi.fn() }));
vi.mock("../lib/trace", () => ({ trace: vi.fn() }));
vi.mock("../lib/i18n", () => ({
  t: vi.fn((key: string) => key),
}));

vi.mock("@tauri-apps/api/window", () => ({
  getCurrentWindow: () => ({
    isVisible: async () => false,
    onFocusChanged: async () => () => {},
  }),
}));
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async () => () => {}),
}));

// ── モック確立後にインポート ──
import SearchWindow from "./SearchWindow";
import { computeParentDir } from "../lib/folderNav";

// ── ヘルパー ──
function renderSearchWindow() {
  return render(() => <SearchWindow />);
}

function getInput(container: HTMLElement): HTMLInputElement {
  return container.querySelector(".search-input")! as HTMLInputElement;
}

function pressKey(el: HTMLElement, key: string, opts: Partial<KeyboardEvent> = {}) {
  fireEvent.keyDown(el, { key, ...opts });
}

// ── テスト ──
beforeEach(() => {
  vi.clearAllMocks();
  // デフォルト: 通常モード（ツール選択なし、フォルダ展開なし、インスタントコマンドなし）
  mockQuery.mockReturnValue("");
  mockResults.mockReturnValue([]);
  mockSelected.mockReturnValue(0);
  mockFolderState.mockReturnValue(null);
  mockFolderFilter.mockReturnValue("");
  mockToolSelectionState.mockReturnValue(null);
  mockIndexing.mockReturnValue(false);
  mockLaunching.mockReturnValue(false);
  mockLaunchNotice.mockReturnValue(null);
  mockNoResults.mockReturnValue(false);
  mockInstantCommandMode.mockReturnValue(false);
  mockExitToolSelection.mockReturnValue(false);
  mockExitFolderExpansion.mockReturnValue(false);
  mockEnterToolSelection.mockResolvedValue(false);
  mockActivateSelected.mockResolvedValue(false);
});

describe("SearchWindow キーボードハンドリング", () => {
  // ── ArrowRight/Left のツール選択中ブロック ──

  describe("ツール選択中の ArrowRight/Left 無効化", () => {
    it("ツール選択中に ArrowRight でフォルダ展開が呼ばれない", () => {
      mockToolSelectionState.mockReturnValue({ targetPath: "C:\\file.txt" });
      mockResults.mockReturnValue([{ name: "folder", path: "C:\\folder", isFolder: true, isError: false }]);
      mockSelected.mockReturnValue(0);

      const { container } = renderSearchWindow();
      pressKey(getInput(container), "ArrowRight");

      expect(mockEnterFolderExpansion).not.toHaveBeenCalled();
    });

    it("ツール選択中に ArrowLeft でフォルダ離脱が呼ばれない", () => {
      mockToolSelectionState.mockReturnValue({ targetPath: "C:\\file.txt" });
      mockFolderState.mockReturnValue({ currentDir: "C:\\folder" });

      const { container } = renderSearchWindow();
      pressKey(getInput(container), "ArrowLeft");

      expect(mockNavigateFolderUp).not.toHaveBeenCalled();
    });

    it("インスタントコマンドモード中に ArrowRight でフォルダ展開が呼ばれない", () => {
      mockInstantCommandMode.mockReturnValue(true);
      mockResults.mockReturnValue([{ name: "folder", path: "C:\\folder", isFolder: true, isError: false }]);
      mockSelected.mockReturnValue(0);

      const { container } = renderSearchWindow();
      pressKey(getInput(container), "ArrowRight");

      expect(mockEnterFolderExpansion).not.toHaveBeenCalled();
    });

    it("通常モードで ArrowRight → フォルダ結果ならフォルダ展開が呼ばれる", () => {
      mockResults.mockReturnValue([{ name: "folder", path: "C:\\folder", isFolder: true, isError: false }]);
      mockSelected.mockReturnValue(0);

      const { container } = renderSearchWindow();
      pressKey(getInput(container), "ArrowRight");

      expect(mockEnterFolderExpansion).toHaveBeenCalledWith("C:\\folder");
    });

    it("通常モードで ArrowLeft → フォルダモード中なら navigateFolderUp が呼ばれる", () => {
      mockFolderState.mockReturnValue({ currentDir: "C:\\folder" });

      const { container } = renderSearchWindow();
      pressKey(getInput(container), "ArrowLeft");

      expect(mockNavigateFolderUp).toHaveBeenCalledTimes(1);
    });

    it("通常モードで ArrowLeft → フォルダモード外なら computeParentDir の結果で展開", () => {
      mockResults.mockReturnValue([{ name: "file.txt", path: "C:\\folder\\file.txt", isFolder: false, isError: false }]);
      mockSelected.mockReturnValue(0);
      vi.mocked(computeParentDir).mockReturnValue("C:\\folder");

      const { container } = renderSearchWindow();
      pressKey(getInput(container), "ArrowLeft");

      expect(mockEnterFolderExpansion).toHaveBeenCalledWith("C:\\folder");
    });
  });

  // ── Escape の優先順位 ──

  describe("Escape の優先順位", () => {
    it("ツール選択中 → exitToolSelection が呼ばれウィンドウは閉じない", () => {
      mockExitToolSelection.mockReturnValue(true);

      const { container } = renderSearchWindow();
      pressKey(getInput(container), "Escape");

      expect(mockExitToolSelection).toHaveBeenCalledTimes(1);
      expect(mockHideMainWindow).not.toHaveBeenCalled();
    });

    it("フォルダ展開中 → exitFolderExpansion が呼ばれウィンドウは閉じない", () => {
      mockExitToolSelection.mockReturnValue(false);
      mockExitFolderExpansion.mockReturnValue(true);

      const { container } = renderSearchWindow();
      pressKey(getInput(container), "Escape");

      expect(mockExitFolderExpansion).toHaveBeenCalledTimes(1);
      expect(mockHideMainWindow).not.toHaveBeenCalled();
    });

    it("通常モード → hideMainWindow が呼ばれる", () => {
      mockExitToolSelection.mockReturnValue(false);
      mockExitFolderExpansion.mockReturnValue(false);

      const { container } = renderSearchWindow();
      pressKey(getInput(container), "Escape");

      expect(mockHideMainWindow).toHaveBeenCalledTimes(1);
    });
  });

  // ── ツール選択中の入力無効化 ──

  describe("ツール選択中の入力無効化", () => {
    it("ツール選択中に文字入力しても setQuery が呼ばれない", () => {
      mockToolSelectionState.mockReturnValue({ targetPath: "C:\\file.txt" });

      const { container } = renderSearchWindow();
      const input = getInput(container);
      fireEvent.input(input, { target: { value: "abc" } });

      expect(mockSetQuery).not.toHaveBeenCalled();
      expect(mockSetFolderFilter).not.toHaveBeenCalled();
    });

    it("通常モードで文字入力すると setQuery が呼ばれる", () => {
      const { container } = renderSearchWindow();
      const input = getInput(container);
      fireEvent.input(input, { target: { value: "test" } });

      expect(mockSetQuery).toHaveBeenCalledWith("test");
    });

    it("インスタントコマンドモード中も文字入力を受理する（綻び2: 入力可否は軸1のみ依存）", () => {
      mockInstantCommandMode.mockReturnValue(true);

      const { container } = renderSearchWindow();
      const input = getInput(container);
      fireEvent.input(input, { target: { value: "@goo" } });

      expect(mockSetQuery).toHaveBeenCalledWith("@goo");
    });
  });

  // ── Shift+Enter でツール選択 ──

  describe("Shift+Enter でツール選択", () => {
    it("Shift+Enter → 結果があれば enterToolSelection が呼ばれる", () => {
      const result = { name: "file.txt", path: "C:\\file.txt", isFolder: false, isError: false };
      mockResults.mockReturnValue([result]);
      mockSelected.mockReturnValue(0);

      const { container } = renderSearchWindow();
      pressKey(getInput(container), "Enter", { shiftKey: true });

      expect(mockEnterToolSelection).toHaveBeenCalledWith(result);
    });

    it("ツール選択中の Shift+Enter → 通常 Enter として activateSelected が呼ばれる", () => {
      mockToolSelectionState.mockReturnValue({ targetPath: "C:\\file.txt" });

      const { container } = renderSearchWindow();
      pressKey(getInput(container), "Enter", { shiftKey: true });

      expect(mockEnterToolSelection).not.toHaveBeenCalled();
      expect(mockActivateSelected).toHaveBeenCalledTimes(1);
    });

    it("インスタントコマンドモード中の Shift+Enter → 通常 Enter として処理", () => {
      mockInstantCommandMode.mockReturnValue(true);
      mockResults.mockReturnValue([{ name: "cmd", path: "@cmd", isFolder: false, isError: false }]);
      mockSelected.mockReturnValue(0);

      const { container } = renderSearchWindow();
      pressKey(getInput(container), "Enter", { shiftKey: true });

      expect(mockEnterToolSelection).not.toHaveBeenCalled();
      expect(mockActivateSelected).toHaveBeenCalledTimes(1);
    });

    it("Enter（Shift なし）→ activateSelected が呼ばれる", () => {
      const { container } = renderSearchWindow();
      pressKey(getInput(container), "Enter");

      expect(mockActivateSelected).toHaveBeenCalledTimes(1);
      expect(mockEnterToolSelection).not.toHaveBeenCalled();
    });
  });
});

describe("SearchWindow 表示", () => {
  it("ツール選択中は placeholder が tool_select キーになる", () => {
    mockToolSelectionState.mockReturnValue({ targetPath: "C:\\folder\\file.txt" });

    const { container } = renderSearchWindow();
    const input = getInput(container);

    expect(input.placeholder).toBe("search.placeholder.tool_select");
  });

  it("インデックス構築中はオーバーレイが表示される", () => {
    mockIndexing.mockReturnValue(true);

    const { container } = renderSearchWindow();

    expect(container.querySelector(".indexing-message")).not.toBeNull();
  });
});
