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

  async function fetchIcons(items: SearchResult[], generation: number) {
    const cache = iconCache();
    const missing = items
      .filter((r) => !r.isError && !cache.has(r.path))
      .map((r) => r.path);
    if (missing.length === 0) return;

    const batch = await api.getIconsBatch(missing);
    if (generation !== latestGeneration) return;

    const next = new Map(cache);
    for (const [k, v] of Object.entries(batch)) {
      next.set(k, v);
    }
    setIconCache(next);
  }

  onMount(() => {
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
              onClick={() => api.notifyResultClicked(result.path)}
              onDoubleClick={() => api.notifyResultDoubleClicked(idx())}
            />
          )}
        </For>
      </div>
    </div>
  );
};

export default ResultsWindow;
