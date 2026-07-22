# SU2: ウィンドウシェル + 状態機械 実装計画

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 製品 `src-tauri` のメインウィンドウに、WebView2 と並行して egui/softbuffer 経路を env フラグ選択で立ち上げる外殻を作る。Alt+Q 表示/非表示・blur 自動非表示・フォーカス列・残留 Alt 解除・位置永続・起動時/初回フローを SPEC §8 と一致させ、フラグ OFF で WebView2 挙動を不変に保つ。

**Architecture:** 両ウィンドウが同じ `tauri::Window` 抽象（`.show()`/`.hide()`/`.set_focus()`/`.hwnd()`）を共有するため、show/hide の Win32 骨格を 1 本の共有オーケストレーション（`show_main`/`hide_main`）に集約し、renderer(resume/suspend)+frontend(emit) の副作用だけを `MainBackend::{WebView2, Egui}` の薄い 3 フック（`pre_show`/`post_show`/`post_hide`）へ逃がす。ホットキー分岐と UI コマンド適用は純関数 `plan_hotkey`/`plan_ui_action` として `egui_shell/lifecycle.rs` に置き、冪等性と show 進行中 Hide の Defer をユニットテストで固定する。宣言窓（`tauri.conf.json`）を廃し両経路とも setup で programmatic 生成へ寄せ、フラグ ON で egui が WebView2 を真に置き換える（子孫 0）。

**Tech Stack:** Rust 2024 / egui 0.35 / `snotra-egui-runtime`（SU1・softbuffer）/ tauri v2・tauri-runtime-wry（unstable）/ windows 0.62 / Windows・PowerShell。

## Global Constraints

- **`main` へ直接コミットしない**（現在ブランチ `docs/532-su2-window-shell-spec`。実装用に `feat/532-su2-window-shell` を切る）。コミットメッセージは一時ファイル `git commit -F <tmpfile>`。bash HEREDOC 不可。パス区切りは `/`。
- **フラグ**: `SNOTRA_EGUI_MAIN=1`（env・setup で 1 回読む）。判定は既存 `crate::trace::env_flag("SNOTRA_EGUI_MAIN")`（`src-tauri/src/trace.rs:20`）を使う。
- **窓生成**: `tauri.conf.json` の `app.windows` を `[]` にする。両経路とも setup で programmatic 生成。ラベルは両方 `"main"`。生成は **setup フェーズ限定**（`WebviewWindowBuilder::build()` はメッセージポンプ進行を要求・`src-tauri/CLAUDE.md`）。
- **2 つの visible を混同しない**: `AppState.main_visible: AtomicBool`（`state.rs:17`・policy＝`plan_hotkey` 入力・Standby/SearchVisible 判定）と runtime 内 `visible`（SU1 の描画ゲート・不変条件⑥）は別物。
- **backend seam**: WebView2 の `resume_webview`/`suspend_and_trim_after_hide`/`emit(window-shown|hidden)` を `MainBackend::WebView2` フックへ**逐語移動**。順序制約（resume を show の前後 2 回・emit を suspend より先・FIFO 直列化＝`src-tauri/CLAUDE.md`「TrySuspend / Resume パターン」）を温存。egui フックは `pre_show`/`post_hide` 空・`post_show` は ime_control のみ。
- **純粋核**: `plan_hotkey`/`plan_ui_action` は Win32 非依存・ユニットテスト。冪等性（`Visible+Show→Refocus`・`Suspended+Hide→Ignore`）と Defer（`Recreating+Hide→Defer`）を固定。
- **focus 観測**: view が `ctx.input(|i| i.focused)` で観測（`snotra-egui-mvp/src/main.rs:174` で実証）。blur policy（100ms 猶予・`auto_hide_on_focus_lost` ゲート・`SettingsProcessState` 非起動ガード）は **src-tauri の view 側**に置く。`snotra-egui-runtime` の公開 API を拡張しない。
- **フォント**: `SearchWindowView::setup` は `jp_font` を Proportional/Monospace の **index 0**（`insert(0, ...)`）。`FONT_PATH` 候補は `C:/Windows/Fonts/YuGothM.ttc` 他。`push`（末尾）にすると #399/#579 のベースラインずれ再発。
- **位置永続**: 復元は show 経路で `position_on_target_monitor`（順序: サイズ確定 → 位置 → show）。保存は save-on-hide + 可視時終了保存。SPEC §8.2 の相対物理座標・`follow_cursor_monitor`・クランプ・中央フォールバックを踏襲。
- **Win32 ヘルパー再利用**: `is_alt_pressed()`（`main.rs:90`）・`wait_alt_release_or_timeout()`（`main.rs:106`）・`send_alt_key_up()`（`main.rs:131`）。残留 Alt 解除は **focus 確定後かつ物理 Alt 解放後にのみ**注入（#558）。
- **子孫 0**: フラグ ON で `msedgewebview2.exe` が 0 件（egui が置き換え）。
- **各タスク境界で** `cargo clippy -p snotra --all-targets` と `cargo test -p snotra` が緑。PostToolUse hook が `*.rs` 編集時に自動実行する（沈黙=合格・失敗時のみ会話に届く）が、各タスクの Step でも明示実行する。

## File Structure

- `src-tauri/src/egui_shell/mod.rs`（新規）: `MainBackend` enum・`create()`（`Window::builder`+`install`+`attach`）・`show_main`/`hide_main` 共有オーケストレーション・3 フック・controller（合流点）。
- `src-tauri/src/egui_shell/lifecycle.rs`（新規・純粋核）: `HotkeyPlan`/`plan_hotkey`・`LifecycleState`/`LifecycleEvent`/`transition`・`HostCommand`/`UiAction`/`plan_ui_action`。Win32 非依存。
- `src-tauri/src/egui_shell/view.rs`（新規）: `SearchWindowView: EguiView`（placeholder）。`setup` で font-first・`update` で focus 観測（自動非表示の起点）。
- `src-tauri/src/main.rs`（改修）: setup 本体に窓生成 flag 分岐 + `build_webview2_window`（新規）。`show_and_focus_main` を `&tauri::Window` へ一般化。WebView2 leaf をフックへ移動。`setup_hotkey_listener`/`setup_startup_display` に flag 分岐。
- `src-tauri/tauri.conf.json`（改修）: `app.windows = []`。
- `src-tauri/Cargo.toml`（改修）: `snotra-egui-runtime` path-dep 追加。

---

### Task 1: 依存追加 + egui_shell モジュール骨格 + 純粋核 lifecycle.rs

撤去済み spike（`soft_host_main.rs:130-207` / テスト `1597-1649`・commit 7558cc8）から純粋核を移植する。Win32 非依存ゆえ単独でコンパイル・ユニットテストできる。

**Files:**
- Modify: `src-tauri/Cargo.toml`（`[dependencies]` に 1 行）
- Modify: `src-tauri/src/main.rs`（`mod egui_shell;` を 1 行追加）
- Create: `src-tauri/src/egui_shell/mod.rs`
- Create: `src-tauri/src/egui_shell/lifecycle.rs`

**Interfaces:**
- Consumes: なし（純粋・std のみ）。
- Produces: `plan_hotkey(visible: bool, alt_pressed: bool) -> HotkeyPlan`／`plan_ui_action(state: LifecycleState, command: &HostCommand) -> UiAction`／`transition(state: LifecycleState, event: LifecycleEvent) -> Result<LifecycleState, String>`／enum `HotkeyPlan{HideNow,ShowAfterAltRelease,ShowNow}`・`LifecycleState{Visible,Suspended,Recreating,Exiting}`・`LifecycleEvent{Hide,Show,FramePresented,Exit}`・`HostCommand{Show{hotkey_started:Instant},Hide}`・`UiAction{Show,Hide,Refocus,Defer,Ignore}`。Task 5/6 の controller が消費。

- [ ] **Step 1: Cargo 依存を追加**

`src-tauri/Cargo.toml` の `[dependencies]` に追加:

```toml
snotra-egui-runtime = { path = "../snotra-egui-runtime" }
```

- [ ] **Step 2: モジュールを登録**

`src-tauri/src/main.rs` の他の `mod` 宣言群（`mod trace;` 等の近く）に追加:

```rust
mod egui_shell;
```

`src-tauri/src/egui_shell/mod.rs` を作成（Task 4 以降で拡張。今は純粋核の入れ物）:

```rust
//! egui/softbuffer メインウィンドウの外殻。WebView2 と並行する window 生成・
//! show/hide 状態機械・blur 自動非表示・位置永続を持つ（#532 SU2）。
mod lifecycle;
```

- [ ] **Step 3: 失敗するテストを書く（純粋核 + テスト）**

`src-tauri/src/egui_shell/lifecycle.rs` を作成（spike から逐語移植・製品用にコメント調整）:

```rust
//! Alt+Q ホットキー分岐と UI コマンド適用の純粋な決定核（Win32 非依存）。
//! 冪等性（表示中+Show / 非表示中+Hide）と show 進行中に届いた Hide の繰り延べを
//! ここに一元化する。SU1 spike（soft_host_main.rs）で実証済み・#532 SU2 で製品へ移植。

use std::time::Instant;

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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum LifecycleState {
    Visible,
    Suspended,
    /// show 済みで最初のフレーム提示を待っている。
    Recreating,
    Exiting,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum LifecycleEvent {
    Hide,
    Show,
    FramePresented,
    Exit,
}

pub(crate) fn transition(
    state: LifecycleState,
    event: LifecycleEvent,
) -> Result<LifecycleState, String> {
    match (state, event) {
        (LifecycleState::Visible, LifecycleEvent::Hide) => Ok(LifecycleState::Suspended),
        (LifecycleState::Suspended, LifecycleEvent::Show) => Ok(LifecycleState::Recreating),
        (LifecycleState::Recreating, LifecycleEvent::FramePresented) => Ok(LifecycleState::Visible),
        (LifecycleState::Visible, LifecycleEvent::Exit)
        | (LifecycleState::Suspended, LifecycleEvent::Exit) => Ok(LifecycleState::Exiting),
        _ => Err(format!("invalid lifecycle transition: {state:?} + {event:?}")),
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum HostCommand {
    Show { hotkey_started: Instant },
    Hide,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum UiAction {
    Show,
    Hide,
    Refocus,
    /// show 完了（FramePresented）後まで Hide を繰り延べる。
    Defer,
    Ignore,
}

/// ホストコマンドを現在の lifecycle 状態へ適用する計画。冪等性
/// （Visible+Show / Suspended+Hide）と、show 進行中に届いた Hide の繰り延べを決める。
pub(crate) fn plan_ui_action(state: LifecycleState, command: &HostCommand) -> UiAction {
    match (state, command) {
        (LifecycleState::Visible, HostCommand::Show { .. }) => UiAction::Refocus,
        (LifecycleState::Visible, HostCommand::Hide) => UiAction::Hide,
        (LifecycleState::Suspended, HostCommand::Show { .. }) => UiAction::Show,
        (LifecycleState::Suspended, HostCommand::Hide) => UiAction::Ignore,
        (LifecycleState::Recreating, HostCommand::Show { .. }) => UiAction::Ignore,
        (LifecycleState::Recreating, HostCommand::Hide) => UiAction::Defer,
        (LifecycleState::Exiting, _) => UiAction::Ignore,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        HostCommand, HotkeyPlan, LifecycleEvent, LifecycleState, UiAction, plan_hotkey,
        plan_ui_action, transition,
    };
    use std::time::Instant;

    fn show_command() -> HostCommand {
        HostCommand::Show { hotkey_started: Instant::now() }
    }

    #[test]
    fn hotkey_branches_match_product_semantics() {
        assert_eq!(plan_hotkey(true, false), HotkeyPlan::HideNow);
        assert_eq!(plan_hotkey(true, true), HotkeyPlan::HideNow);
        assert_eq!(plan_hotkey(false, true), HotkeyPlan::ShowAfterAltRelease);
        assert_eq!(plan_hotkey(false, false), HotkeyPlan::ShowNow);
    }

    #[test]
    fn ui_commands_are_idempotent_and_defer_hide_while_showing() {
        assert_eq!(plan_ui_action(LifecycleState::Visible, &show_command()), UiAction::Refocus);
        assert_eq!(plan_ui_action(LifecycleState::Suspended, &HostCommand::Hide), UiAction::Ignore);
        assert_eq!(plan_ui_action(LifecycleState::Suspended, &show_command()), UiAction::Show);
        assert_eq!(plan_ui_action(LifecycleState::Visible, &HostCommand::Hide), UiAction::Hide);
        assert_eq!(plan_ui_action(LifecycleState::Recreating, &HostCommand::Hide), UiAction::Defer);
        assert_eq!(plan_ui_action(LifecycleState::Recreating, &show_command()), UiAction::Ignore);
        assert_eq!(plan_ui_action(LifecycleState::Exiting, &show_command()), UiAction::Ignore);
    }

    #[test]
    fn lifecycle_requires_frame_before_visible_and_allows_hidden_exit() {
        let showing = transition(LifecycleState::Suspended, LifecycleEvent::Show).unwrap();
        assert_eq!(showing, LifecycleState::Recreating);
        let visible = transition(showing, LifecycleEvent::FramePresented).unwrap();
        assert_eq!(visible, LifecycleState::Visible);
        assert!(transition(LifecycleState::Suspended, LifecycleEvent::Hide).is_err());
    }
}
```

- [ ] **Step 4: テストが通ることを確認する**

Run: `cargo test -p snotra egui_shell::lifecycle`
Expected: 3 tests（`hotkey_branches_match_product_semantics` / `ui_commands_are_idempotent_and_defer_hide_while_showing` / `lifecycle_requires_frame_before_visible_and_allows_hidden_exit`）PASS。

注: `pub(crate)` の未使用シンボルは Task 5/6 で消費されるまで dead-code 警告が出る。clippy を止めないため、`mod.rs` の `mod lifecycle;` 直後にこの時点だけ `#[allow(dead_code)]` を付け、**Task 6 完了時に除去する**（除去を Task 6 Step の最後に明記済み）。

- [ ] **Step 5: コミット**

```
git add src-tauri/Cargo.toml src-tauri/src/main.rs src-tauri/src/egui_shell/
git commit -F <tmpfile>   # "feat(#532): SU2 純粋核 plan_hotkey/plan_ui_action を src-tauri へ移植"
```

---

### Task 2: G0 — 宣言窓を programmatic 生成へ変換（flag OFF byte-identical）

`tauri.conf.json` の宣言窓を廃し、setup で `WebviewWindowBuilder` により逐語再現する。**このタスクではまだ flag 分岐を入れない**（純粋に宣言→programmatic の等価変換だけを接地し、回帰が無いことを確定する = G0）。

**Files:**
- Modify: `src-tauri/tauri.conf.json:14-28`（`windows` 配列を空に）
- Modify: `src-tauri/src/main.rs`（`build_webview2_window` 新規 + setup 本体で呼ぶ）

**Interfaces:**
- Consumes: なし。
- Produces: `fn build_webview2_window(app: &tauri::AppHandle) -> tauri::Result<()>`。Task 5 で flag 分岐の else 側に置かれる。

- [ ] **Step 1: 宣言窓を空にする**

`src-tauri/tauri.conf.json` の `app.windows` を空配列にする（`app.security` / `bundle` / `plugins` は不変）:

```json
  "app": {
    "windows": [],
    "security": {
      "csp": "default-src 'self'; connect-src 'self' ipc: http://ipc.localhost; script-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' data: blob:"
    }
  },
```

- [ ] **Step 2: `build_webview2_window` を書く（宣言窓 10 フィールドを逐語再現）**

`src-tauri/src/main.rs` に追加（`use tauri::{WebviewUrl, WebviewWindowBuilder};` を先頭 use 群へ）:

```rust
/// 旧 tauri.conf.json の宣言窓（label "main"）を programmatic に再現する。
/// フラグ OFF（WebView2 経路）の窓生成。宣言時と挙動一致（G0）。
fn build_webview2_window(app: &tauri::AppHandle) -> tauri::Result<()> {
    WebviewWindowBuilder::new(app, "main", WebviewUrl::App("main.html".into()))
        .title("Snotra")
        .inner_size(600.0, 52.0)
        .visible(false)
        .decorations(false)
        .skip_taskbar(true)
        .always_on_top(true)
        .resizable(false)
        .center()
        .build()?;
    Ok(())
}
```

- [ ] **Step 3: setup 本体の先頭で窓を生成する**

`src-tauri/src/main.rs` の `.setup(move |app| {` 直後、`let app_handle = app.handle().clone();`（現 633 行）の**直後**に追加（`setup_window_geometry` 等の既存呼び出しより前・宣言窓が消えた分をここで作る）:

```rust
    // 宣言窓を廃し programmatic 生成へ変換（#532 SU2 G0）。フラグ分岐は Task 5。
    build_webview2_window(&app_handle)?;
```

- [ ] **Step 4: フラグ OFF で回帰が無いことを確認する（G0）**

Run（PowerShell・`SNOTRA_EGUI_MAIN` 未設定）:
```
npm run smoke:startup
npm run e2e:tauri
```
Expected: 両方 PASS（宣言時と同じ起動・ウィンドウ挙動）。`app.get_webview_window("main")` が従来どおり取れ、`main.html` がロードされる。**FAIL したら STOP** — programmatic 再現が宣言窓と非等価。10 フィールドの差分（特に `center`/`skip_taskbar`/`always_on_top`）を洗い直す。

- [ ] **Step 5: コミット**

```
git add src-tauri/tauri.conf.json src-tauri/src/main.rs
git commit -F <tmpfile>   # "feat(#532): SU2 G0 宣言窓を programmatic 生成へ変換（flag OFF 不変）"
```

---

### Task 3: placeholder view（SearchWindowView）+ font-first カナリア

egui window に描く最小 view を作る。SU1 申し送りの font-first（`insert(0, jp_font)`）を機構化し、実 `setup` を駆動するテストで固定する。本体（検索・結果・モード）は SU3。

**Files:**
- Create: `src-tauri/src/egui_shell/view.rs`
- Modify: `src-tauri/src/egui_shell/mod.rs`（`mod view;`）

**Interfaces:**
- Consumes: `snotra_egui_runtime::{EguiView, RuntimeFrame}`・`egui`。
- Produces: `struct SearchWindowView`（`SearchWindowView::new() -> Self`・`impl EguiView`）。`fn japanese_font_definitions(bytes: &'static [u8]) -> egui::FontDefinitions`（テスト対象）。Task 5 の `create()` が `attach` に渡す。

- [ ] **Step 1: 失敗するテストを書く（view + font-first テスト）**

`src-tauri/src/egui_shell/view.rs` を作成（`japanese_font_definitions` は probe `snotra-egui-mvp/src/main.rs:635-653` から移植・テストは `:798-811` から移植）:

```rust
//! egui メインウィンドウの placeholder view（#532 SU2）。show/hide/focus/位置を
//! 視覚検証できる最小 chrome を描く。検索本体は SU3。font-first（jp_font を index 0）は
//! SU1 申し送りの義務——push（末尾）だと softbuffer で #399/#579 のベースラインずれ再発。

use std::sync::OnceLock;

use snotra_egui_runtime::{EguiView, RuntimeFrame};

static JP_FONT_BYTES: OnceLock<Box<[u8]>> = OnceLock::new();

/// `jp_font` を Proportional/Monospace の先頭（`insert(0, ...)`）へ差し込んだ
/// `FontDefinitions` を組む純粋部分。`push`（末尾）だと Latin=egui既定/CJK=Yu Gothic の
/// 2フォントに分かれ、softbuffer の被覆AA無しラスタが vertical metrics 差を整数pxへ
/// 丸めて混在行のベースラインをずらす（#399/#579）。
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
        // OnceLock はプロセス寿命ゆえ &'static へ延命できる（再表示ごとのリークを作らない）。
        let static_bytes: &'static [u8] = unsafe { std::mem::transmute::<&[u8], &'static [u8]>(bytes) };
        context.set_fonts(japanese_font_definitions(static_bytes));
    }
}

/// SU2 の placeholder view。検索バー枠と最小テキストだけを描く。
pub(crate) struct SearchWindowView {
    // Task 7 で focus 観測 + blur policy 用の状態（was_focused / unfocus_at）を足す。
}

impl SearchWindowView {
    pub(crate) fn new() -> Self {
        Self {}
    }
}

impl EguiView for SearchWindowView {
    fn setup(&mut self, context: &egui::Context) {
        configure_japanese_font(context);
    }

    fn update(&mut self, ui: &mut egui::Ui, _frame: &mut RuntimeFrame) {
        // placeholder: SU3 が検索 UI で置き換える。混在行（Latin+CJK）を出して
        // font-first の視覚検証を可能にする。
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

`src-tauri/src/egui_shell/mod.rs` に追加:

```rust
mod view;
```

- [ ] **Step 2: テストが通ることを確認する**

Run: `cargo test -p snotra egui_shell::view`
Expected: `jp_font_is_registered_at_index_zero_for_both_families` PASS。

注: `SearchWindowView` は Task 5 で `attach` に渡るまで未使用ゆえ、Task 1 で付けた `#[allow(dead_code)]` の傘に入れる（`view` mod も同様）。

- [ ] **Step 3: コミット**

```
git add src-tauri/src/egui_shell/
git commit -F <tmpfile>   # "feat(#532): SU2 placeholder SearchWindowView + font-first カナリア"
```

---

### Task 4: MainBackend seam + 共有 show_main/hide_main（WebView2 leaf 抽出・G1）

show/hide の Win32 骨格を共有関数へ集約し、WebView2 の renderer/frontend 副作用をフックへ逐語移動する。**このタスクは WebView2 側だけを触り**（`Egui` variant は定義するが未使用）、フラグ OFF が不変であることを接地する（G1）。

**Files:**
- Modify: `src-tauri/src/egui_shell/mod.rs`（`MainBackend`・フック・`show_main`/`hide_main`）
- Modify: `src-tauri/src/main.rs`（`show_and_focus_main` を `&tauri::Window` へ一般化 → `show_and_focus_window`。`show_main_and_emit` を `show_main` 経由へ。`resume_webview`/`suspend_and_trim_after_hide`/`emit` をフックから呼ぶ）

**Interfaces:**
- Consumes: `crate::{resume_webview, suspend_and_trim_after_hide, apply_ime_control, position_on_target_monitor, show_and_focus_window}`（`show_and_focus_window` は本タスクで一般化）。
- Produces: `enum MainBackend { WebView2, Egui }`・`fn show_main(app: &AppHandle, backend: MainBackend, t0: Instant)`・`fn hide_main(app: &AppHandle, backend: MainBackend)`。Task 5/6 が消費。

- [ ] **Step 1: WebviewWindow から &tauri::Window を得る手段を 1 行で確定（mini-gate）**

共有 show 列は `&tauri::Window` で回す（egui/WebView2 両対応）。WebView2 側で `WebviewWindow` から `Window` を得る手段を確定する。`src-tauri/src/main.rs` の任意関数内に一時的に置いてコンパイル確認:

```rust
// 確認用（このあと削除）: WebviewWindow -> &Window
if let Some(wv) = app_handle.get_webview_window("main") {
    let _w: &tauri::Window = wv.as_ref().window();
}
```

Run: `cargo build -p snotra`
Expected: コンパイル成功。**失敗したら** `wv.as_ref()` / `wv.window()` / `app.get_window("main")` の別手段を試し、通る 1 行を採用してから次へ（確認コードは削除）。以降のステップはこの確定手段を `webview_window_as_window(&wv)` 相当として使う。

- [ ] **Step 2: `show_and_focus_main` を `&tauri::Window` へ一般化**

`src-tauri/src/main.rs:397` の `show_and_focus_main(app_handle, main: &tauri::WebviewWindow, t0)` を `show_and_focus_window(app_handle: &AppHandle, window: &tauri::Window, t0: Instant)` へ改名し、引数型を `&tauri::Window` に変える。本体（`show()`/`main_visible=true`/`set_focus()`/WM_NULL 同期/残留 Alt: `is_alt_pressed()` skip or `send_alt_key_up()`）はそのまま（すべて `&tauri::Window` で動く）。`.hwnd()` は両型にある。

呼び出し元 `show_main_and_emit`（`main.rs:499`）は Step 4 で `show_main` へ移すため一旦保留。

- [ ] **Step 3: `MainBackend` とフックを書く**

`src-tauri/src/egui_shell/mod.rs` に追加:

```rust
use std::time::Instant;
use tauri::{AppHandle, Manager};

/// メインウィンドウの renderer+frontend backend。show/hide の Win32 骨格は共有し、
/// backend 固有の副作用（WV2: resume/suspend/emit、egui: なし）だけをフックへ逃がす。
/// SU7 で WebView2 variant を削除すれば egui が素で残る。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum MainBackend {
    WebView2,
    Egui,
}

impl MainBackend {
    /// show の Win32 列（show/focus/位置）より前。WV2 はレンダラー resume。
    fn pre_show(self, app: &AppHandle) {
        if self == MainBackend::WebView2
            && let Some(wv) = app.get_webview_window("main")
        {
            crate::resume_webview(&wv);
        }
    }

    /// show の Win32 列の後。WV2 は resume 再適用 + ime_control + emit("window-shown")。
    /// egui は ime_control のみ（repaint は runtime が Focused 観測で運ぶ）。
    fn post_show(self, app: &AppHandle, t0: Instant) {
        match self {
            MainBackend::WebView2 => {
                if let Some(wv) = app.get_webview_window("main") {
                    crate::resume_webview(&wv); // 冒頭 resume 後〜可視反映前の残余窓を是正（#576）
                    crate::apply_ime_control_if_enabled(app, &wv, t0);
                }
                crate::emit_window_shown(app, t0);
            }
            MainBackend::Egui => {
                crate::apply_ime_control_egui(app, t0);
            }
        }
    }

    /// hide の後。WV2 は emit("window-hidden") + suspend + trim。egui は何もしない。
    fn post_hide(self, app: &AppHandle) {
        if self == MainBackend::WebView2 {
            let _ = app.emit("window-hidden", ());
            crate::suspend_and_trim_after_hide(app, "egui_shell_hide");
        }
    }
}

/// 両経路が通る唯一の show 経路。順序: pre_show → 位置 → Win32 show 列 → post_show。
pub(crate) fn show_main(app: &AppHandle, backend: MainBackend, t0: Instant) {
    let Some(window) = app.get_window("main") else { return };
    backend.pre_show(app);
    #[cfg(windows)]
    crate::position_on_target_monitor(app, &window);
    crate::show_and_focus_window(app, &window, t0);
    backend.post_show(app, t0);
}

/// 両経路が通る唯一の hide 経路。window.hide → main_visible=false → post_hide。
pub(crate) fn hide_main(app: &AppHandle, backend: MainBackend) {
    if let Some(window) = app.get_window("main") {
        let _ = window.hide();
    }
    if let Some(state) = app.try_state::<crate::AppState>() {
        state.main_visible.store(false, std::sync::atomic::Ordering::SeqCst);
    }
    backend.post_hide(app);
}
```

注: `app.get_window("main")` が programmatic WebView2 窓と egui 窓の両方を `tauri::Window` として返せることを Step 1 の確認で担保する（返せない場合は Step 1 の確定手段に合わせ `show_main`/`hide_main` の window 取得を差し替える）。

- [ ] **Step 4: 既存の WebView2 side-effect を helper 化してフックから呼ぶ**

`show_main_and_emit`（`main.rs:475`）内の以下を `egui_shell` から呼べるよう整理する:
- `resume_webview`（`main.rs:266`・既存 `pub` 化が要れば `pub(crate)`）
- `emit_window_shown`（`main.rs:463`）→ `pub(crate)`
- `apply_ime_control`（`main.rs:447`）を `apply_ime_control_if_enabled(app, &wv, t0)`（`ime_off_on_show` の live-read 判定込み・現 `show_main_and_emit:511-517` のロジックを移設）と、egui 版 `apply_ime_control_egui(app, t0)`（`get_window("main").hwnd()` に `PlatformCommand::TurnOffIme`）へ切り出す。
- `suspend_and_trim_after_hide`（`main.rs:305`・既に `pub(crate)`）。

そのうえで `show_main_and_emit(app)` を **薄いラッパー**にする（既存の全呼び出し元＝`single_instance` プラグイン `main.rs:607`・`setup_startup_display`・hotkey listener の互換を保つ）:

```rust
fn show_main_and_emit(app_handle: &AppHandle) {
    egui_shell::show_main(app_handle, egui_shell::MainBackend::WebView2, Instant::now());
}
```

`reset_search_height`（`main.rs:379`）は WebView2 の高さ折りたたみ。SU2 placeholder は固定サイズゆえ egui では不要。WebView2 経路では `show_main` の `pre_show` 前に呼ぶ必要があるため、`MainBackend::WebView2::pre_show` の先頭で `reset_search_height` を呼ぶ（順序: height reset → resume → 位置 → show を温存。`src-tauri/CLAUDE.md`「操作順序制約」）。

- [ ] **Step 5: フラグ OFF で回帰が無いことを確認する（G1）**

Run（`SNOTRA_EGUI_MAIN` 未設定）:
```
cargo clippy -p snotra --all-targets
cargo test -p snotra
npm run smoke:startup
npm run e2e:tauri
```
Expected: すべて PASS。hotkey show/hide・focus 喪失・`/s`・二重起動が従来どおり。**FAIL したら STOP** — leaf 移動が順序（resume 2 回・emit を suspend より先・height reset の位置）を壊した。`SNOTRA_TRACE=1` で `show_main:*` / `suspend:*` の順序を従来ログと比較。

- [ ] **Step 6: コミット**

```
git add src-tauri/src/
git commit -F <tmpfile>   # "feat(#532): SU2 MainBackend seam + 共有 show_main/hide_main（WV2 leaf 抽出・G1）"
```

---

### Task 5: egui window 生成 + Egui backend + 起動時表示（子孫 0）

フラグ ON で egui window を setup 生成し、`EguiRuntime` を install/attach する。`show_main` は Task 4 で在るため起動時表示はそれを使う。

**Files:**
- Modify: `src-tauri/src/egui_shell/mod.rs`（`create()`・`EguiRuntime` 保持）
- Modify: `src-tauri/src/main.rs`（setup 窓生成 flag 分岐・`setup_startup_display` 分岐）

**Interfaces:**
- Consumes: `snotra_egui_runtime::EguiRuntime`・`crate::egui_shell::{show_main, MainBackend}`・`SearchWindowView`。
- Produces: `fn create(app: &tauri::App) -> Result<(), snotra_egui_runtime::RuntimeError>`（install + Window::builder("main") + attach）。

- [ ] **Step 1: `create()` を書く**

`src-tauri/src/egui_shell/mod.rs` に追加（probe `snotra-egui-mvp/src/main.rs:691-756` の install/builder/attach 構造を移植・window は `visible(false)`）:

```rust
use snotra_egui_runtime::EguiRuntime;
use crate::egui_shell::view::SearchWindowView;

/// フラグ ON の窓生成。EguiRuntime を install し、webview 無しの "main" 窓を
/// programmatic 生成して SearchWindowView を attach する。生成は setup 限定。
pub(crate) fn create(app: &tauri::App) -> Result<(), snotra_egui_runtime::RuntimeError> {
    let runtime = EguiRuntime::new();
    runtime.install(app);
    let window = tauri::Window::builder(app, "main")
        .title("Snotra")
        .inner_size(600.0, 52.0)
        .decorations(false)
        .skip_taskbar(true)
        .always_on_top(true)
        .resizable(false)
        .visible(false)
        .build()?;
    runtime.attach(window, SearchWindowView::new())?;
    Ok(())
}
```

注: `Window::builder` の利用可能メソッド（`skip_taskbar`/`always_on_top` 等）が `WebviewWindowBuilder` と同名かは Tauri v2 API 依存。`cargo build` で確認し、無いメソッドは setup 後 `window.set_skip_taskbar(true)` 等で補う。

- [ ] **Step 2: setup 本体で flag 分岐する**

`src-tauri/src/main.rs` の Task 2 Step 3 で入れた `build_webview2_window(&app_handle)?;` を flag 分岐に置き換える:

```rust
    // 窓生成: フラグで egui / WebView2 を選ぶ（#532 SU2）。生成は setup 限定。
    if crate::trace::env_flag("SNOTRA_EGUI_MAIN") {
        egui_shell::create(app)?;
    } else {
        build_webview2_window(&app_handle)?;
    }
```

（`app` は `&tauri::App`・`create` が要求。`app_handle` は既存。**エラー変換の確定（mini-gate）**: setup クロージャの戻りは `Result<(), Box<dyn std::error::Error>>`。`create` の `RuntimeError` が `std::error::Error` を実装していれば `?` がそのまま通る（`RuntimeError` は `thiserror::Error` 派生＝`runtime.rs:43`・実装済み）。`build_webview2_window` の `tauri::Result` も同様に `?`。通らなければ `.map_err(|e| e.to_string())?` で `String`（`Box<dyn Error>` へ変換可）にする。`cargo build` で確認。）

- [ ] **Step 3: `setup_startup_display` を flag 分岐する**

`src-tauri/src/main.rs:952` の `setup_startup_display` を、egui のとき `show_main(app, MainBackend::Egui, Instant::now())` を呼ぶよう分岐:

```rust
fn setup_startup_display(app_handle: &AppHandle, show_on_startup: bool) {
    if show_on_startup {
        let backend = if crate::trace::env_flag("SNOTRA_EGUI_MAIN") {
            egui_shell::MainBackend::Egui
        } else {
            egui_shell::MainBackend::WebView2
        };
        egui_shell::show_main(app_handle, backend, Instant::now());
    }
}
```

- [ ] **Step 4: フラグ ON で egui window が上がり子孫 0 を確認する（G2-partial）**

Run（PowerShell）:
```
$env:SNOTRA_EGUI_MAIN="1"; $env:SNOTRA_TRACE="1"; cargo run -p snotra 2>&1 | Select-String "SNOTRA_EGUI|show_main"
```
（`show_on_startup=true` の config で起動、または起動後にウィンドウが出ることを確認）
別ターミナルで:
```
Get-Process msedgewebview2 -ErrorAction SilentlyContinue
```
Expected: egui window が可視・日本語 + 長パスが正しいベースラインで描画。`Get-Process msedgewebview2` が **0 件**。**msedgewebview2 が spawn していたら STOP** — 宣言窓が残存（Task 2 の `windows:[]` を確認）か WebView2 経路へ誤分岐。

- [ ] **Step 5: コミット**

```
git add src-tauri/src/
git commit -F <tmpfile>   # "feat(#532): SU2 egui window 生成 + 起動時表示（子孫 0・G2-partial）"
```

---

### Task 6: ホットキー配線（controller + plan_hotkey/plan_ui_action 適用）

製品ホットキー（`emit("hotkey-pressed")` → `setup_hotkey_listener`）を egui 経路の show/hide へ配線する。controller が全 Show/Hide 要求を `plan_ui_action` に通し、冪等性と Defer を一元化する。

**Files:**
- Modify: `src-tauri/src/egui_shell/mod.rs`（controller: `LifecycleState` 保持 + `apply_command`）
- Modify: `src-tauri/src/main.rs`（`setup_hotkey_listener` の flag 分岐）

**Interfaces:**
- Consumes: `lifecycle::{plan_hotkey, plan_ui_action, transition, HotkeyPlan, HostCommand, LifecycleState, LifecycleEvent, UiAction}`・`show_main`/`hide_main`。
- Produces: controller 状態（`AppState` へ `Mutex<LifecycleState>` を足すか、`egui_shell` 内 `static`）。`fn on_hotkey(app: &AppHandle, generation: ...)`（egui 経路の Alt+Q 処理）。

- [ ] **Step 1: controller を書く（合流点）**

`src-tauri/src/egui_shell/mod.rs` に追加。lifecycle 状態を保持し、`HostCommand` を受けて `plan_ui_action` → 適用 → `transition` する:

```rust
use std::sync::Mutex;
use lifecycle::{HostCommand, LifecycleEvent, LifecycleState, UiAction, plan_ui_action, transition};

/// show/hide の合流点。hotkey・Escape・focus-lost の全 Show/Hide 要求がここを通り、
/// 冪等性と show 進行中 Hide の Defer を一元化する。egui 経路専用（WV2 は従来の直接呼び）。
pub(crate) struct LifecycleController {
    state: Mutex<LifecycleState>,
    deferred_hide: Mutex<bool>,
}

impl LifecycleController {
    pub(crate) fn new() -> Self {
        Self { state: Mutex::new(LifecycleState::Suspended), deferred_hide: Mutex::new(false) }
    }

    pub(crate) fn apply(&self, app: &AppHandle, command: HostCommand) {
        let state = *self.state.lock().unwrap();
        match plan_ui_action(state, &command) {
            UiAction::Show => {
                if let HostCommand::Show { hotkey_started } = command {
                    show_main(app, MainBackend::Egui, hotkey_started);
                }
                self.advance(LifecycleEvent::Show);
                // FramePresented は runtime の初回 present 後に別途通知（Step 3 注）。
            }
            UiAction::Hide => {
                hide_main(app, MainBackend::Egui);
                self.advance(LifecycleEvent::Hide);
            }
            UiAction::Refocus => {
                if let Some(w) = app.get_window("main") { let _ = w.set_focus(); }
            }
            UiAction::Defer => { *self.deferred_hide.lock().unwrap() = true; }
            UiAction::Ignore => {}
        }
    }

    fn advance(&self, event: LifecycleEvent) {
        let mut state = self.state.lock().unwrap();
        if let Ok(next) = transition(*state, event) { *state = next; }
    }
}
```

controller を `AppState` の managed state か `egui_shell` の `OnceLock<LifecycleController>` として保持する（threading: hotkey listener はメインスレッド、view は event loop スレッドから `apply` を呼ぶため `Mutex` で保護）。

- [ ] **Step 2: `setup_hotkey_listener` を flag 分岐する**

`src-tauri/src/main.rs:781` の `setup_hotkey_listener` 内、`hotkey-pressed` クロージャで egui のとき `plan_hotkey` → `HostCommand` → `controller.apply` へ分岐する。WebView2 側は既存ロジック不変。サイドカーガード（`SettingsProcessState`・`main.rs:790`）と generation ガード（`main.rs:795`）と `ShowAfterAltRelease` の alt 解放待ちスレッド（`main.rs:830`）は共有構造で、egui では:

```rust
        // egui 経路: 純粋核で分岐を決め、controller へ HostCommand を送る。
        // current_gen / hotkey_generation_for_listener は既存 listener の変数（main.rs:784,795）。
        if crate::trace::env_flag("SNOTRA_EGUI_MAIN") {
            let visible = handle_for_hotkey.try_state::<AppState>()
                .map(|s| s.main_visible.load(Ordering::SeqCst)).unwrap_or(false);
            // hotkey_toggle の live-read は WebView2 経路（main.rs:808-812）と同じ。
            let hotkey_toggle = handle_for_hotkey.try_state::<AppState>()
                .map(|s| s.engine.lock().unwrap().config().general.hotkey_toggle)
                .unwrap_or(true);
            match egui_shell::plan_hotkey(visible, is_alt_pressed()) {
                HotkeyPlan::HideNow if hotkey_toggle => {
                    egui_shell::controller(&handle_for_hotkey)
                        .apply(&handle_for_hotkey, HostCommand::Hide);
                }
                HotkeyPlan::HideNow => {} // hotkey_toggle=false は可視のまま（hide しない）
                HotkeyPlan::ShowNow => {
                    egui_shell::controller(&handle_for_hotkey)
                        .apply(&handle_for_hotkey, HostCommand::Show { hotkey_started: t0 });
                }
                HotkeyPlan::ShowAfterAltRelease => {
                    let h = handle_for_hotkey.clone();
                    let gen_for_wait = hotkey_generation_for_listener.clone();
                    std::thread::spawn(move || {
                        wait_alt_release_or_timeout();
                        if gen_for_wait.load(Ordering::SeqCst) != current_gen { return; }
                        egui_shell::controller(&h)
                            .apply(&h, HostCommand::Show { hotkey_started: Instant::now() });
                    });
                }
            }
            return;
        }
```

（`plan_hotkey`/`HotkeyPlan`/`HostCommand` は `lifecycle` の `pub(crate)`。`main.rs` から使うため `egui_shell/mod.rs` で `pub(crate) use lifecycle::{plan_hotkey, HotkeyPlan, HostCommand};` を re-export。`egui_shell::controller(app) -> &LifecycleController` は Task 6 Step 1 の managed state を返す薄いヘルパー。`t0` は listener 冒頭の `Instant::now()`＝`main.rs:786`。）

- [ ] **Step 3: dead-code allow を除去し、Alt+Q show/hide/toggle を確認する（G2-full・G4）**

Task 1 で付けた `#[allow(dead_code)]` を除去する（全シンボルが消費された）。

Run: `cargo clippy -p snotra --all-targets`（dead-code 警告が出ないこと）

Run（PowerShell・トレース）:
```
$env:SNOTRA_EGUI_MAIN="1"; $env:SNOTRA_TRACE="1"; cargo run -p snotra
```
手動で Alt+Q を押下:
- 非表示 → 表示（`ShowNow` / Alt 押しっぱなしなら `ShowAfterAltRelease` 後）
- 表示中に Alt+Q（`hotkey_toggle=true`）→ 非表示
- 表示中に focus を奪わず連打 → 冪等（多重 show/hide しない）

Expected: show/hide が対称に動く。**hide 後アイドルで present 失敗リトライが繰り返されない**（G4: 隠れ窓に RedrawRequested が来ないので runtime は描かない）。`SNOTRA_TRACE` に `SNOTRA_EGUI_RENDER_ERROR` の連発が無い。**あれば STOP** — 外部 hide と runtime.visible の不整合。

- [ ] **Step 4: コミット**

```
git add src-tauri/src/
git commit -F <tmpfile>   # "feat(#532): SU2 ホットキー配線 + controller（plan_ui_action 合流・G2/G4）"
```

---

### Task 7: blur 自動非表示 + Escape（focus 観測 + policy）

view が focus 喪失を観測し、100ms 猶予・`auto_hide_on_focus_lost` ゲート・サイドカーガードを満たすとき controller へ `HostCommand::Hide` を送る。Escape も Hide。policy は view（src-tauri）側に置き runtime API を拡張しない。

**Files:**
- Modify: `src-tauri/src/egui_shell/view.rs`（focus 観測 + Escape + policy 判定）

**Interfaces:**
- Consumes: `ctx.input(|i| i.focused)`・`ctx.input(|i| i.key_pressed(egui::Key::Escape))`・`app_handle`（config/`SettingsProcessState` 読み）・controller。
- Produces: view → controller への `HostCommand::Hide`（view は `RuntimeFrame::hide_window` で先に paint 停止してよい＝前倒し）。

- [ ] **Step 1: view に focus 観測 + policy を実装する**

`SearchWindowView` に `app_handle: tauri::AppHandle`・`was_focused: bool`・`unfocus_at: Option<Instant>` を足し、`update` で:

```rust
    fn update(&mut self, ui: &mut egui::Ui, frame: &mut RuntimeFrame) {
        let ctx = ui.ctx().clone();
        let focused = ctx.input(|i| i.focused);
        let escape = ctx.input(|i| i.key_pressed(egui::Key::Escape));

        // Escape → 即 Hide（SPEC §8.1。内側モードの復帰優先は SU3）。
        if escape {
            self.request_hide(frame);
        }

        // focus 喪失 → 100ms 猶予後に policy を満たせば Hide。
        if self.was_focused && !focused {
            self.unfocus_at = Some(Instant::now());
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        }
        if focused {
            self.unfocus_at = None; // refocus で pending 破棄
        }
        if let Some(at) = self.unfocus_at
            && !focused
            && at.elapsed() >= std::time::Duration::from_millis(100)
            && self.auto_hide_enabled()   // config live-read
            && !self.settings_running()   // SettingsProcessState ガード
        {
            self.unfocus_at = None;
            self.request_hide(frame);
        }
        self.was_focused = focused;

        ui.label("Snotra — 検索ウィンドウ（C:/Program Files/example）");
    }
```

`request_hide`・`auto_hide_enabled`・`settings_running` を実装する:

```rust
    fn request_hide(&mut self, frame: &mut RuntimeFrame) {
        frame.hide_window(); // 前倒しで paint 停止（合流点は controller）
        controller_apply_hide(&self.app_handle); // egui_shell の controller へ HostCommand::Hide
    }
    fn auto_hide_enabled(&self) -> bool {
        self.app_handle.try_state::<crate::AppState>()
            .map(|s| s.engine.lock().unwrap().config().general.auto_hide_on_focus_lost)
            .unwrap_or(true) // config.rs の既定と一致（要確認）
    }
    fn settings_running(&self) -> bool {
        self.app_handle.try_state::<crate::SettingsProcessState>()
            .map(|p| p.lock().unwrap().is_some()).unwrap_or(false)
    }
```

（`controller_apply_hide` は `egui_shell` が `pub(crate)` で公開する薄いヘルパー。controller の保持先＝Step 6 Task 6 の managed state を読む。`SearchWindowView::new(app_handle)` に引数追加＝Task 5 の `create` の `SearchWindowView::new()` 呼び出しも更新する。`auto_hide_on_focus_lost` の既定値は `snotra-core` の config.rs で確認。）

- [ ] **Step 2: focus-lost 自動非表示とサイドカーガードを確認する（G3）**

Run（PowerShell）:
```
$env:SNOTRA_EGUI_MAIN="1"; cargo run -p snotra
```
- ウィンドウ表示 → 別アプリをクリックして focus を奪う → **100ms 後に自動非表示**（`auto_hide_on_focus_lost=true` 時）
- 表示中に `/o` 等で `snotra-settings` を起動 → 設定が focus を奪う → **メインは非表示にならない**（サイドカーガード）
- Escape → 非表示

Expected: 上記どおり。**設定起動で本体が消えたら STOP** — サイドカーガードが focus-lost 経路に効いていない。

- [ ] **Step 3: コミット**

```
git add src-tauri/src/
git commit -F <tmpfile>   # "feat(#532): SU2 blur 自動非表示 + Escape（focus 観測・100ms 猶予・サイドカーガード・G3）"
```

---

### Task 8: 位置永続（復元 + save-on-hide + 可視時終了保存）

egui window の位置を SPEC §8.2 どおり復元・保存する。復元は `show_main` の `position_on_target_monitor`（Task 4 で既に共有列に在る）。保存は save-on-hide + 可視時終了保存。

**Files:**
- Modify: `src-tauri/src/egui_shell/mod.rs`（`hide_main` に位置保存を追加）
- Modify: `src-tauri/src/main.rs`（`setup_exit_listener` に egui 可視時の位置保存）

**Interfaces:**
- Consumes: `position_on_target_monitor`（復元・共有列内）・既存の位置保存経路（`commands::save_search_placement` が使う `window.bin` 書き込み。相対座標算出は `monitor.rs`）。
- Produces: なし（副作用）。

- [ ] **Step 1: 位置保存経路を確認する**

`commands::save_search_placement`（`get_webview_window("main")` を使う・`commands/window.rs`）が呼ぶ `window.bin` 書き込み関数（`snotra-core` 側の placement 保存 or `monitor.rs`）を特定する。egui window（`get_window("main")`）の物理位置 + ターゲットモニター作業領域原点から**相対座標**を出して同じ形式で保存する共有ヘルパー `save_placement_relative(app, &window)` を `egui_shell` に用意する（WebView2 の保存ロジックを `&tauri::Window` で一般化・`monitor.rs` の相対座標算出を再利用）。

Run: `cargo build -p snotra`（型が合うこと）

- [ ] **Step 2: `hide_main` に save-on-hide を足す**

`egui_shell::hide_main` の `window.hide()` の**前**に、可視中の現在位置を保存する:

```rust
pub(crate) fn hide_main(app: &AppHandle, backend: MainBackend) {
    if let Some(window) = app.get_window("main") {
        if backend == MainBackend::Egui {
            save_placement_relative(app, &window); // save-on-hide（JS チョークポイントが無いため）
        }
        let _ = window.hide();
    }
    // ... 既存（main_visible=false・post_hide）
}
```

- [ ] **Step 3: `setup_exit_listener` に可視時終了保存を足す**

`src-tauri/src/main.rs:867` の `setup_exit_listener` 内、history/icon flush と並べて、egui かつ可視なら位置保存:

```rust
        if crate::trace::env_flag("SNOTRA_EGUI_MAIN")
            && handle_for_exit.try_state::<AppState>()
                .map(|s| s.main_visible.load(Ordering::SeqCst)).unwrap_or(false)
            && let Some(window) = handle_for_exit.get_window("main")
        {
            egui_shell::save_placement_relative(&handle_for_exit, &window);
        }
```

- [ ] **Step 4: 位置の復元・保存を確認する**

Run（PowerShell）:
```
$env:SNOTRA_EGUI_MAIN="1"; cargo run -p snotra
```
- ウィンドウをドラッグ移動（検索バー余白）→ Alt+Q で非表示 → Alt+Q で再表示 → **同じ位置に復元**
- マルチモニター: カーソルを別モニターへ → Alt+Q → `follow_cursor_monitor=true` ならそのモニターの作業領域内に出る（画面外に出ない = クランプ）
- 保存位置なしの初回相当 → ターゲットモニター中央

Expected: SPEC §8.2 どおり。位置が画面外へ出ない。

- [ ] **Step 5: コミット + PR**

```
git add src-tauri/src/
git commit -F <tmpfile>   # "feat(#532): SU2 位置永続（復元 + save-on-hide + 可視時終了保存）"
git push -u origin HEAD && gh pr create --fill
```

（PR 本文は SU2 の受け入れ条件チェックリスト。**マージ前に** `gh pr view <PR> --json closingIssuesReferences` で #532 を誤 close しないか確認＝ルート `CLAUDE.md` の squash マージ手順。#532 は親 issue ゆえ SU2 単独では閉じない。）

---

## Self-Review（spec 照合）

- **spec §backend seam（3 フック）** → Task 4（`pre_show`/`post_show`/`post_hide`・WV2 leaf 逐語移動）。✓
- **spec §状態機械（plan_hotkey/plan_ui_action・合流点）** → Task 1（純粋核）+ Task 6（controller 適用）。✓
- **spec §blur 自動非表示（100ms・ゲート・サイドカーガード・policy を view 側）** → Task 7。✓
- **spec §フラグと生成（windows:[]・build_webview2_window・egui create・label "main"）** → Task 2（G0）+ Task 5。✓
- **spec §placeholder view + font-first** → Task 3。✓
- **spec §位置永続（復元・save-on-hide・可視時終了保存）** → Task 8。✓
- **spec §再利用部品（残留 Alt #558・monitor.rs・window.bin）** → Task 4（Alt）+ Task 8（位置）。✓
- **spec 検証ゲート G0/G1/G2/G3/G4** → G0=Task 2 / G1=Task 4 / G2=Task 5(partial)+Task 6(full) / G3=Task 7 / G4=Task 6。✓
- **spec §受け入れ条件 1-6** → 全 Task を通じて。SPEC §8 一致=Task 5-8 のスモーク、flag OFF 不変=G0+G1、純関数=Task 1、policy が src-tauri=Task 7、font-first=Task 3、clippy/test/子孫0=各境界。✓

**未解決の実装時確認（各タスクの mini-gate で潰す・fabrication を避けた箇所）:**
- Task 4 Step 1: `WebviewWindow` → `&tauri::Window` の手段（`as_ref().window()` / `get_window`）。
- Task 5 Step 1: `Window::builder` の `skip_taskbar`/`always_on_top` メソッド名（無ければ setup 後 setter）。
- Task 5 Step 2: `RuntimeError` → setup エラー型の変換。
- Task 7 Step 1: `auto_hide_on_focus_lost` の config.rs 既定値。
- Task 8 Step 1: `window.bin` 保存関数の所在（`snotra-core` placement or `monitor.rs`）と `&tauri::Window` 一般化。
