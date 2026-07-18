# research.md — issue #576: 設定の反映に再起動がいる

## issue の要約

設定ウィンドウで「フォーカス喪失時に非表示」（`auto_hide_on_focus_lost`）のチェックを外して保存しても、実行中のメインウィンドウにリアルタイムで反映されない。再起動すると反映される。issue 本文によれば「config.toml の変更は即刻本体が検知する」仕様のはずで、一部の設定だけがこの契約から漏れている。

ユーザーとの確認の結果、今回のスコープは同一パターン（起動時に一度だけ config を読みクロージャ/フロントエンド状態に固定キャッシュする）を共有する **3 項目すべて**: `auto_hide_on_focus_lost` / `hotkey_toggle` / `ime_off_on_show`。

## 根本原因（共通パターン）

`src-tauri/src/config_watcher.rs::apply_config_change()` は `config.toml` 変更時に **`state.engine.lock().unwrap().update_config(new_config)`（180行目）で `Engine` が保持する `Config` 全体を毎回まるごと差し替える**。つまり `engine.config()` を都度読めば、どのフィールドも最新値が取れる。

しかし `apply_config_change()` 自身は特定フィールド（`hotkey` / `show_tray_icon` / `language` / `show_icons` / `IndexInputs` / `instant_command_prefix` / `visual` / `visible_rows` / `result_limit` / `window_width`）だけを新旧比較してイベント発火・Win32 反映する差分適用方式（107〜224行目）。**`auto_hide_on_focus_lost` / `hotkey_toggle` / `ime_off_on_show` はこの差分検知対象に含まれていない**——config 自体はホットリロードされているのに、下流の消費者（フロントエンドの一度きりのリスナー登録、Rust 側の一度きりのクロージャ capture）まで新しい値が伝播しない。

`general.follow_cursor_monitor` は対照的に正しいパターン: `main.rs:335-339` の `position_on_target_monitor()` が **毎回呼び出し時に** `state.engine.lock().unwrap().config().general.follow_cursor_monitor` を読む。キャッシュしないので自動的にホットリロードされる。

## 関連コード（実在確認済み）

### 1. `auto_hide_on_focus_lost`（フロントエンド側の欠落）

- 型・既定値: `snotra-core/src/config.rs:117`（`default_auto_hide_on_focus_lost`）, `141-142`（フィールド）, `159`（既定 `true`）
- 設定 UI: `snotra-settings/src/tabs/general.rs:49` `ui.checkbox(&mut config.general.auto_hide_on_focus_lost, ...)`
- Bootstrap 経路: `src-tauri/src/commands/config.rs:10-13,37-40`（`BootstrapGeneralConfig.auto_hide_on_focus_lost`）→ `ui/src/lib/types.ts:37-38`（`BootstrapGeneralConfig`）
- フロントエンド消費: `ui/src/MainApp.tsx:62-81`（`registerAutoHideOnFocusLost` — `win.onFocusChanged` リスナーを登録し `unlistenFns` に push するだけの一度きり関数）、`215-217`（`onMount` 内で bootstrap 到着時に一度だけ条件呼び出し）
- **config_watcher.rs は `auto_hide_on_focus_lost` の変更を一切検知しない** — `apply_config_change()` の diff 対象リストに含まれていない

### 2. `hotkey_toggle`（Rust 側クロージャ capture）

- 型・既定値: `snotra-core/src/config.rs:109`（`default_hotkey_toggle`）, `137-138`, `157`（既定 `true`）
- 設定 UI: `snotra-settings/src/tabs/general.rs`（チェックボックス、`hotkey_toggle` フィールド名で bind）
- Rust 消費: `src-tauri/src/main.rs:564` で起動時に一度だけ `config.general.hotkey_toggle` を読み `hotkey_toggle` ローカル変数へ、`658` で `setup_hotkey_listener(&app_handle, hotkey_toggle, ime_off)` へ渡す。`777-780` で関数引数からさらに `toggle` へ move キャプチャされ、`hotkey-pressed` イベントリスナークロージャ（`783-830`）内、`801` `if visible && toggle` で判定に使われる。**このクロージャはアプリ生存期間中ずっと同一の `toggle` 値を参照し続ける**

### 3. `ime_off_on_show`（Rust 側クロージャ capture、2 箇所）

- 型・既定値: `snotra-core/src/config.rs:125`（`default_ime_off_on_show`）, `145-146`, `161`（既定 `false`）
- Rust 消費（3 箇所、いずれも起動時に一度だけ読んだ `ime_off`（`main.rs:563`）を再利用）:
  1. `main.rs:596,603` — `tauri_plugin_single_instance` のコールバッククロージャに `ime_off_for_si` として move capture。2 個目のプロセス起動時に `show_main_and_emit(app, ime_off_for_si)` を呼ぶ（**アプリ生存期間中固定**）
  2. `main.rs:780,824,828` — `setup_hotkey_listener` 内で `ime_control` として move capture、`show_main_and_emit(&handle_for_show, ime_control)` / `show_main_and_emit(&handle_for_hotkey, ime_control)` に渡す（**アプリ生存期間中固定**）
  3. `main.rs:681,940-942` — `setup_startup_display(&app_handle, show_on_startup, ime_off)` は setup フェーズで一度きり呼ばれる関数なので問題なし（起動直後の config はまだ最新）

`show_main_and_emit(app_handle: &AppHandle, ime_control: bool)`（`main.rs:475`）自体は単なる bool 引数を取る関数——呼び出し元がどの値を渡すかが問題。`follow_cursor_monitor` と同じパターンで「呼び出し時に `AppState.engine.config()` から都度読む」ヘルパーへ揃えれば解決する。

## 既存パターン（再利用可能）

- **Rust 側「都度読み」パターン**: `main.rs:335-339` の `position_on_target_monitor()` 内 `follow_cursor_monitor` 読み取り。`app_handle.try_state::<AppState>().map(|s| s.engine.lock().unwrap().config().general.XXX).unwrap_or(default)` の形。
- **フロントエンド側「config change イベント購読 → シグナル更新」パターン**: `MainApp.tsx:160-199` の `visual-config-changed` / `max-results-changed` / `show-icons-changed` / `instant-prefix-changed` / `top-n-history-changed` はいずれも `config_watcher.rs::apply_config_change()` が emit したイベントを `listen<T>(...)` で受け、SolidJS シグナルを更新するだけの単純な形。今回は「シグナル更新」ではなく「リスナーの登録/解除」なので、対称ペア（register/unregister）の管理が新規に必要——既存パターンをそのまま流用はできないが、`config_watcher.rs` 側の diff 検知 + emit の骨格は流用できる。

## 技術的制約

- **`registerAutoHideOnFocusLost` は現状シグナルではなく副作用（`listen` 登録）**。ON→OFF→ON と設定を切り替えても二重登録しないよう、現在の unlisten 関数を保持し、OFF 時に呼ぶ「対称な解除」が必要（`/symmetric-check` 対象）。`unlistenFns` 配列に push するだけの現行実装は「後で個別に外す」ことを想定していないため、この関数専用の状態変数（`let unlistenAutoHide: (() => void) | null = null;` 等）に分離する必要がある。
- **`blurTimer`（`createOwnedTimer(100)`）は auto-hide 用の単一インスタンス**。OFF にした瞬間に保留中の非表示アクションが走らないよう、unregister 時に `blurTimer.cancel()` を呼ぶ必要がある（さもないと OFF 直後にタイマーが発火し意図せず非表示になる）。
- **Win32 API の同期性**: 今回の 3 項目はいずれも Win32 API 呼び出しを伴わない（`hotkey_toggle` はホットキー自体の再登録ではなく「trigger 時に toggle するか show 直行するか」という**ロジック分岐フラグ**であり、`RegisterHotKey` の再登録は発生しない。`ime_off_on_show` も `ImmSetOpenStatus` 呼び出しタイミングの条件フラグに過ぎない）。したがって MSDN 同期性確認は不要——単純な「都度読み」で足りる。
- **`config_watcher.rs::apply_config_change()` は新規 diff 追加時に既存の不変条件（`src-tauri/CLAUDE.md`）を守る必要がある**: 言語変更→ホットキー失敗通知の順序、`ReadFailed` 時の早期 return、`index_changed` の `!indexing` ゲートなし。今回追加する diff（`auto_hide_on_focus_lost` 変更検知 → イベント emit）はこれらの既存フローと独立しており、順序制約とは無関係な位置に追加できる。

## 未解決の疑問

なし。3 項目とも原因・修正方針は特定済み。
