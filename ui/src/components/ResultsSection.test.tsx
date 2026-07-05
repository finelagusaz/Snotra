// @vitest-environment jsdom
import { afterEach, beforeAll, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render } from "@solidjs/testing-library";
import { createSignal } from "solid-js";

// ── ブラウザ API スタブ + モック関数宣言（vi.hoisted でモジュールロード前に実行） ──
// requestAnimationFrame は同期実行スタブ: アイコン取得 effect の RAF スケジュールを
// テスト内で決定的に進めるため（実行は fetchIcons 内の await で必ず一時停止する）。
const { mockGetIconsBatch, mockParseBinaryBatch, createdUrls, revokedUrls } = vi.hoisted(() => {
  globalThis.requestAnimationFrame = ((cb: FrameRequestCallback) => {
    cb(0);
    return 0;
  }) as typeof requestAnimationFrame;
  globalThis.cancelAnimationFrame = (() => {}) as typeof cancelAnimationFrame;

  const createdUrls: string[] = [];
  const revokedUrls: string[] = [];
  return {
    mockGetIconsBatch: vi.fn<(paths: string[]) => Promise<ArrayBuffer>>(),
    // 実 parseBinaryBatch と同じ契約: 要求パスごとに URL.createObjectURL で Blob URL を生成
    mockParseBinaryBatch: vi.fn((_buf: ArrayBuffer, paths: string[]) => {
      const map = new Map<string, string>();
      for (const p of paths) {
        map.set(p, URL.createObjectURL(new Blob([])));
      }
      return map;
    }),
    createdUrls,
    revokedUrls,
  };
});

// ── stores/search モック ──
// ResultsSection は results / selected をリアクティブに購読するため、実 SolidJS シグナルで
// モックする（vi.fn の静的モックでは effect が再実行されない）。
vi.mock("../stores/search", async () => {
  const { createSignal } = await import("solid-js");
  const [results, setResults] = createSignal<import("../lib/types").SearchResult[]>([]);
  const [selected, setSelected] = createSignal(0);
  return {
    results,
    selected,
    getSearchGeneration: () => 0,
    __setResults: setResults,
    __setSelected: setSelected,
  };
});

// ── 外部依存モック ──
vi.mock("../lib/invoke", () => ({ getIconsBatch: mockGetIconsBatch }));
vi.mock("../lib/iconBatch", () => ({ parseBinaryBatch: mockParseBinaryBatch }));
vi.mock("../lib/perf", () => ({ perfMarkRenderDone: vi.fn() }));
vi.mock("../lib/truncatePath", () => ({
  truncatePath: (path: string) => path,
  clearTruncateCaches: vi.fn(),
}));
vi.mock("../lib/i18n", () => ({ t: (key: string) => key }));
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async () => () => {}),
}));

// ── モック確立後にインポート ──
import ResultsSection from "./ResultsSection";
import * as searchStore from "../stores/search";
import type { SearchResult } from "../lib/types";

const setResults = (searchStore as unknown as {
  __setResults: (items: SearchResult[]) => void;
}).__setResults;
const setSelected = (searchStore as unknown as {
  __setSelected: (i: number) => void;
}).__setSelected;

// ── ヘルパー ──────────────────────────────────────────────────────────────────

function makeResult(path: string): SearchResult {
  return { name: path.split("\\").pop() ?? path, path, isFolder: false, isError: false };
}

const RESULTS_A = [makeResult("C:\\a1.exe"), makeResult("C:\\a2.exe")];
const RESULTS_B = [makeResult("C:\\b1.exe"), makeResult("C:\\b2.exe")];

/** マイクロタスク + macrotask を1周させて await 済みの継続を流す */
function tick(): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, 0));
}

/** getIconsBatch を手動 resolve 制御にする。呼び出しごとの resolver を返す */
function deferIconsBatch(): Array<(buf: ArrayBuffer) => void> {
  const resolvers: Array<(buf: ArrayBuffer) => void> = [];
  mockGetIconsBatch.mockImplementation(
    () => new Promise<ArrayBuffer>((resolve) => resolvers.push(resolve)),
  );
  return resolvers;
}

interface SetupOptions {
  visible?: boolean;
  showIcons?: boolean;
  skipIcons?: boolean;
  iconCacheSize?: number;
}

function setup(opts: SetupOptions = {}) {
  const [visible, setVisible] = createSignal(opts.visible ?? true);
  const [showIcons, setShowIcons] = createSignal(opts.showIcons ?? true);
  const [skipIcons, setSkipIcons] = createSignal(opts.skipIcons ?? false);
  const [iconCacheSize, setIconCacheSize] = createSignal(opts.iconCacheSize ?? 200);
  const rendered = render(() => (
    <ResultsSection
      visible={visible()}
      showIcons={showIcons()}
      skipIcons={skipIcons()}
      maxResults={8}
      iconCacheSize={iconCacheSize()}
      onClickResult={() => {}}
      onDoubleClickResult={() => {}}
    />
  ));
  return { ...rendered, setVisible, setShowIcons, setSkipIcons, setIconCacheSize };
}

function imgSrcs(container: HTMLElement): string[] {
  return Array.from(container.querySelectorAll("img")).map((img) => img.src);
}

// ── セットアップ ──────────────────────────────────────────────────────────────

beforeAll(() => {
  // jsdom 未実装 API のスタブ
  Element.prototype.scrollIntoView = vi.fn();
  globalThis.ResizeObserver = class {
    observe() {}
    unobserve() {}
    disconnect() {}
  } as unknown as typeof ResizeObserver;

  let urlCounter = 0;
  URL.createObjectURL = vi.fn(() => {
    const url = `blob:mock-${++urlCounter}`;
    createdUrls.push(url);
    return url;
  });
  URL.revokeObjectURL = vi.fn((url: string) => {
    revokedUrls.push(url);
  });
});

beforeEach(() => {
  vi.clearAllMocks();
  createdUrls.length = 0;
  revokedUrls.length = 0;
  setResults([]);
  setSelected(0);
  mockGetIconsBatch.mockResolvedValue(new ArrayBuffer(0));
});

afterEach(() => {
  // vitest globals が無効のため testing-library の自動 cleanup は登録されない。
  // 明示的に unmount しないと、前のテストのコンポーネントがシグナル購読を持ったまま残り、
  // 後続テストの setResults に反応して getIconsBatch の呼び出し回数を汚染する。
  cleanup();
});

// ── テスト ────────────────────────────────────────────────────────────────────

describe("ResultsSection アイコンライフサイクル", () => {
  it("results 到着でキャッシュ未取得パスのみ getIconsBatch が呼ばれ Blob URL が行に描画される", async () => {
    setResults(RESULTS_A);
    const { container } = setup();
    await tick();

    expect(mockGetIconsBatch).toHaveBeenCalledTimes(1);
    expect(mockGetIconsBatch).toHaveBeenCalledWith(["C:\\a1.exe", "C:\\a2.exe"]);
    expect(createdUrls).toHaveLength(2);
    expect(imgSrcs(container)).toEqual(createdUrls);

    // 同一 results の再取得は起きない（キャッシュ済みパスは missing から除外）
    setSelected(1);
    await tick();
    expect(mockGetIconsBatch).toHaveBeenCalledTimes(1);
  });

  it("visible=false 遷移で revokeAll される（生成 URL がすべて revoke され img が消える）", async () => {
    setResults(RESULTS_A);
    const { container, setVisible } = setup();
    await tick();
    expect(createdUrls).toHaveLength(2);
    expect(revokedUrls).toHaveLength(0);

    setVisible(false);
    await tick();

    // 生成した全 Blob URL が回収される（リークなし不変条件）
    expect([...revokedUrls].sort()).toEqual([...createdUrls].sort());
    // 再表示時は再取得（キャッシュは破棄済み）
    setVisible(true);
    await tick();
    expect(mockGetIconsBatch).toHaveBeenCalledTimes(2);
    expect(imgSrcs(container)).toEqual(createdUrls.slice(2));
  });

  it("iconCacheSize 変更で revokeAll される（evict 済み URL の <img> 残留防止）", async () => {
    setResults(RESULTS_A);
    const { setIconCacheSize } = setup({ iconCacheSize: 200 });
    await tick();
    expect(createdUrls).toHaveLength(2);

    setIconCacheSize(100);
    await tick();

    expect([...revokedUrls].sort()).toEqual([...createdUrls].sort());
  });

  it("stale ガード: 古い世代の応答は Blob URL を revoke して破棄し、キャッシュへ入れない", async () => {
    const resolvers = deferIconsBatch();
    setResults(RESULTS_A);
    const { container } = setup();
    expect(mockGetIconsBatch).toHaveBeenCalledTimes(1); // 世代1（A）が in-flight

    // 応答前に results が変わり世代が進む
    setResults(RESULTS_B);
    expect(mockGetIconsBatch).toHaveBeenCalledTimes(2); // 世代2（B）も in-flight

    // 古い世代1の応答が遅れて到着 → parse で生成された URL は即 revoke される
    resolvers[0](new ArrayBuffer(0));
    await tick();
    const batch1Urls = createdUrls.slice(0, 2);
    expect([...revokedUrls].sort()).toEqual([...batch1Urls].sort());
    expect(imgSrcs(container)).toEqual([]);

    // 現行世代2の応答は採用される
    resolvers[1](new ArrayBuffer(0));
    await tick();
    const batch2Urls = createdUrls.slice(2, 4);
    expect(imgSrcs(container)).toEqual(batch2Urls);
    // 世代2の URL は revoke されていない
    expect(revokedUrls).toHaveLength(2);
  });

  it("skipIcons=true 中は取得を抑制し、既存キャッシュは破棄しない", async () => {
    setResults(RESULTS_A);
    const { container, setSkipIcons } = setup();
    await tick();
    expect(mockGetIconsBatch).toHaveBeenCalledTimes(1);
    const cachedUrls = [...createdUrls];

    // skipIcons ON: 取得も破棄も起きない（IC モード中のキャッシュ保持）
    setSkipIcons(true);
    setResults(RESULTS_B);
    await tick();
    expect(mockGetIconsBatch).toHaveBeenCalledTimes(1);
    expect(revokedUrls).toHaveLength(0);

    // skipIcons OFF 復帰: 現在の results で取得再開、既存キャッシュは有効なまま
    setSkipIcons(false);
    await tick();
    expect(mockGetIconsBatch).toHaveBeenCalledTimes(2);
    expect(mockGetIconsBatch).toHaveBeenLastCalledWith(["C:\\b1.exe", "C:\\b2.exe"]);
    expect(cachedUrls.every((u) => !revokedUrls.includes(u))).toBe(true);
    void container;
  });

  it("unmount で revokeAll される（onCleanup の回収経路）", async () => {
    setResults(RESULTS_A);
    const { unmount } = setup();
    await tick();
    expect(createdUrls).toHaveLength(2);

    unmount();

    expect([...revokedUrls].sort()).toEqual([...createdUrls].sort());
  });

  it("取得できなかったパスは 2 回目以降の missing から除外される（fetchedNone）", async () => {
    // parse 結果が空 = 全パス「アイコンなし」（Once: 他テストのデフォルト実装を汚さない）
    mockParseBinaryBatch.mockImplementationOnce(() => new Map<string, string>());
    setResults(RESULTS_A);
    const { setSkipIcons } = setup();
    await tick();
    expect(mockGetIconsBatch).toHaveBeenCalledTimes(1);

    // results 同一のまま skipIcons を往復 → fetchedNone 保持により再取得は 0 件で発火しない
    setSkipIcons(true);
    setSkipIcons(false);
    await tick();
    expect(mockGetIconsBatch).toHaveBeenCalledTimes(1);
  });
});
