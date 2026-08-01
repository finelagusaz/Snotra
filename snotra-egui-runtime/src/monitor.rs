//! 窓が載っているモニターのリフレッシュレート取得（#737・契約②「配送には下限間隔がある」の供給側）。
//!
//! カスケードは 現在モード（動的）→ OS 既定（レジストリ）→ None の順で、None は呼び出し側
//! （`RepaintScheduler::set_min_interval`）が 60Hz フォールバックへ倒す（issue #737 コメントの
//! 取得順・config キーは持たない＝ユーザー合意 2026-07-26）。`dmDisplayFrequency` の 0/1 は
//! 「ハードウェア既定を使う」の番兵値（DEVMODE の慣習）であり、実レートとして扱わない。
//!
//! HWND / HMONITOR は `isize` で受け渡す（`Send` でないハンドル型を持ち回らない
//! `windows_ime.rs` と同じパターン）。

/// 窓が載っている HMONITOR（変化検知用・安価な単独呼び出し）。
/// `Moved` はドラッグ中に連発するため、呼び出し側はこの値の変化を見てから
/// `monitor_refresh_hz`（`EnumDisplaySettingsW` を含み安くない）を呼ぶ。
#[cfg(windows)]
pub(crate) fn window_monitor(hwnd: isize) -> Option<isize> {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::Graphics::Gdi::{MONITOR_DEFAULTTONEAREST, MonitorFromWindow};
    let monitor = unsafe { MonitorFromWindow(HWND(hwnd as *mut _), MONITOR_DEFAULTTONEAREST) };
    (!monitor.is_invalid()).then_some(monitor.0 as isize)
}

#[cfg(not(windows))]
pub(crate) fn window_monitor(_hwnd: isize) -> Option<isize> {
    None
}

/// 窓が載っているモニターのリフレッシュレート（Hz）。取得できなければ None。
#[cfg(windows)]
pub(crate) fn monitor_refresh_hz(hwnd: isize) -> Option<u32> {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::Graphics::Gdi::{
        DEVMODEW, ENUM_CURRENT_SETTINGS, ENUM_REGISTRY_SETTINGS, EnumDisplaySettingsW,
        GetMonitorInfoW, MONITOR_DEFAULTTONEAREST, MONITORINFO, MONITORINFOEXW, MonitorFromWindow,
    };
    use windows::core::PCWSTR;

    unsafe {
        let monitor = MonitorFromWindow(HWND(hwnd as *mut _), MONITOR_DEFAULTTONEAREST);
        if monitor.is_invalid() {
            return None;
        }
        // MONITORINFOEXW は MONITORINFO を先頭に埋め込む（repr(C)）——cbSize に拡張版の
        // サイズを入れて MONITORINFO* へキャストする Win32 の定石（windows 0.61.3 で実確認）。
        let mut info = MONITORINFOEXW::default();
        info.monitorInfo.cbSize = std::mem::size_of::<MONITORINFOEXW>() as u32;
        // ポインタは構造体**全体**から導出する——`&mut info.monitorInfo` 起点だと provenance が
        // 先頭 40 バイトに閉じ、Win32 が cbSize に従って書く szDevice への書き込みが
        // 別名規則上の範囲外になる（レビュー M1・現行 codegen では動くが Miri 相当で検出される形）。
        if !GetMonitorInfoW(monitor, (&raw mut info).cast::<MONITORINFO>()).as_bool() {
            return None;
        }
        for mode in [ENUM_CURRENT_SETTINGS, ENUM_REGISTRY_SETTINGS] {
            let mut devmode = DEVMODEW {
                dmSize: std::mem::size_of::<DEVMODEW>() as u16,
                ..Default::default()
            };
            if EnumDisplaySettingsW(PCWSTR(info.szDevice.as_ptr()), mode, &mut devmode).as_bool()
                && devmode.dmDisplayFrequency > 1
            {
                return Some(devmode.dmDisplayFrequency);
            }
        }
        None
    }
}

#[cfg(not(windows))]
pub(crate) fn monitor_refresh_hz(_hwnd: isize) -> Option<u32> {
    None
}
