# plan — issue #535 起動レーンの `activationInFlight` を `exclusive`(mutex) primitive に集約

## 設計判断（要）: 入れ子の扱い = 「呼び出し順の維持」（再入なし）

issue の「再入許可 or 呼び出し順の維持」の二択に対し、**呼び出し順の維持**を採る。

- JS は単一スレッドでスレッド識別子が無く、再入可能 mutex はトークン/深度カウンタの自作を要し KISS に反する。
  再入を許すと「真の並行 2 回目」と「入れ子」を区別できず、二重起動拒否という本来の目的が緩む。
- 現状の自己ブロック回避は「`tryModalActivate` をガードより前に呼ぶ」順序による構造的回避。この順序は
  primitive の**外側**に保つ。具体的には各関数で `tryModalActivate()` を `activationLane(...)` 呼び出しの
  **前**に置く（`tryModalActivate` が非 null なら早期 return し、外側 lane に入らない）。
- 順序契約は「コメント」から「`activationLane` が直後に来る構造」へ格上げされ、視覚的に自明になる
  （primitive の構造そのものへの完全な機械化はしない＝過剰設計を避ける。issue の「検討する」への回答）。

`activationLane` は**単一の共有インスタンス**（現 `activationInFlight` と同じ単一 mutex の意味論を保つ）。

## 変更ファイル一覧

### 1. 新規: `ui/src/lib/exclusive.ts`

`createExclusive()` を定義。`latestRun.ts` の品位（JSDoc・純粋ファクトリ・SolidJS/api 非依存）に揃える。

```ts
/** mutex / single-flight 調停 primitive。
 *  「実行中なら拒否（undefined を返す）、完了時に必ず解放」を内包する小さな runner。
 *  in-flight フラグを 1 箇所（このクロージャ内）で所有する。
 *  検索 lane の supersede（latestRun）と対をなす起動 lane 用（#535）。 */
export type Exclusive = <T>(task: () => Promise<T>) => Promise<T | undefined>;

export function createExclusive(): Exclusive {
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
```

- 戻りはオブジェクトではなく callable 単体（issue 疑似シグネチャ通り。`isInFlight()` 等は現状不要＝YAGNI）。
- `try { return await task() } finally` で sync/async どちらの throw でも `inFlight` を解放し例外を伝播。

### 2. `ui/src/stores/search.ts` — `activationInFlight` を `activationLane` へ載せ替え

- **L64 削除**: `let activationInFlight = false;`
- **`searchLane` 宣言付近（L73 周辺）に追加**: `const activationLane = createExclusive();`
  併せて import 追加: `import { createExclusive } from "../lib/exclusive";`（L7 の latestRun import の隣）。
- **`launchWithSelectedTool()` L509–548**:
  ```ts
  async function launchWithSelectedTool(): Promise<boolean> {
    const frame = toolSelectionState();
    if (!frame) return false;                 // frame 無しは lane を取らず false（現挙動と等価）
    return (await activationLane(async () => {
      const idx = selected();
      const tool = frame.tools[idx];
      if (!tool) return false;
      trace(...);
      return await withLaunchLifecycle(...);  // 中身は不変
    })) ?? false;
  }
  ```
  注: 現行はガード → frame チェックの順だが、両者とも false を返すだけで副作用なし。frame チェックを
  lane の外へ出しても観測差は無い（blocked かつ frame 無し → false / blocked かつ frame 有り → undefined→false /
  非 blocked かつ frame 無し → false、いずれも現行一致）。
- **`executeInstantCommandSelected()` L630–684**:
  ```ts
  async function executeInstantCommandSelected(): Promise<boolean> {
    return (await activationLane(async () => {
      const items = getInstantCommandItems();
      // ... preGen 捕捉含む本体すべてを lane 内へ（同期プレフィックスは呼び出し tick で走る）
      return await withLaunchLifecycle(...);
    })) ?? false;
  }
  ```
- **`activateSelected()` L702–718**:
  ```ts
  async function activateSelected(): Promise<boolean> {
    const modal = tryModalActivate();
    if (modal !== null) return modal;         // ← lane の外（順序契約を構造で表現）
    return (await activationLane(async () => {
      const target = await resolveActivationTarget();
      if (!target) return false;
      const { idx, result } = target;
      if (idx !== selected()) setSelected(idx);
      return launchAndReset(result);          // launchAndReset は lane を取らない（現状同様）
    })) ?? false;
  }
  ```
- **`activateSelectedByIndex()` L720–738**: 同型（`tryModalActivate(index)` を lane 外へ、本体を lane 内へ）。
- **`tryModalActivate` のコメント L687–688 更新**: 「`activationInFlight` ガードより前に呼ぶこと」を
  「`activationLane(...)` に入る前に呼ぶこと（modal 経路はディスパッチ先が自前の lane を取るため、外側 lane に
  入る前に分岐を確定させる）」へ書き換え。
- **`withLaunchLifecycle` のコメント L484–485 更新**（plan-review で両エージェントが独立に検出した漏れ）:
  「`activationInFlight` ガードは呼び出し元ごとに…各呼び出し元が個別に管理する」の記述が、削除済み識別子
  `activationInFlight` をソースコメントに残す。文言を「起動レーンの排他（`activationLane`）は各呼び出し元が
  `activationLane(...)` で包んで担う」へ更新（実態も「個別管理」→「共有 lane で包む」へ意味が変わる）。

### 3. `ui/src/lib/exclusive.test.ts`（新規・単体テスト）

`latestRun.test.ts` の流儀（`makeGate()` で await 中の task を外部解放）を踏襲:

- 非 blocked 時、task の戻り値をそのまま返す。
- in-flight 中の 2 回目呼び出しは `undefined` を返し、**task を起動しない**（呼び出し回数で検証）。
- task 完了後は解放され、再度実行できる（連続 2 回が両方走る）。
- task が reject しても `finally` で解放され、次回実行できる。かつ reject は伝播する。
- task の同期プレフィックスが `activationLane(fn)` 呼び出し tick で同期実行される（同期観測フラグで検証）。

### 4. `ui/src/stores/search.test.ts`（統合テスト追加）

issue のテスト方針 2 件を新規 describe で追加:

- **「起動 in-flight 中の 2 回目の activate が弾かれる（false）」**:
  `api.launchItem` を deferred（未解決 promise）にし、`activateSelected()` を 2 回呼ぶ。1 回目は pending、
  2 回目は即 `false`。`api.launchItem` は 1 回だけ呼ばれる。deferred を解決して 1 回目 true を確認。
  （通常経路＝tool/instant でないモードで実施。`refreshResults` を直接使う既存 describe の流儀に合わせ、
  `api.search` で結果を 1 件入れてから起動する。）
- **「入れ子経路（modal → tool 起動）が自己ブロックしない」**:
  `enterToolSelection`（tools=2）で tool モードへ → `activateSelected()` が
  `tryModalActivate` → `launchWithSelectedTool` → `activationLane` を取り、**自己ブロックせず**起動成功（true）。
  `api.launchWithTool` が 1 回呼ばれることを確認。
  （追加で instant 経路の自己ブロック非発生も 1 件足すか検討 → 既存 rollback テストが instant 経路の
  activateSelected 成功を間接的に通しているため、tool 経路 1 件で十分。過剰なら足さない＝YAGNI。）

### 5. `ui/CLAUDE.md` 更新

- `stores/search.ts` の「横断規約の choke point」列に `activationLane`（`lib/exclusive.ts` の `createExclusive()`
  インスタンス。起動 lane の単一 mutex。実行中の 2 つ目を拒否＝single-flight）を追記。
- `lib/` セクションに `exclusive.ts`（mutex/single-flight 調停 primitive）を `latestRun.ts` の隣に追記。

### 5b. `docs/architecture.md` 更新（plan-review 独立導出が拾った整合項目）

- L214–215 の「補足」に、検索/データ lane（`searchLane`/`latestRun`・supersede・#534）を説明するバレットがある。
  その直後に姉妹の起動 lane を 1 行足す:「起動（launch/activate）lane は `exclusive` primitive（`activationLane`・
  single-flight mutex）が単一の in-flight フラグを所有し、実行中の 2 つ目の起動を `false` で拒否する。検索 lane の
  supersede と対をなす 2 方針（#535）」。挙動不変ゆえ必須ではないが、姉妹 primitive が同じ補足節に並ぶことで
  「2 方針を別名で明示」という本 program の意図とドキュメントが整合する。検索 lane コード自体には触れない（スコープ外厳守）。

### 6. 保留（ユーザー合意が必要）: `.claude/skills/race-check/SKILL.md`

L43/61/99/112 の `activationInFlight` 例示が stale 化する。**エージェント設定ゆえ単独編集しない**
（CLAUDE.md チーム憲章）。実装フェーズでユーザーに提示し、更新可否を確認する。plan 段階では触らない。

## 実装順序（フェーズ）

1. **Phase 1**: `exclusive.ts` + `exclusive.test.ts` を追加（primitive 単体で Red→Green）。
   - 依存なし。単体で完結。PostToolUse hook が `exclusive.test.ts` 編集で typecheck、`.rs` 無しゆえ vitest は
     `search.test.ts`/`exclusive.test.ts` 手動実行で確認。
2. **Phase 2**: `search.ts` の 4 箇所を載せ替え + `tryModalActivate` コメント更新。
3. **Phase 3**: `search.test.ts` に統合テスト 2 件追加（Red で現行が緑のままなことを利用しづらいので、
   先に Phase 2 を入れてから追加。ただし「二重起動 blocked」テストは Phase 2 前の現行実装でも緑になるべき
   ＝挙動不変の証明になるため、可能なら Phase 2 前に一度現行で緑を確認してもよい）。
4. **Phase 4**: `ui/CLAUDE.md` 更新。
5. **Phase 5**: 検証（typecheck + `vitest run ui/src/lib/exclusive.test.ts ui/src/stores/search.test.ts`）。
   race-check/SKILL.md はユーザー合意後に別途。

## 不変条件

- **`activationLane` は起動レーン共通の単一 mutex**（現 `activationInFlight` と同一意味論）。入れ子経路
  （`activateSelected` → `tryModalActivate` → `launchWithSelectedTool`）で自己ブロックしない。
  → 保証機構: `tryModalActivate()` を `activationLane(...)` の**前**に置く（modal 非 null なら lane に入らない）。
- **戻り値契約**: blocked は `false`（起動されなかった）。`(await activationLane(...)) ?? false` で維持。
  公開関数 `activateSelected`/`activateSelectedByIndex` は boolean を返し続ける（呼び出し側が `.then(boolean)`）。
- **in-flight フラグは必ず解放される**: 失敗・異常終了・throw・予期しない順序いずれでも `finally` で `inFlight=false`。
  → 「戻せない状態に固まる」リスク（.claude/rules/src-tauri.md「true にしたら false に戻す経路とセット」の JS 版）を
     primitive の `try/finally` 1 箇所に閉じ込める。
- **同期起動タイミングの維持**: `await task()` により task の同期プレフィックス（`preGen` 捕捉・`selected()` 読み）が
  現行と同じ tick で走る。→ `executeInstantCommandSelected` の preGen+1 rollback 判定が壊れない（既存テスト
  「rollback（world 世代が進んだら復元しない）」が回帰網）。
- **`withLaunchLifecycle` の `setLaunching` は不変**: UI 表示レイヤーで mutex とは独立。触らない。
- **公開 API 不変**: export 一覧・シグネチャ・戻り型すべて不変。

### 異常系（新規 primitive の失敗時挙動）

- task が例外を投げる → `finally` で `inFlight=false` に戻り、例外は呼び出し側へ伝播（呼び出し側は既存の
  `.catch`/`.then` で処理。現行 try/finally と同じ）。
- blocked（`inFlight===true`）で 2 回目 → task を**起動せず** `undefined` を返す（副作用ゼロ）。現行の
  `if (activationInFlight) return false` と等価（`?? false` で false 化）。
- `activationLane` は単一インスタンスゆえ「複数 mutex の取り違え」は原理的に起きない。

## テスト方針

- 追加テスト:
  - `exclusive.test.ts`: 5 ケース（上記 §3）。primitive の mutex 意味論・解放・同期起動を単体で担保。
  - `search.test.ts`: 2 ケース（上記 §4）。実配線での二重起動拒否・入れ子非ブロックを担保。
- 回帰網: 既存 `search.test.ts` の全 describe（特に instant rollback / supersede / flush スコープ）が緑のまま
  であること＝挙動不変の証明。
- 検証コマンド（`docs/build-commands.md` 準拠）:
  - `npm run typecheck`（PostToolUse hook でも自動発火）
  - `npx vitest run ui/src/lib/exclusive.test.ts ui/src/stores/search.test.ts`
  - 最終: `npx vitest run`（ui 全体で回帰確認）

## SPEC.md 更新要否

**不要**。挙動不変・公開 API 不変。JS の activation mutex は SPEC.md に元々記載が無い（L409 は Rust の
別 Mutex）。#540 も SPEC を触っていない（同種 refactor）。

## セルフレビュー

### 5a. check スキル結果

- **`/plan-review`**（Explore 成果物監査 + Plan 独立導出の 2 体並列）: completeness **高**・着手可否 **可**。
  - **両エージェントが独立に検出した漏れ（反映済み）**: `withLaunchLifecycle` の JSDoc（`search.ts:484-485`）が
    削除予定の識別子 `activationInFlight` を名指しで残す。§2 に更新項目を追記した。
  - **独立導出が拾った整合項目（反映済み）**: `docs/architecture.md:214-215` の補足に姉妹 lane を並記（§5b 追加）。
  - **一致（盲点なしの能動的証拠）**: primitive の同期起動＝`await task()` で preGen 捕捉が現行と同 tick／`?? false` で
    boolean 契約維持／単一 mutex・入れ子非ブロック／`launchWithSelectedTool`・`executeInstantCommandSelected` は
    `tryModalActivate` 単一呼び出し元の private／SPEC.md 更新不要（L409 は Rust 別 Mutex）／既存「二重起動 blocked」
    テスト不在——これらを 2 体が独立に再導出し一致。
- **`/symmetric-check`**: set-true/set-false の 4 対（`:514/:546`・`:632/:682`・`:706/:716`・`:724/:736`）を
  primitive の単一 try/finally へ集約。各 try 内早期 return（`:518/:637/:709/:730`）は finally を必ず通り
  解放保証が保たれる。`setLaunching` ペア（別レイヤー）・enter/exit ペア（activationInFlight 不使用）は対象外。
  **leak/bypass 経路なし**。
- **`/race-check`**: 全 await 地点で状態競合リスクなし。核心 2 点——preGen 捕捉タイミング不変（`await task()` の
  同期起動）／guard-set の原子性（`if(inFlight) return; inFlight=true` の間に await 無し＝TOCTOU 無し）——を
  裏取り。`exclusive.test.ts` の「同期起動」ケースで機械的に担保。

### 5b. チェックリスト

1. **対称コードパス**: `/symmetric-check` 済。set true/false 4 対を primitive へ集約、取りこぼしなし。
2. **影響範囲の網羅性**: `activationInFlight` 全参照を grep（`search.ts` 本体 + `race-check/SKILL.md` 例示）。
   呼び出し元（`SearchWindow.tsx:227`/`MainApp.tsx:293`/`enterToolSelection:565`）の boolean 契約を確認。
3. **境界条件**: blocked / task が false 返し / task が true 返し / frame 無し 3 ケース / 入れ子（modal→tool）/
   異常系（reject）を plan の異常系セクションとテストで網羅。
4. **リソース管理**: in-flight フラグの生成（set true）/破棄（finally set false）ペアを primitive 1 箇所に集約。
   失敗・throw・早期 return いずれでも finally で解放。
5. **既存パターンとの整合**: 姉妹 `latestRun.ts` の品位（純粋ファクトリ・JSDoc・同期起動）に揃える。新規パターン非導入。
6. **YAGNI 違反**: `isInFlight()` 等の未使用 API を足さない。callable 単体。再入可能 mutex（トークン/深度）は導入しない。
7. **シンプル化の挑戦**: 単一スレッド JS で再入可能 mutex は過剰。「呼び出し順の維持」で入れ子を構造的に回避する方が
   単純かつ現行構造と同型。「この操作が失敗したら」＝ finally で解放（異常系セクション参照）。
8. **破壊不変条件の明示**: (i) in-flight が finally で必ず解放される（戻せない状態に固まらない）→ `exclusive.test.ts`
   の reject 後解放テストで検知。(ii) preGen 捕捉が同 tick で走る → `exclusive.test.ts` 同期起動テスト +
   既存 `search.test.ts:927-957` rollback 回帰。(iii) 二重起動拒否 → 新規統合テスト。いずれも検知手段とセット。

### 5c. 実装時の申し送り（ユーザー合意が必要な項目）

- `.claude/skills/race-check/SKILL.md`（L43/61/99/112）の `activationInFlight` 例示は refactor 後 stale 化する。
  **エージェント設定ゆえチーム憲章に従い単独編集しない**。実装フェーズでユーザーに提示し更新可否を確認する。

### 総評

計画の completeness: **高**。実装着手可否: **可**（要対処ブロッカーなし。plan-review 指摘 2 点は反映済み）。
