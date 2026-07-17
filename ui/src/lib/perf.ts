/**
 * 開発時専用の 3 フェーズ（入力→検索→描画）レイテンシ計測。P50/P95 を `console.table` 出力する。
 *
 * 呼び出し側の契約:
 * - **呼び出し順序**: `perfMarkInput()` → `perfStartSearch(requestId, ...)` →
 *   `perfMarkSearchDone(requestId)` → `perfMarkRenderDone(requestId)` の順。`perfMarkInput` を
 *   欠くと `perfStartSearch` は計測を黙って捨てる（単一スロット `pendingInputAt` が undefined のため）。
 * - **解放義務**: stale になった実行は `perfCancelSearch(requestId)` で保留計測を除去すること。
 *   怠ると保留が滞留し、上限 `MAX_PENDING` 到達で全保留が clear され計測精度が落ちる。
 * - **requestId の一意性**: `searchLane.current()`（world 世代）由来であることを前提とする。
 * - 集計対象は `source === "query"` のみ（folder/history/indexing は計測しても集計に載せない）。
 * - `ENABLED`（DEV ビルドかつ `localStorage.snotra_perf === "1"`。非ブラウザ環境では常に偽）が
 *   偽なら全関数 no-op。
 *
 * @packageDocumentation
 */

type PhaseSource = "query" | "folder" | "history" | "indexing";

type Sample = {
  inputToSearch: number;
  searchToRender: number;
  inputToRender: number;
};

type Pending = {
  inputAt: number;
  searchStartAt: number;
  searchDoneAt?: number;
  source: PhaseSource;
  count?: number;
};

const ENABLED =
  import.meta.env.DEV &&
  typeof window !== "undefined" &&
  window.localStorage.getItem("snotra_perf") === "1";
const MAX_PENDING = 256;
const MAX_SAMPLES = 500;
const REPORT_INTERVAL = 20;

let pendingInputAt: number | undefined;
const pendingByRequest = new Map<number, Pending>();
const samples: Sample[] = [];
let completedCount = 0;

function now(): number {
  return performance.now();
}

function percentile(values: number[], p: number): number {
  if (values.length === 0) return 0;
  const sorted = [...values].sort((a, b) => a - b);
  const idx = Math.min(sorted.length - 1, Math.max(0, Math.ceil((p / 100) * sorted.length) - 1));
  return sorted[idx];
}

function round1(v: number): number {
  return Math.round(v * 10) / 10;
}

function reportIfNeeded() {
  if (!ENABLED) return;
  if (completedCount % REPORT_INTERVAL !== 0) return;

  const inputToSearch = samples.map((s) => s.inputToSearch);
  const searchToRender = samples.map((s) => s.searchToRender);
  const inputToRender = samples.map((s) => s.inputToRender);

  const snapshot = {
    n: samples.length,
    input_to_search_p50_ms: round1(percentile(inputToSearch, 50)),
    input_to_search_p95_ms: round1(percentile(inputToSearch, 95)),
    search_to_render_p50_ms: round1(percentile(searchToRender, 50)),
    search_to_render_p95_ms: round1(percentile(searchToRender, 95)),
    input_to_render_p50_ms: round1(percentile(inputToRender, 50)),
    input_to_render_p95_ms: round1(percentile(inputToRender, 95)),
  };

  console.table(snapshot);
  (window as Window & { __snotraPerfSnapshot?: typeof snapshot }).__snotraPerfSnapshot = snapshot;
}

export function perfMarkInput() {
  if (!ENABLED) return;
  pendingInputAt = now();
}

export function perfStartSearch(requestId: number, source: PhaseSource) {
  if (!ENABLED) return;
  if (pendingByRequest.size >= MAX_PENDING) {
    pendingByRequest.clear();
  }
  const inputAt = pendingInputAt;
  pendingInputAt = undefined;
  if (inputAt === undefined) {
    return;
  }
  pendingByRequest.set(requestId, {
    inputAt,
    searchStartAt: now(),
    source,
  });
}

export function perfMarkSearchDone(requestId: number, count: number) {
  if (!ENABLED) return;
  const p = pendingByRequest.get(requestId);
  if (!p) return;
  p.searchDoneAt = now();
  p.count = count;
}

export function perfCancelSearch(requestId: number) {
  if (!ENABLED) return;
  pendingByRequest.delete(requestId);
}

export function perfMarkRenderDone(requestId: number) {
  if (!ENABLED) return;
  const p = pendingByRequest.get(requestId);
  if (!p || p.searchDoneAt === undefined) {
    pendingByRequest.delete(requestId);
    return;
  }
  if (p.source !== "query") {
    pendingByRequest.delete(requestId);
    return;
  }

  const renderDoneAt = now();
  const sample: Sample = {
    inputToSearch: p.searchDoneAt - p.inputAt,
    searchToRender: renderDoneAt - p.searchDoneAt,
    inputToRender: renderDoneAt - p.inputAt,
  };
  samples.push(sample);
  if (samples.length > MAX_SAMPLES) {
    samples.splice(0, samples.length - MAX_SAMPLES);
  }
  completedCount += 1;
  reportIfNeeded();
  pendingByRequest.delete(requestId);
}
