# Issue #130 調査メモ

## Issue の要約

Tauri + WebView2 のメモリ消費量を削減したい。完全な egui 移行ではなく、Tauri を維持したまま不要なウィンドウを削減するアプローチを取る。Issue の考慮事項に以下が明記されている:

- 機能的に重要ではないウィンドウは「開くたびに生成→閉じたら破棄」にする
- バージョン表示（about）は都度生成でよい
- スラッシュコマンドの案内も削ってよい

## 現状のウィンドウ構成

起動時に 4 つの WebView2 ウィンドウを一括生成（`visible: false`）:

| ウィンドウ | 用途 | 使用頻度 | 常駐の必要性 |
|-----------|------|---------|-------------|
| `main` | 検索入力 | 常時 | **必須** |
| `results` | 検索結果 | 検索のたび | **必須**（レイテンシ重要） |
| `about` | バージョン表示 | 極稀 | **不要** |
| `settings` | 設定画面 | 稀 | **不要** |

各ウィンドウは同一 `index.html` を読み込み、`getCurrentWindow().label` でコンポーネントを出し分ける SPA 構成。

## 関連コード

### Rust 側
- `src-tauri/src/main.rs:280-282` — `ensure_window_with_timing` で about/settings を起動時に事前生成
- `src-tauri/src/commands/window.rs:33-66` — `ensure_about_window`: WebView 生成 + CloseRequested で hide
- `src-tauri/src/commands/window.rs:68-96` — `ensure_settings_window`: 同上 + "Keep window alive" コメント
- `src-tauri/src/commands/window.rs:160-180` — `open_about`: ensure → show → setFocus
- `src-tauri/src/commands/window.rs:124-158` — `open_settings`: ensure → 位置復元 → show → setFocus

### JS 側
- `ui/src/components/AboutWindow.tsx` — 静的コンテンツ、状態なし
- `ui/src/components/SettingsWindow.tsx` — `settings-shown` イベントで config リロード、`onMount` は一度のみ
- `ui/src/lib/commands.ts` — スラッシュコマンド定義（`/a`, `/o`, `/r`, `/s`, `/q`）
- `ui/src/stores/search.ts:79-87` — `showCommandResults`: コマンド候補を results ウィンドウに表示

### SPEC.md
- §6.1: 固定ウィンドウは起動時に一括事前生成
- §7.5: about/settings は close で hide、destroy しない
- §14.3: `/` 入力でコマンド候補一覧を results ウィンドウに表示

## 既存パターン

- `ensure_*_window` 関数は既に冪等設計（存在チェック → なければ生成）
- `open_about` / `open_settings` は内部で `ensure` を呼んでから show するので、起動時の事前生成を削除しても動作する
- `CloseRequested` の `prevent_close` → `hide` パターンは、都度破棄に変更可能

## 技術的制約

- WebView2 初期化には数百 ms かかるため、`results` は都度生成に向かない
- `settings` は初回起動時（`is_first_run`）に即座に表示するため、そのパスでは ensure が間に合う必要がある
- about/settings の close 時に `main` の `alwaysOnTop` を復元するロジックは、ウィンドウが破棄される場合にも正しく動作する必要がある
- スモークテストが `main:ensure_window:ok` に加えて `results/about/settings` の存在を検証している可能性 → 確認要

## 未解決の疑問

- スモークテスト（`scripts/smoke-startup.ps1`）が about/settings の起動時生成を前提としているか → 変更が必要かもしれない
- settings の都度生成による初回表示レイテンシが許容範囲か → about は確実に許容、settings は WebView init が入るので要検討
