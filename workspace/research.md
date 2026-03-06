# Research: Issue #153 — UI 部分の最適化

## Issue の要約

度重なる改修で混入した冗長コードを整理する。分岐を減らし、ネストを浅くし、可読性を維持する。修正にはテストを先行させる。

## コード品質分析結果

### Safe（テスト不要で安全に修正可能）

| # | ファイル | 行 | 問題 | 内容 |
|---|---|---|---|---|
| 1 | `stores/search.ts` | L217-451 | DRY 違反 | `cancelAnimationFrame(debounceTimer); debounceTimer = undefined` が5箇所に重複 |
| 2 | `stores/search.ts` | L47-67 | DRY 違反 | `setLaunchNoticeWithAutoClear` 内のタイマー破棄が `clearLaunchNotice` と重複 |
| 3 | `App.tsx` | L20-22 | 型の配置 | `ResultsRenderDonePayload` がローカル定義。`searchEvents.ts` に移すべき |
| 4 | `App.tsx` | L25, 29-30 | 冗長 | `getCurrentWindow()` を2回呼び、`windowLabel` / `label` に分離。1回で済む |
| 5 | `stores/folder.ts` | L14-16 | Dead code | `isInFolderMode()` がエクスポートされているが未使用 |
| 6 | `lib/pathQuery.ts` | L46-48 | Dead code | `lastSlash < 0` ガードが到達不能（直前の `includes("\\")` で保証済み） |
| 7 | `lib/truncatePath.ts` | L24-129 | 非効率 | キャッシュ溢れ時に全クリア → 最古エントリ削除に変更 |

### Needs-test（テスト追加が必要）

| # | ファイル | 行 | 問題 | 内容 |
|---|---|---|---|---|
| 8 | `stores/search.ts` | L649 | 構造 | `refreshResults` がエクスポートされ、テストから直接呼ばれるが `runRefresh` のスタック追跡を迂回する |

### 情報のみ（今回対象外）

| # | ファイル | 問題 | 理由 |
|---|---|---|---|
| 9 | `resultsWindowController.ts` | Promise 割り当てのエラーリカバリ | 現在の happy path に影響なし。エラーパスの挙動変更はリスクが高い |
| 10 | `ResultsWindow.tsx` | onCleanup 配置の可読性 | `ui/CLAUDE.md` の不変条件に準拠済み。パターン変更は不要 |

## 関連コード

### 変更対象

| ファイル | 役割 |
|---|---|
| `ui/src/stores/search.ts` | 検索状態管理。debounce タイマー、通知タイマー |
| `ui/src/App.tsx` | ウィンドウルーティング、イベント初期化 |
| `ui/src/stores/folder.ts` | フォルダモード状態管理 |
| `ui/src/lib/pathQuery.ts` | パスクエリ判定ロジック |
| `ui/src/lib/truncatePath.ts` | パス省略表示のキャッシュ |
| `ui/src/lib/searchEvents.ts` | イベントペイロード型定義 |

### 参照のみ（変更なし）

| ファイル | 参照理由 |
|---|---|
| `ui/src/components/SearchWindow.tsx` | `folderState() !== null` の使用箇所確認 |
| `ui/src/components/ResultsWindow.tsx` | onCleanup パターン確認 |

## 既存パターン

- `debouncedRefresh()` は既に `requestAnimationFrame` でタイマーをセットする関数として存在。逆操作（キャンセル）のヘルパーがない
- `clearLaunchNotice()` はタイマーとシグナルの両方をクリアする関数として存在
- `searchEvents.ts` にはイベントペイロード型が集約されている

## 技術的制約

- `search.ts` は SolidJS リアクティブ文脈で動作。`createEffect` 内のタイマー操作は同期的でなければならない
- `truncatePath.ts` の `Map` は V8 の挿入順序保証に依存可能（ES2015 仕様）
- `refreshResults` のエクスポート変更はテストファイル `search.test.ts` に影響する

## 未解決の疑問

- なし
