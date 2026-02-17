use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt;

use windows::core::PCWSTR;
use windows::Win32::Foundation::HWND;
use windows::Win32::UI::Shell::ShellExecuteW;
use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

pub fn launch(target_path: &str) {
    let wide_path: Vec<u16> = OsStr::new(target_path)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    let wide_open: Vec<u16> = OsStr::new("open")
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();

    unsafe {
        ShellExecuteW(
            HWND::default(),
            PCWSTR(wide_open.as_ptr()),
            PCWSTR(wide_path.as_ptr()),
            PCWSTR::null(),
            PCWSTR::null(),
            SW_SHOWNORMAL,
        );
    }
}
