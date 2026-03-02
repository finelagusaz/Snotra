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
  let listRef: HTMLDivElement | undefined;
  let latestGeneration = 0;
  let lastScrolledSelected = -1;
  let lastScrolledGeneration = -1;
  const objectUrls = new Set<string>();
  onCleanup(() => {
    for (const url of objectUrls) {
      URL.revokeObjectURL(url);
    }
  });

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

  async function fetchIcons(items: SearchResult[], generation: number) {
    if (!showIcons()) return;
    const cache = iconCache();
    const missing = items
      .filter((r) => !r.isError && !cache.has(r.path))
      .map((r) => r.path);
    if (missing.length === 0) return;

    const settled = await Promise.allSettled(
      missing.map(async (path) => {
        const buf = await api.getIconPng(path);
        return { path, buf };
      }),
    );
    if (generation !== latestGeneration) return;

    const next = new Map(cache);
    for (const r of settled) {
      if (r.status === "fulfilled") {
        const url = URL.createObjectURL(
          new Blob([r.value.buf], { type: "image/png" }),
        );
        objectUrls.add(url);
        next.set(r.value.path, url);
      }
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
    void listen<boolean>("show-icons-changed", (event) => {
      setShowIcons(event.payload);
      if (!event.payload) {
        for (const url of objectUrls) {
          URL.revokeObjectURL(url);
        }
        objectUrls.clear();
        setIconCache(new Map());
      }
    }).then((fn) => { unlistenShowIcons = fn; });

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
      latestGeneration = event.payload.generation;
      setResults(event.payload.results);
      setSelected(event.payload.selected);
      fetchIcons(event.payload.results, event.payload.generation);
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
