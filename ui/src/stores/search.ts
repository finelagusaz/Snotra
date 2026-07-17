import { createSignal, createEffect, createRoot, on, createMemo, batch } from "solid-js";
import { listen } from "@tauri-apps/api/event";
import type { OpenerTool, SavedViewState, SearchResult } from "../lib/types";
import * as api from "../lib/invoke";
import { findCommand } from "../lib/commands";
import { perfStartSearch, perfMarkSearchDone, perfCancelSearch } from "../lib/perf";
import { createLatestRun } from "../lib/latestRun";
import { createExclusive } from "../lib/exclusive";
import { createOwnedTimer } from "../lib/ownedTimer";
import { clampSelectedIndex, computeParentDir } from "../lib/folderNav";
import { interpret, type ViewKind, type InterpKind } from "../lib/interpretQuery";
import { trace } from "../lib/trace";
import { folderState, setFolderState, folderFilter, setFolderFilter, type FolderFrame } from "./folder";
import { toolSelectionState, setToolSelectionState, type ToolSelectionFrame } from "./tool-selection";
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
 *  唯一の生成経路（choke point）。frame 固有の追加フィールド（currentDir・restoreQuery/launchQuery 等）は
 *  呼び出し側でスプレッドして合成する。 */
function saveView(): SavedViewState {
  return { savedResults: results(), savedSelected: selected() };
}

/** saveView() で退避した results/selected を復元する唯一の経路（choke point）。pop 時に popView が呼ぶ。
 *  frame 固有のフィールドの復元は意味が frame ごとに異なるため含まず（folder は restoreQuery で query 復元、
 *  tool は savedFolderFilter で filter 復帰）、popView の kind 別 switch が担う。 */
function restoreView(saved: SavedViewState) {
  updateResults(saved.savedResults);
  setSelected(saved.savedSelected);
}

const DEBOUNCE_MS = 50;
/** 検索 debounce の所有タイマー（trailing 50ms）。
 *  leading edge は debouncedRefresh が `!isPending()`（＝バースト先頭）から導出する（policy は
 *  primitive でなく呼び出し側の関心）。plain 打鍵と folderFilter effect の 2 経路が単一インスタンスを
 *  共有し、モード遷移を跨ぐ保留 timer を保存する。 */
const refreshTimer = createOwnedTimer(DEBOUNCE_MS);
let refreshInFlight: Promise<void> | undefined;

/** 検索/データ lane の supersede 調停 primitive（world 世代 + staleness を所有・choke point）。
 *  検索・instant fetch の「最新実行だけが結果を適用する」を `run()` が、モード遷移・起動が
 *  in-flight を無効化する「world 世代の前進」を `invalidate()` が担う。
 *  perf の requestId 源は `searchLane.current()`。
 *  flush 追跡（`refreshInFlight`/`flushPendingRefresh`）は refresh lane 固有のため runner に吸収せず
 *  下の `trackRefresh` に残す（instant/直接 refreshResults を待受対象に載せない現挙動を保つ）。 */
const searchLane = createLatestRun();

/** 起動（launch/activate）lane の mutex 調停 primitive（in-flight フラグを内部で所有・choke point）。
 *  「実行中なら 2 つ目を拒否（`undefined`）、完了時に必ず解放」を `createExclusive()` が担う。
 *  検索 lane の supersede（`searchLane`）と対をなす single-flight の並行方針（#535）。
 *  入れ子で起動系が呼ばれる経路（`activateSelected` → `tryModalActivate` → `launchWithSelectedTool` 等）は、
 *  `tryModalActivate` を `activationLane(...)` の前に置く「呼び出し順」で自己ブロックを回避する（再入は許可しない）。 */
const activationLane = createExclusive();

// ViewKind / InterpKind の定義・instant 判定述語（isInstantPrefix）・入力分類（interpret）は
// lib/interpretQuery.ts（純関数・SSOT）へ移設。公開 API 維持のため型を re-export する。
export type { ViewKind, InterpKind } from "../lib/interpretQuery";

/** 網羅的 switch の default に置き、モード追加時の分岐漏れをコンパイルエラー化する */
function assertNever(x: never): never {
  throw new Error(`unhandled mode: ${x}`);
}

/** モーダルビュースタックに積まれうるフレーム（folder/tool）の判別可能 union。`kind` で型が
 *  分離され、popView の網羅 switch と viewKind の頂点射影が共有する。union は types.ts に置かない
 *  （folder.ts/tool-selection.ts への逆 import が循環を生むため・#538）。 */
type ModalFrame = FolderFrame | ToolSelectionFrame;

/** モーダルビュースタックの頂点（tool > folder の順で射影）。null なら results（スタック空）。
 *  ViewStack の「頂点参照」を一箇所に集約し、viewKind と pop が共有する（#538）。 */
function stackTop(): ModalFrame | null {
  return toolSelectionState() ?? folderState();
}

/** 軸1: 結果リストを占める先頭ビュー（スタック頂点の種類の純関数＝SPEC §18.5 優先度 tool > folder > results）。
 *  プリミティブ（kind 文字列）を返すことで kind 変化時のみ伝播する（オブジェクト union は毎計算で新 identity）。 */
const viewKind = createMemo<ViewKind>(() => stackTop()?.kind ?? "results");

/** 軸2: 入力の意味。viewKind=results のときだけ非 plain。query+prefix からの純粋導出（持続ラッチを廃止）。
 *  分類は interpret（純関数・SSOT）へ委譲し、memo は **プリミティブ（.kind 文字列）を返す**契約を保つ
 *  （オブジェクト union を下流へ流すと query() 依存の再計算が毎打鍵で走る・ui/CLAUDE.md）。 */
const interpKind = createMemo<InterpKind>(
  () => interpret(query(), instantCommandPrefix(), viewKind()).kind,
);

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
  refreshTimer.cancel();
}

function debouncedRefresh() {
  // Leading edge: バースト先頭（保留タイマー無し＝!isPending()）でのみ即時発火する。
  // 発火済みフラグは別に持たない——`refreshTimer.isPending()` と全遷移で等価になり
  // 冗長なため、`isPending()` から導出する。
  if (!refreshTimer.isPending()) void runRefresh();
  // Trailing: 最後の入力から DEBOUNCE_MS 後に発火。arm が前回の保留を破棄して張り直す。
  refreshTimer.arm(() => void runRefresh());
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
      // Command mode: no suggestions shown, just wait for exact match (handled by dispatchQueryInput).
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
function handleInstantQueryInput(filterName: string) {
  cancelDebounce();
  // filterName（コマンド名）の抽出は interpret（純関数・SSOT）が担う。ここは fetch 委譲のみ。
  trace("search:query_input:instant_command", { filterName });
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
    trace("search:query_input:immediate_refresh", { reason: "slash_r" });
    void runRefresh();
    return;
  }

  const cmd = findCommand(q);
  if (cmd && cmd.command !== "/r") {
    cancelDebounce();
    trace("search:query_input:run_command", { command: cmd.command });
    clearCommandModeState();
    cmd.action();
    return;
  }

  // Command mode without exact match: no suggestions, just clear results.
  cancelDebounce();
  trace("search:query_input:slash_noop", { input: q });
  searchLane.invalidate();
  updateResults([]);
  setSelected(0);
}

/** 通常クエリ入力のディスパッチ（interpKind()==="plain"）。leading+trailing デバウンスで
 *  refreshResults() へ委譲する。 */
function handlePlainQueryInput(q: string) {
  setSelected(0);
  trace("search:query_input:debounced_refresh", { query: q });
  debouncedRefresh();
}

/** ユーザー入力の明示 dispatch（唯一の検索起動起点）。setQuery で query を更新し、interpret の
 *  意図に基づいて instant/command/plain へ振り分ける。プログラム的リセット（resetForShow・
 *  instant 成功・command 実行後の clearCommandModeState 等）はこの関数を **呼ばない別経路**であり、
 *  検索を起動しない。この経路分離により「今回だけ effect を黙らせる」類のワンショット
 *  フラグを要しない（#537）。 */
function dispatchQueryInput(value: string) {
  setQuery(value);
  const vk = viewKind();
  // tool/folder ガード（防御的）。実運用では handleInput が
  // tool で早期リターン・folder で setFolderFilter に振るため、ここへは vk==="results" 時のみ到達する。
  if (vk === "tool") {
    trace("search:query_input:ignored_tool_selection", { query: value });
    return;
  }
  if (vk === "folder") {
    trace("search:query_input:ignored_folder_mode", { query: value });
    return;
  }
  trace("search:query_input", { query: value, trimmed: value.trim() });

  // ディスパッチは interpret（純関数・SSOT）経由（優先度の再導出はしない・#431 Phase3）。
  const intent = interpret(value, instantCommandPrefix(), vk);
  if (intent.kind === "instant") {
    handleInstantQueryInput(intent.filterName);
    return;
  }

  // プレフィックスなし → instant モードの保留 IPC / stale 候補を掃除する。
  // interpret は query から純粋導出されるため「モード解除」自体は状態更新不要。
  // 掃除すべき資源（pending fetch / stale items）が現存するときだけ実行する（無ければ no-op）。
  if (hasPendingInstantCommandFetch() || getInstantCommandItems().length > 0) {
    cancelInstantCommandDebounce();
    clearInstantCommandItems();
  }

  switch (intent.kind) {
    case "command":
      handleCommandQueryInput(value);
      return;
    case "plain":
      handlePlainQueryInput(value);
      return;
    default:
      return assertNever(intent);
  }
}

createRoot(() => {
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
    // 新規 push: results/selected を snapshot し folder フレームを積む
    setFolderState({
      kind: "folder",
      currentDir: dir,
      ...saveView(),
      restoreQuery: query(),
    });
  } else {
    // Already in folder mode, navigate deeper（push せずフレーム内で currentDir を書き換え・spread が kind/restoreQuery を保持）
    setFolderState({ ...fs, currentDir: dir });
  }
  void api.recordFolderExpansion(dir);
  setFolderFilter("");
  setSelected(0);
  void runRefresh();
}

/** モーダルビュー（folder/tool）を 1 段 pop する統一規律（choke point・#538）。頂点スロットの frame を
 *  受け取り、共通の「invalidate → restoreView」の後、frame.kind ごとの onExit（folder: query 復元 +
 *  filter クリア、tool: 元の folderFilter 復帰）を網羅 switch で施しスロットを null 化する。個別 setX 順序を
 *  ここへ吸収する。cancelDebounce は **folder 経路のみ・invalidate より前**で呼ぶ——tool 中は入力が無効で
 *  folderFilter effect が保留 timer を張らないため。両経路で cancel すると挙動が変わる（enterToolSelection の
 *  await 窓で稀に残る timer を抑制してしまう）ので folder 固有にとどめ、現挙動を厳密保存する。 */
function popView(frame: ModalFrame): boolean {
  if (frame.kind === "folder") cancelDebounce();
  searchLane.invalidate();
  restoreView(frame);
  switch (frame.kind) {
    case "folder":
      setFolderState(null);    // setFolderFilter("") より先（null 後なら folderFilter effect が debouncedRefresh をスキップ）
      setFolderFilter("");
      setQuery(frame.restoreQuery);
      break;
    case "tool":
      setToolSelectionState(null);
      setFolderFilter(frame.savedFolderFilter);    // 2 段スタック復帰（下段 folder の filter を戻す）
      break;
    default:
      return assertNever(frame);
  }
  return true;
}

function exitFolderExpansion(): boolean {
  const fs = folderState();
  return fs ? popView(fs) : false;
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
  if (refreshTimer.isPending()) {
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
 *  また自身の `invalidate()` 直後に世代を捕捉し、「自分の launch を超えて world が動いたか」を答える
 *  `disturbed()` 述語を成功/失敗分岐へ配る（world 世代 comparison の choke point。呼び出し側は
 *  invalidate の bump 数を知らずに `if (!disturbed())` で保存状態の復元可否を判定できる・#539）。
 *  起動レーンの排他（`activationLane`）は呼び出し元ごとに事前条件チェックの粒度が異なる（tool フレーム有無・
 *  instant コマンド解決など）ため、ここでは扱わず各呼び出し元が `activationLane(...)` で包んで担う。 */
async function withLaunchLifecycle(
  launch: () => Promise<api.LaunchResult>,
  onSuccess: (result: api.LaunchResult, disturbed: () => boolean) => void,
  onFailure: (result: api.LaunchResult, disturbed: () => boolean) => void,
): Promise<boolean> {
  clearLaunchNotice();
  setLaunching(true);
  try {
    searchLane.invalidate();
    // この launch が確立した world 世代。await 中に他の invalidate/run が走れば current() が
    // これを超える＝disturbed。呼び出し側は invalidate の bump 数を知らずに staleness を問える。
    const launchGen = searchLane.current();
    const disturbed = () => searchLane.current() !== launchGen;
    clearResults();
    const launchResult = await launch();
    if (launchResult.status !== "ok") {
      notifyLaunchFailure(launchResult);
      onFailure(launchResult, disturbed);
      return false;
    }
    // disturbed は現状 onSuccess では消費されない（起動成功後の後始末は無条件）。onFailure と
    // 対称の署名を保つため両分岐へ渡す（consumer が生まれたときに配線済みにしておく意図ではない）。
    onSuccess(launchResult, disturbed);
    return true;
  } finally {
    setLaunching(false);
  }
}

async function launchWithSelectedTool(): Promise<boolean> {
  const frame = toolSelectionState();
  if (!frame) return false;

  return (await activationLane(async () => {
    const idx = selected();
    const tool = frame.tools[idx];
    if (!tool) return false;

    trace("search:launch_with_tool:start", {
      path: frame.targetPath,
      tool: tool.exe,
      query: frame.launchQuery,
    });
    return await withLaunchLifecycle(
      () => api.launchWithTool(frame.targetPath, frame.launchQuery, tool.exe, tool.args),
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
  })) ?? false;
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

  const frame: ToolSelectionFrame = {
    kind: "tool",
    targetPath: result.path,
    targetIsFolder: result.isFolder,
    tools,
    ...saveView(),
    launchQuery: query(),
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

  trace("search:exit_tool_selection:ok", { path: frame.targetPath });
  return popView(frame);
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
  return (await activationLane(async () => {
    const items = getInstantCommandItems();
    const idx = clampSelectedIndex(selected(), items.length);
    const cmd = items[idx];
    if (!cmd) return false;

    // クエリ部分（プレフィックス + コマンド名 + 空白以降）を interpret（純関数・SSOT）で抽出。
    // tryModalActivate が interpKind()==="instant" を確認した後の経路ゆえ intent は必ず instant。
    const intent = interpret(query(), instantCommandPrefix(), viewKind());
    const instantQuery = intent.kind === "instant" ? intent.instantQuery : "";

    trace("search:instant_command:execute", { name: cmd.name, query: instantQuery });

    // 失敗時に復元するため、実行前の状態を保存
    const savedResults = results();
    const savedSelected = selected();
    const savedItems = [...items];

    return await withLaunchLifecycle(
      () => api.executeInstantCommand(cmd.name, instantQuery),
      () => {
        // 成功時: モードを完全にクリアする（query="" で interpKind は plain へ純粋導出）。
        // raw setQuery("") は dispatchQueryInput を経由しない＝検索を起動しない。
        cancelInstantCommandDebounce();
        clearInstantCommandItems();
        setQuery("");
        trace("search:instant_command:done", { name: cmd.name });
      },
      (launchResult, disturbed) => {
        trace("search:instant_command:error", {
          name: cmd.name,
          status: launchResult.status,
          code: launchResult.code,
          message: launchResult.message,
        });
        // 失敗時: await 中に world が動いていなければ（＝この launch のみ）候補リストを復元する。
        // 世代比較は withLaunchLifecycle が所有する disturbed() 述語に委ねる（生の +1 算術を持たない・#539）。
        if (!disturbed()) {
          searchLane.invalidate();
          setInstantCommandItems(savedItems);
          updateResults(savedResults);
          setSelected(savedSelected);
        }
      },
    );
  })) ?? false;
}

/** ツール選択 / インスタントコマンドモードなら対応するディスパッチを返す。通常モードなら null
 *  でフォールスルー。index 指定時は先に選択を移す。`activationLane(...)` に入る前に呼ぶこと
 *  （modal 経路はディスパッチ先が自前の lane を取るため、外側 lane に入る前に分岐を確定させる。
 *  順序は plan-review で固定）。 */
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
  return (await activationLane(async () => {
    const target = await resolveActivationTarget();
    if (!target) return false;
    const { idx, result } = target;
    if (idx !== selected()) {
      setSelected(idx);
    }
    return launchAndReset(result);
  })) ?? false;
}

async function activateSelectedByIndex(index: number): Promise<boolean> {
  const modal = tryModalActivate(index);
  if (modal !== null) return modal;
  return (await activationLane(async () => {
    await flushPendingRefresh();
    const items = results();
    const idx = clampSelectedIndex(index, items.length);
    const result = items[idx];
    if (!result) return false;
    if (idx !== selected()) {
      setSelected(idx);
    }
    return launchAndReset(result);
  })) ?? false;
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
  // raw setQuery("") は dispatchQueryInput を経由しない＝検索を起動しない。検索は下の明示 runRefresh()
  // のみが担う（経路分離）。
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
  dispatchQueryInput,
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
// launchNotice の公開 API を search.ts に単一化する re-export（実装は stores/launchNotice.ts・#431）
export {
  launchNotice,
  clearLaunchNotice,
  setLaunchNoticeWithAutoClear,
  setHotkeyFailureNotice,
} from "./launchNotice";
