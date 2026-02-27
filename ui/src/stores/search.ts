import { createSignal, createEffect, on } from "solid-js";
import { emit, listen } from "@tauri-apps/api/event";
import type { SearchResult } from "../lib/types";
import * as api from "../lib/invoke";
import { findCommand, filterCommands, type SlashCommand } from "../lib/commands";
import { perfStartSearch, perfMarkSearchDone, perfCancelSearch } from "../lib/perf";
import { parsePathQuery } from "../lib/pathQuery";
import { trace } from "../lib/trace";
import type { ResultsPresentationReason } from "../lib/searchEvents";
import { folderState, setFolderState, folderFilter, setFolderFilter } from "./folder";

const DEBOUNCE_MS = 30;

const [query, setQuery] = createSignal("");
const [results, setResults] = createSignal<SearchResult[]>([]);
const [selected, setSelected] = createSignal(0);
const [indexing, setIndexing] = createSignal(false);
const [commandMatches, setCommandMatches] = createSignal<SlashCommand[]>([]);
const [launching, setLaunching] = createSignal(false);
const [launchNotice, setLaunchNotice] = createSignal<string | null>(null);

let debounceTimer: ReturnType<typeof setTimeout> | undefined;
let launchNoticeTimer: ReturnType<typeof setTimeout> | undefined;
let refreshInFlight: Promise<void> | undefined;
let latestRequestId = 0;
let activationInFlight = false;
let suppressNextQueryEffectRefresh = false;

function clampSelectedIndex(index: number, len: number): number {
  if (len <= 0) return 0;
  return Math.min(Math.max(index, 0), len - 1);
}

function emitResults(
  items: SearchResult[],
  selectedIndex: number,
  generation: number,
  options?: {
    shouldShow?: boolean;
    reason?: ResultsPresentationReason;
  },
) {
  emit("results-sync", {
    generation,
    results: items,
    selected: selectedIndex,
    shouldShow: options?.shouldShow ?? items.length > 0,
    reason: options?.reason ?? "query",
  });
}

function clearLaunchNotice() {
  if (launchNoticeTimer !== undefined) {
    clearTimeout(launchNoticeTimer);
    launchNoticeTimer = undefined;
  }
  if (launchNotice() !== null) {
    setLaunchNotice(null);
  }
}

function setLaunchNoticeWithAutoClear(message: string) {
  if (launchNoticeTimer !== undefined) {
    clearTimeout(launchNoticeTimer);
    launchNoticeTimer = undefined;
  }
  setLaunchNotice(message);
  launchNoticeTimer = setTimeout(() => {
    launchNoticeTimer = undefined;
    setLaunchNotice(null);
  }, 2400);
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
  emitResults(items, 0, requestId, { reason: "command", shouldShow: items.length > 0 });
}

function clearCommandModeStateAndEmit() {
  const requestId = ++latestRequestId;
  setQuery("");
  setCommandMatches([]);
  setResults([]);
  setSelected(0);
  emitResults([], 0, requestId, { reason: "command", shouldShow: false });
}

function debouncedRefresh() {
  clearTimeout(debounceTimer);
  debounceTimer = setTimeout(() => {
    debounceTimer = undefined;
    void runRefresh();
  }, DEBOUNCE_MS);
}

// Folder expansion state — signals live in ./folder.ts

async function refreshResults() {
  const requestId = ++latestRequestId;
  const fs = folderState();
  const q = query();
  const trimmed = q.trim();
  trace("search:refresh:start", {
    requestId,
    query: q,
    trimmed,
    folderMode: fs !== null,
    indexing: indexing(),
  });
  if (!fs && trimmed === "/r") {
    trace("search:refresh:branch", { requestId, branch: "slash_r_history" });
    perfStartSearch(requestId, "history");
    const items = await api.getHistoryResults();
    if (requestId !== latestRequestId) {
      trace("search:refresh:stale", { requestId, stage: "slash_r_history" });
      perfCancelSearch(requestId);
      return;
    }
    setCommandMatches([]);
    setResults(items);
    setSelected(0);
    trace("search:refresh:done", { requestId, branch: "slash_r_history", count: items.length });
    perfMarkSearchDone(requestId, items.length);
    emitResults(items, 0, requestId, { reason: "query", shouldShow: items.length > 0 });
    return;
  }
  if (!fs && trimmed.startsWith("/")) {
    trace("search:refresh:branch", { requestId, branch: "slash_suggestions", input: q });
    const matches = filterCommands(q);
    setCommandMatches(matches);
    const items = matches.map(commandToResult);
    setResults(items);
    setSelected(0);
    trace("search:refresh:done", { requestId, branch: "slash_suggestions", count: items.length });
    emitResults(items, 0, requestId, { reason: "command", shouldShow: items.length > 0 });
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
    trace("search:refresh:done", { requestId, branch: "indexing_guard", count: 0 });
    perfMarkSearchDone(requestId, 0);
    emitResults([], 0, requestId, { reason: "reset", shouldShow: false });
    return;
  }

  let items: SearchResult[];
  if (fs) {
    trace("search:api:call", {
      requestId,
      api: "list_folder",
      dir: fs.currentDir,
      filter: folderFilter(),
      mode: "folder_state",
    });
    items = await api.listFolder(fs.currentDir, folderFilter());
  } else if (pathQuery) {
    trace("search:api:call", {
      requestId,
      api: "list_folder",
      dir: pathQuery.dir,
      filter: pathQuery.filter,
      mode: "path_query",
    });
    items = await api.listFolder(pathQuery.dir, pathQuery.filter);
  } else if (q.trim() === "") {
    trace("search:refresh:branch", { requestId, branch: "empty_query" });
    items = [];
  } else {
    trace("search:api:call", { requestId, api: "search", query: q });
    items = await api.search(q);
  }

  if (requestId !== latestRequestId) {
    trace("search:refresh:stale", { requestId, stage: "post_api" });
    perfCancelSearch(requestId);
    return;
  }

  setResults(items);
  const nextSelected = clampSelectedIndex(selected(), items.length);
  setSelected(nextSelected);
  trace("search:refresh:done", {
    requestId,
    branch: source,
    count: items.length,
    selected: nextSelected,
  });
  perfMarkSearchDone(requestId, items.length);
  emitResults(items, nextSelected, requestId, {
    reason: "query",
    shouldShow: items.length > 0,
  });
}

// Auto-refresh when query changes (non-folder mode)
createEffect(
  on(query, (q) => {
    if (suppressNextQueryEffectRefresh) {
      trace("search:query_effect:suppressed", { query: q });
      suppressNextQueryEffectRefresh = false;
      return;
    }
    if (folderState()) {
      trace("search:query_effect:ignored_folder_mode", { query: q });
      return;
    }
    const trimmed = q.trim();
    trace("search:query_effect", { query: q, trimmed });

    if (trimmed === "/r") {
      clearTimeout(debounceTimer);
      debounceTimer = undefined;
      setCommandMatches([]);
      setSelected(0);
      trace("search:query_effect:immediate_refresh", { reason: "slash_r" });
      void runRefresh();
      return;
    }

    if (trimmed.startsWith("/")) {
      const cmd = findCommand(q);
      if (cmd && cmd.command !== "/r") {
        clearTimeout(debounceTimer);
        debounceTimer = undefined;
        trace("search:query_effect:run_command", { command: cmd.command });
        clearCommandModeStateAndEmit();
        cmd.action();
        return;
      }

      clearTimeout(debounceTimer);
      debounceTimer = undefined;
      trace("search:query_effect:show_command_suggestions", { input: q });
      showCommandResults(q);
      return;
    }

    setCommandMatches([]);
    setSelected(0);
    trace("search:query_effect:debounced_refresh", { query: q });
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
  emitResults(results(), nextSelected, latestRequestId, {
    reason: "selection",
    shouldShow: results().length > 0,
  });
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
  emitResults(fs.savedResults, fs.savedSelected, requestId, {
    reason: "query",
    shouldShow: fs.savedResults.length > 0,
  });
  return true;
}

function navigateFolderUp() {
  const fs = folderState();
  if (!fs) return;

  // UNC root: \\server\share (2 parts or fewer) → stop navigating up
  if (fs.currentDir.startsWith("\\\\")) {
    const parts = fs.currentDir.replace(/\\+$/, "").slice(2).split("\\").filter((p) => p.length > 0);
    if (parts.length <= 2) return;
  }

  let parent = fs.currentDir.replace(/\\[^\\]+$/, "");
  if (/^[A-Za-z]:$/.test(parent)) {
    parent += "\\";
  }
  if (parent === fs.currentDir || parent === "") {
    return;
  }
  // 二重防衛: parent が \\server（share 未満の UNC ルート）になる場合は遷移を中断する
  // 既存の parts.length <= 2 ガードが \\server\share で止めるが、直接入力等の異常経路への防衛として追加
  if (parent.startsWith("\\\\")) {
    const parentParts = parent.replace(/\\+$/, "").slice(2).split("\\").filter((p) => p.length > 0);
    if (parentParts.length < 2) return;
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
      trace("search:refresh:error", { error: String(e) });
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
      trace("search:command_selected", { index, command: cmd.command });
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

  clearLaunchNotice();
  setLaunching(true);
  trace("search:launch:start", { path: result.path, query: query() });
  try {
    // launch 開始時に results を明示的に隠す
    emitResults([], 0, ++latestRequestId, { reason: "launch", shouldShow: false });
    const launchResult = await api.launchItem(result.path, query());
    if (launchResult.status !== "ok") {
      trace("search:launch:error", {
        path: result.path,
        status: launchResult.status,
        code: launchResult.code,
        message: launchResult.message,
      });
      const detail = launchResult.message ? ` (${launchResult.message})` : "";
      if (launchResult.status === "timeout") {
        setLaunchNoticeWithAutoClear(`起動に時間がかかっています${detail}`);
      } else {
        setLaunchNoticeWithAutoClear(`起動に失敗しました${detail}`);
      }
      void runRefresh();
      return false;
    }

    setFolderState(null);
    setFolderFilter("");
    setResults([]);
    setSelected(0);
    emitResults([], 0, ++latestRequestId, { reason: "launch", shouldShow: false });
    trace("search:launch:done", { path: result.path, code: launchResult.code });
    return true;
  } finally {
    setLaunching(false);
  }
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
  trace("search:reset_for_show", { query: query() });
  setLaunching(false);
  clearLaunchNotice();
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
    trace("search:indexing_state:init", { indexing: state });
  } catch (e) {
    trace("search:indexing_state:error", { error: String(e) });
    console.error("Failed to get indexing state:", e);
  }

  unlistenIndexingComplete = await listen("indexing-complete", () => {
    trace("search:indexing_state:complete");
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
  launching,
  launchNotice,
  clearLaunchNotice,
};

export { folderState, folderFilter, setFolderFilter } from "./folder";
