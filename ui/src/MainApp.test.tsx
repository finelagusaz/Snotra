// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render } from "@solidjs/testing-library";
import type { ResultsSectionProps } from "./components/ResultsSection";

// ── ブラウザ API スタブ + モック関数宣言（vi.hoisted でモジュールロード前に実行） ──
const {
  mockActivateSelectedByIndex, mockSetSelected, mockResetForShow,
  mockInitIndexingState, mockSetHotkeyFailureNotice, mockShouldShowResults,
  mockInterpKind, mockSetInstantCommandPrefix, mockHideMainWindow,
  mockGetBootstrapPayload, mockSetSize,
  listenHandlers, mockOnFocusChanged, mockUnlistenFocus,
} = vi.hoisted(() => {
  globalThis.requestAnimationFrame = ((cb: FrameRequestCallback) => {
    cb(0);
    return 0;
  }) as typeof requestAnimationFrame;
  globalThis.cancelAnimationFrame = (() => {}) as typeof cancelAnimationFrame;

  return {
    mockActivateSelectedByIndex: vi.fn(async (_i: number) => true),
    mockSetSelected: vi.fn(),
    mockResetForShow: vi.fn(),
    mockInitIndexingState: vi.fn(async () => () => {}),
    mockSetHotkeyFailureNotice: vi.fn(),
    mockShouldShowResults: vi.fn(() => true),
    mockInterpKind: vi.fn((): "plain" | "command" | "instant" => "plain"),
    mockSetInstantCommandPrefix: vi.fn(),
    mockHideMainWindow: vi.fn(async () => {}),
    mockGetBootstrapPayload: vi.fn(),
    mockSetSize: vi.fn(async () => {}),
    // イベント名 → ハンドラ を捕捉し、テストから任意イベントを発火できるようにする
    listenHandlers: {} as Record<string, (e: { payload: unknown }) => void>,
    // onFocusChanged の呼び出し回数検証 + コールバック捕捉（focusHandler は calls から取得）
    mockOnFocusChanged: vi.fn(),
    mockUnlistenFocus: vi.fn(),
  };
});

// ── 子コンポーネントモック ──
// ResultsSection の props を捕捉し、MainApp が配線する onClickResult /
// onDoubleClickResult / visible を「コンポーネント境界の契約」としてテストする。
let capturedProps: ResultsSectionProps | undefined;
vi.mock("./components/ResultsSection", () => ({
  default: (props: ResultsSectionProps) => {
    capturedProps = props;
    return null;
  },
}));
vi.mock("./components/SearchWindow", () => ({ default: () => null }));
vi.mock("./components/UpdateToast", () => ({ default: () => null }));

// ── stores/search モック ──
vi.mock("./stores/search", () => ({
  resetForShow: mockResetForShow,
  setSelected: mockSetSelected,
  activateSelectedByIndex: mockActivateSelectedByIndex,
  initIndexingState: mockInitIndexingState,
  setHotkeyFailureNotice: mockSetHotkeyFailureNotice,
  shouldShowResults: mockShouldShowResults,
  interpKind: mockInterpKind,
  setInstantCommandPrefix: mockSetInstantCommandPrefix,
}));

// ── 外部依存モック ──
vi.mock("./lib/commands", () => ({ hideMainWindow: mockHideMainWindow }));
vi.mock("./lib/theme", () => ({ applyTheme: vi.fn() }));
vi.mock("./lib/i18n", () => ({
  t: (key: string) => key,
  setLanguage: vi.fn(),
}));
vi.mock("./lib/invoke", () => ({
  getBootstrapPayload: mockGetBootstrapPayload,
  notifyMainShown: vi.fn(async () => {}),
  saveSearchPlacement: vi.fn(async () => {}),
  quitApp: vi.fn(async () => {}),
}));
vi.mock("./lib/trace", () => ({ trace: vi.fn() }));

vi.mock("@tauri-apps/api/window", () => ({
  getCurrentWindow: () => ({
    isVisible: async () => true,
    scaleFactor: async () => 1,
    innerSize: async () => ({ toLogical: () => ({ width: 600, height: 52 }) }),
    onResized: async () => () => {},
    onMoved: async () => () => {},
    onFocusChanged: (cb: (e: { payload: boolean }) => void) => {
      mockOnFocusChanged(cb);
      return Promise.resolve(mockUnlistenFocus);
    },
    setSize: mockSetSize,
    show: vi.fn(async () => {}),
  }),
}));
vi.mock("@tauri-apps/api/dpi", () => ({
  LogicalSize: class {
    constructor(public width: number, public height: number) {}
  },
}));
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async (event: string, handler: (e: { payload: unknown }) => void) => {
    listenHandlers[event] = handler;
    return () => {};
  }),
}));

// ── モック確立後にインポート ──
import MainApp from "./MainApp";
import type { BootstrapPayload } from "./lib/types";

// auto_update: "disabled" で plugin-updater の動的 import を回避する
const BOOTSTRAP: BootstrapPayload = {
  visual: {
    preset: "dark",
    background_color: "#000",
    input_background_color: "#111",
    text_color: "#fff",
    selected_row_color: "#333",
    hint_text_color: "#888",
    font_family: "Segoe UI",
    font_size: 15,
  },
  general: { auto_hide_on_focus_lost: false, auto_update: "disabled" },
  appearance: { show_icons: true, visible_rows: 8 },
  language: "ja",
  indexing: false,
  instant_command_prefix: "@",
  result_limit: 200,
};

function tick(): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, 0));
}

async function renderMainApp() {
  const rendered = render(() => <MainApp />);
  // onMount の直列 await（listen 登録 → isVisible → bootstrap）を流し切る
  await tick();
  await tick();
  return rendered;
}

beforeEach(() => {
  vi.clearAllMocks();
  // vi.clearAllMocks() はプレーンオブジェクトの中身を消さないため明示クリア
  for (const key of Object.keys(listenHandlers)) delete listenHandlers[key];
  capturedProps = undefined;
  mockGetBootstrapPayload.mockResolvedValue(BOOTSTRAP);
  mockActivateSelectedByIndex.mockResolvedValue(true);
  mockShouldShowResults.mockReturnValue(true);
  mockInterpKind.mockReturnValue("plain");
});

afterEach(() => {
  // vitest globals が無効のため testing-library の自動 cleanup は登録されない（明示 unmount）
  cleanup();
});

describe("MainApp handleClickResult（launched 分岐）", () => {
  it("起動成功: activateSelectedByIndex(index) → hideMainWindow + visible が false になる", async () => {
    await renderMainApp();
    expect(capturedProps).toBeDefined();
    // 初期状態: isVisible=true + shouldShowResults=true → visible
    expect(capturedProps!.visible).toBe(true);

    capturedProps!.onClickResult(2);
    await tick();

    expect(mockActivateSelectedByIndex).toHaveBeenCalledWith(2);
    expect(mockHideMainWindow).toHaveBeenCalledTimes(1);
    // mainVisible=false へ即時リセット（Blob URL の hide 前解放経路）
    expect(capturedProps!.visible).toBe(false);
  });

  it("起動失敗: hideMainWindow は呼ばれず visible は維持、warn を出す", async () => {
    const warnSpy = vi.spyOn(console, "warn").mockImplementation(() => {});
    mockActivateSelectedByIndex.mockResolvedValue(false);
    await renderMainApp();

    capturedProps!.onClickResult(0);
    await tick();

    expect(mockActivateSelectedByIndex).toHaveBeenCalledWith(0);
    expect(mockHideMainWindow).not.toHaveBeenCalled();
    expect(capturedProps!.visible).toBe(true);
    expect(warnSpy).toHaveBeenCalledWith("Failed to launch clicked result", { index: 0 });
    warnSpy.mockRestore();
  });

  it("ダブルクリック: setSelected(index) のみ（起動しない）", async () => {
    await renderMainApp();

    capturedProps!.onDoubleClickResult(3);
    await tick();

    expect(mockSetSelected).toHaveBeenCalledWith(3);
    expect(mockActivateSelectedByIndex).not.toHaveBeenCalled();
    expect(mockHideMainWindow).not.toHaveBeenCalled();
  });
});

describe("MainApp → ResultsSection props 配線", () => {
  it("bootstrap の値が maxResults / iconCacheSize / showIcons に反映される", async () => {
    mockGetBootstrapPayload.mockResolvedValue({
      ...BOOTSTRAP,
      appearance: { show_icons: false, visible_rows: 5 },
      result_limit: 77,
    });
    await renderMainApp();

    expect(capturedProps!.maxResults).toBe(5);
    expect(capturedProps!.iconCacheSize).toBe(77);
    expect(capturedProps!.showIcons).toBe(false);
  });

  it("interpKind=instant のとき skipIcons=true（IC モード中の取得抑制）", async () => {
    mockInterpKind.mockReturnValue("instant");
    await renderMainApp();

    expect(capturedProps!.skipIcons).toBe(true);
  });
});

describe("MainApp auto_hide_on_focus_lost（#576: 常時リスニング + シグナルゲート）", () => {
  // onFocusChanged に渡された最新コールバックを取得する
  const focusHandler = () =>
    mockOnFocusChanged.mock.calls.at(-1)![0] as (e: { payload: boolean }) => void;
  // SolidJS の createEffect（setter で schedule される）を flush する microtask tick
  const flushEffects = () => Promise.resolve();

  it("bootstrap=false: フォーカス喪失イベントを受けても hide しない（ゲートで停止）", async () => {
    // BOOTSTRAP は auto_hide_on_focus_lost: false
    await renderMainApp();
    // 常時登録: onFocusChanged は起動時に1回だけ呼ばれる
    expect(mockOnFocusChanged).toHaveBeenCalledTimes(1);

    vi.useFakeTimers();
    try {
      focusHandler()({ payload: false });
      await vi.advanceTimersByTimeAsync(100);
      expect(mockHideMainWindow).not.toHaveBeenCalled();
    } finally {
      vi.useRealTimers();
    }
  });

  it("イベントで true に切り替えると、フォーカス喪失 100ms 後に hide する", async () => {
    await renderMainApp();

    vi.useFakeTimers();
    try {
      listenHandlers["auto-hide-focus-lost-changed"]({ payload: true });
      await flushEffects();
      focusHandler()({ payload: false });
      await vi.advanceTimersByTimeAsync(100);
      expect(mockHideMainWindow).toHaveBeenCalledTimes(1);
    } finally {
      vi.useRealTimers();
    }
  });

  it("blurTimer 保留中に false へ切り替えると、保留中の hide がキャンセルされる", async () => {
    await renderMainApp();

    vi.useFakeTimers();
    try {
      listenHandlers["auto-hide-focus-lost-changed"]({ payload: true });
      await flushEffects();
      focusHandler()({ payload: false }); // blurTimer arm（100ms）
      // 100ms 経過前に設定オフ → createEffect が blurTimer.cancel()
      listenHandlers["auto-hide-focus-lost-changed"]({ payload: false });
      await flushEffects();
      await vi.advanceTimersByTimeAsync(100);
      expect(mockHideMainWindow).not.toHaveBeenCalled();
    } finally {
      vi.useRealTimers();
    }
  });

  it("イベント発火を繰り返しても onFocusChanged は再登録されない（常時1本）", async () => {
    await renderMainApp();

    listenHandlers["auto-hide-focus-lost-changed"]({ payload: true });
    listenHandlers["auto-hide-focus-lost-changed"]({ payload: false });
    listenHandlers["auto-hide-focus-lost-changed"]({ payload: true });
    await flushEffects();

    expect(mockOnFocusChanged).toHaveBeenCalledTimes(1);
  });

  it("bootstrap=true（実既定）: 初回フォーカス喪失で 100ms 後に hide する", async () => {
    mockGetBootstrapPayload.mockResolvedValue({
      ...BOOTSTRAP,
      general: { ...BOOTSTRAP.general, auto_hide_on_focus_lost: true },
    });
    await renderMainApp();

    vi.useFakeTimers();
    try {
      focusHandler()({ payload: false });
      await vi.advanceTimersByTimeAsync(100);
      expect(mockHideMainWindow).toHaveBeenCalledTimes(1);
    } finally {
      vi.useRealTimers();
    }
  });
});
