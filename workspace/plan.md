# Plan — results-sync 分割で selection-only IPC を軽量化 (#162)

## 変更ファイル一覧

1. **`ui/src/lib/searchEvents.ts`** — 型定義を3イベント分に変更
2. **`ui/src/stores/search.ts`** — `emitResults` / `emitSelectionUpdate` を3つの emit 関数に置き換え
3. **`ui/src/components/ResultsWindow.tsx`** — `results-sync` リスナーを3イベントリスナーに分割
4. **`ui/src/App.tsx`** — `results-sync` リスナーを3イベントリスナーに分割（controller 連携）
5. **`ui/src/lib/resultsWindowController.ts`** — `handleResultsSync` を分割して selection-only / visibility-only ハンドラを追加
6. **`ui/src/stores/search.test.ts`** — テストのイベント名・ペイロード検証を更新
7. **`CLAUDE.md`** — `results-sync` 1本ルールを3イベント分割ルールに更新
8. **`ui/CLAUDE.md`** — マルチウィンドウ通信の不変条件を更新

## 実装順序

### Phase 1: 型定義の更新 (`searchEvents.ts`)

- `ResultsSyncPayload` を廃止し、3つのペイロード型を定義:
  - `ResultsDataPayload`: `{ generation, results, selected, shouldShow, reason }`
  - `ResultsSelectionPayload`: `{ generation, selected }`
  - `ResultsVisibilityPayload`: `{ generation, shouldShow, reason }`

### Phase 2: 送信側の分割 (`search.ts`)

- `emitResults` を3つの関数に分割:
  - `emitDataChanged(items, selectedIndex, generation, reason)`: `emit("results-data-changed", ...)`
  - `emitSelectionChanged(selectedIndex, generation)`: `emit("results-selection-changed", ...)`
  - `emitVisibilityChanged(shouldShow, generation, reason)`: `emit("results-visibility-changed", ...)`
- 既存の `emitResults` 呼び出し 14 箇所を適切な関数に置き換え:

| 行 | 現在の reason | shouldShow | 置き換え先 |
|----|-------------|-----------|-----------|
| L71 (clearCommandModeStateAndEmit) | command | false | `emitVisibilityChanged` |
| L119 (refreshResults/slash_r) | query | items.length > 0 | `emitDataChanged` |
| L127 (refreshResults/slash_noop) | command | false | `emitVisibilityChanged` |
| L145 (refreshResults/indexing) | reset | false | `emitVisibilityChanged` |
| L192 (refreshResults/normal) | query | items.length > 0 | `emitDataChanged` |
| L242 (query effect/slash_noop) | command | false | `emitVisibilityChanged` |
| L268 (emitSelectionUpdate) | selection | results.length > 0 | `emitSelectionChanged` |
| L317 (exitFolderExpansion) | query | savedResults.length > 0 | `emitDataChanged` |
| L407 (launchWithSelectedTool/pre) | launch | false | `emitVisibilityChanged` |
| L436 (launchWithSelectedTool/post) | launch | false | `emitVisibilityChanged` |
| L484 (enterToolSelection) | query | true | `emitDataChanged` |
| L499 (exitToolSelection) | query | savedResults.length > 0 | `emitDataChanged` |
| L515 (launchAndReset/pre) | launch | false | `emitVisibilityChanged` |
| L538 (launchAndReset/post) | launch | false | `emitVisibilityChanged` |

- `emitSelectionUpdate` を `emitSelectionChanged` に変更

### Phase 3: 受信側の分割 — ResultsWindow (`ResultsWindow.tsx`)

- `listen("results-sync")` を3つのリスナーに分割:
  - `results-data-changed`: `setResults` + `fetchIcons` + `setSelected` + スクロール + `results-render-done` emit
  - `results-selection-changed`: `setSelected` + スクロール + `results-render-done` emit
  - `results-visibility-changed`: リスナー不要（ResultsWindow は表示/非表示を自分で管理しない）
- generation stale 判定は各リスナーで維持

### Phase 4: 受信側の分割 — App.tsx + Controller

- `App.tsx`: `listen("results-sync")` → 3つのリスナーに分割
  - `results-data-changed` → `controller.handleDataChanged(payload)`
  - `results-selection-changed` → no-op（ウィンドウサイズ/位置に影響なし）
  - `results-visibility-changed` → `controller.handleVisibilityChanged(payload)`
- `resultsWindowController.ts`:
  - `handleResultsSync` → `handleDataChanged` + `handleVisibilityChanged` に分割
  - `handleDataChanged`: 現在の handleResultsSync の shouldShow=true パス（サイズ計算 + 表示）
  - `handleVisibilityChanged`: shouldShow=false 時の非表示処理

### Phase 5: テスト更新 (`search.test.ts`)

- `emit('results-sync')` の検証を3イベントに合わせて更新
- **新規テスト**: `emitSelectionUpdate` 呼び出し時に `results-data-changed` が emit されず `results-selection-changed` のみ emit されることを検証

### Phase 6: ドキュメント更新

- `CLAUDE.md`: 「`results-sync` 1本で扱い」→ 3イベント分割ルールに更新
- `ui/CLAUDE.md`: マルチウィンドウ通信セクションのイベント名を更新

## 不変条件

1. **generation による stale 判定は3イベントすべてで維持する**: data/selection/visibility いずれも `generation < latestGeneration` なら無視
2. **selection-changed では結果配列を送らない**: ペイロードは `{ generation, selected }` のみ
3. **visibility-changed では結果配列を送らない**: ペイロードは `{ generation, shouldShow, reason }` のみ
4. **data-changed は常に shouldShow を含む**: 受信側でサイズ計算 + 表示/非表示を1回の処理で決定
5. **results-render-done は data-changed と selection-changed の両方で emit**: perf 計測を維持

## テスト方針

- 既存テスト: `npm test` で全パス確認
- 新規テスト: selection-only 経路で `results-data-changed` が emit されないことを検証
- ビルド検証: `npm run build`（プロジェクトルートから）
- 型チェック: `npm run typecheck`（PostToolUse フックが自動実行）

## SPEC.md 更新要否

**更新必要**。SPEC.md §3.7「結果表示同期契約（results-sync）」を3イベント分割に合わせて更新する:
- `results-sync` イベント1本 → 3イベント（`results-data-changed` / `results-selection-changed` / `results-visibility-changed`）
- 各ペイロードの定義を記載
- `shouldShow` は `results-data-changed` と `results-visibility-changed` で使う

## セルフレビュー

### 1. 対称コードパス
- `emitResults` の全 14 箇所を列挙し、data/selection/visibility に分類済み
- `moveSelectionUp` / `moveSelectionDown` は対称ペアで両方 `emitSelectionUpdate` を呼ぶ — 同一変更でカバー

### 2. 影響範囲の網羅性
- 送信側 (`search.ts`): 14 箇所すべてを分類
- 受信側: `ResultsWindow.tsx` と `App.tsx` の2箇所を確認
- `resultsWindowController.ts` の `handleResultsSync` も対応必要 — 計画に含む
- perf.ts は `results-render-done` のみに依存し、`results-sync` を直接参照しない — 変更不要

### 3. 境界条件
- generation stale 判定: 3イベントが異なる順序で到着した場合 → 同一 generation なので問題なし（selection-changed は data-changed と同じ generation を使う）
- data-changed の shouldShow=false ケース: 現状存在しない（shouldShow=false は常に results=[] で visibility-changed に分類）

### 4. リソース管理
- 新規リスナー追加: `listen()` を3つに増やすため、`onCleanup` / `unlisten` も3つ必要
- App.tsx の `Promise.all` で一括登録しているため、3イベント分に拡張

### 5. 既存パターンとの整合
- イベント分割は issue が明示的に指示しているため、CLAUDE.md の「1本ルール」を更新して整合させる

### 6. YAGNI 違反
- なし。issue の要求範囲内

### 7. シンプル化の挑戦
- 3イベント分割は issue 指定だが、実装上は `emitDataChanged` が shouldShow を含むため visibility-changed との重複がある。ただし shouldShow=false 時に配列を送らない目的には必要な分割。
- `resultsWindowController` の selection-changed ハンドラは no-op — App.tsx でリスナー登録自体を省略できる

### 8. 破壊不変条件の明示
- **generation 整合性**: 3イベントが同じ generation カウンタを共有するため、stale 判定が破綻するリスクはない
- **表示/非表示の整合性**: visibility-changed でのみ非表示にし、data-changed でのみ表示する設計により、表示状態の競合は発生しない
