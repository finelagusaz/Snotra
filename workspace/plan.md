# Issue #131 実装計画: 設定ウィンドウを egui 別プロセスに切り出す

## 概要

設定画面を egui ベースの別バイナリ (`snotra-settings`) として切り出し、WebView2 制約の根本解消・メモリ削減・責務分離を実現する。

相互依存はファイルシステム (`config.toml`) 1 点のみ。IPC 不要。

## 変更ファイル一覧

### 新規作成

| ファイル | 内容 |
|---------|------|
| `snotra-settings/Cargo.toml` | egui アプリの crate 定義 |
| `snotra-settings/src/main.rs` | エントリポイント、eframe アプリ起動 |
| `snotra-settings/src/app.rs` | `eframe::App` 実装、タブ管理、保存/破棄ロジック |
| `snotra-settings/src/tabs/general.rs` | 全般タブ UI |
| `snotra-settings/src/tabs/search.rs` | 検索タブ UI |
| `snotra-settings/src/tabs/index.rs` | インデックスタブ UI |
| `snotra-settings/src/tabs/visual.rs` | ビジュアルタブ UI |
| `snotra-settings/src/tabs/opener.rs` | オープナータブ UI |
| `snotra-settings/src/tabs/mod.rs` | タブモジュール定義 |
| `snotra-settings/src/font.rs` | 日本語フォント読み込み + システムフォント列挙 |
| `snotra-settings/src/hotkey_input.rs` | ホットキーキャプチャウィジェット |
| `snotra-settings/src/widgets.rs` | 共通ウィジェット（トグル、編集可能リスト等） |

### 変更

| ファイル | 変更内容 |
|---------|---------|
| `Cargo.toml` | workspace members に `snotra-settings` 追加 |
| `src-tauri/src/main.rs` | settings 事前生成削除、first-run → sentinel ファイル、`notify` 監視開始 |
| `src-tauri/src/commands/window.rs` | `open_settings` → snotra-settings プロセス起動、`hide_settings` 削除 |
| `src-tauri/src/commands/config.rs` | `save_config` の設定反映ロジックを抽出→ファイル変更検知ハンドラに移植 |
| `src-tauri/Cargo.toml` | `notify` クレート追加 |
| `src-tauri/src/config_watcher.rs` | 新規: `notify` による config.toml 監視モジュール |
| `src-tauri/tauri.conf.json` | `bundle.externalBin` に snotra-settings 追加、settings ウィンドウ定義削除 |
| `ui/src/App.tsx` | settings 分岐削除 |
| `ui/src/lib/commands.ts` | `/o` → snotra-settings 起動 |
| `SPEC.md` | §6.1, §7.5, §8 更新 |

### 削除

| ファイル | 理由 |
|---------|------|
| `ui/src/components/SettingsWindow.tsx` | egui に移行 |
| `ui/src/components/SettingsGeneral.tsx` | 同上 |
| `ui/src/components/SettingsSearch.tsx` | 同上 |
| `ui/src/components/SettingsIndex.tsx` | 同上 |
| `ui/src/components/SettingsVisual.tsx` | 同上 |
| `ui/src/components/SettingsOpener.tsx` | 同上 |
| `ui/src/components/SettingsEditableList.tsx` | 同上 |
| `ui/src/components/SettingsEditorModal.tsx` | 同上 |
| `ui/src/components/SettingsEditorActions.tsx` | 同上 |
| `ui/src/stores/settings.ts` | 同上 |
| `ui/src/lib/openerGroups.ts` | 同上 |
| `ui/src/styles/settings.css` | 同上 |

計: 12 新規、10 変更、12 削除、7 フェーズ

---

## 実装順序

### Phase 1: snotra-settings crate 骨格 + General タブ（PoC）

目的: egui で最小限の設定画面が動作することを検証する。

**`Cargo.toml`** (ワークスペース):
- `members` に `"snotra-settings"` 追加

**`snotra-settings/Cargo.toml`**:
```toml
[package]
name = "snotra-settings"
version = "0.1.0"
edition = "2021"

[[bin]]
name = "snotra-settings"
path = "src/main.rs"

[dependencies]
snotra-core = { path = "../snotra-core" }
eframe = { version = "0.31", default-features = false, features = ["default_fonts", "glow", "persistence"] }
rfd = "0.15"
```

**`snotra-settings/src/main.rs`**:
- `eframe::run_native()` でアプリ起動
- `snotra_core::config::Config::load()` で設定読み込み
- コマンドライン引数: `--first-run` で初回起動モード、`--tab <name>` で初期タブ指定

**`snotra-settings/src/app.rs`**:
- `SettingsApp` 構造体: `draft: Config`, `saved: Config`, `active_tab: TabId`
- `eframe::App::update()` でサイドバー + コンテンツ + フッター描画
- フッター: 保存ボタン（変更時のみ有効）、破棄ボタン、ステータス表示
- 保存: `Config::save()` 呼び出し、first-run モード時は sentinel ファイル削除
- Escape: 変更なしなら閉じる

**`snotra-settings/src/font.rs`**:
- `C:\Windows\Fonts\YuGothM.ttc` (Yu Gothic) をフォールバック日本語フォントとして読み込み
- システムフォント列挙（`list_system_fonts` 相当）

**`snotra-settings/src/tabs/general.rs`**:
- ホットキー入力（Phase 3 で実装、ここではテキスト入力で代替）
- トグル: hotkey_toggle, show_on_startup, auto_hide_on_focus_lost, show_tray_icon, ime_off_on_show
- 数値: max_results, window_width
- トグル: show_icons

### Phase 2: 残り 4 タブの UI 実装

**`snotra-settings/src/tabs/search.rs`**:
- ドロップダウン: normal_mode, folder_mode
- トグル: show_hidden_system
- 数値: top_n_history
- ドロップダウン: history_normalization
- 数値: fuzzy_history_cap_ratio（history_normalization が Disabled なら無効化）

**`snotra-settings/src/tabs/index.rs`**:
- 編集可能リスト: scan paths
- 各エントリ: パス、拡張子（カンマ区切り）、フォルダ含むトグル
- モーダル: 追加/編集（パスのフォルダピッカー付き）
- 重複パスのマージ（`dedup_scan_paths` 活用）

**`snotra-settings/src/tabs/visual.rs`**:
- プリセットカード（Obsidian, Paper, Solarized, Monokai, +カスタム）
- カラーピッカー × 5（`egui::color_picker`）
- フォント選択ドロップダウン + フォントサイズ
- テーマプレビュー

**`snotra-settings/src/tabs/opener.rs`**:
- 編集可能リスト: opener rules（ターゲット + ツール）
- モーダル: 追加/編集（実行ファイルピッカー付き）
- 並べ替え（↑↓ボタン）

### Phase 3: ホットキーキャプチャ + 共通ウィジェット

**`snotra-settings/src/hotkey_input.rs`**:
- egui の `Response` + `InputState` でキー入力をキャプチャ
- Modifier (Ctrl/Alt/Shift/Win) + メインキー (A-Z, 0-9, Space 等) を検出
- Backspace/Escape でクリア
- 現在のキーコンビネーションを表示

**`snotra-settings/src/widgets.rs`**:
- トグルスイッチ（カスタム描画 or `egui::Checkbox`）
- 「初期設定に戻す」ボタン（2回クリック確認）

### Phase 4: 本体側 — config.toml ファイル変更検知

**`src-tauri/Cargo.toml`**:
- `notify = "7"` 追加

**`src-tauri/src/config_watcher.rs`** (新規):
- `notify::recommended_watcher` で `config.toml` を監視
- 変更検知時の処理（`save_config` から抽出）:
  1. `Config::load()` で新設定読み込み
  2. 旧設定と比較
  3. ホットキー変更 → `PlatformCommand::SetHotkey`（失敗時はログ）
  4. トレイ変更 → `PlatformCommand::SetTrayVisible`
  5. インデックス変更 → `start_index_build`
  6. ビジュアル変更 → `emit("visual-config-changed")`
  7. show_icons 変更 → `emit("show-icons-changed")`
  8. ウィンドウ幅変更 → main/results リサイズ
  9. `Engine::update_config()` で状態更新

**`src-tauri/src/main.rs`**:
- setup 内で `config_watcher::start()` 起動

### Phase 5: 本体側 — snotra-settings プロセス起動 + 初回フロー

**`src-tauri/src/commands/window.rs`**:
- `open_settings` を変更: `snotra-settings.exe` を `std::process::Command` で起動
  - 実行ファイルパスは自バイナリの隣（`env::current_exe().parent()`）
  - 引数: first-run 時は `--first-run`
  - 既にプロセスが存在する場合はフォーカス（プロセス ID を保持して監視）
- `hide_settings` は削除（プロセス終了は snotra-settings 自身が制御）
- `ensure_settings_window` は削除

**`src-tauri/src/main.rs`**:
- setup から `ensure_settings_window` 呼び出しを削除
- first-run 時: sentinel ファイル (`.first_run_pending`) を作成、`snotra-settings --first-run` を起動
- sentinel ファイルの監視: `config_watcher` で `config.toml` 変更検知 + sentinel 不在チェック → index build 開始

**初回起動フロー**:
1. 本体: `Config::is_first_run()` → sentinel ファイル作成 → `snotra-settings --first-run` 起動
2. snotra-settings: 設定を編集・保存 → `config.toml` 書き込み → sentinel ファイル削除 → 終了
3. 本体: `config_watcher` が `config.toml` 変更を検知 → sentinel 不在確認 → `start_index_build`

### Phase 6: フロントエンド・本体クリーンアップ

**削除**:
- `ui/src/components/Settings*.tsx` (9 ファイル)
- `ui/src/stores/settings.ts`
- `ui/src/lib/openerGroups.ts`
- `ui/src/styles/settings.css`
- `ui/src/App.tsx` の settings 分岐

**変更**:
- `ui/src/lib/commands.ts`: `/o` コマンドを `open_settings` IPC に変更（バックエンドがプロセス起動）
- `src-tauri/src/commands/config.rs`: `save_config` IPC 削除（or 縮小: 本体内部用に保持する場合のみ）
- `tauri.conf.json`: settings ウィンドウ定義削除
- `src-tauri/src/commands/mod.rs`: settings 関連コマンドの invoke_handler 登録整理

### Phase 7: SPEC.md 更新 + ビルド統合

**`SPEC.md`**:
- §6.1: ウィンドウ管理の変更（settings は別プロセス）
- §7.5: settings ウィンドウのライフサイクル変更
- §8: トレイアイコン表示条件から settings 事前生成を除外
- §9: 新セクション「設定プロセス連携」追加

**`src-tauri/tauri.conf.json`**:
- `bundle.externalBin` に `snotra-settings` 追加

**`CLAUDE.md`**:
- アーキテクチャセクション更新（2 バイナリ構成）

---

## 不変条件

1. `config.toml` のフォーマットは変更しない（`snotra-core::Config` の Serialize/Deserialize はそのまま）
2. 設定保存後、本体が 1 秒以内に変更を検知して反映する（`notify` の debounce）
3. first-run 時、snotra-settings で保存→本体が index build を開始する
4. snotra-settings がクラッシュしても本体に影響しない
5. ホットキー登録失敗時、前のホットキーを維持する（本体側で処理）
6. `/o`, `/a`, `/s`, `/q`, `/r` の即実行は引き続き動作する
7. hotkey-pressed リスナーの settings 可視チェック: プロセスベースの判定に変更（settings プロセスがフォアグラウンドか）
8. about ウィンドウは現状維持（WebView2 のまま）

---

## テスト方針

### 自動テスト

- `cargo check -p snotra-settings` — 型チェック
- `cargo clippy -p snotra-settings` — lint
- `cargo test -p snotra-core` — 既存テスト維持
- `npm run build` — フロントエンドビルド（settings 削除後）
- `cargo check -p snotra` — 本体型チェック

### 手動検証（Windows 必須）

- `snotra-settings.exe` 単体起動 → 全タブ操作 → 保存 → config.toml 更新確認
- 本体起動中に設定変更 → ホットキー/テーマ/トレイ反映確認
- `/o` → snotra-settings 起動確認
- first-run フロー: config.toml 削除 → 起動 → 設定保存 → index build 開始確認
- snotra-settings 強制終了 → 本体に影響なし確認

---

## セルフレビュー

### 1. 対称コードパス ✅
- `open_settings` / settings 閉じ: プロセス起動 / プロセス終了（自然終了）
- `alwaysOnTop` 復元: settings プロセス起動時に `false`、プロセス終了検知時に `true` 復元。about との相互チェックも必要（about 表示中は復元しない）
- config 変更検知の subscribe / unsubscribe: `notify` watcher は本体ライフサイクルで管理

### 2. 影響範囲の網羅性 ✅
- `ensure_settings_window` 呼び出し元: `main.rs`（削除）、`open_settings`（プロセス起動に変更）、`ensure_window` IPC（settings ケース削除）
- `settings-shown` イベント: emit (`window.rs:150` 削除)、listen (`SettingsWindow.tsx` 削除)
- `hide_settings` IPC: `SettingsWindow.tsx`（削除）、`invoke_handler`（削除）
- `save_config` IPC: 設定反映ロジックを `config_watcher` に移植
- hotkey-pressed の settings 可視チェック: プロセスベース判定に変更が必要
- `open-settings` Tauri イベント: トレイからの起動 → プロセス起動に変更

### 3. 境界条件 ✅
- snotra-settings が既に起動中に `/o` → 既存プロセスにフォーカス（二重起動防止）
- 設定保存と本体の config 読み込みの競合 → atomic write で安全
- notify のイベントが複数回発火 → debounce で対処
- snotra-settings 起動中にホットキー → settings プロセスのフォアグラウンド判定で無視
- config.toml が壊れた場合 → `Config::load()` のフォールバック（既存動作）

### 4. リソース管理 ✅
- `notify::Watcher`: main で生成、アプリ終了で drop
- snotra-settings プロセス: `std::process::Child` を保持、終了を検知
- alwaysOnTop: プロセス起動時に false、終了検知時に true 復元

### 5. 既存パターンとの整合 ✅
- `Config::load()` / `Config::save()` をそのまま活用（新パターンなし）
- atomic write パターンで `notify` と自然に連携
- snotra-core を共有ライブラリとして活用（DRY）

### 6. YAGNI 違反 ✅
- IPC は使わず、ファイルシステム 1 点のみ
- 設定プロセスの状態管理は最小限（draft + saved のみ）
- ホットキーバリデーションは本体側に委譲（案A: KISS）
