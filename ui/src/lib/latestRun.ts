/** latest-wins（supersede）調停 primitive。
 *
 *  「最新の実行だけが結果を適用し、古い実行は破棄される」を内包する小さな runner。
 *  world 世代カウンタを 1 箇所（この runner 内）で所有し、`run()` は最新 lane タスクを開始、
 *  `invalidate()` は非 lane コード（モード遷移・起動）が world を進めて in-flight を supersede する。
 *
 *  **flush（in-flight 待受）はこの primitive の関心ではない** — 「最新 refresh の完了を activation 前に
 *  待つ」という refresh lane 固有の追跡（search.ts の `refreshInFlight`/`flushPendingRefresh`）まで
 *  吸収すると、instant fetch や別 lane まで待受対象に載り挙動が変わるため、ここは staleness/世代のみを担う。 */

export interface LatestRunContext {
  /** await 後、この task の token が最新でなくなっていれば true（stale として適用スキップ）。 */
  isStale: () => boolean;
  /** この run に割り当てられた world 世代番号（perf 計測の requestId 源）。 */
  requestId: number;
}

export interface LatestRun {
  /** 最新となる lane タスクを開始する。内部で世代を進め、task に isStale/requestId を渡し、
   *  task の戻り promise をそのまま返す（in-flight 追跡はしない）。
   *  task が同期 throw しても `run()` 自体は throw せず reject 済み promise を返す。 */
  run: <T>(task: (ctx: LatestRunContext) => Promise<T>) => Promise<T>;
  /** world 世代を進め、in-flight タスクを supersede する（非 lane コード用）。
   *  世代の読み取りが要る呼び出し側（perf・baseline 比較）は `current()` を使う。 */
  invalidate: () => void;
  /** 現在の world 世代番号。 */
  current: () => number;
}

export function createLatestRun(): LatestRun {
  // world 世代カウンタ。`run()` と `invalidate()` だけが書き換える唯一経路（+1 単調前進）。
  let generation = 0;

  const run = <T>(task: (ctx: LatestRunContext) => Promise<T>): Promise<T> => {
    const requestId = ++generation;
    const isStale = () => requestId !== generation;
    // task は同期起動する（bump 直後・最初の await 前に本体が走ることで、呼び出し側が
    // 同期に読むシグナルのキャプチャタイミングを保つ）。同期 throw は reject へ正規化する。
    try {
      return task({ isStale, requestId });
    } catch (e) {
      return Promise.reject(e);
    }
  };

  return {
    run,
    invalidate: () => {
      ++generation;
    },
    current: () => generation,
  };
}
