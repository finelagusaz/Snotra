import { type Component, For, createSignal, onMount, onCleanup } from "solid-js";
import { listen } from "@tauri-apps/api/event";
import type { SearchResult } from "../lib/types";
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
  let hoverTimer: ReturnType<typeof setTimeout> | undefined;

  async function fetchIcons(items: SearchResult[]) {
    const cache = iconCache();
    const missing = items
      .filter((r) => !r.isError && !cache.has(r.path))
      .map((r) => r.path);
    if (missing.length === 0) return;

    const batch = await api.getIconsBatch(missing);
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

    listen<{ results: SearchResult[]; selected: number }>(
      "results-updated",
      (event) => {
        setResults(event.payload.results);
        setSelected(event.payload.selected);
        fetchIcons(event.payload.results);
        queueMicrotask(() => {
          if (!listRef) return;
          const row = listRef.children[event.payload.selected] as HTMLElement | undefined;
          row?.scrollIntoView({ block: "nearest" });
        });
      },
    );
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
