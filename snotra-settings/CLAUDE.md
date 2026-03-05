# snotra-settings

egui ベースの設定・about バイナリ crate。本体（`src-tauri`）とは別プロセスで動作する。

## アーキテクチャ

- 本体との連携は `config.toml` ファイル1点のみ。IPC は使わない
- 設定モード（デフォルト）: 5タブの設定エディタ
- about モード（`--about`）: バージョン情報ダイアログ（400×300、リサイズ不可）
- 設定の読み書きは `snotra-core::Config` を直接使用。本体は `notify` クレートで変更を検知する

## モジュール構成

- `main.rs`: エントリポイント、`--about` 引数の解析、eframe 起動
- `app.rs`: `eframe::App` 実装、タブ管理、保存/破棄/リセットロジック
- `about.rs`: about ダイアログ UI
- `font.rs`: 日本語フォント読み込み + システムフォント列挙
- `hotkey_input.rs`: ホットキーキャプチャウィジェット
- `tabs/`: 5タブの UI 実装
  - `general.rs`: 全般設定（起動時表示、トレイ、IME、ホットキー）
  - `search.rs`: 検索設定（検索モード、履歴、隠しファイル）
  - `index.rs`: インデックス設定（スキャンパス管理）
  - `visual.rs`: ビジュアル設定（テーマプリセット、カラーピッカー、フォント）
  - `opener.rs`: オープナー設定（ツール/ルール管理）

## egui 実装の注意点

### API の型に注意

- `egui::Key::ALL` は `&[Key]`（`&&[Key]` ではない）。`for &key in egui::Key::ALL` が正しい
- `color_edit_button_srgba` は `&mut Color32` を取る。一時変数に変換して渡すと変更が反映されない。`let mut color = Color32::from_hex(hex)` のように変数を作り、変更後に hex 文字列に書き戻す
- `egui::Stroke::new()` に `StrokeKind` が必要（egui 0.31+）。`Stroke::new(1.0, color)` ではなく `Stroke { width: 1.0, color, kind: StrokeKind::Middle }` または対応するコンストラクタを使う
- `ThemePreset` は `Copy`。`.clone()` ではなく値コピーで渡す（clippy `clone_on_copy`）

### Win キーの制限

egui の `Modifiers` は `ctrl` / `alt` / `shift` / `mac_cmd` / `command` のみ。Win キーは検出できない。ホットキーキャプチャでは Ctrl/Alt/Shift のみサポートする。デフォルトホットキー `Alt+Q` は問題なく動作する。

### フレームごとの重い処理を避ける

egui は毎フレーム `update()` を呼ぶ（60fps）。`list_system_fonts()` のような Win32 API 呼び出しをフレームごとに実行するとパフォーマンスが劣化する。初期化時に一度だけ取得して `SettingsApp` のフィールドにキャッシュする。

## 開発ルール

- ロジック（Config の読み書き、バリデーション）は `snotra-core` に寄せる。このクレートは UI 層のみ
- 境界チェック: 配列アクセス前に必ずインデックスの有効性を確認する（`if idx < vec.len()`）
- opener のターゲット変更: ツールを旧ルールから削除し、新ルールに追加する。OpenerRule.target を上書きしない（他のツールが巻き添えになる）

## 本体との連携パターン

### 設定保存フロー

1. snotra-settings: `Config::save()` で `config.toml` に書き込み
2. 本体: `config_watcher` が `notify` でファイル変更を検知（100ms debounce）
3. 本体: `apply_config_change()` で差分検出 → ホットキー/トレイ/インデックス/テーマを反映

### 初回起動フロー

1. 本体: `Config::is_first_run()` → `launch_settings_process` で直接起動（`open_settings` の indexing ガードをバイパス）
2. snotra-settings: ユーザーが設定を編集・保存
3. 本体: 監視スレッドがプロセス終了を検知 → `start_index_build` を開始

**注意**: 本体の `open_settings` には `if indexing { return }` ガードがある。初回起動時は `indexing=true` なので、`open_settings` 経由ではなく `launch_settings_process` を直接呼ぶ必要がある。
