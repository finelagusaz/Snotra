# research: issue #539 — executeInstantCommandSelected の preGen+1 baseline 判定を withLaunchLifecycle 側の述語へ引き上げる

## issue の要約

#534 で world 世代機構を `createLatestRun()` primitive（`ui/src/lib/latestRun.ts`）へ集約したが、
**非 lane の baseline-delta 判定**（`executeInstantCommandSelected` の失敗ロールバック）だけが生の世代算術
（`searchLane.current() === preGen + 1`）に残っている。

- correctness バグではなく **altitude（実装の深さ）** の改善（`/simplify` の Altitude レビューで検出）。
- `+1` は `withLaunchLifecycle` が内部で `searchLane.invalidate()` を **ちょうど 1 回** 呼ぶという
  別関数の内部実装への hardcode。将来 bump 数が変われば **コンパイラの助けなく静かに壊れる**。
- #534 は mutation（`invalidate`）を choke point 化したが comparison 側は漏らした ── **非対称**。

**目標**: `searchLane.invalidate()` を所有する `withLaunchLifecycle` が「自分の launch を超えて world が
動いたか」を答える述語も所有する。呼び出し側は bump 数を知らずに `if (!disturbed()) { restore }` と書ける。

## 関連コード

| ファイル:行 | 役割 |
|---|---|
| `ui/src/stores/search.ts:496-517` | `withLaunchLifecycle` 本体（choke point）。line 504 が唯一の `invalidate()`＝1 bump |
| `ui/src/stores/search.ts:632-681` | `executeInstantCommandSelected`。line 651 で `preGen` 捕捉、line 672 で `=== preGen+1` 判定 |
| `ui/src/stores/search.ts:607-630` | `launchAndReset`（呼出し元1）。onFailure は `runRefresh` のみ・staleness 不使用 |
| `ui/src/stores/search.ts:519-554` | `launchWithSelectedTool`（呼出し元2）。onFailure は `runRefresh` のみ・staleness 不使用 |
| `ui/src/lib/latestRun.ts:30-53` | `createLatestRun`。`current()` が生 int を返す（comparison の choke point 不在の源） |

### withLaunchLifecycle の world 世代への作用（検証済み）

`searchLane\.` の全 grep（search.ts 内 11 箇所）と helper 定義の確認により:

- `withLaunchLifecycle` 内で `searchLane` を触るのは **line 504 の `invalidate()` ただ 1 つ**。
- `clearLaunchNotice()` / `setLaunching()` / `clearResults()`（→ `updateResults`/`setSelected`）は
  いずれも `searchLane` を触らない（`updateResults` = `setResults`+`setNoResults`、`clearResults` =
  `updateResults([])`+`setSelected(0)`。定義: search.ts:34-45）。
- `setInstantCommandItems`（`instantCommand.ts`）は `search.ts` へ逆依存しない＝`searchLane` を触らない。
- ゆえに「`withLaunchLifecycle` の 1 bump のみ」は事実。既存コメント（line 670）の主張は正しい。

### preGen+1 判定の等価性（refactor 前後で挙動同一であることの証明）

記号: `preGen` 捕捉時点の generation を `G`、await 中の追加 bump 数を `k (k≥0)` とする。

- **旧**: line 651 で `preGen = G`。withLaunchLifecycle が invalidate → `G+1`。await 中に `+k` →
  onFailure 時 `current() = G+1+k`。判定 `current() === preGen+1` ⟺ `G+1+k === G+1` ⟺ `k === 0`。
- **新**: invalidate 直後に `launchGen = current() = G+1` を捕捉。`disturbed = () => current() !== launchGen`。
  onFailure 時 `!disturbed()` ⟺ `current() === launchGen` ⟺ `G+1+k === G+1` ⟺ `k === 0`。

**両者は完全に一致**。捕捉点が「呼出し元 line 651（invalidate 前）」から「withLaunchLifecycle 内（invalidate 直後）」
へ移るが、その間（line 651→504）に `searchLane` を触るコードは無い（`clearLaunchNotice`/`setLaunching` のみ）
ため `preGen+1 === launchGen` が厳密に成立。新設計は「呼出し元〜invalidate 間が bump-free」という暗黙依存を
除去する分、**より堅牢**。

## 既存パターン

- `latestRun` は既に lane タスク向けに `isStale()` 述語を配っている（`LatestRunContext.isStale`）。
  今回はその「非 lane 版（launch 用）」を `withLaunchLifecycle` が合成して callback へ渡す ── 既存パターンの延長。
- `disturbed`（world が launch を超えて動いたか）は `isStale`（run token が最新でないか）の launch-lane 版。

## 対称ペア（/symmetric-check 対象）

- `onSuccess` / `onFailure` は withLaunchLifecycle の対称分岐。署名変更はこのペアに一様に及ぶ。
- 3 つの呼出し元（`launchAndReset` / `launchWithSelectedTool` / `executeInstantCommandSelected`）は
  onSuccess/onFailure の署名を共有する ── 署名を変えるなら 3 元すべての callback 型が追従する。
  実際に `disturbed` を消費するのは `executeInstantCommandSelected` の onFailure のみ。

## 同時更新が必要な docs（issue 明記・4 点セット相当）

現状 `current() === captured + 1`（lane 外の保存状態復元）を **正規手段** として記述している 3 箇所:

| 場所 | 現行記述 |
|---|---|
| `ui/CLAUDE.md:109` | 「`searchLane.current()` の比較（例: `executeInstantCommandSelected` の `current() === preGen + 1`）で」 |
| `.claude/rules/ui.md:10` | 「`await` 前に `searchLane.current()` をキャプチャし `current() === captured + 1` の基準値比較で検証」 |
| `.claude/skills/race-check/SKILL.md:85` | 「`await` 前に `searchLane.current()` をキャプチャし ... `current() === captured + 1` 等で ...」 |

判定方式を変えるので、これらを「`withLaunchLifecycle` が `disturbed()` 述語を callback へ配る」記述へ同時更新する。

## 技術的制約

- Win32 依存なし（純粋な TS/SolidJS リアクティブ層の refactor）。IPC 境界も変えない
  （`api.executeInstantCommand` 等のシグネチャは不変）。
- リアクティブ制約: `disturbed` は closure で generation を読む。SolidJS シグナルではなく
  `latestRun` 内の plain `let generation` を `current()` 経由で読むため、リアクティブ購読は発生しない
  （既存 `current()` 読取と同じ・追跡なし）。
- 挙動変更は無い（behavior-preserving refactor）。ただし `withLaunchLifecycle` の**署名**が変わり
  全呼出し元に波及するため、`/simplify` の機械的クリーンアップからは外し独立 PR とする（issue 指示）。

## テスト現況（既存の安全網）

- `search.test.ts:614-629`「失敗: 候補が復元される」= **非 disturbed → 復元する** 正のパス。
- `search.test.ts:1030-1062`「world 世代が進んだら復元しない」= **disturbed → 復元しない** 負のパス
  （実 `withLaunchLifecycle` を経由し、await 中に `enterToolSelection` で世代を進めて検証）。

両テストは実 `withLaunchLifecycle` を通すため、refactor 後も **挙動同一なら green のまま**。
= この refactor の安全網として機能する。コメント中の `preGen+1` 表現のみ新述語へ追随させる。

## 未解決の疑問

- **設計判断（plan で確定）**: `disturbed` を `onFailure` のみに渡すか、`onSuccess` にも渡すか。
  - 消費者は `executeInstantCommandSelected` の onFailure のみ（YAGNI 観点では onFailure だけ）。
  - issue proposal は「`onFailure`/`onSuccess` へ ... 述語を渡せば」と両方を名指し。対称ペア観点でも両方。
  - → plan.md で判断し `/plan-review` に諮る。
