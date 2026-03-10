//! Win32 monitor helpers for multi-monitor window positioning.
//!
//! All coordinates are in physical (screen) pixels.

#[cfg(windows)]
use windows::Win32::Foundation::{HWND, POINT};
#[cfg(windows)]
use windows::Win32::Graphics::Gdi::{
    GetMonitorInfoW, MonitorFromPoint, MonitorFromWindow, MONITORINFO,
    MONITOR_DEFAULTTONEAREST, MONITOR_DEFAULTTOPRIMARY,
};
#[cfg(windows)]
use windows::Win32::UI::WindowsAndMessaging::GetCursorPos;

/// Monitor work area in physical pixels.
#[derive(Debug, Clone, Copy)]
pub struct WorkArea {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

impl WorkArea {
    pub fn width(&self) -> i32 {
        self.right - self.left
    }

    pub fn height(&self) -> i32 {
        self.bottom - self.top
    }

    /// Clamp a point so that a window of given size stays within this work area.
    pub fn clamp(&self, x: i32, y: i32, win_w: i32, win_h: i32) -> (i32, i32) {
        let cx = x.max(self.left).min(self.right - win_w);
        let cy = y.max(self.top).min(self.bottom - win_h);
        (cx, cy)
    }

    /// Center coordinates for a window of given size within this work area.
    pub fn center(&self, win_w: i32, win_h: i32) -> (i32, i32) {
        let cx = self.left + (self.width() - win_w) / 2;
        let cy = self.top + (self.height() - win_h) / 2;
        (cx, cy)
    }
}

/// Get the work area of the monitor containing the mouse cursor.
/// Falls back to primary monitor if cursor position cannot be retrieved.
#[cfg(windows)]
pub fn cursor_monitor_work_area() -> Option<WorkArea> {
    unsafe {
        let mut pt = POINT::default();
        if GetCursorPos(&mut pt).is_err() {
            return primary_monitor_work_area();
        }
        let hmon = MonitorFromPoint(pt, MONITOR_DEFAULTTOPRIMARY);
        work_area_from_hmonitor(hmon)
    }
}

/// Get the work area of the primary monitor.
#[cfg(windows)]
pub fn primary_monitor_work_area() -> Option<WorkArea> {
    unsafe {
        let hmon = MonitorFromPoint(POINT { x: 0, y: 0 }, MONITOR_DEFAULTTOPRIMARY);
        work_area_from_hmonitor(hmon)
    }
}

/// Get the work area of the monitor containing the given window.
#[cfg(windows)]
pub fn window_monitor_work_area(hwnd_raw: isize) -> Option<WorkArea> {
    unsafe {
        let hmon = MonitorFromWindow(HWND(hwnd_raw as *mut _), MONITOR_DEFAULTTONEAREST);
        work_area_from_hmonitor(hmon)
    }
}

#[cfg(windows)]
unsafe fn work_area_from_hmonitor(
    hmon: windows::Win32::Graphics::Gdi::HMONITOR,
) -> Option<WorkArea> {
    unsafe {
        let mut mi = MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        if !GetMonitorInfoW(hmon, &mut mi).as_bool() {
            return None;
        }
        let rc = mi.rcWork;
        Some(WorkArea {
            left: rc.left,
            top: rc.top,
            right: rc.right,
            bottom: rc.bottom,
        })
    }
}
