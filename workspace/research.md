# research — issue #343: config 退避のトレイ通知 + finding C（save-after-failed-load ガード）

## issue の要約

#338 で `Config::load()` は壊れた `config.toml`（parse 失敗 / 非 UTF-8）を `config.toml.bak` へ退避し既定値で起動するようになった（ログのみ）。本 issue は:

1. **退避をユーザーに可視化**: 起動時・実行中リロード時に退避が起きたら **Win32 トレイバルーン**で通知（「設定の読み込みに失敗し既定値で動作中／`.bak` に退避」）。Ja/En 対応。
2. **finding C（save-after-failed-load ガード）**: 一時的 read 失敗（permission/lock 等）で既定値起動した `snotra-settings` が、ユーザー保存時に実 `config.toml` を既定値で上書きするのを防ぐ。

両者は「`load()` 結果（成功/復旧/読込失敗）を呼び出し側へ surface する」という**同一 seam**で解決する。

## 関連コード（調査済み）

### snotra-core
- `config.rs`: `load()` / `load_from_dir(dir)`（#338 で seam 化済み）/ `save_to_dir(dir)`。`load_from_dir` の分岐（parse OK / parse 失敗→.bak / NotFound→save / InvalidData→.bak / 一時失敗→据え置き）に**結果種別を返す**機能を追加する。

### src-tauri（メインアプリ — バルーン担当）
- `platform/mod.rs`:
  - `PlatformCommand` enum（26-39）: SetHotkey / SetTrayVisible / SetIndexing / TurnOffIme / SetLanguage / RegisterInitialHotkey / Exit。**バルーン用バリアントを追加**。
  - `PlatformBridge::send_command`（94-103）: channel + `PostThreadMessageW(WM_PLATFORM_WAKE)`。
  - `process_commands`（225-294）: トレイスレッドで `tray: &mut Option<TrayIcon>` を保持し各コマンドを処理。**ここにバルーンの arm を追加**し `tray.show_balloon()` を呼ぶ。
- `platform/tray.rs`:
  - `TrayIcon`（`nid: NOTIFYICONDATAW` + `language: Language`）。`create`（147-179）は `uFlags: NIF_ICON|NIF_MESSAGE|NIF_TIP|NIF_SHOWTIP`。
  - メニュー文言は `match self.language { Ja=>.., En=>.. }` インライン（197-208 等）。**バルーン文言も同形式**。
  - `set_language`（143）あり。**`show_balloon()` を追加**（`NIM_MODIFY` + `NIF_INFO` + `szInfo`/`szInfoTitle`/`dwInfoFlags`）。
- `main.rs`:
  - 起動時 `Config::load()`（365）→ `load_reporting()` に変更し結果を捕捉。
  - トレイ生成は `SetTrayVisible(true)`（677、全 setup 後）。**バルーンは 677 より後**に送る。
- `config_watcher.rs`:
  - `apply_config_change`（78〜）が `Config::load()`（79）→ `load_reporting()` に変更。リロードで復旧検知 → バルーンコマンド送信（トレイは実行中存在）。`bridge`（120）経由で送れる。

### snotra-settings（finding C ガード担当）
- `main.rs:20`: `Config::load()` → `load_reporting()` に変更し `LoadOutcome` を `app::run` へ渡す。
- `app.rs`: `draft`/`saved` 二重状態、保存フロー（Save→normalize→validate→`config.save()`、`app.rs:182`）。`ReadFailed` 起因の既定値表示時に保存をガード（警告 or 明示確認）。`i18n.rs`（`Tr(Language)`）に文言追加。

## 既存パターン（再利用）

- **`PlatformCommand` 追加**: SetLanguage/SetIndexing と同形式でバリアント追加 → `process_commands` に arm 追加。新規スレッド・チャネル不要。
- **トレイ Ja/En インライン文言**: `tray.rs` の既存メニュー文言と同じ `match language`。別 i18n モジュール不要（snotra-settings 側は既存 `i18n.rs` の `Tr`）。
- **load 結果 seam**: `load_from_dir` / `save_to_dir`（#338）。
- **snotra-core は UI 文字列を持たない**: `LoadOutcome` は**プレーンな enum（UI 文字列なし）**を返し、文言は src-tauri / snotra-settings 側で組む（`.claude/rules/snotra-core.md`「is_error フラグで伝え表示は UI 層」と整合）。
- **イベント順序**（`src-tauri/CLAUDE.md`）: config_watcher は `language-changed` を他通知より先に emit。バルーンも言語確定後に出す。

## 技術的制約

- **Win32 `NIF_INFO`（バルーン）**: `NOTIFYICONDATAW` の `szInfo`(256)/`szInfoTitle`(64)/`dwInfoFlags`(`NIIF_INFO`/`WARNING`/`ERROR`) を設定し `uFlags |= NIF_INFO` で `Shell_NotifyIconW(NIM_MODIFY)`。`windows` v0.62 の `Win32_UI_Shell` feature は導入済み（`Shell_NotifyIconW`/`NOTIFYICONDATAW` 使用中）。`NIF_INFO`/`NIIF_*` シンボルの正確な名称・可用性を実装時に確認する。
- **トレイ生成タイミング**: 起動時バルーンは `SetTrayVisible(true)`（main.rs:677）後に送る。順序を誤るとトレイ未生成でバルーンが出ない。
- **`show_tray_icon=false` 時はトレイが無い**: バルーンを出せない。ただし**復旧時 config=default=tray ON** なので起動時復旧では問題なし。トレイ無効かつ復旧という組合せは稀。フォールバック（フロントイベント等）は YAGNI として持たない方針を plan で確認。
- **バルーンの非同期性**: `Shell_NotifyIconW(NIM_MODIFY, NIF_INFO)` はトレイスレッドで同期実行（`process_commands` 内）。`SendInput` 系のような非同期問題はない。
- **`LoadOutcome` の伝播**: snotra-core → src-tauri（main + watcher）→ platform thread、および snotra-core → snotra-settings。3 crate に跨る pub enum。
- **モジュール構成ドキュメント同期**: 新規 `.rs` は追加しない見込み（既存ファイルへの追記のみ）。追加する場合は各 `CLAUDE.md` を更新。

## 未解決の疑問（要求判断 — plan で提示）

- **settings GUI の `ReadFailed` 時の保存ガード UX**: 「保存を硬く拒否（成功 load / 明示リロードまで save 不可）」か「警告＋明示確認で保存許可」か。コードからは決まらない製品判断。plan で推奨（警告＋明示確認＝非破壊・ユーザーを詰まらせない）を提示しユーザー確認を仰ぐ。

## SPEC.md 更新要否

- 挙動変更あり（通知・保存ガード）。SPEC に「読み込み失敗の可視化」「読込失敗時の保存ガード」を追記する（plan で具体化）。13.1（設定データ）/ 13.3（バックアップ）周辺。
