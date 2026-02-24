# ui

SolidJS + TypeScript フロントエンド。Tauri IPC 経由で Rust バックエンドと通信。

## モジュール構成

- `App.tsx`: ウィンドウラベルで検索/設定を出し分け、テーマ適用、ウィンドウ位置復元
- `components/SearchWindow.tsx`: 検索入力 + キーボードナビゲーション + `/o` コマンド + ドラッグ移動
- `components/ResultRow.tsx`: アイコン + 名前 + パス + フォルダバッジ
- `stores/search.ts`: 検索状態管理（クエリ/結果/選択/フォルダ展開/アイコンキャッシュ）
- `stores/settings.ts`: 設定ドラフト管理
- `lib/invoke.ts`: 型付き Tauri IPC ラッパー
- `lib/theme.ts`: CSS 変数によるテーマ適用
- `lib/types.ts`: TypeScript 型定義の集約先（DRY）

## 実装パターン

- 検索ウィンドウのドラッグ移動は `.search-bar` の `data-tauri-drag-region` 属性で実現。`<input>` には付与しないため入力操作は維持される
- ドラッグ開始時の一時的なフォーカス喪失で `auto_hide_on_focus_lost` が誤発火するため、`onFocusChanged` の非表示処理に 100ms の猶予を設けフォーカス復帰時にキャンセルする設計

## マルチウィンドウ通信の不変条件

`main` と `results` は別 `WebviewWindow` であり JavaScript コンテキストを共有しない。`ResultsWindow` は `results-updated` Tauri イベントのみで状態を受け取る。以下の不変条件を守ること。

- `search.ts` の状態（`results`・`selected`）を変更したとき、`ResultsWindow` への通知が必要な場合は必ず `emitResults()` または `emitSelectionUpdate()` を呼ぶ
- イベントリスナー（`listen()`）を登録したら必ず `onCleanup()` で後始末する。`onCleanup` の登録は `listen()` の呼び出しより前、同期コンテキストで行うこと（`await` や `.then()` の後ではリアクティブコンテキストが失われる）
- 新しいイベントハンドラを追加するとき、対称ペアのハンドラにも同様の処理が必要か確認する（例: `result-clicked` を変更したら `result-double-clicked` も確認する）
- リソース管理（`ResizeObserver`・`listen()` 等）は生成と破棄を近接した独立したクリーンアップとして記述する。複数のリソースを1つの `onCleanup` にまとめると、一方の条件（`if (listRef)` 等）が他方のクリーンアップ登録を阻害する

## 設計上の注意点

### requestId の二重管理

検索リクエストの ID は `App.tsx`（IPC 呼び出し側）と `search.ts`（ストア側）の両方で管理されている。現状は整合しているが、片方だけを変更すると古いレスポンスの破棄ロジックが壊れる。どちらかを変更するときは必ず両方を確認すること。
