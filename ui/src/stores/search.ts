import { createSignal, createEffect, createRoot, on, createMemo, batch } from "solid-js";
import { listen } from "@tauri-apps/api/event";
import type { OpenerTool, SavedViewState, SearchResult } from "../lib/types";
import * as api from "../lib/invoke";
import { findCommand } from "../lib/commands";
import { perfStartSearch, perfMarkSearchDone, perfCancelSearch } from "../lib/perf";
import { createLatestRun } from "../lib/latestRun";
import { clampSelectedIndex, computeParentDir } from "../lib/folderNav";
import { trace } from "../lib/trace";
import { folderState, setFolderState, folderFilter, setFolderFilter } from "./folder";
import { toolSelectionState, setToolSelectionState } from "./tool-selection";
import { clearLaunchNotice, notifyLaunchFailure } from "./launchNotice";
import {
  getInstantCommandItems,
  setInstantCommandItems,
  clearInstantCommandItems,
  hasPendingInstantCommandFetch,
  cancelInstantCommandDebounce,
  scheduleInstantCommandFetch,
} from "./instantCommand";

const [query, setQuery] = createSignal("");
const [results, setResults] = createSignal<SearchResult[]>([]);
const [selected, setSelected] = createSignal(0);
const [indexing, setIndexing] = createSignal(false);
const [launching, setLaunching] = createSignal(false);
const [instantCommandPrefix, setInstantCommandPrefix] = createSignal("@");
const [noResults, setNoResults] = createSignal(false);

/** setResults のラッパー。通常検索パスでのみ noResults を true にする */
function updateResults(items: SearchResult[], isNormalSearch = false) {
  batch(() => {
    setResults(items);
    setNoResults(isNormalSearch && items.length === 0);
  });
}

/** 結果と選択を初期状態にリセットする */
function clearResults() {
  updateResults([]);
  setSelected(0);
}

/** モーダル的なビュー（フォルダ展開・ツール選択）に入る直前の results/selected を退避する
 *  唯一の生成経路（choke point）。frame 固有の追加フィールド（currentDir・savedQuery 等）は
 *  呼び出し側でスプレッドして合成する。 */
function saveView(): SavedViewState {
  return { savedResults: results(), savedSelected: selected() };
}

/** saveView() で退避した results/selected を復元する唯一の経路（choke point）。
 *  frame 固有のフィールド（savedQuery 等）の復元は意味が呼び出し側ごとに異なるため含まない
 *  （folder は setQuery で復元、tool は復元せず起動時の元クエリとして使うのみ）。 */
function restoreView(saved: SavedViewState) {
  updateResults(saved.savedResults);
  setSelected(saved.savedSelected);
}

const DEBOUNCE_MS = 50;
let debounceTimer: ReturnType<typeof setTimeout> | undefined;
/** leading edge: デバウンス区間の最初の入力で即時発火済みなら true */
let leadingFired = false;
let refreshInFlight: Promise<void> | undefined;
let activationInFlight = false;
let suppressNextQueryEffectRefresh = false;

/** 検索/データ lane の supersede 調停 primitive（world 世代 + staleness を所有・choke point）。
 *  検索・instant fetch の「最新実行だけが結果を適用する」を `run()` が、モード遷移・起動が
 *  in-flight を無効化する「world 世代の前進」を `invalidate()` が担う（旧 `nextGeneration()`）。
 *  perf の requestId 源は `searchLane.current()`（旧 `searchGeneration` 直読）。
 *  flush 追跡（`refreshInFlight`/`flushPendingRefresh`）は refresh lane 固有のため runner に吸収せず
 *  下の `trackRefresh` に残す（instant/直接 refreshResults を待受対象に載せない現挙動を保つ）。 */
const searchLane = createLatestRun();

export type ViewKind = "results" | "folder" | "tool";
export type InterpKind = "plain" | "command" | "instant";

/** 網羅的 switch の default に置き、モード追加時の分岐漏れをコンパイルエラー化する */
function assertNever(x: never): never {
  throw new Error(`unhandled mode: ${x}`);
}

/** instant モード検出述語。`interpKind` と query effect の単一情報源（SSOT）。
 *  空 prefix では false（全入力が instant 化するのを防ぐ）。trim 規則もここに集約する。 */
function isInstantPrefix(rawQuery: string, prefix: string): boolean {
  return prefix !== "" && rawQuery.trimStart().startsWith(prefix);
}

/** 軸1: 結果リストを占める先頭ビュー（tool > folder > results の射影＝SPEC §18.5 優先度）。
 *  プリミティブを返すことで kind 変化時のみ伝播する（オブジェクト union は毎計算で新 identity）。 */
const viewKind = createMemo<ViewKind>(() =>
  toolSelectionState() ? "tool" : folderState() ? "folder" : "results",
);

/** 軸2: 入力の意味。viewKind=results のときだけ非 plain。query+prefix からの純粋導出（持続ラッチを廃止）。 */
const interpKind = createMemo<InterpKind>(() => {
  if (viewKind() !== "results") return "plain";
  const raw = query();
  if (isInstantPrefix(raw, instantCommandPrefix())) return "instant";
  if (raw.trimStart().startsWith("/")) return "command";
  return "plain";
});

/** フォルダ展開（ArrowRight/Left）・ツール選択（Shift+Enter）という「新規モーダル遷移」を
 *  許可するかの述語。tool 選択中（viewKind()==="tool"）または instant コマンドモード中
 *  （interpKind()==="instant"）はいずれも遷移をブロックする——という優先度をここに集約し、
 *  SearchWindow のキーハンドラはこれを消費するだけにする（複合条件を個別に再導出しない。
 *  development-principles.md「優先度・排他律は導出源の一箇所だけに書く」#431）。 */
function allowsFolderNav(): boolean {
  return viewKind() !== "tool" && interpKind() !== "instant";
}

/** 結果を表示すべきかの派生シグナル。MainApp がリアクティブにウィンドウ高さを変更するために使用。
 *  tool/folder は indexing 中でも表示。results は instant 中のみ indexing を無視する。 */
const shouldShowResults = createMemo(() => {
  if (results().length === 0) return false;
  const vk = viewKind();
  switch (vk) {
    case "tool":
    case "folder":
      return true;
    case "results":
      // interpKind 経由でも plain 打鍵では値不変ゆえ非伝播（プリミティブメモの値ゲート伝播）
      return interpKind() === "instant" || !indexing();
    default:
      return assertNever(vk);
  }
});

function clearCommandModeState() {
  searchLane.invalidate();
  setQuery("");
  clearResults();
}

function cancelDebounce() {
  if (debounceTimer !== undefined) {
    clearTimeout(debounceTimer);
    debounceTimer = undefined;
  }
  leadingFired = false;
}

function debouncedRefresh() {
  // Leading edge: デバウンス区間の最初の入力で即時発火する。
  // 以降はタイマーリセットのみ行い、最後の入力から DEBOUNCE_MS 後に trailing 発火する。
  if (!leadingFired) {
    leadingFired = true;
    void runRefresh();
  }
  if (debounceTimer !== undefined) {
    clearTimeout(debounceTimer);
  }
  debounceTimer = setTimeout(() => {
    debounceTimer = undefined;
    leadingFired = false;
    void runRefresh();
  }, DEBOUNCE_MS);
}

// Folder expansion state — signals live in ./folder.ts

async function refreshResults() {
  // ツール選択中・インスタントコマンドモード中は通常の検索で上書きしない。
  // ガードは searchLane.run() の前に置く——早期リターン時に world 世代を進めない現挙動を保つため。
  if (viewKind() === "tool") return;
  if (interpKind() === "instant") return;

  return searchLane.run(async ({ isStale, requestId }) => {
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
      if (isStale()) {
        trace("search:refresh:stale", { requestId, stage: "slash_r_history" });
        perfCancelSearch(requestId);
        return;
      }
      updateResults(items);
      setSelected(0);
      trace("search:refresh:done", { requestId, branch: "slash_r_history", count: items.length });
      perfMarkSearchDone(requestId, items.length);
      return;
    }
    if (!fs && trimmed.startsWith("/")) {
      // Command mode: no suggestions shown, just wait for exact match (handled by query effect).
      trace("search:refresh:branch", { requestId, branch: "slash_noop" });
      clearResults();
      return;
    }
    if (indexing() && !fs) {
      clearResults();
      trace("search:refresh:done", { requestId, branch: "indexing_guard", count: 0 });
      perfMarkSearchDone(requestId, 0);
      return;
    }

    const source = trimmed === "/r"
      ? "history"
      : fs
      ? "folder"
      : "query";
    perfStartSearch(requestId, source);

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
    } else if (trimmed === "") {
      trace("search:refresh:branch", { requestId, branch: "empty_query" });
      items = [];
    } else {
      trace("search:api:call", { requestId, api: "search", query: q });
      items = await api.search(q);
    }

    if (isStale()) {
      trace("search:refresh:stale", { requestId, stage: "post_api" });
      perfCancelSearch(requestId);
      return;
    }

    updateResults(items, source === "query" && trimmed !== "");
    const nextSelected = clampSelectedIndex(selected(), items.length);
    setSelected(nextSelected);
    trace("search:refresh:done", {
      requestId,
      branch: source,
      count: items.length,
      selected: nextSelected,
    });
    perfMarkSearchDone(requestId, items.length);
  });
}

/** instant コマンド入力のディスパッチ（interpKind()==="instant"）。30ms デバウンス IPC 取得は
 *  stores/instantCommand.ts の choke point（scheduleInstantCommandFetch）に委譲し、ここでは
 *  世代管理（staleness 判定用の hooks）と結果反映のみを担う。 */
function handleInstantQueryInput(q: string) {
  cancelDebounce();
  const prefix = instantCommandPrefix();
  // trimStart() を使用: trailing whitespace はクエリの一部として保持する
  const trimmedStart = q.trimStart();
  const input = trimmedStart.slice(prefix.length);
  // スペースがあればコマンド名部分のみでフィルタ（SPEC §18.5: スペースでマッチング確定）
  const spaceIdx = input.indexOf(" ");
  const filterName = spaceIdx >= 0 ? input.slice(0, spaceIdx) : input;
  trace("search:query_effect:instant_command", { prefix, input, filterName });
  scheduleInstantCommandFetch(filterName, {
    run: searchLane.run,
    onFetched: (fetchedResults) => {
      updateResults(fetchedResults);
      setSelected(0);
    },
    onError: (e) => {
      trace("search:instant_command:error", { error: String(e) });
    },
  });
}

/** スラッシュコマンド入力のディスパッチ（interpKind()==="command"）。"/r" は履歴の即時表示という
 *  特例、それ以外は完全一致コマンドの実行 or 候補なしクリアに分岐する。 */
function handleCommandQueryInput(q: string) {
  const trimmed = q.trim();
  if (trimmed === "/r") {
    cancelDebounce();
    setSelected(0);
    trace("search:query_effect:immediate_refresh", { reason: "slash_r" });
    void runRefresh();
    return;
  }

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
  searchLane.invalidate();
  updateResults([]);
  setSelected(0);
}

/** 通常クエリ入力のディスパッチ（interpKind()==="plain"）。leading+trailing デバウンスで
 *  refreshResults() へ委譲する。 */
function handlePlainQueryInput(q: string) {
  setSelected(0);
  trace("search:query_effect:debounced_refresh", { query: q });
  debouncedRefresh();
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
      const vk = viewKind();
      if (vk === "tool") {
        trace("search:query_effect:ignored_tool_selection", { query: q });
        return;
      }
      if (vk === "folder") {
        trace("search:query_effect:ignored_folder_mode", { query: q });
        return;
      }
      trace("search:query_effect", { query: q, trimmed: q.trim() });

      // ディスパッチは interpKind() 経由（優先度の再導出はしない。development-principles.md
      // 「優先度・排他律は導出源の一箇所だけに書く」#431 Phase3）。
      const kind = interpKind();
      if (kind === "instant") {
        handleInstantQueryInput(q);
        return;
      }

      // プレフィックスなし → instant モードの保留 IPC / stale 候補を掃除する。
      // interpKind は query から純粋導出されるため「モード解除」自体は状態更新不要。
      // 掃除すべき資源（pending fetch / stale items）が現存するときだけ実行する（無ければ no-op）。
      if (hasPendingInstantCommandFetch() || getInstantCommandItems().length > 0) {
        cancelInstantCommandDebounce();
        clearInstantCommandItems();
      }

      switch (kind) {
        case "command":
          handleCommandQueryInput(q);
          return;
        case "plain":
          handlePlainQueryInput(q);
          return;
        default:
          return assertNever(kind);
      }
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
      ...saveView(),
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

  searchLane.invalidate();
  restoreView(fs);
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


/** 起動フローの共通骨格（choke point）。「通知クリア→launching→世代更新→結果クリア→
 *  await→成功/失敗分岐→finally で launching 解除」を1箇所に閉じ込め、`launchAndReset` /
 *  `launchWithSelectedTool` / `executeInstantCommandSelected` の三重複を解消する（#431）。
 *  呼び出し側は「起動 API 呼び出し」と「成功/失敗時の後始末（trace・状態復元）」だけを渡す。
 *  `activationInFlight` ガードは呼び出し元ごとに事前条件チェックの粒度が異なる（tool フレーム有無・
 *  instant コマンド解決など）ため、ここでは扱わず各呼び出し元が個別に管理する。 */
async function withLaunchLifecycle(
  launch: () => Promise<api.LaunchResult>,
  onSuccess: (result: api.LaunchResult) => void,
  onFailure: (result: api.LaunchResult) => void,
): Promise<boolean> {
  clearLaunchNotice();
  setLaunching(true);
  try {
    searchLane.invalidate();
    clearResults();
    const launchResult = await launch();
    if (launchResult.status !== "ok") {
      notifyLaunchFailure(launchResult);
      onFailure(launchResult);
      return false;
    }
    onSuccess(launchResult);
    return true;
  } finally {
    setLaunching(false);
  }
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

    trace("search:launch_with_tool:start", {
      path: frame.targetPath,
      tool: tool.exe,
      query: frame.savedQuery,
    });
    return await withLaunchLifecycle(
      () => api.launchWithTool(frame.targetPath, frame.savedQuery, tool.exe, tool.args),
      () => {
        setToolSelectionState(null);
        setFolderState(null);
        setFolderFilter("");
        clearResults();
        searchLane.invalidate();
        trace("search:launch_with_tool:done", { path: frame.targetPath });
      },
      (launchResult) => {
        trace("search:launch_with_tool:error", {
          path: frame.targetPath,
          status: launchResult.status,
          code: launchResult.code,
          message: launchResult.message,
        });
        void runRefresh();
      },
    );
  } finally {
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
    ...saveView(),
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
  searchLane.invalidate();
  updateResults(toolResults);
  setSelected(0);
  trace("search:enter_tool_selection:ok", { path: result.path, toolCount: tools.length });
  return false;
}

function exitToolSelection(): boolean {
  const frame = toolSelectionState();
  if (!frame) return false;

  searchLane.invalidate();
  restoreView(frame);
  setToolSelectionState(null);
  // フォルダ展開中だった場合の folderFilter を復帰
  setFolderFilter(frame.savedFolderFilter);
  trace("search:exit_tool_selection:ok", { path: frame.targetPath });
  return true;
}

async function launchAndReset(result: SearchResult): Promise<boolean> {
  if (result.isError) return false;

  trace("search:launch:start", { path: result.path, query: query() });
  return withLaunchLifecycle(
    () => api.launchItem(result.path, query()),
    (launchResult) => {
      setFolderState(null);
      setFolderFilter("");
      clearResults();
      searchLane.invalidate();
      trace("search:launch:done", { path: result.path, code: launchResult.code });
    },
    (launchResult) => {
      trace("search:launch:error", {
        path: result.path,
        status: launchResult.status,
        code: launchResult.code,
        message: launchResult.message,
      });
      void runRefresh();
    },
  );
}

async function executeInstantCommandSelected(): Promise<boolean> {
  if (activationInFlight) return false;
  activationInFlight = true;
  try {
    const items = getInstantCommandItems();
    const idx = clampSelectedIndex(selected(), items.length);
    const cmd = items[idx];
    if (!cmd) return false;

    // クエリ部分を抽出（プレフィックス + コマンド名 + 空白以降）
    const prefix = instantCommandPrefix();
    const raw = query().trimStart().slice(prefix.length);
    const nameEnd = raw.indexOf(" ");
    const instantQuery = nameEnd >= 0 ? raw.slice(nameEnd + 1) : "";

    trace("search:instant_command:execute", { name: cmd.name, query: instantQuery });

    // 失敗時に復元するため、実行前の状態を保存
    const savedResults = results();
    const savedSelected = selected();
    const savedItems = [...items];
    // withLaunchLifecycle が世代を+1する直前の値。await 中の追加変化を検知するベースライン。
    const preGen = searchLane.current();

    return await withLaunchLifecycle(
      () => api.executeInstantCommand(cmd.name, instantQuery),
      () => {
        // 成功時: モードを完全にクリアする（query="" で interpKind は plain へ純粋導出）
        cancelInstantCommandDebounce();
        clearInstantCommandItems();
        suppressNextQueryEffectRefresh = true;
        setQuery("");
        trace("search:instant_command:done", { name: cmd.name });
      },
      (launchResult) => {
        trace("search:instant_command:error", {
          name: cmd.name,
          status: launchResult.status,
          code: launchResult.code,
          message: launchResult.message,
        });
        // 失敗時: onFailure 判定時点で world 世代が preGen+1（withLaunchLifecycle の 1 bump のみ）
        // ＝await 中に他の変化が無かったときだけ候補リストを復元する。
        if (searchLane.current() === preGen + 1) {
          searchLane.invalidate();
          setInstantCommandItems(savedItems);
          updateResults(savedResults);
          setSelected(savedSelected);
        }
      },
    );
  } finally {
    activationInFlight = false;
  }
}

/** ツール選択 / インスタントコマンドモードなら対応するディスパッチを返す。通常モードなら null
 *  でフォールスルー。index 指定時は先に選択を移す。activationInFlight ガードより前に呼ぶこと
 *  （ディスパッチ先が各自の並行ガードを持つ。順序は plan-review で固定）。 */
function tryModalActivate(index?: number): Promise<boolean> | null {
  if (viewKind() === "tool") {
    // ツール選択中: インデックスを直接使う（同一 exe の複数ツールを正確に区別）
    if (index !== undefined) setSelected(index);
    return launchWithSelectedTool();
  }
  if (interpKind() === "instant") {
    if (index !== undefined) setSelected(index);
    return executeInstantCommandSelected();
  }
  return null;
}

async function activateSelected(): Promise<boolean> {
  const modal = tryModalActivate();
  if (modal !== null) return modal;
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
  const modal = tryModalActivate(index);
  if (modal !== null) return modal;
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
  const skipRefresh = viewKind() === "results" && interpKind() === "plain" && query() === "";
  setLaunching(false);
  clearLaunchNotice();
  setToolSelectionState(null);
  setFolderState(null);
  cancelInstantCommandDebounce();
  clearInstantCommandItems();
  if (query() !== "") {
    suppressNextQueryEffectRefresh = true;
  }
  setQuery("");
  setFolderFilter("");
  setSelected(0);
  setNoResults(false);
  if (!skipRefresh) {
    void runRefresh();
  }
}

let unlistenIndexingComplete: (() => void) | undefined;
let unlistenIndexingStarted: (() => void) | undefined;

async function initIndexingState(): Promise<() => void> {
  try {
    const state = await api.getIndexingState();
    setIndexing(state);
    trace("search:indexing_state:init", { indexing: state });
  } catch (e) {
    trace("search:indexing_state:error", { error: String(e) });
    console.error("Failed to get indexing state:", e);
  }

  // Register indexing-started first to minimise the window between
  // getIndexingState() and the listener being live (indexing-started always
  // precedes indexing-complete in the event timeline).
  unlistenIndexingStarted = await listen("indexing-started", () => {
    trace("search:indexing_state:started");
    setIndexing(true);
  });

  unlistenIndexingComplete = await listen("indexing-complete", () => {
    trace("search:indexing_state:complete");
    setIndexing(false);
    void runRefresh();
  });

  return () => {
    unlistenIndexingComplete?.();
    unlistenIndexingComplete = undefined;
    unlistenIndexingStarted?.();
    unlistenIndexingStarted = undefined;
  };
}

/** 現在の world 世代を返す（perf 計測の requestId として使用）。実体は searchLane が所有する。 */
function getSearchGeneration(): number {
  return searchLane.current();
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
  viewKind,
  interpKind,
  allowsFolderNav,
  indexing,
  initIndexingState,
  launching,
  enterToolSelection,
  exitToolSelection,
  getSearchGeneration,
  noResults,
  setInstantCommandPrefix,
};

export { folderState, folderFilter, setFolderFilter } from "./folder";
export { toolSelectionState } from "./tool-selection";
// 公開 API は変えず、実装のみ stores/launchNotice.ts へ分割（#431 Phase3・DRY 再 export）
export {
  launchNotice,
  clearLaunchNotice,
  setLaunchNoticeWithAutoClear,
  setHotkeyFailureNotice,
} from "./launchNotice";
