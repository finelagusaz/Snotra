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
