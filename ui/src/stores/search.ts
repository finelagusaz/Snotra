import { createSignal, createEffect, createRoot, on, createMemo } from "solid-js";
import { listen } from "@tauri-apps/api/event";
import type { InstantCommand, OpenerTool, SearchResult } from "../lib/types";
import * as api from "../lib/invoke";
import { findCommand } from "../lib/commands";
import { perfStartSearch, perfMarkSearchDone, perfCancelSearch } from "../lib/perf";
import { parsePathQuery } from "../lib/pathQuery";
import { clampSelectedIndex, computeParentDir } from "../lib/folderNav";
import { trace } from "../lib/trace";
import { folderState, setFolderState, folderFilter, setFolderFilter } from "./folder";
import { toolSelectionState, setToolSelectionState } from "./tool-selection";
import { t } from "../lib/i18n";

const [query, setQuery] = createSignal("");
const [results, setResults] = createSignal<SearchResult[]>([]);
const [selected, setSelected] = createSignal(0);
const [indexing, setIndexing] = createSignal(false);
const [launching, setLaunching] = createSignal(false);
const [launchNotice, setLaunchNotice] = createSignal<string | null>(null);
const [instantCommandPrefix, setInstantCommandPrefix] = createSignal("@");
const [instantCommandMode, setInstantCommandMode] = createSignal(false);
/** インスタントコマンドモード中のコマンド一覧（activateSelected で参照） */
let instantCommandItems: InstantCommand[] = [];

let debounceTimer: ReturnType<typeof requestAnimationFrame> | undefined;
let launchNoticeTimer: ReturnType<typeof setTimeout> | undefined;
let refreshInFlight: Promise<void> | undefined;
let searchGeneration = 0;
let activationInFlight = false;
let suppressNextQueryEffectRefresh = false;

/** 結果を表示すべきかの派生シグナル。MainApp がリアクティブにウィンドウ高さを変更するために使用 */
const shouldShowResults = createMemo(() => results().length > 0 && (!indexing() || instantCommandMode()));

function clearLaunchNotice() {
  if (launchNoticeTimer !== undefined) {
    clearTimeout(launchNoticeTimer);
    launchNoticeTimer = undefined;
  }
  if (launchNotice() !== null) {
    setLaunchNotice(null);
  }
}

/** LaunchResult の失敗/タイムアウトに応じた通知を表示する */
function notifyLaunchFailure(result: api.LaunchResult) {
  const detail = result.message ? ` (${result.message})` : "";
  if (result.status === "timeout") {
    setLaunchNoticeWithAutoClear(t("notice.launch.timeout", { detail }));
  } else {
    setLaunchNoticeWithAutoClear(t("notice.launch.failed", { detail }));
  }
}

function setLaunchNoticeWithAutoClear(message: string, delayMs = 2400) {
  clearLaunchNotice();
  setLaunchNotice(message);
  launchNoticeTimer = setTimeout(() => {
    launchNoticeTimer = undefined;
    setLaunchNotice(null);
  }, delayMs);
}

export function setHotkeyFailureNotice(message: string) {
  setLaunchNoticeWithAutoClear(message, 5000);
}

function clearCommandModeState() {
  ++searchGeneration;
  setQuery("");
  setResults([]);
  setSelected(0);
}

function cancelDebounce() {
  if (debounceTimer !== undefined) {
    cancelAnimationFrame(debounceTimer);
    debounceTimer = undefined;
  }
}

function debouncedRefresh() {
  cancelDebounce();
  debounceTimer = requestAnimationFrame(() => {
    debounceTimer = undefined;
    void runRefresh();
  });
}

// Folder expansion state — signals live in ./folder.ts

async function refreshResults() {
  // ツール選択中・インスタントコマンドモード中は通常の検索で上書きしない
  if (toolSelectionState()) return;
  if (instantCommandMode()) return;

  const requestId = ++searchGeneration;
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
    if (requestId !== searchGeneration) {
      trace("search:refresh:stale", { requestId, stage: "slash_r_history" });
      perfCancelSearch(requestId);
      return;
    }
    setResults(items);
    setSelected(0);
    trace("search:refresh:done", { requestId, branch: "slash_r_history", count: items.length });
    perfMarkSearchDone(requestId, items.length);
    return;
  }
  if (!fs && trimmed.startsWith("/")) {
    // Command mode: no suggestions shown, just wait for exact match (handled by query effect).
    trace("search:refresh:branch", { requestId, branch: "slash_noop" });
    setResults([]);
    setSelected(0);
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

  if (requestId !== searchGeneration) {
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
}

createRoot(() => {
  // Auto-refresh when query changes (non-folder mode)
  createEffect(
    on(query, (q) => {
      if (suppressNextQueryEffectRefresh) {
        trace("search:query_effect:suppressed", { query: q });
        suppressNextQueryEffectRefresh = false;
        return;
      }
      if (toolSelectionState()) {
        trace("search:query_effect:ignored_tool_selection", { query: q });
        return;
      }
      if (folderState()) {
        trace("search:query_effect:ignored_folder_mode", { query: q });
        return;
      }
      const trimmed = q.trim();
      const prefix = instantCommandPrefix();
      trace("search:query_effect", { query: q, trimmed });

      // インスタントコマンドモード判定（スラッシュコマンドより先に評価）
      // trimStart() を使用: trailing whitespace はクエリの一部として保持する
      const trimmedStart = q.trimStart();
      if (prefix && trimmedStart.startsWith(prefix)) {
        cancelDebounce();
        const input = trimmedStart.slice(prefix.length);
        // スペースがあればコマンド名部分のみでフィルタ（SPEC §18.5: スペースでマッチング確定）
        const spaceIdx = input.indexOf(" ");
        const filterName = spaceIdx >= 0 ? input.slice(0, spaceIdx) : input;
        trace("search:query_effect:instant_command", { prefix, input, filterName });
        setInstantCommandMode(true);
        // IPC 応答前の Enter/クリックで古いコマンドを誤起動しないよう、先にクリアする
        instantCommandItems = [];
        void (async () => {
          const requestId = ++searchGeneration;
          try {
            const commands = await api.getInstantCommands(filterName);
            if (requestId !== searchGeneration) return;
            instantCommandItems = commands;
            const items: SearchResult[] = commands.map((cmd) => ({
              name: cmd.name,
              path: cmd.command,
              isFolder: false,
              isError: false,
            }));
            setResults(items);
            setSelected(0);
          } catch (e) {
            trace("search:instant_command:error", { error: String(e) });
          }
        })();
        return;
      }

      // プレフィックスなし → インスタントコマンドモードを解除
      if (instantCommandMode()) {
        setInstantCommandMode(false);
        instantCommandItems = [];
      }

      if (trimmed === "/r") {
        cancelDebounce();
        setSelected(0);
        trace("search:query_effect:immediate_refresh", { reason: "slash_r" });
        void runRefresh();
        return;
      }

      if (trimmed.startsWith("/")) {
        const cmd = findCommand(q);
        if (cmd && cmd.command !== "/r") {
          cancelDebounce();
          trace("search:query_effect:run_command", { command: cmd.command });
          clearCommandModeState();
          cmd.action();
          return;
        }

        // Command mode without exact match: no suggestions, just clear results.
        cancelDebounce();
        trace("search:query_effect:slash_noop", { input: q });
        ++searchGeneration;
        setResults([]);
        setSelected(0);
        return;
      }

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
});

function moveSelectionUp() {
  setSelected((s) => clampSelectedIndex(s - 1, results().length));
}

function moveSelectionDown() {
  setSelected((s) => clampSelectedIndex(s + 1, results().length));
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
  cancelDebounce();

  ++searchGeneration;
  setResults(fs.savedResults);
  setSelected(fs.savedSelected);
  setFolderState(null);    // setQuery より先に null にする
  setFolderFilter("");
  setQuery(fs.savedQuery);
  return true;
}

function navigateFolderUp() {
  const fs = folderState();
  if (!fs) return;

  const parent = computeParentDir(fs.currentDir);
  if (parent === null) return;

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
    cancelDebounce();
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


async function launchWithSelectedTool(): Promise<boolean> {
  if (activationInFlight) return false;
  const frame = toolSelectionState();
  if (!frame) return false;

  activationInFlight = true;
  try {
    const idx = selected();
    const tool = frame.tools[idx];
    if (!tool) return false;

    clearLaunchNotice();
    setLaunching(true);
    trace("search:launch_with_tool:start", {
      path: frame.targetPath,
      tool: tool.exe,
      query: frame.savedQuery,
    });
    // 結果を隠す
    ++searchGeneration;
    setResults([]);
    setSelected(0);
    const launchResult = await api.launchWithTool(
      frame.targetPath,
      frame.savedQuery,
      tool.exe,
      tool.args,
    );
    if (launchResult.status !== "ok") {
      trace("search:launch_with_tool:error", {
        path: frame.targetPath,
        status: launchResult.status,
        code: launchResult.code,
        message: launchResult.message,
      });
      notifyLaunchFailure(launchResult);
      void runRefresh();
      return false;
    }

    setToolSelectionState(null);
    setFolderState(null);
    setFolderFilter("");
    setResults([]);
    setSelected(0);
    ++searchGeneration;
    trace("search:launch_with_tool:done", { path: frame.targetPath });
    return true;
  } finally {
    setLaunching(false);
    activationInFlight = false;
  }
}

async function enterToolSelection(result: SearchResult): Promise<boolean> {
  // 残存 debounce タイマーを破棄（C3対策）
  cancelDebounce();

  let tools: OpenerTool[];
  try {
    tools = await api.getMatchingTools(result.path, result.isFolder);
  } catch (e) {
    trace("search:enter_tool_selection:error", { error: String(e) });
    return false;
  }

  if (tools.length <= 1) {
    // ツールが1件以下なら通常起動にフォールバック
    trace("search:enter_tool_selection:fallback", { toolCount: tools.length });
    return activateSelected();
  }

  const frame = {
    targetPath: result.path,
    targetIsFolder: result.isFolder,
    tools,
    savedResults: results(),
    savedSelected: selected(),
    savedQuery: query(),
    savedFolderFilter: folderFilter(),
  };
  setToolSelectionState(frame);

  // ツールを SearchResult として表示
  const toolResults: SearchResult[] = tools.map((tool) => ({
    name: tool.name,
    path: tool.exe,
    isFolder: false,
    isError: false,
  }));
  ++searchGeneration;
  setResults(toolResults);
  setSelected(0);
  trace("search:enter_tool_selection:ok", { path: result.path, toolCount: tools.length });
  return false;
}

function exitToolSelection(): boolean {
  const frame = toolSelectionState();
  if (!frame) return false;

  ++searchGeneration;
  setResults(frame.savedResults);
  setSelected(frame.savedSelected);
  setToolSelectionState(null);
  // フォルダ展開中だった場合の folderFilter を復帰
  setFolderFilter(frame.savedFolderFilter);
  trace("search:exit_tool_selection:ok", { path: frame.targetPath });
  return true;
}

async function launchAndReset(result: SearchResult): Promise<boolean> {
  if (result.isError) return false;

  clearLaunchNotice();
  setLaunching(true);
  trace("search:launch:start", { path: result.path, query: query() });
  try {
    // launch 開始時に results を隠す
    ++searchGeneration;
    setResults([]);
    setSelected(0);
    const launchResult = await api.launchItem(result.path, query());
    if (launchResult.status !== "ok") {
      trace("search:launch:error", {
        path: result.path,
        status: launchResult.status,
        code: launchResult.code,
        message: launchResult.message,
      });
      notifyLaunchFailure(launchResult);
      void runRefresh();
      return false;
    }

    setFolderState(null);
    setFolderFilter("");
    setResults([]);
    setSelected(0);
    ++searchGeneration;
    trace("search:launch:done", { path: result.path, code: launchResult.code });
    return true;
  } finally {
    setLaunching(false);
  }
}

async function executeInstantCommandSelected(): Promise<boolean> {
  if (activationInFlight) return false;
  activationInFlight = true;
  try {
    const idx = clampSelectedIndex(selected(), instantCommandItems.length);
    const cmd = instantCommandItems[idx];
    if (!cmd) return false;

    // クエリ部分を抽出（プレフィックス + コマンド名 + 空白以降）
    const prefix = instantCommandPrefix();
    const raw = query().trimStart().slice(prefix.length);
    const nameEnd = raw.indexOf(" ");
    const instantQuery = nameEnd >= 0 ? raw.slice(nameEnd + 1) : "";

    clearLaunchNotice();
    setLaunching(true);
    trace("search:instant_command:execute", { name: cmd.name, query: instantQuery });

    // 失敗時に復元するため、実行前の状態を保存
    const savedResults = results();
    const savedSelected = selected();
    const savedItems = [...instantCommandItems];

    const preGen = searchGeneration;
    ++searchGeneration;
    setResults([]);
    setSelected(0);

    const launchResult = await api.executeInstantCommand(cmd.name, instantQuery);
    if (launchResult.status !== "ok") {
      trace("search:instant_command:error", {
        name: cmd.name,
        status: launchResult.status,
        code: launchResult.code,
        message: launchResult.message,
      });
      notifyLaunchFailure(launchResult);
      // 失敗時: await 中に状態が変わっていなければ候補リストを復元
      if (searchGeneration === preGen + 1) {
        ++searchGeneration;
        instantCommandItems = savedItems;
        setResults(savedResults);
        setSelected(savedSelected);
      }
      return false;
    }

    // 成功時: モードを完全にクリアする
    setInstantCommandMode(false);
    instantCommandItems = [];
    suppressNextQueryEffectRefresh = true;
    setQuery("");
    trace("search:instant_command:done", { name: cmd.name });
    return true;
  } finally {
    setLaunching(false);
    activationInFlight = false;
  }
}

async function activateSelected(): Promise<boolean> {
  if (toolSelectionState()) {
    return launchWithSelectedTool();
  }
  if (instantCommandMode()) {
    return executeInstantCommandSelected();
  }
  if (activationInFlight) return false;
  activationInFlight = true;
  try {
    const target = await resolveActivationTarget();
    if (!target) return false;
    const { idx, result } = target;
    if (idx !== selected()) {
      setSelected(idx);
    }
    return launchAndReset(result);
  } finally {
    activationInFlight = false;
  }
}

async function activateSelectedByIndex(index: number): Promise<boolean> {
  if (toolSelectionState()) {
    // ツール選択中: インデックスを直接使う（同一 exe の複数ツールを正確に区別）
    setSelected(index);
    return launchWithSelectedTool();
  }
  if (instantCommandMode()) {
    setSelected(index);
    return executeInstantCommandSelected();
  }
  if (activationInFlight) return false;
  activationInFlight = true;
  try {
    await flushPendingRefresh();
    const items = results();
    const idx = clampSelectedIndex(index, items.length);
    const result = items[idx];
    if (!result) return false;
    if (idx !== selected()) {
      setSelected(idx);
    }
    return launchAndReset(result);
  } finally {
    activationInFlight = false;
  }
}

function resetForShow() {
  trace("search:reset_for_show", { query: query() });
  // すでにクリーン状態なら runRefresh() をスキップ。
  // リセット前に確認する（setFolderState / setToolSelectionState が呼ばれる前）。
  // indexing() は含めない: indexing=true 時も results は既に非表示のため、スキップしても問題ない。
  const skipRefresh = query() === "" && folderState() === null && toolSelectionState() === null && !instantCommandMode();
  setLaunching(false);
  clearLaunchNotice();
  setToolSelectionState(null);
  setFolderState(null);
  setInstantCommandMode(false);
  instantCommandItems = [];
  if (query() !== "") {
    suppressNextQueryEffectRefresh = true;
  }
  setQuery("");
  setFolderFilter("");
  setSelected(0);
  if (!skipRefresh) {
    void runRefresh();
  }
}

let unlistenIndexingComplete: (() => void) | undefined;

async function initIndexingState(): Promise<() => void> {
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
  return () => {
    unlistenIndexingComplete?.();
    unlistenIndexingComplete = undefined;
  };
}

/** 現在の searchGeneration を返す（perf 計測の requestId として使用） */
function getSearchGeneration(): number {
  return searchGeneration;
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
  activateSelectedByIndex,
  refreshResults,
  resetForShow,
  shouldShowResults,
  indexing,
  initIndexingState,
  launching,
  launchNotice,
  clearLaunchNotice,
  enterToolSelection,
  exitToolSelection,
  getSearchGeneration,
  instantCommandMode,
  instantCommandPrefix,
  setInstantCommandPrefix,
};

export { folderState, folderFilter, setFolderFilter } from "./folder";
export { toolSelectionState } from "./tool-selection";
