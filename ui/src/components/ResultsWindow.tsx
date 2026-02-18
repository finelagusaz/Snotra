import { type Component, For, createSignal, onMount } from "solid-js";
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

  onMount(async () => {
    // Set WS_EX_NOACTIVATE so this window never steals focus
    await api.setWindowNoActivate();

    listen<{ results: SearchResult[]; selected: number }>(
      "results-updated",
      (event) => {
        setResults(event.payload.results);
        setSelected(event.payload.selected);
        fetchIcons(event.payload.results);
      },
    );
  });

  return (
    <div class="results-window">
      <div class="result-list-standalone">
        <For each={results()}>
          {(result, idx) => (
            <ResultRow
              result={result}
              isSelected={idx() === selected()}
              icon={iconCache().get(result.path)}
              onClick={() => api.notifyResultClicked(idx())}
              onDoubleClick={() => api.notifyResultDoubleClicked(idx())}
              onMouseEnter={() => api.notifyResultClicked(idx())}
            />
          )}
        </For>
      </div>
    </div>
  );
};

export default ResultsWindow;
