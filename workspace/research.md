# Research — results-sync 分割で selection-only IPC を軽量化 (#162)

## issue の要約

矢印キー移動・hover 時の `emitSelectionUpdate()` が `results-sync` イベントで結果配列全体をシリアライズして送っている。ResultsWindow 側は `isSelectionOnly` 判定で結果本体を無視しているが、送信側は不要なデータを送り続けている。これを分割して選択変更時には結果配列を送らないようにする。

## 関連コード

### 送信側: `stores/search.ts`

- `emitResults()` (L29-45): すべてのケースで `emit("results-sync", { generation, results, selected, shouldShow, reason })` を呼ぶ
- `emitSelectionUpdate()` (L263-272): `moveSelectionUp/Down` から呼ばれ、`emitResults(results(), ...)` で結果配列全体を送る — **これが最適化対象**
- `emitResults` の呼び出し箇所は 14 箇所（L71, L119, L127, L145, L192, L242, L268, L317, L407, L436, L484, L499, L515, L538）

### 受信側 1: `components/ResultsWindow.tsx`

- L147-182: `listen("results-sync")` で受信。L151-158 で `isSelectionOnly` 判定済み:
  - `reason === "selection" && generation === latestGeneration` なら `setResults` / `fetchIcons` をスキップし、`setSelected` のみ実行
  - **すでに selection-only 最適化を受信側で行っている**

### 受信側 2: `App.tsx` → `resultsWindowController.ts`

- App.tsx L79: `listen("results-sync")` → `controller.handleResultsSync(event.payload)`
- `resultsWindowController.ts` L84-162: `handleResultsSync` は `results.length` でウィンドウサイズ計算、`shouldShow` で表示/非表示制御
  - selection-only の場合、結果数は変わらないので count は不変 → サイズ変更不要

### 型定義: `lib/searchEvents.ts`

- `ResultsSyncPayload`: `{ generation, results, selected, shouldShow, reason }`
- `ResultsPresentationReason`: `"query" | "reset" | "launch" | "command" | "selection"`

### テスト: `stores/search.test.ts`

- L250-274: `resetForShow` テストで `results-sync` emit の有無を検証

### パフォーマンス計測: `lib/perf.ts`

- `perfMarkRenderDone` は `results-render-done` イベント（ResultsWindow から emit）で呼ばれる
- selection-only の場合も `results-render-done` は emit される（ResultsWindow L176-179）

## 既存パターン

- ResultsWindow は既に `isSelectionOnly` 分岐を持つ — 受信側最適化済み
- `searchGeneration` による stale 判定は全イベントで共通

## 技術的制約

- `results-sync` は CLAUDE.md で「1本で扱い、`results-updated` / `results-count-changed` を新規実装で使わない」と規定されている → **issue が明示的に分割を指示しているため、この制約を SPEC.md/CLAUDE.md 上で更新する**
- Tauri イベント (`emit`/`listen`) はシリアライズ・デシリアライズを伴う。配列を含まないペイロードにすれば IPC コストが大幅に下がる
- `resultsWindowController` は `results.length` を使うが、selection-only 時は結果数が変わらないため不要

## 対称ペア分析

- `emitResults` の呼び出しを reason 別に分類:
  - **data-changed** (結果が変わった): L119, L192, L317, L484, L499 — reason=query, results 配列あり, shouldShow=true
  - **visibility-changed** (表示/非表示が変わった): L71, L127, L145, L242, L407, L436, L515, L538 — shouldShow=false, results=[]
  - **selection-changed** (選択のみ): L268 (emitSelectionUpdate 内) — reason=selection

## 設計判断

issue 指定の3イベント分割:
1. `results-data-changed`: 結果配列 + selected + shouldShow + generation — 結果が変わったとき
2. `results-selection-changed`: selected + generation のみ — 選択だけ変わったとき（配列不要）
3. `results-visibility-changed`: shouldShow + generation — 非表示にするとき（配列不要）

現状の `shouldShow=false` ケースは常に `results=[]` なので、visibility-changed では結果配列は不要。
