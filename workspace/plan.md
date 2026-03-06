# Plan: Issue #153 — UI 部分の最適化

## 方針

Safe と判定された7項目のみ対象とする。制御フローの変更を伴う項目（`refreshResults` エクスポート問題、`resultsWindowController` の Promise 構造）は今回のスコープ外。

## 変更ファイル一覧

| # | ファイル | 変更内容 |
|---|---|---|
| 1 | `stores/search.ts` | `cancelDebounce()` ヘルパー抽出（5箇所の重複解消） |
| 2 | `stores/search.ts` | `setLaunchNoticeWithAutoClear` で `clearLaunchNotice()` を再利用 |
| 3 | `lib/searchEvents.ts` | `ResultsRenderDonePayload` 型を追加 |
| 4 | `App.tsx` | `ResultsRenderDonePayload` のローカル定義を削除、import に変更。`getCurrentWindow()` 呼び出しを1回に統合 |
| 5 | `stores/folder.ts` | 未使用の `isInFolderMode()` を削除 |
| 6 | `lib/pathQuery.ts` | 到達不能な `lastSlash < 0` ガードを削除 |
| 7 | `lib/truncatePath.ts` | キャッシュ溢れ時に全クリアではなく最古エントリ削除に変更 |

## 実装順序

### Phase 1: テスト確認

既存テストで十分カバーされているため、新規テストの追加は不要:
- `stores/search.test.ts` (19 tests) — debounce・通知・クエリ効果
- `lib/pathQuery.test.ts` (9 tests) — パスクエリ解析
- `lib/truncatePath.test.ts` (10 tests) — パス省略

### Phase 2: 実装（7項目、依存なし）

1. `stores/search.ts` — `cancelDebounce()` 抽出 + `setLaunchNoticeWithAutoClear` 修正
2. `lib/searchEvents.ts` + `App.tsx` — 型移動 + `getCurrentWindow()` 統合
3. `stores/folder.ts` — `isInFolderMode()` 削除
4. `lib/pathQuery.ts` — 到達不能ガード削除
5. `lib/truncatePath.ts` — キャッシュ eviction 改善

### Phase 3: 検証

- `npx vitest run` — 全テスト通過
- `npm run build` — typecheck + vite build 通過

## 不変条件

- `cancelDebounce()` は `cancelAnimationFrame` + `undefined` 代入を原子的に行う。呼び出し元の制御フローは変更しない
- `setLaunchNoticeWithAutoClear` で `clearLaunchNotice()` を呼ぶと一瞬 `setLaunchNotice(null)` が走るが、直後に `setLaunchNotice(message)` で上書きされる。SolidJS はバッチ更新するため中間レンダリングは発生しない
- `truncatePath` のキャッシュ eviction 変更は出力値を変えない。キャッシュヒット率のみ改善
- `ResultsRenderDonePayload` の型移動は runtime に影響しない（型のみ）

## テスト方針

検証コマンド: `npx vitest run` + `npm run build`

## SPEC.md 更新要否

不要。ユーザー向け挙動の変更なし。

## セルフレビュー

1. **対称コードパス**: `cancelDebounce` は全5箇所で使われる。`debouncedRefresh` (タイマー設定) と `cancelDebounce` (タイマー破棄) が対称ペアとして明確化される
2. **影響範囲の網羅性**: `isInFolderMode` の grep 結果は定義箇所のみ（0 import）。削除安全
3. **境界条件**: `truncatePath` のキャッシュ eviction で `keys().next().value` が `undefined` になるケースは `size >= MAX` ガードにより到達不能
4. **リソース管理**: 該当なし。新規リソース導入なし
5. **既存パターンとの整合**: `cancelDebounce` は `clearLaunchNotice` と同じ「タイマー破棄ヘルパー」パターン
6. **YAGNI 違反**: なし。全項目が既存コードの整理のみ
7. **シンプル化の挑戦**: 新たな状態・抽象・インターフェースを導入しない。既存コードの削減のみ
8. **破壊不変条件の明示**: `clearLaunchNotice()` 呼び出し追加で `setLaunchNotice(null)` が一瞬走る点が唯一のリスク。SolidJS のバッチ更新で中間レンダリングが発生しないことを確認済み
