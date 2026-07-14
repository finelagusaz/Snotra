# plan.md — issue #534: `latestRun`(supersede) primitive への集約

## 設計の要（★issue 最大の判断点への回答）

**runner は world 世代（staleness/supersede）だけを所有する。in-flight flush 追跡は runner に吸収しない**（codex adversarial review の結論 — 下記「Step 5c」）。

- `run(task)` = 「最新となる lane タスクを開始」。内部で `++generation` して token を捕捉し、`task({ isStale, requestId })` を走らせ、**task の戻り promise をそのまま返す**（in-flight 追跡はしない）。
- `invalidate()` = 「world を進めて in-flight lane タスクを supersede する」。**非 lane コード（モード遷移・起動）が呼ぶ**。現 `nextGeneration()` の pure invalidation 用途を置換。
- `current()` = world 世代の読取（perf requestId 源）。

これにより「モード遷移・起動が in-flight 検索を無効化する」という world 世代の意味が `invalidate()` として明示され、`searchGeneration` の per-lane token 化という罠を回避する。

**flush 追跡（`refreshInFlight`/`trackRefresh`/`flushPendingRefresh`）は search.ts に現状のまま残す。** これは「activation 前に**最新の検索 refresh** の完了を待つ」という refresh lane 固有の関心事であり、runner に吸収して instant fetch や直接 `refreshResults()` まで待受対象に載せると**挙動が変わる**（Step 5c-1/5c-2）。issue の「in-flight 追跡を runner へ吸収」は correctness を優先して見送る（受け入れ条件〔挙動不変・手書き staleness 消滅・公開 API 不変〕はいずれも満たす）。

## primitive API（`lib/latestRun.ts`）

```ts
export interface LatestRunContext {
  /** await 後、この task の token が最新でなくなっていれば true（stale）。 */
  isStale: () => boolean;
  /** この run に割り当てられた world 世代番号（perf 計測の requestId 源）。 */
  requestId: number;
}

export interface LatestRun {
  /** 最新となる lane タスクを開始する。内部で世代を進め、task に isStale/requestId を渡し、
   *  task の戻り promise をそのまま返す（in-flight 追跡はしない）。 */
  run<T>(task: (ctx: LatestRunContext) => Promise<T>): Promise<T>;
  /** world 世代を進め、in-flight タスクを supersede する（非 lane コード用）。新世代番号を返す。 */
  invalidate(): number;
  /** 現在の world 世代番号。 */
  current(): number;
}

export function createLatestRun(): LatestRun;
```

参照実装（骨格）:
```ts
export function createLatestRun(): LatestRun {
  let generation = 0;
  return {
    run(task) {
      const requestId = ++generation;
      const isStale = () => requestId !== generation;
      // 同期起動（下記不変条件）+ 同期 throw の正規化（Step 5c-3）
      try {
        return task({ isStale, requestId });
      } catch (e) {
        return Promise.reject(e);
      }
    },
    invalidate: () => ++generation,
    current: () => generation,
  };
}
```

**不変条件（primitive）**:
- `run()` と `invalidate()` のみが `generation` を書き換える唯一経路（現 `nextGeneration()` choke point の継承）。世代は厳密に **+1 単調前進**。
- `run()` 内で `requestId = ++generation`。`isStale()` は `requestId !== generation`（後続の run/invalidate で true 化）。
- **`run()` は task を同期起動する**（`return task(...)` を直接呼ぶ。`Promise.resolve().then(task)` にしない）。bump 直後・最初の await 前に task 本体が走ることで、`refreshResults` が同期で読む `folderState()`/`query()`/`trim()` のキャプチャタイミングを現行と一致させる（Agent A 軽微-2）。
- **同期 throw の正規化**（Step 5c-3）: `task()` が同期例外を投げても `run()` 自体は throw せず `Promise.reject(e)` を返す。`Promise<T>` を返す契約を守り、将来の非 async callback 流用でも壊れない。
- **`invalidate()` は世代を +1 するだけ**（追跡 state を持たない）。現 `nextGeneration()` が `refreshInFlight` を触らなかった挙動と一致（flush 追跡は runner の外＝search.ts にある）。
- **primitive はクロージャ実装**（`this` 非依存）。`searchLane.run` をメソッド参照として instantCommand へ渡すため（Agent A 要対処-1 の契約）。
- **flush 追跡は持たない**（`inFlight()` を公開しない）。「最新 refresh の待受」は search.ts の `refreshInFlight`/`flushPendingRefresh` が担い、instant/直接 `refreshResults()` を待受対象に載せない現挙動を保つ。

## 変更ファイル一覧

### 1. `ui/src/lib/latestRun.ts`（新規）
- `createLatestRun()` ファクトリと型を実装。上記 API・不変条件のとおり。

### 2. `ui/src/lib/latestRun.test.ts`（新規・単体テスト）
- **stale スキップ**: `run` 中に別の `run`/`invalidate` を挟むと先行 task の `isStale()` が true になる。
- **world 世代による無効化**: run 中に `invalidate()` を呼ぶと isStale が true 化（モード遷移相当）。
- **requestId 相関**: `run` 中の `requestId === current()`、後続 run/invalidate で `current()` が +1 単調で進む。
- **エラー伝播（async reject）**: `async () => { throw }` の task で `run()` の戻り promise が reject する。
- **エラー伝播（同期 throw・Step 5c-3）**: `() => { throw }`（非 async callback）で `run()` が同期 throw せず reject 済み promise を返す。
- 〔削除〕in-flight 待受テストは不要（runner は追跡しない）。

### 3. `ui/src/stores/search.ts`
- `const searchLane = createLatestRun()` を module 冒頭付近に追加（import は top）。
- **削除は 2 シンボルのみ**: `let searchGeneration`（63）、`nextGeneration()`（67-73）。→ runner へ移動。
- **維持（変更しない）**: `let refreshInFlight`（62）、`trackRefresh`（428-436）、`runRefresh`（438-445）、`flushPendingRefresh`（447-456）。flush 追跡は refresh lane 固有として現状のまま残す（Step 5c-1/5c-2）。※`runRefresh` は引き続き `trackRefresh(refreshResults().catch(...))` の形。`flushPendingRefresh` も `refreshInFlight` を await する現形のまま（debounce 部分ともに #536 スコープ）。
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
  へ置換する。これにより **削除される `nextGeneration`/`searchGeneration` への dangling 参照（264/265）を残さない**。staleness 比較は注入点から消え `isStale` は runner 内部へ。instant fetch は `run()` で staleness/bump を得るが `trackRefresh` で包まれない＝flush 追跡に載らない（現挙動と一致）。
- pure invalidation **9 箇所**（131/300/407/493/531/584/595/614/672）の `nextGeneration()` → `searchLane.invalidate()`。〔bump 使用は計 11 = lane-start 2（`168` + instant の `264`／`instantCommand.ts:60` で発火）+ pure 9。`407` は `exitFolderExpansion` の純粋 bump〕
- `executeInstantCommandSelected`(651/671): `preGen = searchGeneration` → `preGen = searchLane.current()`、`searchGeneration === preGen + 1` → `searchLane.current() === preGen + 1`。

### 4. `ui/src/stores/instantCommand.ts`
- `scheduleInstantCommandFetch` の第2引数を `{ nextRequestId, isStale, onFetched, onError }` から **`{ run, onFetched, onError }`** へ変更。
  - `run: <T>(task: (ctx: { isStale: () => boolean }) => Promise<T>) => Promise<T>`。
  - タイマー発火後の body を `void run(async ({ isStale }) => { try { const commands = await api.getInstantCommands(filterName); if (isStale()) return; instantCommandItems = commands; onFetched(mapped); } catch (e) { onError(e); } })` に。
  - `instantCommandItems`/`instantCmdDebounceTimer` の所有・30ms デバウンスは不変（search.ts への逆依存は生じない＝循環 import 回避を維持）。
  - **doc コメント（39/47/49）の同期更新**: `searchGeneration`/`nextGeneration`/`nextRequestId`/`isStale` を名指しする JSDoc を `run` 注入・`searchLane` へ書き換える（実体が変わるため）。
- **不採用: hook redirect 案（Agent A 提案）** — `{ nextRequestId: searchLane.invalidate, isStale: (id) => searchLane.current() !== id, ... }` と現行 hook 形を維持して注入先だけ差し替える案は却下する。理由: `(id) => searchLane.current() !== id` という **手書き staleness 比較が注入点に残り**、受け入れ条件「手書き `requestId !== searchGeneration` の消滅・staleness の runner 内 1 箇所集約」を満たさない。`run` 注入は比較を runner 内へ移す。なお「外部デバウンス駆動なので run() に嵌らない」という懸念は当たらない — `run()` は発火後の setTimeout 内から呼べばよく、デバウンスは呼び出しタイミングを遅らせるだけ（body の同期起動性は保たれる）。

### 5. ドキュメント（プロジェクト文書・Claude が更新可）
- `ui/CLAUDE.md`: `:26` choke point 記述（`nextGeneration()`）を `searchLane`（run/invalidate/current）へ。`:28` instantCommand の「hooks（`nextRequestId`/`isStale`）注入」→「`run` 注入」へ。`:103` 実装パターン「await 後の状態復元は staleness チェック」の `searchGeneration` 名指し → `isStale()`/`current()` へ。lib/ 節に `latestRun.ts` エントリを追加。〔既存の軽微な不正確さ `:27`「instantCommand.ts を re-export」は launchNotice のみが正 — ついでに訂正〕
- `docs/architecture.md`（**独立導出が検出した漏れ**）: `:188/:201/:215` の mermaid Note `nextGeneration() (stale 検出用)` / `searchGeneration で stale チェック` / 補足 `searchGeneration は…カウンタ` が、消えるシンボルを名指し。概念は survive するが名前が腐るため `searchLane`（run/invalidate/isStale）へ更新。
- **SPEC.md**: 更新不要（挙動不変・IPC 契約/フロー/状態遷移の変更なし・公開 API 不変）。3 レビュー全数が「searchGeneration/世代/staleness は SPEC.md に grep 0 件・§8.6/§18.5 は状態遷移と優先度のみ」を実測確認済み。

### 5b. エージェント設定ファイル（合意済み・更新完了）
`searchGeneration` を名指しする以下の agent-config は、チーム憲章上 Claude が単独で変更しない対象だが、**ユーザーの明示指示（「改名にあわせてスキルも更新して」）で合意を得たため、この feature ブランチで更新済み**（commit a244253）。/implement のコード改名と合わせてマージ時に整合し、/implement 中の /race-check も新機構を正しく案内する:
- `.claude/rules/ui.md:10`: `latestRun` の `isStale()`/`run()`/`invalidate()` 基準へ更新済み。
- `.claude/skills/race-check/SKILL.md`（背景パターン3・共有状態表・状態変更経路表・4b・Step5 例）: `searchLane`(`latestRun`) 基準へ更新済み。旧名は履歴注記 1 箇所のみ残す。

## 実装順序（漸進抽出・都度検証）

ビッグバン書き換えをしない。各 Phase で `npm run typecheck` + `npx vitest run ui/src/stores/search.test.ts ui/src/lib/latestRun.test.ts` 緑を確認してからコミット。

1. **Phase 1 — primitive 単体**: `lib/latestRun.ts` + `lib/latestRun.test.ts` を追加。単体テスト緑（Red→Green）。search.ts はまだ触らない。
2. **Phase 2 — 検索 lane 載せ替え**: search.ts で `searchLane` を導入し、`refreshResults` の検索枝・`getSearchGeneration`・pure invalidation 9 箇所・`executeInstantCommandSelected` を移行。`nextGeneration`/`searchGeneration` を削除（`refreshInFlight`/`trackRefresh`/`runRefresh`/`flushPendingRefresh` は**残す**）。**264-265 は削除シンボルを参照するため typecheck が赤になる → Phase 3 と一体で緑化する**（または Phase 2 内で先に 264-265 を `{run}` 注入へ変えてから旧シンボルを削除する順序にする）。既存 `search.test.ts` 緑を維持。
3. **Phase 3 — instant lane 載せ替え**: `instantCommand.ts` の hooks → `run` 注入、`handleInstantQueryInput`(264-265) を `{ run: searchLane.run }` へ更新。既存 instant テスト緑を維持。
4. **Phase 4 — テスト追加**: 下記「テスト方針」の順序不変テスト・rollback 経路テスト・perf 相関テストを `search.test.ts` へ追加。
5. **Phase 5 — ドキュメント同期 + 全体検証**: §5 の文書更新。`npm run typecheck` + `npm run build` + 全 vitest 緑。手書き `!== searchGeneration` が search.ts から消えたことを grep で確認。

## 不変条件（保つべきもの）

1. **挙動不変**: 既存 `search.test.ts` 全緑 + 新規テスト緑 + typecheck/build 緑。
2. **公開 API 不変**: `search.ts` の **export 集合（識別子・シグネチャ）不変** — 追加・削除・改名なし（`getSearchGeneration` は本体のみ `searchLane.current()` 化）。〔issue の「30 exports」は実数と不一致（実測: 直接 export 26 + 再 export 8 = runtime 34、型 export 2）。不変条件は magic number ではなく「集合不変」で記述する。〕**加えて `refreshResults`（テスト用 export）の観測可能な意味＝flush 待受対象を変えない**（Step 5c-2）。
3. **perf requestId 契約**: `perfStartSearch/perfMarkSearchDone/perfCancelSearch` の requestId と `getSearchGeneration()`(= ResultsSection の perfMarkRenderDone) の相関を壊さない。`run()` の `requestId` を task へ渡し、`updateResults` 後・次 bump 前は `current() === requestId`。→ perf 相関テストで固定（テスト方針）。
4. **world 世代の意味保存**: モード遷移・起動（**9 箇所**の `invalidate()`）が in-flight 検索を無効化する挙動を保つ。per-lane token 化しない。
5. **flush 待受スコープ不変（Step 5c-1/5c-2）**: `flushPendingRefresh` が待つのは **refresh lane の in-flight のみ**。instant fetch も直接 `refreshResults()` も待受対象に載せない（＝runner に flush 追跡を吸収しない）。instant→非 instant 遷移直後の activation が stale な instant IPC を待つ回帰を防ぐ。
6. **循環 import 不在**: `instantCommand.ts` は `search.ts` を import しない（`run` を引数注入）。
7. **staleness 判定の SSOT**: `requestId !== searchGeneration` の手書きが search.ts から消え、`isStale()` の定義が runner 内部の 1 箇所に集約される。
8. **early-guard で bump しない**: `refreshResults` の tool/instant ガードは `run()` の前に置き、早期リターン時に world を進めない現挙動を保つ。

### 失敗・異常順序時の振る舞い（新規 primitive のリスク）

- **task が reject（async）**: `run()` の戻りは reject（refresh は `runRefresh` の `.catch` でログ、instant は task 内 try/catch で `onError`）。runner は flush 追跡しないため `flushPendingRefresh` へ影響しない。
- **task が同期 throw**: `run()` は `Promise.reject` へ正規化（Step 5c-3）。同上。
- **run 中に invalidate/run が割り込む**: 先行 task は await 復帰後 `isStale()===true` で適用スキップ（supersede）。
- **preGen+1 ロールバック（Step 5c-4）**: 正しい不変条件は「**`onFailure` 判定時点でのみ** `searchLane.current() === preGen + 1`」。`withLaunchLifecycle` は launch 前に 1 回 `invalidate()` し、`onFailure` はその判定後にロールバック用として更に 1 回 `invalidate()` する（＝失敗経路全体で 2 回）。launch promise が **reject** した場合は `onFailure` 自体が呼ばれず復元も起きない（現挙動）。

## テスト方針

- **新規 `lib/latestRun.test.ts`**: stale スキップ / world 世代無効化 / requestId 相関（+1 単調）/ エラー伝播（async reject **かつ** 同期 throw）。
- **`search.test.ts` 追加**:
  - **順序不変（deferred mock）**: (a) 世代跨ぎ＝古い `api.search` 応答が新しいクエリの後に解決 → 古い結果を適用しない。(b) モード遷移跨ぎ＝検索 in-flight 中に `exitToolSelection`/`enterToolSelection`（`invalidate()`）が走る → 古い検索結果を適用しない。
  - **flush スコープ（Step 5c-1 の回帰網）**: `@foo` の instant IPC を deferred にし、発火後に `/x`（非 instant）へ変更 → `activateSelected()` が **stale な instant IPC を待たない**ことを確認（instant IPC を未 resolve のまま activation が完了する）。
  - **rollback 経路（Step 5c-4）**: deferred launch に対し (a) `status: failed`（既存）、(b) launch reject → `onFailure` 非呼び出しで復元しない、(c) await 中に query/モード変更 → `current() !== preGen+1` で復元をスキップ、を検証。
  - **perf 相関（Step 5c-5）**: `perf` モジュールを spy し、query refresh の `perfStartSearch(id)` / `perfMarkSearchDone(id)` と、`ResultsSection` 経由の `perfMarkRenderDone(id)`（= `getSearchGeneration()`）が **同一 id** であることを固定。世代更新位置のずれを機能テストの外で捕捉する。
- **既存テスト**: 全緑を維持（挙動不変の担保）。
- **検証コマンド**: `npm run typecheck` / `npm run build` / `npx vitest run ui/src`（PostToolUse hook が `*.ts` 編集で typecheck を自動発火。沈黙=合格）。

## セルフレビュー

### Step 5a — check スキル結果

- **/plan-review**（Explore ×2 + 独立導出 Plan ×1）: 中核設計（runner が world 世代を所有・lib/latestRun.ts 配置・循環 import 回避・perf.ts/ResultsSection 無改変・SPEC 不変・iconRequestId/listGeneration/activationInFlight の除外）を **独立に再一致**（完全性の能動的証拠）。反映した指摘: instant lane 264-265 の移行明示化 / bump 数え直し（計 11・pure 9）/ `docs/architecture.md` mermaid の漏れ / agent-config の名前腐り / primitive 仕様の明記 / export「30」→「集合不変」。
- **/race-check**（計画段階・インライン）: 全 await 地点で新規競合リスクなし。`run()` の再入安全性・bump 位置の同値性（ガード後・await 前）を確認。

### Step 5c — codex adversarial review（`codex exec` / read-only）

反証を求める枠組みで実施。5 指摘すべてをコード照合で検証し、以下を反映:

1. **〔要対処・設計変更〕instant fetch を共有 `inFlight()` に載せると activation が変わる** — `@foo` の instant IPC in-flight 中に `/x` へ変更すると `interpKind()!=="instant"` になり `tryModalActivate` が null を返す → 通常 activation が `flushPendingRefresh` に到達し、runner が追跡する stale な instant IPC の完了を待つ（現 `refreshInFlight` は instant を追跡しないので待たない）＝**回帰**。→ **runner から flush 追跡を外す**（v2）。flush 追跡は search.ts の `refreshInFlight`/`trackRefresh`/`flushPendingRefresh` に refresh lane 固有として残す。不変条件5・テスト（flush スコープ）を追加。
2. **〔要対処・受け入れ条件〕`refreshResults`（export）の flush 意味変化** — runner が全 `run()` を追跡すると、直接 `refreshResults()` 後の `activateSelected()` が現状は待たないのに待つようになる。公開 API を識別子/シグネチャに矮小化せず flush 待受対象も不変条件に含める。→ v2 で解消（`refreshResults` は run() で staleness のみ、flush 追跡は `runRefresh` の `trackRefresh` のまま）。不変条件2に追記。
3. **〔軽微・robustness〕`run()` の同期 throw 契約** — `const p=task(); track(p); return p` 形は task が同期 throw すると `run()` 自体が同期 throw し `Promise<T>` 契約を破る。→ `try/catch` で `Promise.reject` へ正規化。単体テストに `() => { throw }` を追加。
4. **〔軽微・明確化+テスト〕`preGen+1` の説明過剰単純化** — 「失敗パスで厳密に1回 invalidate」は誤り。正しくは「`onFailure` 判定時点でのみ `current()===preGen+1`」（判定後にロールバック用の2回目 bump がある）。launch reject 経路は `onFailure` 非呼び出し。→ 不変条件の文言を限定、rollback 経路テスト(a)(b)(c)を追加。
5. **〔軽微・テスト〕perf 相関の証明テスト不在** — 世代更新位置のずれは型/機能テストを通過し perf だけ静かに壊れる。→ perf spy 統合テストを追加（同一 requestId の end-to-end 相関）。

補足指摘の「line 112 に『8 箇所』残存」も確認・修正（grep で line 112 のみ該当・line 74 は既に「9 箇所」）。

### Step 5b — チェックリスト

1. **対称コードパス**: `run`（開始）/`invalidate`（supersede）は対称。flush の set/clear（`trackRefresh` の only-if-latest）は現状のまま。→ OK
2. **影響範囲の網羅性**: `searchGeneration`/`nextGeneration` の全読み書きを grep 済み。独立導出が docs/architecture.md・race-check SKILL.md の名前参照漏れを追加検出 → 反映。→ OK
3. **境界条件**: stale スキップ・world 無効化・requestId 相関・同期/非同期エラーを単体で、順序/flush スコープ/rollback/perf を統合テストで担保。→ OK
4. **リソース管理**: 新規リソースは「世代カウンタ」のみ（+1 単調・破棄概念なし）。flush の `refreshInFlight` は現状の only-if-latest 解除のまま。→ OK
5. **既存パターンとの整合**: choke point パターン（#431）を run/invalidate へ継承。新規の並行制御パターンは足さず、world 世代機構を primitive 化するだけ。flush 追跡は現状維持。→ OK
6. **YAGNI 違反**: primitive の API を run/invalidate/current の **3 つに限定**（flush 追跡を吸収しない＝より小さい）。#535 の `exclusive` を見越した汎用化はしない（1 ファイル）。→ OK
7. **シンプル化の挑戦**: 「runner は世代のみ」は codex 指摘を受けた縮小。`invalidate()` は既存 bare `nextGeneration()` と同義（新概念なし）。→ OK
8. **破壊不変条件の明示**: (a) world 世代 supersede 意味 (b) perf requestId 相関 (c) preGen+1 の判定時点条件 (d) flush 待受スコープ (e) 循環 import 不在。検知手段: 既存テスト（rollback 回帰網）+ 新規順序/flush/rollback/perf テスト + typecheck（dangling 参照）+ build。

### 完成度

- **completeness: 高**（plan-review 3 体が中核を独立再一致 + codex adversarial が flush スコープの回帰を捕捉 → 設計を v2 へ是正済み）。
- **実装着手可否: 可**。着手時の注意: (1) Phase 2/3 の順序（264-265 を先に `{run}` へ変えてから旧シンボル削除）、(2) `refreshInFlight`/`trackRefresh` は**削除しない**（flush 追跡を残す）。
