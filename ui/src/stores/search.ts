import { createSignal, createEffect, on } from "solid-js";
import { emit, listen } from "@tauri-apps/api/event";
import type { SearchResult } from "../lib/types";
import * as api from "../lib/invoke";
import { findCommand } from "../lib/commands";

const DEBOUNCE_MS = 40;

const [query, setQuery] = createSignal("");
const [results, setResults] = createSignal<SearchResult[]>([]);
const [selected, setSelected] = createSignal(0);
const [indexing, setIndexing] = createSignal(false);

let debounceTimer: ReturnType<typeof setTimeout> | undefined;
let latestRequestId = 0;

function debouncedRefresh() {
  clearTimeout(debounceTimer);
  debounceTimer = setTimeout(() => refreshResults(), DEBOUNCE_MS);
}

// Folder expansion state
const [folderState, setFolderState] = createSignal<{
  currentDir: string;
  savedResults: SearchResult[];
  savedSelected: number;
  savedQuery: string;
} | null>(null);

const [folderFilter, setFolderFilter] = createSignal("");

async function refreshResults() {
  const requestId = ++latestRequestId;
  if (indexing()) {
    setResults([]);
    emit("results-updated", { results: [], selected: 0, requestId });
    emit("results-count-changed", { count: 0, requestId });
    return;
  }

  const q = query();
  const fs = folderState();

  let items: SearchResult[];
  if (fs) {
    items = await api.listFolder(fs.currentDir, folderFilter());
  } else if (q.trim() === "") {
    items = await api.getHistoryResults();
  } else {
    items = await api.search(q);
  }

  if (requestId !== latestRequestId) {
    return;
  }

  setResults(items);
  emit("results-updated", { results: items, selected: selected(), requestId });
  emit("results-count-changed", { count: items.length, requestId });
}

// Auto-refresh when query changes (non-folder mode)
createEffect(
  on(query, (q) => {
    if (folderState()) return;

    const cmd = findCommand(q);
    if (cmd) {
      clearTimeout(debounceTimer);
      debounceTimer = undefined;
      const requestId = ++latestRequestId;
      setQuery("");
      setResults([]);
      setSelected(0);
      emit("results-updated", { results: [], selected: 0, requestId });
      emit("results-count-changed", { count: 0, requestId });
      cmd.action();
      return;
    }

    setSelected(0);
    debouncedRefresh();
  }),
);

// Auto-refresh when folder filter changes
createEffect(
  on(folderFilter, () => {
    if (folderState()) {
      setSelected(0);
      debouncedRefresh();
    }
  }),
);

function emitSelectionUpdate() {
  emit("results-updated", {
    results: results(),
    selected: selected(),
    requestId: latestRequestId,
  });
}

function moveSelectionUp() {
  setSelected((s) => Math.max(0, s - 1));
  emitSelectionUpdate();
}

function moveSelectionDown() {
  setSelected((s) => Math.min(results().length - 1, s + 1));
  emitSelectionUpdate();
}

function enterFolderExpansion(dir: string) {
  const fs = folderState();
  if (!fs) {
    // Save current state before entering folder mode
    setFolderState({
      currentDir: dir,
      savedResults: results(),
      savedSelected: selected(),
      savedQuery: query(),
    });
  } else {
    // Already in folder mode, navigate deeper
    setFolderState({ ...fs, currentDir: dir });
  }
  setFolderFilter("");
  setSelected(0);
  refreshResults();
}

function exitFolderExpansion(): boolean {
  const fs = folderState();
  if (!fs) return false;
  setResults(fs.savedResults);
  setSelected(fs.savedSelected);
  setQuery(fs.savedQuery);
  setFolderState(null);
  setFolderFilter("");
  return true;
}

function navigateFolderUp() {
  const fs = folderState();
  if (!fs) return;

  let parent = fs.currentDir.replace(/\\[^\\]+$/, "");
  if (/^[A-Za-z]:$/.test(parent)) {
    parent += "\\";
  }
  if (parent === fs.currentDir || parent === "") {
    return;
  }
  setFolderState({ ...fs, currentDir: parent });
  setFolderFilter("");
  setSelected(0);
  refreshResults();
}

async function flushPendingRefresh() {
  if (debounceTimer !== undefined) {
    clearTimeout(debounceTimer);
    debounceTimer = undefined;
    await refreshResults();
  }
}

async function activateSelected() {
  await flushPendingRefresh();
  const r = results()[selected()];
  if (!r) return;

  if (r.isError) return;

  if (r.isFolder) {
    enterFolderExpansion(r.path);
    return;
  }

  await api.launchItem(r.path, query());
}

function resetForShow() {
  setQuery("");
  setFolderState(null);
  setFolderFilter("");
  setSelected(0);
  refreshResults();
}

async function initIndexingState() {
  const state = await api.getIndexingState();
  setIndexing(state);

  listen("indexing-complete", () => {
    setIndexing(false);
    refreshResults();
  });
}

export {
  query,
  setQuery,
  results,
  selected,
  setSelected,
  folderState,
  folderFilter,
  setFolderFilter,
  moveSelectionUp,
  moveSelectionDown,
  enterFolderExpansion,
  exitFolderExpansion,
  navigateFolderUp,
  activateSelected,
  refreshResults,
  resetForShow,
  indexing,
  initIndexingState,
};
