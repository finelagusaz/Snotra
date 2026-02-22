import { type Component, For, createSignal, onMount, onCleanup } from "solid-js";
import { emit, listen } from "@tauri-apps/api/event";
import type { SearchResult } from "../lib/types";
import * as api from "../lib/invoke";
import ResultRow from "./ResultRow";

type ResultsUpdatedPayload = {
  results: SearchResult[];
  selected: number;
  requestId: number;
};

const ResultsWindow: Component = () => {
  const [results, setResults] = createSignal<SearchResult[]>([]);
  const [selected, setSelected] = createSignal(0);
  const [iconCache, setIconCache] = createSignal<Map<string, string>>(
    new Map(),
  );
  const [containerWidth, setContainerWidth] = createSignal(0);
  let listRef: HTMLDivElement | undefined;
  let hoverTimer: ReturnType<typeof setTimeout> | undefined;
  let latestRequestId = 0;
  let lastScrolledSelected = -1;
  let lastScrolledRequestId = -1;

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

  async function fetchIcons(items: SearchResult[], requestId: number) {
    const cache = iconCache();
    const missing = items
      .filter((r) => !r.isError && !cache.has(r.path))
      .map((r) => r.path);
    if (missing.length === 0) return;

    const batch = await api.getIconsBatch(missing);
    if (requestId !== latestRequestId) return;

    const next = new Map(cache);
    for (const [k, v] of Object.entries(batch)) {
      next.set(k, v);
    }
    setIconCache(next);
  }

  function debouncedHover(index: number) {
    clearTimeout(hoverTimer);
    hoverTimer = setTimeout(() => api.notifyResultClicked(index), 50);
  }

  onMount(() => {
    // Single ResizeObserver for the list container
    if (listRef) {
      const ro = new ResizeObserver((entries) => {
        for (const entry of entries) {
          setContainerWidth(entry.contentRect.width);
        }
      });
      ro.observe(listRef);
      onCleanup(() => ro.disconnect());
    }

    listen<ResultsUpdatedPayload>("results-updated", (event) => {
        if (event.payload.requestId < latestRequestId) {
          return;
        }
        latestRequestId = event.payload.requestId;
        setResults(event.payload.results);
        setSelected(event.payload.selected);
        fetchIcons(event.payload.results, event.payload.requestId);
        if (
          event.payload.selected !== lastScrolledSelected ||
          event.payload.requestId !== lastScrolledRequestId
        ) {
          lastScrolledSelected = event.payload.selected;
          lastScrolledRequestId = event.payload.requestId;
          queueMicrotask(() => {
            if (!listRef) return;
            const row = listRef.children[event.payload.selected] as HTMLElement | undefined;
            if (!row) return;
            ensureRowVisible(listRef, row);
          });
        }
        requestAnimationFrame(() => {
          void emit("results-render-done", { requestId: event.payload.requestId });
        });
      });
  });

  return (
    <div class="results-window">
      <div class="result-list-standalone" ref={listRef}>
        <For each={results()}>
          {(result, idx) => (
            <ResultRow
              result={result}
              isSelected={idx() === selected()}
              icon={iconCache().get(result.path)}
              containerWidth={containerWidth()}
              onClick={() => api.notifyResultClicked(idx())}
              onDoubleClick={() => api.notifyResultDoubleClicked(idx())}
              onMouseEnter={() => debouncedHover(idx())}
            />
          )}
        </For>
      </div>
    </div>
  );
};

export default ResultsWindow;
