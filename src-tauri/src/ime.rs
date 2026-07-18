//! IME をオフにする Win32 IMM API の薄いラッパー。
//!
//! 検索ウィンドウ表示時に `ImmSetOpenStatus(false)` で IME を無効化し、ローマ字を直接
//! 入力できるようにする。

use windows::Win32::Foundation::HWND;
use windows::Win32::UI::Input::Ime::{ImmGetContext, ImmReleaseContext, ImmSetOpenStatus};

pub fn turn_off_ime(hwnd: HWND) {
    unsafe {
        let himc = ImmGetContext(hwnd);
        if !himc.is_invalid() {
            let _ = ImmSetOpenStatus(himc, false);
            let _ = ImmReleaseContext(hwnd, himc);
        }
    }
}
