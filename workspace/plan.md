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

**再入契約（codex P0・`exclusive.ts` の作法に倣う）**: `schedule` は leading fn を timer 設定の**前**に同期発火するため、fn が同期的に本 debouncer の `schedule`/`cancel`/`dispose` を**再入呼び出しすると契約が破れる**（例: `d.schedule(() => d.dispose())` は dispose 後も outer の timer が残り「dispose 後 no-op」に反する）。→ **JSDoc に「fn は本 debouncer のメソッドを同期再入してはならない」と明記**する（`exclusive.ts` L17-19「再入は許可しない」と同じ規範で断る。工学的な再入安全化は採らない＝YAGNI）。現行の呼び出し側 `() => void runRefresh()` / `() => void deps.run(...)` はいずれも searchDebounce/instantDebounce に触れず**再入しない**（コード確認済み）ため、この契約下で安全。

**現 2 実装との等価性（前提条件つき・codex P1）**: **callback が本 debouncer を同期再入せず、fn が同期 throw しない**という現行 store 利用の前提下で、`leading:true, ms:50` は現 `debouncedRefresh`/`cancelDebounce` と字面一致、`leading:false, ms:30` は現 `scheduleInstantCommandFetch` の timer 部と一致（`instantCommandItems = []` の副作用は呼び出し側に残すため primitive 外）。前提を外した「完全に同一」という無条件主張は誤り（AGENTS.md「全称表現は前提条件とセットで書く」）。

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
8. **単一インスタンス共有**: 検索側は `debouncedRefresh` の 2 呼び出し（`handlePlainQueryInput` L318 / folderFilter effect L375）が現状**同一の** `debounceTimer`/`leadingFired` ペアを共有する。載せ替え後も**単一の `searchDebounce` インスタンス**を両者で共有すること。理由は 2 つ（codex P2 で精緻化）: (a) leading/trailing 状態が 2 インスタンスに分裂すると plain 打鍵とフィルタ入力が互いの leading を消す、(b) **モード遷移をまたぐ pending timer の保存** — `enterFolderExpansion`（L389-406）は `cancelDebounce()` を**呼ばない**ため、plain の trailing 保留中に folder 展開へ入ると、その保留 timer は生き残り後で folder state 下の refresh になる（現挙動）。単一インスタンスがこの跨ぎ挙動を保存する。2 インスタンスに割るとこの pending timer が分断され挙動が変わる。

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

### adapter テスト（codex P1・等価性の**直接**測定）

primitive 単体テスト + `search.test.ts` 緑は**必要条件だが十分条件ではない**（既存テストは `runAllTimersAsync()` で一括 flush し leading/trailing を区別しない。「callback の回数」しか見ないテストは古い closure を trailing に使う実装でも緑になる）。等価性を **store 越しに直接測定**する adapter テストを `search.test.ts`（instant は同ファイルの instant describe）へ追加する:

1. **search leading 即時**: query 変更直後（`advanceTimersByTime` 前）に `api.search` が呼ばれている（leading edge で IPC が即開始）。
2. **<50ms burst → trailing 1 回・最後の query**: 連続 setQuery 後、50ms 経過で `api.search` が最後の query で 1 回だけ追加発火する。
3. **flushPendingRefresh の取りこぼし防止**: leading 発火後・trailing 保留中に activation（Enter）を起こすと、timer が消えて refresh が 1 回だけ走る（二重・欠落なし）。
4. **instant: items クリアが timer 設定より前**: `scheduleInstantCommandFetch` 呼び出し直後（timer 発火前）に `getInstantCommandItems()` が空（IPC 応答前 Enter で古いコマンド誤起動しない前提）。
5. **instant: 連続 schedule で古い filterName/deps 非実行**: 30ms 内に filterName を変えて再入力すると、`getInstantCommands` が最後の filterName で 1 回だけ呼ばれる。
6. **cancel 直後の再入力で境界維持**: cancel 後の入力で leading が再発火し、30/50ms の trailing 境界が保たれる。

（既存 `search.test.ts:898` の flush スコープ・instant IPC in-flight テストが 3/4 の一部を既にカバー。実装時に重複を避けつつ未カバー分を足す。）

### 既存テスト（回帰検出・**片方向**の証拠）

- `search.test.ts` 全緑を維持（instant モード・executeInstantCommandSelected・flush 経路を含む）。**1 つでも赤なら統合が挙動を変えた証拠**（十分条件）。ただし**緑は等価の十分条件ではない**（codex P1）——ゆえに上の adapter テストで必要挙動を能動的に固定する。
- 検索 leading+trailing の直接ピン留めは `debouncer.test.ts`（primitive の不変条件）+ 上記 adapter テスト（store 越しの実挙動）の二段で担保する。

### 再入テストは追加しない（codex P0 への判断）

codex は leading callback からの再入（cancel/dispose/再 schedule）テストを提案したが、**再入は契約で禁止**する方針（`exclusive.ts` と同）のため、未サポート挙動を測るテストは置かない（`exclusive.test.ts` も再入を測らない）。契約違反時の挙動は「未定義」として JSDoc に記す。

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

### 5c. codex 敵対的レビュー（反証志向）

`codex exec`（gpt-5.6-terra）に「同意でなく反証」を求めた。結論: 「非再入入力列では search/instant のタイマー挙動は現実装と等価。isPending 置換も単独では取りこぼし・二重 refresh を生まない」。ただし 3 点の実質的指摘を受諾・反映:

- **P0 再入契約**: leading fn を timer 設定前に同期発火するため、fn 内から `cancel`/`dispose`/再 `schedule` を呼ぶと契約が破れる。→ **非再入を JSDoc 契約で断る**（`exclusive.ts` の作法）。現行 callers は非再入（確認済み）ゆえ安全。工学的再入安全化は YAGNI で不採用。
- **P1 等価性は条件付き**: 「完全に同一」を「非再入 callback・同期 throw なしの前提下で等価」へ修正（research.md も）。AGENTS.md「全称表現は前提条件とセット」の自己適用。
- **P1 テストの片方向性**: 既存テスト緑は等価の必要条件だが十分条件でない。→ **adapter テスト 6 件**を追加（leading 即時 IPC・trailing 1 回・flush 取りこぼし防止・items 先クリア・古い filterName 非実行・cancel 後境界）。
- **P2 単一インスタンス根拠**: 「互いの leading を消す」に加え**モード遷移跨ぎの pending timer 保存**を根拠へ追加。
- **P3 dispose の YAGNI**: issue 一次資料（擬似シグネチャ・テスト方針・受け入れ条件）が `dispose`/`isPending`/単一 `createDebouncer({ms,leading})` を明示要求 → 保持が正、削除は要件逸脱（codex も但し書きで同意）。

### 総評

計画の completeness: **高**（2 独立経路 + 3 check スキル + codex 反証で一致、盲点候補は全て解消または透明に記録）。codex の指摘は「挙動の反例」ではなく「主張の過剰さ（無条件等価・テストの片方向性）と契約の穴（再入）」で、すべて計画の**表現・テスト・契約の精緻化**として吸収した。実装着手可否: **可**。
