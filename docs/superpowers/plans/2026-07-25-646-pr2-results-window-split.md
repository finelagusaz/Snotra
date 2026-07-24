# #646 PR2: 結果窓の分離 実装計画

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 結果リストを独立窓 "results"(フォーカスを取らない従属カード)へ分離し、透明ギャップ・DWM 角丸・実件数フィット・メイン窓ドラッグ移動を実現する。

**Architecture:** spec `docs/superpowers/specs/2026-07-24-646-two-window-ui-design.md` の決定 3〜7・10。setup で 2 窓目 "results" を生成し(`focusable(false)` → tao が `WS_EX_NOACTIVATE` を自動適用)、`snotra-egui-runtime` の複数窓機構(`attach` は label ごと・窓ごとに Context/renderer/scheduler)に `ResultsView` を載せる。状態は「main が所有・results は写しを描く」一方向: main の update() が `RowsSnapshot` を発行して results ctx を wake し、クリックは共有スロット経由で main へ逆流する。**results 窓の可視性・サイズ・位置の driver は main の update() が担う**(spec 決定 6 の「results view が毎フレーム突き合わせ」から変更 — hidden 窓は update() が走らない SU5 要石により、隠れた results は自分を show できないため。従属という本質は不変)。ドラッグは runtime に配管済みの `frame.drag_window()` を呼ぶだけ。

**Tech Stack:** Rust / egui 0.35(softbuffer)/ tauri v2.11(`focusable`・`start_dragging`)/ windows 0.62(`Win32_Graphics_Dwm` を新規有効化)

## Global Constraints

- **main へ直接コミットしない**。作業ブランチ: `feat/646-pr2-results-window`(本計画 docs PR マージ後の main から作成)
- bash HEREDOC 禁止。複数行コミットメッセージは PowerShell here-string `@'...'@`(閉じ `'@` は行頭)/ パス区切りは `/`
- PostToolUse hook が `.rs` 編集ごとに clippy + crate テストを自動実行(沈黙 = 合格)。Red/Green は明示コマンドで確認。テストは `cargo test -p <crate>`(**`--lib` は環境不可**)
- `--no-verify` 禁止 / コミット末尾: `Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>`
- spec の定数(変更禁止): `window_gap: u32 = 4`(config `[visual]`・serde default・live-read)/ 結果窓高さ = `min(実件数, max_results) × row_height + 8.0`(実件数フィット・決定 7)/ 角丸 = DWM `DWMWCP_ROUND` 固定(config 化しない)/ ドラッグ掴み領域 = メイン窓の入力欄以外の全域(決定 10)
- **窓生成は setup 限定**(`src-tauri/CLAUDE.md`「ウィンドウ生成の制約」)・**状態を変えたら対象窓の ctx を `request_repaint()`**(イベント駆動 wake 規範を窓間に拡張・spec 決定 5)・**hidden 窓は update() が走らない**(SU5 要石)
- `blur_should_hide`(lifecycle.rs)は**無改修**(results は `focusable(false)` でフォーカスを持ち得ないため・決定 4)
- spec 決定 5 の「indexing 中の案内 overlay は results 窓へ移動」は**実装しない**(errata: 実物の overlay は bar の TextEdit rect 上に描かれており〔view.rs `overlay_text` 節〕、main に残るのが as-built。Task 7 で spec に errata 追記)

---

### Task 1: config に window_gap を追加(snotra-core)

**Files:**
- Modify: `snotra-core/src/config.rs`(`VisualConfig`・default fn・`Default` impl・tests)

**Interfaces:**
- Produces: `VisualConfig.window_gap: u32`(既定 4)。Task 5 が `config().visual.window_gap` で読む

- [ ] **Step 1: 失敗するテストを書く**

既存 `visual_padding_defaults_for_missing_keys`(PR1 で追加・`mod tests` 内)に assert を 1 行足す:

```rust
        assert_eq!(config.visual.window_gap, 4);
        assert_eq!(VisualConfig::default().window_gap, 4);
```

- [ ] **Step 2: 落ちることを確認する(Red)**

Run: `cargo test -p snotra-core visual_padding_defaults_for_missing_keys`
Expected: FAIL(`no field window_gap` のコンパイルエラー)

- [ ] **Step 3: 最小実装**

`VisualConfig` の `bar_padding` の直後へ:

```rust
    #[serde(default = "default_window_gap")]
    pub window_gap: u32,
```

default fn 群(`default_bar_padding` の隣)へ:

```rust
/// #646 PR2: メイン窓と結果窓の隙間 px(透明ギャップ・決定 6)。
fn default_window_gap() -> u32 {
    4
}
```

`impl Default for VisualConfig` へ `window_gap: default_window_gap(),` を追加。

- [ ] **Step 4: 通ることを確認する(Green)**

Run: `cargo test -p snotra-core`
Expected: 全 PASS

- [ ] **Step 5: コミット**

```powershell
git add snotra-core/src/config.rs && git commit -m @'
feat: VisualConfig に window_gap を追加(#646 PR2)

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>
'@
```

---

### Task 2: layout.rs に 2 窓の高さ純粋核を追加

**Files:**
- Modify: `src-tauri/src/egui_shell/layout.rs`(新関数 2 つ + tests。既存 `HeightParams`/`compute_window_height` は**この Task では触らない** — 撤去は Task 5 で view の切替と同時)

**Interfaces:**
- Produces: `pub fn main_window_height(bar_height: f64, toast_height: Option<f64>) -> f64` / `pub fn results_window_height(result_count: usize, max_results: u32, row_height: f64) -> f64`。Task 5 が使う

- [ ] **Step 1: 失敗するテストを書く**

`layout.rs` の `mod tests` へ:

```rust
    /// #646 PR2 決定 6: main 窓は bar(+toast)のみで、結果による伸縮をしない。
    #[test]
    fn main_height_is_bar_plus_optional_toast() {
        assert_eq!(main_window_height(43.0, None), 43.0);
        assert_eq!(main_window_height(43.0, Some(43.0)), 86.0);
    }

    /// #646 PR2 決定 7: 結果窓は実件数フィット(上限 max_results)+ padding 8。
    #[test]
    fn results_height_fits_actual_count_capped_at_max() {
        let row = 37.0;
        assert_eq!(results_window_height(3, 8, row), 3.0 * row + 8.0); // 実件数
        assert_eq!(results_window_height(20, 8, row), 8.0 * row + 8.0); // 上限で頭打ち
        assert_eq!(results_window_height(0, 8, row), 0.0); // 0 件は非表示(高さ 0 = 呼び出し側で hide)
    }
```

- [ ] **Step 2: 落ちることを確認する(Red)**

Run: `cargo test -p snotra main_height_`
Expected: FAIL(関数未定義のコンパイルエラー)

- [ ] **Step 3: 最小実装**

`Metrics` の直後へ:

```rust
/// main 窓の高さ(#646 PR2 決定 6)。bar(+toast)のみで結果に伸縮しない。
pub fn main_window_height(bar_height: f64, toast_height: Option<f64>) -> f64 {
    bar_height + toast_height.unwrap_or(0.0)
}

/// 結果窓の高さ(#646 PR2 決定 7)。実件数フィット・上限 max_results・padding 8。
/// 0 件は 0.0(呼び出し側が hide する契約)。
pub fn results_window_height(result_count: usize, max_results: u32, row_height: f64) -> f64 {
    let n = result_count.min(max_results as usize);
    if n == 0 {
        0.0
    } else {
        n as f64 * row_height + 8.0
    }
}
```

注: bin-only crate ゆえ消費者が入る Task 5 まで clippy dead_code が出る。PR1 Task 2 と同じ前例(`search_state.rs` 系)に従い `#[allow(dead_code)]` を両関数へ付け、**Task 5 で消費配線と同時に除去**する(ledger 申し送り)。

- [ ] **Step 4: 通ることを確認する(Green)**

Run: `cargo test -p snotra`
Expected: 全 PASS(既存 `compute_window_height` テスト含む)

- [ ] **Step 5: コミット**

```powershell
git add src-tauri/src/egui_shell/layout.rs && git commit -m @'
feat: 2 窓の高さ純粋核 main_window_height / results_window_height(#646 PR2)

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>
'@
```

---

### Task 3: results 窓の生成 + ResultsView 骨組み + 角丸 + 共有状態

**Files:**
- Modify: `src-tauri/Cargo.toml`(windows features へ `"Win32_Graphics_Dwm",` を追加)
- Create: `src-tauri/src/egui_shell/results_view.rs`(骨組み)
- Modify: `src-tauri/src/egui_shell/mod.rs`(`mod results_view;`・`create()` の 2 窓目生成・DWM helper・`EguiShellState.results_ctx`・`ResultsShared` 管理・`wake_view` の両窓化・`hide_egui_main` の両窓 hide)
- Modify: `src-tauri/CLAUDE.md`(モジュール構成の egui_shell 行へ `results_view.rs` を追記 — ファイル追加時の索引義務・AGENTS.md 条件別チェック)

**Interfaces:**
- Consumes: なし(骨組み)
- Produces: `pub(crate) struct RowsSnapshot { pub rows: Vec<SearchResult>, pub selected: usize, pub show: bool }`(derive `Clone, PartialEq, Default`)/ `pub(crate) struct ResultsShared { pub snapshot: Mutex<RowsSnapshot>, pub clicked: Mutex<Option<usize>> }`(managed state)/ `EguiShellState.results_ctx: Mutex<Option<egui::Context>>` / `pub(crate) fn wake_results(app: &AppHandle)`。Task 4・5 が使う

- [ ] **Step 1: Cargo.toml に DWM feature を追加**

`src-tauri/Cargo.toml` の windows クレート features 配列(`"Win32_UI_WindowsAndMessaging",` がある並び)へ `"Win32_Graphics_Dwm",` を 1 行追加。hook が cargo check を自動実行(沈黙 = 合格)。

- [ ] **Step 2: results_view.rs の骨組みを書く**

```rust
//! 結果リスト窓("results")の egui view(#646 PR2)。main(SearchWindowView)が発行する
//! RowsSnapshot を描くだけの従属 view——検索状態の所有者は main のまま(一方向データフロー・
//! spec 決定 5)。クリックは ResultsShared.clicked へ積んで main を wake する(遅延 dispatch)。
//! 窓の可視性・サイズ・位置の driver は main 側(hidden 窓は update() が走らないため)。

use tauri::Manager;

use crate::egui_shell::EguiShellState;

/// main が毎フレーム発行する描画用スナップショット(spec 決定 5)。
#[derive(Clone, Default, PartialEq)]
pub(crate) struct RowsSnapshot {
    pub rows: Vec<snotra_core::engine::SearchResult>,
    pub selected: usize,
    pub show: bool,
}

/// main と results が共有する一方向フローの入れ物(managed state)。
#[derive(Default)]
pub(crate) struct ResultsShared {
    pub snapshot: std::sync::Mutex<RowsSnapshot>,
    /// クリックされた行 index(last-wins)。main の update() が take して起動処理する。
    pub clicked: std::sync::Mutex<Option<usize>>,
}

pub(crate) struct ResultsView {
    app_handle: tauri::AppHandle,
}

impl ResultsView {
    pub(crate) fn new(app_handle: tauri::AppHandle) -> Self {
        Self { app_handle }
    }
}

impl snotra_egui_runtime::EguiView for ResultsView {
    fn setup(&mut self, context: &egui::Context) {
        // 日本語フォント: main と同じ config font_family を適用(ctx は窓ごとに独立)。
        let font_family = self
            .app_handle
            .try_state::<crate::AppState>()
            .map(|s| s.engine.lock().unwrap().config().visual.font_family.clone())
            .unwrap_or_else(|| "Segoe UI".to_string());
        crate::egui_shell::view::configure_japanese_font(context, &font_family);
        // 外部 wake 用に ctx を登録(main の egui_ctx と同型・EguiShellState.results_ctx)。
        if let Some(sh) = self.app_handle.try_state::<EguiShellState>()
            && let Ok(mut guard) = sh.results_ctx.lock()
        {
            *guard = Some(context.clone());
        }
    }

    fn update(&mut self, _ui: &mut egui::Ui, _frame: &mut snotra_egui_runtime::RuntimeFrame) {
        // Task 4 で snapshot 描画を実装。骨組みでは何も描かない(窓は visible:false のまま)。
    }
}
```

注: `SearchResult` のパスは実物に合わせる(`snotra_core::engine::SearchResult` を grep で確認。`PartialEq` derive が無ければ snotra-core 側に `PartialEq` を追加 — フィールドは文字列/数値/bool のみのはず。追加した場合はその旨をコミットメッセージに書く)。`configure_japanese_font` が `pub(crate)` でなければ view.rs 側で昇格する。

- [ ] **Step 3: mod.rs へ配線する**

1. `mod results_view;` を既存 `mod view;` の並びへ追加、`pub(crate) use results_view::{ResultsShared, RowsSnapshot};` を既存 re-export の並びへ
2. `EguiShellState` へフィールド追加:

```rust
    /// results 窓の egui Context(外部 wake 用・egui_ctx と同型・#646 PR2)。
    pub results_ctx: Mutex<Option<egui::Context>>,
```

3. `wake_view` の隣へ:

```rust
/// results 窓を起こす(#646 PR2)。snapshot 更新・config 変更を反映させる wake。
pub(crate) fn wake_results(app: &tauri::AppHandle) {
    if let Some(sh) = app.try_state::<EguiShellState>()
        && let Ok(guard) = sh.results_ctx.lock()
        && let Some(ctx) = guard.as_ref()
    {
        ctx.request_repaint();
    }
}
```

4. `register_config_wake_listeners` 等が呼ぶ `wake_view(&handle)` の各所で `wake_results(&handle);` も併せて呼ぶ(config-applied は両窓の visuals に効くため。呼び出し箇所を `wake_view` の grep で数え上げて全部に併記する)
5. `create()` の main 窓 build 後へ results 窓を生成・attach:

```rust
    // #646 PR2: 結果リスト窓。focusable(false) で tao が WS_EX_NOACTIVATE を自動適用し
    // (tao window_state.rs: !FOCUSABLE → style_ex |= WS_EX_NOACTIVATE)、クリックしても
    // フォーカスはメインの入力欄から動かない(決定 4)。可視性・サイズ・位置は main の
    // update() が駆動する(hidden 窓は update() が走らないため自分では show できない)。
    let results = tauri::Window::builder(app, "results")
        .title("Snotra Results")
        .inner_size(window_width, 100.0) // 初期値。実高は main が実件数フィットで設定
        .decorations(false)
        .resizable(false)
        .skip_taskbar(true)
        .always_on_top(true)
        .focusable(false)
        .background_color(bg_color)
        .visible(false)
        .build()?;
    #[cfg(windows)]
    {
        apply_rounded_corners(&window); // main にも適用(輪郭言語を揃える・決定 4)
        apply_rounded_corners(&results);
    }
    runtime.attach(results, results_view::ResultsView::new(app_handle.clone()))?;
```

(main の `runtime.attach(window, ...)` 行との順序に注意 — `window` は attach で move されるため、`apply_rounded_corners(&window)` は attach **前**に呼ぶ。既存の attach 行を末尾に保つ形へ並べ替える。)

6. DWM helper を mod.rs へ:

```rust
/// DWM に窓の角丸を依頼する(#646 PR2 決定 4)。Windows 11(build 22000+)のみ有効で、
/// Windows 10 ではエラーを黙って握りつぶす(装飾なしで受容・best-effort)。
/// softbuffer は AA を持たず自前角丸は品質が出ないため OS 機構に委ねる。
#[cfg(windows)]
fn apply_rounded_corners(window: &tauri::Window) {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::Graphics::Dwm::{
        DWM_WINDOW_CORNER_PREFERENCE, DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_ROUND,
        DwmSetWindowAttribute,
    };
    let Ok(hwnd) = window.hwnd() else { return };
    let pref: DWM_WINDOW_CORNER_PREFERENCE = DWMWCP_ROUND;
    unsafe {
        let _ = DwmSetWindowAttribute(
            HWND(hwnd.0),
            DWMWA_WINDOW_CORNER_PREFERENCE,
            &pref as *const _ as *const core::ffi::c_void,
            std::mem::size_of::<DWM_WINDOW_CORNER_PREFERENCE>() as u32,
        );
    }
}
```

7. `main.rs`(または manage 集約箇所)で `app.manage(crate::egui_shell::ResultsShared::default());` を既存 `UpdaterUiState` 等の manage の並びへ(場所は `manage(` の grep で確認)
8. `hide_egui_main` の `window.hide()` 直後(main_visible 更新より前)へ:

```rust
    // #646 PR2: 従属窓も同時に隠す(決定 6)。show 側は main の update() が snapshot の
    // show 判定で駆動するため、ここが唯一の外部 hide 経路(対称は main update 内の show)。
    if let Some(results) = app.get_window("results") {
        let _ = results.hide();
    }
```

- [ ] **Step 4: 検証**

Run: `cargo clippy -p snotra --all-targets -- -D warnings` → 警告 0(未消費警告が出る項目は `#[allow(dead_code)]` + Task 4/5 除去の申し送り)
Run: `cargo test -p snotra` → 全 PASS
Run: `npm run governance:check` → PASS(モジュール索引に results_view.rs を追加済みであること)

- [ ] **Step 5: コミット**

```powershell
git add src-tauri/Cargo.toml src-tauri/src/egui_shell/results_view.rs src-tauri/src/egui_shell/mod.rs src-tauri/CLAUDE.md && git commit -m @'
feat: results 窓の生成 + ResultsView 骨組み + DWM 角丸(#646 PR2・決定 4)

focusable(false) で WS_EX_NOACTIVATE を tao に委ね、角丸は DWM best-effort。
窓は visible:false のまま(可視化は Task 4 の main driver から)。

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>
'@
```

---

### Task 4: 行描画の results 窓への移設 + snapshot 一方向フロー + main driver

**Files:**
- Modify: `src-tauri/src/egui_shell/results_view.rs`(update() 本実装・行描画一式の受け入れ)
- Modify: `src-tauri/src/egui_shell/view.rs`(行描画の撤去・snapshot 発行・clicked 消費・results 窓 driver・main 高さの分割)
- Modify: `src-tauri/src/egui_shell/layout.rs`(`HeightParams`/`compute_window_height` と旧テスト 3 件の削除・Task 2 の `#[allow(dead_code)]` 除去)

**Interfaces:**
- Consumes: Task 2 の `main_window_height`/`results_window_height`・Task 3 の `RowsSnapshot`/`ResultsShared`/`wake_results`
- Produces: `pub(crate) struct RowTheme` と `pub(crate) fn row_theme(app: &tauri::AppHandle) -> RowTheme`(results_view.rs へ移設・view.rs はバー/toast 用に import)/ `pub(crate) fn truncate_middle`(移設・view.rs のテストも移動)

- [ ] **Step 1: 描画部品を results_view.rs へ移設する**

view.rs から以下を results_view.rs へ**移動**(コピーでなく)し、view.rs 側は import に切替:

- `RowTheme` struct と `row_theme()` — メソッドから `pub(crate) fn row_theme(app: &tauri::AppHandle) -> RowTheme` の自由関数へ(中身は現行どおり: config visual live-read + `layout::path_size`)。view.rs の `self.row_theme()` 呼び出し(バー・toast・overlay 用)は `results_view::row_theme(&self.app_handle)` へ
- `draw_result_row`(8 引数のまま・`#[allow(clippy::too_many_arguments)]` ごと)
- `draw_icon_fallback`・`truncate_middle`(+ view.rs の truncate_middle テストを results_view.rs の `mod tests` へ移動)

- [ ] **Step 2: ResultsView::update() を実装する**

```rust
    fn update(&mut self, ui: &mut egui::Ui, _frame: &mut snotra_egui_runtime::RuntimeFrame) {
        let Some(shared) = self.app_handle.try_state::<ResultsShared>() else {
            return;
        };
        let snapshot = shared.snapshot.lock().unwrap().clone();
        if !snapshot.show {
            return; // 窓は main が hide 済みのはず(backstop で何も描かない)
        }
        let theme = row_theme(&self.app_handle);
        let metrics = crate::egui_shell::read_metrics(&self.app_handle);
        let show_icons = false; // Task 5 でアイコン移設(この Task は placeholder 描画)
        // 選択変化時のみ scroll_to_me(#632 のゲートを view 内フィールドで維持)。
        let do_scroll = self.last_scrolled_selected != Some(snapshot.selected);
        let mut clicked: Option<usize> = None;
        egui::ScrollArea::vertical().show(ui, |ui| {
            for (i, result) in snapshot.rows.iter().enumerate() {
                let sel = i == snapshot.selected;
                if draw_result_row(
                    ui,
                    result,
                    sel,
                    sel && do_scroll,
                    None, // icon: Task 5 まで常に placeholder
                    show_icons,
                    &theme,
                    metrics.row_height as f32,
                ) {
                    clicked = Some(i);
                }
            }
        });
        if do_scroll {
            self.last_scrolled_selected = Some(snapshot.selected);
        }
        // クリック逆流(決定 5): 共有スロットへ積み、main を起こして起動処理させる。
        // ToastAction と同じ遅延 dispatch 型——起動ロジックは main の一箇所に保つ。
        if let Some(i) = clicked {
            *shared.clicked.lock().unwrap() = Some(i);
            crate::egui_shell::wake_view(&self.app_handle);
        }
    }
```

`ResultsView` にフィールド `last_scrolled_selected: Option<usize>` を追加(`new` で `None`)。

- [ ] **Step 3: view.rs — 行描画の撤去と snapshot 発行・driver・clicked 消費**

update() の結果リストブロック(`if show_results { ... ScrollArea ... }` と `if let Some(i) = clicked` と `last_scrolled_selected` フィールド)を撤去し、以下へ置換:

```rust
        // #646 PR2 決定 5: 結果は snapshot として発行し、描画は results 窓(ResultsView)が担う。
        // 変化があったフレームだけ store + wake(毎フレーム wake だと results が常時回る)。
        if let Some(shared) = self.app_handle.try_state::<crate::egui_shell::ResultsShared>() {
            let snapshot = crate::egui_shell::RowsSnapshot {
                rows: if show_results { self.state.results().to_vec() } else { Vec::new() },
                selected: self.state.selected(),
                show: show_results,
            };
            {
                let mut guard = shared.snapshot.lock().unwrap();
                if *guard != snapshot {
                    *guard = snapshot;
                    drop(guard);
                    crate::egui_shell::wake_results(&self.app_handle);
                }
            }
            // クリック逆流の消費(決定 5): 起動ロジックは main の一箇所に保つ。
            let clicked = shared.clicked.lock().unwrap().take();
            if let Some(i) = clicked {
                self.activate_or_execute(i, &ctx);
            }
        }
```

(クリック index は取り込み時点の rows と snapshot 発行時点の rows が 1 フレームずれうるが、rows が変わる操作〔打鍵〕とクリックは同時に起きず、ずれても既存の「クリック時 index で起動」と同じ最悪度。既存 ToastAction と同水準の遅延 dispatch として受容。)

続いて動的高さブロック(`compute_window_height` 呼び出し + `HeightParams`)を置換:

```rust
        // #646 PR2 決定 6: main は bar(+toast)のみ。結果窓の可視性・サイズ・位置も
        // ここ(毎フレーム走る main)が駆動する——hidden 窓は update() が走らず自分では
        // show できない(SU5 要石)。位置 → サイズ → show の順(main の show と同じ制約)。
        let height = crate::egui_shell::layout::main_window_height(
            metrics.bar_height,
            has_toast.then_some(metrics.toast_height),
        );
        let width = self.window_width();
        if (height - self.last_set_height).abs() > 0.5 || (width - self.last_set_width).abs() > 0.5 {
            self.last_set_height = height;
            self.last_set_width = width;
            if let Some(window) = self.app_handle.get_window("main") {
                let _ = window.set_size(tauri::LogicalSize::new(width, height));
            }
            ui.ctx().request_repaint();
        }
        self.drive_results_window(show_results, width, &metrics);
```

新メソッド(view.rs・`drive_results_window`):

```rust
    /// results 窓の可視性・サイズ・位置を main から駆動する(#646 PR2 決定 6)。
    /// 位置 = main の直下 + window_gap(従属)。デルタガードで無変化フレームは no-op。
    /// show は focusable(false) 窓ゆえフォーカスを奪わない(決定 4)。
    fn drive_results_window(
        &mut self,
        show_results: bool,
        width: f64,
        metrics: &crate::egui_shell::layout::Metrics,
    ) {
        let Some(results) = self.app_handle.get_window("results") else {
            return;
        };
        let Some(main) = self.app_handle.get_window("main") else {
            return;
        };
        let count = self.state.results().len();
        let res_h = crate::egui_shell::layout::results_window_height(
            count,
            self.max_results(),
            metrics.row_height,
        );
        let visible = show_results && res_h > 0.0;
        if !visible {
            if self.last_results_visible {
                let _ = results.hide();
                self.last_results_visible = false;
            }
            return;
        }
        // 位置: main の外形直下 + gap(物理座標。gap は論理 px を scale で換算)。
        let gap = self
            .app_handle
            .try_state::<crate::AppState>()
            .map(|s| s.engine.lock().unwrap().config().visual.window_gap)
            .unwrap_or(4) as f64;
        if let (Ok(pos), Ok(size), Ok(scale)) =
            (main.outer_position(), main.outer_size(), main.scale_factor())
        {
            let target = tauri::PhysicalPosition::new(
                pos.x,
                pos.y + size.height as i32 + (gap * scale).round() as i32,
            );
            if self.last_results_pos != Some((target.x, target.y)) {
                let _ = results.set_position(target);
                self.last_results_pos = Some((target.x, target.y));
            }
        }
        if (res_h - self.last_results_height).abs() > 0.5
            || (width - self.last_set_width).abs() > 0.5
        {
            let _ = results.set_size(tauri::LogicalSize::new(width, res_h));
            self.last_results_height = res_h;
        }
        if !self.last_results_visible {
            let _ = results.show();
            self.last_results_visible = true;
        }
        crate::egui_shell::wake_results(&self.app_handle);
    }
```

フィールド追加(初期値): `last_results_visible: bool = false` / `last_results_height: f64 = 0.0` / `last_results_pos: Option<(i32, i32)> = None`。**reset-on-show ブロック(`reset_pending` 消費)で 3 つとも初期値へ戻す**(hide_egui_main が外部 hide した後の再 show で stale ガードが sh を妨げないように)。

- [ ] **Step 4: layout.rs の旧高さモデルを撤去する**

`HeightParams`・`compute_window_height`・テスト 3 件(`collapsed_is_search_bar_only`/`expanded_is_bar_plus_rows_plus_padding`/`toast_adds_height`)と `params()` ヘルパーを削除。Task 2 の `#[allow(dead_code)]` 2 つを除去(消費配線が入ったため)。view.rs の `use` から `compute_window_height`/`HeightParams` を外す。

- [ ] **Step 5: 検証**

Run: `cargo clippy -p snotra --all-targets -- -D warnings` → 警告 0
Run: `cargo test -p snotra` → 全 PASS(truncate_middle テストは results_view.rs 側で PASS)
Run: `cargo test -p snotra-core` → 全 PASS(SearchResult へ PartialEq を足した場合)

- [ ] **Step 6: コミット**

```powershell
git add -A src-tauri/src snotra-core/src && git commit -m @'
feat: 結果リストを results 窓へ移設(snapshot 一方向 + main driver)(#646 PR2・決定 5/6/7)

- ResultsView が RowsSnapshot を描き、クリックは共有スロットで main へ逆流
- main は bar(+toast)固定高になり、results の可視性/サイズ/位置を駆動
- 旧 compute_window_height/HeightParams を撤去(実件数フィットへ全面移行)

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>
'@
```

---

### Task 5: アイコンパイプラインの ResultsView 移設

**Files:**
- Modify: `src-tauri/src/egui_shell/results_view.rs`(icon 一式の受け入れ)
- Modify: `src-tauri/src/egui_shell/view.rs`(icon 一式の撤去)

**Interfaces:**
- Consumes: Task 4 の ResultsView・snapshot
- Produces: なし(内部完結)

- [ ] **Step 1: icon driver 一式を移設する**

view.rs から results_view.rs へ**移動**: フィールド `icon_textures: HashMap<String, egui::TextureHandle>`・`icon_missing: HashSet<String>`・`icon_pending: HashSet<String>`・`icon_tx`/`icon_rx`(channel)・`spawn_icon_load`・`request_icons_for_results`(引数を `&self.state.results()` から `&snapshot.rows` へ)・update() 内の icon drain(`ctx.load_texture` — **results の ctx で呼ぶ**)・`retain_visible` 呼び出し。`show_icons()`(config live-read)も自由関数化して移設。

worker の wake は `egui_ctx.request_repaint()` のまま(clone するのが results の ctx になるだけ)。ResultsView::update() の `show_icons = false` を実 config 読みへ差し替え、`draw_result_row` の `icon` 引数へ `self.icon_textures.get(&result.path)` を渡す。

view.rs 側: reset-on-show ブロックの icon クリア 3 行は削除(results 側は `retain_visible` が空 rows で自然に全クリアする——snapshot.rows が空になれば次フレームで刈られる)。

- [ ] **Step 2: 検証**

Run: `cargo clippy -p snotra --all-targets -- -D warnings` → 警告 0
Run: `cargo test -p snotra` → 全 PASS

- [ ] **Step 3: コミット**

```powershell
git add src-tauri/src/egui_shell/results_view.rs src-tauri/src/egui_shell/view.rs && git commit -m @'
feat: アイコンパイプラインを ResultsView へ移設(#646 PR2・決定 5)

TextureHandle は窓の ctx 従属のため、行描画と同じ窓に置く。

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>
'@
```

---

### Task 6: メイン窓のドラッグ移動 + 移動中の追従

**Files:**
- Modify: `src-tauri/src/egui_shell/view.rs`(背景ドラッグ検出)
- Modify: `snotra-egui-runtime/src/runtime.rs`(`Moved` で repaint 要求)
- Modify: `snotra-egui-runtime/CLAUDE.md`(不変条件へ Moved 挙動を 1 行追記)

**Interfaces:**
- Consumes: runtime 既存の `RuntimeFrame::drag_window()`(→ `tauri::Window::start_dragging()`・runtime.rs 配管済み)

- [ ] **Step 1: 背景ドラッグ検出を追加する**

view.rs update() の**先頭付近(TextEdit・toast 描画より前)**へ:

```rust
        // #646 PR2 決定 10: 入力欄以外の全域を掴んでドラッグ移動。背景 interact を先に
        // 登録し、後続ウィジェット(TextEdit・toast ボタン)はヒットテストで勝つ(egui は
        // 後着が上位)。start_dragging は runtime の frame コマンド経由(配管済み)。
        let drag_resp = ui.interact(
            ui.max_rect(),
            egui::Id::new("main-window-drag"),
            egui::Sense::drag(),
        );
        if drag_resp.drag_started_by(egui::PointerButton::Primary) {
            frame.drag_window();
        }
```

(update のシグネチャが `frame` を受けていることは EguiView trait で保証済み。view.rs の update 実装の引数名を確認して合わせる。)

- [ ] **Step 2: runtime に Moved → repaint を追加する**

`snotra-egui-runtime/src/runtime.rs` の `EguiWindow::on_window_event`(`Focused(true)` 分岐の隣)へ:

```rust
        if matches!(
            event,
            tauri_runtime_wry::tao::event::WindowEvent::Moved(_)
        ) {
            // 窓移動フレームで repaint を要求する。移動そのものは egui 入力にならないが、
            // 従属窓の追従(#646 PR2: main の update() が results を再配置する)を
            // ネイティブ移動ループ中も駆動するために必要。
            return true;
        }
```

`snotra-egui-runtime/CLAUDE.md` の不変条件へ 1 行: 「`Moved` は repaint 要求として扱う(egui 入力へは渡さない)——従属窓の追従を移動中も駆動する(#646 PR2)」

- [ ] **Step 3: 検証**

Run: `cargo clippy --workspace --all-targets -- -D warnings` → 警告 0
Run: `cargo test -p snotra-egui-runtime` → 全 PASS
実機確認は Task 8 の GUI スモークに委ねる。**もしドラッグ中の追従がカクつく/追従しない場合のフォールバック(spec 決定 10 に明記済み)**: `drive_results_window` の冒頭で「ドラッグ中(`drag_resp.dragged()` を view フィールドへ保存)は results を hide し、drop 後の最初のフレームで再表示」へ切り替える。

- [ ] **Step 4: コミット**

```powershell
git add src-tauri/src/egui_shell/view.rs snotra-egui-runtime/src/runtime.rs snotra-egui-runtime/CLAUDE.md && git commit -m @'
feat: メイン窓の背景ドラッグ移動 + Moved repaint(#646 PR2・決定 10)

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>
'@
```

---

### Task 7: 文書同期(SPEC・architecture・spec errata)

**Files:**
- Modify: `SPEC.md`(2 窓構成・実件数フィット・ドラッグ移動・window_gap キー)
- Modify: `docs/architecture.md`(窓構成の記述)
- Modify: `docs/superpowers/specs/2026-07-24-646-two-window-ui-design.md`(errata 2 件)

**Interfaces:** なし(文書のみ)

- [ ] **Step 1: 同期する(行番号でなく引用文 grep で特定)**

1. SPEC の動的高さ節(「show 時に毎回検索バー高さ…リセットしてから結果に応じて拡張する」・PR1 で更新済みの文): main は伸縮せず、結果は独立窓 "results"(実件数フィット `min(件数, max_results) × row_height + 8`・main 直下 + `window_gap`〔既定 4px〕・focusable(false) でフォーカスを取らない・DWM 角丸)である旨へ
2. SPEC §11(PR1 で入れた `[visual]` キーの文): `window_gap`(既定 4)を追記
3. SPEC のウィンドウ操作系の節(show/hide を記す §): メイン窓は入力欄以外の全域ドラッグで移動可・hide は両窓同時、を追記
4. `docs/architecture.md` の egui 窓構成記述(PR1 で bar_height 化した 2 箇所の周辺): 2 窓構成(main = バー + toast / results = 従属カード・main driver 駆動)へ
5. design spec へ errata 追記(節「決定 5」「決定 6」の末尾): (a) overlay は実物が bar 上描画のため main に残した(結果窓へは移動しない) (b) results の可視性・位置・サイズの driver は results view でなく main の update()(hidden 窓は update() が走らないため)。日付と PR2 実装時の判断であることを 1 行ずつ
6. `scripts/smoke-egui.ps1` の前提確認(機能削除トリガー・AGENTS.md 条件別チェック): trace イベント名(`egui_show:done`/`egui_hide:done`)と hotkey 前提は本 PR で不変であることを目視確認(変更不要のはず。smoke の実行自体は Task 8)

- [ ] **Step 2: governance:check**

Run: `npm run governance:check`
Expected: PASS

- [ ] **Step 3: コミット**

```powershell
git add SPEC.md docs/architecture.md docs/superpowers/specs/2026-07-24-646-two-window-ui-design.md && git commit -m @'
docs: 2 窓構成の as-built 同期 + design spec errata 2 件(#646 PR2)

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>
'@
```

---

### Task 8: スモーク検証 + PR 作成

**Files:** なし(検証と PR のみ)

- [ ] **Step 1: 自動スモーク**

Run: `cargo build --release`(数分)
Run: `pwsh -NoProfile -File scripts/smoke-egui.ps1 -HotkeyVks "17,75"`(実機 config は Ctrl+K。CI は既定 Alt+Q + `-SeedConfig`)
Expected: PASS(show/hide 観測・webview delta 0。空クエリでは results 窓は出ないため既存シナリオは不変)

- [ ] **Step 2: 実機 GUI スモーク(人間ペア)**

ユーザーに依頼(release 起動): (a) 検索すると結果が**独立カード**で main 直下 + 4px ギャップに出る・両窓とも角丸 (b) 結果行クリックで起動し、**クリック中も入力欄のキャレットが点滅し続ける**(フォーカス非奪取・決定 4) (c) 結果件数が少ないときカードが中身にフィット(下に空白が残らない・決定 7) (d) バーの余白(入力欄の外)を掴んでドラッグすると窓が動き、結果カードが追従する(カクつくならフォールバック切替を判断・決定 10) (e) ドラッグ後 hide→show で位置が復元される(位置永続との合流) (f) toast 表示中(SNOTRA_EGUI_FAKE_UPDATE=1)は結果カードが toast の分だけ下がる (g) Escape/blur で両窓が同時に消える

- [ ] **Step 3: push + PR**

```powershell
git push -u origin HEAD && gh pr create --title "#646 PR2: 結果窓の分離(2 窓構成)" --body-file <一時ファイル>
```

PR 本文: closing keyword 禁止(`Refs #646` のみ——**#646 を閉じるかはマージ後にユーザーと判断**。PR2 で issue の主要素は揃うが、閉じる場合も本文 `Closes` でなく明示操作で)。マージ直前 `gh pr view <PR> --json closingIssuesReferences` 確認 → squash → マージ後 3 点検証、は従来手順。

---

## Self-Review(記入済み)

- **Spec coverage**: 決定 3(toast 同居)= 変更なしで維持 ✓ / 決定 4(NOACTIVATE + DWM)= Task 3(focusable(false)・apply_rounded_corners 両窓)✓ / 決定 5(一方向 snapshot・クリック逆流・icon 移設)= Task 4・5(overlay は errata として Task 7)✓ / 決定 6(位置従属・gap・hide 両窓)= Task 3 Step 3-8 + Task 4 drive_results_window(driver が main である点は errata)✓ / 決定 7(実件数フィット)= Task 2 + Task 4 ✓ / 決定 10(ドラッグ)= Task 6(位置永続の合流は Task 8 (e) で実測)✓ / config `window_gap` = Task 1 ✓ / SPEC 同期 = Task 7 ✓
- **Placeholder scan**: TBD/TODO なし。Task 4 Step 1 の「移動」は対象シンボルを名指しで列挙済み ✓
- **Type consistency**: `RowsSnapshot { rows, selected, show }` を Task 3 で定義し Task 4 の発行/消費が同形 ✓ / `results_window_height(usize, u32, f64) -> f64` を Task 2 で定義し Task 4 が消費 ✓ / `wake_results(&AppHandle)` Task 3 定義・Task 4 消費 ✓ / `row_theme(&AppHandle) -> RowTheme` Task 4 で移設・view.rs の 3 呼び出しを同 Task で切替 ✓
- **既知の実装時確認点**(実装者へ): `SearchResult` の実パスと `PartialEq` の有無(Task 3)/ `configure_japanese_font` の可視性(Task 3)/ egui の背景 interact とウィジェットのヒットテスト優先(Task 6 — 効かなければ「TextEdit rect 外のみ判定」へ縮退)/ tao の `Moved` がネイティブ移動ループ中に配送されるか(Task 6 — 配送されなければフォールバック)
