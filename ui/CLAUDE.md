# ui

SolidJS + TypeScript フロントエンド。Tauri IPC 経由で Rust バックエンドと通信。

## モジュール構成

### エントリポイント（multi-page build）

- `main.html` / `main.tsx` → `MainApp.tsx`: 検索ウィンドウ用エントリ。テーマ適用、ウィンドウ位置復元、イベントリスナー登録、resultsWindowController 管理
- `results.html` / `results.tsx` → `ResultsApp.tsx`: 結果ウィンドウ用エントリ。テーマ適用のみ。search store を含まない軽量バンドル

### components/

- `SearchWindow.tsx`: 検索入力 + キーボードナビゲーション + スラッシュコマンド補完 + ドラッグ移動
- `ResultsWindow.tsx`: 別 WebviewWindow で動作する検索結果ウィンドウ。`results-data-changed` / `results-selection-changed` イベントを受け取り結果一覧を描画。アイコンのバイナリバッチ取得・Blob URL 管理・スクロール追従を担当
- `ResultRow.tsx`: アイコン + 名前 + パス + フォルダバッジ（1行分の描画）
- `ToggleSwitch.tsx`: トグルスイッチ共通 UI コンポーネント
- `ThemePreview.tsx`: `VisualConfig` を受け取ってテーマの縮小プレビューを描画
- `SettingRow.tsx`: 設定行の共通レイアウト（label + description + control スロット）

### stores/

- `search.ts`: 検索状態管理（クエリ/結果/選択/モード切替/`results-data-changed` / `results-selection-changed` / `results-visibility-changed` emit）
- `folder.ts`: フォルダモードの状態（`FolderFrame` シグナル + `folderFilter`）
- `tool-selection.ts`: ツール選択モードの状態（`ToolSelectionFrame` シグナル）
- `settings.ts`: 設定ドラフト管理

### lib/

- `resultsWindowController.ts`: results ウィンドウの位置・サイズ・表示制御（`createResultsWindowController` ファクトリ）
- `invoke.ts`: 型付き Tauri IPC ラッパー
- `theme.ts`: CSS 変数によるテーマ適用
- `types.ts`: TypeScript 型定義の集約先（DRY）
- `commands.ts`: スラッシュコマンド定義（`/r` `/o` `/s` `/q`）と `SLASH_COMMANDS` 配列・`findCommand()` 関数
- `i18n.ts`: 日本語ローカライズ。`TranslationKey` 型と `t(key, params?)` 関数。`{param}` 形式プレースホルダー対応
- `searchEvents.ts`: `ResultsDataPayload` / `ResultsSelectionPayload` / `ResultsVisibilityPayload` 型定義（3分割イベントのペイロード構造）
- `folderNav.ts`: フォルダナビゲーション純粋ロジック（`computeParentDir`・`clampSelectedIndex`）。ドライブルート・UNC パス対応。テスト可能なため `stores/` から分離
- `pathQuery.ts`: パスクエリ判定ロジック（`parsePathQuery`・`isPathQuery`）。入力がパス形式かを判定しフォルダ参照モードへの切り替えをトリガー
- `hotkeyValidation.ts`: ホットキーの有効性チェック（`isHotkeyInvalid`・`formatHotkeyLabel`）。Win キー・禁止キー・修飾キーなしをガード
- `lruIconCache.ts`: Blob URL 管理付き LRU アイコンキャッシュ（`LruIconCache` クラス）。ResultsWindow で使用
- `truncatePath.ts`: Canvas API でフォント依存のピクセル幅を計測し、長いパスを中間省略する（`truncatePath`）。結果はキャッシュ済み
- `perf.ts`: 開発時専用パフォーマンス計測（`localStorage.snotra_perf=1` で有効化）。入力→検索→描画の3フェーズ時間を計測し P50/P95 を `console.table` 出力
- `trace.ts`: 開発時専用トレースログ（`localStorage.snotra_trace=1` で有効化）。`trace(event, data)` で `console.debug` 出力

## テスト基盤

### 構成

- `vitest.config.ts` に `vite-plugin-solid({ hot: false })` を設定（`hot: false` は Windows の `@solid-refresh` URL 解決エラー回避。macOS でも無害）
- テストファイルパターン: `ui/src/**/*.test.{ts,tsx}`
- デフォルト環境: `node`。コンポーネントテスト（`.test.tsx`）は先頭に `// @vitest-environment jsdom` を付けて個別に jsdom を使用

### 注意点

- **vite-plugin-solid により SolidJS のリアクティブ初期化が走る**: SolidJS モジュール（`search.ts` 等）を import するテストは、`requestAnimationFrame` 等のブラウザ API スタブが必要。`vi.hoisted(() => { globalThis.requestAnimationFrame = ... })` でモジュールロード前に差し込むこと（`vi.stubGlobal` はホイストされないため間に合わない）
- **Canvas API モック**: `truncatePath.ts` のように遅延初期化（初回呼び出しまで `document.createElement` しない）の場合、`vi.stubGlobal("document", { createElement: ... })` を `beforeAll` で設定すれば jsdom 不要でテスト可能
- **コンポーネントテストでは `render(() => <Component />)` を使う**: SolidJS の `render` は関数ラッパーが必須（React と異なり直接 JSX を渡さない）

## 実装パターン

- 検索ウィンドウのドラッグ移動は `.search-bar` の `data-tauri-drag-region` 属性で実現。`<input>` には付与しないため入力操作は維持される
- ドラッグ開始時の一時的なフォーカス喪失で `auto_hide_on_focus_lost` が誤発火するため、`onFocusChanged` の非表示処理に 100ms の猶予を設けフォーカス復帰時にキャンセルする設計

## マルチウィンドウ通信の不変条件

`main` と `results` は別 `WebviewWindow` であり JavaScript コンテキストを共有しない。`ResultsWindow` は `results-data-changed` / `results-selection-changed` Tauri イベントで状態を受け取る。以下の不変条件を守ること。

- `search.ts` の状態（`results`・`selected`）を変更したとき、`ResultsWindow` への通知が必要な場合は必ず `emitDataChanged()` / `emitSelectionChanged()` / `emitVisibilityChanged()` のいずれかを呼ぶ
- イベントリスナー（`listen()`）を登録したら必ず `onCleanup()` で後始末する。`onCleanup` の登録は `listen()` の呼び出しより前、同期コンテキストで行うこと（`await` や `.then()` の後ではリアクティブコンテキストが失われる）
- 新しいイベントハンドラを追加するとき、対称ペアのハンドラにも同様の処理が必要か確認する（例: `result-clicked` を変更したら `result-double-clicked` も確認する）
- リソース管理（`ResizeObserver`・`listen()` 等）は生成と破棄を近接した独立したクリーンアップとして記述する。複数のリソースを1つの `onCleanup` にまとめると、一方の条件（`if (listRef)` 等）が他方のクリーンアップ登録を阻害する

## 設計上の注意点

### result-clicked / result-double-clicked のペイロード型

`result-clicked`（クリック起動）と `result-double-clicked`（行選択）はどちらも **リスト行インデックス（`number`）** をペイロードとして送る。パス文字列をペイロードに使ってはならない。理由: パスは通常検索では一意だが、ツール選択モード中は同一 exe の複数ツールが同じパス（`tool.exe`）を持ちうるため非一意になる。インデックスは全コンテキストで常に一意。

### searchGeneration と windowOpsGeneration の関係

`search.ts` の `searchGeneration`（検索リクエストの stale 判定）と `resultsWindowController.ts` の `windowOpsGeneration`（ウィンドウ操作の stale 判定）は別責務のカウンタであり、`windowOpsGeneration` は `searchGeneration` の派生値。二重管理ではなく、それぞれ独立した責務を持つ。ただし、どちらかの stale 判定ロジックを変更するときは両方に影響しないか確認すること。

### i18n キー設計のルール

- **新キー追加前に既存キーを確認する**: `ui/src/lib/i18n.ts` に新しい `TranslationKey` を追加する前に、同じ文字列値を持つ既存キーがないか確認する。特に `settings.*` 名前空間のキーと機能的に同一の文字列を別名で追加しない
- **動的文字列は `{param}` テンプレートで管理する**: `t("key") + variable` の文字列末尾連結ではなく、`t("key", { param: value })` の `{param}` 置換に統一する。語順が言語によって変わる場合でも対応でき、t() の設計意図と一致する
- **実装しない機能のコメントは書かない（YAGNI）**: i18n モジュールに「将来 locales/ ファイルで上書き可能にする予定」等の未実装計画をコメントで残さない
