# plan: #536 検索/instant の debounce を所有 Debouncer primitive に統合する

## ゴール

生 `setTimeout`/`clearTimeout` + フラグ（`debounceTimer`/`leadingFired`/`instantCmdDebounceTimer`）を、`lib/debouncer.ts` の `createDebouncer({ ms, leading })` primitive に隠蔽する。検索（leading+trailing 50ms）と instant（trailing 30ms）を**同一 primitive の 2 インスタンス**にする。公開 API・挙動は不変。

## Debouncer API（確定シグネチャ）

```ts
export interface Debouncer {
  /** leading（設定時は区間最初の呼び出しで fn を同期発火）+ trailing（最後の呼び出しから ms 後に fn）。 */
  schedule(fn: () => void): void;
  /** 保留中の trailing タイマーを破棄し、leading 状態もリセットする。 */
  cancel(): void;
  /** trailing タイマーが保留中か（flush 経路が「取りこぼし防止で即 run」を判定するために問う）。 */
  isPending(): boolean;
  /** cancel + 以後 schedule を no-op 化する（将来の per-component 流用時の teardown 用）。 */
  dispose(): void;
}
export function createDebouncer(opts: { ms: number; leading: boolean }): Debouncer;
```

- 内部状態: `timer`（`ReturnType<typeof setTimeout> | undefined`）, `leadingFired: boolean`, `disposed: boolean`。closure が唯一の書き換え経路（`latestRun`/`exclusive` と同作法）。
- `schedule(fn)`: `disposed` なら no-op。`opts.leading && !leadingFired` なら `leadingFired = true; fn()`（**同期発火**）。既存 timer を clear し、`setTimeout(() => { timer = undefined; leadingFired = false; fn(); }, opts.ms)` を再セット。
- `cancel()`: timer を clear し `undefined` に、`leadingFired = false`。
- `isPending()`: `timer !== undefined`。
- `dispose()`: `disposed = true; cancel()`。

**現 2 実装との等価性**: `leading:true, ms:50` は現 `debouncedRefresh`/`cancelDebounce` と字面一致。`leading:false, ms:30` は現 `scheduleInstantCommandFetch` の timer 部と一致（`instantCommandItems = []` の副作用は呼び出し側に残すため primitive 外）。

## 変更ファイル一覧

1. **`ui/src/lib/debouncer.ts`（新規）**: 上記 `createDebouncer` + `Debouncer` 型。JSDoc に leading 同期発火の理由・dispose の用途・「flush は含まない（isPending で呼び出し側が判断）」を注釈。
2. **`ui/src/lib/debouncer.test.ts`（新規）**: 単体テスト（下記テスト方針）。
3. **`ui/src/stores/search.ts`**:
   - `const searchDebounce = createDebouncer({ ms: DEBOUNCE_MS, leading: true });` を追加（`DEBOUNCE_MS = 50` は残す or インライン。残す方が可読）。
   - `debounceTimer` / `leadingFired` 削除。
   - `cancelDebounce()` 関数削除 → **`.cancel()` 置換は計 7 箇所**: 直接呼び出し 6（L263, 289, 298, 306, 413, 555）+ `flushPendingRefresh` 内 1（L457、下記）。plan-review 実測で L457 の見落とし防止のため明記。
   - `debouncedRefresh()` を薄いラッパに: `function debouncedRefresh() { searchDebounce.schedule(() => void runRefresh()); }`（2 呼び出し L318/L375 は無変更。closure `() => void runRefresh()` を 1 箇所に保つ DRY）。
   - `flushPendingRefresh()`: `if (debounceTimer !== undefined)` → `if (searchDebounce.isPending())`、内部の `cancelDebounce()` → `searchDebounce.cancel()`。
   - import に `createDebouncer` を追加。
4. **`ui/src/stores/instantCommand.ts`**:
   - `const instantDebounce = createDebouncer({ ms: INSTANT_CMD_DEBOUNCE_MS, leading: false });`（`INSTANT_CMD_DEBOUNCE_MS = 30` 残す）。
   - `instantCmdDebounceTimer` 削除。
   - `hasPendingInstantCommandFetch()` → `return instantDebounce.isPending();`
   - `cancelInstantCommandDebounce()` → `instantDebounce.cancel();`
   - `scheduleInstantCommandFetch()`: `instantCommandItems = [];`（副作用は残す）→ `instantDebounce.schedule(() => { void deps.run(async ({ isStale }) => { ... }); });`（旧 `cancelInstantCommandDebounce()` + `setTimeout` は schedule 内の clear-and-reset が担うため不要）。
   - import に `createDebouncer` を追加。
5. **`ui/CLAUDE.md`**: lib/ に `debouncer.ts` 追記。L108（検索 debounce 説明）・L29（`scheduleInstantCommandFetch` 説明）を primitive 経由に更新。
6. **`.claude/rules/ui.md`**: 「モード遷移時にデバウンスをキャンセル」を「所有 Debouncer の `cancel()` を呼ぶ」表現へ更新。
7. **`docs/architecture.md`**: L186 の mermaid 注記 `debouncedRefresh()<br/>setTimeout で leading+trailing 50ms` を primitive 経由の表現へ更新（`setTimeout` が primitive 内へ移るため実装記述がドリフト。挙動ラベル「leading+trailing 50ms」は保持）。L143 の `scheduleInstantCommandFetch（30ms デバウンス）` は関数名・挙動不変ゆえ**無変更**。

### 編集しないが 50ms trailing タイミングの回帰網（挙動不変を守る証跡）

- `PERFORMANCE.md:6`「入力デバウンスは leading edge（初回即時発火）+ trailing 50ms」— 観測挙動の記述。本 refactor で不変ゆえ無変更。
- `e2e/tauri.slash.e2e.ts:598-603` — 「trailing リフレッシュ（+50ms）の危険ゾーン」を前提にした stale 要素対策コメント。50ms trailing タイミングを変えると前提が崩れる。**タイミングは厳密維持**（無変更）。

## 実装順序（フェーズ分け）

- **Phase 1**: `debouncer.ts` + `debouncer.test.ts` を作り、単体テスト緑（TDD: Red→Green）。ここで primitive の契約を確定。
- **Phase 2**: `search.ts` を載せ替え。`npm run typecheck` + `search.test.ts` 緑（既存 debounce 挙動の回帰検出）。
- **Phase 3**: `instantCommand.ts` を載せ替え。`search.test.ts` の instant モードテスト緑。
- **Phase 4**: ドキュメント（`ui/CLAUDE.md` / `.claude/rules/ui.md`）更新。
- 各 Phase 完了時（検証緑）にコミット可能な粒度にする。

## 不変条件

1. **leading 同期発火**: `searchDebounce.schedule` の leading 分岐は `fn()` を同期呼び出しする。非同期化（`queueMicrotask` 等）すると `refreshResults` の `query()` 同期読みが崩れる。→ 単体テストで「schedule 直後（await 前）に leading fn が呼ばれている」をアサート。
2. **trailing 発火後の leading リセット**: trailing コールバックで `leadingFired = false` に戻す。戻さないと次の区間で leading が発火しない（体感速度劣化）。
3. **cancel の完全性**: `cancel()` は timer 破棄 **かつ** `leadingFired` リセット。片方だけだと「タイマーは消えたが leading が発火済みのまま」＝次入力で leading が飛ぶ。
4. **isPending の意味**: `timer !== undefined` のみ。leading が発火済みでも trailing timer が生きていれば pending=true（現 `flushPendingRefresh` の判定と一致）。
5. **dispose 後 no-op**: `dispose()` 後の `schedule` は timer を作らない。異常な二重 dispose でも安全（`cancel` は冪等）。
6. **副作用の分離**: instant の `instantCommandItems = []` は `scheduleInstantCommandFetch` に残す。primitive に混ぜない。
7. **公開 API 不変**: `hasPendingInstantCommandFetch`/`cancelInstantCommandDebounce`/`scheduleInstantCommandFetch`/`debouncedRefresh`（内部）/`refreshResults`/`resetForShow` 等のシグネチャ・export・呼び出し側は不変。
8. **単一インスタンス共有**: 検索側は `debouncedRefresh` の 2 呼び出し（`handlePlainQueryInput` L318 / folderFilter effect L375）が現状**同一の** `debounceTimer`/`leadingFired` ペアを共有する。載せ替え後も**単一の `searchDebounce` インスタンス**を両者で共有すること（2 インスタンスに割ると leading/trailing 状態が分裂し、plain 打鍵とフィルタ入力が互いの leading を消す挙動変化になる）。

### 失敗・異常順序時の振る舞い

- primitive は Win32 フック・IPC・プロセスのような「戻ってこない」系リソースを持たない（純粋な JS タイマーのみ）。最悪ケースは「タイマーの取りこぼし＝余分な/欠けた refresh」で、検知は単体テスト + `search.test.ts`。回復不能な状態固着は起きない（`cancel`/`dispose` はいつでも安全に呼べる冪等操作）。
- `fn()` が throw した場合: leading の同期 throw は `schedule` の呼び出し元へ伝播しうる。現 `debouncedRefresh` は `void runRefresh()`（runRefresh は内部で `.catch`）を渡すため throw しない。instant も `void deps.run(...)`（run は reject を promise 化）で throw しない。→ primitive は fn の throw を握り潰さない（呼び出し側が安全な fn を渡す責務。現状の呼び出し側は両方安全）。この前提を JSDoc に明記。

## テスト方針

### `debouncer.test.ts`（新規・主装置）

`vi.useFakeTimers()` 前提。`describe("createDebouncer")`:

- **leading:true** — `schedule` 直後（`advanceTimers` 前）に fn が 1 回呼ばれる（leading 即時）。
- **leading:true** — leading 後さらに ms 経過で trailing がもう 1 回（計 2 回）。
- **leading:false** — `schedule` 直後は fn 未呼び出し、ms 経過で初めて 1 回（trailing のみ）。
- **連続 schedule** — burst 中は trailing タイマーがリセットされ、最後の呼び出しから ms 後に 1 回だけ trailing。
- **cancel** — schedule 後 cancel すると ms 経過しても trailing 不発。cancel 後の再 schedule で leading が再度発火する（`leadingFired` リセット確認）。
- **isPending** — schedule 後 true、trailing 発火後 false、cancel 後 false。
- **dispose** — dispose 後 schedule は no-op（fn 不呼び出し・isPending false）。dispose は保留タイマーも破棄する。
- **同期発火** — `let sync=false; d.schedule(()=>{sync=true}); expect(sync).toBe(true)`（leading:true、await/advance なし）。

### 既存テスト（回帰検出＝統合の測定）

- `search.test.ts` 全緑を維持（instant モード・executeInstantCommandSelected・flush 経路を含む）。**1 つでも赤なら統合が挙動を変えた証拠** → primitive 設定の見直し or 分離維持へ差し戻す。
- 既存テストが検索 leading+trailing を**直接**ピン留めしていないため、`debouncer.test.ts` がその不変条件の証明責任を負う（AGENTS.md「改名・転用で失われる不変条件を孤立させない」— ここでは新設だが同趣旨で primitive 側に明示的テストを置く）。

### 検証コマンド

- `npm run typecheck`（PostToolUse hook 自動発火・沈黙=合格）
- `npx vitest run ui/src/lib/debouncer.test.ts ui/src/stores/search.test.ts`（Phase 2/3）
- 全体: `npx vitest run`（最終）

## SPEC.md 更新要否

**不要**（挙動・IPC 契約・状態遷移すべて不変。研究で grep 実測済み）。

## セルフレビュー

### 5a. check スキル結果

**`/plan-review`**（Explore 監査 + Plan 独立導出の 2 体並列）: 要対処なし。independent derivation が計画の分解を**枠組みごと再一致**（盲点なしの能動的証拠）。反映済みの指摘:
- `cancelDebounce()` の `.cancel()` 置換は計 7（直接 6 + flush 1）と明記（不変条件・変更ファイル一覧を修正）。
- `docs/architecture.md:186` の mermaid 注記を doc-update に追加。`PERFORMANCE.md:6` / `e2e:598-603` は 50ms trailing の回帰網として無変更を明記。
- 単一 `searchDebounce` インスタンス共有を不変条件#8 に追加。
- `dispose()` は issue 契約由来（擬似シグネチャ・受け入れ条件）ゆえ YAGNI 違反ではなく許容。store 側呼び出し元は無い旨を透明に記録。

**`/symmetric-check`**: 対称ペア見落としなし。schedule↔cancel / create↔dispose は既存対応の 1:1 保存。timer の全 setTimeout に clearTimeout 対あり（orchan 無し）。instant の `instantCommandItems=[]` と timer cancel の**意図的分離**を保つ（不変条件#6）。

**`/race-check`**: 全 await 地点で新規レースなし。primitive は同期（await 皆無）で、async 面（`runRefresh`/`deps.run` の staleness）は無変更。唯一の要点＝leading 同期発火は不変条件#1 + 単体テストで固定。

### 5b. チェックリスト

1. **対称コードパス**: 済（/symmetric-check）。schedule/cancel・create/dispose・7 cancel 載せ替えを検証。
2. **影響範囲の網羅性**: 済。全識別子を grep（研究 + plan-review 独立導出の 2 経路で一致）。`flushPendingRefresh` の timer 直読（唯一の間接参照）を捕捉。
3. **境界条件**: debouncer.test.ts で leading:true/false × trailing/cancel/dispose/isPending/同期発火/burst を網羅。
4. **リソース管理**: timer の生成/破棄ペアを primitive 内に閉包。dispose 後 no-op。singleton ゆえ store 側 teardown 不要（`latestRun`/`exclusive` と同）。
5. **既存パターンとの整合**: `lib/latestRun.ts`/`lib/exclusive.ts` の純粋ファクトリ作法に準拠。新規パターンなし。
6. **YAGNI 違反**: `dispose()` のみが「未使用の将来 API」。issue 契約が明示要求 + テスト方針で担保するため許容（姉妹 primitive の最小 API 慣行を 1 つだけ超えるが issue 由来）。
7. **シンプル化の挑戦**: 2 実装を 1 primitive × 2 config（leading + ms）に統合するのは issue の明示ゴール。過剰な汎用化ではない（差異は leading/ms の 2 パラメータのみで、構造的等価が測定で裏付く）。「操作が失敗したら」= JS タイマーのみで回復不能状態に固まらない（不変条件「失敗・異常順序時の振る舞い」）。
8. **破壊不変条件**: primitive は Win32 フック・ホットキー・IPC のような「戻ってこない」系リソースを持たない。最悪ケース＝余分/欠けた refresh で、検知は debouncer.test.ts + search.test.ts + e2e の 50ms タイミング回帰網。

### 総評

計画の completeness: **高**（2 独立経路 + 3 チェックで一致、盲点候補は全て解消または透明に記録）。実装着手可否: **可**。
