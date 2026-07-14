# research.md — issue #534: `latestRun`(supersede) primitive への集約

## issue の要約

`ui/src/stores/search.ts` の非同期調停（「新しいクエリ/更新が古い結果を無効化する」= **latest-wins / supersede**）は、現在
`searchGeneration` カウンタ + 各 async 経路に散在する手書き `if (requestId !== searchGeneration) return` で実装されている。
これを **`latestRun`(supersede) primitive** に集約し、staleness 判定を runner 内部の 1 箇所へ寄せる。挙動は不変。公開 API（30 exports）も不変。

`search.ts` 抽象化プログラム全 5 件の **項番 1（基盤 primitive）**。項番 2〜5（#535〜#538）の前提。本 issue のスコープは検索/データ lane と instant lane の載せ替えのみ。

## 関連コード（現状の実測）

### `searchGeneration` の全読み書き（grep 実測）

| 行 | 種別 | 文脈 | 移行後 |
|---|---|---|---|
| `search.ts:63` | 宣言 `let searchGeneration = 0` | — | runner 内部の `generation` へ移動 |
| `search.ts:72` | 書込 `++searchGeneration`（`nextGeneration()` 本体） | choke point | runner の `run()`/`invalidate()` 内部 |
| `search.ts:183` | 読取 `requestId !== searchGeneration` | `refreshResults` slash_r_history 枝 | `isStale()` |
| `search.ts:232` | 読取 `requestId !== searchGeneration` | `refreshResults` post_api 枝 | `isStale()` |
| `search.ts:265` | 読取 `isStale: (id) => id !== searchGeneration` | instant hook 注入 | runner 内部へ吸収 |
| `search.ts:651` | 読取 `preGen = searchGeneration` | `executeInstantCommandSelected` ロールバック基準 | `preGen = searchLane.current()` |
| `search.ts:671` | 読取 `searchGeneration === preGen + 1` | 同上・「await 中に他変化なし」判定 | `searchLane.current() === preGen + 1` |
| `search.ts:799` | 読取 `return searchGeneration`（`getSearchGeneration()`） | perf requestId 源（ResultsSection が消費） | `return searchLane.current()` |

### `nextGeneration()` の全呼び出し（10 箇所・grep 実測）

**lane-start（token を捕捉して task を走らせる）= 2 箇所** → `searchLane.run()` へ:
- `search.ts:168` `refreshResults` の検索 lane 開始（`const requestId = nextGeneration()`）
- `instantCommand.ts:60` instant fetch の lane 開始（`hooks.nextRequestId()`、`handleInstantQueryInput` が `nextGeneration` を注入）

**pure invalidation（token を使わず world を進めて in-flight を無効化）= 9 箇所** → `searchLane.invalidate()` へ:
- `search.ts:131` `clearCommandModeState`（コマンド実行前クリア）
- `search.ts:300` `handleCommandQueryInput` slash_noop（候補なしクリア）
- `search.ts:407` `exitFolderExpansion`（フォルダ離脱・in-flight 検索を無効化）
- `search.ts:493` `withLaunchLifecycle`（起動前クリア。**ここが `preGen+1` の +1 に対応**）
- `search.ts:531` `launchWithSelectedTool` onSuccess（成功後の追加 bump）
- `search.ts:584` `enterToolSelection`（ツール一覧表示前）
- `search.ts:595` `exitToolSelection`（ツール離脱・復元前）
- `search.ts:614` `launchAndReset` onSuccess（成功後の追加 bump）
- `search.ts:672` `executeInstantCommandSelected` 失敗ロールバック（復元前 bump）

### 消費側（公開契約）

- `ResultsSection.tsx:5,137` `import { getSearchGeneration }`。`createEffect(on(results, ...))` 内で results 更新後に `getSearchGeneration()` を読み、`perfMarkRenderDone(perfRequestId)` へ渡す（**perf の requestId 相関**）。
- `perf.ts` `perfStartSearch(requestId, source)` / `perfMarkSearchDone(requestId, count)` / `perfCancelSearch(requestId)` は `refreshResults` から `requestId` を受け取る。**この requestId は world 世代の値そのもの**。runner 移行後は `run()` の task ctx に `requestId` を渡して供給する。
- `refreshResults` は **本番コードからは import されていない**（`SearchWindow.tsx` は import しない）。テスト（`search.test.ts`）の直接呼び出し + 内部 `runRefresh` 経由のみ。→ export は維持（テストフック）。

### in-flight 追跡（★runner へ吸収しない — v2 決定）

- `search.ts:62` `let refreshInFlight` / `trackRefresh`(428) / `runRefresh`(438) / `flushPendingRefresh`(447)。
- `trackRefresh` は **catch 済み（エラー握り潰し）promise** を追跡し、`flushPendingRefresh` の `await refreshInFlight` は決して throw しない。
- `flushPendingRefresh` は **debounce timer（`debounceTimer`）も見る**（debounce は #536 のスコープ）。
- **当初は runner の `inFlight()` へ吸収する計画だったが、codex adversarial review が回帰を確定**（plan.md Step 5c-1）: instant fetch や直接 `refreshResults()` まで flush 待受に載せると、instant→非 instant 遷移直後の activation が stale な instant IPC を待つ挙動変化になる。→ **flush 追跡は refresh lane 固有として search.ts に現状のまま残し、runner は staleness/世代のみを担う**（plan.md「設計の要」）。

### instant 経路と循環 import

- `instantCommand.ts` は `api`/`lib/types` のみに依存し **`search.ts` へ逆依存しない**（循環 import 回避。ui/CLAUDE.md の不変条件）。
- 現構造は `search.ts` が `nextRequestId`/`isStale` の 2 hooks を注入。→ **`searchLane.run` の 1 関数注入**に置換すれば staleness 判定は runner 内部へ移り、循環 import も生じない（instantCommand は引き続き search.ts を import しない）。
- instant fetch の 30ms デバウンスタイマー（`INSTANT_CMD_DEBOUNCE_MS`）は **#536 のスコープ**なので instantCommand.ts に残す。runner の `run()` は「タイマー発火後（await 前）」に呼ぶ＝現在 `hooks.nextRequestId()` が呼ばれる位置と同一。

## 既存パターン（再利用）

- **choke point パターン**: `nextGeneration()`（世代更新の唯一経路）は既に choke 化済み（#431）。runner はこの choke を `run()`+`invalidate()` の 2 経路に置換し、SSOT を維持する。
- **await 後の staleness チェック**（ui/CLAUDE.md「await 後に保存状態を復元する場合は staleness チェック」）: 本 issue はこのパターン自体を primitive に昇格させる。
- **pure factory の `lib/` 配置**: `folderNav.ts`/`windowHeight.ts`/`truncatePath.ts` は「テスト可能なため stores から分離」。`createLatestRun` も純粋なファクトリ（SolidJS/api 非依存）→ `lib/latestRun.ts` + `lib/latestRun.test.ts`。

## 技術的制約

- **Win32/IPC 非依存**: 本変更はフロントエンド TS のみ。Win32 入力・ウィンドウ API の同期性の懸念は無い。
- **リアクティブ制約**: `getSearchGeneration()`（= `searchLane.current()`）は ResultsSection の `createEffect(on(results, ...))` が results 更新後に読む。runner の `run()` は `requestId` を task に渡し、`updateResults` 後・次の `run()`/`invalidate()` 前は `current() === requestId` が成立する（perf 相関の不変条件）。
- **世代は world 世代**（★issue 最大の判断点）: `searchGeneration` は検索 lane 専用ではなく、モード遷移・起動でも進んで in-flight 検索を無効化する。**素朴な per-lane token 化は不可**。→ 本計画は **runner が world カウンタを所有し、非 lane コード（モード遷移・起動）は `searchLane.invalidate()` で world を進める**設計にする（lane と world 世代を 1 オブジェクトへ統一）。

## 未解決の疑問

- **`createLatestRun` の配置**: `lib/` か `stores/`。純粋ファクトリかつ単体テスト対象のため `lib/` を第一候補とする（folderNav/windowHeight の前例）。#535 の `exclusive`(mutex) も同族 primitive のため、将来的な集約先（例: `lib/concurrency/`）は #535 着手時に判断（YAGNI: 本 issue では 1 ファイル）。
- **in-flight 共有の広がり（解決済み）**: 当初「instant fetch も runner 経由で `inFlight()` に載るが、instant モード中の activation は `tryModalActivate` で分岐し到達しないため実害無し」と見立てたが、**codex が遷移ケース（instant→非 instant 変更後の activation）で `flushPendingRefresh` に到達し stale instant IPC を待つ回帰を確定**。→ 設計を v2 へ是正（runner は flush 追跡を持たない。plan.md Step 5c-1・不変条件5）。
