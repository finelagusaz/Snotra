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
