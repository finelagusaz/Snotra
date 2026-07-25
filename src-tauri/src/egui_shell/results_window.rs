//! results 窓の所有型（#671 spec 決定 2）。
//!
//! 生 Win32 の 3 点セット（`SW_SHOWNOACTIVATE` / `SW_HIDE` / `SetWindowPos`）と可視フラグを
//! 1 つの型が同時に所有する。#646 PR2 では 3 関数が自由関数で、可視フラグは
//! `SearchWindowView` 側の view-local な bool であり、片方の hide 経路（`drive_results_window`）
//! だけが更新し、もう片方（`hide_egui_main`）は更新しない非対称があった。reset-on-show が
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
