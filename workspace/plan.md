# plan: #536 検索/instant の生タイマーを所有 OwnedTimer primitive に統合する

> **設計転換の記録**: 当初計画は `createDebouncer({ ms, leading })`（leading policy を primitive に内包）だった。多視点の設計レビュー（canonical / minimalist / FSM の 3 レンズ + codex 敵対レビュー）の結果、**Design C（OwnedTimer）**を採用。issue の真の痛点は「所有者の曖昧な生**タイマー**（resource）」であって leading という **policy** ではない。ゆえに primitive は timer resource だけを所有し、leading は search.ts の policy として残す。経緯は本ファイル末尾「設計探索の要約」参照。issue #536 も本設計へ改訂済み。

## ゴール

散在する生タイマー（`debounceTimer` / `instantCmdDebounceTimer`）を、`lib/ownedTimer.ts` の `createOwnedTimer(ms)` primitive に隠蔽する。primitive は **timer resource の所有**（`arm`/`cancel`/`isPending`）だけを担い、leading edge / trailing / items クリアといった **policy は呼び出し側**に残す。検索（leading+trailing 50ms）と instant（trailing-only 30ms）を**同一 primitive の 2 インスタンス**にする。公開 API・挙動は不変。

## なぜ OwnedTimer か（createDebouncer を採らない理由）

- 痛点＝「leak しうる resource（setTimeout ハンドル）の所有者が曖昧」。`leadingFired` は leak しない policy latch であって痛点ではない。→ primitive は timer だけ所有すれば痛点を解く。
- **再入問題が構造的に消える**: `arm` は fn を `setTimeout` の中（別マクロタスク）でしか呼ばない＝**同期発火しない**。ゆえに fn からの再入は起こり得ず、「再入禁止」を JSDoc 契約で断る必要がない（＝ *illegal state unrepresentable* > *documented-forbidden*）。`exclusive.ts` の非再入契約は「mutex 自己待ち」という解けない問題への対処であり、debounce の再スケジュール（解ける問題）に借用すべきでない（codex 指摘）。
- **`leadingFired` が不要**: 現行実装で `leadingFired ≡ (timer === undefined)`（バースト先頭か）が全遷移で成立（検算済み）。leading は search.ts で `!refreshTimer.isPending()` から導出でき、フラグは primitive にも search.ts にも要らない。
- **dead code なし**: `dispose()` は現 caller ゼロ（当初計画自身が「将来用」と明記）。OwnedTimer は `cancel()` が teardown を兼ね、dispose を持たない。将来 per-component 流用時に必要なら約4行で足せる（かつスコープ外タイマー focus retry/blur/move は**すべて trailing-only** ＝ OwnedTimer に適合）。
- **doctrine 整合**: `latestRun`（世代1個）/ `exclusive`（フラグ1個）と同じ「1 resource / 1 owner」。`createDebouncer` は timer + leadingFired + disposed + leading policy の 4 つを抱え姉妹より重い。

## OwnedTimer API（確定シグネチャ）

```ts
export interface OwnedTimer {
  /** 保留中タイマーを破棄し、ms 後に fn を 1 回呼ぶタイマーを張り直す。
   *  fn は必ず setTimeout の中（別マクロタスク）で呼ばれる＝同期発火しない。 */
  arm(fn: () => void): void;
  /** 保留中タイマーを破棄する（冪等）。teardown も兼ねる。 */
  cancel(): void;
  /** タイマーが保留中か（flush 経路の「取りこぼし防止で即 run」判定・instant の pending 観測に使う）。 */
  isPending(): boolean;
}
export function createOwnedTimer(ms: number): OwnedTimer;
```

- 内部状態: `timer`（`ReturnType<typeof setTimeout> | undefined`）のみ。closure が唯一の書き換え経路（`latestRun`/`exclusive` と同作法）。
- `arm(fn)`: 既存 timer を clear し、`setTimeout(() => { timer = undefined; fn(); }, ms)` を張り直す。**`timer = undefined` を `fn()` の前**に置く（fn 実行中の `isPending()` を false にし、flush の二重発火を防ぐ）。
- `cancel()`: timer を clear し `undefined` に（冪等・二重 cancel 安全）。
- `isPending()`: `timer !== undefined`。

## 変更ファイル一覧

1. **`ui/src/lib/ownedTimer.ts`（新規）**: `createOwnedTimer` + `OwnedTimer` 型。JSDoc に「resource だけ所有・policy は持たない」「fn は同期発火しない＝再入安全」「timer を fn 前に undefined 化する理由」を注釈。
2. **`ui/src/lib/ownedTimer.test.ts`（新規）**: 単体テスト（下記テスト方針）。
3. **`ui/src/stores/search.ts`**:
   - `const refreshTimer = createOwnedTimer(DEBOUNCE_MS);` を追加（`DEBOUNCE_MS = 50` 残す）。
   - `debounceTimer` / `leadingFired` 削除。
   - `cancelDebounce()` は**名前と 6+1 呼び出し元を維持**し、本体を `refreshTimer.cancel();` の 1 行に（`leadingFired` リセットは消える）。call site（L263, 289, 298, 306, 413, 555 + flush 内 457）は**無変更**。
   - `debouncedRefresh()`: leading を `!isPending()` から導出。
     ```ts
     function debouncedRefresh() {
       // Leading edge: バースト先頭（保留タイマー無し）でのみ即時発火。
       if (!refreshTimer.isPending()) void runRefresh();
       // Trailing: 最後の入力から DEBOUNCE_MS 後。arm が前回の保留を破棄して張り直す。
       refreshTimer.arm(() => void runRefresh());
     }
     ```
     呼び出し（L318 / L375）は無変更。
   - `flushPendingRefresh()`: `if (debounceTimer !== undefined)` → `if (refreshTimer.isPending())`、内部 `cancelDebounce()` は維持（`refreshTimer.cancel()` へ委譲）。第2分岐 `if (refreshInFlight) await refreshInFlight;` は**無変更**（debounce 経路外の refresh 追跡）。
   - import に `createOwnedTimer` を追加。
4. **`ui/src/stores/instantCommand.ts`**:
   - `const fetchTimer = createOwnedTimer(INSTANT_CMD_DEBOUNCE_MS);`（`INSTANT_CMD_DEBOUNCE_MS = 30` 残す）。
   - `instantCmdDebounceTimer` 削除。
   - `hasPendingInstantCommandFetch()` → `return fetchTimer.isPending();`
   - `cancelInstantCommandDebounce()` → `fetchTimer.cancel();`
   - `scheduleInstantCommandFetch()`: `instantCommandItems = [];`（副作用は残す）→ `fetchTimer.arm(() => { void deps.run(async ({ isStale }) => { ... }); });`（旧 `cancelInstantCommandDebounce()` + `setTimeout` は arm 内の clear-and-reset が担う。instant に leading は無い＝arm のみ）。
   - import に `createOwnedTimer` を追加。
5. **`ui/CLAUDE.md`**: lib/ に `ownedTimer.ts` 追記。L108（検索 debounce 説明）を「OwnedTimer + search の leading policy」へ、L29（`scheduleInstantCommandFetch` 説明）を OwnedTimer 経由へ更新。
6. **`.claude/rules/ui.md`**: 「モード遷移時にデバウンスをキャンセル」を「そのモードが所有する OwnedTimer の `cancel()` を呼ぶ」表現へ更新。
7. **`docs/architecture.md`**: L186 の mermaid 注記 `debouncedRefresh()<br/>setTimeout で leading+trailing 50ms` を「OwnedTimer(trailing 50ms) + search の leading」へ更新。L143 の `scheduleInstantCommandFetch（30ms デバウンス）` は関数名・挙動不変ゆえ**無変更**。

### 編集しないが 50ms trailing タイミングの回帰網（挙動不変を守る証跡）

- `PERFORMANCE.md:6`「入力デバウンスは leading edge（初回即時発火）+ trailing 50ms」— 観測挙動。不変ゆえ無変更。
- `e2e/tauri.slash.e2e.ts:598-603` — 「trailing リフレッシュ（+50ms）の危険ゾーン」前提の stale 対策コメント。50ms trailing を変えると崩れる。**厳密維持**（無変更）。

## 実装順序（フェーズ分け）

- **Phase 1**: `ownedTimer.ts` + `ownedTimer.test.ts`。単体テスト緑（TDD: Red→Green）。契約確定。
- **Phase 2**: `search.ts` を載せ替え。`npm run typecheck` + `search.test.ts` 緑 + adapter テスト追加。
- **Phase 3**: `instantCommand.ts` を載せ替え。`search.test.ts` の instant テスト緑。
- **Phase 4**: ドキュメント（`ui/CLAUDE.md` / `.claude/rules/ui.md` / `docs/architecture.md`）更新。
- 各 Phase 完了時（検証緑）にコミット可能な粒度。

## 不変条件

1. **arm は fn を同期発火しない**: fn は必ず `setTimeout` 内で呼ばれる。→ primitive への再入は構造的に不能（再入契約は不要）。single test で「arm 直後の同期時点で fn 未呼び出し」を固定。
2. **arm は `timer = undefined` を `fn()` の前**に置く: fn 実行中 `isPending()` は false（flush の二重発火防止・現 `debounceTimer` 挙動一致）。
3. **cancel は冪等**: timer 破棄のみ。二重 cancel 安全。teardown を兼ねる。
4. **leading policy は search.ts**: `!refreshTimer.isPending()`（バースト先頭）で導出。`leadingFired` フラグは primitive にも search.ts にも持たない。現行 `leadingFired ≡ (timer === undefined)` の等価性で挙動保存（検算済み）。leading の同期発火（`void runRefresh()`）は search.ts の自コードで、refreshTimer に再入しない。
5. **items クリア副作用の分離**: instant の `instantCommandItems = []` は `scheduleInstantCommandFetch` に残す。primitive（arm）に混ぜない。
6. **単一インスタンス共有**: `debouncedRefresh` の 2 呼び出し（plain L318 / folderFilter L375）は**単一 `refreshTimer`** を共有。理由: (a) 状態分裂で互いの leading を消さない、(b) モード遷移跨ぎの pending timer 保存（`enterFolderExpansion` は cancel しない＝plain trailing 保留中に folder 展開へ入ると保留 timer が生き残り folder refresh になる現挙動を保つ）。
7. **公開 API 不変**: `hasPendingInstantCommandFetch`/`cancelInstantCommandDebounce`/`scheduleInstantCommandFetch`/`refreshResults`/`resetForShow` のシグネチャ・export・呼び出し側は不変。`cancelDebounce`（内部）も名前維持。

### 失敗・異常順序時の振る舞い

- primitive は Win32 フック・IPC・プロセスのような「戻ってこない」系リソースを持たず、純粋な JS タイマーのみ。最悪ケースは「余分/欠けた refresh」で、検知は `ownedTimer.test.ts` + `search.test.ts` + adapter テスト。回復不能な状態固着は起きない（`cancel` はいつでも安全な冪等操作）。
- `fn()` が throw した場合: `arm` の fn は setTimeout callback 内で呼ばれるため、throw は callback 境界で失われ `arm` 呼び出し元へ伝播しない。現行の呼び出し側 `() => void runRefresh()`（runRefresh は内部 `.catch`）/ `() => void deps.run(...)` はいずれも throw しない。leading の `void runRefresh()` も同様。この前提を JSDoc に明記。

## テスト方針

### `ownedTimer.test.ts`（新規・primitive 単体）

`vi.useFakeTimers()` 前提。`describe("createOwnedTimer")`:

- **arm → ms 後に fn 1 回**（trailing）。
- **burst（連続 arm）→ 最後の 1 回だけ**（前回タイマー破棄）。
- **cancel → ms 経過しても不発**。二重 cancel が安全。
- **isPending**: 初期 false → arm 後 true → 発火後 false → cancel 後 false。
- **同期発火しない**: `let ran=false; t.arm(()=>ran=true); expect(ran).toBe(false)`（advance 前）。＝再入安全の構造的証跡。
- leading / dispose / 再入テストは**不要**（primitive は policy を持たず、fn は同期発火せず、dispose は存在しない）。

### adapter テスト（等価性の**直接**測定・codex P1）

primitive 単体 + `search.test.ts` 緑は必要条件だが十分条件ではない。等価性を **store 越しに**固定するテストを `search.test.ts`（instant は同ファイル）へ追加:

1. **search leading 即時**: query 変更直後（advance 前）に `api.search` が呼ばれる。
2. **<50ms burst → trailing 1 回・最後の query**。
3. **flush 取りこぼし防止**: leading 後・trailing 保留中の activation で refresh が 1 回だけ。
4. **instant: items クリアが arm より前**（`scheduleInstantCommandFetch` 直後に `getInstantCommandItems()` 空）。
5. **instant: 連続 arm で古い filterName 非実行**（`getInstantCommands` が最後の filterName で 1 回）。
6. **cancel 直後の再入力で境界維持**（leading 再発火・30/50ms 保持）。

（既存 `search.test.ts:898` の flush スコープ・instant IPC in-flight が 3/4 の一部を既カバー。重複回避しつつ未カバー分を足す。）

### 既存テスト（回帰検出・片方向の証拠）

- `search.test.ts` 全緑を維持。**1 つでも赤なら挙動変更の十分証拠**。緑は等価の十分条件ではない（→ adapter テストで能動固定）。

## issue / SPEC 更新

- **issue #536**: 本設計（OwnedTimer）へ改訂済み。
- **SPEC.md**: 更新不要（挙動・IPC 契約・状態遷移すべて不変。grep 実測済み）。

## セルフレビュー

### 5a. check スキル結果（当初 createDebouncer 計画に対して実施・大半が C へ継承）

- **`/plan-review`**（監査 + 独立導出）: 影響範囲・不変条件・スコープを検証、要対処なし。7 箇所の cancel 載せ替え・`docs/architecture.md:186` の doc ドリフト・単一インスタンス共有・SPEC 更新不要を確認。→ C では cancel 呼び出し元が**無変更**（本体のみ差替）になり影響がさらに局所化。
- **`/symmetric-check`**: arm↔cancel の対称・全 setTimeout に clearTimeout 対あり。instant の items クリアと timer cancel の意図的分離を保持。
- **`/race-check`**: primitive は同期（arm/cancel/isPending に await なし）。async 面（runRefresh/deps.run の staleness）は無変更。**C では leading の同期発火が primitive 外（search.ts）に移り、primitive が完全に再入安全**になったため race 面はさらに縮小。

### 5b. チェックリスト

1. 対称コードパス: arm/cancel を /symmetric-check で検証。
2. 影響範囲: 全識別子を grep（研究 + 独立導出の 2 経路で一致）。`flushPendingRefresh` の timer 直読（唯一の間接参照）を捕捉。
3. 境界条件: ownedTimer.test.ts で arm/burst/cancel/isPending/同期非発火、adapter で store 越し挙動。
4. リソース管理: timer の arm/cancel ペアを primitive 内に閉包。singleton ゆえ teardown 不要（cancel が兼務）。
5. 既存パターン整合: `latestRun`/`exclusive` の「1 resource/1 owner」純粋ファクトリに準拠。
6. YAGNI: dispose・leading param・leadingFired・再入契約をすべて削除（当初計画から純減）。
7. シンプル化: resource（timer）と policy（leading）を分離。偶発的複雑さ（同期発火順序・再入契約・未使用 dispose）を表現不能化。
8. 破壊不変条件: primitive は「戻ってこない」系を持たず、最悪でも余分/欠けた refresh。検知は 3 層テスト + 50ms 回帰網。

### 5c. codex 敵対レビュー（2 回）

- 1 回目（createDebouncer 計画に対して）: 再入契約の穴・等価性の無条件主張・テスト片方向性・単一インスタンス根拠を指摘 → 計画へ反映。
- 2 回目（設計攻め）: 「leadingFired は冗長」「再入は順序で解ける（契約不要）」を独立導出。flush awaitable 案は `refreshInFlight` 第2分岐が消せないため退け、`isPending` 維持。→ これらが Design C 採用の決め手。

### 総評

計画の completeness: **高**（4 独立レンズ + 3 check + codex 2 回で収束）。Design C は当初計画から**不変条件を減らし・dead code を消し・再入問題を構造的に解消**した純改善。実装着手可否: **可**。

---

## 設計探索の要約（なぜ A→C に至ったか）

| レンズ | 結論 | C への寄与 |
|---|---|---|
| canonical/lodash | 現形が正。canonical は family 分裂 + flush が `refreshInFlight` を待てず退行 | `isPending` 維持・flush awaitable 却下の根拠 |
| minimalist | **OwnedTimer**（timer だけ所有・leading は policy） | **C 本体**。痛点=resource の再定義・dispose 削除・doctrine 整合 |
| FSM | FSM は過剰。ただし **leadingFired は冗長**（timer 由来） | leadingFired 廃止・不変条件 #2/#3 消滅 |
| codex 設計攻め | 再入は順序で解ける（契約不要）・leadingFired 冗長を独立再導出 | 再入契約の構造的消去 |

**収束点（独立に一致）**: ①`leadingFired` 冗長（FSM + codex）、②再入は文書でなく構造で消せる（minimalist + codex）。この 2 点が偶発的複雑さの正体で、C はそれを設計で除去する。
