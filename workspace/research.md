# Research: マルチモニター・高DPI環境でのウィンドウ挙動 (#225)

## issue の要約

マルチモニター環境で記憶した位置のモニターが外されている場合、ウィンドウが画面外に表示される。
ホットキー押下時にカーソルのあるモニターに表示する機能、および表示先の設定（カーソル追尾 / プライマリ固定）が求められている。

## ユーザーとの合意事項

1. **モニター原点からの相対座標で1つ保持**（ディスプレイ単位の保持はしない — 複数ディスプレイの付け外しを考慮）
2. **設定で表示先を選択**: `follow_cursor_monitor`（カーソル追尾 / プライマリ固定）
3. **異サイズモニター対応**: ターゲットモニターの作業領域にクランプ
4. **高DPI混在**: Tauri/WebView2 に委ねる（明示的な非対応を SPEC に記載）

## 関連コード

### ウィンドウ位置の保存/復元

- `snotra-core/src/window_data.rs`: `WindowPlacement { x: i32, y: i32 }` — 現在は絶対論理座標で保存
  - `window.bin` に `BinFile` 経由で永続化（magic `WNDW`, version V4, postcard）
  - `WindowPlacementState { search, settings, settings_size }` の内部構造
- `src-tauri/src/main.rs:338-344`: setup フェーズで `load_search_placement()` → `set_position(Logical)`
- `ui/src/MainApp.tsx:128-142`: `onMoved` イベントで 500ms デバウンス → `saveSearchPlacement(x, y)`
- `src-tauri/src/commands/window.rs:154-161`: IPC コマンド `get_search_placement` / `save_search_placement`

### ウィンドウ表示

- `src-tauri/src/main.rs:157-242`: `show_main_and_emit()` — 高さリセット(52px) → show() → set_focus() → IME → emit
- `src-tauri/src/main.rs:420-469`: ホットキーリスナー — toggle/show 分岐
- ホットキーリスナーのキャプチャ変数 (`toggle`, `ime_control`) は起動時に固定される
  - ただし `follow_cursor_monitor` は `show_main_and_emit` 内で Engine から毎回読むため、この制約の影響を受けない

### 設定

- `snotra-core/src/config.rs`: `GeneralConfig` に設定フィールドを追加するパターンが確立
  - `default_xxx()` 関数 + `#[serde(default = "...")]` + `impl Default`
- `src-tauri/src/config_watcher.rs`: `apply_config_change()` で diff 検出 → イベント発火
  - `update_config(new_config)` で Engine に反映（全フィールド自動）
- `src-tauri/src/state.rs`: `AppState { engine: Mutex<Engine>, ... }` — config は Engine 内部

### Win32 API

- `GetCursorPos`: `platform/tray.rs` で使用済み
- `Win32_Graphics_Gdi` feature: Cargo.toml に含まれる → `MonitorFromPoint`, `GetMonitorInfoW`, `MONITORINFO` 利用可
- `Win32_UI_HiDpi` feature: 含まれるが今回は使用しない
- 全て同期 API — platform スレッド経由不要

## 技術的制約

### 座標系の整理

| コンテキスト | 座標系 |
|---|---|
| `onMoved` (Tauri) | 論理座標 |
| `set_position(Logical)` | 論理座標 |
| `set_position(Physical)` | 物理座標 |
| `outer_position()` | 物理座標 |
| `inner_size()` | 物理座標 |
| `GetCursorPos` | 物理スクリーン座標 |
| `MonitorFromPoint` | 物理スクリーン座標 |
| `GetMonitorInfoW.rcWork` | 物理スクリーン座標 |

→ **方針**: Win32 API 呼び出しは物理座標で統一。Tauri の `set_position(Physical)` / `outer_position()` と組み合わせる。保存は物理相対座標。

### MonitorFromWindow vs MonitorFromPoint

保存時: `MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST)` でウィンドウのあるモニターを取得
→ ウィンドウの HWND から直接モニターを取得できるため、座標変換が不要でより安全。

表示時: `GetCursorPos` → `MonitorFromPoint(cursor_pt, MONITOR_DEFAULTTOPRIMARY)` でカーソルのモニターを取得

### window.bin バージョン移行

V4（現行）: 絶対論理座標
V5（新規）: モニター相対物理座標

V4 データは変換せず破棄（None 返却）。初回表示はモニター中央。
