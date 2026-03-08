import { type Component, For, createSignal, createEffect, onMount, onCleanup } from "solid-js";
import { listen } from "@tauri-apps/api/event";
import type { SearchResult } from "../lib/types";
import { results, selected, getSearchGeneration } from "../stores/search";
import * as api from "../lib/invoke";
import { applyTheme } from "../lib/theme";
import { LruIconCache } from "../lib/lruIconCache";
import { perfMarkRenderDone } from "../lib/perf";
import ResultRow from "./ResultRow";

export interface ResultsSectionProps {
  visible: boolean;
  onClickResult: (index: number) => void;
  onDoubleClickResult: (index: number) => void;
  onHoverResult: (index: number) => void;
}

const ResultsSection: Component<ResultsSectionProps> = (props) => {
  const iconCache = new LruIconCache();
  const [iconCacheVersion, setIconCacheVersion] = createSignal(0);
  const [containerWidth, setContainerWidth] = createSignal(0);
  const [showIcons, setShowIcons] = createSignal(true);
  const [font, setFont] = createSignal("15px 'Segoe UI'");
  // ウィンドウの可視行数（max_results 設定値）。bootstrap と max-results-changed で更新する。
  let cachedMaxResults = 8;
  let listRef: HTMLDivElement | undefined;
  let latestDataGeneration = 0;
  let lastScrolledSelected = -1;
  let lastScrolledGeneration = -1;
  // results() の参照で世代を追跡（シグナルが変わるたびにインクリメント）
  let dataGeneration = 0;

  function ensureRowVisible(container: HTMLDivElement, row: HTMLElement) {
    const cRect = container.getBoundingClientRect();
    const rRect = row.getBoundingClientRect();

    if (rRect.top < cRect.top) {
      container.scrollTop -= cRect.top - rRect.top;
      return;
    }
    if (rRect.bottom > cRect.bottom) {
      container.scrollTop += rRect.bottom - cRect.bottom;
    }
  }

  /** Parse length-prefixed binary batch into per-path Blob URLs.
   *  Format: [count:u32 LE] then per icon: [status:u8] [if 1: png_len:u32 LE, png_bytes] */
  function parseBinaryBatch(
    buf: ArrayBuffer,
    paths: string[],
  ): Map<string, string> {
    const view = new DataView(buf);
    let offset = 0;
    const count = view.getUint32(offset, true);
    offset += 4;
    const result = new Map<string, string>();
    for (let i = 0; i < count; i++) {
      const status = view.getUint8(offset);
      offset += 1;
      if (status === 1) {
        const pngLen = view.getUint32(offset, true);
        offset += 4;
        const pngBytes = new Uint8Array(buf, offset, pngLen);
        offset += pngLen;
        const blob = new Blob([pngBytes], { type: "image/png" });
        const url = URL.createObjectURL(blob);
        result.set(paths[i], url);
      }
    }
    return result;
  }

  /** キャッシュにないアイコンを一括取得してキャッシュに格納する。stale なら Blob URL を破棄して早期リターン */
  async function fetchIconBatch(items: SearchResult[], generation: number): Promise<void> {
    const missing = items
      .filter((r) => !r.isError && !iconCache.has(r.path))
      .map((r) => r.path);
    if (missing.length === 0) return;

    let parsed: Map<string, string>;
    try {
      const buf = await api.getIconsBatch(missing);
      parsed = parseBinaryBatch(buf, missing);
    } catch (e) {
      console.warn("fetchIcons failed:", e);
      return;
    }
    if (generation !== latestDataGeneration) {
      for (const url of parsed.values()) {
        URL.revokeObjectURL(url);
      }
      return;
    }
    if (parsed.size === 0) return;

    for (const [path, url] of parsed) {
      iconCache.set(path, url);
    }
    setIconCacheVersion((v) => v + 1);
  }

  async function fetchIcons(items: SearchResult[], generation: number) {
    if (!showIcons()) return;
    // 呼び出し時点の値を固定し、await 中の設定変更でスライス境界がずれるのを防ぐ
    const visibleCount = cachedMaxResults;
    // Stage 1: 可視行（先頭 visibleCount 件）を優先取得 → 即時表示
    await fetchIconBatch(items.slice(0, visibleCount), generation);
    // Stage 2: スクロール域の残り分を補完（stage 1 完了後、stale なら自動スキップ）
    if (items.length > visibleCount) {
      await fetchIconBatch(items.slice(visibleCount), generation);
    }
  }

  // visible prop が false になったとき Blob URL を解放する
  createEffect(() => {
    if (!props.visible) {
      iconCache.revokeAll();
      setIconCacheVersion((v) => v + 1);
    }
  });

  // results シグナルの変更を監視してアイコン取得と perf 計測を行う
  createEffect(() => {
    const items = results();
    const sel = selected();
    const gen = ++dataGeneration;
    latestDataGeneration = gen;

    // アイコン取得
    requestAnimationFrame(() => {
      void fetchIcons(items, gen);
    });

    // スクロール追従
    scrollToSelected(sel, gen);

    // perf 計測: rAF 後に描画完了をマーク（searchGeneration を使用して perf エントリと一致させる）
    if (items.length > 0) {
      const perfRequestId = getSearchGeneration();
      requestAnimationFrame(() => {
        perfMarkRenderDone(perfRequestId);
      });
    }
  });

  // selected シグナルの変更を監視してスクロール追従
  createEffect(() => {
    const sel = selected();
    scrollToSelected(sel, dataGeneration);
  });

  onMount(() => {
    // Load initial values from bootstrap payload
    void api.getBootstrapPayload().then((bootstrap) => {
      setShowIcons(bootstrap.appearance.show_icons);
      cachedMaxResults = bootstrap.appearance.max_results;
      applyTheme(bootstrap.visual);
    }).catch((e) => console.warn("ResultsSection: failed to load bootstrap payload:", e));

    // Listen for show_icons setting changes
    let unlistenShowIcons: (() => void) | undefined;
    onCleanup(() => unlistenShowIcons?.());

    // Listen for max_results setting changes
    let unlistenMaxResults: (() => void) | undefined;
    onCleanup(() => unlistenMaxResults?.());
    void listen<number>("max-results-changed", (event) => {
      cachedMaxResults = event.payload;
    }).then((fn) => { unlistenMaxResults = fn; })
      .catch((e) => console.warn("ResultsSection: failed to listen max-results-changed:", e));
    onCleanup(() => iconCache.revokeAll());
    void listen<boolean>("show-icons-changed", (event) => {
      setShowIcons(event.payload);
      if (!event.payload) {
        iconCache.revokeAll();
        setIconCacheVersion((v) => v + 1);
      }
    }).then((fn) => { unlistenShowIcons = fn; })
      .catch((e) => console.warn("ResultsSection: failed to listen show-icons-changed:", e));

    // Measure font once at list level for all ResultRow instances
    if (listRef) {
      const style = getComputedStyle(listRef);
      setFont(`${style.fontSize} ${style.fontFamily}`);
    }

    // Listen for theme changes to update font
    let unlistenVisualFont: (() => void) | undefined;
    onCleanup(() => unlistenVisualFont?.());
    void listen("visual-config-changed", () => {
      if (listRef) {
        const style = getComputedStyle(listRef);
        setFont(`${style.fontSize} ${style.fontFamily}`);
      }
    }).then((fn) => { unlistenVisualFont = fn; })
      .catch((e) => console.warn("ResultsSection: failed to listen visual-config-changed:", e));

    if (listRef) {
      const ro = new ResizeObserver((entries) => {
        for (const entry of entries) {
          setContainerWidth(entry.contentRect.width);
        }
      });
      ro.observe(listRef);
      onCleanup(() => ro.disconnect());
    }
  });

  function scrollToSelected(selectedIdx: number, generation: number) {
    if (selectedIdx !== lastScrolledSelected || generation !== lastScrolledGeneration) {
      lastScrolledSelected = selectedIdx;
      lastScrolledGeneration = generation;
      queueMicrotask(() => {
        if (!listRef) return;
        const row = listRef.children[selectedIdx] as HTMLElement | undefined;
        if (!row) return;
        ensureRowVisible(listRef, row);
      });
    }
  }

  let hoverTimer: ReturnType<typeof setTimeout> | undefined;
  onCleanup(() => clearTimeout(hoverTimer));
  function handleHover(idx: number) {
    clearTimeout(hoverTimer);
    hoverTimer = setTimeout(() => {
      props.onHoverResult(idx);
    }, 50);
  }

  return (
    <div class="results-section">
      <div class="result-list-standalone" ref={listRef} role="listbox" aria-label="検索結果">
        <For each={results()}>
          {(result, idx) => (
            <ResultRow
              result={result}
              isSelected={idx() === selected()}
              icon={(iconCacheVersion(), iconCache.get(result.path))}
              showIcons={showIcons()}
              containerWidth={containerWidth()}
              font={font()}
              onClick={() => props.onClickResult(idx())}
              onDoubleClick={() => props.onDoubleClickResult(idx())}
              onMouseEnter={() => handleHover(idx())}
            />
          )}
        </For>
      </div>
    </div>
  );
};

export default ResultsSection;
