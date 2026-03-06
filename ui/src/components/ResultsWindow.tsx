import { type Component, For, createSignal, onMount, onCleanup } from "solid-js";
import { emit, listen } from "@tauri-apps/api/event";
import type { SearchResult } from "../lib/types";
import type { ResultsSyncPayload } from "../lib/searchEvents";
import * as api from "../lib/invoke";
import ResultRow from "./ResultRow";

const ResultsWindow: Component = () => {
  const [results, setResults] = createSignal<SearchResult[]>([]);
  const [selected, setSelected] = createSignal(0);
  const [iconCache, setIconCache] = createSignal<Map<string, string>>(
    new Map(),
  );
  const [containerWidth, setContainerWidth] = createSignal(0);
  const [showIcons, setShowIcons] = createSignal(true);
  const [font, setFont] = createSignal("15px 'Segoe UI'");
  let listRef: HTMLDivElement | undefined;
  let latestGeneration = 0;
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
        iconUrls.add(url);
      }
    }
    return result;
  }

  /** Revoke all tracked Blob URLs */
  function revokeAllIconUrls() {
    for (const url of iconUrls) {
      URL.revokeObjectURL(url);
    }
    iconUrls.clear();
  }

  const iconUrls = new Set<string>();

  async function fetchIcons(items: SearchResult[], generation: number) {
    if (!showIcons()) return;
    const cache = iconCache();
    const missing = items
      .filter((r) => !r.isError && !cache.has(r.path))
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
    if (generation !== latestGeneration) return;
    if (parsed.size === 0) return;

    const next = new Map(cache);
    for (const [path, url] of parsed) {
      next.set(path, url);
    }
    setIconCache(next);
  }

  onMount(() => {
    // Load initial show_icons from bootstrap payload
    void api.getBootstrapPayload().then((bootstrap) => {
      setShowIcons(bootstrap.appearance.show_icons);
    });

    // Listen for show_icons setting changes
    let unlistenShowIcons: (() => void) | undefined;
    onCleanup(() => unlistenShowIcons?.());
    onCleanup(() => revokeAllIconUrls());
    void listen<boolean>("show-icons-changed", (event) => {
      setShowIcons(event.payload);
      if (!event.payload) {
        revokeAllIconUrls();
        setIconCache(new Map());
      }
    }).then((fn) => { unlistenShowIcons = fn; });

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
    }).then((fn) => { unlistenVisualFont = fn; });

    if (listRef) {
      const ro = new ResizeObserver((entries) => {
        for (const entry of entries) {
          setContainerWidth(entry.contentRect.width);
        }
      });
      ro.observe(listRef);
      onCleanup(() => ro.disconnect());
    }

    let unlisten: (() => void) | undefined;
    onCleanup(() => unlisten?.());

    void listen<ResultsSyncPayload>("results-sync", (event) => {
      if (event.payload.generation < latestGeneration) {
        return;
      }
      const isSelectionOnly =
        event.payload.reason === "selection" &&
        event.payload.generation === latestGeneration;
      latestGeneration = event.payload.generation;
      if (!isSelectionOnly) {
        setResults(event.payload.results);
        void fetchIcons(event.payload.results, event.payload.generation);
      }
      setSelected(event.payload.selected);
      if (
        event.payload.selected !== lastScrolledSelected ||
        event.payload.generation !== lastScrolledGeneration
      ) {
        lastScrolledSelected = event.payload.selected;
        lastScrolledGeneration = event.payload.generation;
        queueMicrotask(() => {
          if (!listRef) return;
          const row = listRef.children[event.payload.selected] as HTMLElement | undefined;
          if (!row) return;
          ensureRowVisible(listRef, row);
        });
      }
      // rAF でフレームが確定した直後に emit する。
      // results-render-done は perfMarkRenderDone（計測専用）に使われるため、
      // 非表示ウィンドウで rAF がスロットリングされても UX には影響しない。
      requestAnimationFrame(() => {
        void emit("results-render-done", { requestId: event.payload.generation });
      });
    }).then((fn) => {
      unlisten = fn;
    });
  });

  let hoverTimer: ReturnType<typeof setTimeout> | undefined;
  onCleanup(() => clearTimeout(hoverTimer));
  function handleHover(idx: number) {
    clearTimeout(hoverTimer);
    hoverTimer = setTimeout(() => {
      void api.notifyResultHovered(idx);
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
              icon={iconCache().get(result.path)}
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
