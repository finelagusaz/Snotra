# plan.md — issue #534: `latestRun`(supersede) primitive への集約

## 設計の要（★issue 最大の判断点への回答）

**runner が world 世代カウンタを所有する。** lane と world 世代を 1 オブジェクトへ統一する:

- `run(task)` = 「最新となる lane タスクを開始」。内部で `++generation` して token を捕捉し、`task({ isStale, requestId })` を走らせ、in-flight として追跡する。
- `invalidate()` = 「world を進めて in-flight lane タスクを supersede する」。**非 lane コード（モード遷移・起動）が呼ぶ**。現 `nextGeneration()` の pure invalidation 用途を置換。
- `current()` = world 世代の読取（perf requestId 源）。
- `inFlight()` = 追跡中の promise（settle-on-both）。`flushPendingRefresh` が await。

これにより「モード遷移・起動が in-flight 検索を無効化する」という world 世代の意味が `invalidate()` として明示され、`searchGeneration` の per-lane token 化という罠を回避する。

## primitive API（`lib/latestRun.ts`）

```ts
export interface LatestRunContext {
  /** await 後、この task の token が最新でなくなっていれば true（stale）。 */
  isStale: () => boolean;
  /** この run に割り当てられた world 世代番号（perf 計測の requestId 源）。 */
  requestId: number;
}

export interface LatestRun {
  /** 最新となる lane タスクを開始する。内部で世代を進め、task に isStale/requestId を渡す。 */
  run<T>(task: (ctx: LatestRunContext) => Promise<T>): Promise<T>;
  /** world 世代を進め、in-flight タスクを supersede する（非 lane コード用）。新世代番号を返す。 */
  invalidate(): number;
  /** 現在の world 世代番号。 */
  current(): number;
  /** 追跡中の in-flight promise（無ければ undefined）。resolve/reject どちらでも解決。 */
  inFlight(): Promise<void> | undefined;
}

export function createLatestRun(): LatestRun;
```

**不変条件（primitive）**:
- `run()` と `invalidate()` のみが `generation` を書き換える唯一経路（choke point の継承）。
- `run()` 内で `requestId = ++generation`。`isStale()` は `requestId !== generation`（後続の run/invalidate で true 化）。
- **`run()` は task を同期起動する**（`const id = ++generation; const p = task(...); track(p); return p;`）。bump 直後・最初の await 前に task 本体が走ることで、`refreshResults` が同期で読む `folderState()`/`query()`/`trim()` のキャプチャタイミングを現行と一致させる（Agent A 軽微-2）。
- `inFlight()` は catch 済み（エラー握り潰し）void promise を追跡し、`flushPendingRefresh` の await が throw しない現挙動を保つ。追跡の finally で **自身が最新のときだけ**（`tracked === latest`）undefined へ戻す（現 `trackRefresh` の `refreshInFlight === pending` と同一の「最新のみ保持」）。
- **`invalidate()` は世代を +1 するだけで、追跡中の `inFlight` を drop しない**（現 `nextGeneration()` が `refreshInFlight` を触らなかった挙動と一致）。drop すると instant キーストローク後に `flushPendingRefresh` が in-flight refresh を await しなくなる（Agent A 軽微-1(3)）。
- **世代は厳密に +1 単調前進**（`run()`/`invalidate()` のみが書き換える）。`executeInstantCommandSelected` の `current() === preGen + 1` 判定がこの単調性に結合している。
- `run()` の戻り値は **生の task promise**（reject しうる）を返し、呼び出し側（`runRefresh`）が従来どおり `.catch` でログする。in-flight 追跡は握り潰し版を使い、戻り値と分離する。
- **primitive はクロージャ実装**（`this` 非依存）。`searchLane.run` をメソッド参照として instantCommand へ渡すため（Agent A 要対処-1 の契約）。

## 変更ファイル一覧

### 1. `ui/src/lib/latestRun.ts`（新規）
- `createLatestRun()` ファクトリと型を実装。上記 API・不変条件のとおり。

### 2. `ui/src/lib/latestRun.test.ts`（新規・単体テスト）
- **stale スキップ**: `run` 中に別の `run`/`invalidate` を挟むと先行 task の `isStale()` が true になる。
- **in-flight 待受**: `inFlight()` が run 中は promise を返し、完了で undefined に戻る。最新 run のみ保持。
- **エラー伝播**: task が throw すると `run()` の戻り promise は reject するが、`inFlight()` の await は throw しない。
- **world 世代による無効化**: run 中に `invalidate()` を呼ぶと isStale が true 化（モード遷移相当）。
- **requestId 相関**: `run` 中の `requestId === current()`、後続 run/invalidate で `current()` が進む。

### 3. `ui/src/stores/search.ts`
- `const searchLane = createLatestRun()` を module 冒頭付近に追加（import は top）。
- **削除**: `let searchGeneration`（63）、`nextGeneration()`（67-73）、`let refreshInFlight`（62）、`trackRefresh`（428-436）。
- `getSearchGeneration()`(799) → `return searchLane.current()`。
- `refreshResults`(163-248): 冒頭 2 ガード（tool/instant）は **`run()` の前**に残す（早期リターンで bump しない現挙動を保つ）。ガード通過後を `return searchLane.run(async ({ isStale, requestId }) => { ...本体... })` で包む。本体内の `const requestId = nextGeneration()`(168) を削除し ctx の `requestId` を使用。`requestId !== searchGeneration`(183/232) → `isStale()`。`perfCancelSearch(requestId)` はそのまま。
- **instant lane（264-265 の移行を明示・Agent A 要対処-1）**: `handleInstantQueryInput`(253-274) が `scheduleInstantCommandFetch` へ渡す第2引数を、現行の
  ```ts
  { nextRequestId: nextGeneration, isStale: (requestId) => requestId !== searchGeneration, onFetched, onError }
  ```
  から
  ```ts
  { run: searchLane.run, onFetched, onError }
  ```
  へ置換する。これにより **削除される `nextGeneration`/`searchGeneration` への dangling 参照（264/265）を残さない**。staleness 比較（`requestId !== searchGeneration`）は注入点から消え、`isStale` は runner 内部に移る＝受け入れ条件「手書き比較の消滅・staleness の runner 集約」を満たす。
- pure invalidation **9 箇所**（131/300/407/493/531/584/595/614/672）の `nextGeneration()` → `searchLane.invalidate()`。〔bump 使用は計 11 = lane-start 2（`168` + instant の `264`／`instantCommand.ts:60` で発火）+ pure 9。`407` は `exitFolderExpansion` の純粋 bump〕
- `executeInstantCommandSelected`(651/671): `preGen = searchGeneration` → `preGen = searchLane.current()`、`searchGeneration === preGen + 1` → `searchLane.current() === preGen + 1`。
- `runRefresh`(438-445): `trackRefresh(...)` を外し `return refreshResults().catch(...)` に。in-flight 追跡は runner が担う。
- `flushPendingRefresh`(447-456): `if (refreshInFlight) await refreshInFlight` → `const p = searchLane.inFlight(); if (p) await p`。debounce timer 部分は不変（#536 スコープ）。

### 4. `ui/src/stores/instantCommand.ts`
- `scheduleInstantCommandFetch` の第2引数を `{ nextRequestId, isStale, onFetched, onError }` から **`{ run, onFetched, onError }`** へ変更。
  - `run: <T>(task: (ctx: { isStale: () => boolean }) => Promise<T>) => Promise<T>`。
  - タイマー発火後の body を `void run(async ({ isStale }) => { try { const commands = await api.getInstantCommands(filterName); if (isStale()) return; instantCommandItems = commands; onFetched(mapped); } catch (e) { onError(e); } })` に。
  - `instantCommandItems`/`instantCmdDebounceTimer` の所有・30ms デバウンスは不変（search.ts への逆依存は生じない＝循環 import 回避を維持）。
  - **doc コメント（39/47/49）の同期更新**: `searchGeneration`/`nextGeneration`/`nextRequestId`/`isStale` を名指しする JSDoc を `run` 注入・`searchLane` へ書き換える（実体が変わるため）。
- **不採用: hook redirect 案（Agent A 提案）** — `{ nextRequestId: searchLane.invalidate, isStale: (id) => searchLane.current() !== id, ... }` と現行 hook 形を維持して注入先だけ差し替える案は却下する。理由: `(id) => searchLane.current() !== id` という **手書き staleness 比較が注入点に残り**、受け入れ条件「手書き `requestId !== searchGeneration` の消滅・staleness の runner 内 1 箇所集約」を満たさない。`run` 注入（Approach P）は比較を runner 内へ移す。なお「外部デバウンス駆動なので run() に嵌らない」という懸念は当たらない — `run()` は発火後の setTimeout 内から呼べばよく、デバウンスは呼び出しタイミングを遅らせるだけ（body の同期起動性は保たれる）。

### 5. ドキュメント（プロジェクト文書・Claude が更新可）
- `ui/CLAUDE.md`: `:26` choke point 記述（`nextGeneration()`）を `searchLane`（run/invalidate/current/inFlight）へ。`:28` instantCommand の「hooks（`nextRequestId`/`isStale`）注入」→「`run` 注入」へ。`:103` 実装パターン「await 後の状態復元は staleness チェック」の `searchGeneration` 名指し → `isStale()`/`current()` へ。lib/ 節に `latestRun.ts` エントリを追加。〔既存の軽微な不正確さ `:27`「instantCommand.ts を re-export」は launchNotice のみが正 — ついでに訂正〕
- `docs/architecture.md`（**独立導出が検出した漏れ**）: `:188/:201/:215` の mermaid Note `nextGeneration() (stale 検出用)` / `searchGeneration で stale チェック` / 補足 `searchGeneration は…カウンタ` が、消えるシンボルを名指し。概念は survive するが名前が腐るため `searchLane`（run/invalidate/isStale）へ更新。
- **SPEC.md**: 更新不要（挙動不変・IPC 契約/フロー/状態遷移の変更なし・公開 API 不変）。3 レビュー全数が「searchGeneration/世代/staleness は SPEC.md に grep 0 件・§8.6/§18.5 は状態遷移と優先度のみ」を実測確認済み。

### 5b. エージェント設定ファイル（★合意してから・Claude 単独判断不可）
以下は `searchGeneration` を名指しするが、**スキル・rules は「エージェント設定」でありチーム憲章上 Claude が単独で変更しない**（CLAUDE.md 最重要ルール 3）。実装フェーズで**ユーザーに提案し合意を得てから**更新する（本 refactor で名前が腐るのは事実なので、放置は false green を生む・独立導出が safety-nets.md の引き金と指摘）:
- `.claude/rules/ui.md:10`: 「await 後の状態復元は staleness チェック必須」の `searchGeneration` カウンタ名指し。
- `.claude/skills/race-check/SKILL.md`（`:22/43/60/62/64/82/84-85/108` 付近）: `searchGeneration` を「モジュールスコープ変数」例・staleness パターン例として多数名指し。更新しないと将来の race-check が存在しない変数を探す。

## 実装順序（漸進抽出・都度検証）

ビッグバン書き換えをしない。各 Phase で `npm run typecheck` + `npx vitest run ui/src/stores/search.test.ts ui/src/lib/latestRun.test.ts` 緑を確認してからコミット。

1. **Phase 1 — primitive 単体**: `lib/latestRun.ts` + `lib/latestRun.test.ts` を追加。単体テスト緑（Red→Green）。search.ts はまだ触らない。
2. **Phase 2 — 検索 lane 載せ替え**: search.ts で `searchLane` を導入し、`refreshResults` の検索枝・`getSearchGeneration`・`runRefresh`/`flushPendingRefresh`・pure invalidation 9 箇所・`executeInstantCommandSelected` を移行。`nextGeneration`/`searchGeneration`/`refreshInFlight`/`trackRefresh` を削除。**この時点で 264-265（instant hook 注入）は削除シンボルを参照するため typecheck が赤になる → Phase 3 と一体で緑化する**（または Phase 2 内で先に 264-265 を `{run}` 注入へ変えてから削除する順序にする）。既存 `search.test.ts` 緑を維持。
3. **Phase 3 — instant lane 載せ替え**: `instantCommand.ts` の hooks → `run` 注入、`handleInstantQueryInput`(264-265) を `{ run: searchLane.run }` へ更新。既存 instant テスト緑を維持。
4. **Phase 4 — 順序不変テスト追加**: `search.test.ts` に「古い await が新しい結果を上書きしない」テスト（世代跨ぎ + モード遷移跨ぎ）を追加。deferred mock で stale IPC を新しい bump の後に解決させ、結果が適用されないことをアサート。
5. **Phase 5 — ドキュメント同期 + 全体検証**: ui/CLAUDE.md・rules/ui.md 更新。`npm run typecheck` + `npm run build` + 全 vitest 緑。手書き `!== searchGeneration` が search.ts から消えたことを grep で確認。

## 不変条件（保つべきもの）

1. **挙動不変**: 既存 `search.test.ts` 全緑 + 新規順序テスト緑 + typecheck/build 緑。
2. **公開 API 不変**: `search.ts` の **export 集合（識別子・シグネチャ）不変** — 追加・削除・改名なし（`getSearchGeneration` は本体のみ `searchLane.current()` 化）。〔issue の「30 exports」は実数と不一致（実測: 直接 export 26 + 再 export 8 = runtime 34、型 export 2）。不変条件は magic number ではなく「集合不変」で記述する（3 レビュー一致）。〕
3. **perf requestId 契約**: `perfStartSearch/perfMarkSearchDone/perfCancelSearch` の requestId と `getSearchGeneration()`(= ResultsSection の perfMarkRenderDone) の相関を壊さない。`run()` の `requestId` を task へ渡し、`updateResults` 後・次 bump 前は `current() === requestId`。
4. **world 世代の意味保存**: モード遷移・起動（8 箇所の `invalidate()`）が in-flight 検索を無効化する挙動を保つ。per-lane token 化しない。
5. **循環 import 不在**: `instantCommand.ts` は `search.ts` を import しない（`run` を引数注入）。
6. **staleness 判定の SSOT**: `requestId !== searchGeneration` の手書きが search.ts から消え、`isStale()` の定義が runner 内部の 1 箇所に集約される。
7. **early-guard で bump しない**: `refreshResults` の tool/instant ガードは `run()` の前に置き、早期リターン時に world を進めない現挙動を保つ。
8. **in-flight の握り潰し**: `inFlight()` の await が throw しない（現 `trackRefresh` の catch 済み追跡と同一）。

### 失敗・異常順序時の振る舞い（新規 primitive のリスク）

- **task が reject**: `run()` の戻りは reject（`runRefresh` が `.catch` でログ）。`inFlight()` は握り潰し版のため resolve。→ `flushPendingRefresh` は throw せず継続。
- **run 中に invalidate/run が割り込む**: 先行 task は await 復帰後 `isStale()===true` で適用スキップ（supersede）。in-flight は最新のみ保持。
- **instant fetch が in-flight に載る**: 現状 `refreshInFlight` は instant を追跡しないが、統一 lane では load される。instant モード中の activation は `tryModalActivate` で分岐し `flushPendingRefresh` に到達しないため実害無し（/race-check・/plan-review で確認）。

## テスト方針

- **新規 `lib/latestRun.test.ts`**: stale スキップ / in-flight 待受 / エラー伝播 / world 世代無効化 / requestId 相関（上記 2 参照）。
- **`search.test.ts` 追加**: 順序不変テスト（deferred mock）。
  - 世代跨ぎ: 古い `api.search` 応答が新しいクエリの後に解決 → 古い結果を適用しない。
  - モード遷移跨ぎ: 検索 in-flight 中に `exitFolderExpansion`/起動相当の `invalidate()` が走る → 古い検索結果を適用しない。
- **既存テスト**: 全緑を維持（挙動不変の担保）。
- **検証コマンド**: `npm run typecheck` / `npm run build` / `npx vitest run ui/src`（PostToolUse hook が `*.ts` 編集で typecheck を自動発火。沈黙=合格）。

## セルフレビュー

### Step 5a — check スキル結果

- **/plan-review**（Explore ×2 + 独立導出 Plan ×1）: 中核設計（runner が world 世代を所有・lib/latestRun.ts 配置・循環 import 回避・perf.ts/ResultsSection 無改変・SPEC 不変・iconRequestId/listGeneration/activationInFlight の除外）を **独立に再一致**（完全性の能動的証拠）。反映した指摘:
  - 〔要対処〕instant lane 264-265 の移行を明示化（Approach P・dangling 参照を残さない）。hook redirect 案は受け入れ条件違反で却下。
  - 〔要対処〕bump 使用の数え直し: 計 11（lane-start 2 + pure 9）。「8 箇所」誤記を「9 箇所」へ訂正。
  - 〔漏れ〕`docs/architecture.md:188/201/215`（mermaid）を doc 更新対象に追加。
  - 〔漏れ／governance〕`.claude/rules/ui.md:10`・`.claude/skills/race-check/SKILL.md` は agent-config → §5b で「合意してから」に分離。
  - 〔軽微〕primitive 仕様に「同期起動」「invalidate は inFlight を drop しない」「+1 単調」「クロージャ実装」を明記。
  - 〔軽微〕export「30」→「集合不変」へ。
- **/race-check**（計画段階・インライン）: 全 await 地点で新規競合リスクなし。`run()` の再入安全性・bump 位置の同値性（ガード後・await 前）・instant を inFlight に載せる benign 変化を確認。詳細は会話ログ。

### Step 5b — チェックリスト

1. **対称コードパス**: `run`（開始）/`invalidate`（supersede）、`inFlight` の set/clear（only-if-latest）は対称に設計。/symmetric-check 相当は race-check で吸収。→ OK
2. **影響範囲の網羅性**: `searchGeneration`/`nextGeneration`/`refreshInFlight`/`trackRefresh` を grep 済み（コード参照は search.ts 内のみ、instantCommand は注入経由、消費者は perf/ResultsSection の公開 API のみ）。独立導出が docs/architecture.md・race-check SKILL.md の名前参照漏れを追加検出 → 反映。→ OK
3. **境界条件**: stale スキップ・in-flight 待受・エラー伝播・world 無効化・requestId 相関を単体テストで、世代跨ぎ／モード遷移跨ぎを順序テストで担保。→ OK
4. **リソース管理**: 新規リソースは「世代カウンタ」と「inFlight promise」のみ。カウンタは +1 単調（破棄概念なし）。inFlight は settle の finally で only-if-latest 解除＝滞留しない。異常系（task reject）は握り潰し版が await を throw させない。→ OK
5. **既存パターンとの整合**: choke point パターン（#431）を run/invalidate の 2 経路へ継承。新規の並行制御パターンは導入せず、既存の world 世代機構を primitive 化するだけ。→ OK
6. **YAGNI 違反**: primitive の API は run/invalidate/current/inFlight の 4 つに限定（現行機構の 1:1 写像に必要な最小）。#535 の `exclusive` を見越した汎用化・共通基底は作らない（1 ファイル）。→ OK
7. **シンプル化の挑戦**: 「runner が world カウンタを所有」は per-lane token 化という誤設計を避けるための最小構造。`invalidate()` は既存の bare `nextGeneration()` と同義（新概念を足さない）。→ OK
8. **破壊不変条件の明示**: 「壊れたら即アウト」= (a) world 世代の supersede 意味（モード遷移・起動が in-flight 検索を無効化）(b) perf requestId 相関 (c) preGen+1 の +1 単調性 (d) 循環 import 不在。検知手段: 既存 `search.test.ts`（executeInstantCommandSelected 失敗復元 = (c) の回帰網）+ 新規順序テスト（(a)）+ typecheck（(d) の dangling 参照・264-265）+ build。手動確認は不要（PostToolUse hook が typecheck 自動発火）。

### 完成度

- **completeness: 高**（3 レビューが中核を独立再一致・漏れは docs/agent-config の名前参照のみで反映済み）。
- **実装着手可否: 可**（要対処 2 件は plan.md に反映済み。着手時の唯一の注意は Phase 2/3 の順序＝264-265 を先に `{run}` 注入へ変えてから旧シンボルを削除し typecheck 赤を避ける）。
