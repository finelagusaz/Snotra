import { createSignal, createEffect, on } from "solid-js";
import { emit, listen } from "@tauri-apps/api/event";
import type { SearchResult } from "../lib/types";
import * as api from "../lib/invoke";
import { findCommand, filterCommands, type SlashCommand } from "../lib/commands";
import { perfStartSearch, perfMarkSearchDone, perfCancelSearch } from "../lib/perf";
import { parsePathQuery } from "../lib/pathQuery";

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
let suppressNextQueryEffectRefresh = false;

function clampSelectedIndex(index: number, len: number): number {
  if (len <= 0) return 0;
  return Math.min(Math.max(index, 0), len - 1);
}

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
    void runRefresh();
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

async function refreshResults() {
  const requestId = ++latestRequestId;
  const fs = folderState();
  const q = query();
  const trimmed = q.trim();
  if (!fs && trimmed === "/r") {
    perfStartSearch(requestId, "history");
    const items = await api.getHistoryResults();
    if (requestId !== latestRequestId) {
      perfCancelSearch(requestId);
      return;
    }
    setCommandMatches([]);
    setResults(items);
    setSelected(0);
    perfMarkSearchDone(requestId, items.length);
    emitResults(items, 0, requestId);
    return;
  }
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
    : trimmed === "/r"
      ? "history"
      : fs || pathQuery
      ? "folder"
      : "query";
  perfStartSearch(requestId, source);

  if (indexing()) {
    setResults([]);
    setSelected(0);
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
    items = [];
  } else {
    items = await api.search(q);
  }

  if (requestId !== latestRequestId) {
    perfCancelSearch(requestId);
    return;
  }

  setResults(items);
  const nextSelected = clampSelectedIndex(selected(), items.length);
  setSelected(nextSelected);
  perfMarkSearchDone(requestId, items.length);
  emitResults(items, nextSelected, requestId);
}

// Auto-refresh when query changes (non-folder mode)
createEffect(
  on(query, (q) => {
    if (suppressNextQueryEffectRefresh) {
      suppressNextQueryEffectRefresh = false;
      return;
    }
    if (folderState()) return;
    const trimmed = q.trim();

    if (trimmed === "/r") {
      clearTimeout(debounceTimer);
      debounceTimer = undefined;
      setCommandMatches([]);
      setSelected(0);
      void runRefresh();
      return;
    }

    if (trimmed.startsWith("/")) {
      const cmd = findCommand(q);
      if (cmd && cmd.command !== "/r") {
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
  const nextSelected = clampSelectedIndex(selected(), results().length);
  if (nextSelected !== selected()) {
    setSelected(nextSelected);
  }
  emitResults(results(), nextSelected, latestRequestId);
}

function moveSelectionUp() {
  setSelected((s) => clampSelectedIndex(s - 1, results().length));
  emitSelectionUpdate();
}

function moveSelectionDown() {
  setSelected((s) => clampSelectedIndex(s + 1, results().length));
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
  void runRefresh();
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
  void runRefresh();
}

function trackRefresh(pending: Promise<void>): Promise<void> {
  refreshInFlight = pending;
  void pending.finally(() => {
    if (refreshInFlight === pending) {
      refreshInFlight = undefined;
    }
  });
  return pending;
}

function runRefresh(): Promise<void> {
  return trackRefresh(
    refreshResults().catch((e) => {
      console.error("Failed to refresh results:", e);
    }),
  );
}

async function flushPendingRefresh() {
  if (debounceTimer !== undefined) {
    clearTimeout(debounceTimer);
    debounceTimer = undefined;
    await runRefresh();
    return;
  }
  if (refreshInFlight) {
    await refreshInFlight;
  }
}

function resolveActivationIndex(items: SearchResult[], preferredPath?: string): number {
  if (preferredPath !== undefined) {
    return items.findIndex((item) => item.path === preferredPath);
  }
  return clampSelectedIndex(selected(), items.length);
}

function sleep(ms: number): Promise<void> {
  return new Promise((resolve) => {
    setTimeout(resolve, ms);
  });
}

async function resolveActivationTarget(
  preferredPath?: string,
): Promise<{ idx: number; result: SearchResult } | null> {
  await flushPendingRefresh();
  const items = results();
  const idx = resolveActivationIndex(items, preferredPath);
  const result = idx >= 0 ? items[idx] : undefined;
  if (!result) {
    return null;
  }
  return { idx, result };
}

function consumeCommandSelection(index: number): boolean {
  const trimmed = query().trim();
  if (!folderState() && trimmed.startsWith("/") && commandMatches().length > 0) {
    const cmd = commandMatches()[index];
    if (cmd) {
      if (cmd.command === "/r") {
        setQuery("/r");
        setCommandMatches([]);
        setSelected(0);
        void runRefresh();
        return true;
      }
      clearCommandModeStateAndEmit();
      cmd.action();
      return true;
    }
  }
  return false;
}

async function launchAndReset(result: SearchResult): Promise<boolean> {
  if (result.isError) return false;

  // Fix C: launchItem の前に count=0 を先行 emit し、flushPendingRefresh が
  // 発生させた count>0 ハンドラを rw.show() 到達前に stale 化する
  emitResults([], 0, ++latestRequestId);
  let launchError: unknown;
  for (let attempt = 0; attempt < 3; attempt += 1) {
    try {
      await api.launchItem(result.path, query());
      launchError = undefined;
      break;
    } catch (e) {
      launchError = e;
      if (attempt < 2) {
        await sleep(120);
      }
    }
  }
  if (launchError !== undefined) {
    console.error("Failed to launch item:", launchError);
    void runRefresh();
    return false;
  }

  setFolderState(null);
  setFolderFilter("");
  setResults([]);
  setSelected(0);
  emitResults([], 0, ++latestRequestId);

  return true;
}

async function activateSelected(): Promise<boolean> {
  if (activationInFlight) return false;
  activationInFlight = true;
  try {
    const target = await resolveActivationTarget();
    if (!target) return false;
    const { idx, result } = target;
    if (idx !== selected()) {
      setSelected(idx);
    }
    if (consumeCommandSelection(idx)) return false;
    return launchAndReset(result);
  } finally {
    activationInFlight = false;
  }
}

async function activateSelectedByPath(path: string): Promise<boolean> {
  if (activationInFlight) return false;
  activationInFlight = true;
  try {
    const target = await resolveActivationTarget(path);
    if (!target) {
      return false;
    }
    const { idx, result } = target;
    if (idx !== selected()) {
      setSelected(idx);
    }
    if (consumeCommandSelection(idx)) return false;
    return launchAndReset(result);
  } finally {
    activationInFlight = false;
  }
}

function resetForShow() {
  setFolderState(null);
  if (query() !== "") {
    suppressNextQueryEffectRefresh = true;
  }
  setQuery("");
  setFolderFilter("");
  setCommandMatches([]);
  setSelected(0);
  void runRefresh();
}

let unlistenIndexingComplete: (() => void) | undefined;

async function initIndexingState() {
  try {
    const state = await api.getIndexingState();
    setIndexing(state);
  } catch (e) {
    console.error("Failed to get indexing state:", e);
  }

  unlistenIndexingComplete = await listen("indexing-complete", () => {
    setIndexing(false);
    void runRefresh();
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
  activateSelectedByPath,
  refreshResults,
  resetForShow,
  indexing,
  initIndexingState,
  emitSelectionUpdate,
};
