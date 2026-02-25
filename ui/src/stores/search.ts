import { createSignal, createEffect, on } from "solid-js";
import { emit, listen } from "@tauri-apps/api/event";
import type { SearchResult } from "../lib/types";
import * as api from "../lib/invoke";
import { findCommand, filterCommands, type SlashCommand } from "../lib/commands";
import { perfStartSearch, perfMarkSearchDone, perfCancelSearch } from "../lib/perf";

const DEBOUNCE_MS = 30;

const [query, setQuery] = createSignal("");
const [results, setResults] = createSignal<SearchResult[]>([]);
const [selected, setSelected] = createSignal(0);
const [indexing, setIndexing] = createSignal(false);
const [commandMatches, setCommandMatches] = createSignal<SlashCommand[]>([]);

let debounceTimer: ReturnType<typeof setTimeout> | undefined;
let refreshInFlight: Promise<void> | undefined;
let latestRequestId = 0;
let activationInFlight = false;

function emitResults(items: SearchResult[], selectedIndex: number, requestId: number) {
  emit("results-updated", { results: items, selected: selectedIndex, requestId });
  emit("results-count-changed", { count: items.length, requestId });
}

function commandToResult(cmd: SlashCommand): SearchResult {
  return {
    name: cmd.label,
    path: `${cmd.command} ${cmd.description}`,
    isFolder: false,
    isError: false,
  };
}

function showCommandResults(input: string) {
  const matches = filterCommands(input);
  setCommandMatches(matches);
  const items = matches.map(commandToResult);
  const requestId = ++latestRequestId;
  setResults(items);
  setSelected(0);
  emitResults(items, 0, requestId);
}

function clearCommandModeStateAndEmit() {
  const requestId = ++latestRequestId;
  setQuery("");
  setCommandMatches([]);
  setResults([]);
  setSelected(0);
  emitResults([], 0, requestId);
}

function debouncedRefresh() {
  clearTimeout(debounceTimer);
  debounceTimer = setTimeout(() => {
    debounceTimer = undefined;
    const pending = refreshResults();
    refreshInFlight = pending;
    void pending.finally(() => {
      if (refreshInFlight === pending) {
        refreshInFlight = undefined;
      }
    });
  }, DEBOUNCE_MS);
}

// Folder expansion state
const [folderState, setFolderState] = createSignal<{
  currentDir: string;
  savedResults: SearchResult[];
  savedSelected: number;
  savedQuery: string;
} | null>(null);

const [folderFilter, setFolderFilter] = createSignal("");

function isPathQuery(input: string): boolean {
  const trimmed = input.trim();
  return trimmed.startsWith("\\") || trimmed.includes("\\") || trimmed.includes("/");
}

function parsePathQuery(input: string): { dir: string; filter: string } | null {
  const trimmed = input.trim();
  if (!isPathQuery(trimmed)) return null;

  const normalized = trimmed.replace(/\//g, "\\");
  if (normalized.endsWith("\\")) {
    return { dir: normalized, filter: "" };
  }

  const lastSlash = normalized.lastIndexOf("\\");
  if (lastSlash < 0) return null;

  let dir = normalized.slice(0, lastSlash + 1);
  if (dir === "") {
    dir = "\\";
  }

  return {
    dir,
    filter: normalized.slice(lastSlash + 1),
  };
}

async function refreshResults() {
  const requestId = ++latestRequestId;
  const fs = folderState();
  const q = query();
  const trimmed = q.trim();
  if (!fs && trimmed.startsWith("/")) {
    const matches = filterCommands(q);
    setCommandMatches(matches);
    const items = matches.map(commandToResult);
    setResults(items);
    setSelected(0);
    emitResults(items, 0, requestId);
    return;
  }
  const pathQuery = fs ? null : parsePathQuery(q);
  const source = indexing()
    ? "indexing"
    : fs || pathQuery
      ? "folder"
      : q.trim() === ""
        ? "history"
        : "query";
  perfStartSearch(requestId, source);

  if (indexing()) {
    setResults([]);
    perfMarkSearchDone(requestId, 0);
    emitResults([], 0, requestId);
    return;
  }

  let items: SearchResult[];
  if (fs) {
    items = await api.listFolder(fs.currentDir, folderFilter());
  } else if (pathQuery) {
    items = await api.listFolder(pathQuery.dir, pathQuery.filter);
  } else if (q.trim() === "") {
    items = await api.getHistoryResults();
  } else {
    items = await api.search(q);
  }

  if (requestId !== latestRequestId) {
    perfCancelSearch(requestId);
    return;
  }

  setResults(items);
  perfMarkSearchDone(requestId, items.length);
  emitResults(items, selected(), requestId);
}

// Auto-refresh when query changes (non-folder mode)
createEffect(
  on(query, (q) => {
    if (folderState()) return;
    const trimmed = q.trim();

    if (trimmed.startsWith("/")) {
      const cmd = findCommand(q);
      if (cmd) {
        clearTimeout(debounceTimer);
        debounceTimer = undefined;
        clearCommandModeStateAndEmit();
        cmd.action();
        return;
      }

      clearTimeout(debounceTimer);
      debounceTimer = undefined;
      showCommandResults(q);
      return;
    }

    setCommandMatches([]);
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
  emitResults(results(), selected(), latestRequestId);
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
  void api.recordFolderExpansion(dir);
  setFolderFilter("");
  setSelected(0);
  refreshResults();
}

function exitFolderExpansion(): boolean {
  const fs = folderState();
  if (!fs) return false;

  // デバウンスタイマーをクリア（フォルダモード中の入力残り処理を防止）
  clearTimeout(debounceTimer);
  debounceTimer = undefined;

  const requestId = ++latestRequestId;
  setResults(fs.savedResults);
  setSelected(fs.savedSelected);
  setFolderState(null);    // setQuery より先に null にする
  setFolderFilter("");
  setQuery(fs.savedQuery);
  emitResults(fs.savedResults, fs.savedSelected, requestId);
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
    const pending = refreshResults();
    refreshInFlight = pending;
    await pending.finally(() => {
      if (refreshInFlight === pending) {
        refreshInFlight = undefined;
      }
    });
    return;
  }
  if (refreshInFlight) {
    await refreshInFlight;
  }
}

async function activateSelected(): Promise<boolean> {
  if (activationInFlight) return false;
  activationInFlight = true;
  try {
    await flushPendingRefresh();
    if (!folderState() && query().trim().startsWith("/")) {
      const cmd = commandMatches()[selected()];
      if (cmd) {
        clearCommandModeStateAndEmit();
        cmd.action();
      }
      return false;
    }
    const r = results()[selected()];
    if (!r) return false;

    if (r.isError) return false;

    // Fix C: launchItem の前に count=0 を先行 emit し、flushPendingRefresh が
    // 発生させた count>0 ハンドラを rw.show() 到達前に stale 化する
    emitResults([], 0, ++latestRequestId);
    await api.launchItem(r.path, query());

    setFolderState(null);
    setFolderFilter("");
    setResults([]);
    setSelected(0);
    emitResults([], 0, ++latestRequestId);

    return true;
  } finally {
    activationInFlight = false;
  }
}

function resetForShow() {
  setQuery("");
  setFolderState(null);
  setFolderFilter("");
  setCommandMatches([]);
  setSelected(0);
  refreshResults();
}

let unlistenIndexingComplete: (() => void) | undefined;

async function initIndexingState() {
  const state = await api.getIndexingState();
  setIndexing(state);

  unlistenIndexingComplete = await listen("indexing-complete", () => {
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
  emitSelectionUpdate,
};
