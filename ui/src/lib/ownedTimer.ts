/** 所有されたワンショットタイマー。
 *
 *  `setTimeout`/`clearTimeout` ハンドルを 1 closure が唯一所有し、arm↔cancel のペアリングを
 *  構造で保証する（`.claude/rules/src-tauri.md`「状態を作ったら破棄経路とセットで設計する」の
 *  JS 版を primitive の構造で担保）。検索/データ lane の `createLatestRun`・起動 lane の
 *  `createExclusive` と同じ「1 resource / 1 owner」の純粋ファクトリ family（SolidJS/api 非依存・
 *  fake timer で単体テスト可能）。
 *
 *  **timer resource だけを所有し、debounce policy（leading edge 等）は持たない** — leading は
 *  「区間最初の入力で即時発火する」という呼び出し側の関心事であり、timer の所有とは別レイヤー。
 *  検索は `!isPending()`（＝バースト先頭）から leading を導出して自分で発火する（`stores/search.ts`）。
 *  instant 候補取得に leading は無く、`arm` のみを使う（`stores/instantCommand.ts`）。
 *
 *  **`arm` の `msOverride` は呼び出し単位で ms を差し替える resource 属性であり、leading 等の
 *  policy ではない**（`launchNotice.ts` の自動クリア遅延が呼び出し元ごとに可変なため導入。
 *  `createOwnedTimer(ms)` の `ms` は「override が無いときの既定値」になる）。
 *
 *  **`arm` は fn を `setTimeout` の中（別マクロタスク）でしか呼ばない＝同期発火しない**。ゆえに `arm()`
 *  実行中への同期再入は起こらず、保持し続けるロック状態も無い。fn が（別マクロタスクで）`arm`/`cancel`
 *  を呼ぶこと自体は安全で（発火時は `timer` を先に `undefined` 化してから fn を呼ぶため、fn 内の再 arm は
 *  新しいタイマーとして正しく生きる）、「再入禁止」を契約で断る必要がない（`createExclusive` の非再入
 *  契約は mutex 自己待ちという解けない問題への対処であり、同期発火しない本タイマーには借用しない）。 */
export interface OwnedTimer {
  /** 保留中タイマーを破棄し、`msOverride ?? ms`（`ms` は生成時の既定値）後に `fn` を 1 回だけ
   *  呼ぶタイマーを張り直す。`fn` は必ず `setTimeout` の中で呼ばれる（同期発火しない）。`fn` の
   *  throw は setTimeout の callback 境界で失われ `arm` の呼び出し元へは伝播しない — 呼び出し側は
   *  throw しない `fn`（例: `() => void runRefresh()`。`runRefresh` は内部で `.catch` 済み）を
   *  渡す責務を持つ。 */
  arm(fn: () => void, msOverride?: number): void;
  /** 保留中タイマーを破棄する（冪等・二重 cancel 安全）。teardown も兼ねる。 */
  cancel(): void;
  /** タイマーが保留中か。flush 経路の「保留なら cancel して即 run」判定
   *  （`flushPendingRefresh`）や instant の pending 観測（`hasPendingInstantCommandFetch`）に使う。 */
  isPending(): boolean;
}

export function createOwnedTimer(ms: number): OwnedTimer {
  // タイマーハンドル。この closure だけが書き換える唯一経路（arm で set、cancel と発火で undefined）。
  let timer: ReturnType<typeof setTimeout> | undefined;

  return {
    arm(fn, msOverride) {
      if (timer !== undefined) clearTimeout(timer);
      timer = setTimeout(() => {
        // fn の前に undefined 化する: fn 実行中の isPending() を false にし、
        // flush 経路（保留なら cancel して即 run）の二重発火を防ぐ。
        timer = undefined;
        fn();
      }, msOverride ?? ms);
    },
    cancel() {
      if (timer !== undefined) {
        clearTimeout(timer);
        timer = undefined;
      }
    },
    isPending() {
      return timer !== undefined;
    },
  };
}
