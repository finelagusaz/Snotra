//! results 窓の所有型（#671 spec 決定 2）。
//!
//! 生 Win32 の 3 点セット（`SW_SHOWNOACTIVATE` / `SW_HIDE` / `SetWindowPos`）と可視フラグを
//! 1 つの型が同時に所有する。#646 PR2 では 3 関数が自由関数で、可視フラグは
//! `SearchWindowView` 側の view-local な bool であり、片方の hide 経路（`drive_results_window`）
//! だけが更新し、もう片方（`hide_egui_main`）は更新しない非対称があった。reset-on-show が
//! 後始末することで閉じていたが、**窓とフラグを同じ物として持てば 2 経路が同じ
//! オブジェクトを通る**ため、この非対称は構造的に消える。
//!
//! **得られないもの**: `Manager` から results の生ハンドルを引いて `.hide()` を呼ぶ書き方は
//! 依然コンパイルが通り、実行時に黙って no-op する（tao の `WindowFlags::VISIBLE` が raw show
//! 後も false のままであるため）。ハンドルの取得は `AppHandle` を持つ誰からでもできるため
//! footgun は表現不能にできない。本型の目的は**正しい経路を 1 つにし、誤った経路を書く動機を
//! 消す**ことであって、表現不能化ではない（spec §2.6 / §7-1）。
//!
//! **可視性の全体像はこの型だけでは閉じない。** main が hidden の間に results が出る事故は
//! show 述語側のゲート（`layout::present_results`）が塞ぐ——本型は「誰が raw 操作を
//! 撃つか」を一点に集めるだけで、「撃ってよい状況か」は判定しない。

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
        // raw 操作が二重に撃たれない）。**保証するのはそこまでである**——swap と `raw_show()`
        // の間に他スレッドの `hide()` が挟まると「フラグ=false・窓=可視」の不一致が残りうる
        // （回収は次の show 遷移）。フラグと窓の同時性は保証しない。
        // この型が閉じられないぶんは、show 述語側の `main_visible` ゲート
        // （`layout::present_results`）が main hidden 中の再表示を塞ぐ。
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

    /// 論理サイズを設定する。**tao 経由のままにする。**
    ///
    /// 理由は「差分適用を通らないから」では**ない**——tao 0.35.3 の `set_inner_size` /
    /// `set_outer_position` はどちらも `set_window_flags(|f| f.set(MAXIMIZED, false))` を呼び
    /// `apply_diff` に**入る**。results では MAXIMIZED が元から false ゆえ**フラグ差分が空**に
    /// なり、`apply_diff` 冒頭の `if diff == empty { return }` で助かっている。
    ///
    /// **判定基準は「apply_diff を通るか」ではなく「フラグ差分が生じるか」である。**
    /// 差分を生む操作（`set_resizable` 等）は `apply_diff` 末尾の
    /// `if !new.contains(VISIBLE) { ShowWindow(SW_HIDE) }` に到達し、results 窓を消す
    /// （`set_always_on_top` が #646 PR2 で窓を消したのと同一機構）。
    pub(crate) fn set_size(&self, width: f64, height: f64) {
        let _ = self.window.set_size(tauri::LogicalSize::new(width, height));
    }

    /// 物理 ↔ 論理の換算係数（#675）。
    ///
    /// **`set_size` が渡す `LogicalSize` を tao が物理へ戻すときと同じ factor でなければ
    /// ならない**——tao の `set_inner_size` は**この窓の** `scale_factor()` で `to_physical`
    /// する。main の scale を流用すると混在 DPI 環境で高さが食い違う。
    pub(crate) fn scale_factor(&self) -> Option<f64> {
        self.window.scale_factor().ok()
    }

    /// 物理座標で位置を設定する（`set_size` と同じ理由で tao 経由）。
    pub(crate) fn set_position(&self, x: i32, y: i32) {
        let _ = self.window.set_position(tauri::PhysicalPosition::new(x, y));
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
