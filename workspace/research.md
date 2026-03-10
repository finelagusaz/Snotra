# Research: マルチモニター・高DPI環境でのウィンドウ挙動 (#225)

## issue の要約

マルチモニター環境で記憶した位置のモニターが外されている場合、ウィンドウが画面外に表示される。
また、ホットキー押下時にマウスカーソルのあるモニターに表示する機能が求められている。
高DPI混在は Tauri/WebView2 のデフォルト挙動に委ねる（意図的な非対応を明記）。

## 関連コード

### 1. ウィンドウ位置の保存/復元

- `snotra-core/src/window_data.rs`: `WindowPlacement { x: i32, y: i32 }` — 論理座標で保存
- `src-tauri/src/main.rs:338-344`: setup フェーズで `load_search_placement()` → `set_position(Logical)`
- `ui/src/MainApp.tsx:128-142`: `onMoved` イベントで 500ms デバウンス保存
- `src-tauri/src/commands/window.rs:154-161`: IPC コマンド `get_search_placement` / `save_search_placement`

### 2. ウィンドウ表示

- `src-tauri/src/main.rs:157-235`: `show_main_and_emit()` — ホットキー・初回表示時の中心処理
  - 高さリセット(52px) → show() → set_focus() → IME → emit
- `src-tauri/src/main.rs:444-469`: ホットキーリスナー — toggle/show の分岐

### 3. Win32 API 利用状況

- `GetCursorPos`: `platform/tray.rs` で使用済み
- `Win32_Graphics_Gdi`: Cargo.toml feature に含まれる → `MonitorFromPoint`, `GetMonitorInfoW` 利用可
- `Win32_UI_HiDpi`: feature に含まれる（ただし今回は使わない）

## 既存パターン

- `GetCursorPos` は `tray.rs` で既に使用 → 同じパターンを流用
- `show_main_and_emit` が表示の一元制御 → ここにモニター判定を追加するのが自然
- position は論理座標 (Logical) で管理 → `MonitorFromPoint` は物理座標を使うため変換が必要

## 技術的制約

### Win32 Monitor API (同期 API、platform スレッド不要)

- `GetCursorPos(lpPoint)`: 同期、カーソルの物理スクリーン座標を返す
- `MonitorFromPoint(pt, dwFlags)`: 同期、指定点を含むモニターの HMONITOR を返す
  - `MONITOR_DEFAULTTONULL`: 該当なしで NULL
  - `MONITOR_DEFAULTTOPRIMARY`: 該当なしでプライマリ
  - `MONITOR_DEFAULTTONEAREST`: 該当なしで最近接
- `GetMonitorInfoW(hMonitor, lpmi)`: 同期、モニターの作業領域等を返す
  - `MONITORINFO.rcWork`: タスクバーを除いた作業領域 (物理座標)

### 座標系

- Tauri の `set_position(Logical(...))` は論理座標
- Win32 の `MonitorFromPoint` / `GetMonitorInfoW` は物理スクリーン座標
- `scale_factor()` で変換可能
- **重要**: マルチモニター環境では各モニターの DPI が異なる可能性がある。`MonitorFromPoint` で得たモニターの DPI と、Tauri ウィンドウの `scale_factor` が異なる場合がある

### Tauri の available_monitors / current_monitor

- Tauri v2 の `Window` には `available_monitors()` / `current_monitor()` / `primary_monitor()` メソッドがある
- これらは Tauri のクロスプラットフォーム API で、物理座標のモニター情報を返す
- Win32 API を直接使う代わりに、Tauri API を使う方がポータブルだが、カーソル位置のモニターを取得する API は無い

### 方針: Tauri API + 最小限の Win32

- カーソル位置取得: `GetCursorPos`（Win32、既存利用あり）
- モニター判定: `MonitorFromPoint`（Win32）
- モニター作業領域取得: `GetMonitorInfoW`（Win32）
- ウィンドウ移動: `set_position`（Tauri API）

## 未解決の疑問

- なし。issue の要求は明確。
