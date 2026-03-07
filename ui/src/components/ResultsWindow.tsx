import { type Component, For, createSignal, onMount, onCleanup } from "solid-js";
import { emit, listen } from "@tauri-apps/api/event";
import type { SearchResult } from "../lib/types";
import type { ResultsDataPayload, ResultsSelectionPayload, ResultsVisibilityPayload } from "../lib/searchEvents";
import * as api from "../lib/invoke";
import { applyTheme } from "../lib/theme";
import { LruIconCache } from "../lib/lruIconCache";
import ResultRow from "./ResultRow";

const ResultsWindow: Component = () => {
  const [results, setResults] = createSignal<SearchResult[]>([]);
  const [selected, setSelected] = createSignal(0);
  const iconCache = new LruIconCache();
  const [iconCacheVersion, setIconCacheVersion] = createSignal(0);
  const [containerWidth, setContainerWidth] = createSignal(0);
  const [showIcons, setShowIcons] = createSignal(true);
  const [font, setFont] = createSignal("15px 'Segoe UI'");
  let listRef: HTMLDivElement | undefined;
  let latestGeneration = 0;
  let latestDataGeneration = 0;
  let lastScrolledSelected = -1;
  let lastScrolledGeneration = -1;
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

  async function fetchIcons(items: SearchResult[], generation: number) {
    if (!showIcons()) return;
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

  onMount(() => {
    // Load initial show_icons from bootstrap payload
    void api.getBootstrapPayload().then((bootstrap) => {
      setShowIcons(bootstrap.appearance.show_icons);
      applyTheme(bootstrap.visual);
    }).catch((e) => console.warn("ResultsWindow: failed to load bootstrap payload:", e));

    // Listen for show_icons setting changes
    let unlistenShowIcons: (() => void) | undefined;
    onCleanup(() => unlistenShowIcons?.());
    onCleanup(() => iconCache.revokeAll());
    void listen<boolean>("show-icons-changed", (event) => {
      setShowIcons(event.payload);
      if (!event.payload) {
        iconCache.revokeAll();
        setIconCacheVersion((v) => v + 1);
      }
    }).then((fn) => { unlistenShowIcons = fn; })
      .catch((e) => console.warn("ResultsWindow: failed to listen show-icons-changed:", e));

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
      .catch((e) => console.warn("ResultsWindow: failed to listen visual-config-changed:", e));

    if (listRef) {
      const ro = new ResizeObserver((entries) => {
        for (const entry of entries) {
          setContainerWidth(entry.contentRect.width);
        }
      });
      ro.observe(listRef);
      onCleanup(() => ro.disconnect());
    }

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

    function emitRenderDone(generation: number) {
      // rAF でフレームが確定した直後に emit する。
      // results-render-done は perfMarkRenderDone（計測専用）に使われるため、
      // 非表示ウィンドウで rAF がスロットリングされても UX には影響しない。
      requestAnimationFrame(() => {
        void emit("results-render-done", { requestId: generation })
          .catch((e) => console.warn("ResultsWindow: failed to emit results-render-done:", e));
      });
    }

    let unlistenVisibility: (() => void) | undefined;
    onCleanup(() => unlistenVisibility?.());
    void listen<ResultsVisibilityPayload>("results-visibility-changed", (event) => {
      if (event.payload.generation < latestGeneration) return;
      latestGeneration = event.payload.generation;
      iconCache.revokeAll();
      setIconCacheVersion((v) => v + 1);
    }).then((fn) => { unlistenVisibility = fn; })
      .catch((e) => console.warn("ResultsWindow: failed to listen results-visibility-changed:", e));

    let unlistenData: (() => void) | undefined;
    onCleanup(() => unlistenData?.());
    void listen<ResultsDataPayload>("results-data-changed", (event) => {
      if (event.payload.generation < latestGeneration) return;
      latestGeneration = event.payload.generation;
      latestDataGeneration = event.payload.generation;
      setResults(event.payload.results);
      const gen = event.payload.generation;
      requestAnimationFrame(() => {
        void fetchIcons(event.payload.results, gen);
      });
      setSelected(event.payload.selected);
      scrollToSelected(event.payload.selected, event.payload.generation);
      emitRenderDone(event.payload.searchRequestId);
    }).then((fn) => { unlistenData = fn; })
      .catch((e) => console.warn("ResultsWindow: failed to listen results-data-changed:", e));

    let unlistenSelection: (() => void) | undefined;
    onCleanup(() => unlistenSelection?.());
    void listen<ResultsSelectionPayload>("results-selection-changed", (event) => {
      if (event.payload.generation < latestGeneration) return;
      latestGeneration = event.payload.generation;
      setSelected(event.payload.selected);
      scrollToSelected(event.payload.selected, event.payload.generation);
    }).then((fn) => { unlistenSelection = fn; })
      .catch((e) => console.warn("ResultsWindow: failed to listen results-selection-changed:", e));
  });

  let hoverTimer: ReturnType<typeof setTimeout> | undefined;
  onCleanup(() => clearTimeout(hoverTimer));
  function handleHover(idx: number) {
    clearTimeout(hoverTimer);
    hoverTimer = setTimeout(() => {
      void api.notifyResultHovered(idx)
        .catch((e) => console.warn("ResultsWindow: failed to notify result hovered:", e));
    }, 50);
  }

  return (
    <div class="results-window">
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
              onClick={() => api.notifyResultClicked(idx())}
              onDoubleClick={() => api.notifyResultDoubleClicked(idx())}
              onMouseEnter={() => handleHover(idx())}
            />
          )}
        </For>
      </div>
    </div>
  );
};

export default ResultsWindow;
