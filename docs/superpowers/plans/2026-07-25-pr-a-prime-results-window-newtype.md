# PR A′: `ResultsWindow` newtype 実装計画

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** results 窓の生 Win32 3 点セット（show / hide / topmost）と可視フラグを 1 つの型が同時に所有し、可視性の更新が 2 経路で非対称になる構造を消す。

**Architecture:** `src-tauri/src/egui_shell/results_window.rs` に `pub(crate) struct ResultsWindow { window: tauri::Window, visible: AtomicBool }` を新設する。`Deref` は実装しない。`create()` が構築して返し、`main.rs` が managed state へ載せる。results の 5 呼び出し点（`view.rs` の show / hide / set_size、`mod.rs` の hide / set_position、`commands/window.rs` の topmost 2 箇所）と `get_window("results")` の 5 箇所をすべて本型経由へ移す。

**Tech Stack:** Rust / Tauri v2.11 / windows crate v0.62 / egui + softbuffer

**根拠となる spec:** `docs/superpowers/specs/2026-07-25-egui-window-ownership-and-event-delivery-design.md` の**決定 2**（型の形・`AtomicBool` の理由・`last_results_visible` の吸収・height/width を view に残す意図的分割）、**決定 7**（trace は呼び出し側）、**決定 8「A′ の中間形」**（manage 順序）、**決定 9**（窓単位の層の選択規則を `src-tauri/CLAUDE.md` へ明記）。

## Global Constraints

- **`Deref<Target = tauri::Window>` を実装しない。** 実装すると `.hide()` が生えて元の footgun（tao の `WindowFlags::VISIBLE` 不一致で `hide()` が黙って no-op する）が復活する（spec 決定 2）。
- **`Cell` ではなく `AtomicBool` を使う。** managed state は `Send + Sync` を要求し、可視性の読み書きはイベントループスレッドに閉じない（`commands/window.rs` の topmost 復帰は spawn したポーリングスレッドから来る）。`Ordering` は同ファイル群の既存（`main_visible` / `hotkey_generation`）に合わせ **`SeqCst`** で統一する。
- **raw へ寄せるのは show / hide / topmost の 3 操作だけである。** `set_size` / `set_position` は今日 tao 経由で正しく動いており、**tao 経由のまま**メソッドで包む。`src-tauri/CLAUDE.md`「tao の窓状態を迂回して生 Win32 で操作したら、その窓の同種操作はすべて迂回側へ寄せる」を size / position へ拡大適用してはならない（差分適用が `VISIBLE` を読むのは `ShowWindow` 系の 3 操作である）。
- **trace は呼び出し側に置いたまま動かさない**（spec 決定 7）。`egui_results:show` / `egui_results:hide` の event 名と発火条件を PR A から変えない——PR A が入れた `smoke:egui` の results 被覆がこの 2 つを assert している。
- **`EguiShellState` の manage 位置（`main.rs:285`）は動かさない。** `create()` の後へ移すのは **PR D** の担当であり、`register_ctx` の撤去とセットでなければ `Mutex<Option<_>>` スロットが残って主張どおりにならない（spec 決定 8）。A′ が触るのは「`ResultsWindow` の manage を `create()` の直後・listener 登録より前に追加する」ことだけである。
- **`view.rs` の `last_results_height` / `last_results_width` は view に残す。** 可視性（correctness）とサイズガード（性能）は別概念であり、同じ表層形に見えても統合しない（spec 決定 2）。
- 新規ユニットテストは追加しない（下記「テストの位置づけ」）。

## テストの位置づけ（AGENTS.md ステップ 9 への回答）

`ResultsWindow` は実 `tauri::Window` を要し、`egui_shell` にヘッドレステスト基盤は無い（spec §7-4「受容する残余」）。本 PR の検証はユニットテストの代わりに次の 4 つが担う。**この 4 つ以外に green の根拠は無い**:

1. **型による移行の強制（compile-fail）** — 旧 3 関数を削除するため、移行漏れはコンパイルエラーになる
2. `cargo clippy --workspace --all-targets -- -D warnings` と `cargo test -p snotra`（既存 139 件の非回帰）
3. **`npm run smoke:egui -- -ResultsQuery <letter>`（実機）** — PR A が構築した results 被覆の最初の顧客。`egui_results:show` → Escape → `egui_results:hide` の観測
4. **実機目視 4 点**（spec §6「人間の実機目視でしか守れないもの」）— 1 文字目で results が出てフォーカスを奪わない / 設定サイドカー起動中に topmost が外れ終了後に戻る / 両窓が白紙にならない / ドラッグ移動中の results 追従

---

### Task 1: `ResultsWindow` newtype の新設と全呼び出し点の移行

**なぜ 1 タスクか:** 旧 3 関数の削除・新型の導入・呼び出し点の移行は**分けるとコンパイルが通らない**（`-D warnings` 下で未使用の新メソッドは `dead_code` エラー、旧関数を残せば raw Win32 の実装が二重になる）。レビュアーが片方だけ棄却できる境界が無いため 1 タスクとする。

**Files:**
- Create: `src-tauri/src/egui_shell/results_window.rs`
- Modify: `src-tauri/src/egui_shell/mod.rs`（`mod` 宣言 + re-export / `create()` の戻り値 / 旧 3 関数の削除 / `hide_egui_main` / `position_results_below_main`）
- Modify: `src-tauri/src/egui_shell/view.rs`（`drive_results_window` / `last_results_visible` の撤去 / reset-on-show のコメント改訂）
- Modify: `src-tauri/src/commands/window.rs`（topmost 2 箇所）
- Modify: `src-tauri/src/main.rs`（`create()` の戻り値を manage）
- Test: なし（上記「テストの位置づけ」）

**Interfaces:**
- Produces:
  - `pub(crate) struct ResultsWindow`（`egui_shell::ResultsWindow` として re-export）
  - `ResultsWindow::new(window: tauri::Window) -> Self`
  - `ResultsWindow::show(&self) -> bool` / `hide(&self) -> bool`（**戻り値は「遷移したか」**。既に同じ状態なら `false` を返し raw 操作を撃たない）
  - `ResultsWindow::set_topmost(&self, topmost: bool)`
  - `ResultsWindow::set_size(&self, width: f64, height: f64)` / `set_position(&self, x: i32, y: i32)`
  - `egui_shell::create(...) -> Result<ResultsWindow, snotra_egui_runtime::RuntimeError>`（戻り値の型が変わる）
- Consumes: なし（PR A までの main にそのまま乗る）

- [ ] **Step 1: `results_window.rs` を新規作成する**

`src-tauri/src/egui_shell/results_window.rs` を以下の内容で作る。`mod.rs:247-322` の 3 関数の doc コメントは**本ファイルへ移設**する（原文の説明を落とさない）。

```rust
//! results 窓の所有型（#671 spec 決定 2）。
//!
//! 生 Win32 の 3 点セット（`SW_SHOWNOACTIVATE` / `SW_HIDE` / `SetWindowPos`）と可視フラグを
//! 1 つの型が同時に所有する。#646 PR2 では 3 関数が自由関数で、可視フラグ
//! （`view.rs` の `last_results_visible`）は片方の hide 経路（`drive_results_window`）だけが
//! 更新し、もう片方（`hide_egui_main`）は更新しない非対称があった。reset-on-show が
//! 後始末することで閉じていたが、**窓とフラグを同じ物として持てば 2 経路が同じ
//! オブジェクトを通る**ため、この非対称は構造的に消える。
//!
//! **得られないもの**: `app.get_window("results").hide()` は依然コンパイルが通り、実行時に
//! 黙って no-op する（tao の `WindowFlags::VISIBLE` が raw show 後も false のままであるため）。
//! `tauri::Manager::get_window` は `AppHandle` を持つ誰からでも呼べるため footgun は
//! 表現不能にできない。本型の目的は**正しい経路を 1 つにし、誤った経路を書く動機を消す**
//! ことであって、表現不能化ではない（spec §2.6 / §7-1）。

use std::sync::atomic::{AtomicBool, Ordering};

/// results 窓（`focusable(false)` の従属窓）とその可視状態。
///
/// `Deref<Target = tauri::Window>` は**実装しない**——実装すると `.hide()` / `.show()` /
/// `.set_always_on_top()` が生え、この型が避けている当の footgun が復活する。
/// 必要な操作は inherent method として明示的に公開する。
pub(crate) struct ResultsWindow {
    window: tauri::Window,
    /// raw show / hide の直近状態。`Cell` ではなく `AtomicBool` である理由: managed state は
    /// `Send + Sync` を要求し、かつ topmost 復帰（`commands/window.rs` の設定プロセス監視）は
    /// spawn したポーリングスレッドから来る——可視性の読み書きはイベントループスレッドに
    /// 閉じない。`Ordering` は同居する `main_visible` / `hotkey_generation` に合わせ `SeqCst`。
    visible: AtomicBool,
}

impl ResultsWindow {
    /// 窓ハンドルを取り込む。**`create()` が `.visible(false)` で生成した直後に呼ぶ**前提で
    /// 初期値は false（builder の宣言と一致させる。`mod.rs` の `create` を参照）。
    pub(crate) fn new(window: tauri::Window) -> Self {
        Self {
            window,
            visible: AtomicBool::new(false),
        }
    }

    /// results 窓を**フォーカスを奪わずに**表示する（#646 PR2・実機スモークで発見）。
    /// 既に可視なら raw 操作を撃たず `false` を返す。**表示へ遷移したときだけ `true`**。
    ///
    /// 戻り値は呼び出し側の trace 用である（trace を本型の内側に置かない理由は
    /// spec 決定 7——`egui_results:show` は `drive_results_window` が 1 回だけ出す）。
    ///
    /// `tauri::Window::show()` は tao の `set_visible(true)` を経て `ShowWindow(hwnd, SW_SHOW)` を
    /// 呼ぶが、`SW_SHOW` は**プログラム的に窓を活性化する**。`focusable(false)` が付ける
    /// `WS_EX_NOACTIVATE` が防ぐのはユーザークリックによる活性化だけなので、1 文字目の入力で
    /// results が現れた瞬間に入力欄からフォーカスが奪われ 2 文字目が打てなくなる。
    /// tao 内部で `SW_SHOWNOACTIVATE` に至る唯一の経路（`MARKER_DONT_FOCUS`）は窓生成時に
    /// 1 回だけ立ち初回 show で消費されるため、繰り返し show する用途には使えない。
    pub(crate) fn show(&self) -> bool {
        // 先に flag を swap して test-and-set を原子にする（別スレッドの hide と競っても
        // raw 操作が二重に撃たれない）。raw 側の失敗（hwnd 取得不能）は今日も黙って
        // 握り潰す best-effort ゆえ、順序による観測差は無い。
        if self.visible.swap(true, Ordering::SeqCst) {
            return false;
        }
        self.raw_show();
        true
    }

    /// results 窓を隠す（`show` の対）。既に不可視なら raw 操作を撃たず `false` を返す。
    ///
    /// raw show は tao の `WindowFlags::VISIBLE` を false のまま残すため、`Window::hide()` は
    /// 「差分なし」と判定して早期 return し窓が隠れない。ゆえに hide も対で raw にする。
    pub(crate) fn hide(&self) -> bool {
        if !self.visible.swap(false, Ordering::SeqCst) {
            return false;
        }
        self.raw_hide();
        true
    }

    /// results 窓の TOPMOST を切り替える（設定サイドカー起動中の一時解除・#646 PR2）。
    /// `set_always_on_top` は tao のフラグ差分適用を通り、`VISIBLE` を false と信じている
    /// results 窓に対しては `SW_HIDE` を副作用で撃ってしまう。`SWP_NOACTIVATE` 付きの
    /// `SetWindowPos` で Z オーダーだけを動かす。**可視フラグは変えない**——Z 順の変更は
    /// 表示/非表示の遷移ではない。
    #[cfg(windows)]
    pub(crate) fn set_topmost(&self, topmost: bool) {
        use windows::Win32::Foundation::HWND;
        use windows::Win32::UI::WindowsAndMessaging::{
            HWND_NOTOPMOST, HWND_TOPMOST, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SetWindowPos,
        };
        let Ok(hwnd) = self.window.hwnd() else { return };
        let insert_after = if topmost { HWND_TOPMOST } else { HWND_NOTOPMOST };
        unsafe {
            let _ = SetWindowPos(
                HWND(hwnd.0),
                Some(insert_after),
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
            );
        }
    }

    #[cfg(not(windows))]
    pub(crate) fn set_topmost(&self, topmost: bool) {
        let _ = self.window.set_always_on_top(topmost);
    }

    /// 論理サイズを設定する。**tao 経由のままにする**——raw へ寄せるのは
    /// `ShowWindow` 系の 3 操作（show / hide / topmost）だけであり、`set_size` は
    /// `WindowFlags::VISIBLE` の差分適用を通らない。
    pub(crate) fn set_size(&self, width: f64, height: f64) {
        let _ = self.window.set_size(tauri::LogicalSize::new(width, height));
    }

    /// 物理座標で位置を設定する（`set_size` と同じ理由で tao 経由）。
    pub(crate) fn set_position(&self, x: i32, y: i32) {
        let _ = self
            .window
            .set_position(tauri::PhysicalPosition::new(x, y));
    }

    #[cfg(windows)]
    fn raw_show(&self) {
        use windows::Win32::Foundation::HWND;
        use windows::Win32::UI::WindowsAndMessaging::{SW_SHOWNOACTIVATE, ShowWindow};
        let Ok(hwnd) = self.window.hwnd() else { return };
        unsafe {
            let _ = ShowWindow(HWND(hwnd.0), SW_SHOWNOACTIVATE);
        }
    }

    #[cfg(not(windows))]
    fn raw_show(&self) {
        let _ = self.window.show();
    }

    #[cfg(windows)]
    fn raw_hide(&self) {
        use windows::Win32::Foundation::HWND;
        use windows::Win32::UI::WindowsAndMessaging::{SW_HIDE, ShowWindow};
        let Ok(hwnd) = self.window.hwnd() else { return };
        unsafe {
            let _ = ShowWindow(HWND(hwnd.0), SW_HIDE);
        }
    }

    #[cfg(not(windows))]
    fn raw_hide(&self) {
        let _ = self.window.hide();
    }
}
```

- [ ] **Step 2: `mod.rs` にモジュール宣言と re-export を足す**

`src-tauri/src/egui_shell/mod.rs:16` 付近（`mod results_view;` の並び）へ:

```rust
mod results_view;
mod results_window;
mod view;
```

`mod.rs:21-22`（`ResultsShared` の re-export）の直後へ:

```rust
// main.rs（managed state 化）・view.rs（drive）・commands/window.rs（topmost）が消費する
// （#671 PR A′ spec 決定 2）。
pub(crate) use results_window::ResultsWindow;
```

- [ ] **Step 3: `mod.rs` の旧 3 関数を削除する**

`mod.rs:247-322` の `show_results_no_activate` / `hide_results` / `set_results_topmost`（`#[cfg(windows)]` と `#[cfg(not(windows))]` の両版・doc コメント含む）を**すべて削除**する。doc の内容は Step 1 で `results_window.rs` へ移設済み。

削除の際、`show_results_no_activate` の doc 末尾にある次の一文は `results_window.rs` の `//!` が担うため、重複して残さないこと:

> **results の可視性は本モジュールの 3 関数が唯一の経路であり、tauri の show/hide/set_always_on_top を results へ呼んではならない。**

- [ ] **Step 4: `create()` が `ResultsWindow` を返すようにする**

`mod.rs:165-169` のシグネチャ:

```rust
pub(crate) fn create(
    app: &mut tauri::App,
    window_width: f64,
    background_color_hex: &str,
) -> Result<ResultsWindow, snotra_egui_runtime::RuntimeError> {
```

`mod.rs:204-209`（`apply_rounded_corners` と `runtime.attach(results, ...)` の間）:

```rust
    #[cfg(windows)]
    {
        apply_rounded_corners(&window); // main にも適用（輪郭言語を揃える・決定 4）
        apply_rounded_corners(&results);
    }
    // #671 PR A′: attach は window を move するため、その**前**に clone から所有型を作る。
    // `tauri::Window` は Arc ベースのハンドルで、clone は同一窓を指す（tauri 2.11 の
    // `impl Clone for Window` を実測）。
    let results_window = ResultsWindow::new(results.clone());
    runtime.attach(results, results_view::ResultsView::new(app_handle.clone()))?;
```

`mod.rs:222`（末尾の tail expression）:

```rust
    runtime.attach(window, SearchWindowView::new(app_handle))?;
    Ok(results_window)
}
```

- [ ] **Step 5: `hide_egui_main` を移行する**

`mod.rs:456-462` を置き換える:

```rust
    if let Some(results) = app.try_state::<ResultsWindow>() {
        results.hide();
        // 呼び出し側に置く（spec 決定 7）。results の hide は 2 経路あり
        // （ここと view.rs の drive_results_window）、trace は要求レベルゆえ
        // 既に隠れていても出る——smoke は presence のみを assert する。
        crate::trace_main("egui_results:hide", serde_json::json!({ "from": "hide_main" }));
    }
```

**`hide()` の戻り値を無視するのは意図的である**（trace の発火条件を PR A から変えないため。ここは要求レベル）。

- [ ] **Step 6: `position_results_below_main` を移行する**

`mod.rs:554-570` を置き換える（`gap` 算出と `main` の取得は現行のまま）:

```rust
pub(crate) fn position_results_below_main(app: &tauri::AppHandle) {
    let (Some(main), Some(results)) = (app.get_window("main"), app.try_state::<ResultsWindow>())
    else {
        return;
    };
    let gap = app
        .try_state::<crate::AppState>()
        .map(|s| s.engine.lock().unwrap().config().visual.window_gap)
        .unwrap_or(4) as f64;
    if let (Ok(pos), Ok(size), Ok(scale)) =
        (main.outer_position(), main.outer_size(), main.scale_factor())
    {
        results.set_position(pos.x, pos.y + size.height as i32 + (gap * scale).round() as i32);
    }
}
```

- [ ] **Step 7: `view.rs` の `drive_results_window` を移行する**

`view.rs:700-739` の本体を置き換える（doc コメント `:691-693` は変更しない）:

```rust
        let Some(results) = self
            .app_handle
            .try_state::<crate::egui_shell::ResultsWindow>()
        else {
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
            // 可視フラグは ResultsWindow が持つ（#671 PR A′ spec 決定 2）。hide() は
            // 遷移したときだけ true を返すため、trace は 1 回だけ出る（毎フレーム
            // 撃たない）。trace を型の内側でなく呼び出し側に置く理由は spec 決定 7。
            if results.hide() {
                crate::trace_main("egui_results:hide", serde_json::json!({ "from": "drive" }));
            }
            return;
        }
        // 位置: main の外形直下 + gap(物理座標。gap は論理 px を scale で換算)。無ガードの
        // 単一点(position_results_below_main・mod.rs)へ委譲——Moved リスナーと共用する
        // ため、デルタガードはヘルパー側に持たない(#646 PR2 決定 10)。
        crate::egui_shell::position_results_below_main(&self.app_handle);
        if (res_h - self.last_results_height).abs() > 0.5
            || (width - self.last_results_width).abs() > 0.5
        {
            results.set_size(width, res_h);
            self.last_results_height = res_h;
            self.last_results_width = width;
        }
        // フォーカスを奪わない表示（tauri show() は SW_SHOW で活性化する・#646 PR2）。
        if results.show() {
            crate::trace_main("egui_results:show", serde_json::json!({ "rows": count }));
        }
        crate::egui_shell::wake_results(&self.app_handle);
    }
```

- [ ] **Step 8: `view.rs` の `last_results_visible` フィールドを撤去する**

`view.rs:188-189` の宣言を削除する:

```rust
    /// results 窓の直近可視状態（drive_results_window のデルタガード・#646 PR2 決定 6）。
    last_results_visible: bool,
```

`view.rs:222` の初期化 `last_results_visible: false,` を削除する。

残す `last_results_height` の doc（`:190`）に、なぜ可視性だけが移ったのかを 1 行足す:

```rust
    /// results 窓の直近設定高さ（デルタガード）。**可視フラグは `ResultsWindow` が持つ**——
    /// こちらは冗長な `set_size` を避ける性能上のガードであり概念が別（#671 spec 決定 2）。
    last_results_height: f64,
```

- [ ] **Step 9: reset-on-show のコメントを書き直す**

`view.rs:1041-1047` を置き換える。**旧コメントが述べる事故（stale な `last_results_visible=true` が残って show をスキップし続ける）は本 PR で構造的に消えるため、そのまま残すと偽になる**:

```rust
            // results 窓の drive **サイズ**デルタガードを初期値へ戻す（#646 PR2 決定 6）。
            // これは冗長な set_size を避ける性能上のガードであり、可視性のような
            // correctness のフラグではない（#671 spec 決定 2 の意図的な分割）。0 へ戻すことで
            // 再 show 後に必ず 1 度は現行 metrics で set_size させる。
            // 可視フラグはここに無い——`ResultsWindow` が所有し、hide_egui_main と
            // drive_results_window の 2 経路が同じ型を通るため後始末が要らない（PR A′）。
            self.last_results_height = 0.0;
            self.last_results_width = 0.0;
```

- [ ] **Step 10: `commands/window.rs` の topmost 2 箇所を移行する**

`commands/window.rs:96-100`:

```rust
    if let Some(results) = app.try_state::<crate::egui_shell::ResultsWindow>() {
        // results は tauri の set_always_on_top を使えない（tao の差分適用が VISIBLE を
        // false と信じて SW_HIDE を撃つ・#646 PR2）。Z オーダーのみ動かす専用経路を通す。
        results.set_topmost(false);
    }
```

`commands/window.rs:142-144`（監視スレッド内）:

```rust
        if let Some(results) = handle_for_monitor.try_state::<crate::egui_shell::ResultsWindow>() {
            results.set_topmost(true);
        }
```

- [ ] **Step 11: `main.rs` で managed state へ載せる**

`main.rs:285-288` を置き換える:

```rust
            app.manage(egui_shell::EguiShellState::default());
            let results_window = egui_shell::create(app, window_width as f64, &bg_color)?;
            // #671 PR A′: results 窓の所有型を managed state へ（spec 決定 8「A′ の中間形」）。
            // **listener 登録より前**に置く——hide_egui_main が try_state で引くため、hide が
            // 起こりうる時点より前に manage されている必要がある。`EguiShellState` の manage
            // 位置は動かさない（create() の後へ移すのは PR D の担当。register_ctx の撤去と
            // セットでなければ Option スロットが残る）。
            app.manage(results_window);
            // view→emit→listener の合流点。**main の** hide を hide_egui_main の 1 経路に集約（codex #7）。
            egui_shell::register_hide_listener(&app_handle);
```

- [ ] **Step 12: ビルドとリントを通す**

Run: `cargo clippy --workspace --all-targets -- -D warnings`
Expected: 警告 0。移行漏れがあればここでコンパイルエラーになる（旧 3 関数は削除済みのため）。

Run: `cargo test -p snotra`
Expected: 既存 139 件 pass（本 PR は純粋なリファクタリングゆえ件数は変わらない）。

- [ ] **Step 13: 移行の完全性を grep で確認する**

Run: `grep -rn 'get_window("results")\|show_results_no_activate\|hide_results\|set_results_topmost\|last_results_visible' src-tauri/src --include=*.rs`

Expected: **0 件**。`create()` 内も `tauri::Window::builder` が窓を直接返すため `get_window("results")` は残らない。
**期待値を「1 件」と書かないこと**——PR A で同型の誤り（新設コメントが検索語を含んで grep が期待外にヒット）を 2 度踏んでいる。**新しいコメントに旧関数名を書かない**（書くとこの grep が 0 にならない）。0 でなければ、ヒット箇所がコメントか実コードかを見て、コメントなら文言から旧名を外す。

- [ ] **Step 14: コミット**

```bash
git add src-tauri/src/egui_shell/results_window.rs src-tauri/src/egui_shell/mod.rs src-tauri/src/egui_shell/view.rs src-tauri/src/commands/window.rs src-tauri/src/main.rs
git commit -F <tmpfile>
```

コミットメッセージ（`$env:TEMP` 配下の一時ファイル経由。HEREDOC は使わない）:

```
refactor: #671 PR A′ results 窓の所有型 ResultsWindow を導入

生 Win32 3 点セット（SW_SHOWNOACTIVATE / SW_HIDE / SetWindowPos）と可視フラグを
1 つの型が同時に所有する。hide の 2 経路（hide_egui_main / drive_results_window）が
同じオブジェクトを通るため、片方だけがフラグを更新する非対称が構造的に消える。
Deref は実装しない（.hide() が生えると元の footgun が復活するため）。
```

---

### Task 2: 文書の同期（モジュール索引 + 窓単位の層の選択規則）

**Files:**
- Modify: `src-tauri/CLAUDE.md`（`egui_shell/` のファイル索引 / 「tao の窓状態を迂回して…」の規則）

**Interfaces:**
- Consumes: Task 1 が作った `results_window.rs` と、そこへ移った 3 操作

- [ ] **Step 1: モジュール索引に `results_window.rs` を足す**

`src-tauri/CLAUDE.md`「モジュール構成」の `egui_shell/` 箇条書きの冒頭（ディレクトリモジュールの構成列挙）へ `results_window.rs` を追加し、末尾の責務列挙にも 1 句足す。責務の散文の正本はファイルの `//!` であり、ここはファイル名の索引を保つのが目的である（#562）。

構成列挙:

```
`egui_shell/`: ディレクトリモジュール（`mod.rs` + `lifecycle.rs` / `search_state.rs` / `layout.rs` / `icon_textures.rs` / `notify.rs` / `strings.rs` / `view.rs` / `results_view.rs` / `results_window.rs`）
```

責務列挙（`results_view.rs` の句の直後へ）:

```
`results_window.rs` は results 窓の所有型（生 Win32 の show/hide/topmost と可視フラグを同時に持つ・#671 PR A′）
```

- [ ] **Step 2: 「窓単位の層の選択」規則へ書き直す（spec 決定 9）**

`src-tauri/CLAUDE.md`「Win32 / Tauri 注意事項」の既存箇条書き **「tao の窓状態を迂回して生 Win32 で操作したら、その窓の同種操作はすべて迂回側へ寄せる。」** を、次で置き換える:

```markdown
- **ある窓の show / hide / topmost のいずれか 1 つが tao を迂回したら、残り 2 つも必ず迂回側へ寄せる。混在は許されない。** `apply_diff` はフラグ差分がゼロなら早期 return し、`VISIBLE` を持たない窓には `SW_HIDE` を副作用で撃つ。片方だけ raw にすると「`hide()` が何もしない」「`set_always_on_top` で窓が消える」が同時に生まれる（#646 PR2）。窓ごとの層は次で固定する:
  - main（主窓）= 3 操作すべて tao 経由（tauri `show` / `hide` / `set_always_on_top`）
  - results（従属窓）= 3 操作すべて raw（`SW_SHOWNOACTIVATE` / `SW_HIDE` / `SetWindowPos`）。実装は `egui_shell::ResultsWindow` が唯一の所有点（#671 PR A′）
  - **「main の show だけ raw にして統一する」は禁止。** main の tao `VISIBLE` が stale 化し、`set_always_on_top` が main を消す（`commands/window.rs` の topmost 対称がその瞬間に凶器になる）
  - **対象は `ShowWindow` 系の 3 操作だけである。** `set_size` / `set_position` は `VISIBLE` の差分適用を通らないため、results でも tao 経由のままにする（raw へ広げない）
```

- [ ] **Step 3: ガバナンス検査**

Run: `npm run governance:check`
Expected: G1..G10 pass。**ファイルを追加した PR ではこれを PR 作成前に必ず走らせる**（#629 / #630 で索引更新漏れが同型再発している。`*.md` と新規ファイルは post-edit hook の対象外で、沈黙は「何も走らなかった」であって合格ではない）。

- [ ] **Step 4: コミット**

```bash
git add src-tauri/CLAUDE.md
git commit -F <tmpfile>
```

メッセージ:

```
docs: #671 PR A′ モジュール索引と窓単位の層の選択規則

results_window.rs を egui_shell の索引へ追加し、「tao を迂回したら寄せる」規則を
窓種ごとの層の固定（main=tao / results=raw）として書き直す（spec 決定 9）。
対象が ShowWindow 系 3 操作に限られること（set_size/set_position は tao のまま）も明記。
```

---

### Task 3: 実機検証

**Files:** なし（検証のみ）

- [ ] **Step 1: GUI smoke（results 被覆つき）**

Run: `npm run smoke:egui -- -ResultsQuery <索引に当たる 1 文字>`
Expected: `egui_show:done` → `egui_results:show` → `egui_results:hide` → `egui_hide:done` がすべて観測され、`failures` 0 で PASS。

**注意**: このスクリプトは実機のフォアグラウンドへホットキーと文字を注入する。実行前に他の操作を止めること。ホットキーは `hotkey:registered` trace から自動導出されるため `-HotkeyVks` は通常不要（PR A 決定 10）。

- [ ] **Step 2: 実機目視 4 点**

1. 1 文字目の入力で results が現れ、**かつ 2 文字目が入力欄に入る**（フォーカスを奪わない）
2. 設定サイドカー起動中に results が設定画面の上に浮かない / 終了後に topmost が戻る
3. main / results が白紙にならない
4. main をドラッグ移動する間、results が追従する（`Moved` リスナー経路 = `position_results_below_main`）

いずれも `egui_show:done` 等の trace では検出できないため、目視でのみ守れる（spec §6）。

- [ ] **Step 3: 結果を PR 本文へ記録する**

「追加/更新テスト名 + 検証した不変条件」の形式（AGENTS.md ステップ 9）。本 PR は新規テストを持たないため、**「テストの位置づけ」の 4 点それぞれについて実行結果を明記する**（未実施があれば未実施と書く。実施したことにしない）。
