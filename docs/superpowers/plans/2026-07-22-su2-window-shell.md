# SU2: ウィンドウシェル + 状態機械 実装計画（簡素化版）

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 製品 `src-tauri` のメインウィンドウに、WebView2 と並行して egui/softbuffer 経路を env フラグで立ち上げる外殻を作る。Alt+Q 表示/非表示・blur 自動非表示・フォーカス列・残留 Alt 解除・位置永続・起動時/初回フローを SPEC §8 と一致させ、**フラグ OFF で WebView2 経路・E2E 注入を完全に不変**に保つ。

**Architecture:** codex 敵対的レビューを受けた簡素化版。egui 専用の `show_egui_main`/`hide_egui_main` を WebView2 経路（無改変）と分離し、共有は `position_on_target_monitor` の `&tauri::Window` 一般化のみ。状態は `AppState.main_visible`(bool) + 純粋核 `plan_hotkey`。controller/4 状態機械/`plan_ui_action`/`Defer` は持たない（egui 同期 show で live 到達不能・spec「否定の知識」）。フラグは `tauri.conf.json` を空にせず、`main()` で `config_mut().app.windows` からフラグ ON のときだけ "main" を除去（E2E 注入が宣言窓に依存するため・codex #2）。全 hide は外部 `window.hide()`（runtime.visible を false にせず空白窓を避ける・codex #4/#7）。

**Tech Stack:** Rust 2024 / egui 0.35 / `snotra-egui-runtime`（SU1） / tauri v2・tauri-runtime-wry（unstable） / windows 0.62 / Windows・PowerShell。

## Global Constraints

- **`main` へ直接コミットしない**（実装用に `feat/532-su2-window-shell` を切る）。コミットは一時ファイル `git commit -F`。bash HEREDOC 不可。パス区切り `/`。
- **フラグ**: `SNOTRA_EGUI_MAIN=1`。判定は `crate::trace::env_flag("SNOTRA_EGUI_MAIN")`。
- **窓生成**: `tauri.conf.json` は不変（宣言窓 "main" を残す）。`main()` でフラグ ON のとき `app_context.config_mut().app.windows.retain(|w| w.label != "main")`。setup フェーズで `egui_shell::create`。両生成とも setup 限定（`WebviewWindowBuilder`/`Window::builder` はメッセージポンプ進行を要求）。
- **WebView2 経路（フラグ OFF）は変更しない**。`show_main_and_emit`・resume/suspend/emit・hooks 化はしない。触るのは `position_on_target_monitor` の型一般化のみ（呼び出し元 1 箇所の互換は mini-gate で確認）。
- **状態**: `AppState.main_visible: AtomicBool`（`state.rs:17`・既存）。egui は controller/4 状態を持たず、`plan_hotkey(main_visible, is_alt_pressed())` で分岐。
- **`EguiRuntime::install(&self, app: &mut tauri::App<Wry>)`**（`runtime.rs:77`）: `create` は setup の `&mut App` を受ける（codex #1）。
- **全 hide は外部 `window.hide()`**（codex #4/#7）: `RuntimeFrame::hide_window`（runtime.visible=false）を使わない。view 起点 hide は `emit("egui-hide-requested")` → listener → `hide_egui_main`。
- **focus 観測**: view 内 `ctx.input(|i| i.focused)`（probe `main.rs:174`）。blur policy（100ms 猶予・`auto_hide_on_focus_lost` live-read・`SettingsProcessState` ガード・stale リセット）は view（src-tauri）側。runtime API を拡張しない。
- **フォント**: `SearchWindowView::setup` は `jp_font` を Proportional/Monospace の **index 0**（`insert(0, ...)`）。`push` は #399/#579 再発。
- **Win32 ヘルパー再利用**: `is_alt_pressed()`（`main.rs:90`）・`wait_alt_release_or_timeout()`（`main.rs:106`）・`send_alt_key_up()`（`main.rs:131`）。残留 Alt は focus 確定後かつ物理 Alt 解放後にのみ注入（#558）。
- **各タスク境界で** `cargo clippy -p snotra --all-targets` + `cargo test -p snotra` 緑。PostToolUse hook が `*.rs` 編集で自動実行（沈黙=合格）。

## File Structure

- `src-tauri/src/egui_shell/mod.rs`（新規）: `create(&mut App)`・`show_egui_main`/`hide_egui_main`・`save_placement_relative`・`egui-hide-requested` listener・`plan_hotkey` re-export。
- `src-tauri/src/egui_shell/lifecycle.rs`（新規・純粋核）: `HotkeyPlan`/`plan_hotkey` のみ。
- `src-tauri/src/egui_shell/view.rs`（新規）: `SearchWindowView`（placeholder・font-first・focus 観測 → emit）。
- `src-tauri/src/main.rs`（改修）: `config_mut` 除去・窓生成 flag 分岐・`setup_hotkey_listener`/`setup_startup_display`/`setup_exit_listener`/設定 launch-exit(§8.5) の flag 分岐・`position_on_target_monitor` の `&Window` 一般化。**WebView2 経路本体は不変**。
- `src-tauri/Cargo.toml`（改修）: `snotra-egui-runtime` path-dep 追加。
- `src-tauri/tauri.conf.json`: **不変**。

---

### Task 1: 依存追加 + 純粋核 lifecycle.rs（plan_hotkey）+ モジュール骨格

**Files:**
- Modify: `src-tauri/Cargo.toml`（`[dependencies]` 1 行）
- Modify: `src-tauri/src/main.rs`（`mod egui_shell;` 1 行）
- Create: `src-tauri/src/egui_shell/mod.rs`・`src-tauri/src/egui_shell/lifecycle.rs`

**Interfaces:**
- Produces: `pub(crate) fn plan_hotkey(visible: bool, alt_pressed: bool) -> HotkeyPlan`・`pub(crate) enum HotkeyPlan { HideNow, ShowAfterAltRelease, ShowNow }`。Task 4 の hotkey listener が消費。

- [ ] **Step 1: Cargo 依存を追加**

`src-tauri/Cargo.toml` の `[dependencies]` に:

```toml
snotra-egui-runtime = { path = "../snotra-egui-runtime" }
```

- [ ] **Step 2: モジュール登録 + mod.rs 骨格**

`src-tauri/src/main.rs` の `mod` 群に `mod egui_shell;` を追加。`src-tauri/src/egui_shell/mod.rs`:

```rust
//! egui/softbuffer メインウィンドウの外殻（#532 SU2）。WebView2 と並行する
//! egui 専用 window 生成・show/hide・blur 自動非表示・位置永続。WebView2 経路は触らない。
mod lifecycle;
mod view;

pub(crate) use lifecycle::{HotkeyPlan, plan_hotkey};
```

- [ ] **Step 3: 失敗するテストを書く（plan_hotkey + テスト）**

`src-tauri/src/egui_shell/lifecycle.rs`（spike `soft_host_main.rs:131-147` から移植）:

```rust
//! Alt+Q ホットキー分岐の純粋な決定核（Win32 非依存）。SU1 spike で実証済み・#532 SU2 で移植。

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum HotkeyPlan {
    HideNow,
    ShowAfterAltRelease,
    ShowNow,
}

/// Alt+Q 押下時の分岐。表示中なら即 hide、非表示中は Alt が押されている限り
/// 解放を待ってから show する（製品 WebView2 経路と同じ意味論）。
pub(crate) fn plan_hotkey(visible: bool, alt_pressed: bool) -> HotkeyPlan {
    if visible {
        HotkeyPlan::HideNow
    } else if alt_pressed {
        HotkeyPlan::ShowAfterAltRelease
    } else {
        HotkeyPlan::ShowNow
    }
}

#[cfg(test)]
mod tests {
    use super::{HotkeyPlan, plan_hotkey};

    #[test]
    fn hotkey_branches_match_product_semantics() {
        assert_eq!(plan_hotkey(true, false), HotkeyPlan::HideNow);
        assert_eq!(plan_hotkey(true, true), HotkeyPlan::HideNow);
        assert_eq!(plan_hotkey(false, true), HotkeyPlan::ShowAfterAltRelease);
        assert_eq!(plan_hotkey(false, false), HotkeyPlan::ShowNow);
    }
}
```

注: `HotkeyPlan`/`plan_hotkey` は Task 4 で消費されるまで dead-code。mod.rs の `pub(crate) use` 直前に一時的に `#[allow(dead_code)]` は付けない——`pub(crate) use` の re-export があれば警告は出ない想定。出る場合のみ `#[allow(dead_code)]` を lifecycle.rs 先頭に付け Task 4 完了時に除去。

- [ ] **Step 4: テスト実行**

Run: `cargo test -p snotra egui_shell::lifecycle`
Expected: `hotkey_branches_match_product_semantics` PASS。

- [ ] **Step 5: コミット**

```
git add src-tauri/Cargo.toml src-tauri/src/main.rs src-tauri/src/egui_shell/
git commit -F <tmpfile>   # "feat(#532): SU2 純粋核 plan_hotkey を src-tauri へ移植"
```

---

### Task 2: placeholder view（SearchWindowView）+ font-first カナリア

**Files:**
- Create: `src-tauri/src/egui_shell/view.rs`

**Interfaces:**
- Produces: `struct SearchWindowView`（`SearchWindowView::new(app_handle: tauri::AppHandle) -> Self`・`impl EguiView`）。`fn japanese_font_definitions(bytes: &'static [u8]) -> egui::FontDefinitions`（テスト対象）。Task 3 の `create` が `attach` に渡す。

- [ ] **Step 1: 失敗するテストを書く（view + font-first）**

`src-tauri/src/egui_shell/view.rs`（`japanese_font_definitions` は probe `snotra-egui-mvp/src/main.rs:635-670` から移植・テストは `:798-811` から）。**focus 観測フィールドは置くが emit 配線は Task 5**:

```rust
//! egui メインウィンドウの placeholder view（#532 SU2）。show/hide/focus/位置を視覚検証できる
//! 最小 chrome を描く。検索本体は SU3。font-first（jp_font を index 0）は SU1 申し送りの義務。

use std::sync::OnceLock;
use std::time::Instant;

use snotra_egui_runtime::{EguiView, RuntimeFrame};

static JP_FONT_BYTES: OnceLock<Box<[u8]>> = OnceLock::new();

fn japanese_font_definitions(bytes: &'static [u8]) -> egui::FontDefinitions {
    let mut fonts = egui::FontDefinitions::default();
    let mut font = egui::FontData::from_static(bytes);
    font.tweak = egui::FontTweak { scale: 1.0, y_offset_factor: 0.3, y_offset: 0.0, ..Default::default() };
    fonts.font_data.insert("jp_font".to_owned(), font.into());
    for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
        fonts.families.entry(family).or_default().insert(0, "jp_font".to_owned());
    }
    fonts
}

fn configure_japanese_font(context: &egui::Context) {
    let candidates = [
        "C:/Windows/Fonts/YuGothM.ttc",
        "C:/Windows/Fonts/yugothic.ttf",
        "C:/Windows/Fonts/msgothic.ttc",
        "C:/Windows/Fonts/meiryo.ttc",
    ];
    if JP_FONT_BYTES.get().is_none() {
        for path in candidates {
            if let Ok(bytes) = std::fs::read(path) {
                let _ = JP_FONT_BYTES.set(bytes.into_boxed_slice());
                break;
            }
        }
    }
    if let Some(bytes) = JP_FONT_BYTES.get() {
        let static_bytes: &'static [u8] = unsafe { std::mem::transmute::<&[u8], &'static [u8]>(bytes) };
        context.set_fonts(japanese_font_definitions(static_bytes));
    }
}

pub(crate) struct SearchWindowView {
    app_handle: tauri::AppHandle,
    was_focused: bool,
    unfocus_at: Option<Instant>,
    // emit dedup は共有 EguiShellState.hide_pending（show がクリア・codex #8）。view-local には持たない。
}

impl SearchWindowView {
    pub(crate) fn new(app_handle: tauri::AppHandle) -> Self {
        Self { app_handle, was_focused: false, unfocus_at: None }
    }
}

impl EguiView for SearchWindowView {
    fn setup(&mut self, context: &egui::Context) {
        configure_japanese_font(context);
    }

    fn update(&mut self, ui: &mut egui::Ui, _frame: &mut RuntimeFrame) {
        // placeholder: SU3 が検索 UI で置き換える。混在行（Latin+CJK）で font-first を視覚検証。
        // focus 観測 + blur emit は Task 5 で本体を実装する。
        ui.label("Snotra — 検索ウィンドウ（C:/Program Files/example）");
    }
}

#[cfg(test)]
mod tests {
    use super::japanese_font_definitions;

    #[test]
    fn jp_font_is_registered_at_index_zero_for_both_families() {
        let dummy: &'static [u8] = &[0u8; 4];
        let fonts = japanese_font_definitions(dummy);
        for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
            let list = fonts.families.get(&family).expect("family present");
            assert_eq!(
                list.first().map(String::as_str),
                Some("jp_font"),
                "jp_font must be index 0 for {family:?}（push=末尾だと #579 再発）"
            );
        }
    }
}
```

- [ ] **Step 2: テスト実行**

Run: `cargo test -p snotra egui_shell::view`
Expected: `jp_font_is_registered_at_index_zero_for_both_families` PASS。`was_focused`/`unfocus_at` は Task 5 まで未使用ゆえ dead-code 警告が出る場合は `#[allow(dead_code)]` を struct に付け Task 5 で除去。

- [ ] **Step 3: コミット**

```
git add src-tauri/src/egui_shell/
git commit -F <tmpfile>   # "feat(#532): SU2 placeholder SearchWindowView + font-first カナリア"
```

---

### Task 3: フラグで宣言窓を除去 + egui window 生成（子孫 0・G1/G4）

`main()` でフラグ ON のとき `config_mut().app.windows` から "main" を除去し、setup で egui window を生成する。フラグ OFF は一切変えない。

**Files:**
- Modify: `src-tauri/src/main.rs`（`config_mut` 除去・setup 窓生成分岐）
- Modify: `src-tauri/src/egui_shell/mod.rs`（`create`）

**Interfaces:**
- Produces: `pub(crate) fn create(app: &mut tauri::App) -> Result<(), snotra_egui_runtime::RuntimeError>`。

- [ ] **Step 1: `main()` でフラグ ON のとき宣言窓を除去**

`src-tauri/src/main.rs` の `let app_context = tauri::generate_context!();` と E2E 注入ブロック（`main.rs:588-599`）の**後**に:

```rust
    // フラグ ON: 宣言窓 "main"（WebView2）を除去して egui が置き換える（#532 SU2・codex #2）。
    // tauri.conf.json は変えず config を実行時ミューテート（E2E 注入と同じ経路）。flag OFF は不変。
    #[allow(unused_mut)]
    let mut app_context = app_context;
    if crate::trace::env_flag("SNOTRA_EGUI_MAIN") {
        app_context.config_mut().app.windows.retain(|w| w.label != "main");
    }
```

（E2E ブロックが既に `let app_context = { ... };` で shadow している場合は、その後に置く。`config_mut()` は `&mut Config` を返す＝E2E ブロックが `config_mut().app.windows` を触っているので同 API で可。）

- [ ] **Step 2: `create()` を書く**

`src-tauri/src/egui_shell/mod.rs` に（probe `snotra-egui-mvp/src/main.rs:691-756` の install/builder/attach を移植・`install` は `&mut App`＝codex #1）:

```rust
use snotra_egui_runtime::EguiRuntime;
use crate::egui_shell::view::SearchWindowView;

/// フラグ ON の窓生成。EguiRuntime を install し webview 無しの "main" 窓を生成して attach。setup 限定。
/// 宣言窓の全プロパティ（600×52 は既定・width は config の window_width・skipTaskbar・
/// alwaysOnTop・decorations:false・resizable:false・visible:false）を再現する（codex #11・(B)#1）。
pub(crate) fn create(app: &mut tauri::App, window_width: f64) -> Result<(), snotra_egui_runtime::RuntimeError> {
    let runtime = EguiRuntime::new();
    runtime.install(app); // &mut App を要求（runtime.rs:77）
    let app_handle = app.handle().clone();
    let window = tauri::Window::builder(app, "main")
        .title("Snotra")
        .inner_size(window_width, 52.0) // 保存幅を尊重（codex #11）
        .decorations(false)
        .resizable(false)
        .skip_taskbar(true)   // 宣言窓 skipTaskbar:true の再現（(B)#1）
        .always_on_top(true)  // 宣言窓 alwaysOnTop:true の再現（(B)#1）
        .visible(false)
        .build()
        .map_err(snotra_egui_runtime::RuntimeError::from)?;
    runtime.attach(window, SearchWindowView::new(app_handle))
}
```

mini-gate: `Window::builder` に `skip_taskbar`/`always_on_top` が無ければ `build()` 後に `window.set_skip_taskbar(true)`/`window.set_always_on_top(true)` で補う（**未確定のままにしない**＝(B)#1）。`tauri::Error` → `RuntimeError` の `From`（`RuntimeError::Tauri(#[from] tauri::Error)`＝`runtime.rs:45`）を `cargo build` で確認。`window_width` は `main()` の既存 `config.appearance.window_width`（`main.rs:572`）を渡す。

- [ ] **Step 3: setup で窓生成を flag 分岐（platform thread の後・codex #12）**

`src-tauri/src/main.rs` の setup 内、**`setup_platform_thread(&app_handle, ...)`（`main.rs:652`）の後**に置く。SPEC §8.5「platform thread を窓生成より前に spawn し Win32 初期化と窓生成を並列実行」を満たすため、egui 生成も platform thread の後にする（codex #12）:

```rust
    // 窓生成: フラグ ON は egui（platform thread spawn 後・SPEC §8.5 並列化）、
    // OFF は宣言窓が Tauri により既に生成済み（何もしない）。
    if crate::trace::env_flag("SNOTRA_EGUI_MAIN") {
        // show/hide を跨ぐ共有状態（世代・emit dedup）。view/hotkey/hide が参照するので窓生成前に管理下へ。
        app.manage(egui_shell::EguiShellState::default());
        egui_shell::create(app, window_width as f64)?; // app: &mut App・保存幅を渡す（codex #11）
    }
```

（`EguiShellState` は Task 4 Step 2 で定義。`app.manage` は setup の `&mut App`／`&App` どちらでも可＝`Manager` トレイト。`SearchWindowView` は `app_handle` 経由で `try_state::<EguiShellState>()` を読む。）

（`window_width` は既存の `config.appearance.window_width`＝`main.rs:572`。`?` は setup の戻り `Result<(), Box<dyn Error>>` へ。`RuntimeError` は `thiserror::Error` 派生ゆえ乗る。乗らなければ `.map_err(|e| e.to_string())?`。`cargo build` で確認。`setup_first_run`（`main.rs:656`）より前後どちらでもよいが、`setup_hotkey_listener` より前に窓が在ること。）

- [ ] **Step 4: フラグ OFF 不変（G1）とフラグ ON 子孫 0（G4）を確認**

Run（`SNOTRA_EGUI_MAIN` 未設定）:
```
cargo clippy -p snotra --all-targets
cargo test -p snotra
npm run smoke:startup
npm run e2e:tauri
```
Expected: 全 PASS（宣言窓・E2E 注入とも無改変）。**FAIL したら STOP** — `config_mut` 除去がフラグ OFF で走っていないか、他を触った。

Run（フラグ ON）:
```
$env:SNOTRA_EGUI_MAIN="1"; cargo run -p snotra
```
別ターミナル: `Get-Process msedgewebview2 -ErrorAction SilentlyContinue`
Expected: egui window が生成される（この時点では非表示＝`visible(false)`。表示は Task 4）。`msedgewebview2` **0 件**。**spawn していたら STOP** — `config_mut` 除去が効いていない（`retain` の label 比較を確認）。

- [ ] **Step 5: コミット**

```
git add src-tauri/src/
git commit -F <tmpfile>   # "feat(#532): SU2 フラグで宣言窓除去 + egui window 生成（子孫 0・G1/G4）"
```

---

### Task 4: show_egui_main / hide_egui_main + 位置一般化 + ホットキー配線 + 起動時表示（G2/G3）

egui 専用の show/hide を書き、`position_on_target_monitor` を `&tauri::Window` へ一般化して共有し、ホットキーと起動時表示を配線する。

**Files:**
- Modify: `src-tauri/src/main.rs`（`position_on_target_monitor` 一般化・`setup_hotkey_listener`/`setup_startup_display` の flag 分岐）
- Modify: `src-tauri/src/egui_shell/mod.rs`（`show_egui_main`/`hide_egui_main`/`save_placement_relative`）

**Interfaces:**
- Consumes: `crate::{position_on_target_monitor, is_alt_pressed, wait_alt_release_or_timeout, send_alt_key_up}`・`snotra_core::window_data`。
- Produces: `pub(crate) fn show_egui_main(app: &AppHandle, t0: Instant)`・`pub(crate) fn hide_egui_main(app: &AppHandle)`・`pub(crate) fn save_placement_relative(app: &AppHandle, window: &tauri::Window)`。

- [ ] **Step 1: `position_on_target_monitor` を `&tauri::Window` へ一般化**

`src-tauri/src/main.rs:329` の引数 `main: &tauri::WebviewWindow` を `main: &tauri::Window` に変える。本体（`main.outer_size()`・`window_data::load_search_placement()`・`main.set_position()`）は不変。**呼び出し元 `show_main_and_emit:497`（WebView2・`&main` は `WebviewWindow`）の互換を確認**（mini-gate）: `&WebviewWindow` が `&Window` に deref 強制されるか、`main.as_ref()` 等が要るか。`pub(crate)` へ可視性調整。

Run: `cargo build -p snotra`
Expected: コンパイル成功。失敗＝呼び出し元で `&WebviewWindow → &Window` の変換を 1 行足す（WebView2 の**挙動**は不変ゆえ G1 スモークで裏取り）。

- [ ] **Step 2: `show_egui_main`/`hide_egui_main`/`save_placement_relative` を書く**

`src-tauri/src/egui_shell/mod.rs` に（show 列は WebView2 の `show_and_focus_main:397-443` を egui 用に自前複製＝WebView2 本体を触らないため。残留 Alt・WM_NULL は逐語）:

```rust
use std::time::Instant;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use tauri::Manager;

/// egui 経路の show/hide を跨ぐ共有状態（managed state）。
/// - hotkey_generation: alt 解放待ち show の世代。hide が bump して保留 show を無効化する（codex #5/(B)#2）。
/// - hide_pending: view の emit dedup。show がクリアして「hide 後に Focused(true) が来ず抑止が残る」を断つ（codex #8）。
#[derive(Default)]
pub(crate) struct EguiShellState {
    pub(crate) hotkey_generation: AtomicU64,
    pub(crate) hide_pending: AtomicBool,
}

/// egui 経路の show。共有するのは position_on_target_monitor のみ。全 hide は外部化ゆえ
/// runtime.visible は false にならず、show は Focused(true) に依存せず確実に描ける（codex #4）。
pub(crate) fn show_egui_main(app: &tauri::AppHandle, _t0: Instant) {
    let Some(window) = app.get_window("main") else { return };
    if let Some(state) = app.try_state::<crate::AppState>() {
        state.main_visible.store(true, Ordering::SeqCst);
    }
    // show のたびに view の emit dedup をリセット（Focused(true) 非依存・codex #8）。
    if let Some(sh) = app.try_state::<EguiShellState>() {
        sh.hide_pending.store(false, Ordering::SeqCst);
    }
    #[cfg(windows)]
    crate::position_on_target_monitor(app, &window);
    let _ = window.show();
    let _ = window.set_focus();
    #[cfg(windows)]
    if let Ok(hwnd) = window.hwnd() {
        use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
        use windows::Win32::UI::WindowsAndMessaging::{SendMessageTimeoutW, SMTO_NORMAL, WM_NULL};
        let hwnd = HWND(hwnd.0);
        let mut result = 0usize;
        unsafe {
            let _ = SendMessageTimeoutW(hwnd, WM_NULL, WPARAM(0), LPARAM(0), SMTO_NORMAL, 100, Some(&mut result));
        }
    }
    // 残留 Alt 解除: focus 確定後かつ物理 Alt 解放後のみ（#558）。
    if !crate::is_alt_pressed() {
        crate::send_alt_key_up();
    }
    // ime_off_on_show は SU2 では省略可（IME off は SU5/SU6 で config 反映と合わせる。要判断）。
}

/// egui 経路の hide。全 hide の唯一の副作用所有点（codex #7）。外部 window.hide() のみ。
pub(crate) fn hide_egui_main(app: &tauri::AppHandle) {
    // 保留中の alt 解放待ち show を無効化（codex #5/(B)#2）: 世代を bump し、
    // spawn 済み show スレッドの gen 一致チェックを外して再表示を防ぐ。
    if let Some(sh) = app.try_state::<EguiShellState>() {
        sh.hotkey_generation.fetch_add(1, Ordering::SeqCst);
    }
    if let Some(window) = app.get_window("main") {
        save_placement_relative(app, &window); // save-on-hide
        let _ = window.hide();
    }
    if let Some(state) = app.try_state::<crate::AppState>() {
        state.main_visible.store(false, Ordering::SeqCst);
    }
}

/// 現在の物理位置をターゲットモニター作業領域原点からの相対座標で window.bin に保存。
pub(crate) fn save_placement_relative(app: &tauri::AppHandle, window: &tauri::Window) {
    // WebView2 の commands::save_search_placement（commands/window.rs）の相対座標算出を
    // &tauri::Window で行う。monitor.rs の作業領域原点 + window.outer_position()。
    // 具体は Step の mini-gate で window_data::save_search_placement のシグネチャに合わせる。
    let _ = (app, window); // 実装は下記 mini-gate
}
```

mini-gate: `commands/window.rs` の `save_search_placement` を読み、`window_data::save_search_placement(placement)` の placement 型と相対座標算出（`monitor.rs` の原点 + `window.outer_position()`）を `save_placement_relative` に写す。`get_window("main")` が egui 窓を返すことは Task 3 で確認済み。

- [ ] **Step 3: 起動時表示を flag 分岐**

`src-tauri/src/main.rs:952` の `setup_startup_display`:

```rust
fn setup_startup_display(app_handle: &AppHandle, show_on_startup: bool) {
    if show_on_startup {
        if crate::trace::env_flag("SNOTRA_EGUI_MAIN") {
            egui_shell::show_egui_main(app_handle, Instant::now());
        } else {
            show_main_and_emit(app_handle);
        }
    }
}
```

- [ ] **Step 4: ホットキーを flag 分岐**

`src-tauri/src/main.rs:781` の `setup_hotkey_listener` の `hotkey-pressed` クロージャ冒頭（サイドカーガード `main.rs:790` の後、generation ガード `main.rs:795` の付近）で egui 分岐。既存の generation/alt-wait 構造を egui 用に流用:

```rust
        if crate::trace::env_flag("SNOTRA_EGUI_MAIN") {
            // 世代は共有の EguiShellState を使う（hide が bump して保留 show を無効化・codex #5/(B)#2）。
            let current_gen = handle_for_hotkey.try_state::<egui_shell::EguiShellState>()
                .map(|sh| sh.hotkey_generation.fetch_add(1, Ordering::SeqCst) + 1)
                .unwrap_or(0);
            let visible = handle_for_hotkey.try_state::<AppState>()
                .map(|s| s.main_visible.load(Ordering::SeqCst)).unwrap_or(false);
            let hotkey_toggle = handle_for_hotkey.try_state::<AppState>()
                .map(|s| s.engine.lock().unwrap().config().general.hotkey_toggle)
                .unwrap_or(true); // main.rs:808-812 と同じ live-read
            match egui_shell::plan_hotkey(visible, is_alt_pressed()) {
                HotkeyPlan::HideNow if hotkey_toggle => egui_shell::hide_egui_main(&handle_for_hotkey),
                HotkeyPlan::HideNow => {} // hotkey_toggle=false は可視のまま
                HotkeyPlan::ShowNow => egui_shell::show_egui_main(&handle_for_hotkey, t0),
                HotkeyPlan::ShowAfterAltRelease => {
                    let h = handle_for_hotkey.clone();
                    std::thread::spawn(move || {
                        wait_alt_release_or_timeout();
                        // 共有世代が変わっていたら（別の press や hide が bump）show を諦める。
                        let gen_now = h.try_state::<egui_shell::EguiShellState>()
                            .map(|sh| sh.hotkey_generation.load(Ordering::SeqCst)).unwrap_or(0);
                        if gen_now != current_gen { return; }
                        egui_shell::show_egui_main(&h, Instant::now());
                    });
                }
            }
            return;
        }
```

（`HotkeyPlan` は `egui_shell::HotkeyPlan`。`t0` は listener 冒頭の `Instant::now()`＝`main.rs:786`。**egui 分岐は WebView2 の既存 `hotkey_generation_for_listener` を使わず共有 `EguiShellState.hotkey_generation` を使う**——hide 経路（`hide_egui_main`）からも bump できるようにするため。Task 1 で dead-code allow を付けていれば除去。）

- [ ] **Step 5: Alt+Q show/hide/toggle と hide→show 反復（空白窓不在）を確認（G2/G3）**

Run: `cargo clippy -p snotra --all-targets`（dead-code なし）

Run（フラグ ON・トレース）:
```
$env:SNOTRA_EGUI_MAIN="1"; $env:SNOTRA_TRACE="1"; cargo run -p snotra
```
- Alt+Q で 非表示→表示（`ShowNow` / Alt 押しっぱなしは `ShowAfterAltRelease`）
- 表示中 Alt+Q（`hotkey_toggle=true`）→ 非表示
- **hide→show を 10 回反復 → 毎回ちゃんと描画される（空白窓が出ない・G3）**
- 位置がドラッグ後も復元される

Expected: 上記どおり。`SNOTRA_EGUI_RENDER_ERROR` 連発なし。**空白窓が出たら STOP** — codex #4 が現実化（runtime.visible が false のまま）。全 hide が外部 `window.hide()` か、`RuntimeFrame::hide_window` を誤用していないか確認。破れるなら runtime に最小の可視化フックを足す設計へ戻る（要相談）。

- [ ] **Step 6: コミット**

```
git add src-tauri/src/
git commit -F <tmpfile>   # "feat(#532): SU2 show_egui_main/hide_egui_main + 位置一般化 + hotkey（G2/G3）"
```

---

### Task 5: blur 自動非表示 + Escape（view→emit→listener）

view が focus 喪失/Escape を観測し `emit("egui-hide-requested")`、src-tauri listener が `hide_egui_main` を呼ぶ。全 hide を 1 経路に集約（codex #7）。

**Files:**
- Modify: `src-tauri/src/egui_shell/view.rs`（focus/Escape 観測 + policy + emit）
- Modify: `src-tauri/src/egui_shell/mod.rs`（`egui-hide-requested` listener 登録）
- Modify: `src-tauri/src/main.rs`（setup で listener 登録の flag 分岐）

**Interfaces:**
- Produces: `pub(crate) fn register_hide_listener(app: &tauri::AppHandle)`（`egui-hide-requested` → `hide_egui_main`）。

- [ ] **Step 1: listener を登録する**

`src-tauri/src/egui_shell/mod.rs`:

```rust
/// view からの hide 要求を受け、メインスレッドで hide_egui_main を実行する（全 hide の合流点）。
pub(crate) fn register_hide_listener(app: &tauri::AppHandle) {
    let handle = app.clone();
    app.listen("egui-hide-requested", move |_| {
        hide_egui_main(&handle);
    });
}
```

`src-tauri/src/main.rs` の setup、`setup_hotkey_listener` 付近でフラグ ON のとき登録:

```rust
    if crate::trace::env_flag("SNOTRA_EGUI_MAIN") {
        egui_shell::register_hide_listener(&app_handle);
    }
```

- [ ] **Step 2: view に focus/Escape 観測 + policy + emit を実装**

`SearchWindowView::update` を実装（stale リセット込み・codex #8）:

```rust
    fn update(&mut self, ui: &mut egui::Ui, _frame: &mut RuntimeFrame) {
        let ctx = ui.ctx().clone();
        let focused = ctx.input(|i| i.focused);
        let escape = ctx.input(|i| i.key_pressed(egui::Key::Escape));

        // 再表示直後の stale 猶予をリセット: focused に戻ったら pending 破棄。
        // emit dedup（hide_pending）は show_egui_main がクリアするので view では触らない（codex #8）。
        if focused {
            self.unfocus_at = None;
        }
        // Escape → 即 hide 要求（内側モード優先は SU3）。
        if escape {
            self.emit_hide();
        }
        // focus 喪失 → 100ms 猶予後に policy を満たせば hide 要求。
        if self.was_focused && !focused {
            self.unfocus_at = Some(std::time::Instant::now());
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        }
        if let Some(at) = self.unfocus_at
            && !focused
            && at.elapsed() >= std::time::Duration::from_millis(100)
            && self.auto_hide_enabled()
            && !self.settings_running()
        {
            self.unfocus_at = None;
            self.emit_hide();
        }
        self.was_focused = focused;

        ui.label("Snotra — 検索ウィンドウ（C:/Program Files/example）");
    }
```

ヘルパー（`SearchWindowView` に追加）:

```rust
    fn emit_hide(&mut self) {
        // 多重防止は共有 EguiShellState.hide_pending（show_egui_main がクリア・codex #8）。
        // view-local フラグだと hide 後 Focused(true) 非着信で永久 true 化し以後の hide を抑止する。
        let already = self.app_handle.try_state::<crate::egui_shell::EguiShellState>()
            .map(|sh| sh.hide_pending.swap(true, std::sync::atomic::Ordering::SeqCst))
            .unwrap_or(false);
        if already { return; }
        let _ = self.app_handle.emit("egui-hide-requested", ());
    }
    fn auto_hide_enabled(&self) -> bool {
        self.app_handle.try_state::<crate::AppState>()
            .map(|s| s.engine.lock().unwrap().config().general.auto_hide_on_focus_lost)
            .unwrap_or(true) // config.rs 既定と一致（mini-gate で確認）
    }
    fn settings_running(&self) -> bool {
        self.app_handle.try_state::<crate::SettingsProcessState>()
            .map(|p| p.lock().unwrap().is_some()).unwrap_or(false)
    }
```

（`use tauri::{Emitter, Manager};` を view.rs へ。`auto_hide_on_focus_lost` の既定値を `snotra-core` config.rs で確認＝mini-gate。Task 2 の dead-code allow を除去。）

- [ ] **Step 3: focus-lost 自動非表示とサイドカーガードを確認（G3 blur）**

Run（フラグ ON）: `$env:SNOTRA_EGUI_MAIN="1"; cargo run -p snotra`
- 表示 → 別アプリをクリック → **100ms 後に自動非表示**（`auto_hide_on_focus_lost=true`）
- 表示中に `snotra-settings` を外部起動（Task 6 前ゆえ手動 spawn か、Task 6 の §8.5 と合わせて検証）→ 設定が focus を奪っても**メインは非表示にならない**
- Escape → 非表示
- focus 喪失→即 refocus（100ms 未満）→ hide しない（stale リセット）

Expected: 上記どおり。**設定 focus で本体が消えたら STOP** — サイドカーガード不全。

- [ ] **Step 4: コミット**

```
git add src-tauri/src/
git commit -F <tmpfile>   # "feat(#532): SU2 blur 自動非表示 + Escape（view→emit→listener・100ms・stale リセット）"
```

---

### Task 6: SPEC §8.5 — 設定サイドカー起動中の alwaysOnTop 解除（egui 窓）

現行は設定起動中にメインの `alwaysOnTop=false`、終了検知で復元（`get_webview_window("main")` キー）。egui 窓では no-op ゆえ検索窓が設定 UI を覆う（codex #3）。フラグ ON のとき `get_window("main")` に対し同制御を適用する。

**Files:**
- Modify: 設定 launch/exit 監視箇所（`commands/window.rs` の `open_settings` 系 + 設定プロセス終了監視。`get_webview_window("main")` を使う `commands/window.rs:91-94, 131-134`）

**Interfaces:**
- Consumes: `get_window("main")`・`set_always_on_top`。

- [ ] **Step 1: 現行の alwaysOnTop 制御箇所を特定**

`commands/window.rs` の設定起動（`always_on_top(false)` 相当）と終了監視（`always_on_top(true)` 復元）を読む。`get_webview_window("main")` で窓を取り `set_always_on_top` している 2 箇所（launch 時 false・exit 検知時 true）。

- [ ] **Step 2: フラグ ON で `get_window("main")` に同制御を並置**

各箇所を「フラグ ON なら `get_window("main")`、OFF なら既存 `get_webview_window("main")`」で分岐（または `get_window` は両窓を返すなら共通化＝Task 3 で確認済みの挙動に合わせる）。`set_always_on_top(false/true)` を egui 窓へ適用。

```rust
    let window = if crate::trace::env_flag("SNOTRA_EGUI_MAIN") {
        app.get_window("main")
    } else {
        app.get_webview_window("main").map(|w| /* &Window 化 or 既存のまま set */ )
    };
    // 既存が WebviewWindow 前提なら、egui 分岐だけ get_window で set_always_on_top する薄い分岐に留める。
```

mini-gate: 既存コードが `WebviewWindow` の `set_always_on_top` を呼ぶ形なら、egui 分岐は `get_window("main").set_always_on_top(...)` を別途呼ぶ最小差分にする（WebView2 側の既存呼びは不変）。

- [ ] **Step 3: 設定を開いても egui 窓が最前面を明け渡すことを確認（§8.5）**

Run（フラグ ON）: `$env:SNOTRA_EGUI_MAIN="1"; cargo run -p snotra` → 設定を開く（トレイ「設定」等）
Expected: 設定 UI が検索窓に覆われず操作できる。設定終了後にメインが `alwaysOnTop=true` へ復元。

- [ ] **Step 4: コミット + PR**

```
git add src-tauri/src/
git commit -F <tmpfile>   # "feat(#532): SU2 設定起動中の egui 窓 alwaysOnTop 解除（SPEC §8.5・codex #3）"
git push -u origin HEAD && gh pr create --fill
```

（PR 本文は SU2 受け入れ条件チェックリスト。**マージ前に** `gh pr view <PR> --json closingIssuesReferences` で #532 誤 close を確認＝ルート `CLAUDE.md` squash 手順。#532 は親ゆえ SU2 単独では閉じない。）

---

## Self-Review（spec 照合）

- **spec §フラグと生成（config_mut 除去・create &mut App・tauri.conf.json 不変）** → Task 3（+ codex #1/#2）。✓
- **spec §egui show/hide（show_egui_main/hide_egui_main・全 hide 外部化・main_visible のみ）** → Task 4（+ codex #4/#7）。✓
- **spec §状態機械（plan_hotkey・view→emit→listener 合流）** → Task 1 + Task 4 + Task 5。✓
- **spec §blur（100ms・ゲート・サイドカーガード・stale リセット・policy を view）** → Task 5（+ codex #8）。✓
- **spec §8.5 alwaysOnTop（codex #3）** → Task 6。✓
- **spec §placeholder + font-first** → Task 2。✓
- **spec §位置永続（復元 show 内・save-on-hide・一般化）** → Task 4。デバウンス保存は残余（記録済み・codex #10）。
- **spec 検証ゲート G1/G2/G3/G4** → G1/G4=Task 3・G2/G3=Task 4・blur=Task 5。✓
- **spec §受け入れ条件 1-6** → 全 Task。flag OFF 完全不変=G1、plan_hotkey 純関数=Task 1、blur policy が view=Task 5、font-first=Task 2、子孫 0=G4。✓
- **spec 否定の知識（seam/controller/静的空化/RuntimeFrame 直 hide の却下）** → 本計画は簡素化版で seam を持たない。✓

**codex 再レビュー（2026-07-22）で追加修正した箇所:**
- #12: egui `create` を `setup_platform_thread` の後へ（SPEC §8.5 並列化順序）。Task 3 Step 3。
- #11: `create` が config の `window_width` を使う（固定 600px でなく保存幅）。Task 3。
- (B)#1: `create` が `skip_taskbar(true)`/`always_on_top(true)` を明示（宣言窓プロパティ再現・未確定にしない）。Task 3 Step 2。
- #8: emit dedup を view-local `hide_sent` から共有 `EguiShellState.hide_pending` へ移し `show_egui_main` がクリア（hide 後 Focused(true) 非着信で抑止が残る問題を断つ）。Task 4/5。
- #5/(B)#2: `hide_egui_main` が共有 `EguiShellState.hotkey_generation` を bump し、保留中の alt 解放待ち show を無効化（hide 後の再表示を防ぐ）。Task 4。
- #10: デバウンス保存欠落は残余として spec §位置永続に記録（受容）。

**実装時 mini-gate（fabrication を避けた箇所）:**
- Task 3: `Window::builder` の `skip_taskbar`/`always_on_top` 有無（無ければ setter・未確定にしない）・`RuntimeError` From・`config_mut().app.windows` の label フィールド名。
- Task 4: `position_on_target_monitor` 一般化での呼び出し元互換（`&WebviewWindow→&Window`）・`save_search_placement` シグネチャ・`get_window("main")` が egui 窓を返す（Task 3 で確認）。
- Task 5: `auto_hide_on_focus_lost` の config.rs 既定値・`Emitter`/`Manager` トレイト import。
- Task 6: 既存 alwaysOnTop 制御の実型（WebviewWindow）と egui 分岐の最小差分。
