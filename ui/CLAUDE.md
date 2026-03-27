# ui

SolidJS + TypeScript フロントエンド。Tauri IPC 経由で Rust バックエンドと通信。

## モジュール構成

### エントリポイント（single-page build）

- `../main.html` / `main.tsx` → `MainApp.tsx`: 検索ウィンドウ用エントリ。テーマ適用、ウィンドウ位置復元、イベントリスナー登録、動的ウィンドウ高さ管理

### components/

- `SearchWindow.tsx`: 検索入力 + キーボードナビゲーション + スラッシュコマンド補完 + ドラッグ移動
- `ResultsSection.tsx`: 検索結果をメインウィンドウ内にインライン描画。`search.ts` のシグナル（`results`・`selected`）を直接参照。アイコンのバイナリバッチ取得・Blob URL 管理・スクロール追従を担当
- `ResultRow.tsx`: アイコン + 名前 + パス + フォルダバッジ（1行分の描画）
- `UpdateToast.tsx`: 自動更新通知トースト（2行 52px）。`canInstall` prop で [今すぐ更新] ボタンの表示を制御
- `ToggleSwitch.tsx`: トグルスイッチ共通 UI コンポーネント
- `ThemePreview.tsx`: `VisualConfig` を受け取ってテーマの縮小プレビューを描画
- `SettingRow.tsx`: 設定行の共通レイアウト（label + description + control スロット）

### stores/

- `search.ts`: 検索状態管理（クエリ/結果/選択/モード切替/`shouldShowResults` メモシグナル）。主要な公開関数: `resetForShow()`（window-shown 時の全状態リセット）、`refreshResults()`（ソースに応じた検索実行）、`initIndexingState()`（起動時のインデックス状態初期化 + `indexing-started` / `indexing-complete` リスナー登録）。`suppressNextQueryEffectRefresh` フラグで query effect の不要な再実行を抑制
- `folder.ts`: フォルダモードの状態（`FolderFrame` シグナル + `folderFilter`）
- `tool-selection.ts`: ツール選択モードの状態（`ToolSelectionFrame` シグナル）

### lib/

- `invoke.ts`: 型付き Tauri IPC ラッパー
- `theme.ts`: CSS 変数によるテーマ適用
- `types.ts`: TypeScript 型定義の集約先（DRY）
- `commands.ts`: スラッシュコマンド定義（`/r` `/o` `/s` `/q`）と `SLASH_COMMANDS` 配列・`findCommand()` 関数。`hideMainWindow()` でメインウィンドウを非表示にする
- `i18n.ts`: 多言語対応（日本語・英語）。`TranslationKey` 型と `t(key, params?)` 関数。`{param}` 形式プレースホルダー対応。SolidJS シグナルで言語を管理し、`setLanguage()` で切替。初期言語は `navigator.language` から同期的に決定（bootstrap 到着前のフラッシュ防止）
- `folderNav.ts`: フォルダナビゲーション純粋ロジック（`computeParentDir`・`clampSelectedIndex`）。ドライブルート・UNC パス対応。テスト可能なため `stores/` から分離
- `hotkeyValidation.ts`: ホットキーの有効性チェック（`isHotkeyInvalid`・`formatHotkeyLabel`）。Win キー・禁止キー・修飾キーなしをガード
- `lruIconCache.ts`: Blob URL 管理付き LRU アイコンキャッシュ（`LruIconCache` クラス）。ResultsSection で使用
- `truncatePath.ts`: Canvas API でフォント依存のピクセル幅を計測し、長いパスを中間省略する（`truncatePath`）。結果はキャッシュ済み
- `perf.ts`: 開発時専用パフォーマンス計測（`localStorage.snotra_perf=1` で有効化）。入力→検索→描画の3フェーズ時間を計測し P50/P95 を `console.table` 出力
- `trace.ts`: 開発時専用トレースログ（`localStorage.snotra_trace=1` で有効化）。`trace(event, data)` で `console.debug` 出力

## インスタントコマンドモード

プレフィックス（デフォルト `@`）で始まる入力はインスタントコマンドモードに入る。`instantCommandMode` シグナルで状態管理。

- **検出**: `query` effect 内でスラッシュコマンド判定より先に `startsWith(prefix)` を評価
- **結果表示**: `getInstantCommands()` IPC で前方一致フィルタ済みコマンドを取得し `SearchResult[]` に変換
- **実行**: `executeInstantCommand(name, query)` IPC。クリップボード読み取り・変数展開・ShellExecuteW はバックエンド側
- **shouldShowResults**: `indexing()` 中でもインスタントコマンドモードなら結果を表示
- **ガード**: ArrowRight/Left（フォルダ展開）、Shift+Enter（ツール選択）、handleInput の indexing ガードをバイパス
- **アイコン**: インスタントコマンドモード中は `skipIcons` prop で取得をスキップ
- **プレフィックス更新**: bootstrap payload + `instant-prefix-changed` イベントで同期

## テスト基盤

### 構成

- `vitest.config.ts` に `vite-plugin-solid({ hot: false })` を設定（`hot: false` は Windows の `@solid-refresh` URL 解決エラー回避。macOS でも無害）
- テストファイルパターン: `ui/src/**/*.test.{ts,tsx}`
- デフォルト環境: `node`。コンポーネントテスト（`.test.tsx`）は先頭に `// @vitest-environment jsdom` を付けて個別に jsdom を使用

### 注意点

- **vite-plugin-solid により SolidJS のリアクティブ初期化が走る**: SolidJS モジュール（`search.ts` 等）を import するテストは、`requestAnimationFrame` 等のブラウザ API スタブが必要。`vi.hoisted(() => { globalThis.requestAnimationFrame = ... })` でモジュールロード前に差し込むこと（`vi.stubGlobal` はホイストされないため間に合わない）
- **`lib/` モジュールが `stores/` を import するようになった場合**: そのモジュールのテストファイルに `vi.mock("../stores/...")` を追加する。追加しないと transitive import 経由で SolidJS のモジュールレベルコードが走り `requestAnimationFrame` 等の未定義エラーが CI で発生する。`vi.hoisted` でモック関数を宣言してから `vi.mock` ファクトリ内で参照するパターンを使う
- **Canvas API モック**: `truncatePath.ts` のように遅延初期化（初回呼び出しまで `document.createElement` しない）の場合、`vi.stubGlobal("document", { createElement: ... })` を `beforeAll` で設定すれば jsdom 不要でテスト可能
- **コンポーネントテストでは `render(() => <Component />)` を使う**: SolidJS の `render` は関数ラッパーが必須（React と異なり直接 JSX を渡さない）
- `cspValidation.test.ts`: Tauri v2 IPC の CSP 検証（`connect-src` に `ipc:` と `http://ipc.localhost` が必要）

### コンポーネントテスト（.test.tsx）のモックパターン

ストアをモック化するコンポーネントテストでは、`vi.mock` ファクトリがホイストされるため、モック関数を `vi.hoisted` 内で宣言する必要がある。ストアテスト（`search.test.ts`）のように `vi.mock` → `import` の順序制御だけでは不十分（コンポーネントの transitive import でモジュールロード順が変わるため）:

```typescript
const { mockFn1, mockFn2 } = vi.hoisted(() => ({
  mockFn1: vi.fn(),
  mockFn2: vi.fn(),
}));
vi.mock("../stores/search", () => ({ fn1: mockFn1, fn2: mockFn2 }));
```

- `SearchWindow.test.tsx` が基盤実装。新しいコンポーネントテストはこのパターンを踏襲する
- Tauri API（`@tauri-apps/api/window`, `@tauri-apps/api/event`）は空モックで無害化

## 実装パターン

- 検索ウィンドウのドラッグ移動は `.search-bar` の `data-tauri-drag-region` 属性で実現。`<input>` には付与しないため入力操作は維持される
- ドラッグ開始時の一時的なフォーカス喪失で `auto_hide_on_focus_lost` が誤発火するため、`onFocusChanged` の非表示処理に 100ms の猶予を設けフォーカス復帰時にキャンセルする設計
- **`async` 関数内で `await` をまたぐ可変変数はローカルキャプチャする**: `let` 変数やモジュールスコープの可変変数を `await` をまたいで参照する場合、関数冒頭で `const` にコピーしてから使う。`await` 中に外部イベントで値が書き換わると後続処理が意図しない値を参照する（例: `const visibleCount = cachedMaxResults`）
- **`await` 後に保存状態を復元する場合は staleness チェックを入れる**: `await` 前に保存した状態を失敗時に復元するパターンでは、`searchGeneration` 等の世代カウンタで「`await` 中に状態が変わっていないか」を検証してから復元する。無条件復元は新しい状態を上書きするレースコンディションを生む。加えて、`await` 中にユーザー入力で状態が変わること自体を防ぐガード（`handleInput` の `launching()` チェック等）を根本対策として併用する
- 検索デバウンスは leading edge（初回即時発火）+ trailing 50ms。`leadingFired` フラグでデバウンス区間の最初の入力を即座に `runRefresh()` し、以降は trailing タイマーのみ。`cancelDebounce()` でフラグもリセットする

## 単一ウィンドウの高さ管理

検索バーと検索結果は1つの Tauri ウィンドウ内に共存する。結果の表示/非表示はシグナルで管理し、ウィンドウ高さは動的に変更する。

- `shouldShowResults` メモシグナル: `results().length > 0 && (!indexing() || instantCommandMode() || folderState() !== null)` — 結果を表示すべきかの判定（インスタントコマンドモード中・フォルダモード中はインデックス構築中でも結果を表示）
- `mainVisible` ローカルシグナル: `window-shown` / `window-hidden` イベントで同期される — ウィンドウが可視かの判定
- `ResultsSection` の `visible` prop: `shouldShowResults() && mainVisible()` — 実際の描画と Blob URL ライフサイクルを制御
- `createEffect` でウィンドウ高さを計算: `shouldShowResults()` が true なら `SEARCH_BAR_HEIGHT + maxResults * RESULT_ROW_HEIGHT + RESULTS_PADDING`、false なら `SEARCH_BAR_HEIGHT`
- Rust 側の `show_main_and_emit` で毎回 52px にリセット → フロントエンドが結果に応じて拡張。これにより表示時の一瞬のフラッシュを防止する

## Blob URL 管理の不変条件

- アイコンの Blob URL は `LruIconCache`（`lruIconCache.ts`）が一元管理する。`URL.createObjectURL` で生成した URL は必ず `cache.set()` または早期リターン時の明示的 `revokeObjectURL` で回収する
- `parseBinaryBatch` で Blob URL を生成した後、`cache.set()` に到達する前に早期リターンするパス（stale guard 等）では、`parsed` 内の全 URL を明示的に `revokeObjectURL` すること
- `ResultsSection` の `visible` prop が `false` になったとき `cache.revokeAll()` + `iconCacheVersion` 更新で Blob URL を一括解放する

## 設計上の注意点

### マウスイベントハンドラ

クリック起動 (`handleClickResult`) とダブルクリック選択 (`handleDoubleClickResult`) はどちらも **リスト行インデックス（`number`）** を引数として受け取る。パス文字列を使ってはならない。理由: パスは通常検索では一意だが、ツール選択モード中は同一 exe の複数ツールが同じパス（`tool.exe`）を持ちうるため非一意になる。インデックスは全コンテキストで常に一意。

マウスホバーは CSS `:hover` による視覚フィードバックのみ。`selected` シグナルは変更しない（キーボードナビゲーションとの干渉を防ぐため）。

### i18n キー設計のルール

- **新キー追加前に既存キーを確認する**: `ui/src/lib/i18n.ts` に新しい `TranslationKey` を追加する前に、同じ文字列値を持つ既存キーがないか確認する。特に `settings.*` 名前空間のキーと機能的に同一の文字列を別名で追加しない
- **動的文字列は `{param}` テンプレートで管理する**: `t("key") + variable` の文字列末尾連結ではなく、`t("key", { param: value })` の `{param}` 置換に統一する。語順が言語によって変わる場合でも対応でき、t() の設計意図と一致する
- **実装しない機能のコメントは書かない（YAGNI）**: i18n モジュールに「将来 locales/ ファイルで上書き可能にする予定」等の未実装計画をコメントで残さない
