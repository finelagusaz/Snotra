/** mutex / single-flight 調停 primitive。
 *
 *  「実行中なら拒否（`undefined` を返す）、完了時に必ず解放する」を内包する小さな runner。
 *  in-flight フラグを 1 箇所（この closure 内）で所有し、`try/finally` で成功・失敗・throw の
 *  いずれでも解放を保証する（`.claude/rules/src-tauri.md`「状態フラグを true にしたら false に戻す
 *  経路とセットで設計する」の JS 版を primitive の構造で担保）。
 *
 *  検索/データ lane の supersede（`createLatestRun`）と対をなす起動（launch/activate）lane 用（#535）。
 *  supersede は「新しい実行が古い実行を無効化する」、mutex は「実行中は 2 つ目を拒否する」——
 *  2 つの並行方針を別名 primitive として明示する。
 *
 *  **task は同期起動する**（`await task()` は `task()` を同期呼び出ししてから await するため、task の
 *  同期プレフィックス——例: `executeInstantCommandSelected` の保存状態（savedResults/savedSelected/
 *  savedItems）捕捉・`selected()` 読み——が `Exclusive` 呼び出しと同一同期実行内で走る）。この同期起動を崩す（`Promise.resolve().then(task)` 等で
 *  遅延させる）と、呼び出し側が同期に読むシグナルのキャプチャタイミングが崩れる。
 *
 *  **再入は許可しない**（トークン/深度カウンタを持たない）。入れ子で起動系が呼ばれる経路は、mutex に
 *  入る前に分岐を確定させる「呼び出し順」で自己ブロックを回避する（呼び出し側の責務。search.ts の
 *  `tryModalActivate` を `activationLane(...)` の前に置く構造がこれに当たる）。 */
export type Exclusive = <T>(task: () => Promise<T>) => Promise<T | undefined>;

export function createExclusive(): Exclusive {
  // in-flight フラグ。この closure だけが書き換える唯一経路。
  // 判定（`if (inFlight)`）と set（`inFlight = true`）の間に await が無く、JS 単一スレッドで
  // 不可分に走る（TOCTOU race なし）。
  let inFlight = false;

  return async <T>(task: () => Promise<T>): Promise<T | undefined> => {
    if (inFlight) return undefined; // 実行中: task を起動せず拒否
    inFlight = true;
    try {
      return await task(); // task の同期プレフィックスは呼び出し tick で走る
    } finally {
      inFlight = false; // 成功・失敗・throw いずれでも解放
    }
  };
}
