# research — issue #535 起動レーンの `activationInFlight` を `exclusive`(mutex) primitive に集約

## issue の要約

`ui/src/stores/search.ts` の起動（launch/activate）経路が二重起動防止のために持つ module スコープの可変 boolean
`activationInFlight` を、4 箇所で手書き反復している
`if (activationInFlight) return false; activationInFlight = true; try { ... } finally { activationInFlight = false; }`
から、`exclusive`(mutex / single-flight) primitive へ集約する。

- **検索 lane（#534/#540 で完了）= supersede**（新しい実行が古い実行を無効化）→ `createLatestRun()`
- **起動 lane（本 issue）= mutex / single-flight**（実行中は 2 つ目を拒否し `false` を返す）→ `createExclusive()`

この 2 方針を別名 primitive として明示するのが「search.ts 抽象化プログラム」の核。本 issue はその **項番 2**、#540 の姉妹 primitive。純粋 refactor（挙動不変・公開 API 不変）。

## 関連コード

### 本体: `ui/src/stores/search.ts`

- `activationInFlight` 宣言: L64（`let activationInFlight = false;`）
- 手書き try/finally の 4 箇所:
  1. `launchWithSelectedTool()` L509–548 — ガード L510 → `frame` チェック L511 → set L514 → try/finally L515–547
  2. `executeInstantCommandSelected()` L630–684 — ガード L631 → set L632 → try/finally L633–683
  3. `activateSelected()` L702–718 — `tryModalActivate()` L703 → ガード L705 → set L706 → try/finally L707–717
  4. `activateSelectedByIndex()` L720–738 — `tryModalActivate(index)` L721 → ガード L723 → set L724 → try/finally L725–737
- `tryModalActivate(index?)` L689–700 — tool/instant なら対応ディスパッチを返す。コメント L687–688 に
  「`activationInFlight` ガードより前に呼ぶこと」の順序契約が明記されている。
- `withLaunchLifecycle()` L486–507 — 起動フロー共通骨格。`setLaunching(true/false)` を持つ（UI 表示レイヤー、mutex とは独立。コメント L484–485 が「activationInFlight ガードは各呼び出し元が個別管理」と明記）。

### 呼び出し関係（実測）

- `activateSelected` は `SearchWindow.tsx:227`（Enter）と `enterToolSelection`（L565、tools≤1 のフォールバック）から呼ばれる。exported。
- `activateSelectedByIndex` は `MainApp.tsx:293`（クリック起動）から。exported。両者とも `.then((launched: boolean) => ...)` で boolean を消費 → **boolean 戻り契約の維持が必須**。
- `launchWithSelectedTool` / `executeInstantCommandSelected` は **`tryModalActivate` からのみ**呼ばれる private 関数。直接呼び出し無し。
- `tryModalActivate` は `activateSelected` / `activateSelectedByIndex` からのみ。

### 入れ子構造（自己ブロック回避の核）

```
activateSelected / activateSelectedByIndex
  → tryModalActivate()            ← ガード/set より前に呼ぶ（現状はコメントで固定）
       → (tool)    launchWithSelectedTool()      ← 自前のガード/set
       → (instant) executeInstantCommandSelected() ← 自前のガード/set
       → (通常)    null を返す
  → （modal===null のときだけ）ガード/set → launchAndReset()
```

- modal 経路（tool/instant）と通常経路は**排他**。`tryModalActivate` が非 null を返すと外側は早期 return し、外側は決してガード/set を実行しない。
- ゆえに **1 ユーザー操作で mutex を取る箇所は常に 1 つ**。単一の共有 mutex で足り、自己ブロックは「順序」で回避されている（再入は不要）。
- `enterToolSelection` は mutex を保持しないため、そこから `activateSelected` を呼んでも入れ子ロックにならない。

## 既存パターン（再利用元）

### 姉妹 primitive: `ui/src/lib/latestRun.ts`（#534/#540）

- `createLatestRun(): LatestRun` — `run` / `invalidate` / `current` を持つオブジェクトを返す純粋ファクトリ（SolidJS/api 非依存）。
- 設計思想: **world 世代のみを所有し、flush 追跡はあえて含めない**（関心の絞り込み）。
- `run` は task を**同期起動**する（bump 直後・最初の await 前に本体が走ることで、呼び出し側が同期に読むシグナルの
  キャプチャタイミングを保つ）。同期 throw は `Promise.reject` へ正規化。
- テスト `ui/src/lib/latestRun.test.ts` — `makeGate()` で await 中の task を外部解放し supersede 順序を検証。
  current 単調前進 / requestId 相関 / isStale / async throw / 同期 throw 正規化 を網羅。

`exclusive` はこの品位に揃える: **in-flight フラグだけを内包する純粋ファクトリ**。ただし戻りは
オブジェクトではなく**単一の callable**（issue の疑似シグネチャ通り。single-flight runner ゆえ最小）。

### `exclusive` の疑似シグネチャ（issue 提案）

```ts
type Exclusive = <T>(task: () => Promise<T>) => Promise<T | undefined>; // 実行中は undefined
const activationLane = createExclusive();

// Before → After（4 箇所）
if (activationInFlight) return false;
activationInFlight = true;
try { /* ... */ } finally { activationInFlight = false; }
// ↓
return (await activationLane(async () => { /* ... */ })) ?? false;
```

`?? false` の意味論: blocked → `undefined ?? false === false`、task が `false` を返す（起動対象なし）→ `false ?? false === false`、
task が `true` → `true ?? false === true`。**現挙動（blocked も「対象なし」も false）を完全維持**。
task は決して `undefined` を返さない（全経路 boolean）ため、`undefined` は一意に「blocked」を意味する。

## 技術的制約

- **Win32 API 依存なし**（純粋 TS/SolidJS の並行制御。`SendInput`/`SetForegroundWindow` 等の非同期性は無関係）。
- **同期起動タイミング**: `async (task) => { if (inFlight) return undefined; inFlight = true; try { return await task(); } finally { inFlight = false; } }`
  の形なら、`activationLane(fn)` 呼び出し時に body が第一 await（`await task()`）まで同期実行され、`task()` の同期プレフィックスも
  同期実行される。`executeInstantCommandSelected` の `preGen = searchLane.current()` 捕捉・`selected()` 読みが
  現行と同じ tick で走る（挙動不変の要）。
- **async 関数の同期 throw は起きない**: 4 task はすべて `async () => {}`（呼ぶと必ず promise を返す）。よって
  `latestRun` のような同期 throw 正規化は必須ではないが、primitive 側で `try/finally` を使えば sync/async どちらの
  throw でも in-flight は解放され例外は伝播する（頑健性のため primitive 単体テストで両方担保する）。
- **リアクティブ制約**: `activationLane` は module スコープの単一インスタンス（`searchLane` と同じ位置づけ）。
  SolidJS のリアクティブ文脈外の素の runner。

## 波及先の確認

- **SPEC.md**: JS の activation mutex は**未記載**（L409 は Rust の `SettingsProcessState` Mutex で別物）。
  挙動不変ゆえ **SPEC 更新不要**。
- **`ui/CLAUDE.md`**: `search.ts` の「横断規約の choke point」列（`searchLane` を挙げる箇所）と `lib/` セクションに
  `exclusive.ts` を追記する必要あり（AGENTS.md「ファイル追加 → サブディレクトリ CLAUDE.md のモジュール構成更新」）。
- **`.claude/skills/race-check/SKILL.md`**: L43/61/99/112 が `activationInFlight` を「module スコープ変数」「再入ガード」の
  **例示**として参照。refactor 後は `activationLane`(Exclusive) になり例が stale 化する。
  **これはエージェント設定（スキル）ゆえ、チーム憲章「設定変更は合意してから」に従い単独編集しない。plan で要相談として提示する。**
- **テスト**: `search.test.ts` に既存の「二重起動 blocked」テストは**無い**（grep 済み）。本 refactor で追加する
  （回帰網 兼カバレッジ向上）。`launchWithTool` / `launchItem` / `executeInstantCommand` は既にモック済み。

## 未解決の疑問

- なし（設計の要である「入れ子の扱い」は「呼び出し順の維持（再入なし）」で確定。下記 plan 参照）。
- 唯一の判断保留: `race-check/SKILL.md` の例示更新はユーザー合意が要る（実装時に確認）。
